//! Picking por rayo DDA sobre la rejilla, en coordenadas de sub-cubo, y la regla del corte por planta (la misma que
//! descarta sub-cubos en el shader, `StructureCutaway.hlsl`).

use glam::Vec3;

use crate::model::{SUB, VoxelGrid};

/// Sub-cubo bajo el rayo y la normal de la cara por la que entra (`[0, 0, 0]` si el rayo nace dentro).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub voxel: [i32; 3],
    pub normal: [i32; 3],
}

/// Alto del corte sobre los pies en los muros del lado de la cámara y del fondo, en sub-cubos
/// (`StructureCutaway.FrontHeight` = 0,25 niveles y `BackHeight` = `OuterHeight` = 15/8 niveles).
pub const CUT_FRONT: f32 = 2.0;
pub const CUT_BACK: f32 = 15.0;

/// Corte por planta como el cutaway del juego con la región = toda la rejilla: sobre los pies de la planta, de
/// [`CUT_FRONT`] en el lado de la cámara a [`CUT_BACK`] en el del fondo; el detalle (muebles, escaleras) se queda
/// entero hasta [`CUT_BACK`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    /// Nivel de los pies (suelo de la planta).
    pub feet: i32,
    /// Hacia dónde mira la cámara sobre el suelo `(x, z)`, unitario.
    pub forward: [f32; 2],
    /// Tamaño de la rejilla en sub-cubos `(sx, sz)`: la región que se corta.
    pub size: [i32; 2],
}

impl Cut {
    /// Altura de corte de la columna `(x, z)` en sub-cubos (`CutHeight`): se descarta todo `y` ≥ esto.
    pub fn height(&self, x: i32, z: i32) -> i32 {
        let half = [self.size[0] as f32 * 0.5, self.size[1] as f32 * 0.5];
        let [fx, fz] = self.forward;
        let extent = (fx.abs() * half[0] + fz.abs() * half[1]).max(1e-3);
        let u = ((x as f32 + 0.5 - half[0]) * fx + (z as f32 + 0.5 - half[1]) * fz) / extent;
        let t = smoothstep(-0.5, 0.5, u);
        ((self.feet * SUB) as f32 + CUT_FRONT + (CUT_BACK - CUT_FRONT) * t + 0.5).floor() as i32
    }

