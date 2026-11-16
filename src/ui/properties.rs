//! Panel «Propiedades» de Editar (§5): estructura (nombre, estilo, tamaño, franjas, entrada), partes móviles e
//! interactuables. Todo cambio pasa por `Doc::edit_extras` o `Doc::replace_grid` para poder deshacerlo.

use eframe::egui::{self, DragValue, RichText, Ui};
use egui_phosphor::regular as icon;

use super::widgets::{self, dim, section};
use crate::bridge::proto::{Catalog, Interactable, Part};
use crate::edit::{Doc, Extras};
use crate::model::{SUB, VoxelGrid};
use crate::{labels, theme};

pub const PART_KINDS: [&str; 5] = ["Door", "Breakable", "Portcullis", "Bascule", "GateLeaves"];
pub const INTERACT_KINDS: [&str; 12] =
    ["Door", "Bed", "Chair", "Chest", "Counter", "HarbourDesk", "Helm", "Cannon", "CargoHatch", "Anchor", "Gangway", "Mooring"];
/// Tope de partes móviles del juego (`StructureParts.MaxParts`); el puente lo comprueba igualmente.
pub const MAX_PARTS: usize = 16;

/// Tamaño que se está escribiendo antes de aplicarlo (celdas x, niveles, celdas z).
#[derive(Default)]
pub struct Resize(pub Option<[i32; 3]>);

/// Explicaciones de los candados de un diseño del juego (§10).
pub const LOCK_SIZE: &str = "Este diseño tiene ese tamaño en el juego. Para otro tamaño, pulsa «Convertir en estructura propia» (arriba).";
pub const LOCK_DOOR: &str = "El juego pone la calle delante de esta celda. Puedes cambiar la hoja, no su sitio.";
pub const LOCK_STYLE: &str = "Es el estilo del diseño del juego que sustituyes.";

/// Propiedades de un modelo de vóxeles: nombre (el de su fichero), de dónde sale, tamaño y punto de apoyo.
pub fn ui_model(ui: &mut Ui, doc: &mut Doc, resize: &mut Resize) {
    let Some(model) = doc.model.clone() else { return };
    section(ui, "Modelo");
    ui.label(RichText::new(&doc.meta.name).color(theme::TITLE)).on_hover_text("El nombre es el de su fichero: el juego lo busca por él");
    if !model.note().is_empty() {
        ui.label(dim(model.note()));
    }
    if model.edited() {
        ui.label(dim("Retocado a mano: los scripts de arte ya no lo regeneran."));
    }

    section(ui, "Tamaño (vóxeles)");
    let size = doc.s.grid.size;
    let want = resize.0.get_or_insert(size);
    ui.horizontal(|ui| {
        for (k, name) in [(0, "Ancho"), (2, "Fondo"), (1, "Alto")] {
            ui.label(name);
            ui.add(egui::DragValue::new(&mut want[k]).range(1..=crate::vmodel::MAX_SIDE));
        }
    });
    let want = *want;
    ui.label(dim(format!("Una casilla del juego son {} vóxeles. Crece o encoge por la derecha, por detrás y por arriba.", model.cell)));
    if want != size {
        ui.horizontal(|ui| {
            if ui.button("Aplicar").clicked() && !doc.resize_model(want) {
                resize.0 = Some(size);
            }
            if ui.button("Cancelar").clicked() {
                resize.0 = Some(size);
            }
        });
        ui.label(dim("No se puede encoger por debajo de lo que ya hay dibujado."));
    }

    section(ui, "Punto de apoyo");
    match model.origin {
        Some(mut o) => {
            ui.horizontal(|ui| {
                for (k, axis) in [(0, "x"), (1, "y"), (2, "z")] {
                    ui.add(egui::DragValue::new(&mut o[k]).prefix(format!("{axis} ")));
                }
            });
            ui.label(dim("El punto del modelo que cae en la esquina de su casilla en el juego."));
            if Some(o) != model.origin {
                doc.edit_model("Punto de apoyo", |m| m.origin = Some(o));
            }
        }
        None => {
            ui.label(dim("Este modelo se apoya en su esquina."));
            if ui.button("Mover el punto de apoyo").clicked() {
                doc.edit_model("Punto de apoyo", |m| m.origin = Some([0, 0, 0]));
            }
        }
    }
}

