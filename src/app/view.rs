//! Visores de Explorar: la Vista con sus controles flotantes (§5: capas, cámara, ascensor de plantas, leyenda), el
//! Muestrario de semillas, Comparar y la captura.

use eframe::egui::{self, Align2, Color32, Rect, RichText, Sense, Ui, UiBuilder, vec2};
use egui_phosphor::regular as icon;

use super::{SheetRows, State, loading};
use crate::model::Structure;
use crate::render::{Layers, Projection, ViewState, ViewportResponse};
use crate::theme;
use crate::ui::widgets::{self, ToastKind, dim};

/// Capas en el orden de sus teclas 1–8: icono, nombre, qué enseña y color de leyenda.
pub(super) const LAYERS: [(&str, &str, &str, Color32); 8] = [
    (icon::GRID_FOUR, "Rejilla", "Celdas del suelo", theme::STONE4),
    (icon::FOOTPRINTS, "Navegación", "Por dónde se anda; la valla roja es paso cortado", theme::overlay::NAV_FILL),
    (icon::DOOR_OPEN, "Partes", "Puertas, rastrillos y muros que se mueven o rompen", theme::overlay::PART),
    (icon::HAND_TAP, "Interactuables", "Lo que se puede usar: camas, sillas, cofres…", theme::overlay::PIN_PLATE),
    (icon::SIGN_IN, "Entrada", "Por dónde se entra", theme::overlay::ENTRANCE),
    (icon::BOUNDING_BOX, "Huella", "Lo que ocupa en el mapa", theme::overlay::FOOTPRINT),
    (icon::ARMCHAIR, "Detalle", "Muebles y escaleras: el corte no los quita", theme::overlay::DETAIL_HATCH),
    (icon::STACK, "Franjas", "Dónde empieza cada planta", theme::overlay::BANDS),
];

pub(super) fn layer_mut(l: &mut Layers, i: usize) -> &mut bool {
    match i {
        0 => &mut l.grid,
        1 => &mut l.nav,
        2 => &mut l.parts,
        3 => &mut l.interactables,
        4 => &mut l.entrance,
        5 => &mut l.footprint,
        6 => &mut l.detail,
        _ => &mut l.bands,
    }
}

/// Panel flotante del visor (§5: fondo de panel al 92 %).
fn float<R>(ui: &mut Ui, rect: Rect, f: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        egui::Frame::new()
            .fill(theme::alpha(theme::PANEL, 0.92))
            .corner_radius(3.0)
            .inner_margin(egui::Margin::same(4))
            .show(ui, f)
            .inner
    })
    .inner
}

fn toggle(ui: &mut Ui, on: bool, glyph: &str, tip: &str) -> bool {
    let text = RichText::new(glyph).size(16.0).color(if on { theme::TITLE } else { theme::TEXT_DIM });
    let b = egui::Button::new(text)
        .fill(if on { theme::HOLE } else { Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0, if on { theme::GOLD } else { Color32::TRANSPARENT }));
    ui.add(b).on_hover_text(tip).clicked()
}

/// Brújula (§5): la N apunta al norte (+Z) del plano visto con esta cámara.
fn compass(ui: &mut Ui, view: &ViewState) {
    let (sy, cy) = view.yaw_deg.to_radians().sin_cos();
    let n = vec2(-sy, -view.elevation_deg.to_radians().sin() * cy).normalized();
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
    let c = r.center();
    ui.painter().circle_stroke(c, 11.0, egui::Stroke::new(1.0, theme::STONE3));
    ui.painter().line_segment([c - n * 6.0, c + n * 3.0], egui::Stroke::new(1.5, theme::TEXT_DIM));
    ui.painter().text(c + n * 7.0, Align2::CENTER_CENTER, "N", theme::semibold(12.0), theme::TEXT);
    resp.on_hover_text("Norte (+Z). La puerta de los planos sin girar mira al sur.");
}

