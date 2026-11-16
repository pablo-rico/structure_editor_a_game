//! Paleta de materiales (§6): familias plegables, fila de recientes, búsqueda y muestras como mini cubo isométrico
//! con las tres bandas del toon.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use super::widgets::{dim, section};
use crate::bridge::proto::Catalog;
use crate::labels::{self, Family};
use crate::theme;

const SWATCH: f32 = 28.0;
const RECENT: usize = 8;

#[derive(Default)]
pub struct Palette {
    pub selected: u8,
    pub recent: Vec<u8>,
    pub search: String,
    /// Elemento con que se previsualizan los materiales de color variable.
    pub theme: Option<usize>,
}

impl Palette {
    /// Modelos: añade `rgb` a la paleta del modelo (si no está ya) y lo deja elegido. `false` si no caben más.
    pub fn add_color(&mut self, doc: &mut crate::edit::Doc, rgb: [u8; 3]) -> bool {
        let Some(model) = doc.model.as_ref() else { return false };
        let at = match model.palette.iter().position(|&c| c == rgb) {
            Some(i) => i,
            None if model.palette.len() >= crate::vmodel::MAX_COLORS => return false,
            None => {
                doc.edit_model("Añadir color", |m| m.palette.push(rgb));
                model_len(doc) - 1
            }
        };
        self.selected = at as u8 + 1;
        true
    }

