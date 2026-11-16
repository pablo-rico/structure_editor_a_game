//! Consola (§6, «Chip de recarga»): arriba un resumen para artistas, el detalle técnico plegado.

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icon;

use super::widgets::dim;
use crate::host::{Host, Status};
use crate::theme;

pub fn ui(ui: &mut Ui, host: &mut Host) {
    let errors = host.compile_errors().to_vec();
    match &host.status {
        Status::Broken(_) | Status::Off(_) if !errors.is_empty() => {
            ui.label(
                RichText::new(format!("{}  El código del juego tiene errores; avisa a programación.", icon::X_CIRCLE))
                    .color(theme::ERROR),
            );
            ui.label(dim("Mientras tanto ves la última versión buena del generador."));
            if ui.button(format!("{} Copiar errores", icon::COPY_SIMPLE)).clicked() {
                let text: Vec<String> =
                    errors.iter().map(|e| format!("{}({},{}): {} {}", e.file, e.line, e.col, e.code, e.message)).collect();
                ui.ctx().copy_text(text.join("\n"));
            }
        }
        Status::Off(why) => {
            ui.label(RichText::new(format!("{}  {why}", icon::PLUGS)).color(theme::WARN));
        }
        _ => {
            ui.label(RichText::new(format!("{}  El generador funciona.", icon::CHECK_CIRCLE)).color(theme::GREEN));
        }
    }
    ui.horizontal(|ui| {
        if ui.button(format!("{} Recompilar", icon::ARROWS_CLOCKWISE)).clicked() {
            host.recompile();
        }
    });
    egui::CollapsingHeader::new("Detalle técnico").default_open(!errors.is_empty()).show(ui, |ui| {
        for e in &errors {
            ui.horizontal_wrapped(|ui| {
                let at = format!("{}:{}", short(&e.file), e.line);
                if ui.link(RichText::new(at).font(theme::mono(12.0))).on_hover_text("Abrir en VS Code").clicked() {
                    let _ = std::process::Command::new("code").arg("-g").arg(format!("{}:{}:{}", e.file, e.line, e.col)).spawn();
                }
                ui.label(RichText::new(format!("{} {}", e.code, e.message)).color(theme::ERROR).font(theme::mono(12.0)));
            });
        }
        egui::ScrollArea::vertical().stick_to_bottom(true).max_height(260.0).show(ui, |ui| {
            for l in &host.log {
                let c = if l.error { theme::ERROR } else { theme::TEXT_DIM };
                ui.label(RichText::new(&l.text).color(c).font(theme::mono(12.0)));
            }
        });
    });
}

/// Ruta desde `Assets/` para que quepa.
fn short(path: &str) -> &str {
    path.find("Assets/").map_or(path, |i| &path[i..])
}
