//! Modo Editar (docs/diseno-gui.md §5, §8 y §10): tira de herramientas vertical, barra contextual con la planta de
//! trabajo, visor con pincel, construcción con los pinceles del juego (vista previa en vivo), colocar muebles y sellos,
//! Biblioteca, MagicaVoxel, Aspecto, sustitución de diseños del juego, abrir/guardar/exportar y atajos.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Key, Rect, RichText, Ui, UiBuilder, vec2};
use egui_phosphor::regular as icon;
use serde_json::json;

use super::{Mode as AppMode, State, Tab, loading, view};
use crate::agvox::{self, Look, Replaces};
use crate::bridge::proto::{Catalog, ErrorInfo, FurnitureCatalog, Stamp, ToolInfo};
use crate::edit::ops::{self, BoxShape, Clip, V};
use crate::edit::tools::{Hand, Mode, Pointer, Preview, Tool};
use crate::editor::source_dir;
use crate::mesh::Paint;
use crate::model::{SUB, Structure, VoxelGrid};
use crate::pick;
use crate::render::{Gizmo, ViewportResponse};
use crate::theme;
use crate::ui::library::{self, Action, Library};
use crate::ui::vox_dialog::{VoxDialog, VoxOutcome};
use crate::ui::widgets::{self, ToastKind, dim};
use crate::ui::{inspector, properties};

/// Qué se iba a hacer cuando apareció «hay cambios sin guardar» (o una confirmación de borrado).
pub(super) enum Confirm {
    Fork,
    New,
    Open(PathBuf),
    Delete(PathBuf),
    /// El modelo abierto ha cambiado en disco desde que se abrió: guardar lo pisaría.
    Overwrite,
}

/// Con qué se pinta lo que se edita: la paleta del modelo o la del catálogo (con el tema de la Paleta).
fn paint_of(doc: Option<&crate::edit::Doc>, cat: &Catalog, theme: Option<usize>) -> Paint {
    match doc.and_then(|d| d.model.as_ref()) {
        Some(m) => m.paint(),
        None => Paint::new(cat, theme, None),
    }
}

/// Un modelo de vóxeles del juego (y no un `.agvox`), por el nombre del fichero.
fn is_model_path(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e == "json")
}

/// Gesto de construcción (celdas en el suelo; `dir` para caras y escaleras; `y` del sub-cubo señalado).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gesture {
    anchor: [i32; 2],
    current: [i32; 2],
    dir: i32,
    y: i32,
}

/// Estado de Editar de la fase 3 (construcción, piezas, biblioteca, Aspecto).
#[derive(Default)]
pub(super) struct EditUi {
    pub tools_cat: Vec<ToolInfo>,
    params: HashMap<String, BTreeMap<String, i64>>,
    pub furniture: Option<FurnitureCatalog>,
    furniture_asked: HashSet<String>,
    /// Papeles de material del Aspecto que da el generador (los cambiados a mano van en el documento).
    pub roles: BTreeMap<String, String>,
    look_asked: String,
    /// Papel que espera un material de la Paleta.
    role_pick: Option<String>,
    /// Planta de trabajo (índice de franja); `None` = Todo.
    pub floor: Option<usize>,
    gesture: Option<Gesture>,
    pressed: bool,
    preview: Option<Arc<Structure>>,
    preview_error: Option<String>,
    seq: u64,
    sent_sig: String,
    received: u64,
    commit_on: Option<(u64, String)>,
    /// Lo que se ve en el visor: (vista previa, número) para no volver a mallar.
    shown: Option<(bool, u64)>,
    pub library: Library,
    pub vox: Option<VoxDialog>,
    /// Diseños del juego ya avisados (el aviso modal sale una vez por diseño).
    warned: HashSet<String>,
    warn: Option<String>,
    /// Rango, barrio y clase del original (para acotar la sustitución).
    source_scope: Option<(Option<i64>, Option<String>, Option<String>)>,
    stamp_name: Option<String>,
    /// Explorar enseña el original aunque haya una sustitución.
    pub explore_original: bool,
    /// Si el Aspecto se abrió para construir (se cierra al dejar de construir).
    look_for: Option<bool>,
}

/// Pista de una herramienta de construcción según su gesto (barra de estado y tooltip de la tira).
pub(super) fn build_hint(target: &str) -> &'static str {
    match target {
        "rect" => "Arrastra para marcar las celdas (o clic en una) · Suelta para construir · Esc cancela",
        "line" => "Arrastra de un extremo al otro del muro · Suelta para construir · Esc cancela",
        "face" => "Clic en la cara de un muro",
        _ => "Clic en una celda; arrastra hacia donde sube o mira",
    }
}

/// Glifo Phosphor por nombre (el catálogo de herramientas lo da en texto); un cubo si no existe.
fn glyph(name: &str) -> &'static str {
    icon::ICONS.iter().find(|(n, _)| *n == name).map_or(icon::CUBE, |(_, g)| *g)
}

/// Control compacto para la barra contextual: desplegable en las listas y número en los rangos (los segmentados no
/// caben en una fila).
fn compact_field(ui: &mut Ui, cat: &Catalog, f: &crate::bridge::proto::Field, v: &mut i64, style: &str) -> bool {
    use crate::bridge::proto::FieldKind;
    match &f.kind {
        FieldKind::Choice { options } => {
            let before = *v;
            let current = options.iter().find(|o| o.value == *v).map_or("—", |o| o.label.as_str());
            egui::ComboBox::from_id_salt(("barra", &f.key)).selected_text(current).show_ui(ui, |ui| {
                for o in options {
                    ui.selectable_value(v, o.value, &o.label);
                }
            });
            *v != before
        }
        FieldKind::Range { min, max } => ui.add(egui::DragValue::new(v).range(*min..=*max)).changed(),
        _ => crate::ui::options::field(ui, cat, f, v, style),
    }
}

/// Atajo de las herramientas de construcción que lo tienen (§10).
fn build_key(id: &str) -> Option<&'static str> {
    match id {
        "room" => Some("H"),
        "wall" => Some("W"),
        "floor" => Some("S"),
        "roof" => Some("T"),
        _ => None,
    }
}

/// Dirección (`dirs` = +X, −X, +Z, −Z) de una normal horizontal.
fn dir_of(n: V) -> Option<i32> {
    match n {
        [1, 0, 0] => Some(0),
        [-1, 0, 0] => Some(1),
        [0, 0, 1] => Some(2),
        [0, 0, -1] => Some(3),
        _ => None,
    }
}

