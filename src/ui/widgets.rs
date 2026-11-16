//! Controles propios de la especificación (§6): segmentado, paso a paso, semilla, chip, cabecera de sección y toasts.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Response, RichText, Sense, Stroke, Ui, vec2};
use egui_phosphor::regular as icon;

use crate::theme;

/// Título de sección dentro de un panel (SemiBold 13, crema).
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).font(theme::semibold(13.0)).color(theme::TEXT));
}

/// Texto atenuado (solo sobre fondos de panel o más oscuros).
pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(theme::TEXT_DIM).small()
}

/// Botones segmentados: devuelve true si cambió.
pub fn segmented<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, options: &[(T, String)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (v, label) in options {
            let on = v == value;
            let text = RichText::new(label.as_str()).color(if on { theme::TITLE } else { theme::TEXT });
            let b = egui::Button::new(text)
                .fill(if on { theme::HOLE } else { theme::STONE1 })
                .stroke(Stroke::new(1.0, if on { theme::GOLD } else { theme::STONE0 }));
            if ui.add(b).clicked() && !on {
                *value = v.clone();
                changed = true;
            }
        }
    });
    changed
}

/// Conmutador con marco (como los segmentados): encendido hundido y dorado, apagado en relieve.
pub fn toggle(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let t = RichText::new(text).color(if *on { theme::TITLE } else { theme::TEXT });
    let b = egui::Button::new(t)
        .fill(if *on { theme::HOLE } else { theme::STONE1 })
        .stroke(Stroke::new(1.0, if *on { theme::GOLD } else { theme::STONE0 }));
    let mut r = ui.add(b);
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    r
}

/// Número con su sustantivo en singular o plural: «1 error», «3 errores».
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// «‹ Diseño 2 de 6 ›» sobre un índice 0..=max. Devuelve true si cambió.
pub fn stepper(ui: &mut Ui, label: &str, value: &mut i64, min: i64, max: i64) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        if ui.add_enabled(*value > min, egui::Button::new(icon::CARET_LEFT)).on_hover_text("Anterior").clicked() {
            *value -= 1;
        }
        let n = *value - min + 1;
        let total = max - min + 1;
        ui.label(RichText::new(format!("{label} {n} de {total}")).font(theme::mono(13.0)));
        if ui.add_enabled(*value < max, egui::Button::new(icon::CARET_RIGHT)).on_hover_text("Siguiente").clicked() {
            *value += 1;
        }
    });
    *value = (*value).clamp(min, max);
    *value != before
}

/// Campo de semilla: número mono, dado, ‹ › y copiar.
pub fn seed(ui: &mut Ui, seed: &mut i32) -> bool {
    let before = *seed;
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(seed).speed(1.0)).on_hover_text("Semilla: el mismo número da siempre lo mismo");
        if ui.button(icon::DICE_FIVE).on_hover_text("Semilla al azar (D)").clicked() {
            *seed = random_seed();
        }
        if ui.button(icon::CARET_LEFT).on_hover_text(format!("Semilla anterior ({})", icon::ARROW_LEFT)).clicked() {
            *seed = seed.wrapping_sub(1);
        }
        if ui.button(icon::CARET_RIGHT).on_hover_text(format!("Semilla siguiente ({})", icon::ARROW_RIGHT)).clicked() {
            *seed = seed.wrapping_add(1);
        }
        if ui.button(icon::COPY_SIMPLE).on_hover_text("Copiar la semilla").clicked() {
            ui.ctx().copy_text(seed.to_string());
        }
    });
    *seed != before
}

/// Semilla al azar sin dependencias: reloj + contador mezclados.
pub fn random_seed() -> i32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let mut x = t ^ N.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x % 100_000) as i32
}

/// Chip con icono y texto (24 pt, fondo hueco). Devuelve su respuesta (clicable).
pub fn chip(ui: &mut Ui, glyph: &str, text: &str, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(format!("{glyph}  {text}"), egui::FontId::proportional(12.0), color);
    let size = vec2(galley.size().x + 16.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.hovered() { theme::STONE1 } else { theme::HOLE };
    ui.painter().rect_filled(rect, 2.0, fill);
    theme::bevel(ui.painter(), rect, false);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Botón principal (ámbar, texto oscuro).
pub fn primary(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        for (state, fill) in [(&mut w.inactive, theme::AMBER), (&mut w.hovered, theme::GOLD), (&mut w.active, theme::GOLD)] {
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.fg_stroke.color = theme::STONE0;
        }
        ui.button(RichText::new(text).strong())
    })
    .inner
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Ok,
    Error,
}

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    /// Acción opcional («Mostrar en Finder»): etiqueta y ruta a revelar.
    pub reveal: Option<std::path::PathBuf>,
    born: Instant,
    hovered: bool,
}

/// Avisos abajo a la derecha (§6): máximo 3, 4 s, el hover los detiene, los de error esperan a cerrarse.
#[derive(Default)]
pub struct Toasts(Vec<Toast>);

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>, reveal: Option<std::path::PathBuf>) {
        self.0.push(Toast { kind, text: text.into(), reveal, born: Instant::now(), hovered: false });
        if self.0.len() > 3 {
            self.0.remove(0);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, area: egui::Rect) {
        let life = Duration::from_secs(4);
        self.0.retain(|t| t.kind == ToastKind::Error || t.hovered || t.born.elapsed() < life);
        let mut close = None;
        let count = self.0.len();
        for (i, t) in self.0.iter_mut().enumerate().rev() {
            let (glyph, color) = match t.kind {
                ToastKind::Info => (icon::INFO, theme::CYAN),
                ToastKind::Ok => (icon::CHECK_CIRCLE, theme::GREEN),
                ToastKind::Error => (icon::X_CIRCLE, theme::ERROR),
            };
            let y = area.bottom() - 12.0 - (count - 1 - i) as f32 * 72.0;
            let r = egui::Area::new(egui::Id::new(("toast", i)))
                .order(egui::Order::Foreground)
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(area.right() - 12.0, y))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(336.0);
                        ui.horizontal(|ui| {
                            let (bar, _) = ui.allocate_exact_size(vec2(3.0, 28.0), Sense::hover());
                            ui.painter().rect_filled(bar, 0.0, color);
                            ui.label(RichText::new(glyph).color(color).size(16.0));
                            ui.vertical(|ui| {
                                ui.set_max_width(270.0);
                                ui.add(egui::Label::new(&t.text).wrap());
                                if let Some(path) = &t.reveal
                                    && ui.link(crate::platform::REVEAL_LABEL).clicked()
                                {
                                    crate::platform::reveal(path);
                                }
                            });
                            if ui.small_button(icon::X).on_hover_text("Cerrar").clicked() {
                                close = Some(i);
                            }
                        });
                    });
                });
            t.hovered = r.response.contains_pointer();
        }
        if let Some(i) = close {
            self.0.remove(i);
        }
        if !self.0.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}
