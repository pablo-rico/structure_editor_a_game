//! Tokens de tema de `docs/diseno-gui.md` («Taller de cantero»). Todo color de la interfaz y de las capas del visor sale
//! de aquí.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Painter, Rect, Shadow, Stroke, TextStyle, Visuals,
    epaint::Pos2,
};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Color con opacidad (0–1) sobre un token opaco.
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

// Piedra (rampa STONE de ui_palette.py) y fondos.
pub const STONE0: Color32 = hex(0x1c2130);
pub const STONE1: Color32 = hex(0x343c50);
pub const STONE2: Color32 = hex(0x4f5a70);
pub const STONE3: Color32 = hex(0x717e96);
pub const STONE4: Color32 = hex(0x9eabbf);
pub const PANEL: Color32 = hex(0x2a3040);
pub const HOLE: Color32 = hex(0x242a38);
pub const DEEP: Color32 = hex(0x161a24);
pub const FAINT: Color32 = hex(0x2f3646);
pub const SHADOW: Color32 = hex(0x0d1018);

// Texto.
pub const TEXT: Color32 = hex(0xf2ead8);
pub const TEXT_SHADOW: Color32 = hex(0x141822);
/// Solo sobre fondos iguales o más oscuros que `FAINT` (contraste).
pub const TEXT_DIM: Color32 = hex(0x9aa4b8);
pub const TITLE: Color32 = hex(0xffd77a);

// Acentos.
pub const GOLD: Color32 = hex(0xf0c030);
pub const AMBER: Color32 = hex(0xe8a23a);
pub const SELECTION_BG: Color32 = hex(0x6a4410);
pub const CYAN: Color32 = hex(0x6ad4f0);
pub const FOCUS_RING: Color32 = hex(0xa8ecff);
pub const GREEN: Color32 = hex(0x7ad860);
pub const WARN: Color32 = hex(0xf8c860);
/// Texto de error (el rojo del juego `e8564a` no da contraste como texto).
pub const ERROR: Color32 = hex(0xf48c6d);
pub const RED: Color32 = hex(0xe8564a);
pub const DANGER_BG: Color32 = hex(0xa02830);

// Visor.
pub const VIEW_BG: Color32 = hex(0x303338);
pub const VIEW_CHECKER_A: Color32 = hex(0x33373f);
pub const VIEW_CHECKER_B: Color32 = hex(0x373b44);

/// Capas del visor (§7): color principal y contorno en su propio tono oscuro.
pub mod overlay {
    use super::{Color32, hex};

    pub const GRID: Color32 = hex(0x0d1018); // 35 %, marcada 55 %, sub-cubo 12 %
    pub const NAV_FILL: Color32 = hex(0x6ad4f0); // 30 % (otras plantas 10 %)
    pub const NAV_EDGE: Color32 = hex(0x2a82a8);
    pub const BLOCKED: Color32 = hex(0xf07a5a);
    pub const BLOCKED_OUTLINE: Color32 = hex(0x5a1420);
    pub const PART: Color32 = hex(0xf07a2a);
    pub const PART_OUTLINE: Color32 = hex(0x6a2410);
    pub const PIN_PLATE: Color32 = hex(0xf2ead8);
    pub const PIN_GLYPH: Color32 = hex(0x1c2130);
    pub const PIN_BORDER: Color32 = hex(0x3e2414);
    pub const PIN_DOT: Color32 = hex(0xf0c030);
    pub const ENTRANCE: Color32 = hex(0xf8c860);
    pub const ENTRANCE_OUTLINE: Color32 = hex(0x5a3010);
    pub const FOOTPRINT: Color32 = hex(0xffe070); // relleno 15 %
    pub const DETAIL_HATCH: Color32 = hex(0xf2ead8); // 35 %
    pub const BANDS: Color32 = hex(0xf2ead8); // 40 %
    pub const CUT: Color32 = hex(0xf0c030);
    pub const HOVER: Color32 = hex(0xf2ead8);
    pub const SELECTION: Color32 = hex(0xf0c030);
    pub const SELECTION_ALT: Color32 = hex(0x1c2130);
    pub const ERASE: Color32 = hex(0xe8564a);
    pub const OCCLUDED_ALPHA: f32 = 0.25;
}

