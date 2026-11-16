//! Geometría de las capas y gizmos del visor (docs/diseno-gui.md §7) en el mundo, en primitivas de 1 píxel de arte.
//! Colores de `theme::overlay`; cada forma lleva en su máscara el tono oscuro de su contorno.

use eframe::egui::Color32;
use glam::Vec3;

use super::gpu::{OvVertex, Overlay, Prims};
use super::{Gizmo, Layers, ViewState};
use crate::mesh::{Paint, sub_to_local};
use crate::model::{SUB, Structure};
use crate::pick::{Cut, Hit};
use crate::theme::{self, overlay as ov};

const NO_MASK: [u8; 4] = [0; 4];
/// Largo de un trazo de las líneas a rayas, en sub-cubos.
const DASH: f32 = 2.0;
/// Más de esto y el pincel fantasma se dibuja como su caja.
const MAX_GHOST_CUBES: usize = 512;

fn rgba(c: Color32, a: f32) -> [u8; 4] {
    [
        c.r(),
        c.g(),
        c.b(),
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn mask(c: Color32) -> [u8; 4] {
    [c.r(), c.g(), c.b(), 255]
}

/// Punto de mundo de unas coordenadas de sub-cubo.
fn w(x: f32, y: f32, z: f32) -> Vec3 {
    sub_to_local(x, y, z)
}

impl Prims {
    fn vertex(pos: Vec3, color: [u8; 4], mask: [u8; 4]) -> OvVertex {
        OvVertex {
            pos: pos.into(),
            color,
            mask,
        }
    }

    fn line(&mut self, a: Vec3, b: Vec3, color: [u8; 4], mask: [u8; 4]) {
        self.lines
            .extend([Self::vertex(a, color, mask), Self::vertex(b, color, mask)]);
    }

    /// Línea a rayas de puntas en sub-cubos (`a`, `b`); `phase` desplaza los trazos (hormigas) y `alt` pinta los huecos.
    fn dashed(
        &mut self,
        a: Vec3,
        b: Vec3,
        color: [u8; 4],
        mask: [u8; 4],
        phase: f32,
        alt: Option<[u8; 4]>,
    ) {
        let n = ((b - a).length() / DASH).ceil().max(1.0) as i32;
        let shift = phase.rem_euclid(2.0) as i32;
        for i in 0..n {
            let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            let c = if (i + shift) % 2 == 0 {
                Some(color)
            } else {
                alt
            };
            if let Some(c) = c {
                self.line(w_of(a.lerp(b, t0)), w_of(a.lerp(b, t1)), c, mask);
            }
        }
    }

    fn quad(&mut self, p: [Vec3; 4], color: [u8; 4], mask: [u8; 4]) {
        for k in [0, 1, 2, 0, 2, 3] {
            self.tris.push(Self::vertex(p[k], color, mask));
        }
    }

    /// Las 12 aristas de la caja `[min, max]` en sub-cubos, a rayas si `dash`.
    fn wire_box(
        &mut self,
        min: Vec3,
        max: Vec3,
        color: [u8; 4],
        mask: [u8; 4],
        dash: Option<(f32, Option<[u8; 4]>)>,
    ) {
        let c = |i: usize| {
            Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            )
        };
        for (i, j) in [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ] {
            match dash {
                Some((phase, alt)) => self.dashed(c(i), c(j), color, mask, phase, alt),
                None => self.line(w_of(c(i)), w_of(c(j)), color, mask),
            }
        }
    }

    /// Las 6 caras de la caja `[min, max]` en sub-cubos.
    fn box_faces(&mut self, min: Vec3, max: Vec3, color: [u8; 4], mask: [u8; 4]) {
        let c = |x: bool, y: bool, z: bool| {
            w_of(Vec3::new(
                if x { max.x } else { min.x },
                if y { max.y } else { min.y },
                if z { max.z } else { min.z },
            ))
        };
        for s in [false, true] {
            self.quad(
                [
                    c(s, false, false),
                    c(s, true, false),
                    c(s, true, true),
                    c(s, false, true),
                ],
                color,
                mask,
            );
            self.quad(
                [
                    c(false, s, false),
                    c(true, s, false),
                    c(true, s, true),
                    c(false, s, true),
                ],
                color,
                mask,
            );
            self.quad(
                [
                    c(false, false, s),
                    c(true, false, s),
                    c(true, true, s),
                    c(false, true, s),
                ],
                color,
                mask,
            );
        }
    }

    /// Rectángulo a rayas alrededor de la rejilla a la altura `y` (sub-cubos), separado `pad` sub-cubos.
    fn dashed_rect(&mut self, s: &Structure, y: f32, pad: f32, color: [u8; 4]) {
        let [sx, _, sz] = s.grid.size.map(|v| v as f32);
        let p = [
            Vec3::new(-pad, y, -pad),
            Vec3::new(sx + pad, y, -pad),
            Vec3::new(sx + pad, y, sz + pad),
            Vec3::new(-pad, y, sz + pad),
        ];
        for i in 0..4 {
            self.dashed(p[i], p[(i + 1) % 4], color, NO_MASK, 0.0, None);
        }
    }
}

fn w_of(p: Vec3) -> Vec3 {
    w(p.x, p.y, p.z)
}

fn dir_of(name: &str) -> (i32, i32) {
    match name {
        "+X" => (1, 0),
        "-X" => (-1, 0),
        "+Z" => (0, 1),
        _ => (0, -1),
    }
}

/// Lo que hace falta para dibujar las capas de una imagen.
pub(crate) struct Inputs<'a> {
    pub structure: &'a Structure,
    pub paint: &'a Paint,
    pub view: &'a ViewState,
    pub cut: Option<&'a Cut>,
    pub hover: Option<Hit>,
    /// Caja de «Localizar» en su fase encendida.
    pub highlight: Option<[i32; 6]>,
    /// Segundos para las hormigas de la selección.
    pub time: f64,
}

