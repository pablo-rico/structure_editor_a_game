//! Arranque del editor.

// Sin ventana de consola en Windows al abrir la versión publicada.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(concat!("Estructuras · a_game ", env!("CARGO_PKG_VERSION")))
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "structure_editor_a_game",
        options,
        Box::new(|cc| Ok(Box::new(structure_editor_a_game::app::App::new(cc)))),
    )
}