fn floor_v(p: glam::Vec3) -> V {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// Eje del mundo más parecido a una dirección del suelo (flechas relativas a la cámara).
fn snap(x: f32, z: f32) -> V {
    if x.abs() >= z.abs() { [x.signum() as i32, 0, 0] } else { [0, 0, z.signum() as i32] }
}

fn role_label(role: &str) -> &str {
    match role {
        "wall" => "Muro",
        "timber" => "Entramado",
        "plinth" => "Zócalo",
        "quoin" => "Esquinas",
        "roof" => "Tejado",
        "ridge" => "Cumbrera",
        "verge" => "Alero",
        "frame" => "Marcos",
        "shutter" => "Contraventanas",
        "floor" => "Suelo",
        "leaf" => "Hojas de puerta",
        "trim" => "Molduras",
        "chimney" => "Chimenea",
        "dome" => "Cúpula",
        other => other,
    }
}

/// Valor inicial de un parámetro de construcción. Los muros empiezan en «Liso» fuera del estilo norteño (el entramado
/// es norteño).
fn tool_default(f: &crate::bridge::proto::Field, style: &str) -> i64 {
    if f.key == "walls" && style != "Northern" { 2 } else { f.default }
}

/// Aspecto por defecto: el primer barrio y la segunda clase (Modesta) del catálogo.
fn default_look(cat: &Catalog) -> Look {
    Look {
        district: cat.districts.first().map_or_else(|| "Residential".into(), |d| d.name.clone()),
        tier: cat.tiers.get(1).or(cat.tiers.first()).map_or_else(|| "Modest".into(), |t| t.name.clone()),
        roles: BTreeMap::new(),
    }
}

impl State {
    fn materials(&self) -> Vec<String> {
        self.catalog.as_ref().map(|c| c.materials.clone()).unwrap_or_default()
    }

    pub(super) fn editor_start(&mut self) {
        if let Some(cat) = self.catalog.clone() {
            self.editor.new_doc(&cat, 6, 6, 4);
            self.after_open();
            if let Some(path) = crate::editor::Editor::recovery_available() {
                self.toasts.push(ToastKind::Info, "Hay una copia de recuperación de la última sesión (Archivo › Abrir).", Some(path));
            }
        }
    }

    /// Tras abrir o crear: modo Editar, encuadrar, planta «Todo», fantasma encendido y Aspecto del documento.
    pub(super) fn after_open(&mut self) {
        self.mode = AppMode::Edit;
        self.cancel_build();
        self.ed.floor = None;
        self.ed.shown = None;
        self.edit_view.cut_level = None;
        self.edit_view.ghost_upper = true;
        self.plane.1 = false;
        if let Some(doc) = &self.editor.doc {
            self.edit_view.frame(&doc.s);
        }
        self.ed.look_asked.clear();
    }

    pub(super) fn ask_new(&mut self) {
        if self.editor.doc.as_ref().is_some_and(|d| d.unsaved) {
            self.confirm = Some(Confirm::New);
        } else {
            self.editor_start();
        }
    }

    pub(super) fn ask_open(&mut self) {
        let dir = source_dir(&self.host.paths.agame_root);
        let Some(path) = rfd::FileDialog::new().add_filter("Estructura a mano", &["agvox"]).set_directory(&dir).pick_file()
        else {
            return;
        };
        self.ask_open_path(path);
    }

    pub(super) fn ask_open_path(&mut self, path: PathBuf) {
        if self.editor.doc.as_ref().is_some_and(|d| d.unsaved && d.path.as_ref() != Some(&path)) {
            self.confirm = Some(Confirm::Open(path));
        } else {
            self.open(&path);
        }
    }

    fn remember_recent(&mut self, path: &std::path::Path) {
        let r = &mut self.settings.recent;
        r.retain(|p| p != path);
        r.insert(0, path.to_path_buf());
        r.truncate(10);
    }

    fn open(&mut self, path: &std::path::Path) {
        let materials = self.materials();
        let model = is_model_path(path);
        let opened = if model { self.editor.open_model(path) } else { self.editor.open(path, &materials) };
        match opened {
            Ok(()) => {
                if model {
                    // Las herramientas de edificio no valen para un modelo.
                    self.editor.tools.cancel();
                    self.editor.tools.tool = Tool::Pencil;
                    self.editor.palette.selected = 1;
                } else {
                    self.remember_recent(path);
                }
                self.after_open();
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido abrir: {e:#}"), None),
        }
    }

    pub(super) fn save(&mut self, as_new: bool) {
        self.save_checked(as_new, false);
    }

    /// Guarda un modelo en su fichero del juego. `overwrite`: aunque haya cambiado en disco.
    fn save_model(&mut self, as_new: bool, overwrite: bool) {
        let Some(doc) = &self.editor.doc else { return };
        let root = crate::editor::models_root(&self.host.paths.agame_root);
        let target = if as_new || doc.path.is_none() {
            let name = format!("{}_model.json", crate::editor::slug(&doc.meta.name));
            let dir = doc.path.as_ref().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_else(|| root.join("Objects/Voxel"));
            let Some(p) = rfd::FileDialog::new().add_filter("Modelo del juego", &["json"]).set_directory(&dir).set_file_name(name).save_file()
            else {
                return;
            };
            // El juego busca los modelos por su nombre: siempre `<nombre>_model.json`.
            let file = p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let stem = file.trim_end_matches(".json").trim_end_matches("_model");
            Some(p.with_file_name(format!("{stem}_model.json")))
        } else {
            None
        };
        if target.is_none() && !overwrite && self.editor.changed_on_disk() {
            self.confirm = Some(Confirm::Overwrite);
            return;
        }
        match self.editor.save(target, &[]) {
            Ok(p) => {
                if let Some(doc) = self.editor.doc.as_mut() {
                    let file = p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    doc.meta.name = file.trim_end_matches("_model.json").to_string();
                }
                self.ed.library.refresh(&self.host.paths.agame_root, &[], true);
                let shown = p.strip_prefix(&self.host.paths.agame_root).unwrap_or(&p).display().to_string();
                self.toasts.push(ToastKind::Ok, format!("Guardado en el juego: {shown}"), Some(p));
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido guardar: {e:#}"), None),
        }
    }

    fn save_checked(&mut self, as_new: bool, overwrite: bool) {
        if self.editor.doc.as_ref().is_some_and(|d| d.is_model()) {
            return self.save_model(as_new, overwrite);
        }
        let materials = self.materials();
        let Some(doc) = &self.editor.doc else { return };
        let target = if as_new || doc.path.is_none() {
            let dir = source_dir(&self.host.paths.agame_root);
            let name = format!("{}.agvox", crate::editor::slug(&doc.meta.name));
            match rfd::FileDialog::new().add_filter("Estructura a mano", &["agvox"]).set_directory(&dir).set_file_name(name).save_file() {
                Some(p) => Some(p),
                None => return,
            }
        } else {
            None
        };
        match self.editor.save(target, &materials) {
            Ok(p) => {
                self.remember_recent(&p);
                self.ed.library.refresh(&self.host.paths.agame_root, &materials, true);
                self.toasts.push(ToastKind::Ok, "Guardado", Some(p));
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido guardar: {e:#}"), None),
        }
    }

    fn export(&mut self) {
        let materials = self.materials();
        let errors = self.editor.errors();
        if errors > 0 {
            let n = widgets::count(errors, "error", "errores");
            self.toasts.push(ToastKind::Error, format!("Corrige {n} para exportar."), None);
            return;
        }
        match self.editor.export(&mut self.host, &materials) {
            Ok(p) => {
                self.remember_recent(&p);
                self.toasts.push(ToastKind::Info, "Exportando al juego…", Some(p));
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha exportado: {e:#}"), None),
        }
    }

    pub(super) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(what) = &self.confirm else { return };
        if matches!(what, Confirm::Overwrite) {
            let mut go = None;
            egui::Modal::new(egui::Id::new("confirmar-pisar")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new("Este modelo ha cambiado en disco").font(theme::semibold(18.0)).color(theme::TITLE));
                ui.label("Alguien lo ha cambiado desde que lo abriste (un script de arte, git…). Si guardas, se pierde ese cambio.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Guardar igualmente").color(theme::TEXT)).fill(theme::DANGER_BG)).clicked() {
                        go = Some(true);
                    }
                    if ui.button("Cancelar").clicked() {
                        go = Some(false);
                    }
                });
            });
            if let Some(go) = go {
                self.confirm = None;
                if go {
                    self.save_checked(false, true);
                }
            }
            return;
        }
        let delete = match what {
            Confirm::Delete(p) => Some(p.clone()),
            _ => None,
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("confirmar")).show(ctx, |ui| {
            ui.set_width(400.0);
            match &delete {
                Some(p) => {
                    let name = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                    ui.label(RichText::new(format!("¿Borrar «{name}»?")).font(theme::semibold(18.0)).color(theme::TITLE));
                    ui.label("Se borra el fichero. Si sustituía un diseño del juego, al exportar el juego volverá al original.");
                }
                None => {
                    ui.label(RichText::new("Hay cambios sin guardar").font(theme::semibold(18.0)).color(theme::TITLE));
                    ui.label("Si sigues, se pierden los cambios de la estructura que estás editando.");
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if delete.is_none() && widgets::primary(ui, "Guardar y seguir").clicked() {
                    choice = Some(true);
                }
                let danger = if delete.is_some() { "Borrar" } else { "Descartar" };
                if ui.add(egui::Button::new(RichText::new(danger).color(theme::TEXT)).fill(theme::DANGER_BG)).clicked() {
                    choice = Some(false);
                }
                if ui.button("Cancelar").clicked() {
                    self.confirm = None;
                }
            });
        });
        let Some(save_first) = choice else { return };
        if save_first {
            self.save(false);
            if self.editor.doc.as_ref().is_some_and(|d| d.unsaved) {
                return; // canceló el diálogo de guardar
            }
        }
        if delete.is_none()
            && let Some(doc) = self.editor.doc.as_mut()
        {
            doc.unsaved = false;
        }
        match self.confirm.take() {
            Some(Confirm::Fork) => self.fork_main(),
            Some(Confirm::New) => self.editor_start(),
            Some(Confirm::Open(p)) => self.open(&p),
            Some(Confirm::Delete(p)) => {
                match std::fs::remove_file(&p) {
                    Ok(()) => self.toasts.push(ToastKind::Ok, "Borrado.", None),
                    Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido borrar: {e}"), None),
                }
                let materials = self.materials();
                self.ed.library.refresh(&self.host.paths.agame_root, &materials, true);
            }
            Some(Confirm::Overwrite) | None => {}
        }
    }

    pub(super) fn edit_title(&mut self, ui: &mut Ui) {
        let Some(doc) = &self.editor.doc else { return };
        let dirty = if doc.unsaved { " ●" } else { "" };
        ui.label(RichText::new(format!("{}{dirty}", doc.meta.name)).color(theme::TEXT));
        if let (Some(r), Some(cat)) = (&doc.meta.replaces, &self.catalog) {
            let tip = format!("Sustituye en el juego a {}", library::describe(r, cat));
            widgets::chip(ui, icon::SWAP, "Diseño del juego", theme::CYAN).on_hover_text(tip);
        }
        let (can_undo, can_redo) = (doc.can_undo(), doc.can_redo());
        if ui.add_enabled(can_undo, egui::Button::new(icon::ARROW_COUNTER_CLOCKWISE)).on_hover_text("Deshacer (Cmd+Z)").clicked() {
            self.undo(false);
        }
        if ui.add_enabled(can_redo, egui::Button::new(icon::ARROW_CLOCKWISE)).on_hover_text("Rehacer (Shift+Cmd+Z)").clicked() {
            self.undo(true);
        }
    }

    pub(super) fn edit_actions(&mut self, ui: &mut Ui) {
        if self.editor.doc.is_none() {
            return;
        }
        let errors = self.editor.errors();
        let ready = errors == 0 && !self.editor.exporting && self.host.status == crate::host::Status::Ready;
        let tip = if errors > 0 {
            format!("Corrige {} para exportar (pestaña Validación)", widgets::count(errors, "error", "errores"))
        } else {
            "Guarda la estructura y la añade al juego (Cmd+E)".into()
        };
        let export = ui.add_enabled_ui(ready, |ui| widgets::primary(ui, &format!("{}  EXPORTAR AL JUEGO", icon::EXPORT))).inner;
        if export.on_hover_text(&tip).on_disabled_hover_text(&tip).clicked() {
            self.export();
        }
        if ui.button(format!("{}  Guardar", icon::FLOPPY_DISK)).on_hover_text("Cmd+S").clicked() {
            self.save(false);
        }
    }

    fn undo(&mut self, redo: bool) {
        self.cancel_build();
        let Some(doc) = self.editor.doc.as_mut() else { return };
        let done = if redo { doc.redo() } else { doc.undo() };
        if let Some(label) = done {
            let verb = if redo { "Rehecho" } else { "Deshecho" };
            self.toasts.push(ToastKind::Info, format!("{verb}: {label}"), None);
        }
    }

    /// Planta de trabajo: corta la vista, fija el plano en su suelo y es la de las herramientas de construcción.
    pub(super) fn set_floor(&mut self, floor: Option<usize>) {
        self.ed.floor = floor;
        let Some(doc) = &self.editor.doc else { return };
        match floor.and_then(|i| doc.s.band_floors.get(i)) {
            Some(&f) => {
                self.edit_view.cut_level = Some(f);
                self.plane = (f * SUB, true);
            }
            None => {
                self.edit_view.cut_level = None;
                self.plane.1 = false;
            }
        }
    }

    /// Altura donde se pisa para las herramientas de construcción: la de la planta de trabajo o la de la planta bajo
    /// el sub-cubo señalado.
    fn floor_y(&self, at_y: Option<i32>) -> i32 {
        let Some(doc) = &self.editor.doc else { return SUB };
        let s = &doc.s;
        if let Some(f) = self.ed.floor.and_then(|i| s.band_floors.get(i)) {
            return f * SUB;
        }
        let k = at_y.map_or(0, |y| s.band_starts.iter().rposition(|&b| b <= y).unwrap_or(0));
        s.band_floors.get(k).map_or(s.ground_level * SUB, |f| f * SUB)
    }

    /// «+»: una planta más encima (crece la rejilla si hace falta), en un paso de deshacer.
    fn add_floor(&mut self) {
        let Some(doc) = self.editor.doc.as_mut() else { return };
        let mut s = doc.s.clone();
        let next = s.band_floors.last().map_or(s.ground_level, |f| f + 2);
        let need = (next + 2) * SUB;
        if need > s.grid.size[1] {
            // El índice crece hacia arriba (y es el eje lento): basta copiar por delante.
            let mut g = VoxelGrid::new([s.grid.size[0], need, s.grid.size[2]]);
            let n = s.grid.data.len();
            g.data[..n].copy_from_slice(&s.grid.data);
            if let Some(d) = &s.grid.detail {
                let mut nd = vec![0; g.data.len()];
                nd[..n].copy_from_slice(d);
                g.detail = Some(nd);
            }
            s.grid = g;
        }
        s.band_starts.push(next * SUB);
        s.band_floors.push(next);
        doc.apply_external("Planta nueva", &s);
        let i = doc.s.band_floors.len() - 1;
        self.set_floor(Some(i));
    }

    pub(super) fn cancel_build(&mut self) {
        self.ed.gesture = None;
        self.ed.pressed = false;
        self.ed.preview = None;
        self.ed.preview_error = None;
        self.ed.commit_on = None;
        self.ed.sent_sig.clear();
    }

    /// Papeles del Aspecto: se piden al generador cuando cambia el estilo, el barrio o la clase.
    fn ensure_look(&mut self) {
        let (Some(doc), Some(cat)) = (&self.editor.doc, &self.catalog) else { return };
        let look = doc.meta.look.clone().unwrap_or_else(|| default_look(cat));
        let key = format!("{}|{}|{}", doc.meta.style, look.district, look.tier);
        if key != self.ed.look_asked && self.host.status == crate::host::Status::Ready {
            self.ed.look_asked = key;
            self.host.look(&doc.meta.style, &look.district, &look.tier);
        }
    }

    /// Papeles finales: los del generador con los cambiados a mano encima.
    fn roles(&self) -> BTreeMap<String, String> {
        let mut r = self.ed.roles.clone();
        if let Some(look) = self.editor.doc.as_ref().and_then(|d| d.meta.look.as_ref()) {
            r.extend(look.roles.clone());
        }
        r
    }

    pub(super) fn on_look(&mut self, roles: BTreeMap<String, String>) {
        self.ed.roles = roles;
    }

    /// Respuesta de una herramienta de construcción: vista previa, y confirmación si ya se soltó.
    pub(super) fn on_tool(&mut self, seq: u64, result: Result<Arc<Structure>, ErrorInfo>) {
        self.ed.received = seq;
        match result {
            Ok(s) => {
                self.ed.preview = Some(s.clone());
                self.ed.preview_error = None;
                if let Some((wait, label)) = self.ed.commit_on.clone()
                    && wait == seq
                    && let Some(doc) = self.editor.doc.as_mut()
                {
                    doc.apply_external(&label, &s);
                    self.cancel_build();
                }
            }
            Err(e) => {
                self.ed.preview = None;
                self.ed.preview_error = Some(e.message.clone());
                if self.ed.commit_on.take().is_some() {
                    self.toasts.push(ToastKind::Error, e.message, None);
                    self.cancel_build();
                }
            }
        }
    }

    /// Respuesta de `stamp`: miniatura de la Biblioteca (`mini:<id>`) o pieza a la mano (`mano:<id>:<facing>`).
    pub(super) fn on_stamp(&mut self, ctx: &egui::Context, key: String, result: Result<Stamp, ErrorInfo>) {
        let st = match result {
            Ok(s) => s,
            Err(e) => {
                if key.starts_with("mano:") {
                    self.toasts.push(ToastKind::Error, format!("No se ha podido coger la pieza: {}", e.message), None);
                }
                return;
            }
        };
        if let Some(id) = key.strip_prefix("mini:") {
            let s = Structure { grid: st.grid, interactables: st.interactables, ..Default::default() };
            if let Some(t) = self.picture(ctx, &key, &s, [140, 116]) {
                self.ed.library.thumbs.insert(format!("mueble:{id}"), t);
            }
            return;
        }
        if let Some(rest) = key.strip_prefix("mano:") {
            let (id, facing) = rest.rsplit_once(':').map_or((rest, 3), |(i, f)| (i, f.parse().unwrap_or(3)));
            let name = self.ed.furniture.as_ref().and_then(|f| f.pieces.iter().find(|p| p.id == id)).map_or(id.to_string(), |p| p.label.clone());
            let len = st.grid.data.len();
            let clip = Clip { size: st.grid.size, data: st.grid.data, detail: st.grid.detail.unwrap_or_else(|| vec![0; len]) };
            self.editor.tools.hand =
                Some(Hand { name, clip, interactables: st.interactables, anchor: st.anchor, furniture: Some(id.to_string()), facing });
            self.editor.tools.tool = Tool::Place;
            self.cancel_build();
        }
    }

    /// Coge un mueble del juego (lo pide al generador ya girado).
    fn take_piece(&mut self, id: &str, facing: i32) {
        self.host.stamp(format!("mano:{id}:{facing}"), id, facing, None, 0);
    }

    fn take_stamp(&mut self, path: &std::path::Path) {
        let materials = self.materials();
        match std::fs::read_to_string(path).map_err(anyhow::Error::from).and_then(|t| agvox::load(&t, &materials)) {
            Ok((meta, s)) => {
                let [sx, sy, sz] = s.grid.size;
                let clip = Clip::copy(&s.grid, [0, 0, 0], [sx - 1, sy - 1, sz - 1]);
                self.editor.tools.hand =
                    Some(Hand { name: meta.name, clip, interactables: s.interactables, anchor: [0, 0, 0], furniture: None, facing: 3 });
                self.editor.tools.tool = Tool::Place;
                self.cancel_build();
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido leer el sello: {e:#}"), None),
        }
    }

    fn rotate_hand(&mut self, back: bool) {
        let Some(h) = self.editor.tools.hand.as_mut() else { return };
        let turns = if back { 3 } else { 1 };
        match h.furniture.clone() {
            Some(id) => {
                let f = (0..turns).fold(h.facing, |d, _| ops::rotate_dir(d));
                self.take_piece(&id, f);
            }
            None => (0..turns).for_each(|_| h.rotate()),
        }
    }

    /// Acciones de la Biblioteca.
    fn library_action(&mut self, act: Action) {
        let materials = self.materials();
        match act {
            Action::New => self.ask_new(),
            Action::Open(p) => self.ask_open_path(p),
            Action::Duplicate(p) => match std::fs::read_to_string(&p).map_err(anyhow::Error::from).and_then(|t| agvox::load(&t, &materials)) {
                Ok((meta, s)) => {
                    self.editor.fork(&s, format!("{} (copia)", meta.name), meta.style.clone());
                    if let Some(doc) = self.editor.doc.as_mut() {
                        doc.meta.look = meta.look;
                        doc.unsaved = true;
                    }
                    self.after_open();
                }
                Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido duplicar: {e:#}"), None),
            },
            Action::Delete(p) => self.confirm = Some(Confirm::Delete(p)),
            Action::ExportVox(p) => self.export_vox(Some(p)),
            Action::ImportVox => self.import_vox(),
            Action::CopyGame => self.load_game(false),
            Action::EditGame => self.load_game(true),
            Action::Piece(id) => self.take_piece(&id, 3),
            Action::Stamp(p) => self.take_stamp(&p),
            Action::OpenModel(p) => self.ask_open_path(p),
        }
    }

    /// «Del juego»: copia como estructura nueva, o empieza a sustituir ese diseño del juego.
    fn load_game(&mut self, replace: bool) {
        let Some(cat) = self.catalog.clone() else { return };
        let (Some(s), Some(p)) = (self.ed.library.game_s.clone(), self.ed.library.game.clone()) else { return };
        if self.editor.doc.as_ref().is_some_and(|d| d.unsaved) {
            self.toasts.push(ToastKind::Error, "Guarda antes la estructura abierta.", None);
            return;
        }
        let label = cat.kind(&p.kind).map_or(p.kind.clone(), |k| k.label.clone());
        let style = cat.styles.iter().find(|s| s.name == p.style).map_or(String::new(), |s| s.label.to_lowercase());
        let design = p.fields.get("design").map(|d| format!(", diseño {}", d + 1)).unwrap_or_default();
        self.editor.fork(&s, format!("{label} {style}{design}").trim().to_string(), p.style.clone());
        if replace && let (Some(key), Some(doc)) = (self.ed.library.game_key(&cat), self.editor.doc.as_mut()) {
            let tag = format!("{key:?}");
            self.ed.warn = self.ed.warned.insert(tag).then(|| library::describe(&key, &cat));
            doc.meta.replaces = Some(key);
            self.ed.source_scope = Some((
                p.fields.get("rank").copied(),
                p.fields.get("district").and_then(|v| cat.districts.get(*v as usize)).map(|d| d.name.clone()),
                p.fields.get("tier").and_then(|v| cat.tiers.get(*v as usize)).map(|t| t.name.clone()),
            ));
        }
        self.after_open();
    }

    fn import_vox(&mut self) {
        let Some(cat) = self.catalog.clone() else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("MagicaVoxel", &["vox"]).pick_file() else { return };
        match std::fs::read(&path).map_err(anyhow::Error::from).and_then(|b| crate::vox::read(&b)) {
            Ok(file) => {
                let key = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let remembered: Option<BTreeMap<u8, Option<u8>>> = self.settings.vox_maps.get(&key).map(|m| {
                    m.iter().map(|(k, v)| (*k, v.as_ref().and_then(|n| cat.materials.iter().position(|x| x == n)).map(|i| i as u8))).collect()
                });
                self.ed.vox = Some(VoxDialog::new(path, file, &cat, remembered.as_ref()));
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("{e:#}"), None),
        }
    }

    pub(super) fn export_vox(&mut self, from: Option<PathBuf>) {
        let Some(cat) = self.catalog.clone() else { return };
        let materials = self.materials();
        let (name, s) = match from {
            Some(p) => match std::fs::read_to_string(&p).map_err(anyhow::Error::from).and_then(|t| agvox::load(&t, &materials)) {
                Ok((m, s)) => (m.name, s),
                Err(e) => return self.toasts.push(ToastKind::Error, format!("{e:#}"), None),
            },
            None => match &self.editor.doc {
                Some(d) => (d.meta.name.clone(), d.s.clone()),
                None => return,
            },
        };
        let hidden = cat.materials.iter().position(|m| m == "Hidden").map(|i| i as u8);
        let bytes = match crate::vox::write(&s, &cat.palette, hidden) {
            Ok(b) => b,
            Err(e) => return self.toasts.push(ToastKind::Error, format!("{e:#}"), None),
        };
        let file = format!("{}.vox", crate::editor::slug(&name));
        if let Some(out) = rfd::FileDialog::new().add_filter("MagicaVoxel", &["vox"]).set_file_name(file).save_file() {
            match std::fs::write(&out, bytes) {
                Ok(()) => self.toasts.push(ToastKind::Ok, "Exportado a MagicaVoxel.", Some(out)),
                Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido escribir: {e}"), None),
            }
        }
    }

    /// Ventanas del modo Editar: importar .vox, aviso de diseño del juego y nombre del sello.
    pub(super) fn edit_dialogs(&mut self, ctx: &egui::Context) {
        if let (Some(mut dlg), Some(cat)) = (self.ed.vox.take(), self.catalog.clone()) {
            let rs = self.viewports.as_ref().map(|v| v.rs.clone());
            let mut render = |s: &Structure| {
                let rs = rs.as_ref()?;
                let view = crate::render::ViewState { layers: crate::render::Layers::none(), ..Default::default() };
                let img = crate::render::render_image(rs, s, &Paint::new(&cat, None, None), &view, [520, 400]);
                Some(ctx.load_texture("vox-previa", img, egui::TextureOptions::LINEAR))
            };
            match dlg.ui(ctx, &cat, &mut render) {
                Some(VoxOutcome::Import { name, s, map }) => {
                    let key = dlg.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let named = map.iter().map(|(k, v)| (*k, v.and_then(|m| cat.materials.get(m as usize).cloned()))).collect();
                    self.settings.vox_maps.insert(key, named);
                    let style = cat.styles.first().map_or_else(String::new, |s| s.name.clone());
                    self.editor.fork(&s, name, style);
                    if let Some(doc) = self.editor.doc.as_mut() {
                        doc.unsaved = true;
                    }
                    self.after_open();
                }
                Some(VoxOutcome::Cancel) => {}
                None => self.ed.vox = Some(dlg),
            }
        }
        if let Some(text) = self.ed.warn.clone() {
            egui::Modal::new(egui::Id::new("aviso-diseno")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(RichText::new("Editar un diseño del juego").font(theme::semibold(18.0)).color(theme::TITLE));
                ui.label(format!(
                    "Vas a retocar «{text}». Al exportarlo, el juego usará tu versión en todas las ciudades donde salga \
                     (puedes acotarla a un barrio, una clase o una altura en Propiedades)."
                ));
                ui.label(
                    "No podrás cambiar el tamaño ni el sitio de la puerta; las medianeras deben quedar sin huecos; tiendas y \
                     bancos necesitan mostrador.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::primary(ui, "Editar el diseño").clicked() {
                        self.ed.warn = None;
                    }
                    if ui.button("Hacer solo una copia").on_hover_text("Te quedas con una estructura propia; el juego no cambia").clicked() {
                        self.ed.warn = None;
                        if let Some(doc) = self.editor.doc.as_mut() {
                            doc.meta.replaces = None;
                        }
                    }
                });
            });
        }
        if let Some(mut name) = self.ed.stamp_name.take() {
            let mut keep = true;
            egui::Modal::new(egui::Id::new("nombre-sello")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new("Guardar como sello").font(theme::semibold(18.0)).color(theme::TITLE));
                ui.text_edit_singleline(&mut name).request_focus();
                ui.horizontal(|ui| {
                    if widgets::primary(ui, "Guardar").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.save_stamp(&name);
                        keep = false;
                    }
                    if ui.button("Cancelar").clicked() {
                        keep = false;
                    }
                });
            });
            if keep {
                self.ed.stamp_name = Some(name);
            }
        }
    }

    fn save_stamp(&mut self, name: &str) {
        let materials = self.materials();
        let Some(doc) = &self.editor.doc else { return };
        let Some(hand) = self.editor.tools.selection_as_hand(doc, name) else { return };
        let mut grid = VoxelGrid::new(hand.clip.size);
        grid.data = hand.clip.data.clone();
        grid.detail = hand.clip.detail.iter().any(|&d| d != 0).then(|| hand.clip.detail.clone());
        let s = Structure { grid, interactables: hand.interactables, ..Default::default() };
        let meta = agvox::Meta { name: name.to_string(), ..Default::default() };
        let path = library::stamps_dir().join(format!("{}.agvox", crate::editor::slug(name)));
        let result = agvox::save(&meta, &s, &materials).and_then(|t| {
            std::fs::create_dir_all(library::stamps_dir())?;
            std::fs::write(&path, t)?;
            Ok(())
        });
        match result {
            Ok(()) => {
                self.ed.library.refresh(&self.host.paths.agame_root, &materials, true);
                self.toasts.push(ToastKind::Ok, format!("Sello «{name}» guardado (Biblioteca › Piezas)."), None);
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido guardar el sello: {e:#}"), None),
        }
    }

    pub(super) fn edit_shortcuts(&mut self, ctx: &egui::Context) {
        if self.editor.doc.is_none() {
            return;
        }
        let keys = ctx.input(|i| i.clone());
        let m = keys.modifiers;
        let plain = m.is_none();
        let shift_only = m.shift && !m.command && !m.alt;
        let pressed = |k: Key| keys.key_pressed(k);
        // Herramientas básicas, modos y construcción con tecla.
        for t in Tool::BASIC {
            if plain && Key::from_name(t.key()).is_some_and(pressed) {
                self.editor.tools.tool = t;
                self.cancel_build();
            }
        }
        for (k, mode) in [(Key::A, Mode::Add), (Key::N, Mode::Paint), (Key::E, Mode::Erase)] {
            if plain && pressed(k) {
                self.editor.tools.set_mode(mode);
            }
        }
        let keyed: Vec<String> =
            self.ed.tools_cat.iter().filter(|t| plain && build_key(&t.id).and_then(Key::from_name).is_some_and(pressed)).map(|t| t.id.clone()).collect();
        if let Some(id) = keyed.into_iter().next() {
            self.editor.tools.tool = Tool::Build;
            self.editor.tools.build = id;
            self.cancel_build();
        }
        if plain && pressed(Key::Comma) {
            self.editor.tools.brush_down();
        }
        if plain && pressed(Key::Period) {
            self.editor.tools.brush_up();
        }
        if (plain || shift_only) && pressed(Key::O) {
            self.rotate_hand(shift_only);
        }
        // K: plano en la cara señalada (otra vez, lo suelta).
        if plain && pressed(Key::K) {
            if self.plane.1 {
                self.plane.1 = false;
            } else if let Some((v, n)) = self.edit_pointer.hit {
                self.plane = (if n[1] > 0 { v[1] + 1 } else { v[1] }, true);
            }
        }
        // Esc: cancela el arrastre, luego suelta la pieza, luego quita la selección.
        if plain && pressed(Key::Escape) {
            if self.ed.gesture.is_some() || self.ed.preview.is_some() {
                self.cancel_build();
                self.editor.tools.cancel();
            } else if self.editor.tools.hand.take().is_some() {
                self.editor.tools.tool = Tool::Pencil;
            } else {
                self.editor.tools.selection = None;
            }
        }
        if m.command && pressed(Key::E) {
            self.export();
        }
        let (yaw, place_hint) = (self.edit_view.yaw_deg, self.edit_place);
        let Some(doc) = self.editor.doc.as_mut() else { return };
        let tools = &mut self.editor.tools;
        // Flechas: con selección la mueven (según la cámara); sin ella, el plano sube o baja.
        if tools.selection.is_some() && matches!(tools.tool, Tool::Select | Tool::Move) {
            let (sy, cy) = yaw.to_radians().sin_cos();
            let step = if m.shift { SUB } else { 1 };
            let (fwd, right) = (snap(sy, cy), snap(cy, -sy));
            let mut d = [0, 0, 0];
            if m.alt {
                if pressed(Key::ArrowUp) {
                    d[1] = step;
                }
                if pressed(Key::ArrowDown) {
                    d[1] = -step;
                }
            } else {
                for (k, v, s) in [(Key::ArrowUp, fwd, 1), (Key::ArrowDown, fwd, -1), (Key::ArrowRight, right, 1), (Key::ArrowLeft, right, -1)] {
                    if pressed(k) {
                        d = [d[0] + v[0] * s * step, 0, d[2] + v[2] * s * step];
                    }
                }
            }
            if d != [0, 0, 0] {
                tools.nudge(doc, d);
            }
        } else if plain && (pressed(Key::ArrowUp) || pressed(Key::ArrowDown)) {
            let step = if pressed(Key::ArrowUp) { 1 } else { -1 };
            self.plane = ((self.plane.0 + step).clamp(0, doc.s.grid.size[1] - 1), true);
        }
        if plain && (pressed(Key::Delete) || pressed(Key::Backspace)) {
            tools.clear_selection(doc);
        }
        let place = place_hint.or(tools.selection.map(|s| s.0)).unwrap_or([0, 0, 0]);
        for e in &keys.events {
            match e {
                egui::Event::Copy => {
                    tools.copy(doc);
                }
                egui::Event::Cut => {
                    tools.cut(doc);
                }
                egui::Event::Paste(_) => {
                    tools.paste(doc, place);
                }
                _ => {}
            }
        }
        if m.command && !m.shift && pressed(Key::Z) {
            doc.undo();
        }
        if m.command && m.shift && pressed(Key::Z) {
            doc.redo();
        }
        if m.command && pressed(Key::D) && tools.copy(doc) {
            let at = tools.selection.map_or(place, |s| [s.0[0] + SUB, s.0[1], s.0[2]]);
            tools.paste(doc, at);
        }
        if shift_only && pressed(Key::X) {
            tools.transform(doc, "Espejo este-oeste", |c| c.mirror_x());
        }
        if shift_only && pressed(Key::Z) {
            tools.transform(doc, "Espejo norte-sur", |c| c.mirror_z());
        }
        if m.command && pressed(Key::R) {
            let turns = if m.shift { 3 } else { 1 };
            tools.transform(doc, "Girar", |c| (0..turns).fold(c.clone(), |c, _| c.rotate_quarter()));
        }
    }

    /// Gesto de construcción: manda la vista previa cuando cambia y confirma al soltar (o al hacer clic en una cara).
    fn build_step(&mut self, r: &ViewportResponse, pressed: bool, down: bool, released: bool) {
        let Some(info) = self.ed.tools_cat.iter().find(|t| t.id == self.editor.tools.build).cloned() else { return };
        let hit = r.hovered;
        let cell_of = |v: V| [v[0].div_euclid(SUB), v[2].div_euclid(SUB)];
        let hover_cell = hit.map(|h| cell_of(h.voxel)).or(self.edit_place.map(cell_of));
        let y = hit.map_or(0, |h| h.voxel[1]);
        let target = if info.target == "face" {
            hit.and_then(|h| dir_of(h.normal).map(|d| Gesture { anchor: cell_of(h.voxel), current: cell_of(h.voxel), dir: d, y: h.voxel[1] }))
        } else {
            if pressed && let Some(c) = hover_cell {
                self.ed.gesture = Some(Gesture { anchor: c, current: c, dir: 3, y });
                self.ed.pressed = true;
            }
            if self.ed.pressed
                && down
                && let (Some(g), Some(c)) = (self.ed.gesture.as_mut(), hover_cell)
            {
                g.current = c;
                // Escaleras y demás «cell»: la dirección es hacia donde se arrastra.
                let (dx, dz) = (c[0] - g.anchor[0], c[1] - g.anchor[1]);
                if dx != 0 || dz != 0 {
                    g.dir = if dx.abs() >= dz.abs() {
                        if dx > 0 { 0 } else { 1 }
                    } else if dz > 0 {
                        2
                    } else {
                        3
                    };
                }
            }
            match self.ed.gesture {
                Some(g) if self.ed.pressed => Some(g),
                // Sin arrastrar: vista previa en la celda señalada.
                _ => hover_cell.map(|c| Gesture { anchor: c, current: c, dir: 3, y }),
            }
        };
        let Some(g) = target else {
            if !self.ed.pressed {
                self.ed.preview = None;
            }
            return;
        };
        let floor_y = self.floor_y(Some(g.y));
        let (a, b) = (g.anchor, g.current);
        let target_json = match info.target.as_str() {
            "rect" | "line" => json!({ "rect": [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])] }),
            _ => json!({ "cell": a, "dir": g.dir, "y": g.y }),
        };
        let style = self.editor.doc.as_ref().map_or(String::new(), |d| d.meta.style.clone());
        let params: BTreeMap<String, i64> = self
            .ed
            .params
            .entry(info.id.clone())
            .or_insert_with(|| info.params.iter().map(|f| (f.key.clone(), tool_default(f, &style))).collect())
            .clone();
        let roles = self.roles();
        let Some(doc) = &self.editor.doc else { return };
        let sig = format!("{}{params:?}{roles:?}{target_json}{floor_y}{}", info.id, doc.version);
        if sig != self.ed.sent_sig {
            self.ed.sent_sig = sig;
            self.ed.seq += 1;
            let extra = json!({ "name": info.id, "params": params, "roles": roles, "target": target_json, "floorY": floor_y });
            self.host.tool(self.ed.seq, &doc.s, doc.meta.replaces.as_ref(), extra);
        }
        let commit = if info.target == "face" { pressed } else { released && self.ed.pressed };
        if !commit {
            return;
        }
        self.ed.pressed = false;
        let label = if info.target == "rect" {
            format!("{} {}×{} celdas", info.label, (a[0] - b[0]).abs() + 1, (a[1] - b[1]).abs() + 1)
        } else {
            info.label.clone()
        };
        if self.ed.received == self.ed.seq {
            match (self.ed.preview.clone(), self.ed.preview_error.clone()) {
                (Some(p), _) => {
                    if let Some(doc) = self.editor.doc.as_mut() {
                        doc.apply_external(&label, &p);
                    }
                    self.cancel_build();
                }
                (None, Some(e)) => {
                    self.toasts.push(ToastKind::Error, e, None);
                    self.cancel_build();
                }
                _ => self.ed.commit_on = Some((self.ed.seq, label)),
            }
        } else {
            // La vista previa de este gesto aún no ha llegado: se confirma cuando llegue.
            self.ed.commit_on = Some((self.ed.seq, label));
        }
    }

    /// Solo para `EDITOR_CAPTURA`: deja Editar en un estado concreto para revisarlo sin ratón.
    pub(super) fn capture_setup(&mut self, lib: Option<String>, tool: Option<String>, vox: Option<PathBuf>) {
        if let Ok(q) = std::env::var("EDITOR_BUSCAR") {
            self.ed.library.search = q;
        }
        if let Some(section) = lib {
            self.ed.library.section = match section.as_str() {
                "juego" => library::Section::Game,
                "piezas" => library::Section::Pieces,
                "modelos" => library::Section::Models,
                _ => library::Section::Structures,
            };
            self.open_tab = Some(Tab::Library);
        }
        if let Some(path) = std::env::var_os("EDITOR_MODELO") {
            self.open(std::path::Path::new(&path));
        }
        if let Some(id) = tool {
            self.editor.tools.tool = Tool::Build;
            self.editor.tools.build = id;
        }
        if let (Some(path), Some(cat)) = (vox, self.catalog.clone())
            && let Ok(file) = std::fs::read(&path).map_err(anyhow::Error::from).and_then(|b| crate::vox::read(&b))
        {
            self.ed.vox = Some(VoxDialog::new(path, file, &cat, None));
        }
    }

    /// «Abrir en Editar» del aviso de Explorar: el `.agvox` que sustituye a lo que se ve.
    pub(super) fn open_override_of_main(&mut self) {
        let materials = self.materials();
        self.ed.library.refresh(&self.host.paths.agame_root, &materials, true);
        let p = &self.params;
        let design = library::design_field(&p.fields).map_or(0, |(_, v)| v);
        let found = self
            .ed
            .library
            .entries
            .iter()
            .find(|e| {
                e.meta.replaces.as_ref().is_some_and(|r| r.kind == p.kind && r.style == p.style && r.design == design && r.size == [p.width, p.depth])
            })
            .map(|e| e.path.clone());
        match found {
            Some(path) => self.ask_open_path(path),
            None => self.toasts.push(ToastKind::Error, "No encuentro su fichero en ArtSource/Structures.", None),
        }
    }

    /// «Ver el original»: el diseño del juego sin la sustitución, en Explorar.
    pub(super) fn show_original(&mut self, r: &Replaces) {
        let Some(cat) = self.catalog.clone() else { return };
        let Some(kind) = cat.kind(&r.kind) else { return };
        let mut p = crate::params::Params::defaults(kind, &cat);
        p.style = r.style.clone();
        if let Some((key, _)) = library::design_field(&p.fields) {
            p.fields.insert(key.to_string(), r.design);
        }
        p.width = r.size[0];
        p.depth = r.size[1];
        p.fit(kind, &cat);
        self.settings.params.insert(self.params.kind.clone(), self.params.clone());
        self.params = p;
        self.settings.kind = r.kind.clone();
        self.ed.explore_original = true;
        self.mode = AppMode::Explore;
        self.rebuild_main();
    }
}

