//! Estructura en memoria tal como la devuelve el puente (`docs/contrato.md`): rejilla de sub-cubos, navegación,
//! partes, interactuables y comprobaciones. Mismo índice que `VoxelGrid.Raw` del juego.

use crate::bridge::proto::{Check, Interactable, Part};

/// Sub-cubos por lado de celda y por nivel (unidades del contrato, `WorldConstants.SubPerCell`/`SubPerLevel`).
pub const SUB: i32 = 8;
/// Alto de un nivel en unidades de vista (`WorldConstants.LevelHeight3D`).
pub const LEVEL_HEIGHT: f32 = 0.816_496_6;

/// Rejilla de sub-cubos: un byte de `StructureMaterial` por sub-cubo (0 = aire). Fuera de límites todo es aire.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelGrid {
    pub size: [i32; 3],
    pub data: Vec<u8>,
    /// Marca de detalle por sub-cubo (muebles, escaleras); `None` si no hay ninguna.
    pub detail: Option<Vec<u8>>,
}

impl VoxelGrid {
    pub fn new(size: [i32; 3]) -> Self {
        assert!(size.iter().all(|&s| s > 0), "tamaño no válido: {size:?}");
        let n = (size[0] * size[1] * size[2]) as usize;
        Self { size, data: vec![0; n], detail: None }
    }

    pub fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        (x as u32) < self.size[0] as u32 && (y as u32) < self.size[1] as u32 && (z as u32) < self.size[2] as u32
    }

    /// Índice de `(x, y, z)` dentro de `data`: `(y · sz + z) · sx + x`. Solo para coordenadas dentro de límites.
    pub fn index(&self, x: i32, y: i32, z: i32) -> usize {
        ((y * self.size[2] + z) * self.size[0] + x) as usize
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if self.in_bounds(x, y, z) { self.data[self.index(x, y, z)] } else { 0 }
    }

    pub fn is_detail(&self, x: i32, y: i32, z: i32) -> bool {
        self.in_bounds(x, y, z) && self.detail.as_ref().is_some_and(|d| d[self.index(x, y, z)] != 0)
    }

    /// Escribe un sub-cubo; fuera de límites se ignora (como `VoxelGrid.Set`).
    pub fn set(&mut self, x: i32, y: i32, z: i32, material: u8) {
        if self.in_bounds(x, y, z) {
            let i = self.index(x, y, z);
            self.data[i] = material;
        }
    }

    /// Celdas que cubre en el suelo: `[sx / 8, sz / 8]`.
    pub fn cells(&self) -> [i32; 2] {
        [self.size[0] / SUB, self.size[2] / SUB]
    }
}

/// Color propio de una cara de un modelo: clave [`face_key`], valor = byte de color como en la rejilla (índice de
/// paleta + 1). Las estructuras no lo usan (mapa vacío).
pub type FaceColors = std::collections::HashMap<u32, u8>;

/// Clave de la cara `dir` (`+x −x +y −y +z −z`) del vóxel `v`: `x | y << 8 | z << 16 | dir << 24`.
pub fn face_key(v: [i32; 3], dir: u8) -> u32 {
    v[0] as u32 | (v[1] as u32) << 8 | (v[2] as u32) << 16 | (dir as u32) << 24
}

pub fn face_unkey(k: u32) -> ([i32; 3], u8) {
    ([(k & 255) as i32, (k >> 8 & 255) as i32, (k >> 16 & 255) as i32], (k >> 24) as u8)
}

/// Navegación derivada de los sub-cubos (`StructureNav`), en el formato de la carga de `build`.
#[derive(Clone, Debug, Default)]
pub struct Nav {
    pub levels: i32,
    pub cells_x: i32,
    pub cells_z: i32,
    /// Orden de las direcciones de los bits de `edges` (`StructureNav.Directions`: `+X, -X, +Z, -Z`).
    pub dirs: Vec<String>,
    /// Por celda (`z · cells_x + x`): bit N = el nivel N tiene suelo transitable.
    pub floors: Vec<u64>,
    /// Por celda: bit N = el nivel N tiene forjado.
    pub slabs: Vec<u64>,
    /// Por nivel y celda (`(level · cells_z + z) · cells_x + x`): bits 0–3 paso llano bloqueado, 4–7 escalón bloqueado.
    pub edges: Vec<u8>,
}