/// Controles flotantes encima del visor (capas, cámara, plantas, leyenda).
pub(super) fn overlay(ui: &mut Ui, rect: Rect, view: &mut ViewState, s: &Structure) {
    let m = 10.0;
    let (row, gap) = (ui.spacing().interact_size.y, ui.spacing().item_spacing.y);
    // Arriba a la izquierda: capas.
    float(ui, Rect::from_min_size(rect.min + vec2(m, m), vec2(330.0, 36.0)), |ui| {
        ui.horizontal(|ui| {
            for (i, (glyph, name, what, _)) in LAYERS.iter().enumerate() {
                let on = *layer_mut(&mut view.layers, i);
                if toggle(ui, on, glyph, &format!("{name} ({})\n{what}", i + 1)) {
                    *layer_mut(&mut view.layers, i) = !on;
                }
            }
        });
    });
    // Arriba a la derecha: cámara, con la brújula entre los dos giros.
    float(ui, Rect::from_min_size(egui::pos2(rect.right() - 262.0 - m, rect.top() + m), vec2(262.0, 36.0)), |ui| {
        ui.horizontal(|ui| {
            if toggle(ui, false, icon::ARROW_ARC_LEFT, "Girar la cámara 90° (Q)") {
                view.rotate(-90.0);
            }
            compass(ui, view);
            if toggle(ui, false, icon::ARROW_ARC_RIGHT, "Girar la cámara 90° al otro lado (R)") {
                view.rotate(90.0);
            }
            let iso = view.projection == Projection::Iso;
            let (glyph, tip) =
                if iso { (icon::CUBE, "Iso, como el juego (P: vista libre)") } else { (icon::PERSPECTIVE, "Vista libre (P: iso)") };
            if toggle(ui, !iso, glyph, tip) {
                view.toggle_projection();
            }
            if toggle(ui, false, icon::CORNERS_OUT, "Encuadrar (F)") {
                view.frame(s);
            }
            if toggle(ui, view.ghost_upper, icon::GHOST, "Mostrar lo de encima como fantasma") {
                view.ghost_upper = !view.ghost_upper;
            }
        });
    });
    // Derecha: ascensor de plantas (corte y leyenda de franjas a la vez).
    let floors = &s.band_floors;
    let h = (floors.len() + 1) as f32 * (26.0 + gap) - gap + 8.0;
    let lift = Rect::from_min_size(egui::pos2(rect.right() - 140.0 - m, rect.center().y - h / 2.0), vec2(140.0, h));
    float(ui, lift, |ui| {
        ui.vertical(|ui| {
            let mut pick = |ui: &mut Ui, level: Option<i32>, text: String, tip: &str| {
                let on = view.cut_level == level;
                let t = RichText::new(text).color(if on { theme::TITLE } else { theme::TEXT });
                let b = egui::Button::new(t).fill(if on { theme::HOLE } else { Color32::TRANSPARENT }).min_size(vec2(130.0, 26.0));
                let b = b.stroke(egui::Stroke::new(1.0, if on { theme::GOLD } else { Color32::TRANSPARENT }));
                if ui.add(b).on_hover_text(tip).clicked() {
                    view.cut_level = level;
                }
            };
            pick(ui, None, format!("{} Todo", icon::STACK), "Ver el edificio entero (C)");
            let tip = format!("Ver hasta esta planta, sin lo de encima · Shift+{}/{} cambia de planta", icon::ARROW_UP, icon::ARROW_DOWN);
            for (i, f) in floors.iter().enumerate().rev() {
                pick(ui, Some(*f), floor_name(i), &tip);
            }
        });
    });
    // Abajo a la izquierda: leyenda de las capas encendidas.
    let on: Vec<usize> = (0..8).filter(|&i| *layer_mut(&mut view.layers, i)).collect();
    if !on.is_empty() {
        let lh = on.len() as f32 * (row + gap) - gap + 8.0;
        float(ui, Rect::from_min_size(egui::pos2(rect.left() + m, rect.bottom() - lh - m), vec2(150.0, lh)), |ui| {
            for i in on {
                let (glyph, name, _, color) = LAYERS[i];
                ui.horizontal(|ui| {
                    ui.label(RichText::new(glyph).color(color));
                    ui.label(RichText::new(name).small());
                });
            }
        });
    }
}

/// Nombre de una planta (el mismo en el ascensor y en Propiedades).
pub(crate) fn floor_name(i: usize) -> String {
    if i == 0 { "Planta baja".into() } else { format!("Planta {i}") }
}

/// Mensaje encima del visor (fallo de una combinación, juego que no compila).
fn banner(ui: &mut Ui, rect: Rect, text: String, color: Color32) {
    let r = Rect::from_min_size(egui::pos2(rect.center().x - 260.0, rect.top() + 56.0), vec2(520.0, 30.0));
    ui.painter().rect_filled(r, 3.0, theme::alpha(theme::STONE0, 0.92));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, egui::FontId::proportional(13.0), color);
}