/// Botón de la tira (30×28) con canto tallado; dorado si está activo.
fn strip_button(ui: &mut Ui, glyph: &str, on: bool, tip: &str) -> bool {
    let text = RichText::new(glyph).size(17.0).color(if on { theme::TITLE } else { theme::TEXT });
    let b = egui::Button::new(text)
        .fill(if on { theme::HOLE } else { theme::STONE1 })
        .stroke(egui::Stroke::new(1.0, if on { theme::GOLD } else { theme::STONE0 }))
        .min_size(vec2(30.0, 28.0));
    let r = ui.add(b);
    if on {
        ui.painter().rect_stroke(r.rect, 2.0, egui::Stroke::new(1.5, theme::GOLD), egui::StrokeKind::Inside);
    } else {
        theme::bevel(ui.painter(), r.rect, true);
    }
    r.on_hover_text(tip).clicked()
}

/// Tira de herramientas vertical (§10): Básicas, Construir y Colocar.
fn tool_strip(ui: &mut Ui, st: &mut State) {
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    let title = |ui: &mut Ui, t: &str| {
        ui.add_space(4.0);
        ui.label(RichText::new(t).size(11.0).color(theme::TEXT_DIM));
    };
    title(ui, "BÁSICAS");
    egui::Grid::new("tira-basicas").spacing([4.0, 4.0]).min_col_width(30.0).show(ui, |ui| {
        for (i, t) in Tool::BASIC.into_iter().enumerate() {
            let on = st.editor.tools.tool == t;
            if strip_button(ui, t.icon(), on, &format!("{} ({})\n{}", t.label(), t.key(), t.hint())) {
                st.editor.tools.tool = t;
                st.cancel_build();
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    if !st.ed.tools_cat.is_empty() {
        title(ui, "CONSTRUIR");
        let cat = st.ed.tools_cat.clone();
        egui::Grid::new("tira-construir").spacing([4.0, 4.0]).min_col_width(30.0).show(ui, |ui| {
            for (i, t) in cat.iter().enumerate() {
                let on = st.editor.tools.tool == Tool::Build && st.editor.tools.build == t.id;
                let key = build_key(&t.id).map(|k| format!(" ({k})")).unwrap_or_default();
                if strip_button(ui, glyph(&t.icon), on, &format!("{}{key}\n{}", t.label, build_hint(&t.target))) {
                    st.editor.tools.tool = Tool::Build;
                    st.editor.tools.build = t.id.clone();
                    st.cancel_build();
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    }
    title(ui, "COLOCAR");
    ui.horizontal(|ui| {
        let on = st.editor.tools.tool == Tool::Place;
        let furniture = st.editor.tools.hand.as_ref().is_some_and(|h| h.furniture.is_some());
        if strip_button(ui, icon::COUCH, on && furniture, "Mueble: elige uno en Biblioteca › Piezas") {
            st.ed.library.section = library::Section::Pieces;
            st.open_tab = Some(Tab::Library);
        }
        if strip_button(ui, icon::STAMP, on && !furniture, "Sello: elige uno en Biblioteca › Piezas (o guarda una selección)") {
            st.ed.library.section = library::Section::Pieces;
            st.open_tab = Some(Tab::Library);
        }
    });
}

/// Barra contextual (§10): planta de trabajo y los parámetros de la herramienta activa.
fn context_bar(ui: &mut Ui, st: &mut State, cat: &Catalog) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Planta").on_hover_text("Planta de trabajo: corta la vista aquí y construyes sobre su suelo (Shift+↑/↓).");
        let floors = st.editor.doc.as_ref().map_or(0, |d| d.s.band_floors.len());
        let mut f = st.ed.floor;
        let opts: Vec<(Option<usize>, String)> = std::iter::once((None, "Todo".to_string()))
            .chain((0..floors).map(|i| (Some(i), if i == 0 { "Baja".to_string() } else { i.to_string() })))
            .collect();
        if widgets::segmented(ui, &mut f, &opts) {
            st.set_floor(f);
        }
        if ui.button(icon::PLUS).on_hover_text("Añadir una planta encima").clicked() {
            st.add_floor();
        }
        ui.separator();
        match st.editor.tools.tool {
            Tool::Build => {
                let doc = st.editor.doc.as_ref();
                let look = doc.and_then(|d| d.meta.look.clone()).unwrap_or_else(|| default_look(cat));
                let style = doc.and_then(|d| cat.styles.iter().find(|s| s.name == d.meta.style)).map_or("—", |s| s.label.as_str());
                let district = cat.districts.iter().find(|d| d.name == look.district).map_or("—", |d| d.label.as_str());
                let tier = cat.tiers.iter().find(|t| t.name == look.tier).map_or("—", |t| t.label.as_str());
                if widgets::chip(ui, icon::PALETTE, &format!("{style} · {district} · {tier}"), theme::TEXT)
                    .on_hover_text("Aspecto: los materiales de las herramientas de construcción (en Propiedades)")
                    .clicked()
                {
                    st.open_tab = Some(Tab::Properties);
                }
                if let Some(info) = st.ed.tools_cat.iter().find(|t| t.id == st.editor.tools.build).cloned() {
                    let style = st.editor.doc.as_ref().map_or(String::new(), |d| d.meta.style.clone());
                    let params = st
                        .ed
                        .params
                        .entry(info.id.clone())
                        .or_insert_with(|| info.params.iter().map(|f| (f.key.clone(), tool_default(f, &style))).collect());
                    for f in &info.params {
                        if let Some(v) = params.get_mut(&f.key) {
                            if !matches!(f.kind, crate::bridge::proto::FieldKind::Bool | crate::bridge::proto::FieldKind::Index { .. }) {
                                ui.label(&f.label);
                            }
                            compact_field(ui, cat, f, v, &style);
                        }
                    }
                }
            }
            Tool::Place => {
                if let Some(h) = &st.editor.tools.hand {
                    let [x, y, z] = h.clip.size;
                    ui.label(RichText::new(format!("{} · {}×{}×{} sub-cubos", h.name, x, z, y)).color(theme::TEXT));
                }
                if ui.button(icon::ARROW_COUNTER_CLOCKWISE).on_hover_text("Girar al revés (Shift+O)").clicked() {
                    st.rotate_hand(true);
                }
                if ui.button(icon::ARROW_CLOCKWISE).on_hover_text("Girar (O)").clicked() {
                    st.rotate_hand(false);
                }
                widgets::toggle(ui, &mut st.editor.tools.detail, &format!("{} Detalle", icon::ARMCHAIR))
                    .on_hover_text("Muebles y escaleras: el corte por planta no los quita");
            }
            t => {
                let tools = &mut st.editor.tools;
                if t.uses_mode() {
                    let mut mode = tools.mode;
                    let opts = [Mode::Add, Mode::Paint, Mode::Erase].map(|m| (m, m.label().to_string()));
                    if widgets::segmented(ui, &mut mode, &opts) {
                        tools.mode = mode;
                    }
                }
                if t == Tool::Pencil {
                    ui.label("Pincel").on_hover_text("Tamaño en sub-cubos; Celda = una celda entera (Coma/Punto)");
                    widgets::segmented(ui, &mut tools.brush, &[(1, "1".into()), (2, "2".into()), (4, "4".into()), (8, "Celda".into())]);
                }
                if t == Tool::Box {
                    widgets::segmented(
                        ui,
                        &mut tools.box_shape,
                        &[(BoxShape::Solid, "Llena".into()), (BoxShape::Hollow, "Hueca".into()), (BoxShape::Walls, "Solo paredes".into())],
                    );
                }
                if t == Tool::Fill {
                    ui.label("Hasta");
                    let mut this = tools.fill_range.is_some();
                    if widgets::segmented(ui, &mut this, &[(true, "Esta planta".into()), (false, "Toda".into())]) {
                        tools.fill_range = this.then_some((0, 0));
                    }
                }
                if matches!(t, Tool::Pencil | Tool::Box | Tool::Line) {
                    ui.label("Espejo");
                    widgets::toggle(ui, &mut tools.mirror_x, "Este-oeste").on_hover_text("Pintar también al otro lado, de este a oeste");
                    widgets::toggle(ui, &mut tools.mirror_z, "Norte-sur").on_hover_text("Pintar también al otro lado, de norte a sur");
                }
                let (y, mut on) = st.plane;
                let tip = format!("Pinta solo a esta altura; {}/{} la sube o la baja · K: en la cara señalada", icon::ARROW_UP, icon::ARROW_DOWN);
                if widgets::toggle(ui, &mut on, &format!("{} Plano", icon::ARROWS_IN_LINE_VERTICAL)).on_hover_text(tip).changed() {
                    st.plane.1 = on;
                }
                if on {
                    ui.label(RichText::new(format!("altura {y} · nivel {}", y.div_euclid(SUB))).font(theme::mono(12.0)));
                }
                widgets::toggle(ui, &mut st.editor.tools.detail, &format!("{} Detalle", icon::ARMCHAIR))
                    .on_hover_text("Muebles y escaleras: el corte por planta no los quita");
            }
        }
    });
}

/// Pestaña Vista del modo Editar.
pub(super) fn view(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    if st.editor.doc.is_none() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label("No hay ninguna estructura abierta.");
            if widgets::primary(ui, &format!("{} Estructura nueva", icon::FILE_PLUS)).clicked() {
                st.editor_start();
            }
            ui.label(dim("O ábrela desde la Biblioteca, «Editar» en Explorar, o Archivo › Abrir."));
        });
        return;
    }
    st.ensure_look();
    // Relleno «hasta esta planta»: la franja de la planta de trabajo (o la del sub-cubo señalado).
    if st.editor.tools.fill_range.is_some() {
        let y = st.edit_pointer.hit.map(|h| h.0[1]);
        let fy = st.floor_y(y);
        let h = st.editor.doc.as_ref().map_or(1, |d| d.s.grid.size[1]);
        st.editor.tools.fill_range = Some(((fy - 1).clamp(0, h - 1), (fy + 2 * SUB - 2).clamp(0, h - 1)));
    }
    let full = ui.available_rect_before_wrap();
    let strip = Rect::from_min_size(full.min, vec2(72.0, full.height()));
    ui.scope_builder(UiBuilder::new().max_rect(strip), |ui| tool_strip(ui, st));
    let rest = Rect::from_min_max(egui::pos2(full.min.x + 78.0, full.min.y), full.max);
    ui.scope_builder(UiBuilder::new().max_rect(rest), |ui| {
        context_bar(ui, st, &cat);
        viewport(ui, st, &cat);
    });
}

fn viewport(ui: &mut Ui, st: &mut State, cat: &Catalog) {
    let Some(s) = st.editor.structure() else { return };
    // Lo que se ve: la vista previa de construcción si la hay; si no, el documento.
    let building = st.editor.tools.tool == Tool::Build;
    let want = match (&st.ed.preview, building) {
        (Some(_), true) => (true, st.ed.received),
        _ => (false, st.editor.revision),
    };
    if st.ed.shown != Some(want) || st.edit_theme != st.editor.palette.theme {
        st.ed.shown = Some(want);
        st.edit_theme = st.editor.palette.theme;
        let shown = if want.0 { st.ed.preview.clone().unwrap_or_else(|| s.clone()) } else { s.clone() };
        if let Some(vp) = st.viewports.as_mut() {
            vp.edit.set_structure(shown, &paint_of(st.editor.doc.as_ref(), cat, st.editor.palette.theme));
        }
    }
    st.editor.tools.material = st.editor.palette.selected.max(1);
    let (plane_y, plane_on) = st.plane;
    let mut gizmos: Vec<Gizmo> = Vec::new();
    if plane_on {
        gizmos.push(Gizmo::Plane { y: plane_y });
    }
    if let Some(doc) = &st.editor.doc {
        for p in st.editor.tools.preview(doc, &st.edit_pointer) {
            gizmos.push(match p {
                Preview::Ghost { cells, erase } => Gizmo::Ghost { cells, material: st.editor.tools.material, erase },
                Preview::Selection { min, max } => Gizmo::Selection { min, max },
            });
        }
    }
    // Construcción: el rectángulo que se arrastra.
    if building && st.ed.pressed && let Some(g) = st.ed.gesture {
        let (a, b) = (g.anchor, g.current);
        let y = st.floor_y(Some(g.y));
        gizmos.push(Gizmo::Selection {
            min: [a[0].min(b[0]) * SUB, y, a[1].min(b[1]) * SUB],
            max: [a[0].max(b[0]) * SUB + SUB - 1, y, a[1].max(b[1]) * SUB + SUB - 1],
        });
    }
    st.edit_view.gizmos = gizmos;
    let Some(vp) = st.viewports.as_mut() else { return };
    let r = vp.edit.ui(ui, &mut st.edit_view);
    st.view_rect = Some(r.response.rect);
    view::overlay(ui, r.response.rect, &mut st.edit_view, &s, true);
    // Mensaje junto al cursor mientras se construye.
    if building && let Some(pos) = r.response.hover_pos() {
        let (text, color) = match (&st.ed.preview_error, st.ed.pressed) {
            (Some(e), _) => (e.clone(), theme::ERROR),
            (None, true) => {
                let info = st.ed.tools_cat.iter().find(|t| t.id == st.editor.tools.build);
                let what = match (info, st.ed.gesture) {
                    (Some(i), Some(g)) if i.target == "rect" => {
                        let n = st
                            .ed
                            .params
                            .get(&i.id)
                            .and_then(|p| p.get("storeys"))
                            .map(|n| format!(" · {}", widgets::count(*n as usize, "planta", "plantas")))
                            .unwrap_or_default();
                        let (w, d) = ((g.anchor[0] - g.current[0]).abs() + 1, (g.anchor[1] - g.current[1]).abs() + 1);
                        format!("{} {w}×{d} celdas{n} — ", i.label)
                    }
                    _ => String::new(),
                };
                (format!("{what}Suelta para construir · Esc cancela"), theme::TEXT)
            }
            (None, false) => (String::new(), theme::TEXT),
        };
        if !text.is_empty() {
            let at = pos + vec2(16.0, 18.0);
            let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(12.0), color);
            let bg = Rect::from_min_size(at, galley.size() + vec2(10.0, 6.0));
            ui.painter().rect_filled(bg, 3.0, theme::alpha(theme::STONE0, 0.9));
            ui.painter().galley(at + vec2(5.0, 3.0), galley, color);
        }
    }
    // Puntero → sub-cubos.
    let (pressed, down, released, alt) = ui.input(|i| {
        let primary = egui::PointerButton::Primary;
        (i.pointer.button_pressed(primary), i.pointer.button_down(primary), i.pointer.button_released(primary), i.modifiers.alt)
    });
    let over = r.response.hovered() || r.response.dragged() || r.response.drag_stopped();
    let ground = s.ground_level * SUB;
    let on_plane = |y: i32| r.ray.and_then(|(o, d)| pick::ray_plane_y(o, d, y as f32 + 0.5)).map(floor_v).map(|v| [v[0], y, v[2]]);
    let (hit, place) = if plane_on {
        let v = on_plane(plane_y);
        (v.filter(|v| s.grid.get(v[0], v[1], v[2]) != 0).map(|v| (v, [0, 1, 0])), v)
    } else {
        let hit = r.hovered.map(|h| (h.voxel, h.normal));
        let place = match hit {
            Some((v, n)) if n != [0, 0, 0] => Some([v[0] + n[0], v[1] + n[1], v[2] + n[2]]),
            Some((v, _)) => Some(v),
            None => on_plane(ground),
        };
        (hit, place)
    };
    let place = place.filter(|v| s.grid.in_bounds(v[0], v[1], v[2]));
    st.edit_place = place;
    st.hover = hit.map(|h| h.0).or(place);
    let was_down = st.edit_pointer.down;
    st.edit_pointer = Pointer { hit, place, pressed: pressed && over, down: down && (over || was_down), released: released && was_down, alt };
    if building {
        if over || st.ed.pressed {
            st.build_step(&r, pressed && over, down, released);
        } else if st.ed.preview.is_some() && st.ed.commit_on.is_none() {
            // El puntero salió del visor sin arrastrar: fuera la vista previa.
            st.ed.preview = None;
            st.ed.sent_sig.clear();
        }
    } else {
        let pointer = st.edit_pointer;
        if let Some(doc) = st.editor.doc.as_mut()
            && (pointer.pressed || pointer.down || pointer.released)
            && let Some(m) = st.editor.tools.step(doc, &pointer)
        {
            st.editor.palette.pick(m);
        }
    }
    if released {
        st.edit_pointer.down = false;
    }
    if st.editor.tools.selection.is_some() || st.edit_pointer.down || st.ed.commit_on.is_some() {
        ui.ctx().request_repaint();
    }
}

pub(super) fn palette(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    if st.editor.palette.selected == 0 {
        st.editor.palette.pick(cat.materials.iter().position(|m| m == "Stone").unwrap_or(1) as u8);
    }
    if let Some(role) = &st.ed.role_pick {
        ui.label(RichText::new(format!("{}  Elige el material para «{}»", icon::HAND_POINTING, role_label(role))).color(theme::TITLE));
    }
    let before = st.editor.palette.selected;
    st.editor.palette.ui(ui, &cat);
    // Con un papel del Aspecto esperando, el clic en la Paleta lo cambia.
    if st.editor.palette.selected != before
        && let Some(role) = st.ed.role_pick.take()
        && let Some(doc) = st.editor.doc.as_mut()
        && let Some(name) = cat.materials.get(st.editor.palette.selected as usize).cloned()
    {
        let mut look = doc.meta.look.clone().unwrap_or_else(|| default_look(&cat));
        look.roles.insert(role, name);
        doc.edit_extras("Aspecto", |e| e.meta.look = Some(look));
    }
}

pub(super) fn library(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    st.ed.library.refresh(&st.host.paths.agame_root, &cat.materials, false);
    // «Del juego»: el original (sin sustituciones), sin girar e intacto.
    if st.ed.library.section == library::Section::Game
        && let Some(p) = st.ed.library.game.clone()
    {
        let mut req = p.request();
        req.facing = cat.facings.iter().find(|f| f.turns == 0).map_or(3, |f| f.value);
        req.damage = cat.damages.first().map_or_else(|| "Intact".into(), |d| d.name.clone());
        req.dynamic = 0;
        req.party = st.ed.library.game_party;
        req.procedural = true;
        let sig = format!("{req:?}");
        if sig != st.ed.library.game_asked && st.host.status == crate::host::Status::Ready {
            st.ed.library.game_asked = sig;
            st.host.build(crate::host::Purpose::Library, req);
        }
    }
    // Miniaturas de muebles: se piden al abrir Piezas.
    if st.ed.library.section == library::Section::Pieces
        && let Some(f) = &st.ed.furniture
    {
        let missing: Vec<String> = f.pieces.iter().map(|p| p.id.clone()).filter(|id| !st.ed.furniture_asked.contains(id)).collect();
        for id in missing {
            st.ed.furniture_asked.insert(id.clone());
            st.host.stamp(format!("mini:{id}"), &id, 3, None, 0);
        }
    }
    let ctx = ui.ctx().clone();
    let rs = st.viewports.as_ref().map(|v| v.rs.clone());
    let mut budget = 2; // miniaturas por fotograma, para no trabar la interfaz
    let mut thumb = |key: &str, s: &Structure, paint: Option<&Paint>| {
        if budget == 0 {
            ctx.request_repaint();
            return None;
        }
        budget -= 1;
        let rs = rs.as_ref()?;
        let view = crate::render::ViewState { layers: crate::render::Layers::none(), ..Default::default() };
        let own;
        let paint = match paint {
            Some(p) => p,
            None => {
                own = Paint::new(&cat, s.theme, None);
                &own
            }
        };
        let img = crate::render::render_image(rs, s, paint, &view, [208, 160]);
        Some(ctx.load_texture(key, img, egui::TextureOptions::LINEAR))
    };
    let recent = st.settings.recent.clone();
    let current = st.editor.doc.as_ref().and_then(|d| d.path.clone());
    let furniture = st.ed.furniture.clone();
    if let Some(act) = st.ed.library.ui(ui, &cat, furniture.as_ref(), &recent, current.as_deref(), &mut thumb) {
        st.library_action(act);
    }
}

pub(super) fn properties(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    if st.editor.doc.is_none() {
        ui.label(dim("Abre o crea una estructura."));
        return;
    }
    replaces_section(ui, st, &cat);
    look_section(ui, st, &cat);
    let ed = &mut st.editor;
    let Some(doc) = ed.doc.as_mut() else { return };
    properties::ui(ui, &cat, doc, ed.tools.selection, &mut ed.resize);
    let Some((a, b)) = ed.tools.selection else { return };
    ui.separator();
    widgets::section(ui, "Selección");
    let size = [(a[0] - b[0]).abs() + 1, (a[1] - b[1]).abs() + 1, (a[2] - b[2]).abs() + 1];
    ui.label(RichText::new(format!("{}×{}×{} sub-cubos", size[0], size[2], size[1])).font(theme::mono(13.0)));
    let mut to = a;
    ui.horizontal(|ui| {
        ui.label("Mover a");
        for (k, axis) in [(0, "x"), (1, "y"), (2, "z")] {
            ui.add(egui::DragValue::new(&mut to[k]).prefix(format!("{axis} ")));
        }
    });
    if to != a {
        ed.tools.nudge(doc, [to[0] - a[0], to[1] - a[1], to[2] - a[2]]);
    }
    let mut stamp = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button("Rellenar con el material").clicked() {
            doc.begin_stroke("Rellenar selección");
            for v in ops::cuboid(a, b) {
                doc.paint(v[0], v[1], v[2], ed.tools.material, ed.tools.detail);
            }
            doc.end_stroke();
        }
        if ui.button("Vaciar").on_hover_text("Supr").clicked() {
            ed.tools.clear_selection(doc);
        }
        if ui.button("Marcar como detalle").clicked() {
            doc.begin_stroke("Marcar detalle");
            for v in ops::cuboid(a, b) {
                let m = doc.s.grid.get(v[0], v[1], v[2]);
                if m != 0 {
                    doc.paint(v[0], v[1], v[2], m, true);
                }
            }
            doc.end_stroke();
        }
        stamp = ui.button(format!("{} Guardar como sello…", icon::STAMP)).clicked();
    });
    if stamp {
        st.ed.stamp_name = Some("Sello".into());
    }
}