impl Nav {
    fn cell(&self, cx: i32, cz: i32) -> Option<usize> {
        ((cx as u32) < self.cells_x as u32 && (cz as u32) < self.cells_z as u32)
            .then(|| (cz * self.cells_x + cx) as usize)
    }

    pub fn has_floor(&self, cx: i32, cz: i32, level: i32) -> bool {
        (0..64).contains(&level) && self.cell(cx, cz).is_some_and(|i| self.floors[i] >> level & 1 != 0)
    }

    pub fn has_slab(&self, cx: i32, cz: i32, level: i32) -> bool {
        (0..64).contains(&level) && self.cell(cx, cz).is_some_and(|i| self.slabs[i] >> level & 1 != 0)
    }

    /// Bits de aristas de una celda y nivel (0 fuera de límites).
    pub fn edges_at(&self, cx: i32, cz: i32, level: i32) -> u8 {
        if !(0..self.levels).contains(&level) {
            return 0;
        }
        self.cell(cx, cz).map_or(0, |i| self.edges[level as usize * (self.cells_x * self.cells_z) as usize + i])
    }
}

/// Una estructura construida (procedural o hecha a mano) lista para mallar, pintar e inspeccionar.
#[derive(Clone, Debug, Default)]
pub struct Structure {
    pub grid: VoxelGrid,
    /// Semilla de la instancia: el mallado la usa para el tono por sub-cubo (`VoxelTone`), como el juego.
    pub seed: i32,
    pub ground_level: i32,
    pub band_starts: Vec<i32>,
    pub band_floors: Vec<i32>,
    /// Celda de fuera pegada a la puerta, en el marco de la rejilla.
    pub entrance: [i32; 2],
    pub courtyard: bool,
    /// Elemento mágico con que se tiñen los materiales `Theme*` (escuelas).
    pub theme: Option<usize>,
    pub parts: Vec<Part>,
    pub interactables: Vec<Interactable>,
    /// Celdas cubiertas (1) si la huella tiene huecos; `None` = toda la caja.
    pub footprint: Option<Vec<u8>>,
    pub nav: Nav,
    pub checks: Vec<Check>,
    /// Nombre de la sustitución hecha a mano si lo construido no es el plano procedural del juego.
    pub override_by: Option<String>,
    /// Solo modelos (`vmodel`): caras con un color distinto del de su vóxel (sombreado dibujado).
    pub face_colors: FaceColors,
    /// Solo modelos: vóxeles por casilla del modelo (`cell`). `None` en las estructuras.
    pub model_cell: Option<i32>,
}

impl Structure {
    /// Vóxeles por lado de celda: los del modelo o `SUB`.
    // shortcut: el vóxel de un modelo se dibuja como un sub-cubo (el doble de grande que en el juego);
    // parametrizar SUB si algún día se mezclan modelos y estructuras a escala real.
    pub fn cell_size(&self) -> i32 {
        self.model_cell.unwrap_or(SUB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indice_como_voxelgrid_raw() {
        let mut g = VoxelGrid::new([16, 8, 24]);
        g.set(3, 2, 5, 7);
        assert_eq!(g.data[(2 * 24 + 5) * 16 + 3], 7);
        assert_eq!(g.get(3, 2, 5), 7);
        assert_eq!(g.get(-1, 0, 0), 0);
        assert_eq!(g.get(16, 0, 0), 0);
        g.set(99, 0, 0, 1); // fuera: se ignora
        assert_eq!(g.cells(), [2, 3]);
    }
}