/// Pestaña Vista de Explorar.
pub(super) fn explore(ui: &mut Ui, st: &mut State) {
    let Some(s) = st.main.s.clone() else { return loading(ui, &st.host.status) };
    let Some(vp) = st.viewports.as_mut() else {
        ui.label(RichText::new("Este equipo no tiene GPU compatible con el visor.").color(theme::WARN));
        return;
    };
    let ViewportResponse { response, hovered, clicked, .. } = vp.main.ui(ui, &mut st.view);
    if !st.pinned {
        st.hover = hovered.map(|h| h.voxel);
    }
    if response.clicked() {
        st.pinned = clicked.is_some() && !st.pinned;
        if let Some(h) = clicked {
            st.hover = Some(h.voxel);
        }
    }
    let rect = response.rect;
    st.view_rect = Some(rect);
    overlay(ui, rect, &mut st.view, &s);
    if let Some(e) = &st.main.error {
        banner(ui, rect, format!("{}  {} · Ves la última versión buena.", icon::WARNING, e.message), theme::ERROR);
    } else if matches!(st.host.status, crate::host::Status::Broken(_)) {
        banner(ui, rect, format!("{}  El juego no compila. Ves la última versión buena.", icon::X_CIRCLE), theme::ERROR);
    }
    if st.main.loading {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Muestrario de semillas (§6).
pub(super) fn sheet(ui: &mut Ui, st: &mut State) {
    if st.catalog.is_none() {
        return loading(ui, &st.host.status);
    }
    ui.horizontal(|ui| {
        ui.label("Muestras");
        let mut n = st.sheet.n;
        widgets::segmented(ui, &mut n, &[(3, "3×3".into()), (4, "4×4".into()), (6, "6×6".into())]);
        st.sheet.n = n;
        ui.separator();
        ui.label("Cada fila cambia");
        let mut rows = st.sheet.rows;
        widgets::segmented(
            ui,
            &mut rows,
            &[
                (SheetRows::Nothing, "Nada".into()),
                (SheetRows::Style, "Estilo".into()),
                (SheetRows::Damage, "Daño".into()),
                (SheetRows::Design, "Diseño".into()),
            ],
        );
        st.sheet.rows = rows;
        ui.separator();
        if ui.button(format!("{} Otra tanda", icon::DICE_FIVE)).clicked() {
            st.sheet.base_seed = widgets::random_seed();
        }
        ui.label(dim("Clic: abrir en la Vista · Cmd+clic: mandar a Comparar"));
    });
    st.refresh_sheet();
    let n = st.sheet.n;
    let avail = ui.available_size();
    let cell = vec2((avail.x / n as f32).floor() - 6.0, (avail.y / n as f32).floor() - 6.0);
    let mut pick = None;
    egui::Grid::new("muestrario").spacing([6.0, 6.0]).show(ui, |ui| {
        for i in 0..n * n {
            let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
            ui.painter().rect_filled(rect, 2.0, theme::VIEW_BG);
            theme::bevel(ui.painter(), rect, false);
            let slot = st.sheet.cells.get(i);
            if let Some(Some(img)) = st.sheet.images.get(i) {
                let size = img.size_vec2();
                let scale = ((rect.width() - 8.0) / size.x).min((rect.height() - 8.0) / size.y);
                let draw = Rect::from_center_size(rect.center(), size * scale);
                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(img.id(), draw, uv, Color32::WHITE);
            } else if let Some(e) = slot.and_then(|c| c.error.as_ref()) {
                // Rayado rojo con ✕ y el informe a mano.
                for k in 0..12 {
                    let x = rect.left() + k as f32 * rect.width() / 6.0;
                    ui.painter().line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x - rect.height(), rect.bottom())],
                        egui::Stroke::new(1.0, theme::alpha(theme::RED, 0.25)),
                    );
                }
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::X_CIRCLE, egui::FontId::proportional(22.0), theme::ERROR);
                resp.clone().on_hover_ui(|ui| {
                    ui.label(&e.message);
                    ui.label(dim("Clic derecho: copiar el informe"));
                });
                if resp.secondary_clicked() {
                    ui.ctx().copy_text(format!("{}\n{}", e.message, e.detail));
                }
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
            }
            if let Some(req) = slot.and_then(|c| c.req.as_ref()) {
                let tag = Rect::from_min_size(rect.left_bottom() + vec2(4.0, -22.0), vec2(60.0, 18.0));
                ui.painter().rect_filled(tag, 2.0, theme::alpha(theme::STONE0, 0.85));
                ui.painter().text(tag.center(), Align2::CENTER_CENTER, req.seed.to_string(), theme::mono(12.0), theme::TEXT);
            }
            if i % n == 0
                && st.sheet.rows != SheetRows::Nothing
                && let Some(p) = st.sheet_params(i)
            {
                let cat = st.catalog.as_ref();
                let label = match st.sheet.rows {
                    SheetRows::Style => cat.and_then(|c| c.styles.iter().find(|s| s.name == p.style)).map(|s| s.label.clone()),
                    SheetRows::Damage => cat.and_then(|c| c.damages.iter().find(|d| d.name == p.damage)).map(|d| d.label.clone()),
                    _ => p.fields.get("design").map(|d| format!("Diseño {}", d + 1)),
                };
                if let Some(label) = label {
                    ui.painter().text(rect.left_top() + vec2(6.0, 4.0), Align2::LEFT_TOP, label, egui::FontId::proportional(12.0), theme::TEXT);
                }
            }
            if resp.hovered() {
                ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, theme::STONE3), egui::StrokeKind::Inside);
            }
            if resp.clicked() {
                pick = Some((i, ui.input(|inp| inp.modifiers.command)));
            }
            if (i + 1) % n == 0 {
                ui.end_row();
            }
        }
    });
    if let Some((i, to_compare)) = pick
        && let Some(p) = st.sheet_params(i)
    {
        if to_compare {
            st.send_to_compare(&p);
            st.toasts.push(ToastKind::Info, format!("Semilla {} enviada a Comparar.", p.seed), None);
        } else {
            st.params = p;
            st.rebuild_main();
        }
    }
}