pub fn ui(ui: &mut Ui, cat: &Catalog, doc: &mut Doc, selection: Option<([i32; 3], [i32; 3])>, resize: &mut Resize) {
    // Sustituir un diseño del juego fija estilo, tamaño y puerta (los niveles quedan libres).
    let locked = doc.meta.replaces.is_some();
    let mut e = doc.extras();
    let before = e.clone();
    let mut label = "Propiedades";

    egui::CollapsingHeader::new(RichText::new("Estructura").font(theme::semibold(13.0))).default_open(true).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Nombre");
            let r = ui.add_enabled(!locked, egui::TextEdit::singleline(&mut e.meta.name));
            if r.on_disabled_hover_text("El nombre lo da el diseño del juego que sustituyes.").changed() {
                label = "Nombre";
            }
        });
        if locked {
            ui.label(dim(format!("{} Lo fija el juego: estilo, tamaño y sitio de la puerta.", icon::LOCK_SIMPLE)));
        }
        section(ui, &lock_label("Estilo", locked));
        let styles: Vec<(String, String)> = cat.styles.iter().map(|s| (s.name.clone(), s.label.clone())).collect();
        let r = ui.add_enabled_ui(!locked, |ui| widgets::segmented(ui, &mut e.meta.style, &styles));
        r.response.on_disabled_hover_text(LOCK_STYLE);
        if r.inner {
            label = "Estilo";
        }
        if let Some(id) = e.meta.id {
            ui.label(dim(format!("En el juego: estructura a mano nº {id}")));
        }
        size_editor(ui, doc, resize, locked);
        section(ui, &lock_label("Entrada", locked));
        let cells = doc.s.grid.cells();
        ui.horizontal(|ui| {
            ui.label("Puerta en la celda");
            let r = ui.add_enabled(!locked, DragValue::new(&mut e.entrance[0]).range(0..=cells[0] - 1));
            if r.on_disabled_hover_text(LOCK_DOOR).changed() {
                label = "Entrada";
            }
        });
        ui.label(dim("En la fachada delantera (sur), contando desde el oeste."));
        if ui.checkbox(&mut e.courtyard, "Patio sin techo (cuenta como dentro)").changed() {
            label = "Patio";
        }
        bands(ui, &mut e, &mut label);
    });

    egui::CollapsingHeader::new(RichText::new(format!("Partes móviles ({} de {MAX_PARTS})", e.parts.len())).font(theme::semibold(13.0)))
        .default_open(true)
        .show(ui, |ui| parts(ui, cat, &mut e, selection, &mut label));

    egui::CollapsingHeader::new(RichText::new(format!("Interactuables ({})", e.interactables.len())).font(theme::semibold(13.0)))
        .default_open(false)
        .show(ui, |ui| interactables(ui, &mut e, selection, &mut label));

    if e != before {
        doc.edit_extras(label, |x| *x = e);
    }
}

/// Etiqueta de sección con candado si el juego la fija.
fn lock_label(text: &str, locked: bool) -> String {
    if locked { format!("{} {text}", icon::LOCK_SIMPLE) } else { text.to_string() }
}

