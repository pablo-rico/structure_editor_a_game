//! «Bajo el cursor» (material, coordenadas, detalle, parte, interactuable y suelo del sub-cubo señalado) y la lista de
//! comprobaciones del plano.

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icon;

use super::widgets::{dim, section};
use crate::bridge::proto::{Catalog, CheckAt, Severity};
use crate::model::{SUB, Structure};
use crate::{labels, theme};

fn inside(b: &[i32; 6], v: [i32; 3]) -> bool {
    (b[0]..b[3]).contains(&v[0]) && (b[1]..b[4]).contains(&v[1]) && (b[2]..b[5]).contains(&v[2])
}

/// Panel del sub-cubo `voxel`. `pinned` = fijado con clic.
pub fn under_cursor(ui: &mut Ui, cat: &Catalog, s: &Structure, voxel: Option<[i32; 3]>, pinned: &mut bool) {
    ui.horizontal(|ui| {
        if voxel.is_some() {
            let label = if *pinned { format!("{} Fijado", icon::PUSH_PIN) } else { format!("{} Fijar", icon::PUSH_PIN) };
            if ui.selectable_label(*pinned, label).on_hover_text("Clic en el visor para fijar o soltar").clicked() {
                *pinned = !*pinned;
            }
        }
    });
    let Some(v) = voxel else {
        ui.label(dim("Pasa el ratón por la estructura."));
        return;
    };
    let m = s.grid.get(v[0], v[1], v[2]);
    let name = cat.materials.get(m as usize).map_or("?", String::as_str);
    egui::Grid::new("bajo-cursor").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
        ui.label(dim("Material"));
        ui.horizontal(|ui| {
            if let Some(rgb) = cat.palette_for(s.theme).get(m as usize) {
                let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            }
            ui.label(labels::material_label(name));
        });
        ui.end_row();
        let mono = |t: String| RichText::new(t).font(theme::mono(13.0));
        ui.label(dim("Celda"));
        ui.label(mono(format!("{}, {}", v[0].div_euclid(SUB), v[2].div_euclid(SUB))));
        ui.end_row();
        let level = v[1].div_euclid(SUB);
        ui.label(dim("Nivel"));
        ui.label(mono(format!("{level}")));
        ui.end_row();
        ui.label(dim("Sub-cubo"));
        ui.label(mono(format!("{}, {}, {}", v[0].rem_euclid(SUB), v[1].rem_euclid(SUB), v[2].rem_euclid(SUB))));
        ui.end_row();
        if s.grid.is_detail(v[0], v[1], v[2]) {
            ui.label(dim("Detalle"));
            ui.label("Mueble o escalera (no se corta en la planta)");
            ui.end_row();
        }
        if let Some(p) = s.parts.iter().find(|p| inside(&p.bbox, v)) {
            ui.label(dim("Parte"));
            ui.label(labels::part_kind(&p.kind));
            ui.end_row();
        }
        if let Some(it) = s.interactables.iter().find(|i| inside(&i.bbox, v)) {
            ui.label(dim("Uso"));
            ui.label(labels::interactable_kind(&it.kind));
            ui.end_row();
        }
        let (cx, cz) = (v[0].div_euclid(SUB), v[2].div_euclid(SUB));
        let floor = (level..=level + 1).find(|&l| s.nav.has_floor(cx, cz, l));
        ui.label(dim("Se anda"));
        ui.label(match floor {
            Some(l) => format!("Sí, suelo en el nivel {l}"),
            None => "No".into(),
        });
        ui.end_row();
    });
}

/// Lista de comprobaciones; devuelve la ubicación pulsada («Localizar»).
pub fn checks(ui: &mut Ui, s: &Structure) -> Option<CheckAt> {
    let mut located = None;
    let mut list: Vec<_> = s.checks.iter().collect();
    list.sort_by_key(|c| c.severity);
    let errors = list.iter().filter(|c| c.severity == Severity::Error).count();
    let warnings = list.iter().filter(|c| c.severity == Severity::Warning).count();
    let title = match (errors, warnings) {
        (0, 0) => format!("{}  Comprobaciones", icon::CHECK_CIRCLE),
        _ => format!("Comprobaciones  {} {errors}  {} {warnings}", icon::X_CIRCLE, icon::WARNING),
    };
    section(ui, &title);
    if list.is_empty() {
        ui.label(RichText::new("Todo en orden: se entra por la puerta y se llega a todas las plantas.").color(theme::GREEN));
    }
    for c in list {
        let (glyph, color) = match c.severity {
            Severity::Error => (icon::X_CIRCLE, theme::ERROR),
            Severity::Warning => (icon::WARNING, theme::WARN),
            Severity::Info => (icon::INFO, theme::CYAN),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(glyph).color(color));
            let row = ui.add(egui::Label::new(&c.message).sense(egui::Sense::click()));
            if let Some(at) = &c.at
                && (row.clicked() || ui.small_button(format!("{} Localizar", icon::CROSSHAIR)).clicked())
            {
                located = Some(at.clone());
            }
        });
    }
    located
}