/// Comparar: dos visores, con la cámara enlazada por defecto.
pub(super) fn compare(ui: &mut Ui, st: &mut State) {
    if st.compare[0].req.is_none() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label("Compara dos versiones lado a lado.");
            ui.label(dim("Cmd+clic en el Muestrario manda una semilla aquí."));
            if widgets::primary(ui, "Comparar con la semilla siguiente").clicked() {
                let mut p = st.params.clone();
                p.seed = p.seed.wrapping_add(1);
                st.send_to_compare(&p);
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut st.linked, format!("{} Cámara enlazada", icon::LINK));
    });
    let cat = st.catalog.clone();
    let heads: Vec<String> = (0..2)
        .map(|i| {
            st.compare[i].req.as_ref().map_or("—".into(), |r| {
                let label = cat.as_ref().and_then(|c| c.kind(&r.kind)).map_or(r.kind.clone(), |k| k.label.clone());
                let style =
                    cat.as_ref().and_then(|c| c.styles.iter().find(|s| s.name == r.style)).map_or(r.style.clone(), |s| s.label.clone());
                format!("{}: {label} · {style} · {}", if i == 0 { "A" } else { "B" }, r.seed)
            })
        })
        .collect();
    let Some(vp) = st.viewports.as_mut() else { return };
    let linked = st.linked;
    ui.columns(2, |cols| {
        for (i, ui) in cols.iter_mut().enumerate() {
            ui.label(RichText::new(&heads[i]).font(theme::mono(12.0)));
            if st.compare[i].s.is_none() {
                loading(ui, &st.host.status);
                continue;
            }
            let v = if linked { 0 } else { i };
            vp.compare[i].ui(ui, &mut st.compare_views[v]);
        }
    });
}

impl State {
    /// Captura de la Vista a 3× (Cmd+Shift+I) en `~/Pictures/Estructuras/`.
    pub(super) fn capture_view(&mut self) {
        let (Some(s), Some(vp), Some(paint)) =
            (self.main.s.clone(), self.viewports.as_ref(), self.main.s.as_deref().and_then(|s| self.paint(s)))
        else {
            return;
        };
        let view = ViewState { fit: true, scale: 3, highlight: None, gizmos: Vec::new(), ..self.view.clone() };
        let img = crate::render::render_image(&vp.rs, &s, &paint, &view, [1920, 1440]);
        let p = &self.params;
        let dir = crate::platform::home_dir().join("Pictures/Estructuras");
        let path = dir.join(format!("{}_{}_{}.png", p.kind, p.style, p.seed));
        match super::save_png(&img, &path) {
            Ok(()) => self.toasts.push(ToastKind::Ok, "Captura guardada", Some(path)),
            Err(e) => self.toasts.push(ToastKind::Error, format!("No se ha podido guardar la captura: {e}"), None),
        }
    }
}