fn size_editor(ui: &mut Ui, doc: &mut Doc, resize: &mut Resize, locked: bool) {
    section(ui, &lock_label("Tamaño (celdas)", locked));
    let g = &doc.s.grid;
    let now = [g.size[0] / SUB, g.size[1] / SUB, g.size[2] / SUB];
    let mut want = resize.0.unwrap_or(now);
    ui.horizontal(|ui| {
        ui.label("Ancho");
        ui.add_enabled(!locked, DragValue::new(&mut want[0]).range(1..=64)).on_disabled_hover_text(LOCK_SIZE);
        ui.label("Fondo");
        ui.add_enabled(!locked, DragValue::new(&mut want[2]).range(1..=64)).on_disabled_hover_text(LOCK_SIZE);
        ui.label("Niveles");
        ui.add(DragValue::new(&mut want[1]).range(1..=60));
    });
    // Solo se recuerda mientras difiere del tamaño actual (un deshacer no deja un «Aplicar» huérfano).
    resize.0 = (want != now).then_some(want);
    if want != now {
        ui.horizontal(|ui| {
            if widgets::primary(ui, "Aplicar tamaño").on_hover_text("Lo que quede fuera se pierde (se puede deshacer)").clicked() {
                let w = want;
                let mut grid = VoxelGrid::new([w[0] * SUB, w[1] * SUB, w[2] * SUB]);
                let old = &doc.s.grid;
                let detail = old.detail.is_some();
                if detail {
                    grid.detail = Some(vec![0; grid.data.len()]);
                }
                // Se conserva la esquina del origen: el frente (−Z) y el suelo no se mueven.
                for y in 0..old.size[1].min(grid.size[1]) {
                    for z in 0..old.size[2].min(grid.size[2]) {
                        for x in 0..old.size[0].min(grid.size[0]) {
                            let (i, j) = (grid.index(x, y, z), old.index(x, y, z));
                            grid.data[i] = old.data[j];
                            if let (Some(d), Some(od)) = (grid.detail.as_mut(), old.detail.as_ref()) {
                                d[i] = od[j];
                            }
                        }
                    }
                }
                doc.replace_grid("Tamaño", grid);
                resize.0 = None;
            }
            if ui.button("Cancelar").clicked() {
                resize.0 = None;
            }
        });
    }
}

fn bands(ui: &mut Ui, e: &mut Extras, label: &mut &'static str) {
    section(ui, "Plantas");
    ui.label(dim("Altura (en sub-cubos) a la que empieza cada planta y nivel de su suelo."));
    let mut remove = None;
    for i in 0..e.band_starts.len() {
        ui.horizontal(|ui| {
            ui.label(crate::app::floor_name(i));
            if ui.add(DragValue::new(&mut e.band_starts[i]).prefix("altura ")).changed() {
                *label = "Plantas";
            }
            if let Some(f) = e.band_floors.get_mut(i)
                && ui.add(DragValue::new(f).prefix("nivel ")).changed()
            {
                *label = "Plantas";
            }
            if ui.small_button(icon::X).on_hover_text("Quitar planta").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        e.band_starts.remove(i);
        if i < e.band_floors.len() {
            e.band_floors.remove(i);
        }
        *label = "Plantas";
    }
    if ui.button(format!("{} Añadir planta", icon::PLUS)).clicked() {
        let start = e.band_starts.last().map_or(e.ground_level * SUB, |s| s + 2 * SUB);
        let floor = e.band_floors.last().map_or(e.ground_level, |f| f + 2);
        e.band_starts.push(start);
        e.band_floors.push(floor);
        *label = "Plantas";
    }
}

fn box_from(sel: Option<([i32; 3], [i32; 3])>) -> Option<[i32; 6]> {
    let (a, b) = sel?;
    Some([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2]), a[0].max(b[0]) + 1, a[1].max(b[1]) + 1, a[2].max(b[2]) + 1])
}

fn box_row(ui: &mut Ui, b: &mut [i32; 6], sel: Option<([i32; 3], [i32; 3])>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Caja {}×{}×{}", b[3] - b[0], b[4] - b[1], b[5] - b[2])).font(theme::mono(12.0)));
        if let Some(nb) = box_from(sel)
            && ui.small_button("Ajustar a la selección").clicked()
        {
            *b = nb;
            changed = true;
        }
    });
    changed
}

