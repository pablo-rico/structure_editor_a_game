//! El sprite dibujado a mano de un objeto como fuente de un modelo: su perfil (para revolverlo) y su dibujo (para
//! forrar el modelo con él). En el juego un vóxel mide más o menos un píxel de arte, así que el detalle que se ve es
//! el que cabe en el sprite: en vez de imitarlo con formas, se proyecta.
//!
//! La cámara del juego (ortográfica, 45° de giro, 30° de elevación) lleva un punto del modelo al píxel
//! `(x − z, y + (x + z) / 2)` respecto al eje. La cámara orbita: el modelo se parte en cuatro cuartos, uno por cada
//! diagonal desde la que se puede mirar de frente, y cada cara se pinta con el píxel del sprite que le cae encima
//! vista desde la diagonal de su cuarto. Desde cualquiera de las cuatro, el centro del modelo (el 70 % de su ancho) es
//! el centro del sprite, píxel a píxel; las costuras quedan hacia los bordes, donde las caras se ven de canto, y van
//! en zigzag por altura para que no dibujen una raya.

use std::path::Path;

use anyhow::{Context, Result};

use crate::bridge::proto::Rgb;
use crate::model::Structure;
use crate::vmodel::{self, DIRS};

pub struct Sprite {
    width: i32,
    height: i32,
    /// Color de cada píxel opaco.
    pixels: Vec<Option<Rgb>>,
    /// Píxeles opacos que no tocan la transparencia: sin el contorno que el sprite trae dibujado por fuera.
    inner: Vec<bool>,
}

impl Sprite {
    pub fn load(path: &Path) -> Result<Self> {
        let img = image::open(path)
            .with_context(|| format!("no puedo leer el sprite {}", path.display()))?
            .to_rgba8();
        let (width, height) = (img.width() as i32, img.height() as i32);
        let pixels: Vec<Option<Rgb>> = img
            .pixels()
            .map(|p| (p[3] >= 128).then_some([p[0], p[1], p[2]]))
            .collect();
        let at = |x: i32, y: i32| {
            x >= 0
                && y >= 0
                && x < width
                && y < height
                && pixels[(y * width + x) as usize].is_some()
        };
        let inner = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| at(x, y) && at(x - 1, y) && at(x + 1, y) && at(x, y - 1) && at(x, y + 1))
            .collect();
        Ok(Self {
            width,
            height,
            pixels,
            inner,
        })
    }

    fn inner_at(&self, col: i32, row: i32) -> Option<Rgb> {
        let inside = col >= 0 && row >= 0 && col < self.width && row < self.height;
        if inside && self.inner[(row * self.width + col) as usize] {
            self.pixels[(row * self.width + col) as usize]
        } else {
            None
        }
    }

    /// Radio (en vóxeles) de la capa `y` de un sólido de revolución con la silueta del sprite: la media anchura de su
    /// fila, sin el contorno, entre √2 (un vóxel en planta mide √2 píxeles de ancho en pantalla). `axis` es la columna
    /// del eje (16 = entre los píxeles 15 y 16) y `ground` la fila del sprite donde y = 0.
    pub fn radius(&self, axis: f32, ground: i32, y: i32) -> f32 {
        let row = ground - y;
        (0..self.width)
            .filter(|&col| self.inner_at(col, row).is_some())
            .map(|col| (col as f32 + 0.5 - axis).abs() + 0.5)
            .fold(0.0, f32::max)
            / std::f32::consts::SQRT_2
    }

    /// Píxel del sprite a `side` píxeles a la derecha del eje (negativo, a la izquierda) y `up` píxeles sobre el
    /// suelo. Si cae fuera del dibujo o en su contorno, el de dentro más cercano de esa fila hacia el eje.
    fn sample(&self, axis: f32, ground: i32, side: f32, up: f32) -> Option<Rgb> {
        let row = ground - up.floor() as i32;
        let col = (axis + side).floor() as i32;
        let center = axis.floor() as i32;
        if col <= center {
            (col..=center).find_map(|c| self.inner_at(c, row))
        } else {
            (center..=col).rev().find_map(|c| self.inner_at(c, row))
        }
    }
}

/// Un forrado pendiente: se aplica al acabar la receta, sobre las caras ya sombreadas.
pub struct Wrap {
    pub sprite: Sprite,
    pub axis: f32,
    pub ground: i32,
    /// Caja semiabierta de vóxeles a los que afecta.
    pub bounds: [i32; 6],
}

