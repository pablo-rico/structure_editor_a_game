//! Biblioteca del modo Editar (docs/diseno-gui.md §10, «Biblioteca»): estructuras propias y recientes, diseños del
//! juego para copiar o editar, y piezas (muebles del juego y sellos propios).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, StrokeKind, TextureHandle, Ui, vec2};
use egui_phosphor::regular as icon;

use super::widgets::{self, dim, section};
use crate::agvox::{self, Meta, Replaces};
use crate::bridge::proto::{Catalog, FurnitureCatalog};
use crate::labels::ModelGroup;
use crate::mesh::Paint;
use crate::model::{SUB, Structure};
use crate::params::Params;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Structures,
    Game,
    Pieces,
    /// Modelos de vóxeles del juego (árboles, objetos, intro…).
    Models,
}

/// Un `_model.json` del juego. Se carga la primera vez que hace falta su miniatura.
pub struct ModelEntry {
    pub path: PathBuf,
    /// Nombre del fichero sin `_model.json`.
    pub name: String,
    pub group: ModelGroup,
    pub modified: SystemTime,
    /// Lo cargado (con qué se pinta y si ya se retocó en el editor), o por qué no se puede abrir.
    pub loaded: Option<Result<(Arc<Structure>, Paint, bool), String>>,
}

/// Los modelos que hay bajo `root` (la carpeta de arte del juego), por grupo y nombre. `old` es la lista anterior:
/// lo ya cargado se conserva si el fichero no ha cambiado.
pub fn scan_models(root: &Path, old: Vec<ModelEntry>) -> Vec<ModelEntry> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                walk(&path, out);
            } else if path.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_model.json")) {
                out.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
            }
        }
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut old: HashMap<PathBuf, ModelEntry> = old.into_iter().map(|e| (e.path.clone(), e)).collect();
    let mut out: Vec<ModelEntry> = files
        .into_iter()
        .map(|(path, modified)| {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
            let file = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let loaded = old.remove(&path).filter(|o| o.modified == modified).and_then(|o| o.loaded);
            ModelEntry { name: file.trim_end_matches("_model.json").to_string(), group: ModelGroup::of(&rel), path, modified, loaded }
        })
        .collect();
    out.sort_by(|a, b| (a.group, a.name.to_lowercase()).cmp(&(b.group, b.name.to_lowercase())));
    out
}

/// Un `.agvox` en disco (estructura propia o sello).
pub struct Entry {
    pub path: PathBuf,
    pub meta: Meta,
    pub s: Arc<Structure>,
    pub modified: SystemTime,
}

/// Hace una miniatura con el visor: clave de textura, qué pintar y con qué paleta (`None` = la del catálogo).
pub type Thumb<'a> = dyn FnMut(&str, &Structure, Option<&Paint>) -> Option<TextureHandle> + 'a;

/// Lo que la Biblioteca pide a la app.
pub enum Action {
    New,
    Open(PathBuf),
    Duplicate(PathBuf),
    Delete(PathBuf),
    ExportVox(PathBuf),
    ImportVox,
    /// «Del juego»: copiar como estructura nueva o editar ese diseño (sustituirlo en el juego).
    CopyGame,
    EditGame,
    Piece(String),
    Stamp(PathBuf),
    OpenModel(PathBuf),
}

pub struct Library {
    pub section: Section,
    pub search: String,
    pub entries: Vec<Entry>,
    pub stamps: Vec<Entry>,
    pub models: Vec<ModelEntry>,
    scanned: Option<Instant>,
    pub thumbs: HashMap<String, TextureHandle>,
    /// «Del juego»: parámetros, medianeras y lo construido (original, sin sustituciones).
    pub game: Option<Params>,
    pub game_party: u32,
    pub game_s: Option<Arc<Structure>>,
    pub game_thumb: Option<TextureHandle>,
    pub game_asked: String,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            section: Section::Structures,
            search: String::new(),
            entries: Vec::new(),
            stamps: Vec::new(),
            models: Vec::new(),
            scanned: None,
            thumbs: HashMap::new(),
            game: None,
            game_party: 0,
            game_s: None,
            game_thumb: None,
            game_asked: String::new(),
        }
    }
}