    /// Paleta de un modelo: sus colores, añadir uno y cambiar el elegido (recolorea todo lo que lo usa).
    pub fn ui_model(&mut self, ui: &mut Ui, doc: &mut crate::edit::Doc) {
        let Some(palette) = doc.model.as_ref().map(|m| m.palette.clone()) else { return };
        self.selected = self.selected.clamp(1, palette.len().max(1) as u8);
        let at = self.selected as usize - 1;
        let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        if let Some(&current) = palette.get(at) {
            ui.horizontal(|ui| {
                let mut rgb = current;
                ui.color_edit_button_srgb(&mut rgb).on_hover_text("Cambiar este color en todo el modelo");
                ui.label(egui::RichText::new(hex(rgb)).font(theme::mono(13.0)).color(theme::TITLE));
                if rgb != current {
                    doc.edit_model("Cambiar color", |m| m.palette[at] = rgb);
                }
            });
        }
        section(ui, &format!("Colores del modelo ({})", palette.len()));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (i, &rgb) in palette.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::click());
                    iso_cube(ui.painter(), r, rgb);
                    if self.selected as usize == i + 1 {
                        ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.5, theme::GOLD), egui::StrokeKind::Inside);
                    }
                    // El magenta de la intro no es un color: el juego lo cambia por el del reino.
                    let tip = if rgb == [255, 0, 255] { "Color de la nación: el juego lo cambia por el del reino".to_string() } else { hex(rgb) };
                    if resp.on_hover_text(tip).clicked() {
                        self.selected = i as u8 + 1;
                    }
                }
            });
            ui.add_space(6.0);
            let full = palette.len() >= crate::vmodel::MAX_COLORS;
            let add = ui.add_enabled(!full, egui::Button::new(format!("{} Añadir color", egui_phosphor::regular::PLUS)));
            if add.on_disabled_hover_text("Un modelo admite 255 colores como mucho").clicked() {
                // Parte del color elegido: se ajusta luego con el selector de arriba.
                let base = palette.get(at).copied().unwrap_or([128, 128, 128]);
                let mut rgb = base;
                while palette.contains(&rgb) {
                    rgb[1] = rgb[1].wrapping_add(8);
                }
                self.add_color(doc, rgb);
            }
        });
    }

    pub fn pick(&mut self, m: u8) {
        self.selected = m;
        self.recent.retain(|&r| r != m);
        self.recent.insert(0, m);
        self.recent.truncate(RECENT);
    }

    pub fn ui(&mut self, ui: &mut Ui, cat: &Catalog) {
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(format!(
            "{}  Buscar material",
            egui_phosphor::regular::MAGNIFYING_GLASS
        )));
        let palette = cat.palette_for(self.theme).to_vec();
        // Material en uso, siempre a la vista.
        if let Some(name) = cat.materials.get(self.selected as usize) {
            ui.horizontal(|ui| {
                let rgb = palette.get(self.selected as usize).copied().unwrap_or([255, 0, 255]);
                let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                ui.label(egui::RichText::new(labels::material_label(name)).color(theme::TITLE));
            });
        }
        if !self.recent.is_empty() {
            section(ui, "Recientes");
            let recent = self.recent.clone();
            ui.horizontal_wrapped(|ui| {
                for m in recent {
                    self.swatch(ui, cat, &palette, m);
                }
            });
        }
        let needle = self.search.to_lowercase();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for fam in Family::ALL {
                let ms: Vec<u8> = cat
                    .materials
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| labels::material(n).is_some_and(|(_, f)| f == fam))
                    .filter(|(_, n)| needle.is_empty() || labels::material_label(n).to_lowercase().contains(&needle))
                    .map(|(i, _)| i as u8)
                    .collect();
                if ms.is_empty() {
                    continue;
                }
                egui::CollapsingHeader::new(egui::RichText::new(fam.label()).font(theme::semibold(13.0)))
                    .id_salt(fam.label())
                    .default_open(matches!(fam, Family::Stone | Family::Wood | Family::Roofs))
                    .open(if needle.is_empty() { None } else { Some(true) })
                    .show(ui, |ui| {
                        if fam == Family::Variable {
                            self.theme_picker(ui, cat);
                        }
                        ui.horizontal_wrapped(|ui| {
                            for m in ms {
                                self.swatch(ui, cat, &palette, m);
                            }
                        });
                    });
            }
        });
    }

    fn theme_picker(&mut self, ui: &mut Ui, cat: &Catalog) {
        ui.horizontal(|ui| {
            ui.label(dim("Ver con:"));
            let current = self.theme.and_then(|t| cat.elements.get(t)).map_or("Sin tema", |e| e.label.as_str());
            egui::ComboBox::from_id_salt("ver-con").selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.theme, None, "Sin tema");
                for (i, e) in cat.elements.iter().enumerate() {
                    ui.selectable_value(&mut self.theme, Some(i), &e.label);
                }
            });
        });
    }

    fn swatch(&mut self, ui: &mut Ui, cat: &Catalog, palette: &[[u8; 3]], m: u8) {
        let name = cat.materials.get(m as usize).map_or("", String::as_str);
        let rgb = palette.get(m as usize).copied().unwrap_or([255, 0, 255]);
        let (rect, resp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, theme::HOLE);
        theme::bevel(p, rect, false);
        iso_cube(p, rect.shrink(3.0), rgb);
        if labels::is_variable(name) {
            // Esquina partida: el color depende del tema o del reino.
            let r = rect.shrink(1.0);
            p.add(Shape::convex_polygon(
                vec![r.right_bottom(), r.right_bottom() - vec2(9.0, 0.0), r.right_bottom() - vec2(0.0, 9.0)],
                theme::TEXT,
                Stroke::new(1.0, theme::STONE0),
            ));
        }
        if m == self.selected {
            p.rect_stroke(rect, 2.0, Stroke::new(2.0, theme::GOLD), StrokeKind::Inside);
        } else if resp.hovered() {
            p.rect_stroke(rect, 2.0, Stroke::new(1.0, theme::STONE3), StrokeKind::Inside);
        }
        if resp.on_hover_text(labels::material_label(name)).clicked() {
            self.pick(m);
        }
    }
}

/// Cubo isométrico: tapa clara, lado izquierdo medio y derecho oscuro (como las tres bandas del toon).
fn model_len(doc: &crate::edit::Doc) -> usize {
    doc.model.as_ref().map_or(0, |m| m.palette.len())
}

pub fn iso_cube(p: &egui::Painter, r: Rect, rgb: [u8; 3]) {
    let k = |f: f32| Color32::from_rgb((rgb[0] as f32 * f) as u8, (rgb[1] as f32 * f) as u8, (rgb[2] as f32 * f) as u8);
    let (cx, w, h) = (r.center().x, r.width() / 2.0, r.height() / 4.0);
    let top = pos2(cx, r.top());
    let mid = pos2(cx, r.top() + 2.0 * h);
    let left = pos2(cx - w, r.top() + h);
    let right = pos2(cx + w, r.top() + h);
    let bottom = pos2(cx, r.bottom());
    let quad = |pts: [Pos2; 4], c: Color32| Shape::convex_polygon(pts.to_vec(), c, Stroke::NONE);
    p.add(quad([top, right, mid, left], k(1.0)));
    p.add(quad([left, mid, bottom, pos2(cx - w, r.bottom() - h)], k(0.78)));
    p.add(quad([mid, right, pos2(cx + w, r.bottom() - h), bottom], k(0.58)));
}
