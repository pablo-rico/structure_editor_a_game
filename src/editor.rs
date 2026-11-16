//! Estado del modo Editar (fase 2): documento abierto, herramientas, paleta, validación en vivo con el puente,
//! guardar/abrir `.agvox`, exportar al juego y copia de recuperación.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use crate::agvox::{self, Meta};
use crate::bridge::proto::{Catalog, Severity};
use crate::edit::Doc;
use crate::edit::tools::Tools;
use crate::host::Host;
use crate::model::{SUB, Structure, VoxelGrid};
use crate::ui::palette::Palette;
use crate::ui::properties::Resize;
use crate::vmodel::{self, Model};

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
    /// Modelos: fecha y tamaño del fichero al abrirlo o guardarlo (para saber si otro lo ha cambiado).
    disk: Option<(SystemTime, u64)>,
    /// Modelos: tamaño con que se abrió y si aún no existía en el juego (avisos de Validación).
    opened_size: Option<[i32; 3]>,
    model_new: bool,
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
            disk: None,
            opened_size: None,
            model_new: false,
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

fn model_recovery_path() -> PathBuf {
    crate::platform::data_dir().join("recuperacion_model.json")
}

/// Carpeta del juego donde viven los modelos de vóxeles (`*_model.json`), en subcarpetas.
pub fn models_root(agame_root: &Path) -> PathBuf {
    agame_root.join("Assets/_Game/Art")
}

/// Dónde apunta la copia de recuperación de un modelo: la ruta del fichero del juego que se estaba editando.
fn model_recovery_origin() -> PathBuf {
    crate::platform::data_dir().join("recuperacion_model.txt")
}

/// Fichero en que se guarda un modelo a partir de lo que devuelve el diálogo de guardar: el juego los busca por
/// nombre, así que siempre es `<nombre>_model.json`. Si al corregir el nombre cae encima de otro modelo (por el que el
/// diálogo no ha preguntado), error.
pub fn model_target(picked: &Path) -> Result<PathBuf> {
    let file = picked.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let stem = file.trim_end_matches(".json").trim_end_matches("_model");
    anyhow::ensure!(!stem.is_empty(), "el modelo necesita un nombre");
    let target = picked.with_file_name(format!("{stem}_model.json"));
    anyhow::ensure!(
        target == picked || !target.exists(),
        "ya hay un modelo del juego que se llama «{stem}». Elige otro nombre, o ábrelo y edítalo."
    );
    Ok(target)
}

fn disk_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// `.meta` de Unity para un `_model.json` nuevo (los que ya existen no se tocan: el juego los enlaza por su GUID).
fn unity_meta() -> String {
    use std::hash::{BuildHasher, Hasher};
    // Dos hashes con semilla aleatoria del sistema: 128 bits sin añadir dependencias.
    let part = || std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!(
        "fileFormatVersion: 2\nguid: {:016x}{:016x}\nTextScriptImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n",
        part(),
        part()
    )
}

