//! Lámina de vistas para revisar una estructura o un modelo sin ventana (`cantero render`): varias vistas con nombre
//! en un PNG, con reglas de coordenadas en sub-cubos en los alzados y la planta. Las letras son un mapa de bits 5×7
//! propio: aquí no hay `egui::Context` que pinte texto.

use eframe::egui::Color32;
use eframe::egui_wgpu;
use image::{Rgba, RgbaImage};

use super::{Layers, Offscreen, Projection, Shot, ViewState};
use crate::mesh::Paint;
use crate::model::Structure;
use crate::theme;

/// Vistas con nombre. El frente de una estructura es −Z (por donde se entra).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    /// Isométrica del juego desde el frente (se ven las caras −X y −Z).
    Iso,
    /// Isométrica desde detrás (caras +X y +Z).
    IsoBack,
    /// Alzado del frente: X a la derecha, Y arriba.
    Front,
    /// Alzado desde +X: Z a la derecha, Y arriba.
    Side,
    /// Planta desde arriba: X a la derecha, Z arriba (el frente queda abajo).
    Top,
}

impl ViewKind {
    pub const ALL: [ViewKind; 5] = [ViewKind::Iso, ViewKind::IsoBack, ViewKind::Front, ViewKind::Side, ViewKind::Top];

    /// Nombre en la línea de comandos y en el título de la vista.
    pub fn name(self) -> &'static str {
        match self {
            ViewKind::Iso => "iso",
            ViewKind::IsoBack => "iso-atras",
            ViewKind::Front => "frente",
            ViewKind::Side => "lado",
            ViewKind::Top => "planta",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == name)
    }

    fn angles(self) -> (f32, f32) {
        match self {
            ViewKind::Iso => (45.0, 30.0),
            ViewKind::IsoBack => (225.0, 30.0),
            ViewKind::Front => (0.0, 0.0),
            ViewKind::Side => (270.0, 0.0),
            ViewKind::Top => (0.0, 90.0),
        }
    }

    /// Ejes de las reglas (horizontal, vertical) como índices de `[x, y, z]`; las isométricas no llevan.
    fn rulers(self) -> Option<(usize, usize)> {
        match self {
            ViewKind::Front => Some((0, 1)),
            ViewKind::Side => Some((2, 1)),
            ViewKind::Top => Some((0, 2)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub views: Vec<ViewKind>,
    /// Tamaño de la lámina entera.
    pub size: [u32; 2],
    /// Encuadrar solo esta caja `[x0, y0, z0, x1, y1, z1)`.
    pub region: Option<[i32; 6]>,
    /// Caja resaltada en rojo.
    pub mark: Option<[i32; 6]>,
    /// Corte por planta: nivel del suelo de la planta.
    pub cut_level: Option<i32>,
    pub layers: Layers,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            views: vec![ViewKind::Iso, ViewKind::IsoBack, ViewKind::Front, ViewKind::Top],
            size: [1400, 1050],
            region: None,
            mark: None,
            cut_level: None,
            layers: Layers::none(),
        }
    }
}

/// Altura del título, margen de la regla vertical y de la horizontal, en píxeles.
const TITLE: u32 = 20;
const LEFT: u32 = 44;
const BOTTOM: u32 = 26;
/// Tamaño de la letra: píxeles por punto del mapa de bits.
const DOT: u32 = 2;