    /// ¿Se ve el sub-cubo `(x, y, z)` con el corte?
    pub fn keeps(&self, x: i32, y: i32, z: i32, detail: bool) -> bool {
        let mut cut = self.height(x, z);
        if detail {
            cut = cut.max(self.feet * SUB + CUT_BACK as i32);
        }
        y < cut
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Primer sub-cubo visible que cruza el rayo `origin + t · dir` (t ≥ 0), en coordenadas de sub-cubo. Se salta el
/// aire, `hidden` (no se pinta) y lo que quita el corte.
pub fn raycast(
    grid: &VoxelGrid,
    origin: Vec3,
    dir: Vec3,
    cut: Option<&Cut>,
    hidden: Option<u8>,
) -> Option<Hit> {
    if dir.length_squared() < 1e-12 {
        return None;
    }
    let size = Vec3::new(
        grid.size[0] as f32,
        grid.size[1] as f32,
        grid.size[2] as f32,
    );
    // Recorta el rayo a la caja de la rejilla (slabs).
    let inv = dir.recip();
    let (t0, t1) = ((Vec3::ZERO - origin) * inv, (size - origin) * inv);
    let t_enter = t0.min(t1).max_element().max(0.0);
    let t_exit = t0.max(t1).min_element();
    if t_enter > t_exit {
        return None;
    }
    let start = origin + dir * t_enter;
    let mut cell = start
        .floor()
        .as_ivec3()
        .clamp(glam::IVec3::ZERO, grid_max(grid));
    let step = dir.signum().as_ivec3();
    let next = |c: i32, s: i32| (c + i32::from(s > 0)) as f32;
    let mut t_max = Vec3::new(
        if step.x != 0 {
            (next(cell.x, step.x) - origin.x) * inv.x
        } else {
            f32::INFINITY
        },
        if step.y != 0 {
            (next(cell.y, step.y) - origin.y) * inv.y
        } else {
            f32::INFINITY
        },
        if step.z != 0 {
            (next(cell.z, step.z) - origin.z) * inv.z
        } else {
            f32::INFINITY
        },
    );
    let t_delta = inv.abs();
    // Normal de entrada: el eje por el que el rayo entró en la caja.
    let mut normal = if t_enter > 0.0 {
        let te = t0.min(t1);
        let axis = if te.x >= te.y && te.x >= te.z {
            0
        } else if te.y >= te.z {
            1
        } else {
            2
        };
        let mut n = [0; 3];
        n[axis] = -step[axis];
        n
    } else {
        [0; 3]
    };
    loop {
        if !grid.in_bounds(cell.x, cell.y, cell.z) {
            return None;
        }
        let m = grid.get(cell.x, cell.y, cell.z);
        let visible = m != 0
            && Some(m) != hidden
            && cut.is_none_or(|c| {
                c.keeps(
                    cell.x,
                    cell.y,
                    cell.z,
                    grid.is_detail(cell.x, cell.y, cell.z),
                )
            });
        if visible {
            return Some(Hit {
                voxel: cell.to_array(),
                normal,
            });
        }
        let axis = if t_max.x < t_max.y && t_max.x < t_max.z {
            0
        } else if t_max.y < t_max.z {
            1
        } else {
            2
        };
        if t_max[axis] > t_exit + 1e-4 {
            return None;
        }
        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];
        normal = [0; 3];
        normal[axis] = -step[axis];
    }
}

/// Punto donde el rayo `origin + t · dir` (t ≥ 0) corta el plano horizontal `y` (sub-cubos); `None` si va paralelo o
/// lo cruza por detrás.
pub fn ray_plane_y(origin: Vec3, dir: Vec3, y: f32) -> Option<Vec3> {
    let t = (y - origin.y) / dir.y;
    (dir.y.abs() > 1e-6 && t >= 0.0).then(|| origin + dir * t)
}

fn grid_max(grid: &VoxelGrid) -> glam::IVec3 {
    glam::IVec3::new(grid.size[0] - 1, grid.size[1] - 1, grid.size[2] - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> VoxelGrid {
        let mut g = VoxelGrid::new([8, 8, 8]);
        g.set(4, 2, 4, 1);
        g.set(4, 6, 4, 1);
        g
    }

    #[test]
    fn rayo_vertical_da_el_primero_desde_arriba() {
        let g = grid();
        let hit = raycast(&g, Vec3::new(4.5, 20.0, 4.5), Vec3::NEG_Y, None, None).unwrap();
        assert_eq!(
            hit,
            Hit {
                voxel: [4, 6, 4],
                normal: [0, 1, 0]
            }
        );
        // Desde abajo, el otro.
        let hit = raycast(&g, Vec3::new(4.5, -3.0, 4.5), Vec3::Y, None, None).unwrap();
        assert_eq!(
            hit,
            Hit {
                voxel: [4, 2, 4],
                normal: [0, -1, 0]
            }
        );
    }

    #[test]
    fn rayo_en_diagonal_y_fallo() {
        let g = grid();
        let hit = raycast(
            &g,
            Vec3::new(0.5, 6.5, 0.2),
            Vec3::new(1.0, 0.0, 1.0),
            None,
            None,
        )
        .unwrap();
        assert_eq!(hit.voxel, [4, 6, 4]);
        assert!(hit.normal == [-1, 0, 0] || hit.normal == [0, 0, -1]);
        assert!(raycast(&g, Vec3::new(0.5, 4.5, 0.5), Vec3::X, None, None).is_none());
        assert!(raycast(&g, Vec3::new(-5.0, 50.0, -5.0), Vec3::Y, None, None).is_none());
    }

    #[test]
    fn hidden_y_corte_se_saltan() {
        let mut g = grid();
        g.set(4, 6, 4, 61);
        let down = |g: &VoxelGrid, cut: Option<&Cut>, hidden| {
            raycast(g, Vec3::new(4.5, 20.0, 4.5), Vec3::NEG_Y, cut, hidden)
        };
        assert_eq!(down(&g, None, Some(61)).unwrap().voxel, [4, 2, 4]);
        // Columna del frente (z = 0, la cámara mira a +Z): con los pies en el nivel 0 se corta a 2 sub-cubos.
        let mut g = VoxelGrid::new([8, 8, 8]);
        g.set(4, 1, 0, 1);
        g.set(4, 6, 0, 1);
        let cut = Cut {
            feet: 0,
            forward: [0.0, 1.0],
            size: [8, 8],
        };
        let down =
            |g: &VoxelGrid| raycast(g, Vec3::new(4.5, 20.0, 0.5), Vec3::NEG_Y, Some(&cut), None);
        assert_eq!(cut.height(4, 0), 2);
        assert_eq!(down(&g).unwrap().voxel, [4, 1, 0]);
        // El detalle sigue entero hasta 15 sub-cubos sobre los pies.
        let mut detail = vec![0; 512];
        detail[g.index(4, 6, 0)] = 1;
        g.detail = Some(detail);
        assert_eq!(down(&g).unwrap().voxel, [4, 6, 0]);
    }

    #[test]
    fn rayo_contra_plano_horizontal() {
        let p = ray_plane_y(Vec3::new(1.0, 10.0, 2.0), Vec3::new(1.0, -2.0, 0.0), 4.0).unwrap();
        assert!(p.distance(Vec3::new(4.0, 4.0, 2.0)) < 1e-5);
        assert!(ray_plane_y(Vec3::ZERO, Vec3::X, 4.0).is_none());
        assert!(ray_plane_y(Vec3::ZERO, Vec3::NEG_Y, 4.0).is_none());
    }

    #[test]
    fn corte_bajo_delante_y_alto_detras() {
        let cut = Cut {
            feet: 1,
            forward: [0.0, 1.0],
            size: [64, 64],
        };
        assert_eq!(cut.height(32, 0), 8 + 2);
        assert_eq!(cut.height(32, 63), 8 + 15);
        let mid = cut.height(32, 32);
        assert!(mid > 10 && mid < 23);
    }
}
