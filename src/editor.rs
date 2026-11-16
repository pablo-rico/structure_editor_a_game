//! Estado del modo Editar (fase 2): documento abierto, herramientas, paleta, validación en vivo con el puente,
//! guardar/abrir `.agvox`, exportar al juego y copia de recuperación.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::agvox::{self, Meta};
use crate::bridge::proto::{Catalog, Severity};
use crate::edit::Doc;
use crate::edit::tools::Tools;
use crate::host::Host;
use crate::model::{SUB, Structure, VoxelGrid};
use crate::ui::palette::Palette;
use crate::ui::properties::Resize;

/// Espera tras el último cambio antes de pedir la validación (§ plan: 300 ms).
const VALIDATE_AFTER: Duration = Duration::from_millis(300);
/// Copia de recuperación (§8: cada 2 minutos).
const RECOVER_EVERY: Duration = Duration::from_secs(120);

pub struct Editor {
    pub doc: Option<Doc>,
    pub tools: Tools,
    pub palette: Palette,
    pub resize: Resize,
    /// Sube con cada cambio visible del documento (remallar, validar).
    pub revision: u64,
    seen_version: u64,
    changed_at: Option<Instant>,
    validated: u64,
    recovered_at: Instant,
    /// Lo último que se ha mandado a dibujar.
    pub shown: Option<(u64, Arc<Structure>)>,
    pub exporting: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            doc: None,
            tools: Tools::default(),
            palette: Palette::default(),
            resize: Resize::default(),
            revision: 0,
            seen_version: 0,
            changed_at: None,
            validated: 0,
            recovered_at: Instant::now(),
            shown: None,
            exporting: false,
        }
    }
}

/// Carpeta de las fuentes `.agvox` dentro del juego (regla de a_game: fuentes en ArtSource).
pub fn source_dir(agame_root: &Path) -> PathBuf {
    agame_root.join("ArtSource/Structures")
}

/// Nombre de fichero a partir del nombre visible: minúsculas, sin acentos, `_` entre palabras.
pub fn slug(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'ü' | 'Ú' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            _ => ' ',
        })
        .collect();
    let s = plain.split_whitespace().collect::<Vec<_>>().join("_");
    if s.is_empty() { "estructura".into() } else { s }
}

/// Siguiente id libre: máximo de los `.agvox` de la carpeta + 1 (los ids nunca se reutilizan mientras exista el
/// fichero que lo tenía).
pub fn next_id(dir: &Path, materials: &[String]) -> u32 {
    let ids = std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| {
        let p = e.path();
        (p.extension()? == "agvox").then_some(())?;
        agvox::load(&std::fs::read_to_string(&p).ok()?, materials).ok()?.0.id
    });
    ids.max().map_or(1, |m| m + 1)
}

fn recovery_path() -> Option<PathBuf> {
    Some(crate::platform::data_dir().join("recuperacion.agvox"))
}

impl Editor {
    /// Documento nuevo: `w`×`d` celdas y `levels` niveles, con un forjado de cimientos de piedra en el nivel 0.
    pub fn new_doc(&mut self, cat: &Catalog, w: i32, d: i32, levels: i32) {
        let mut grid = VoxelGrid::new([w * SUB, levels * SUB, d * SUB]);
        let stone = cat.materials.iter().position(|m| m == "Stone").unwrap_or(1) as u8;
        for z in 0..grid.size[2] {
            for x in 0..grid.size[0] {
                for y in 0..SUB {
                    grid.set(x, y, z, stone);
                }
            }
        }
        let s = Structure {
            grid,
            ground_level: 1,
            band_starts: vec![SUB],
            band_floors: vec![1],
            entrance: [w / 2, -1],
            ..Default::default()
        };
        let style = cat.styles.first().map_or_else(String::new, |s| s.name.clone());
        self.set_doc(Doc::new(Meta { name: "Estructura nueva".into(), style, ..Default::default() }, s, None));
    }

    /// «Duplicar para editar»: parte de una estructura procedural (construida sin girar, intacta y con estado 0).
    pub fn fork(&mut self, s: &Structure, name: String, style: String) {
        let mut copy = s.clone();
        copy.checks.clear();
        for p in &mut copy.parts {
            p.cell = None;
        }
        // Las puertas interactuables las crea el juego a partir de cada parte `Door` (`VoxelGrid.AddPart`).
        copy.interactables.retain(|i| i.kind != "Door");
        self.set_doc(Doc::new(Meta { name, style, ..Default::default() }, copy, None));
    }

    fn set_doc(&mut self, doc: Doc) {
        self.seen_version = doc.version;
        self.doc = Some(doc);
        self.tools.selection = None;
        self.resize = Resize::default();
        self.touch();
    }

    pub fn open(&mut self, path: &Path, materials: &[String]) -> Result<()> {
        let text = std::fs::read_to_string(path).with_context(|| format!("no puedo leer {}", path.display()))?;
        let (meta, s) = agvox::load(&text, materials)?;
        self.set_doc(Doc::new(meta, s, Some(path.to_path_buf())));
        Ok(())
    }