fn parts(ui: &mut Ui, cat: &Catalog, e: &mut Extras, sel: Option<([i32; 3], [i32; 3])>, label: &mut &'static str) {
    let mut remove = None;
    for (i, p) in e.parts.iter_mut().enumerate() {
        ui.push_id(("parte", i), |ui| {
            ui.separator();
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("tipo").selected_text(labels::part_kind(&p.kind)).show_ui(ui, |ui| {
                    for k in PART_KINDS {
                        if ui.selectable_label(p.kind == k, labels::part_kind(k)).clicked() {
                            p.kind = k.into();
                            *label = "Parte";
                        }
                    }
                });
                if ui.small_button(icon::TRASH).on_hover_text("Quitar parte").clicked() {
                    remove = Some(i);
                }
            });
            if box_row(ui, &mut p.bbox, sel) {
                *label = "Parte";
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("material").selected_text(labels::material_label(&p.material)).show_ui(ui, |ui| {
                    for m in cat.materials.iter().filter(|m| labels::material(m).is_some()) {
                        if ui.selectable_label(&p.material == m, labels::material_label(m)).clicked() {
                            p.material = m.clone();
                            *label = "Parte";
                        }
                    }
                });
                let mut bars = p.pattern == "Bars";
                if ui.checkbox(&mut bars, "Barrotes").changed() {
                    p.pattern = if bars { "Bars" } else { "Solid" }.into();
                    *label = "Parte";
                }
            });
            if p.kind == "Door" {
                ui.horizontal(|ui| {
                    let mut swing = p.swing;
                    if widgets::segmented(ui, &mut swing, &[(-1, "Bisagra izquierda".into()), (1, "Bisagra derecha".into())]) {
                        p.swing = swing;
                        *label = "Parte";
                    }
                });
                ui.horizontal(|ui| {
                    *label = if ui.checkbox(&mut p.starts_open, "Empieza abierta").changed() { "Parte" } else { *label };
                    *label = if ui.checkbox(&mut p.locked, "Cerrada con llave").changed() { "Parte" } else { *label };
                });
            }
            *label = if ui.checkbox(&mut p.detail, "Detalle (el corte por planta no la quita)").changed() { "Parte" } else { *label };
        });
    }
    if let Some(i) = remove {
        e.parts.remove(i);
        *label = "Quitar parte";
    }
    let full = e.parts.len() >= MAX_PARTS;
    let add = ui.add_enabled(!full && sel.is_some(), egui::Button::new(format!("{} Parte desde la selección", icon::PLUS)));
    let add = add.on_disabled_hover_text(if full { "Ya hay 16 partes (el máximo del juego)" } else { "Selecciona antes una caja (M)" });
    if add.clicked()
        && let Some(b) = box_from(sel)
    {
        e.parts.push(Part {
            kind: "Door".into(),
            pattern: "Solid".into(),
            material: "Plank".into(),
            bbox: b,
            swing: 1,
            detail: true,
            starts_open: false,
            locked: false,
            cell: None,
        });
        *label = "Añadir parte";
    }
}

fn interactables(ui: &mut Ui, e: &mut Extras, sel: Option<([i32; 3], [i32; 3])>, label: &mut &'static str) {
    let mut remove = None;
    for (i, it) in e.interactables.iter_mut().enumerate() {
        ui.push_id(("inter", i), |ui| {
            ui.separator();
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("tipo").selected_text(labels::interactable_kind(&it.kind)).show_ui(ui, |ui| {
                    for k in INTERACT_KINDS {
                        if ui.selectable_label(it.kind == k, labels::interactable_kind(k)).clicked() {
                            it.kind = k.into();
                            *label = "Interactuable";
                        }
                    }
                });
                if ui.small_button(icon::TRASH).on_hover_text("Quitar").clicked() {
                    remove = Some(i);
                }
            });
            if box_row(ui, &mut it.bbox, sel) {
                *label = "Interactuable";
            }
            ui.horizontal(|ui| {
                ui.label("Mira hacia");
                let mut f = it.facing;
                if widgets::segmented(ui, &mut f, &[(0, "Este".into()), (1, "Oeste".into()), (2, "Norte".into()), (3, "Sur".into())]) {
                    it.facing = f;
                    *label = "Interactuable";
                }
            });
        });
    }
    if let Some(i) = remove {
        e.interactables.remove(i);
        *label = "Quitar interactuable";
    }
    if ui
        .add_enabled(sel.is_some(), egui::Button::new(format!("{} Interactuable desde la selección", icon::PLUS)))
        .on_disabled_hover_text("Selecciona antes una caja (M)")
        .clicked()
        && let Some(b) = box_from(sel)
    {
        e.interactables.push(Interactable { kind: "Chest".into(), bbox: b, arg: 0, facing: 3 });
        *label = "Añadir interactuable";
    }
}