/// «Sustituye en el juego a…» (§10): qué diseño, a qué barrio/clase/altura se acota, ver el original o soltarlo.
fn replaces_section(ui: &mut Ui, st: &mut State, cat: &Catalog) {
    let Some(r) = st.editor.doc.as_ref().and_then(|d| d.meta.replaces.clone()) else { return };
    egui::CollapsingHeader::new(RichText::new(format!("{}  Sustituye en el juego a", icon::SWAP)).font(theme::semibold(13.0)))
        .default_open(true)
        .show(ui, |ui| {
            ui.label(RichText::new(library::describe(&r, cat)).color(theme::CYAN));
            ui.label(dim("Acotar a (si no, vale para todas):"));
            let mut changed = r.clone();
            let scope = st.ed.source_scope.clone();
            let scope_box = |ui: &mut Ui, label: &str, cur: &mut Option<String>, src: Option<String>| {
                let mut on = cur.is_some();
                if ui.add_enabled(cur.is_some() || src.is_some(), egui::Checkbox::new(&mut on, label)).changed() {
                    *cur = if on { src } else { None };
                }
            };
            scope_box(ui, "Este barrio", &mut changed.district, scope.as_ref().and_then(|s| s.1.clone()));
            scope_box(ui, "Esta clase", &mut changed.tier, scope.as_ref().and_then(|s| s.2.clone()));
            let mut rank = changed.rank.map(|v| v.to_string());
            scope_box(ui, "Esta altura", &mut rank, scope.as_ref().and_then(|s| s.0).map(|v| v.to_string()));
            changed.rank = rank.and_then(|v| v.parse().ok());
            if changed != r
                && let Some(doc) = st.editor.doc.as_mut()
            {
                doc.edit_extras("Alcance de la sustitución", |e| e.meta.replaces = Some(changed));
            }
            ui.horizontal(|ui| {
                if ui.button("Ver el original").clicked() {
                    st.show_original(&r);
                }
                if ui.button("Convertir en estructura propia").on_hover_text("Deja de sustituir al diseño del juego y lo desbloquea todo").clicked()
                    && let Some(doc) = st.editor.doc.as_mut()
                {
                    doc.edit_extras("Convertir en estructura propia", |e| e.meta.replaces = None);
                }
            });
        });
}

