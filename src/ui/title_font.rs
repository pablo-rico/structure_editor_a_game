//! `font_title` del juego (fuente bitmap grabada, §3): solo para la marca «Estructuras» y los títulos de los estados
//! vacíos y del primer arranque. Se lee de a_game; si no está, quien llama usa SemiBold.

use std::collections::HashMap;
use std::path::Path;

use eframe::egui::{Color32, ColorImage};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    cell_height: u32,
    space_advance: u32,
    glyphs: HashMap<String, Glyph>,
}

#[derive(Deserialize, Clone, Copy)]
struct Glyph {
    x: u32,
    y: u32,
    w: u32,
    advance: u32,
}

pub struct TitleFont {
    atlas: image::RgbaImage,
    m: Metrics,
}

impl TitleFont {
    pub fn load(agame_root: &Path) -> Option<Self> {
        let dir = agame_root.join("Assets/_Game/Art/UI/Fonts");
        let m: Metrics = serde_json::from_slice(&std::fs::read(dir.join("font_title.json")).ok()?).ok()?;
        // Con Git LFS sin descargar, el PNG es un puntero de texto: image falla y se usa la fuente normal.
        let atlas = image::open(dir.join("font_title.png")).ok()?.to_rgba8();
        Some(Self { atlas, m })
    }

    /// Texto a escala entera, teñido de `color` con sombra de 1 píxel de arte abajo-derecha.
    pub fn render(&self, text: &str, color: Color32, shadow: Color32, scale: u32) -> ColorImage {
        let glyph = |c: char| self.m.glyphs.get(c.encode_utf8(&mut [0; 4]) as &str).copied();
        let width: u32 = text.chars().map(|c| glyph(c).map_or(self.m.space_advance, |g| g.advance)).sum::<u32>() + 1;
        let (w, h) = (width * scale, (self.m.cell_height + 1) * scale);
        let mut img = ColorImage::filled([w as usize, h as usize], Color32::TRANSPARENT);
        for (dx, dy, tint) in [(1, 1, shadow), (0, 0, color)] {
            let mut pen = 0;
            for c in text.chars() {
                let Some(g) = glyph(c) else {
                    pen += self.m.space_advance;
                    continue;
                };
                for gy in 0..self.m.cell_height {
                    for gx in 0..g.w {
                        let a = self.atlas.get_pixel(g.x + gx, g.y + gy);
                        if a[3] == 0 {
                            continue;
                        }
                        // La fuente es blanca con relieve en grises: se conserva el relieve multiplicando.
                        let k = a[0] as f32 / 255.0;
                        let c = Color32::from_rgb(
                            (tint.r() as f32 * k) as u8,
                            (tint.g() as f32 * k) as u8,
                            (tint.b() as f32 * k) as u8,
                        );
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = ((pen + gx + dx) * scale + sx) as usize;
                                let py = ((gy + dy) * scale + sy) as usize;
                                img[(px, py)] = c;
                            }
                        }
                    }
                }
                pen += g.advance;
            }
        }
        img
    }
}
