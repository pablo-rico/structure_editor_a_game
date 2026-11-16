//! Operaciones puras de las herramientas: qué sub-cubos toca cada una. Quien las usa las pinta en un `Doc` dentro de
//! un trazo.

use std::collections::{HashSet, VecDeque};

use crate::model::{SUB, VoxelGrid};

pub type V = [i32; 3];

/// Caja entre dos esquinas incluidas (en cualquier orden).
pub fn cuboid(a: V, b: V) -> Vec<V> {
    let lo = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
    let hi = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
    let mut out = Vec::new();
    for y in lo[1]..=hi[1] {
        for z in lo[2]..=hi[2] {
            for x in lo[0]..=hi[0] {
                out.push([x, y, z]);
            }
        }
    }
    out
}

/// Forma de la caja.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxShape {
    Solid,
    /// Solo la cáscara.
    Hollow,
    /// Sin suelo ni techo.
    Walls,
}

/// Caja con forma: llena, hueca (las seis caras) o solo paredes (las cuatro laterales).
pub fn shaped_box(a: V, b: V, shape: BoxShape) -> Vec<V> {
    let lo = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
    let hi = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
    let side = |v: &V| v[0] == lo[0] || v[0] == hi[0] || v[2] == lo[2] || v[2] == hi[2];
    cuboid(a, b)
        .into_iter()
        .filter(|v| match shape {
            BoxShape::Solid => true,
            BoxShape::Walls => side(v),
            BoxShape::Hollow => side(v) || v[1] == lo[1] || v[1] == hi[1],
        })
        .collect()
}

/// Caja semiabierta `[x0, y0, z0, x1, y1, z1)` girada un cuarto de vuelta en una rejilla de fondo `size_z`, con la
/// regla de `RotateQuarter`: `(x, z) → (size_z − 1 − z, x)`.
pub fn rotate_box(b: [i32; 6], size_z: i32) -> [i32; 6] {
    [size_z - b[5], b[1], b[0], size_z - b[2], b[4], b[3]]
}

/// Dirección (`dirs` = +X, −X, +Z, −Z) tras un cuarto de vuelta: +X→+Z, −X→−Z, +Z→−X, −Z→+X.
pub fn rotate_dir(d: i32) -> i32 {
    [2, 3, 1, 0][d.rem_euclid(4) as usize]
}

/// Línea 3D sin huecos (DDA entero, Bresenham en el eje dominante).
pub fn line(a: V, b: V) -> Vec<V> {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let n = d.iter().map(|v| v.abs()).max().unwrap_or(0);
    (0..=n)
        .map(|i| {
            let t = |k: usize| a[k] + if n == 0 { 0 } else { (2 * d[k] * i + n * d[k].signum()) / (2 * n) };
            [t(0), t(1), t(2)]
        })
        .collect()
}

/// Pincel de lado `size` (1, 2, 4 u 8) sobre el sub-cubo `v`; con 8 ocupa la celda entera (alineado a celda).
pub fn brush(v: V, size: i32) -> Vec<V> {
    let base = |c: i32| if size >= SUB { c.div_euclid(SUB) * SUB } else { c - (size - 1) / 2 };
    let lo = [base(v[0]), base(v[1]), base(v[2])];
    cuboid(lo, [lo[0] + size - 1, lo[1] + size - 1, lo[2] + size - 1])
}