/// Fuentes de la app (§3), dentro del binario: Atkinson Hyperlegible Next (texto), SemiBold (títulos) y Mono
/// (números), con los iconos Phosphor detrás de cada familia.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    let mut put = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), egui::FontData::from_static(bytes).into());
    };
    put("atkinson", include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-Regular.ttf"));
    put("atkinson-semibold", include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-SemiBold.ttf"));
    put("atkinson-mono", include_bytes!("../assets/fonts/AtkinsonHyperlegibleMono-Regular.ttf"));
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    let with = |first: &str| std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, with("atkinson"));
    fonts.families.insert(FontFamily::Monospace, with("atkinson-mono"));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), with("atkinson-semibold"));
    ctx.set_fonts(fonts);
}

/// Familia de los títulos de sección y diálogo.
pub const SEMIBOLD: &str = "semibold";

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Aplica tokens, tamaños y espaciado (§2–§3). Solo tema oscuro: se fija aunque el sistema esté en claro.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Heading, semibold(18.0)),
            (TextStyle::Monospace, mono(13.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(6.0, 5.0);
        s.button_padding = egui::vec2(8.0, 4.0);
        s.interact_size.y = 24.0;
        s.indent = 14.0;
        s.window_margin = Margin::same(10);
        s.menu_margin = Margin::same(6);
        s.tooltip_width = 280.0;
        style.interaction.tooltip_delay = 0.4;
        s.scroll = egui::style::ScrollStyle::solid();
        s.scroll.bar_width = 8.0;
        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, STONE2);
    v.extreme_bg_color = DEEP;
    v.code_bg_color = HOLE;
    v.faint_bg_color = FAINT;
    v.hyperlink_color = CYAN;
    v.warn_fg_color = WARN;
    v.error_fg_color = ERROR;
    v.selection.bg_fill = SELECTION_BG;
    v.selection.stroke = Stroke::new(1.5, GOLD);
    v.text_cursor.stroke = Stroke::new(2.0, GOLD);
    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(3);
    v.window_shadow = Shadow { offset: [3, 3], blur: 0, spread: 0, color: alpha(SHADOW, 0.7) };
    v.popup_shadow = Shadow { offset: [2, 2], blur: 0, spread: 0, color: alpha(SHADOW, 0.7) };
    v.override_text_color = None;
    let w = &mut v.widgets;
    let radius = CornerRadius::same(2);
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, STONE1);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    for (state, bg, border) in [
        (&mut w.inactive, STONE1, STONE0),
        (&mut w.hovered, STONE2, STONE3),
        (&mut w.active, HOLE, GOLD),
        (&mut w.open, STONE1, STONE3),
    ] {
        state.bg_fill = bg;
        state.weak_bg_fill = bg;
        state.bg_stroke = Stroke::new(1.0, border);
        state.fg_stroke = Stroke::new(1.0, TEXT);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.noninteractive.corner_radius = radius;
    v
}

/// Canto tallado (§2): en relieve, claro arriba-izquierda y oscuro abajo-derecha; hundido, al revés.
pub fn bevel(painter: &Painter, rect: Rect, raised: bool) {
    let (light, dark) = if raised { (STONE3, STONE0) } else { (DEEP, STONE2) };
    let r = rect.shrink(0.5);
    let (tl, tr, bl, br) = (r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom());
    painter.line_segment([tl, tr], Stroke::new(1.0, light));
    painter.line_segment([tl, bl], Stroke::new(1.0, light));
    painter.line_segment([bl, br], Stroke::new(1.0, dark));
    painter.line_segment([tr, br + Pos2::new(0.0, 0.5).to_vec2()], Stroke::new(1.0, dark));
}
