//! Vista «como en el juego» de un modelo: a su tamaño real en pantalla (un vóxel ≈ un píxel de arte) y con la luz lisa
//! que el juego da a los árboles de vóxeles (ni tapas claras ni laterales oscuros: solo el color pintado), con el
//! contorno casi negro por fuera. La lámina de `render` enseña cada vóxel al doble y con luz cara a cara: sirve para
//! medir, no para juzgar cómo se verá.
//!
//! Proyección de la cámara del juego (ortográfica, 45° de giro y 30° de elevación) en píxeles de arte, con el vóxel
//! tan alto como lo hace el juego (`LevelHeight3D` / 16): `x_pantalla = x − z`, `y_pantalla = y + (x + z) / 2`. Se
//! dibuja en CPU, un rayo por píxel: no hace falta tarjeta gráfica.

use image::{Rgba, RgbaImage};

use crate::mesh::Paint;
use crate::model::{Structure, face_key};

const BACKGROUND: Rgba<u8> = Rgba([0x6b, 0x7a, 0x3a, 255]);
const OUTLINE: Rgba<u8> = Rgba([0x14, 0x10, 0x18, 255]);
/// Paso del rayo, en unidades de fondo (x + z): fino para no saltarse la arista por la que entra.
const STEP: f32 = 0.05;

/// El modelo visto con la cámara girada `turns` cuartos de vuelta, a un píxel por vóxel.
pub fn shot(s: &Structure, paint: &Paint, turns: u8, outline: bool) -> RgbaImage {
    let g = &s.grid;
    let [gx, gy, gz] = g.size;
    // Tamaño visto y, por giro: de coordenadas de vista a las de la rejilla, y qué cara de la rejilla es la −x y la −z
    // de la vista (las dos laterales que la cámara ve).
    let (vw, vd) = if turns.is_multiple_of(2) {
        (gx, gz)
    } else {
        (gz, gx)
    };
    let to_grid = |x: i32, z: i32| match turns % 4 {
        0 => (x, z),
        1 => (z, gz - 1 - x),
        2 => (gx - 1 - x, gz - 1 - z),
        _ => (gx - 1 - z, x),
    };
    let (face_x, face_z) = [(1u8, 5u8), (4, 1), (0, 4), (5, 0)][(turns % 4) as usize];

    let width = (vw + vd + 2) as u32;
    let height = (gy + (vw + vd + 1) / 2 + 2) as u32;
    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    for row in 0..height {
        for col in 0..width {
            let a = col as f32 - 1.0 - vd as f32 + 0.5;
            let h = (height - 1 - row) as f32 - 1.0 + 0.5;
            let mut prev = [i32::MIN; 3];
            let mut depth = a.abs().max(0.0);
            let end = (vw + vd) as f32;
            while depth <= end {
                let p = [
                    ((depth + a) * 0.5).floor() as i32,
                    (h - depth * 0.5).floor() as i32,
                    ((depth - a) * 0.5).floor() as i32,
                ];
                if p != prev {
                    let (x, z) = to_grid(p[0], p[2]);
                    let inside = (0..vw).contains(&p[0]) && (0..vd).contains(&p[2]);
                    let byte = if inside { g.get(x, p[1], z) } else { 0 };
                    if byte != 0 {
                        // Por dónde ha entrado el rayo: baja, y avanza en +x y +z de la vista.
                        let dir = if prev[1] != p[1] && prev[1] != i32::MIN {
                            2
                        } else if prev[0] != p[0] {
                            face_x
                        } else {
                            face_z
                        };
                        let color = s
                            .face_colors
                            .get(&face_key([x, p[1], z], dir))
                            .copied()
                            .unwrap_or(byte);
                        let c = paint
                            .palette
                            .get(color as usize)
                            .copied()
                            .unwrap_or([255, 0, 255]);
                        img.put_pixel(col, row, Rgba([c[0], c[1], c[2], 255]));
                        break;
                    }
                    prev = p;
                }
                depth += STEP;
            }
        }
    }
    if outline {
        let solid = img.clone();
        let filled = |x: i64, y: i64| {
            x >= 0
                && y >= 0
                && x < width as i64
                && y < height as i64
                && solid.get_pixel(x as u32, y as u32)[3] != 0
        };
        for row in 0..height as i64 {
            for col in 0..width as i64 {
                if !filled(col, row)
                    && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .any(|d| filled(col + d.0, row + d.1))
                {
                    img.put_pixel(col as u32, row as u32, OUTLINE);
                }
            }
        }
    }
    img
}

/// Las vistas pedidas, una al lado de otra sobre un fondo de hierba y ampliadas `zoom` veces sin suavizar.
pub fn sheet(s: &Structure, paint: &Paint, turns: &[u8], zoom: u32, outline: bool) -> RgbaImage {
    let shots: Vec<RgbaImage> = turns.iter().map(|&t| shot(s, paint, t, outline)).collect();
    let gap = 3;
    let width = shots.iter().map(|i| i.width() + gap).sum::<u32>() + gap;
    let height = shots.iter().map(|i| i.height()).max().unwrap_or(1) + 2 * gap;
    let mut out = RgbaImage::from_pixel(width, height, BACKGROUND);
    let mut x = gap;
    for shot in &shots {
        image::imageops::overlay(
            &mut out,
            shot,
            x as i64,
            (height - gap - shot.height()) as i64,
        );
        x += shot.width() + gap;
    }
    image::imageops::resize(
        &out,
        width * zoom,
        height * zoom,
        image::imageops::FilterType::Nearest,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VoxelGrid;

    #[test]
    fn un_cubo_ensena_tapa_y_dos_laterales() {
        let mut s = Structure {
            grid: VoxelGrid::new([4, 4, 4]),
            model_cell: Some(16),
            ..Default::default()
        };
        for y in 0..4 {
            for z in 0..4 {
                for x in 0..4 {
                    s.grid.set(x, y, z, 1);
                }
            }
        }
        // La tapa en otro color, y la cara −x de un vóxel de la esquina del frente.
        for z in 0..4 {
            for x in 0..4 {
                s.face_colors.insert(face_key([x, 3, z], 2), 2);
            }
        }
        let paint = Paint {
            palette: vec![[0, 0, 0], [10, 20, 30], [200, 210, 220]],
            hidden: None,
            flat: true,
        };
        let img = shot(&s, &paint, 0, true);
        let count = |c: [u8; 3]| {
            img.pixels()
                .filter(|p| p.0 == [c[0], c[1], c[2], 255])
                .count()
        };
        assert!(count([200, 210, 220]) > 6, "se ve la tapa");
        assert!(count([10, 20, 30]) > 10, "se ven los laterales");
        assert!(
            count([OUTLINE[0], OUTLINE[1], OUTLINE[2]]) > 10,
            "lleva contorno"
        );
        // Simétrico: las cuatro vistas de un cubo son iguales.
        assert_eq!(shot(&s, &paint, 1, true), img);
        assert_eq!(shot(&s, &paint, 3, false).dimensions(), img.dimensions());
    }
}
