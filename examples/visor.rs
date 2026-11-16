//! Visor suelto para probar el render a mano: `cargo run --example visor [fixture]` (por defecto `home_northern`).
//! Con `--shots` escribe PNG de todas las fixtures en `target-render/shots/` sin abrir ventana.
//! La LUT se busca en `AGAME_ROOT` o `../a_game`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use eframe::egui;
use structure_editor_a_game::bridge::proto::{self, Catalog};
use structure_editor_a_game::mesh::Paint;
use structure_editor_a_game::render::{self, Gizmo, Layers, Projection, ViewState, Viewport};
use structure_editor_a_game::theme;

const FIXTURES: [&str; 5] = [
    "home_northern",
    "shop_arabic",
    "walltower_ports",
    "schooltower",
    "harbour",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn main() -> Result<()> {
    let arg = std::env::args().nth(1);
    let catalog = proto::load_catalog_fixture(&fixtures())?;
    if arg.as_deref() == Some("--shots") {
        return shots(&catalog);
    }
    if arg.as_deref() == Some("--egui") {
        return egui_frame(&catalog);
    }
    let name = arg.unwrap_or_else(|| FIXTURES[0].into());
    let (_, s) = proto::load_fixture(&fixtures(), &name)?;
    let paint = Paint::new(&catalog, s.theme, None);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Visor · {name}"))
            .with_inner_size([1200.0, 800.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "visor",
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            let rs = cc.wgpu_render_state.as_ref().expect("renderer wgpu");
            let mut viewport = Viewport::new(rs);
            let s = Arc::new(s);
            viewport.set_structure(s.clone(), &paint);
            let mut view = ViewState::default();
            view.frame(&s);
            Ok(Box::new(Visor {
                viewport,
                view,
                status: String::new(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

struct Visor {
    viewport: Viewport,
    view: ViewState,
    status: String,
}

impl eframe::App for Visor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let levels = self.viewport.structure().map_or(0, |s| s.grid.size[1] / 8);
        egui::Panel::top("capas").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let l = &mut self.view.layers;
                for (on, name) in [
                    (&mut l.grid, "1 Rejilla"),
                    (&mut l.nav, "2 Navegación"),
                    (&mut l.parts, "3 Partes"),
                    (&mut l.interactables, "4 Interactuables"),
                    (&mut l.entrance, "5 Entrada"),
                    (&mut l.footprint, "6 Huella"),
                    (&mut l.detail, "7 Detalle"),
                    (&mut l.bands, "8 Franjas"),
                ] {
                    ui.toggle_value(on, name);
                }
                ui.separator();
                let mut cut = self.view.cut_level.is_some();
                if ui.toggle_value(&mut cut, "C Corte").changed() {
                    self.view.cut_level = cut.then_some(1);
                }
                if let Some(level) = &mut self.view.cut_level {
                    ui.add(
                        egui::DragValue::new(level)
                            .range(0..=levels)
                            .prefix("pies "),
                    );
                }
                ui.toggle_value(&mut self.view.ghost_upper, "Fantasma");
                let mut free = self.view.projection == Projection::Free;
                if ui.toggle_value(&mut free, "P Libre").changed() {
                    self.view.projection = if free {
                        Projection::Free
                    } else {
                        Projection::Iso
                    };
                }
                ui.label(format!(
                    "{}× · yaw {:.0}° · elev {:.0}°",
                    self.view.scale, self.view.yaw_deg, self.view.elevation_deg
                ));
            });
        });
        egui::Panel::bottom("estado").show(ui, |ui| ui.label(&self.status));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let r = self.viewport.ui(ui, &mut self.view);
                if let Some(h) = r.hovered {
                    self.status = format!("Sub {:?} · normal {:?}", h.voxel, h.normal);
                }
                if let Some(h) = r.clicked {
                    // Prueba de «Localizar» con la celda del clic.
                    let [x, y, z] = h.voxel.map(|v| v.div_euclid(8) * 8);
                    self.view.highlight = Some([x, y, z, x + 8, y + 8, z + 8]);
                }
            });
    }
}

