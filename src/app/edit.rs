//! Modo Editar (fase 2, §5 y §8): barra de herramientas, visor con pincel y gizmos, paleta, propiedades, validación,
//! historial, atajos, abrir/guardar/exportar.

use std::path::PathBuf;

use eframe::egui::{self, Key, RichText, Ui, vec2};
use egui_phosphor::regular as icon;

use super::{Mode, State, loading, view};
use crate::edit::ops::{self, V};
use crate::edit::tools::{Pointer, Preview, Tool};
use crate::editor::source_dir;
use crate::mesh::Paint;
use crate::model::SUB;
use crate::pick;
use crate::render::Gizmo;
use crate::theme;
use crate::ui::widgets::{self, ToastKind, dim};
use crate::ui::{inspector, properties};

/// Qué se iba a hacer cuando apareció «hay cambios sin guardar».
pub(super) enum Confirm {
    Fork,
    New,
    Open(PathBuf),
}

fn floor_v(p: glam::Vec3) -> V {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

impl State {
    pub(super) fn editor_start(&mut self) {
        if let Some(cat) = self.catalog.clone() {
            self.editor.new_doc(&cat, 6, 6, 4);
            if let Some(doc) = &self.editor.doc {
                self.edit_view.frame(&doc.s);
            }
            if let Some(path) = crate::editor::Editor::recovery_available() {
                self.toasts.push(ToastKind::Info, "Hay una copia de recuperación de la última sesión (Archivo › Abrir).", Some(path));
            }
        }
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
        if self.editor.doc.as_ref().is_some_and(|d| d.unsaved) {
            self.confirm = Some(Confirm::Open(path));
        } else {
            self.open(&path);
        }
    }

    fn open(&mut self, path: &std::path::Path) {
        let Some(cat) = self.catalog.clone() else { return };
        match self.editor.open(path, &cat.materials) {
            Ok(()) => {
                self.mode = Mode::Edit;
                if let Some(doc) = &self.editor.doc {
                    self.edit_view.frame(&doc.s);
                }
            }
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido abrir: {e:#}"), None),
        }
    }

    pub(super) fn save(&mut self, as_new: bool) {
        let Some(cat) = self.catalog.clone() else { return };
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
        match self.editor.save(target, &cat.materials) {
            Ok(p) => self.toasts.push(ToastKind::Ok, "Guardado", Some(p)),
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido guardar: {e:#}"), None),
        }
    }

    fn export(&mut self) {
        let Some(cat) = self.catalog.clone() else { return };
        let errors = self.editor.errors();
        if errors > 0 {
            self.toasts.push(ToastKind::Error, format!("Corrige {} para exportar.", widgets::count(errors, "error", "errores")), None);
            return;
        }
        match self.editor.export(&mut self.host, &cat.materials) {
            Ok(p) => self.toasts.push(ToastKind::Info, "Exportando al juego…", Some(p)),
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha exportado: {e:#}"), None),
        }
    }

    pub(super) fn confirm_dialog(&mut self, ctx: &egui::Context) {
    if self.confirm.is_none() {
            return;
        }
        let mut choice = None;
        egui::Modal::new(egui::Id::new("sin-guardar")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Hay cambios sin guardar").font(theme::semibold(18.0)).color(theme::TITLE));
            ui.label("Si sigues, se pierden los cambios de la estructura que estás editando.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::primary(ui, "Guardar y seguir").clicked() {
                    choice = Some(true);
                }
                if ui.add(egui::Button::new(RichText::new("Descartar").color(theme::TEXT)).fill(theme::DANGER_BG)).clicked() {
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
        if let Some(doc) = self.editor.doc.as_mut() {
            doc.unsaved = false;
        }
        match self.confirm.take() {
            Some(Confirm::Fork) => self.fork_main(),
            Some(Confirm::New) => self.editor_start(),
            Some(Confirm::Open(p)) => self.open(&p),
            None => {}
        }
    }

    pub(super) fn edit_title(&mut self, ui: &mut Ui) {
        let Some(doc) = &self.editor.doc else { return };
        let dirty = if doc.unsaved { " ●" } else { "" };
        ui.label(RichText::new(format!("{}{dirty}", doc.meta.name)).color(theme::TEXT));
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
        let Some(doc) = self.editor.doc.as_mut() else { return };
        let done = if redo { doc.redo() } else { doc.undo() };
        if let Some(label) = done {
            let verb = if redo { "Rehecho" } else { "Deshecho" };
            self.toasts.push(ToastKind::Info, format!("{verb}: {label}"), None);
        }
    }

    pub(super) fn edit_shortcuts(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.editor.doc.as_mut() else { return };
        let tools = &mut self.editor.tools;
        let (keys, events) = ctx.input(|i| (i.clone(), i.events.clone()));
        let plain = keys.modifiers.is_none();
        let cmd = keys.modifiers.command;
        let shift = keys.modifiers.shift;
        for t in Tool::ALL {
            let k = Key::from_name(t.key()).unwrap_or(Key::Space);
            if plain && keys.key_pressed(k) {
                tools.tool = t;
            }
        }
        if plain && keys.key_pressed(Key::Comma) {
            tools.brush_down();
        }
        if plain && keys.key_pressed(Key::Period) {
            tools.brush_up();
        }
        if plain && (keys.key_pressed(Key::ArrowUp) || keys.key_pressed(Key::ArrowDown)) {
            let step = if keys.key_pressed(Key::ArrowUp) { 1 } else { -1 };
            self.plane = ((self.plane.0 + step).clamp(0, doc.s.grid.size[1] - 1), true);
        }
        if plain && keys.key_pressed(Key::Escape) {
            tools.selection = None;
        }
        let place = self.edit_place.or(tools.selection.map(|s| s.0)).unwrap_or([0, 0, 0]);
        for e in &events {
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
        if cmd && !shift && keys.key_pressed(Key::Z) {
            doc.undo();
        }
        if cmd && shift && keys.key_pressed(Key::Z) {
            doc.redo();
        }
        if cmd && keys.key_pressed(Key::D) && tools.copy(doc) {
            // Duplicar: copia y pega una celda más allá en X.
            let at = tools.selection.map_or(place, |s| [s.0[0] + SUB, s.0[1], s.0[2]]);
            tools.paste(doc, at);
        }
        if shift && !cmd && keys.key_pressed(Key::X) {
            tools.transform(doc, "Espejo X", |c| c.mirror_x());
        }
        if shift && !cmd && keys.key_pressed(Key::Z) {
            tools.transform(doc, "Espejo Z", |c| c.mirror_z());
        }
        if cmd && keys.key_pressed(Key::R) {
            let turns = if shift { 3 } else { 1 };
            tools.transform(doc, "Girar", |c| (0..turns).fold(c.clone(), |c, _| c.rotate_quarter()));
        }
        if cmd && keys.key_pressed(Key::E) {
            self.export();
        }
    }
}

/// Tira de herramientas y opciones del pincel, encima y a la izquierda del visor.
fn toolbar(ui: &mut Ui, st: &mut State) {
    let tools = &mut st.editor.tools;
    ui.horizontal_wrapped(|ui| {
        for t in Tool::ALL {
            let on = tools.tool == t;
            let text = RichText::new(t.icon()).size(17.0).color(if on { theme::TITLE } else { theme::TEXT });
            let b = egui::Button::new(text)
                .fill(if on { theme::HOLE } else { theme::STONE1 })
                .stroke(egui::Stroke::new(1.0, if on { theme::GOLD } else { theme::STONE0 }))
                .min_size(vec2(30.0, 28.0));
            let r = ui.add(b);
            theme::bevel(ui.painter(), r.rect, !on);
            if r.on_hover_text(format!("{} ({})\n{}", t.label(), t.key(), t.hint())).clicked() {
                tools.tool = t;
            }
        }
        ui.separator();
        ui.label("Pincel");
        widgets::segmented(ui, &mut tools.brush, &[(1, "1".into()), (2, "2".into()), (4, "4".into()), (8, "Celda".into())]);
        ui.separator();
        ui.label("Espejo");
        widgets::toggle(ui, &mut tools.mirror_x, "X").on_hover_text("Pintar también al otro lado en X (este-oeste)");
        widgets::toggle(ui, &mut tools.mirror_z, "Z").on_hover_text("Pintar también al otro lado en Z (norte-sur)");
        ui.separator();
        let (y, mut on) = st.plane;
        let tip = format!("Pinta solo a esta altura; {}/{} la sube o la baja", icon::ARROW_UP, icon::ARROW_DOWN);
        if widgets::toggle(ui, &mut on, &format!("{} Solo plano", icon::LOCK_SIMPLE)).on_hover_text(tip).changed() {
            st.plane.1 = on;
        }
        if on {
            ui.label(RichText::new(format!("altura {y} · nivel {}", y.div_euclid(SUB))).font(theme::mono(12.0)));
        }
        widgets::toggle(ui, &mut tools.detail, &format!("{} Detalle", icon::ARMCHAIR))
            .on_hover_text("Pintar como detalle: muebles y escaleras, que el corte por planta no quita");
    });
}

/// Pestaña Vista del modo Editar: pinta con la herramienta activa.
pub(super) fn view(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    if st.editor.doc.is_none() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label("No hay ninguna estructura abierta.");
            if widgets::primary(ui, &format!("{} Estructura nueva", icon::FILE_PLUS)).clicked() {
                st.editor_start();
            }
            ui.label(dim("O «Duplicar para editar» desde Explorar, o Archivo › Abrir."));
        });
        return;
    }
    toolbar(ui, st);
    let Some(s) = st.editor.structure() else { return };
    let Some(vp) = st.viewports.as_mut() else { return };
    if st.edit_uploaded != st.editor.revision || st.edit_theme != st.editor.palette.theme {
        st.edit_uploaded = st.editor.revision;
        st.edit_theme = st.editor.palette.theme;
        vp.edit.set_structure(s.clone(), &Paint::new(&cat, st.editor.palette.theme, None));
    }
    st.editor.tools.material = st.editor.palette.selected.max(1);
    // Gizmos de este fotograma (con el puntero del anterior: un fotograma de retraso no se nota).
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
    st.edit_view.gizmos = gizmos;
    let r = vp.edit.ui(ui, &mut st.edit_view);
    st.view_rect = Some(r.response.rect);
    view::overlay(ui, r.response.rect, &mut st.edit_view, &s);

    // Puntero → sub-cubos.
    let (pressed, down, released, alt) = ui.input(|i| {
        let primary = egui::PointerButton::Primary;
        (i.pointer.button_pressed(primary), i.pointer.button_down(primary), i.pointer.button_released(primary), i.modifiers.alt)
    });
    let over = r.response.hovered() || r.response.dragged() || r.response.drag_stopped();
    let ground = s.ground_level * SUB;
    let on_plane = |y: i32| {
        r.ray.and_then(|(o, d)| pick::ray_plane_y(o, d, y as f32 + 0.5)).map(floor_v).map(|v| [v[0], y, v[2]])
    };
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
    st.edit_pointer = Pointer {
        hit,
        place,
        pressed: pressed && over,
        down: down && (over || st.edit_pointer.down),
        released: released && st.edit_pointer.down,
        alt,
    };
    let pointer = st.edit_pointer;
    if let Some(doc) = st.editor.doc.as_mut()
        && (pointer.pressed || pointer.down || pointer.released)
        && let Some(m) = st.editor.tools.step(doc, &pointer)
    {
        st.editor.palette.pick(m);
    }
    if pointer.released {
        st.edit_pointer.down = false;
    }
    if st.editor.tools.selection.is_some() || pointer.down {
        ui.ctx().request_repaint();
    }
}

pub(super) fn palette(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    if st.editor.palette.selected == 0 {
        st.editor.palette.pick(cat.materials.iter().position(|m| m == "Stone").unwrap_or(1) as u8);
    }
    st.editor.palette.ui(ui, &cat);
}

pub(super) fn properties(ui: &mut Ui, st: &mut State) {
    let Some(cat) = st.catalog.clone() else { return loading(ui, &st.host.status) };
    let ed = &mut st.editor;
    let Some(doc) = ed.doc.as_mut() else {
        ui.label(dim("Abre o crea una estructura."));
        return;
    };
    properties::ui(ui, &cat, doc, ed.tools.selection, &mut ed.resize);
    if let Some((a, b)) = ed.tools.selection {
        ui.separator();
        widgets::section(ui, "Selección");
        let size = [(a[0] - b[0]).abs() + 1, (a[1] - b[1]).abs() + 1, (a[2] - b[2]).abs() + 1];
        ui.label(RichText::new(format!("{}×{}×{} sub-cubos", size[0], size[1], size[2])).font(theme::mono(13.0)));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Rellenar con el material").clicked() {
                doc.begin_stroke("Rellenar selección");
                for v in ops::cuboid(a, b) {
                    doc.paint(v[0], v[1], v[2], ed.tools.material, ed.tools.detail);
                }
                doc.end_stroke();
            }
            if ui.button("Vaciar").clicked() {
                doc.begin_stroke("Vaciar selección");
                for v in ops::cuboid(a, b) {
                    doc.paint(v[0], v[1], v[2], 0, false);
                }
                doc.end_stroke();
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
        });
    }
}

pub(super) fn validation(ui: &mut Ui, st: &mut State) {
    let Some(doc) = &st.editor.doc else { return };
    if doc.s.nav.levels == 0 && st.host.status != crate::host::Status::Ready {
        ui.label(dim("La validación llega cuando el generador esté listo."));
        return;
    }
    if let Some(at) = inspector::checks(ui, &doc.s) {
        st.locate(&at);
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