/// Carpeta de los sellos propios.
pub fn stamps_dir() -> PathBuf {
    crate::platform::data_dir().join("sellos")
}

fn scan(dir: &Path, materials: &[String]) -> Vec<Entry> {
    let mut out: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "agvox").then_some(())?;
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            let (meta, s) = agvox::load(&std::fs::read_to_string(&path).ok()?, materials).ok()?;
            Some(Entry { path, meta, s: Arc::new(s), modified })
        })
        .collect();
    out.sort_by_key(|e| e.meta.name.to_lowercase());
    out
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn lista_de_modelos_sin_cargarlos() {
        let root = std::env::temp_dir().join(format!("modelos-lista-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (voxel, trees) = (root.join("Objects/Voxel"), root.join("Objects/Trees3D/voxel"));
        std::fs::create_dir_all(&voxel).unwrap();
        std::fs::create_dir_all(&trees).unwrap();
        std::fs::write(voxel.join("roca_model.json"), "{}").unwrap();
        std::fs::write(voxel.join("roca_model.json.meta"), "").unwrap();
        std::fs::write(voxel.join("otra_cosa.json"), "{}").unwrap();
        std::fs::write(trees.join("abeto_model.json"), "{}").unwrap();
        let found = scan_models(&root, Vec::new());
        let names: Vec<(&str, ModelGroup)> = found.iter().map(|e| (e.name.as_str(), e.group)).collect();
        assert_eq!(names, [("abeto", ModelGroup::Trees), ("roca", ModelGroup::Nature)]);
        assert!(found.iter().all(|e| e.loaded.is_none()), "se cargan al pedir la miniatura");
        // Lo ya cargado se conserva mientras el fichero no cambie.
        let mut found = found;
        found[0].loaded = Some(Err("x".into()));
        let again = scan_models(&root, found);
        assert!(again[0].loaded.is_some() && again[1].loaded.is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

/// Clave de miniatura de un fichero: ruta + fecha (cambia al guardar).
fn thumb_key(e: &Entry) -> String {
    let t = e.modified.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    format!("{}@{t}", e.path.display())
}

/// Campos que hacen de «diseño» en la clave de una sustitución, según el tipo (docs/contrato.md): `design`, el tipo
/// de tienda, el mueble urbano, el arco viejo del pasaje o la torre del puente. Los tipos sin campos usan 0.
pub const DESIGN_FIELDS: [&str; 5] = ["design", "shop", "prop", "oldGate", "gateTower"];

/// El campo de diseño de unos parámetros y su valor.
pub fn design_field(fields: &std::collections::BTreeMap<String, i64>) -> Option<(&'static str, i64)> {
    DESIGN_FIELDS.iter().find_map(|k| fields.get(*k).map(|v| (*k, *v)))
}

/// Texto de una sustitución para artistas: «Ayuntamiento · Norteño · Diseño 2 · 10×10».
pub fn describe(r: &Replaces, cat: &Catalog) -> String {
    let kind = cat.kind(&r.kind).map_or(r.kind.as_str(), |k| k.label.as_str()).to_string();
    let style = cat.styles.iter().find(|s| s.name == r.style).map_or(r.style.clone(), |s| s.label.clone());
    let mut t = format!("{kind} · {style} · Diseño {} · {}×{}", r.design + 1, r.size[0], r.size[1]);
    for (lado, bit) in [("izquierda", 1), ("derecha", 0), ("detrás", 2)] {
        if r.party >> bit & 1 != 0 {
            t.push_str(&format!(" · medianera {lado}"));
        }
    }
    t
}

impl Library {
    /// Vuelve a leer las carpetas (como mucho cada 3 s, o ya si `force`).
    pub fn refresh(&mut self, agame_root: &Path, materials: &[String], force: bool) {
        if !force && self.scanned.is_some_and(|t| t.elapsed().as_secs() < 3) {
            return;
        }
        self.scanned = Some(Instant::now());
        self.entries = scan(&crate::editor::source_dir(agame_root), materials);
        self.stamps = scan(&stamps_dir(), materials);
        // La carpeta de arte es grande: solo se recorre con la sección a la vista.
        if self.section == Section::Models {
            self.models = scan_models(&crate::editor::models_root(agame_root), std::mem::take(&mut self.models));
        }
    }

    /// El `.agvox` que ya sustituye exactamente ese diseño (sea cual sea su alcance de barrio o clase).
    pub fn override_of(&self, key: &Replaces) -> Option<&Entry> {
        self.entries.iter().find(|e| {
            e.meta.replaces.as_ref().is_some_and(|r| {
                r.kind == key.kind && r.style == key.style && r.design == key.design && r.size == key.size && r.party == key.party
            })
        })
    }

    /// Dibuja la Biblioteca. `thumb` hace una miniatura (con el visor) cuando falta; `current` es el fichero abierto.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        cat: &Catalog,
        furniture: Option<&FurnitureCatalog>,
        recent: &[PathBuf],
        current: Option<&Path>,
        thumb: &mut Thumb,
    ) -> Option<Action> {
        let mut act = None;
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(format!("{}  Buscar en la biblioteca", icon::MAGNIFYING_GLASS)));
        let mut sec = self.section;
        widgets::segmented(
            ui,
            &mut sec,
            &[
                (Section::Structures, "Estructuras".into()),
                (Section::Game, "Del juego".into()),
                (Section::Pieces, "Piezas".into()),
                (Section::Models, "Modelos".into()),
            ],
        );
        if sec != self.section {
            self.scanned = None; // la sección nueva se lee ya
        }
        self.section = sec;
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.section {
            Section::Structures => act = self.structures(ui, cat, recent, current, thumb),
            Section::Game => act = self.game_section(ui, cat),
            Section::Pieces => act = self.pieces(ui, furniture, thumb),
            Section::Models => act = self.models_section(ui, current, thumb),
        });
        act
    }

    /// Los modelos de vóxeles del juego, por grupos. Cada uno se carga cuando su tarjeta entra en pantalla.
    fn models_section(&mut self, ui: &mut Ui, current: Option<&Path>, thumb: &mut Thumb) -> Option<Action> {
        let mut act = None;
        if self.models.is_empty() {
            ui.label(dim("No encuentro modelos en la carpeta del juego."));
            return None;
        }
        let search = self.search.to_lowercase();
        let mut any = false;
        let mut loads = 2; // modelos leídos por fotograma, para no trabar la interfaz
        for group in ModelGroup::ALL {
            let shown: Vec<usize> = (0..self.models.len())
                .filter(|&i| self.models[i].group == group && (search.is_empty() || self.models[i].name.to_lowercase().contains(&search)))
                .collect();
            if shown.is_empty() {
                continue;
            }
            any = true;
            section(ui, &format!("{} ({})", group.label(), shown.len()));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for i in shown {
                    let e = &self.models[i];
                    let t = e.modified.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                    let key = format!("{}@{t}", e.path.display());
                    let line = match &e.loaded {
                        None => "…".to_string(),
                        Some(Err(_)) => format!("{} No se puede abrir", icon::WARNING),
                        Some(Ok((s, _, edited))) => {
                            let [sx, sy, sz] = s.grid.size;
                            format!("{sx}×{sy}×{sz}{}", if *edited { " · retocado" } else { "" })
                        }
                    };
                    let r = card(ui, &e.name, &line, self.thumbs.get(&key), current == Some(e.path.as_path()), false);
                    let visible = ui.is_rect_visible(r.rect);
                    let r = match &e.loaded {
                        Some(Err(why)) => r.on_hover_text(format!("Este modelo no se puede abrir: {why}")),
                        Some(Ok((_, _, true))) => r.on_hover_text(format!("{} · retocado a mano en el editor", e.name)),
                        _ => r.on_hover_text(&e.name),
                    };
                    if r.clicked() && !matches!(e.loaded, Some(Err(_))) {
                        act = Some(Action::OpenModel(e.path.clone()));
                    }
                    r.context_menu(|ui| {
                        if ui.button(format!("{} {}", icon::FOLDER_OPEN, crate::platform::REVEAL_LABEL)).clicked() {
                            crate::platform::reveal(&e.path);
                            ui.close();
                        }
                    });
                    if !visible {
                        continue;
                    }
                    if e.loaded.is_none() && loads > 0 {
                        loads -= 1;
                        let loaded = std::fs::read_to_string(&e.path)
                            .map_err(anyhow::Error::from)
                            .and_then(|t| crate::vmodel::load(&t))
                            .map(|(m, s)| (Arc::new(s), m.paint(), m.edited()))
                            .map_err(|err| format!("{err:#}"));
                        self.models[i].loaded = Some(loaded);
                        ui.ctx().request_repaint();
                    } else if let Some(Ok((s, paint, _))) = &e.loaded
                        && !self.thumbs.contains_key(&key)
                        && let Some(t) = thumb(&key, s, Some(paint))
                    {
                        self.thumbs.insert(key, t);
                    }
                }
            });
        }
        if !any {
            ui.label(dim(format!("Nada con «{}»", self.search)));
        }
        act
    }

    fn matches(&self, name: &str) -> bool {
        self.search.is_empty() || name.to_lowercase().contains(&self.search.to_lowercase())
    }

    fn structures(
        &mut self,
        ui: &mut Ui,
        cat: &Catalog,
        recent: &[PathBuf],
        current: Option<&Path>,
        thumb: &mut Thumb,
    ) -> Option<Action> {
        let mut act = None;
        ui.horizontal(|ui| {
            // Con la biblioteca vacía, «Nueva» va en el estado vacío (una sola acción).
            if !self.entries.is_empty() && ui.button(format!("{} Nueva", icon::FILE_PLUS)).clicked() {
                act = Some(Action::New);
            }
            if ui.button(format!("{} Importar de MagicaVoxel…", icon::DOWNLOAD_SIMPLE)).clicked() {
                act = Some(Action::ImportVox);
            }
        });
        let recent: Vec<&PathBuf> = recent.iter().filter(|p| p.is_file()).take(6).collect();
        if !recent.is_empty() && self.search.is_empty() {
            section(ui, "Recientes");
            for p in recent {
                let name = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                if ui.selectable_label(current == Some(p.as_path()), format!("{}  {name}", icon::CLOCK_COUNTER_CLOCKWISE)).clicked() {
                    act = Some(Action::Open(p.clone()));
                }
            }
        }
        section(ui, "Mis estructuras");
        let shown: Vec<usize> = (0..self.entries.len()).filter(|&i| self.matches(&self.entries[i].meta.name)).collect();
        if self.entries.is_empty() {
            ui.label(dim("Aún no has hecho ninguna."));
            if widgets::primary(ui, &format!("{} Nueva estructura", icon::FILE_PLUS)).clicked() {
                act = Some(Action::New);
            }
        } else if shown.is_empty() {
            ui.label(dim(format!("Nada con «{}»", self.search)));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for i in shown {
                let e = &self.entries[i];
                let key = thumb_key(e);
                if !self.thumbs.contains_key(&key)
                    && let Some(t) = thumb(&key, &e.s, None)
                {
                    self.thumbs.insert(key.clone(), t);
                }
                let [sx, sy, sz] = e.s.grid.size;
                let line = match &e.meta.replaces {
                    Some(r) => format!("Sustituye · diseño {}", r.design + 1),
                    None => format!("{}×{} celdas · {} plantas", sx / SUB, sz / SUB, e.s.band_floors.len().max(1)),
                };
                let tip = e.meta.replaces.as_ref().map(|r| format!("Sustituye en el juego a {}", describe(r, cat)));
                let r = card(ui, &e.meta.name, &line, self.thumbs.get(&key), current == Some(e.path.as_path()), e.meta.replaces.is_some());
                let r = match tip {
                    Some(t) => r.on_hover_text(t),
                    None => r.on_hover_text(format!("{} · {} niveles", e.meta.name, sy / SUB)),
                };
                if r.clicked() {
                    act = Some(Action::Open(e.path.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button(format!("{} Duplicar", icon::COPY)).clicked() {
                        act = Some(Action::Duplicate(e.path.clone()));
                        ui.close();
                    }
                    if ui.button(format!("{} Exportar a MagicaVoxel…", icon::EXPORT)).clicked() {
                        act = Some(Action::ExportVox(e.path.clone()));
                        ui.close();
                    }
                    if ui.button(format!("{} {}", icon::FOLDER_OPEN, crate::platform::REVEAL_LABEL)).clicked() {
                        crate::platform::reveal(&e.path);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(RichText::new(format!("{} Borrar…", icon::TRASH)).color(theme::ERROR)).clicked() {
                        act = Some(Action::Delete(e.path.clone()));
                        ui.close();
                    }
                });
            }
        });
        act
    }

    fn game_section(&mut self, ui: &mut Ui, cat: &Catalog) -> Option<Action> {
        let mut act = None;
        let p = self.game.get_or_insert_with(|| {
            let k = cat.kind("TownHall").or(cat.kinds.first()).expect("catálogo vacío");
            Params::defaults(k, cat)
        });
        // Tipo, por grupos.
        let label = cat.kind(&p.kind).map_or(p.kind.clone(), |k| k.label.clone());
        let mut pick = None;
        egui::ComboBox::from_id_salt("lib-tipo").selected_text(label).width(220.0).show_ui(ui, |ui| {
            for g in &cat.groups {
                ui.label(dim(&g.label));
                for k in cat.kinds.iter().filter(|k| k.group == g.key) {
                    if ui.selectable_label(k.name == p.kind, &k.label).clicked() {
                        pick = Some(k.name.clone());
                    }
                }
            }
        });
        if let Some(k) = pick.and_then(|n| cat.kind(&n)) {
            *p = Params::defaults(k, cat);
            self.game_party = 0;
        }
        let kind = cat.kind(&p.kind).cloned()?;
        if !kind.styles.is_empty() {
            let opts: Vec<(String, String)> = kind
                .styles
                .iter()
                .map(|n| (n.clone(), cat.styles.iter().find(|s| &s.name == n).map_or(n.clone(), |s| s.label.clone())))
                .collect();
            if widgets::segmented(ui, &mut p.style, &opts) {
                p.fit(&kind, cat);
            }
        }
        for f in &kind.fields {
            if let Some(v) = p.fields.get_mut(&f.key) {
                if !matches!(f.kind, crate::bridge::proto::FieldKind::Bool | crate::bridge::proto::FieldKind::Index { .. }) {
                    ui.label(dim(&f.label));
                }
                super::options::field(ui, cat, f, v, &p.style);
            }
        }
        if let Some(r) = kind.size {
            for (name, v, lo, hi) in [("Ancho", &mut p.width, r.min[0], r.max[0]), ("Fondo", &mut p.depth, r.min[1], r.max[1])] {
                ui.horizontal(|ui| {
                    ui.label(name);
                    if lo == hi {
                        ui.label(RichText::new(lo.to_string()).font(theme::mono(13.0)));
                    } else {
                        ui.add(egui::Slider::new(v, lo..=hi).integer());
                    }
                });
            }
        }
        let city = kind.group == "city";
        if city {
            ui.label("Medianeras");
            ui.horizontal(|ui| {
                for (lado, bit) in [("izquierda", 1u32), ("derecha", 0), ("detrás", 2)] {
                    let mut on = self.game_party >> bit & 1 != 0;
                    if ui.checkbox(&mut on, lado).changed() {
                        self.game_party ^= 1 << bit;
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Semilla");
            widgets::seed(ui, &mut p.seed);
        });
        // Vista previa del original (la pide la app con `procedural`).
        let w = ui.available_width().max(160.0);
        let size = vec2(w, w * 0.75);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, 2.0, theme::VIEW_BG);
        theme::bevel(ui.painter(), rect, false);
        match &self.game_thumb {
            Some(t) => {
                let s = t.size_vec2();
                let k = (size.x / s.x).min(size.y / s.y);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(t.id(), egui::Rect::from_center_size(rect.center(), s * k), uv, egui::Color32::WHITE);
            }
            None => {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "…", FontId::proportional(20.0), theme::TEXT_DIM);
            }
        }
        ui.add_space(4.0);
        let ready = self.game_s.is_some();
        if ui.add_enabled(ready, egui::Button::new(format!("{}  Copiar como estructura nueva", icon::COPY)).min_size(vec2(w, 28.0))).clicked() {
            act = Some(Action::CopyGame);
        }
        ui.label(dim("Una estructura tuya, aparte. El juego no cambia."));
        if city {
            let existing = self.game_key(cat).and_then(|k| self.override_of(&k).map(|e| e.path.clone()));
            let text = if existing.is_some() { "Abrir el diseño editado" } else { "Editar este diseño del juego" };
            let b = egui::Button::new(format!("{}  {text}", icon::PENCIL_SIMPLE_LINE)).min_size(vec2(w, 28.0));
            if ui.add_enabled(ready, b).clicked() {
                act = Some(match existing {
                    Some(path) => Action::Open(path),
                    None => Action::EditGame,
                });
            }
            ui.label(dim("Tu versión sustituye a este diseño en todas las ciudades del juego."));
        }
        act
    }

    /// Clave de sustitución de lo elegido en «Del juego» (diseño = campo `design`, o el tipo de tienda).
    pub fn game_key(&self, cat: &Catalog) -> Option<Replaces> {
        let p = self.game.as_ref()?;
        cat.kind(&p.kind)?;
        let design = design_field(&p.fields).map_or(0, |(_, v)| v);
        Some(Replaces {
            kind: p.kind.clone(),
            style: p.style.clone(),
            design,
            size: [p.width, p.depth],
            party: self.game_party,
            ..Default::default()
        })
    }

    fn pieces(
        &mut self,
        ui: &mut Ui,
        furniture: Option<&FurnitureCatalog>,
        thumb: &mut Thumb,
    ) -> Option<Action> {
        let mut act = None;
        match furniture {
            None => {
                ui.label(dim("Los muebles del juego llegan cuando el generador esté listo."));
            }
            Some(f) => {
                for c in &f.categories {
                    let pieces: Vec<_> = f.pieces.iter().filter(|p| p.category == c.key && self.matches(&p.label)).collect();
                    if pieces.is_empty() {
                        continue;
                    }
                    egui::CollapsingHeader::new(RichText::new(format!("{}  {}", category_icon(&c.key), c.label)).font(theme::semibold(13.0)))
                        .id_salt(("muebles", &c.key))
                        .default_open(false)
                        .open(if self.search.is_empty() { None } else { Some(true) })
                        .show_unindented(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                                for p in pieces {
                                    let key = format!("mueble:{}", p.id);
                                    let r = small_card(ui, &p.label, self.thumbs.get(&key));
                                    let tip = format!("{} · {}×{}×{} sub-cubos{}", p.label, p.size[0], p.size[1], p.size[2], if p.interacts { " · se puede usar" } else { "" });
                                    if r.on_hover_text(tip).clicked() {
                                        act = Some(Action::Piece(p.id.clone()));
                                    }
                                }
                            });
                        });
                }
            }
        }
        section(ui, "Mis sellos");
        if self.stamps.is_empty() {
            ui.label(dim("Selecciona un trozo (M) y pulsa «Guardar como sello» en Propiedades."));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for e in self.stamps.iter().filter(|e| self.search.is_empty() || e.meta.name.to_lowercase().contains(&self.search.to_lowercase())) {
                let key = thumb_key(e);
                if !self.thumbs.contains_key(&key)
                    && let Some(t) = thumb(&key, &e.s, None)
                {
                    self.thumbs.insert(key.clone(), t);
                }
                let r = small_card(ui, &e.meta.name, self.thumbs.get(&key));
                if r.clicked() {
                    act = Some(Action::Stamp(e.path.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button(RichText::new(format!("{} Borrar sello", icon::TRASH)).color(theme::ERROR)).clicked() {
                        act = Some(Action::Delete(e.path.clone()));
                        ui.close();
                    }
                });
            }
        });
        act
    }
}