pub(crate) fn build(i: &Inputs) -> Overlay {
    let mut o = Overlay::default();
    let s = i.structure;
    let layers: &Layers = &i.view.layers;
    let ground = (s.ground_level * SUB) as f32;
    let [cells_x, cells_z] = s.grid.cells();
    let cell = SUB as f32;

    if layers.grid {
        // Por celda en el suelo de fuera, más marcadas cada 4, con dos celdas de margen. La celda de un modelo es la
        // suya (`cell_size`) y su tamaño no tiene por qué ser múltiplo: se redondea hacia arriba.
        let m = 2;
        let cs = s.cell_size();
        let (gx, gz) = ((s.grid.size[0] + cs - 1) / cs, (s.grid.size[2] + cs - 1) / cs);
        for k in -m..=gx + m {
            let a = if k.rem_euclid(4) == 0 { 0.55 } else { 0.35 };
            let x = (k * cs) as f32;
            o.plain.line(
                w(x, ground, (-m * cs) as f32),
                w(x, ground, ((gz + m) * cs) as f32),
                rgba(ov::GRID, a),
                NO_MASK,
            );
        }
        for k in -m..=gz + m {
            let a = if k.rem_euclid(4) == 0 { 0.55 } else { 0.35 };
            let z = (k * cs) as f32;
            o.plain.line(
                w((-m * cs) as f32, ground, z),
                w(((gx + m) * cs) as f32, ground, z),
                rgba(ov::GRID, a),
                NO_MASK,
            );
        }
    }

    if layers.footprint {
        o.xray.dashed_rect(s, ground, 0.0, rgba(ov::FOOTPRINT, 1.0));
        for cz in 0..cells_z {
            for cx in 0..cells_x {
                let covered = s
                    .footprint
                    .as_ref()
                    .is_none_or(|f| f.get((cz * cells_x + cx) as usize) == Some(&1));
                let (x0, z0) = ((cx * SUB) as f32, (cz * SUB) as f32);
                if covered {
                    let p = [
                        w(x0, ground, z0),
                        w(x0 + cell, ground, z0),
                        w(x0 + cell, ground, z0 + cell),
                        w(x0, ground, z0 + cell),
                    ];
                    o.xray.quad(p, rgba(ov::FOOTPRINT, 0.15), NO_MASK);
                } else {
                    // Lo no ocupado, rayado tenue.
                    for t in [0.25, 0.5, 0.75] {
                        let d = t * 2.0 * cell;
                        let (a, b) = if d <= cell {
                            ((x0 + d, z0), (x0, z0 + d))
                        } else {
                            ((x0 + cell, z0 + d - cell), (x0 + d - cell, z0 + cell))
                        };
                        o.xray.line(
                            w(a.0, ground, a.1),
                            w(b.0, ground, b.1),
                            rgba(ov::FOOTPRINT, 0.3),
                            NO_MASK,
                        );
                    }
                }
            }
        }
    }

    if layers.nav {
        nav(&mut o, s, i.cut);
    }

    if layers.parts {
        for p in &s.parts {
            let [x0, y0, z0, x1, y1, z1] = p.bbox.map(|v| v as f32);
            let color = rgba(ov::PART, 1.0);
            o.xray.wire_box(
                Vec3::new(x0, y0, z0),
                Vec3::new(x1, y1, z1),
                color,
                mask(ov::PART_OUTLINE),
                None,
            );
            if p.kind == "Door" {
                // Bisagra en el extremo mínimo del eje largo; abre hacia `swing` en el del grosor (VoxelPart).
                let along_x = x1 - x0 >= z1 - z0;
                let (hinge, closed, open, r) = if along_x {
                    (
                        Vec3::new(x0, y0, (z0 + z1) * 0.5),
                        Vec3::X,
                        Vec3::Z * p.swing.signum() as f32,
                        x1 - x0,
                    )
                } else {
                    (
                        Vec3::new((x0 + x1) * 0.5, y0, z0),
                        Vec3::Z,
                        Vec3::X * p.swing.signum() as f32,
                        z1 - z0,
                    )
                };
                let steps = 8;
                for k in 0..steps {
                    if k % 2 == 1 {
                        continue;
                    }
                    let at = |k: i32| {
                        let t = k as f32 / steps as f32 * std::f32::consts::FRAC_PI_2;
                        hinge + (closed * t.cos() + open * t.sin()) * r
                    };
                    o.xray
                        .line(w_of(at(k)), w_of(at(k + 1)), color, mask(ov::PART_OUTLINE));
                }
                let d = Vec3::splat(0.5);
                o.xray
                    .box_faces(hinge - d, hinge + d, color, mask(ov::PART_OUTLINE));
            }
        }
    }

    if layers.entrance {
        entrance(&mut o, s);
    }

    if layers.bands {
        for &b in &s.band_starts {
            o.plain.dashed_rect(s, b as f32, 0.2, rgba(ov::BANDS, 0.4));
        }
    }

    if let Some(c) = i.cut {
        o.plain
            .dashed_rect(s, (c.feet * SUB) as f32, 0.2, rgba(ov::CUT, 1.0));
    }

    for g in &i.view.gizmos {
        gizmo(&mut o, s, i.paint, g, i.time);
    }

    if let Some(h) = i.hover {
        let min = Vec3::from(h.voxel.map(|v| v as f32));
        o.plain
            .box_faces(min, min + Vec3::ONE, NO_MASK, mask(ov::HOVER));
    }

    if let Some([x0, y0, z0, x1, y1, z1]) = i.highlight {
        let (min, max) = (
            Vec3::new(x0 as f32, y0 as f32, z0 as f32),
            Vec3::new(x1 as f32, y1 as f32, z1 as f32),
        );
        o.xray.box_faces(min, max, rgba(theme::RED, 0.35), NO_MASK);
        o.xray.wire_box(
            min,
            max,
            rgba(theme::RED, 1.0),
            mask(theme::DANGER_BG),
            None,
        );
    }
    o
}