pub fn render(rs: &egui_wgpu::RenderState, s: &Structure, paint: &Paint, o: &Options) -> RgbaImage {
    let n = o.views.len().max(1) as u32;
    let cols = match n {
        1 => 1,
        2..=4 => 2,
        _ => 3,
    };
    let rows = n.div_ceil(cols);
    let (tw, th) = ((o.size[0] / cols).max(LEFT + 64), (o.size[1] / rows).max(TITLE + BOTTOM + 64));
    let mut sheet = RgbaImage::from_pixel(tw * cols, th * rows, px(theme::DEEP));
    let mut off = Offscreen::new(rs, s, paint);
    off.fill = true;
    for (i, &kind) in o.views.iter().enumerate() {
        let (yaw_deg, elevation_deg) = kind.angles();
        let view = ViewState {
            yaw_deg,
            elevation_deg,
            projection: Projection::Iso,
            cut_level: o.cut_level,
            layers: o.layers,
            ghost_upper: false,
            highlight: o.mark,
            fit: true,
            ..Default::default()
        };
        let size = [tw - LEFT - 2, th - TITLE - BOTTOM];
        let shot = off.shot(&view, size, o.region);
        let origin = [i as u32 % cols * tw + LEFT, i as u32 / cols * th + TITLE];
        image::imageops::replace(&mut sheet, &shot.image, origin[0] as i64, origin[1] as i64);
        let title = match kind.rulers() {
            Some((h, v)) => format!("{}  {}/{}", kind.name(), AXES[h], AXES[v]),
            None => kind.name().to_string(),
        };
        text(&mut sheet, [origin[0] as i32, origin[1] as i32 - TITLE as i32 + 3], &title.to_uppercase(), theme::TEXT);
        match kind.rulers() {
            Some(axes) => rulers(&mut sheet, &shot, s, origin, size, axes),
            None => corners(&mut sheet, &shot, s, origin, size),
        }
    }
    sheet
}

const AXES: [&str; 3] = ["X", "Y", "Z"];

fn px(c: Color32) -> Rgba<u8> {
    Rgba([c.r(), c.g(), c.b(), 255])
}

/// Paso entre marcas: la menor potencia de dos de sub-cubos que deja `room` píxeles entre una y la siguiente.
fn tick_step(px_per_sub: f32, room: f32) -> i32 {
    let mut step = 1;
    while step as f32 * px_per_sub < room && step < 256 {
        step *= 2;
    }
    step
}

/// Reglas de un alzado o de la planta: marcas y cifras en sub-cubos en los márgenes y guías tenues sobre la imagen.
fn rulers(img: &mut RgbaImage, shot: &Shot, s: &Structure, origin: [u32; 2], size: [u32; 2], (h, v): (usize, usize)) {
    let at = |axis: usize, value: i32| {
        let mut p = [0.0; 3];
        p[axis] = value as f32;
        shot.project(p)
    };
    let (ox, oy) = (origin[0] as i32, origin[1] as i32);
    let (w, hgt) = (size[0] as i32, size[1] as i32);
    for (axis, horizontal) in [(h, true), (v, false)] {
        let (Some(a), Some(b)) = (at(axis, 0), at(axis, 1)) else { continue };
        let k = if horizontal { 0 } else { 1 };
        let per_sub = (b[k] - a[k]).abs();
        let digits = s.grid.size[axis].to_string().len() as f32;
        let room = if horizontal { (digits * 6.0 + 3.0) * DOT as f32 } else { 9.0 * DOT as f32 };
        let step = tick_step(per_sub, room);
        let minor = tick_step(per_sub, 4.0);
        let mut value = 0;
        while value <= s.grid.size[axis] {
            if let Some(p) = at(axis, value) {
                let c = p[k].round() as i32;
                let labelled = value % step == 0;
                let inside = c >= 0 && c < if horizontal { w } else { hgt };
                if inside {
                    let len = if labelled { 6 } else { 3 };
                    if horizontal {
                        line(img, [ox + c, oy + hgt], [ox + c, oy + hgt + len], theme::TEXT_DIM, 1.0);
                        if labelled {
                            line(img, [ox + c, oy], [ox + c, oy + hgt - 1], theme::TEXT, 0.10);
                            let label = value.to_string();
                            let half = label.len() as i32 * 3 * DOT as i32;
                            text(img, [ox + c - half, oy + hgt + 7], &label, theme::TEXT_DIM);
                        }
                    } else {
                        line(img, [ox - len, oy + c], [ox - 1, oy + c], theme::TEXT_DIM, 1.0);
                        if labelled {
                            line(img, [ox, oy + c], [ox + w - 1, oy + c], theme::TEXT, 0.10);
                            let label = value.to_string();
                            let width = label.len() as i32 * 6 * DOT as i32;
                            let top = (c - 7).clamp(0, hgt - 7 * DOT as i32);
                            text(img, [ox - 8 - width, oy + top], &label, theme::TEXT_DIM);
                        }
                    }
                }
            }
            value += minor;
        }
    }
}