    /// Guarda en `path` (o en el del documento). Devuelve la ruta escrita.
    pub fn save(&mut self, path: Option<PathBuf>, materials: &[String]) -> Result<PathBuf> {
        let doc = self.doc.as_mut().context("no hay nada abierto")?;
        let path = path.or_else(|| doc.path.clone()).context("elige dónde guardar")?;
        let text = agvox::save(&doc.meta, &doc.s, materials)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, text).with_context(|| format!("no puedo escribir {}", path.display()))?;
        doc.path = Some(path.clone());
        doc.unsaved = false;
        Ok(path)
    }

    /// Exportar al juego: asigna id si no tiene, guarda la fuente en `ArtSource/Structures/<slug>.agvox` y pide al
    /// puente que regenere el C#. El resultado llega como `Reply::Exported`.
    pub fn export(&mut self, host: &mut Host, materials: &[String]) -> Result<PathBuf> {
        let dir = source_dir(&host.paths.agame_root);
        let doc = self.doc.as_mut().context("no hay nada abierto")?;
        if doc.meta.id.is_none() {
            let id = next_id(&dir, materials);
            doc.edit_extras("Asignar id", |e| e.meta.id = Some(id));
        }
        let in_sources = doc.path.as_ref().is_some_and(|p| p.parent() == Some(dir.as_path()));
        let target = if in_sources { doc.path.clone() } else { Some(dir.join(format!("{}.agvox", slug(&doc.meta.name)))) };
        let path = self.save(target, materials)?;
        if !host.export(&dir) {
            anyhow::bail!("el generador no está listo");
        }
        self.exporting = true;
        Ok(path)
    }

    /// Errores que impiden exportar.
    pub fn errors(&self) -> usize {
        self.doc.as_ref().map_or(0, |d| d.s.checks.iter().filter(|c| c.severity == Severity::Error).count())
    }

    /// Marca el documento como cambiado (remallar y revalidar).
    pub fn touch(&mut self) {
        self.revision += 1;
        self.changed_at = Some(Instant::now());
    }

    /// Una vez por fotograma: detecta cambios del documento, valida con espera y hace la copia de recuperación.
    pub fn tick(&mut self, host: &mut Host, materials: &[String]) {
        let Some(version) = self.doc.as_ref().map(|d| d.version) else { return };
        if version != self.seen_version {
            self.seen_version = version;
            self.touch();
        }
        let Some(doc) = self.doc.as_ref() else { return };
        if let Some(t) = self.changed_at
            && t.elapsed() >= VALIDATE_AFTER
            && self.validated != self.revision
        {
            self.validated = self.revision;
            self.changed_at = None;
            host.validate(self.revision, &doc.s, doc.meta.replaces.as_ref());
        }
        if doc.unsaved && self.recovered_at.elapsed() >= RECOVER_EVERY {
            self.recovered_at = Instant::now();
            if let (Some(path), Ok(text)) = (recovery_path(), agvox::save(&doc.meta, &doc.s, materials)) {
                let _ = path.parent().map(std::fs::create_dir_all);
                let _ = std::fs::write(path, text);
            }
        }
    }

    /// Respuesta de `validate`: navegación y comprobaciones nuevas, si siguen siendo de esta revisión.
    pub fn on_validated(&mut self, revision: u64, s: Structure) {
        if revision != self.revision {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.s.nav = s.nav;
            doc.s.checks = s.checks;
            doc.s.footprint = s.footprint;
            self.shown = None; // las capas de navegación cambian
        }
    }

    /// Estructura para el visor (se clona solo cuando cambia).
    pub fn structure(&mut self) -> Option<Arc<Structure>> {
        let doc = self.doc.as_ref()?;
        if self.shown.as_ref().is_none_or(|(r, _)| *r != self.revision) {
            self.shown = Some((self.revision, Arc::new(doc.s.clone())));
        }
        self.shown.as_ref().map(|(_, s)| s.clone())
    }

    /// Hay copia de recuperación de una sesión anterior.
    pub fn recovery_available() -> Option<PathBuf> {
        recovery_path().filter(|p| p.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_de_fichero() {
        assert_eq!(slug("Torre del Puerto"), "torre_del_puerto");
        assert_eq!(slug("Señal ¡Única!"), "senal_unica");
        assert_eq!(slug("  "), "estructura");
    }

    #[test]
    fn ids_sin_repetir() {
        let dir = std::env::temp_dir().join(format!("agvox-ids-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mats: Vec<String> = vec!["Air".into(), "Stone".into()];
        assert_eq!(next_id(&dir, &mats), 1);
        let s = Structure { grid: VoxelGrid::new([8, 8, 8]), ..Default::default() };
        let meta = Meta { id: Some(4), name: "a".into(), ..Default::default() };
        std::fs::write(dir.join("a.agvox"), agvox::save(&meta, &s, &mats).unwrap()).unwrap();
        assert_eq!(next_id(&dir, &mats), 5);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