/// Aspecto (§10): barrio, clase y papeles de material de las herramientas de construcción.
fn look_section(ui: &mut Ui, st: &mut State, cat: &Catalog) {
    let building = st.editor.tools.tool == Tool::Build;
    // Se abre al empezar a construir y se cierra al dejar de hacerlo; entre medias manda el clic del usuario.
    let open = (st.ed.look_for != Some(building)).then_some(building);
    st.ed.look_for = Some(building);
    egui::CollapsingHeader::new(RichText::new(format!("{}  Aspecto", icon::PALETTE)).font(theme::semibold(13.0)))
        .open(open)
        .show(ui, |ui| {
            ui.label(dim("Lo que usan Habitación, Muro, Tejado… Pulsa un elemento (Muro, Tejado…) y luego un material en la Paleta."));
            if let Some(style) = st.editor.doc.as_ref().and_then(|d| cat.styles.iter().find(|s| s.name == d.meta.style)) {
                ui.label(dim(format!("Estilo: {} (se cambia en Estructura)", style.label)));
            }
            let generated = st.ed.roles.clone();
            let Some(doc) = st.editor.doc.as_mut() else { return };
            let mut look = doc.meta.look.clone().unwrap_or_else(|| default_look(cat));
            let before = look.clone();
            ui.horizontal(|ui| {
                ui.label("Barrio");
                let cur = cat.districts.iter().find(|d| d.name == look.district).map_or("—", |d| d.label.as_str());
                egui::ComboBox::from_id_salt("aspecto-barrio").selected_text(cur).show_ui(ui, |ui| {
                    for d in &cat.districts {
                        ui.selectable_value(&mut look.district, d.name.clone(), &d.label);
                    }
                });
            });
            let tiers: Vec<(String, String)> = cat.tiers.iter().map(|t| (t.name.clone(), t.label.clone())).collect();
            ui.label("Clase");
            widgets::segmented(ui, &mut look.tier, &tiers);
            let mut roles = generated;
            roles.extend(look.roles.clone());
            // Orden de lectura (de abajo arriba de un edificio), no alfabético.
            const ORDER: [&str; 14] =
                ["wall", "timber", "plinth", "quoin", "frame", "shutter", "leaf", "trim", "floor", "roof", "ridge", "verge", "chimney", "dome"];
            let mut roles: Vec<(String, String)> = roles.into_iter().collect();
            roles.sort_by_key(|(r, _)| ORDER.iter().position(|o| o == r).unwrap_or(ORDER.len()));
            egui::Grid::new("papeles").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                for (role, mat) in &roles {
                    let picking = st.ed.role_pick.as_deref() == Some(role.as_str());
                    if ui.selectable_label(picking, role_label(role)).on_hover_text("Clic y luego un material en la Paleta").clicked() {
                        st.ed.role_pick = if picking { None } else { Some(role.clone()) };
                        if !picking {
                            st.open_tab = Some(Tab::Palette);
                        }
                    }
                    ui.horizontal(|ui| {
                        if let Some(rgb) = cat.materials.iter().position(|m| m == mat).and_then(|i| cat.palette.get(i)) {
                            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::hover());
                            ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                            theme::bevel(ui.painter(), r, false);
                        }
                        ui.label(crate::labels::material_label(mat));
                    });
                    if look.roles.contains_key(role) {
                        if ui.small_button(icon::ARROW_COUNTER_CLOCKWISE).on_hover_text("Cambiado a mano: volver al del estilo").clicked() {
                            look.roles.remove(role);
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
            if roles.is_empty() {
                ui.label(dim("Los papeles llegan cuando el generador esté listo."));
            }
            if look != before {
                doc.edit_extras("Aspecto", |e| e.meta.look = Some(look));
            }
        });
}

pub(super) fn validation(ui: &mut Ui, st: &mut State) {
    let Some(doc) = &st.editor.doc else { return };
    if doc.s.nav.levels == 0 && st.host.status != crate::host::Status::Ready {
        ui.label(dim("La validación llega cuando el generador esté listo."));
        return;
    }
    let counter = doc.s.checks.iter().any(|c| c.code == "override-counter");
    if let Some(at) = inspector::checks(ui, &doc.s) {
        st.locate(&at);
    }
    // Falta de mostrador: atajo para colocarlo.
    if counter && ui.button(format!("{} Colocar mostrador", icon::STOREFRONT)).clicked() {
        st.take_piece("Counter", 3);
    }
}

pub(super) fn history(ui: &mut Ui, st: &mut State) {
    let Some(doc) = &st.editor.doc else { return };
    let names: Vec<String> = doc.history().map(str::to_string).collect();
    if names.is_empty() {
        ui.label(dim("Aún no hay cambios."));
    }
    for (i, n) in names.iter().enumerate().rev() {
        let c = if i + 1 == names.len() { theme::TITLE } else { theme::TEXT };
        ui.label(RichText::new(format!("{}. {n}", i + 1)).color(c));
    }
}