/// Icono de cada categoría de muebles (§10).
pub fn category_icon(key: &str) -> &'static str {
    match key {
        "bedroom" => icon::BED,
        "kitchen" => icon::COOKING_POT,
        "shop" => icon::STOREFRONT,
        "church" => icon::CROSS,
        "jail" => icon::KEY,
        "fields" => icon::GRAINS,
        "school" => icon::MAGIC_WAND,
        _ => icon::FLOWER_TULIP,
    }
}

fn card(ui: &mut Ui, name: &str, line: &str, thumb: Option<&TextureHandle>, open: bool, swap: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(112.0, 128.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, theme::STONE1);
    theme::bevel(p, rect, true);
    let well = egui::Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(104.0, 80.0));
    p.rect_filled(well, 2.0, theme::HOLE);
    if let Some(t) = thumb {
        let s = t.size_vec2();
        let k = (well.width() / s.x).min(well.height() / s.y);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        p.image(t.id(), egui::Rect::from_center_size(well.center(), s * k), uv, egui::Color32::WHITE);
    }
    if swap {
        // Placa cian (no dorada: el dorado es solo lo activo).
        let tag = egui::Rect::from_min_size(well.min + vec2(3.0, 3.0), vec2(22.0, 16.0));
        p.rect_filled(tag, 2.0, theme::alpha(theme::STONE0, 0.9));
        p.text(tag.center(), Align2::CENTER_CENTER, icon::SWAP, FontId::proportional(12.0), theme::CYAN);
    }
    let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), FontId::proportional(13.0), theme::TEXT);
    job.wrap = egui::text::TextWrapping { max_width: 100.0, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    p.galley(egui::pos2(rect.left() + 6.0, well.bottom() + 4.0), p.layout_job(job), theme::TEXT);
    p.text(egui::pos2(rect.left() + 6.0, well.bottom() + 22.0), Align2::LEFT_TOP, line, FontId::proportional(11.5), theme::TEXT_DIM);
    if open {
        p.rect_stroke(rect, 2.0, Stroke::new(2.0, theme::GOLD), StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_stroke(rect, 2.0, Stroke::new(1.0, theme::STONE3), StrokeKind::Inside);
    }
    resp
}

fn small_card(ui: &mut Ui, name: &str, thumb: Option<&TextureHandle>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(76.0, 98.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, theme::STONE1);
    theme::bevel(p, rect, true);
    let well = egui::Rect::from_min_size(rect.min + vec2(3.0, 3.0), vec2(70.0, 58.0));
    p.rect_filled(well, 2.0, theme::HOLE);
    if let Some(t) = thumb {
        let s = t.size_vec2();
        let k = (well.width() / s.x).min(well.height() / s.y);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        p.image(t.id(), egui::Rect::from_center_size(well.center(), s * k), uv, egui::Color32::WHITE);
    }
    let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), FontId::proportional(11.5), theme::TEXT);
    job.wrap = egui::text::TextWrapping { max_width: 70.0, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
    p.galley(egui::pos2(rect.left() + 4.0, well.bottom() + 4.0), p.layout_job(job), theme::TEXT);
    if resp.hovered() {
        p.rect_stroke(rect, 2.0, Stroke::new(1.0, theme::STONE3), StrokeKind::Inside);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_y_sustitucion_existente() {
        let cat: Catalog = serde_json::from_value(serde_json::json!({
            "materials": ["Air"], "palette": [[0,0,0]], "elements": [],
            "styles": [{"name": "Northern", "label": "Norteño"}],
            "facings": [], "damages": [], "groups": [],
            "kinds": [{"id": 6, "name": "TownHall", "label": "Ayuntamiento", "group": "city"}]
        }))
        .unwrap();
        let r = Replaces { kind: "TownHall".into(), style: "Northern".into(), design: 1, size: [10, 10], party: 2, ..Default::default() };
        assert_eq!(describe(&r, &cat), "Ayuntamiento · Norteño · Diseño 2 · 10×10 · medianera izquierda");
        let mut lib = Library::default();
        lib.entries.push(Entry {
            path: "/x/a.agvox".into(),
            meta: Meta { replaces: Some(Replaces { district: Some("Noble".into()), ..r.clone() }), ..Default::default() },
            s: Arc::new(Structure::default()),
            modified: SystemTime::UNIX_EPOCH,
        });
        // Aunque esté acotada a un barrio, es la sustitución de ese diseño.
        assert!(lib.override_of(&r).is_some());
        assert!(lib.override_of(&Replaces { design: 0, ..r }).is_none());
    }
}
