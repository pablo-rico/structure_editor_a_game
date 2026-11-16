//! Catálogo de tipos (§6, «Tarjeta del catálogo»): grupos plegables con recuento, filtro y tarjetas con miniatura.

use std::collections::HashMap;

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, StrokeKind, TextureHandle, Ui, vec2};

use super::widgets::dim;
use crate::bridge::proto::{Catalog, KindInfo};
use crate::{labels, theme};

const CARD: egui::Vec2 = egui::vec2(112.0, 128.0);
const THUMB: egui::Vec2 = egui::vec2(104.0, 88.0);

/// Dibuja el catálogo; devuelve el tipo pulsado.
pub fn ui(
    ui: &mut Ui,
    cat: &Catalog,
    selected: &str,
    filter: &mut String,
    thumbs: &HashMap<String, TextureHandle>,
) -> Option<String> {
    let mut picked = None;
    ui.add(egui::TextEdit::singleline(filter).hint_text(format!("{}  Filtrar", egui_phosphor::regular::MAGNIFYING_GLASS)));
    let needle = filter.to_lowercase();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for g in &cat.groups {
            let kinds: Vec<&KindInfo> = cat
                .kinds
                .iter()
                .filter(|k| k.group == g.key)
                .filter(|k| needle.is_empty() || k.label.to_lowercase().contains(&needle))
                .collect();
            if kinds.is_empty() {
                continue;
            }
            let title = RichText::new(format!("{}  {}  {}", labels::group_icon(&g.key), g.label, kinds.len()))
                .font(theme::semibold(13.0));
            egui::CollapsingHeader::new(title)
                .id_salt(&g.key)
                .default_open(g.key != "legacy")
                .open(if needle.is_empty() { None } else { Some(true) })
                // Sin sangría: así caben dos tarjetas por fila en el ancho por defecto.
                .show_unindented(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        for k in kinds {
                            if card(ui, cat, k, k.name == selected, thumbs.get(&k.name)).clicked() {
                                picked = Some(k.name.clone());
                            }
                        }
                    });
                });
        }
    });
    picked
}

fn card(ui: &mut Ui, cat: &Catalog, k: &KindInfo, selected: bool, thumb: Option<&TextureHandle>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(CARD, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, theme::STONE1);
    theme::bevel(p, rect, true);
    let well = egui::Rect::from_min_size(rect.min + vec2(4.0, 4.0), THUMB);
    p.rect_filled(well, 2.0, theme::HOLE);
    theme::bevel(p, well, false);
    match thumb {
        Some(t) => {
            let size = t.size_vec2();
            let scale = (THUMB.x / size.x).min(THUMB.y / size.y).clamp(0.1, 1.0);
            let img = egui::Rect::from_center_size(well.center(), size * scale);
            p.image(t.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            p.text(well.center(), Align2::CENTER_CENTER, labels::group_icon(&k.group), FontId::proportional(28.0), theme::STONE2);
        }
    }
    // Nombre en una línea, con «…» si no cabe; debajo, el tamaño en celdas.
    let mut job = egui::text::LayoutJob::simple_singleline(k.label.clone(), FontId::proportional(13.0), theme::TEXT);
    job.wrap = egui::text::TextWrapping { max_width: CARD.x - 12.0, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    p.galley(egui::pos2(rect.left() + 6.0, well.bottom() + 3.0), p.layout_job(job), theme::TEXT);
    let size = match k.size {
        Some(r) if r.min == r.max => format!("{}×{} celdas", r.min[0], r.min[1]),
        Some(r) => format!("{}–{} celdas", r.min[0].min(r.min[1]), r.max[0].max(r.max[1])),
        None => "Tamaño por semilla".into(),
    };
    p.text(egui::pos2(rect.left() + 6.0, well.bottom() + 20.0), Align2::LEFT_TOP, size, FontId::proportional(12.0), theme::TEXT_DIM);
    let border = if selected {
        Some(Stroke::new(2.0, theme::GOLD))
    } else if resp.hovered() {
        Some(Stroke::new(1.0, theme::STONE3))
    } else {
        None
    };
    if let Some(s) = border {
        p.rect_stroke(rect, 2.0, s, StrokeKind::Inside);
    }
    if resp.has_focus() {
        p.rect_stroke(rect.expand(2.0), 3.0, Stroke::new(1.0, theme::FOCUS_RING), StrokeKind::Outside);
    }
    let mut tip = k.description.clone();
    if let Some(r) = k.size {
        tip.push_str(&format!("\nTamaño: {}–{} × {}–{} celdas", r.min[0], r.max[0], r.min[1], r.max[1]));
    }
    if !k.styles.is_empty() {
        tip.push_str(&format!("\n{} estilos", k.styles.len().min(cat.styles.len())));
    }
    resp.on_hover_ui(|ui| {
        ui.label(RichText::new(&k.label).strong());
        ui.label(dim(tip.trim()));
    })
}