/// Escribe `text` en `path` sin dejarlo a medias (temporal en la misma carpeta y renombrar).
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("no puedo escribir {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("no puedo escribir {}", path.display()))
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

    /// Abre un modelo de vóxeles del juego. La copia de recuperación se abre sin ruta (hay que decir dónde guardarla).
    pub fn open_model(&mut self, path: &Path) -> Result<()> {
        if path == model_recovery_path() {
            // La copia de recuperación vuelve a su fichero del juego, si se sabe cuál era.
            let origin = std::fs::read_to_string(model_recovery_origin()).ok().map(|t| PathBuf::from(t.trim())).filter(|p| p.is_absolute());
            return self.open_model_as(path, origin);
        }
        self.open_model_as(path, Some(path.to_path_buf()))
    }

    /// Abre el modelo de `file` como si fuera el de `doc_path` (una copia de recuperación); con `None`, sin fichero.
    pub fn open_model_as(&mut self, file: &Path, doc_path: Option<PathBuf>) -> Result<()> {
        let text = std::fs::read_to_string(file).with_context(|| format!("no puedo leer {}", file.display()))?;
        let (model, s) = vmodel::load(&text)?;
        let size = s.grid.size;
        let own = doc_path.as_deref() == Some(file);
        self.set_doc(Doc::new_model(model, s, doc_path.clone()));
        self.disk = doc_path.as_deref().and_then(disk_stamp);
        self.opened_size = Some(size);
        self.model_new = doc_path.is_none();
        self.model_checks();
        if let Some(doc) = self.doc.as_mut() {
            doc.unsaved = !own;
        }
        Ok(())
    }

    /// Modelo nuevo y vacío; no tiene fichero hasta que se guarda.
    pub fn new_model(&mut self, size: [i32; 3]) {
        let (model, s) = Model::new(size);
        self.adopt_model(model, s);
    }

    /// Abre como modelo nuevo algo que no viene de un fichero del juego (vacío o importado).
    pub fn adopt_model(&mut self, model: Model, s: Structure) {
        self.set_doc(Doc::new_model(model, s, None));
        self.model_new = true;
        self.model_checks();
        if let Some(doc) = self.doc.as_mut() {
            doc.unsaved = true;
        }
    }

    /// Modelos: rehace los avisos de la pestaña Validación.
    fn model_checks(&mut self) {
        if let Some(doc) = self.doc.as_mut().filter(|d| d.is_model()) {
            doc.s.checks = vmodel::checks(&doc.s, self.opened_size, self.model_new);
        }
    }

    /// El fichero del modelo abierto ya no es el que se abrió o guardó (un script de arte, git…).
    pub fn changed_on_disk(&self) -> bool {
        match self.doc.as_ref().filter(|d| d.is_model()).and_then(|d| d.path.as_ref()) {
            Some(path) => self.disk.is_some() && disk_stamp(path) != self.disk,
            None => false,
        }
    }

    fn set_doc(&mut self, doc: Doc) {
        // Entre un modelo y una estructura no se pega nada: unos bytes son colores y los otros, materiales.
        if self.doc.as_ref().is_none_or(|d| d.is_model() != doc.is_model()) {
            self.tools.clipboard = None;
            self.tools.hand = None;
        }
        self.disk = None;
        self.opened_size = None;
        self.model_new = false;
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
        if let Some(model) = doc.model.as_mut() {
            // El fichero del juego es la fuente: guardar ya es llevarlo al juego.
            let text = vmodel::save(model, &doc.s)?;
            write_atomic(&path, &text)?;
            let meta = PathBuf::from(format!("{}.meta", path.display()));
            if !meta.exists() {
                std::fs::write(&meta, unity_meta())?;
            }
            // La cabecera y el orden de las caras pasan a ser los de lo recién escrito.
            model.sync(&text)?;
            doc.path = Some(path.clone());
            doc.unsaved = false;
            self.disk = disk_stamp(&path);
            return Ok(path);
        }
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
        anyhow::ensure!(!doc.is_model(), "los modelos se guardan directamente en el juego");
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
            self.model_checks();
        }
        let Some(doc) = self.doc.as_ref() else { return };
        // Los modelos no tienen reglas de juego que validar: el generador no interviene.
        if !doc.is_model()
            && let Some(t) = self.changed_at
            && t.elapsed() >= VALIDATE_AFTER
            && self.validated != self.revision
        {
            self.validated = self.revision;
            self.changed_at = None;
            host.validate(self.revision, &doc.s, doc.meta.replaces.as_ref());
        }
        if doc.unsaved && self.recovered_at.elapsed() >= RECOVER_EVERY {
            self.recovered_at = Instant::now();
            let copy = match &doc.model {
                Some(m) => {
                    let origin = doc.path.as_ref().map_or_else(String::new, |p| p.display().to_string());
                    let _ = std::fs::write(model_recovery_origin(), origin);
                    (Some(model_recovery_path()), vmodel::save(m, &doc.s))
                }
                None => (recovery_path(), agvox::save(&doc.meta, &doc.s, materials)),
            };
            if let (Some(path), Ok(text)) = copy {
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
        // La más reciente de las dos copias (estructura o modelo).
        let modified = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        [recovery_path(), Some(model_recovery_path())].into_iter().flatten().filter(|p| p.is_file()).max_by_key(modified)
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

    /// Carpeta temporal con una copia del cofre y su `.meta`.
    fn chest_dir(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("modelos-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chest_0_model.json");
        std::fs::write(&path, crate::vmodel::tests::lf(crate::vmodel::tests::CHEST)).unwrap();
        std::fs::write(dir.join("chest_0_model.json.meta"), "guid: del-juego\n").unwrap();
        (dir, path)
    }

    fn solids(s: &Structure) -> usize {
        s.grid.data.iter().filter(|&&b| b != 0).count()
    }

    #[test]
    fn guardar_modelo_escribe_en_su_sitio() {
        let (dir, path) = chest_dir("guardar");
        let mut ed = Editor::default();
        ed.open_model(&path).unwrap();
        let doc = ed.doc.as_mut().unwrap();
        assert_eq!(doc.meta.name, "chest_0");
        let n = solids(&doc.s);
        let i = doc.s.grid.data.iter().position(|&b| b != 0).unwrap() as i32;
        let [sx, _, sz] = doc.s.grid.size;
        doc.paint(i % sx, i / (sx * sz), i / sx % sz, 0, false);
        doc.end_stroke();
        assert_eq!(ed.save(None, &[]).unwrap(), path);
        assert!(!ed.doc.as_ref().unwrap().unsaved);
        let (m, s) = crate::vmodel::load(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(m.edited());
        assert_eq!(solids(&s), n - 1);
        assert_eq!(std::fs::read_to_string(dir.join("chest_0_model.json.meta")).unwrap(), "guid: del-juego\n");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "sin ficheros temporales");
        // Guardar otra vez sin tocar nada deja el mismo texto.
        let once = std::fs::read_to_string(&path).unwrap();
        ed.save(None, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), once);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn modelo_nuevo_lleva_meta() {
        let (dir, _) = chest_dir("nuevo");
        let mut ed = Editor::default();
        ed.new_model([8, 8, 8]);
        assert!(ed.save(None, &[]).is_err(), "sin ruta no se guarda");
        let path = dir.join("farol_model.json");
        ed.save(Some(path.clone()), &[]).unwrap();
        let meta = std::fs::read_to_string(dir.join("farol_model.json.meta")).unwrap();
        let lines: Vec<&str> = meta.lines().collect();
        assert_eq!(lines[0], "fileFormatVersion: 2");
        let guid = lines[1].strip_prefix("guid: ").unwrap();
        assert!(guid.len() == 32 && guid.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(lines[2], "TextScriptImporter:");
        ed.save(None, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("farol_model.json.meta")).unwrap(), meta);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn guardar_avisa_si_cambio_en_disco() {
        let (dir, path) = chest_dir("disco");
        let mut ed = Editor::default();
        ed.open_model(&path).unwrap();
        assert!(!ed.changed_on_disk());
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{text}\n")).unwrap();
        assert!(ed.changed_on_disk());
        ed.save(None, &[]).unwrap();
        assert!(!ed.changed_on_disk());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn el_nombre_al_guardar_no_pisa_otro_modelo() {
        let (dir, path) = chest_dir("nombre");
        // Lo que devuelve el diálogo se lleva a `<nombre>_model.json`.
        assert_eq!(model_target(&dir.join("farol.json")).unwrap(), dir.join("farol_model.json"));
        assert_eq!(model_target(&dir.join("farol")).unwrap(), dir.join("farol_model.json"));
        assert_eq!(model_target(&dir.join("farol_model.json")).unwrap(), dir.join("farol_model.json"));
        // Elegir el propio fichero (el diálogo ya preguntó si reemplazarlo) vale.
        assert_eq!(model_target(&path).unwrap(), path);
        // Escribir «chest_0» no puede acabar encima del cofre sin que nadie pregunte.
        assert!(model_target(&dir.join("chest_0.json")).is_err());
        assert!(model_target(&dir.join("chest_0")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn la_copia_de_recuperacion_vuelve_a_su_fichero() {
        let (dir, path) = chest_dir("recuperar");
        let copy = dir.join("recuperacion_model.json");
        std::fs::copy(&path, &copy).unwrap();
        let mut ed = Editor::default();
        ed.open_model_as(&copy, Some(path.clone())).unwrap();
        let doc = ed.doc.as_ref().unwrap();
        assert_eq!(doc.path.as_deref(), Some(path.as_path()));
        assert_eq!(doc.meta.name, "chest_0");
        assert!(doc.unsaved, "lo recuperado aún no está en su fichero");
        assert!(!ed.changed_on_disk());
        // Sin saber de dónde venía, se abre sin ruta.
        ed.open_model_as(&copy, None).unwrap();
        assert!(ed.doc.as_ref().unwrap().path.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cambiar_de_documento_vacia_el_portapapeles() {
        let (dir, path) = chest_dir("portapapeles");
        let mut ed = Editor::default();
        ed.open_model(&path).unwrap();
        ed.tools.selection = Some(([0, 0, 0], [3, 3, 3]));
        assert!(ed.tools.copy(ed.doc.as_ref().unwrap()));
        // Los bytes de un modelo son colores y los de una estructura, materiales: no se mezclan.
        ed.fork(&Structure { grid: VoxelGrid::new([8, 8, 8]), ..Default::default() }, "x".into(), String::new());
        assert!(ed.tools.clipboard.is_none() && ed.tools.hand.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn modelo_sin_errores_del_juego() {
        let (dir, path) = chest_dir("errores");
        let mut ed = Editor::default();
        ed.open_model(&path).unwrap();
        assert_eq!(ed.errors(), 0);
        assert!(ed.open_model(&dir.join("chest_0_model.json.meta")).is_err());
        assert!(ed.doc.as_ref().unwrap().is_model(), "un fallo al abrir no cierra lo que había");
        std::fs::remove_dir_all(&dir).unwrap();
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