/// Un fotograma de `Viewport::ui` sin ventana (contexto egui y renderer de egui_wgpu a una textura), a 2 píxeles
/// por punto y con el puntero encima: prueba el callback de pintado, el escalado entero y las chinchetas.
fn egui_frame(catalog: &Catalog) -> Result<()> {
    use eframe::egui_wgpu::wgpu;
    let rs = render::headless_render_state()?;
    let (_, s) = proto::load_fixture(&fixtures(), "home_northern")?;
    let paint = Paint::new(catalog, s.theme, None);
    let s = Arc::new(s);
    let mut viewport = Viewport::new(&rs);
    viewport.set_structure(s.clone(), &paint);
    let mut view = ViewState {
        cut_level: Some(1),
        ..Default::default()
    };
    view.layers.interactables = true;
    view.frame(&s);
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    theme::apply(&ctx);
    let (size, ppp) = ([1000u32, 700u32], 2.0f32);
    let mut hovered = None;
    let mut output: Option<egui::FullOutput> = None;
    let mut textures = egui::TexturesDelta::default();
    for frame in 0..3 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32) / ppp,
            )),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        if frame > 0 {
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(250.0, 190.0)));
        }
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    hovered = viewport.ui(ui, &mut view).hovered;
                });
        });
        textures.append(std::mem::take(&mut out.textures_delta));
        output = Some(out);
    }
    println!("bajo el puntero: {hovered:?}");
    let output = output.unwrap();
    let prims = ctx.tessellate(output.shapes, ppp);
    let desc = eframe::egui_wgpu::ScreenDescriptor {
        size_in_pixels: size,
        pixels_per_point: ppp,
    };
    let target = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: rs.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view_tex = target.create_view(&Default::default());
    let mut encoder = rs.device.create_command_encoder(&Default::default());
    {
        let mut renderer = rs.renderer.write();
        for (id, deltas) in std::mem::take(&mut textures.set) {
            for delta in deltas {
                renderer.update_texture(&rs.device, &rs.queue, id, &delta);
            }
        }
        let extra = renderer.update_buffers(&rs.device, &rs.queue, &mut encoder, &prims, &desc);
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &prims, &desc);
        drop(pass);
        rs.queue.submit(extra.into_iter().chain([encoder.finish()]));
    }
    let row = (size[0] * 4).div_ceil(256) * 256;
    let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * size[1]) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = rs.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    rs.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    rs.device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = buffer.slice(..).get_mapped_range()?;
    let mut rgba = Vec::new();
    for y in 0..size[1] {
        rgba.extend_from_slice(&data[(y * row) as usize..(y * row + size[0] * 4) as usize]);
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target-render/shots/egui_frame.png");
    image::save_buffer(
        &path,
        &rgba,
        size[0],
        size[1],
        image::ExtendedColorType::Rgba8,
    )?;
    println!("{}", path.display());
    Ok(())
}

/// Primer plano a escala 5 del centro de la estructura (para ver el tono por sub-cubo y el contorno).
fn zoom(s: &structure_editor_a_game::model::Structure) -> ViewState {
    let mut view = ViewState {
        scale: 5,
        ..Default::default()
    };
    view.frame(s);
    view.fit = false;
    view
}