/// Isométricas: el origen y el extremo de cada eje, para orientarse.
fn corners(img: &mut RgbaImage, shot: &Shot, s: &Structure, origin: [u32; 2], size: [u32; 2]) {
    // Solo las esquinas del suelo: la del alto caería encima de lo dibujado.
    let [sx, _, sz] = s.grid.size;
    let marks = [
        ([0, 0, 0], "0".to_string()),
        ([sx, 0, 0], format!("X{sx}")),
        ([0, 0, sz], format!("Z{sz}")),
    ];
    for (p, label) in marks {
        let Some(at) = shot.project(p.map(|v| v as f32)) else { continue };
        let width = label.len() as i32 * 6 * DOT as i32;
        let x = (at[0].round() as i32 - width / 2).clamp(0, size[0] as i32 - width);
        let y = (at[1].round() as i32 + 3).clamp(0, size[1] as i32 - 8 * DOT as i32);
        let at = [origin[0] as i32 + x, origin[1] as i32 + y];
        text(img, [at[0] + 1, at[1] + 1], &label, theme::TEXT_SHADOW);
        text(img, at, &label, theme::TEXT);
    }
}

fn blend(img: &mut RgbaImage, x: i32, y: i32, c: Color32, alpha: f32) {
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return;
    }
    let p = img.get_pixel_mut(x as u32, y as u32);
    for (dst, src) in p.0.iter_mut().zip([c.r(), c.g(), c.b()]) {
        *dst = (*dst as f32 + (src as f32 - *dst as f32) * alpha).round() as u8;
    }
}

/// Línea horizontal o vertical entre dos puntos, ambos incluidos.
fn line(img: &mut RgbaImage, a: [i32; 2], b: [i32; 2], c: Color32, alpha: f32) {
    for x in a[0].min(b[0])..=a[0].max(b[0]) {
        for y in a[1].min(b[1])..=a[1].max(b[1]) {
            blend(img, x, y, c, alpha);
        }
    }
}

/// Texto con la esquina superior izquierda en `at`.
fn text(img: &mut RgbaImage, at: [i32; 2], s: &str, c: Color32) {
    let d = DOT as i32;
    for (i, ch) in s.chars().enumerate() {
        let Some(rows) = glyph(ch) else { continue };
        for (y, row) in rows.iter().enumerate() {
            for x in 0..5 {
                if row >> (4 - x) & 1 != 0 {
                    for (dx, dy) in (0..d).flat_map(|dx| (0..d).map(move |dy| (dx, dy))) {
                        blend(img, at[0] + (i as i32 * 6 + x) * d + dx, at[1] + y as i32 * d + dy, c, 1.0);
                    }
                }
            }
        }
    }
}

/// Mapa de bits 5×7 de una letra (mayúsculas, cifras y unos pocos signos): una fila por byte, el bit 4 a la izquierda.
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_de_vista() {
        for v in ViewKind::ALL {
            assert_eq!(ViewKind::parse(v.name()), Some(v));
        }
        assert_eq!(ViewKind::parse("arriba"), None);
    }

    #[test]
    fn el_paso_deja_sitio_a_las_cifras() {
        assert_eq!(tick_step(10.0, 44.0), 8);
        assert_eq!(tick_step(2.0, 44.0), 32);
        assert_eq!(tick_step(50.0, 44.0), 1);
    }

    #[test]
    fn los_titulos_tienen_letra() {
        for v in ViewKind::ALL {
            assert!(v.name().to_uppercase().chars().all(|c| glyph(c).is_some()), "{}", v.name());
        }
    }
}
