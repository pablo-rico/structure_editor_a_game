//! Panel «Opciones» de Explorar (§5–§6): estilo, campos de variante, tamaño, orientación, cimientos, daño, semilla y
//! estado de las partes móviles. Los campos vienen descritos por el puente; aquí solo se elige el control.

use eframe::egui::{self, RichText, Sense, Stroke, Ui, vec2};

use super::widgets::{self, dim, section};
use crate::bridge::proto::{Catalog, FieldKind, KindInfo};
use crate::model::Structure;
use crate::params::{self, FOUNDATION, PART_GROUPS, Params};
use crate::theme;

/// Dibuja el panel; devuelve true si cambió algo que pide reconstruir.
pub fn ui(ui: &mut Ui, cat: &Catalog, kind: &KindInfo, p: &mut Params, s: Option<&Structure>) -> bool {
    let mut changed = false;
    ui.label(RichText::new(&kind.label).font(theme::semibold(18.0)).color(theme::TITLE));
    if !kind.description.is_empty() {
        ui.label(dim(&kind.description));
    }

    if !kind.styles.is_empty() {
        section(ui, "Estilo");
        let opts: Vec<(String, String)> = kind
            .styles
            .iter()
            .map(|n| (n.clone(), cat.styles.iter().find(|s| &s.name == n).map_or(n.clone(), |s| s.label.clone())))
            .collect();
        if widgets::segmented(ui, &mut p.style, &opts) {
            p.fit(kind, cat);
            changed = true;
        }
    }

    for f in &kind.fields {
        let Some(v) = p.fields.get_mut(&f.key) else { continue };
        // El paso a paso y la casilla ya llevan su nombre.
        if !matches!(f.kind, FieldKind::Bool | FieldKind::Index { .. }) {
            section(ui, &f.label);
        }
        changed |= field(ui, cat, f, v, &p.style);
    }

    if let Some(r) = kind.size {
        section(ui, "Tamaño (celdas)");
        for (label, value, lo, hi) in
            [("Ancho", &mut p.width, r.min[0], r.max[0]), ("Fondo", &mut p.depth, r.min[1], r.max[1])]
        {
            ui.horizontal(|ui| {
                ui.label(label);
                if lo == hi {
                    ui.label(RichText::new(lo.to_string()).font(theme::mono(13.0)));
                    ui.label(dim("fijo"));
                } else {
                    changed |= ui.add(egui::Slider::new(value, lo..=hi).integer()).changed();
                    ui.label(dim(format!("{lo}–{hi}")));
                }
            });
        }
    }

    section(ui, "Orientación");
    let facings: Vec<(i32, String)> = cat.facings.iter().map(|f| (f.value, f.label.clone())).collect();
    changed |= widgets::segmented(ui, &mut p.facing, &facings);

    section(ui, "Cimientos");
    ui.horizontal(|ui| {
        if ui.add_enabled(p.foundation > FOUNDATION.0, egui::Button::new("−")).clicked() {
            p.foundation -= 1;
            changed = true;
        }
        ui.label(RichText::new(p.foundation.to_string()).font(theme::mono(13.0)));
        if ui.add_enabled(p.foundation < FOUNDATION.1, egui::Button::new("+")).clicked() {
            p.foundation += 1;
            changed = true;
        }
        ui.label(dim("niveles bajo el suelo"));
    });

    section(ui, "Daño");
    let damages: Vec<(String, String)> = cat.damages.iter().map(|d| (d.name.clone(), d.label.clone())).collect();
    changed |= widgets::segmented(ui, &mut p.damage, &damages);

    section(ui, "Semilla");
    changed |= widgets::seed(ui, &mut p.seed);

    if let Some(s) = s.filter(|s| !s.parts.is_empty()) {
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("Estado (vista previa)").font(theme::semibold(13.0)))
            .default_open(true)
            .show(ui, |ui| {
                ui.label(dim("No se guarda en el diseño."));
                for g in PART_GROUPS.iter() {
                    let Some(mut state) = params::group_state(&s.parts, p.dynamic, g) else { continue };
                    ui.label(g.label);
                    let opts: Vec<(u32, String)> =
                        g.states.iter().enumerate().map(|(i, l)| (i as u32, (*l).to_string())).collect();
                    if widgets::segmented(ui, &mut state, &opts) {
                        p.dynamic = params::set_group_state(&s.parts, p.dynamic, g, state);
                        changed = true;
                    }
                }
            });
    }
    changed
}

/// Control de un campo descrito por el generador (variantes de un tipo y parámetros de las herramientas de
/// construcción). Devuelve true si cambió.
pub fn field(ui: &mut Ui, cat: &Catalog, f: &crate::bridge::proto::Field, v: &mut i64, style: &str) -> bool {
    match &f.kind {
        FieldKind::Index { min, .. } => {
            let max = params::field_max(&f.kind, style);
            widgets::stepper(ui, &f.label, v, *min, max)
        }
        FieldKind::Range { min, max } => {
            ui.horizontal(|ui| {
                let r = ui.add(egui::Slider::new(v, *min..=*max).integer()).changed();
                ui.label(dim(format!("{min}–{max}")));
                r
            })
            .inner
        }
        FieldKind::Bool => {
            let mut on = *v != 0;
            let r = ui.checkbox(&mut on, &f.label).changed();
            *v = on as i64;
            r
        }
        FieldKind::Choice { options } if options.len() <= 4 => {
            let opts: Vec<(i64, String)> = options.iter().map(|o| (o.value, o.label.clone())).collect();
            widgets::segmented(ui, v, &opts)
        }
        FieldKind::Choice { options } => {
            let before = *v;
            let current = options.iter().find(|o| o.value == *v).map_or("—", |o| o.label.as_str());
            egui::ComboBox::from_id_salt(&f.key).selected_text(current).show_ui(ui, |ui| {
                for o in options {
                    ui.selectable_value(v, o.value, &o.label);
                }
            });
            *v != before
        }
        FieldKind::Mask { bits } => {
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for b in bits {
                    let mut on = *v >> b.bit & 1 != 0;
                    if ui.checkbox(&mut on, &b.label).changed() {
                        *v ^= 1 << b.bit;
                        changed = true;
                    }
                }
            });
            changed
        }
        FieldKind::Element => element_grid(ui, cat, v),
    }
}

/// Rejilla 7×2 de elementos con el color de su tema; el nombre en el tooltip y bajo la elegida.
fn element_grid(ui: &mut Ui, cat: &Catalog, v: &mut i64) -> bool {
    let glow = cat.materials.iter().position(|m| m == "ThemeGlow").or_else(|| cat.materials.iter().position(|m| m == "ThemeStone"));
    let before = *v;
    egui::Grid::new("elementos").spacing(vec2(4.0, 4.0)).show(ui, |ui| {
        for (i, e) in cat.elements.iter().enumerate() {
            let rgb = glow.and_then(|g| e.palette.get(g)).copied().unwrap_or([128, 128, 128]);
            let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
            ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            theme::bevel(ui.painter(), rect, true);
            if i as i64 == *v {
                ui.painter().rect_stroke(rect, 2.0, Stroke::new(2.0, theme::GOLD), egui::StrokeKind::Outside);
            }
            if resp.on_hover_text(&e.label).clicked() {
                *v = i as i64;
            }
            if i % 7 == 6 {
                ui.end_row();
            }
        }
    });
    if let Some(e) = cat.elements.get(*v as usize) {
        ui.label(RichText::new(&e.label).color(theme::TEXT));
    }
    *v != before
}