/// PNG de cada fixture: iso a 45° y 135°, corte en la planta baja y en la primera, todas las capas.
fn shots(catalog: &Catalog) -> Result<()> {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target-render/shots");
    std::fs::create_dir_all(&out)?;
    let rs = render::headless_render_state()?;
    // Sombras: pilar con voladizo sobre una losa, visto desde el lado del sol (225°) y desde el contrario (45°).
    let mut s = structure_editor_a_game::model::Structure {
        grid: structure_editor_a_game::model::VoxelGrid::new([64, 48, 64]),
        ..Default::default()
    };
    for z in 0..64 {
        for x in 0..64 {
            for y in 0..2 {
                s.grid.set(x, y, z, 1);
            }
        }
    }
    for y in 2..40 {
        for z in 28..36 {
            for x in 28..36 {
                s.grid.set(x, y, z, 5);
            }
        }
    }
    for z in 20..44 {
        for x in 36..56 {
            s.grid.set(x, 30, z, 3);
        }
    }
    let paint = Paint::new(catalog, None, None);
    for yaw in [45.0, 225.0] {
        let img = render::render_image(
            &rs,
            &s,
            &paint,
            &ViewState {
                yaw_deg: yaw,
                ..Default::default()
            },
            [800, 600],
        );
        let path = out.join(format!("sombras_{yaw}.png"));
        image::save_buffer(
            &path,
            img.as_raw(),
            img.width() as u32,
            img.height() as u32,
            image::ExtendedColorType::Rgba8,
        )?;
    }
    for name in FIXTURES {
        let (_, s) = proto::load_fixture(&fixtures(), name)?;
        let paint = Paint::new(catalog, s.theme, None);
        let t = std::time::Instant::now();
        let mesh = structure_editor_a_game::mesh::build(&s, &paint);
        println!(
            "{name} {:?}: mallado {:.1} ms, {} vértices",
            s.grid.size,
            t.elapsed().as_secs_f64() * 1e3,
            mesh.vertices.len()
        );
        let all = Layers {
            grid: true,
            nav: true,
            parts: true,
            interactables: true,
            entrance: true,
            footprint: true,
            detail: true,
            bands: true,
        };
        let floor = |i: usize| s.band_floors.get(i).copied().unwrap_or(s.ground_level);
        let variants = [
            ("45", ViewState::default()),
            (
                "135",
                ViewState {
                    yaw_deg: 135.0,
                    ..Default::default()
                },
            ),
            (
                "corte0",
                ViewState {
                    cut_level: Some(floor(0)),
                    ..Default::default()
                },
            ),
            (
                "corte1_capas",
                ViewState {
                    cut_level: Some(floor(1)),
                    layers: all,
                    ghost_upper: true,
                    yaw_deg: 225.0,
                    ..Default::default()
                },
            ),
            (
                "libre",
                ViewState {
                    projection: Projection::Free,
                    yaw_deg: 20.0,
                    elevation_deg: 35.0,
                    ..Default::default()
                },
            ),
            ("zoom", zoom(&s)),
            (
                "gizmos",
                ViewState {
                    cut_level: Some(floor(0)),
                    highlight: Some([0, 8, 0, 8, 16, 8]),
                    gizmos: vec![
                        Gizmo::Plane { y: 12 },
                        Gizmo::Selection {
                            min: [8, 8, 8],
                            max: [23, 15, 23],
                        },
                        Gizmo::Ghost {
                            cells: (0..16).map(|i| [24 + i % 4, 9, 24 + i / 4]).collect(),
                            material: 3,
                            erase: false,
                        },
                        Gizmo::Ghost {
                            cells: (0..8).map(|i| [10 + i % 4, 9, 30 + i / 4]).collect(),
                            material: 0,
                            erase: true,
                        },
                    ],
                    ..Default::default()
                },
            ),
        ];
        for (tag, view) in variants {
            let t = std::time::Instant::now();
            let img = render::render_image(&rs, &s, &paint, &view, [800, 600]);
            let path = out.join(format!("{name}_{tag}.png"));
            image::save_buffer(
                &path,
                img.as_raw(),
                img.width() as u32,
                img.height() as u32,
                image::ExtendedColorType::Rgba8,
            )?;
            println!(
                "{} ({:.0} ms)",
                path.display(),
                t.elapsed().as_secs_f64() * 1e3
            );
        }
    }
    Ok(())
}