/// Relleno 3D: sub-cubos del mismo material unidos por caras desde `start`; con `ys` (alturas incluidas), solo dentro
/// de esa franja (una planta). `limit` corta rellenos desbocados (el aire de fuera, por ejemplo).
pub fn flood(grid: &VoxelGrid, start: V, ys: Option<(i32, i32)>, limit: usize) -> Vec<V> {
    if !grid.in_bounds(start[0], start[1], start[2]) {
        return Vec::new();
    }
    let target = grid.get(start[0], start[1], start[2]);
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    let mut out = Vec::new();
    while let Some(v) = queue.pop_front() {
        out.push(v);
        if out.len() >= limit {
            break;
        }
        for d in DIRS {
            let n = [v[0] + d[0], v[1] + d[1], v[2] + d[2]];
            if ys.is_some_and(|(lo, hi)| n[1] < lo || n[1] > hi) || !grid.in_bounds(n[0], n[1], n[2]) {
                continue;
            }
            if grid.get(n[0], n[1], n[2]) == target && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    out
}

pub const DIRS: [V; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// Extruir cara: la región de caras visibles del mismo material y en el mismo plano que la cara `(start, normal)`,
/// unida por aristas; devuelve dónde van los sub-cubos nuevos (una capa hacia `normal`).
pub fn extrude(grid: &VoxelGrid, start: V, normal: V) -> Vec<V> {
    let m = grid.get(start[0], start[1], start[2]);
    let face = |v: V| {
        grid.get(v[0], v[1], v[2]) == m
            && m != 0
            && grid.get(v[0] + normal[0], v[1] + normal[1], v[2] + normal[2]) == 0
    };
    if !face(start) {
        return Vec::new();
    }
    let steps: Vec<V> = DIRS.iter().copied().filter(|d| d.iter().zip(normal).all(|(a, b)| a * b == 0)).collect();
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    let mut out = Vec::new();
    while let Some(v) = queue.pop_front() {
        out.push([v[0] + normal[0], v[1] + normal[1], v[2] + normal[2]]);
        for d in &steps {
            let n = [v[0] + d[0], v[1] + d[1], v[2] + d[2]];
            if face(n) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    out.retain(|v| grid.in_bounds(v[0], v[1], v[2]));
    out
}

/// Copias simétricas de `v` para pintar con espejo en X y/o Z (sin repetir).
pub fn mirrored(v: V, size: [i32; 3], x: bool, z: bool) -> Vec<V> {
    let mut out = vec![v];
    if x {
        out.push([size[0] - 1 - v[0], v[1], v[2]]);
    }
    if z {
        let n = out.len();
        for i in 0..n {
            let w = out[i];
            out.push([w[0], w[1], size[2] - 1 - w[2]]);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Trozo copiado (portapapeles y sellos).
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub size: [i32; 3],
    pub data: Vec<u8>,
    pub detail: Vec<u8>,
}

impl Clip {
    fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((y * self.size[2] + z) * self.size[0] + x) as usize
    }

    /// Copia la caja entre dos esquinas incluidas.
    pub fn copy(grid: &VoxelGrid, a: V, b: V) -> Self {
        let lo = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
        let size = [(a[0] - b[0]).abs() + 1, (a[1] - b[1]).abs() + 1, (a[2] - b[2]).abs() + 1];
        let mut c = Self { size, data: Vec::new(), detail: Vec::new() };
        for [x, y, z] in cuboid([0; 3], [size[0] - 1, size[1] - 1, size[2] - 1]) {
            let (gx, gy, gz) = (lo[0] + x, lo[1] + y, lo[2] + z);
            c.data.push(grid.get(gx, gy, gz));
            c.detail.push(grid.is_detail(gx, gy, gz) as u8);
        }
        c
    }

    /// Sub-cubos sólidos con su posición relativa: `(pos, material, detalle)`.
    pub fn solids(&self) -> impl Iterator<Item = (V, u8, bool)> + '_ {
        cuboid([0; 3], [self.size[0] - 1, self.size[1] - 1, self.size[2] - 1]).into_iter().filter_map(|v| {
            let i = self.idx(v[0], v[1], v[2]);
            (self.data[i] != 0).then_some((v, self.data[i], self.detail[i] != 0))
        })
    }

    fn remap(&self, size: [i32; 3], f: impl Fn(V) -> V) -> Self {
        let mut c = Self { size, data: vec![0; self.data.len()], detail: vec![0; self.data.len()] };
        for v in cuboid([0; 3], [self.size[0] - 1, self.size[1] - 1, self.size[2] - 1]) {
            let (from, to) = (self.idx(v[0], v[1], v[2]), f(v));
            let t = c.idx(to[0], to[1], to[2]);
            c.data[t] = self.data[from];
            c.detail[t] = self.detail[from];
        }
        c
    }

    pub fn mirror_x(&self) -> Self {
        self.remap(self.size, |v| [self.size[0] - 1 - v[0], v[1], v[2]])
    }

    pub fn mirror_z(&self) -> Self {
        self.remap(self.size, |v| [v[0], v[1], self.size[2] - 1 - v[2]])
    }

    /// Un cuarto de vuelta con la regla de `VoxelGrid.RotateQuarter`: `(x, z) → (sizeZ − 1 − z, x)`.
    pub fn rotate_quarter(&self) -> Self {
        let size = [self.size[2], self.size[1], self.size[0]];
        self.remap(size, |v| [self.size[2] - 1 - v[2], v[1], v[0]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formas() {
        assert_eq!(cuboid([1, 0, 0], [0, 1, 0]).len(), 4);
        let l = line([0, 0, 0], [5, 2, 0]);
        assert_eq!(l.len(), 6);
        assert_eq!((l[0], l[5]), ([0, 0, 0], [5, 2, 0]));
        assert!(l.windows(2).all(|w| (0..3).all(|k| (w[0][k] - w[1][k]).abs() <= 1)));
        assert_eq!(brush([13, 2, 9], 8)[0], [8, 0, 8]);
        assert_eq!(brush([5, 5, 5], 1), vec![[5, 5, 5]]);
        assert_eq!(mirrored([1, 0, 1], [8, 8, 8], true, true).len(), 4);
        assert_eq!(mirrored([1, 0, 1], [8, 8, 8], false, false), vec![[1, 0, 1]]);
    }

    #[test]
    fn relleno_y_extruir() {
        let mut g = VoxelGrid::new([4, 4, 4]);
        for v in cuboid([0, 0, 0], [3, 0, 3]) {
            g.set(v[0], v[1], v[2], 1);
        }
        g.set(0, 0, 0, 2);
        assert_eq!(flood(&g, [1, 0, 1], None, 1000).len(), 15);
        assert_eq!(flood(&g, [1, 1, 1], Some((1, 1)), 1000).len(), 16); // aire de la planta 1
        assert_eq!(flood(&g, [1, 1, 1], Some((1, 2)), 1000).len(), 32);
        let up = extrude(&g, [2, 0, 2], [0, 1, 0]);
        assert_eq!(up.len(), 15);
        assert!(up.iter().all(|v| v[1] == 1));
        assert!(extrude(&g, [2, 0, 2], [0, -1, 0]).is_empty()); // fuera de la rejilla
    }

    #[test]
    fn cajas_con_forma_y_giros() {
        assert_eq!(shaped_box([0, 0, 0], [2, 2, 2], BoxShape::Solid).len(), 27);
        assert_eq!(shaped_box([0, 0, 0], [2, 2, 2], BoxShape::Hollow).len(), 26);
        assert_eq!(shaped_box([0, 0, 0], [2, 2, 2], BoxShape::Walls).len(), 24);
        // Una caja girada cuatro veces vuelve a su sitio (en una rejilla cuadrada).
        let b = [1, 0, 2, 3, 4, 5];
        assert_eq!((0..4).fold(b, |b, _| rotate_box(b, 8)), b);
        // (x, z) = (1, 2) → (8 − 1 − 2, 1) = (5, 1): la caja [1,3)×[2,5) pasa a [3,6)×[1,3).
        assert_eq!(rotate_box(b, 8), [3, 0, 1, 6, 4, 3]);
        assert_eq!((0..4).fold(3, |d, _| rotate_dir(d)), 3);
        assert_eq!(rotate_dir(0), 2);
    }

    #[test]
    fn portapapeles_gira_como_el_juego() {
        let mut g = VoxelGrid::new([3, 1, 2]);
        g.set(0, 0, 0, 7);
        let c = Clip::copy(&g, [0, 0, 0], [2, 0, 1]);
        let r = c.rotate_quarter();
        assert_eq!(r.size, [2, 1, 3]);
        // (x, z) = (0, 0) → (sizeZ − 1 − 0, 0) = (1, 0)
        assert_eq!(r.solids().next().unwrap().0, [1, 0, 0]);
        assert_eq!(c.rotate_quarter().rotate_quarter().rotate_quarter().rotate_quarter(), c);
        assert_eq!(c.mirror_x().solids().next().unwrap().0, [2, 0, 0]);
    }
}