/// Navegación: relleno del suelo transitable (la planta del corte al 30 %, el resto al 10 %), vallas en las aristas
/// bloqueadas entre dos suelos y chevrones donde se sube un nivel.
fn nav(o: &mut Overlay, s: &Structure, cut: Option<&Cut>) {
    let n = &s.nav;
    let cell = SUB as f32;
    let dirs: Vec<(i32, i32)> = n.dirs.iter().map(|d| dir_of(d)).collect();
    for level in 0..n.levels {
        if cut.is_some_and(|c| level > c.feet + 1) {
            continue;
        }
        let current = cut.is_none_or(|c| level >= c.feet);
        let y = (level * SUB) as f32;
        for cz in 0..n.cells_z {
            for cx in 0..n.cells_x {
                if !n.has_floor(cx, cz, level) {
                    continue;
                }
                let (x0, z0) = ((cx * SUB) as f32, (cz * SUB) as f32);
                let p = [
                    w(x0, y, z0),
                    w(x0 + cell, y, z0),
                    w(x0 + cell, y, z0 + cell),
                    w(x0, y, z0 + cell),
                ];
                let (a, m) = if current {
                    (0.3, mask(ov::NAV_EDGE))
                } else {
                    (0.1, NO_MASK)
                };
                o.plain.quad(p, rgba(ov::NAV_FILL, a), m);
                if !current {
                    continue;
                }
                let edges = n.edges_at(cx, cz, level);
                let centre = Vec3::new(x0 + cell * 0.5, y, z0 + cell * 0.5);
                for (bit, &(dx, dz)) in dirs.iter().enumerate().take(4) {
                    let (nx, nz) = (cx + dx, cz + dz);
                    let d = Vec3::new(dx as f32, 0.0, dz as f32);
                    let side = Vec3::new(dz as f32, 0.0, dx as f32).abs();
                    if edges >> bit & 1 != 0 && n.has_floor(nx, nz, level) {
                        // Valla baja sobre la arista, de 2 sub-cubos de alto.
                        let mid = centre + d * (cell * 0.5);
                        let half = side * (cell * 0.5 - 1.0);
                        let up = Vec3::Y * 2.0;
                        let q =
                            [mid - half, mid + half, mid + half + up, mid - half + up].map(w_of);
                        o.plain
                            .quad(q, rgba(ov::BLOCKED, 1.0), mask(ov::BLOCKED_OUTLINE));
                    }
                    if edges >> (bit + 4) & 1 == 0
                        && n.has_floor(nx, nz, level + 1)
                        && !n.has_floor(nx, nz, level)
                    {
                        // Escalera: chevron hacia donde se sube.
                        let tip = centre + d * 2.5;
                        let back = centre - d * 0.5;
                        for sgn in [-1.0, 1.0] {
                            o.plain.line(
                                w_of(tip),
                                w_of(back + side * (2.5 * sgn)),
                                rgba(ov::NAV_FILL, 1.0),
                                mask(ov::NAV_EDGE),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Doble chevron en el suelo de la celda de entrada, hacia dentro.
fn entrance(o: &mut Overlay, s: &Structure) {
    let [cells_x, cells_z] = s.grid.cells();
    let [ex, ez] = s.entrance;
    let (dx, dz) = if ez < 0 {
        (0, 1)
    } else if ez >= cells_z {
        (0, -1)
    } else if ex < 0 {
        (1, 0)
    } else if ex >= cells_x {
        (-1, 0)
    } else {
        (0, 1)
    };
    let cell = SUB as f32;
    let y = (s.ground_level * SUB) as f32;
    let centre = Vec3::new(
        (ex * SUB) as f32 + cell * 0.5,
        y,
        (ez * SUB) as f32 + cell * 0.5,
    );
    let (d, side) = (
        Vec3::new(dx as f32, 0.0, dz as f32),
        Vec3::new(dz as f32, 0.0, dx as f32).abs(),
    );
    for t in [-1.5, 1.5] {
        let tip = centre + d * (t + 1.5);
        for sgn in [-1.0, 1.0] {
            let end = tip - d * 2.0 + side * (2.5 * sgn);
            let back = -d * 1.0;
            let q = [tip, end, end + back, tip + back].map(w_of);
            o.xray
                .quad(q, rgba(ov::ENTRANCE, 1.0), mask(ov::ENTRANCE_OUTLINE));
        }
    }
}

fn gizmo(o: &mut Overlay, s: &Structure, paint: &Paint, g: &Gizmo, time: f64) {
    match g {
        Gizmo::Ghost {
            cells,
            material,
            erase,
        } => {
            let (color, outline) = if *erase {
                (rgba(ov::ERASE, 0.25), mask(ov::ERASE))
            } else {
                let c = paint
                    .palette
                    .get(*material as usize)
                    .copied()
                    .unwrap_or([255, 0, 255]);
                ([c[0], c[1], c[2], 140], mask(ov::HOVER))
            };
            let boxes: Vec<(Vec3, Vec3)> = if cells.len() > MAX_GHOST_CUBES {
                let lo = cells.iter().fold([i32::MAX; 3], |a, c| {
                    std::array::from_fn(|k| a[k].min(c[k]))
                });
                let hi = cells.iter().fold([i32::MIN; 3], |a, c| {
                    std::array::from_fn(|k| a[k].max(c[k] + 1))
                });
                vec![(
                    Vec3::from(lo.map(|v| v as f32)),
                    Vec3::from(hi.map(|v| v as f32)),
                )]
            } else {
                cells
                    .iter()
                    .map(|c| {
                        let p = Vec3::from(c.map(|v| v as f32));
                        (p, p + Vec3::ONE)
                    })
                    .collect()
            };
            for (min, max) in boxes {
                // Al borrar, el rayado es el de lo tapado: la caja va en las dos pasadas.
                if *erase {
                    o.xray.box_faces(min, max, color, outline);
                } else {
                    o.plain.box_faces(min, max, color, outline);
                }
            }
        }
        Gizmo::Selection { min, max } => {
            let (lo, hi) = (
                Vec3::from(min.map(|v| v as f32)),
                Vec3::from(max.map(|v| v as f32 + 1.0)),
            );
            o.xray.box_faces(lo, hi, rgba(ov::SELECTION, 0.2), NO_MASK);
            // Hormigas a 4 pasos por segundo.
            let phase = (time * 4.0).floor() as f32;
            o.xray.wire_box(
                lo,
                hi,
                rgba(ov::SELECTION, 1.0),
                NO_MASK,
                Some((phase, Some(rgba(ov::SELECTION_ALT, 1.0)))),
            );
        }
        Gizmo::Plane { y } => {
            let [sx, _, sz] = s.grid.size;
            let y = *y as f32;
            let p = [
                w(0.0, y, 0.0),
                w(sx as f32, y, 0.0),
                w(sx as f32, y, sz as f32),
                w(0.0, y, sz as f32),
            ];
            o.plain.quad(p, rgba(ov::HOVER, 0.08), NO_MASK);
            for x in 0..=sx {
                o.plain.line(
                    w(x as f32, y, 0.0),
                    w(x as f32, y, sz as f32),
                    rgba(ov::HOVER, 0.12),
                    NO_MASK,
                );
            }
            for z in 0..=sz {
                o.plain.line(
                    w(0.0, y, z as f32),
                    w(sx as f32, y, z as f32),
                    rgba(ov::HOVER, 0.12),
                    NO_MASK,
                );
            }
        }
    }
}