/// Color del sprite para cada cara expuesta de la caja. El eje del modelo es el centro de su rejilla en planta.
pub fn wrap(s: &Structure, w: &Wrap) -> Vec<([i32; 3], u8, Rgb)> {
    let [sx, _, sz] = s.grid.size;
    let center = [sx as f32 / 2.0, sz as f32 / 2.0];
    let b = w.bounds;
    vmodel::exposed(&s.grid)
        .into_iter()
        .filter(|(v, _)| (0..3).all(|i| v[i] >= b[i] && v[i] < b[i + 3]))
        .filter_map(|(v, d)| {
            let n = DIRS[d as usize];
            // Centro de la cara, respecto al eje.
            let p = [
                v[0] as f32 + 0.5 + n[0] as f32 * 0.5 - center[0],
                v[1] as f32 + 0.5 + n[1] as f32 * 0.5,
                v[2] as f32 + 0.5 + n[2] as f32 * 0.5 - center[1],
            ];
            // Su cuarto: la diagonal hacia la que más sobresale (la cámara que la ve de frente mira hacia −esa),
            // con un vaivén por altura en la costura.
            let (fx, fz) = [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
                .into_iter()
                .enumerate()
                .map(|(k, f)| (f, -(p[0] * f.0 + p[2] * f.1) + seam_jitter(k as u32, v[1])))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(f, _)| f)
                .unwrap_or((1.0, 1.0));
            // A la derecha de esa cámara y hacia su fondo.
            let side = p[0] * fz - p[2] * fx;
            let up = p[1] + (p[0] * fx + p[2] * fz) / 2.0;
            w.sprite
                .sample(w.axis, w.ground, side, up)
                .map(|c| (v, d, c))
        })
        .collect()
}

/// Cuánto se corre la costura de un cuarto (en vóxeles) a la altura `y`: fijo, a tramos de dos capas.
fn seam_jitter(quarter: u32, y: i32) -> f32 {
    let h = crate::edit::ops::noise(
        quarter.wrapping_mul(7919).wrapping_add(17),
        [0, y.div_euclid(2), 0],
    ) % 100;
    (h as f32 / 99.0 - 0.5) * 2.4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VoxelGrid;

    /// Un sprite de 8 × 6: un rectángulo de 6 × 4 con contorno negro, mitad izquierda roja y derecha azul.
    fn sprite() -> Sprite {
        let (width, height) = (8, 6);
        let mut pixels = vec![None; 48];
        for y in 0..6 {
            for x in 0..8 {
                let border = x == 0 || x == 7 || y == 0 || y == 5;
                pixels[y * 8 + x] = Some(if border {
                    [0, 0, 0]
                } else if x < 4 {
                    [200, 0, 0]
                } else {
                    [0, 0, 200]
                });
            }
        }
        let at = |x: i32, y: i32| x >= 0 && y >= 0 && x < width && y < height;
        let inner = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| at(x - 1, y) && at(x + 1, y) && at(x, y - 1) && at(x, y + 1))
            .collect();
        Sprite {
            width,
            height,
            pixels,
            inner,
        }
    }

    #[test]
    fn perfil_sin_el_contorno() {
        let s = sprite();
        // Filas 1..4 por dentro: media anchura 3 píxeles → 3/√2 vóxeles; la del contorno, nada.
        assert!((s.radius(4.0, 4, 0) - 3.0 / std::f32::consts::SQRT_2).abs() < 1e-4);
        assert_eq!(s.radius(4.0, 4, 4), 0.0);
        assert_eq!(s.radius(4.0, 4, 9), 0.0);
    }

    #[test]
    fn forra_cada_cuarto_con_el_dibujo_visto_de_frente() {
        let mut st = Structure {
            grid: VoxelGrid::new([8, 8, 8]),
            model_cell: Some(16),
            ..Default::default()
        };
        for y in 0..8 {
            for z in 0..8 {
                for x in 0..8 {
                    st.grid.set(x, y, z, 1);
                }
            }
        }
        let w = Wrap {
            sprite: sprite(),
            axis: 4.0,
            ground: 4,
            bounds: [0, 0, 0, 8, 8, 8],
        };
        let faces = wrap(&st, &w);
        assert!(!faces.is_empty());
        // Nunca el contorno: rojo o azul.
        assert!(
            faces
                .iter()
                .all(|f| f.2 == [200, 0, 0] || f.2 == [0, 0, 200]),
            "{faces:?}"
        );
        let color = |v: [i32; 3], d: u8| faces.iter().find(|f| f.0 == v && f.1 == d).map(|f| f.2);
        // Una caja grande para que el vaivén de la costura no llegue a las esquinas, y a media altura para que el
        // fondo no saque el píxel del dibujo: en la esquina del frente
        // (cuarto de la cámara que mira hacia +x, +z), la cara −x queda a su izquierda (rojo) y la −z a su derecha (azul).
        assert_eq!(color([0, 5, 0], 1), Some([200, 0, 0]));
        assert_eq!(color([0, 5, 0], 5), Some([0, 0, 200]));
        // En la esquina de atrás pasa lo mismo desde su propia cámara: la +x a su izquierda y la +z a su derecha.
        assert_eq!(color([7, 5, 7], 0), Some([200, 0, 0]));
        assert_eq!(color([7, 5, 7], 4), Some([0, 0, 200]));
        // Fuera de la caja no se toca nada.
        let part = wrap(
            &st,
            &Wrap {
                bounds: [0, 5, 0, 8, 6, 8],
                ..w
            },
        );
        assert!(part.iter().all(|f| f.0[1] == 5) && part.len() < faces.len());
    }
}
