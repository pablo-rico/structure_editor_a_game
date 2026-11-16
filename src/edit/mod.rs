//! Modo Editar (fase 2): el documento que se edita, su historial (deshacer infinito con diferencias dispersas
//! agrupadas por trazo) y las operaciones de las herramientas (`ops`).

pub mod ops;
pub mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::agvox::Meta;
use crate::bridge::proto::{Interactable, Part};
use crate::model::{Structure, VoxelGrid};

/// Lo que no son sub-cubos: cambia poco y pesa poco, así que se guarda entero en cada paso del historial.
#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    pub meta: Meta,
    pub ground_level: i32,
    pub band_starts: Vec<i32>,
    pub band_floors: Vec<i32>,
    pub entrance: [i32; 2],
    pub courtyard: bool,
    pub parts: Vec<Part>,
    pub interactables: Vec<Interactable>,
}

/// Un paso del historial.
#[derive(Clone, Debug)]
enum Step {
    /// `(índice, material antes, después, detalle antes, después)`.
    Voxels(Vec<(u32, u8, u8, u8, u8)>),
    Extras(Box<Extras>, Box<Extras>),
    /// Cambio de tamaño u otra operación que rehace la rejilla entera.
    Grid(Box<VoxelGrid>, Box<VoxelGrid>),
    /// Varios pasos que se deshacen juntos (lo que llega del generador: rejilla + partes + plantas).
    Many(Vec<Step>),
}

impl Step {
    fn bytes(&self) -> usize {
        match self {
            Step::Voxels(v) => v.len() * 8,
            Step::Extras(..) => 1024,
            Step::Grid(a, b) => a.data.len() * 2 + b.data.len() * 2,
            Step::Many(steps) => steps.iter().map(Step::bytes).sum(),
        }
    }
}

/// Ventana en que dos cambios de extras con el mismo nombre se funden.
const MERGE_EXTRAS: Duration = Duration::from_millis(700);

/// Tope de memoria del historial; al pasarlo se olvidan los pasos más viejos.
pub const HISTORY_BYTES: usize = 512 << 20;

pub struct Doc {
    pub meta: Meta,
    pub s: Structure,
    /// Fichero `.agvox` de origen, si lo hay.
    pub path: Option<PathBuf>,
    /// Hay cambios sin guardar.
    pub unsaved: bool,
    /// Sube con cada cambio (pintar, deshacer, extras, tamaño): quien dibuja o valida lo compara.
    pub version: u64,
    undo: Vec<(String, Step)>,
    redo: Vec<(String, Step)>,
    /// Cuándo se apuntó el último paso de extras (arrastrar un número no llena el historial).
    last_extras: Option<Instant>,
    stroke: Option<Stroke>,
}

/// Trazo en curso: diferencias y dónde está la de cada sub-cubo (repintar dentro del trazo no crece la lista).
struct Stroke {
    label: String,
    diffs: Vec<(u32, u8, u8, u8, u8)>,
    at: HashMap<u32, usize>,
}

impl Doc {
    pub fn new(meta: Meta, s: Structure, path: Option<PathBuf>) -> Self {
        Self { meta, s, path, unsaved: false, version: 0, undo: Vec::new(), redo: Vec::new(), last_extras: None, stroke: None }
    }

    pub fn extras(&self) -> Extras {
        Extras {
            meta: self.meta.clone(),
            ground_level: self.s.ground_level,
            band_starts: self.s.band_starts.clone(),
            band_floors: self.s.band_floors.clone(),
            entrance: self.s.entrance,
            courtyard: self.s.courtyard,
            parts: self.s.parts.clone(),
            interactables: self.s.interactables.clone(),
        }
    }

    fn set_extras(&mut self, e: &Extras) {
        self.meta = e.meta.clone();
        self.s.ground_level = e.ground_level;
        self.s.band_starts = e.band_starts.clone();
        self.s.band_floors = e.band_floors.clone();
        self.s.entrance = e.entrance;
        self.s.courtyard = e.courtyard;
        self.s.parts = e.parts.clone();
        self.s.interactables = e.interactables.clone();
    }

    /// Empieza un trazo: todo lo que se pinte hasta `end_stroke` se deshace de una vez.
    pub fn begin_stroke(&mut self, label: &str) {
        self.end_stroke();
        self.stroke = Some(Stroke { label: label.to_string(), diffs: Vec::new(), at: HashMap::new() });
    }

    /// Pinta un sub-cubo dentro del trazo en curso (abre uno si no hay). `detail` marca o desmarca el detalle.
    pub fn paint(&mut self, x: i32, y: i32, z: i32, material: u8, detail: bool) {
        if !self.s.grid.in_bounds(x, y, z) {
            return;
        }
        if self.stroke.is_none() {
            self.begin_stroke("Pintar");
        }
        let i = self.s.grid.index(x, y, z);
        let old_m = self.s.grid.data[i];
        let old_d = self.s.grid.detail.as_ref().map_or(0, |d| d[i]);
        let new_d = (detail && material != 0) as u8;
        if old_m == material && old_d == new_d {
            return;
        }
        let st = self.stroke.as_mut().unwrap();
        // Primera vez que el trazo toca este sub-cubo: se apunta el valor de antes.
        match st.at.get(&(i as u32)) {
            Some(&k) => {
                st.diffs[k].2 = material;
                st.diffs[k].4 = new_d;
            }
            None => {
                st.at.insert(i as u32, st.diffs.len());
                st.diffs.push((i as u32, old_m, material, old_d, new_d));
            }
        }
        self.s.grid.data[i] = material;
        self.version += 1;
        self.unsaved = true;
        if new_d != 0 || old_d != 0 {
            let len = self.s.grid.data.len();
            self.s.grid.detail.get_or_insert_with(|| vec![0; len])[i] = new_d;
        }
    }

    /// Cierra el trazo en curso y lo apunta en el historial (si cambió algo).
    pub fn end_stroke(&mut self) {
        let Some(Stroke { label, mut diffs, .. }) = self.stroke.take() else { return };
        diffs.retain(|d| d.1 != d.2 || d.3 != d.4);
        if !diffs.is_empty() {
            self.push(label, Step::Voxels(diffs));
        }
    }

    /// Cambia lo que no son sub-cubos (partes, franjas, entrada, nombre…) con un paso de historial.
    pub fn edit_extras(&mut self, label: &str, f: impl FnOnce(&mut Extras)) {
        self.end_stroke();
        let before = self.extras();
        let mut after = before.clone();
        f(&mut after);
        if after == before {
            return;
        }
        self.set_extras(&after);
        self.version += 1;
        // Cambios seguidos del mismo campo (arrastrar un número) se funden en un paso.
        let recent = self.last_extras.is_some_and(|t| t.elapsed() < MERGE_EXTRAS);
        self.last_extras = Some(Instant::now());
        if recent
            && self.redo.is_empty()
            && let Some((l, Step::Extras(_, last))) = self.undo.last_mut()
            && l == label
        {
            **last = after;
            self.unsaved = true;
            return;
        }
        self.push(label.into(), Step::Extras(Box::new(before), Box::new(after)));
    }

    /// Sustituye la rejilla entera (cambio de tamaño, giro de todo).
    pub fn replace_grid(&mut self, label: &str, grid: VoxelGrid) {
        self.end_stroke();
        self.version += 1;
        let before = std::mem::replace(&mut self.s.grid, grid);
        let after = self.s.grid.clone();
        self.push(label.into(), Step::Grid(Box::new(before), Box::new(after)));
    }

    /// Cambia la estructura por `result` (herramienta de construcción, mueble, sello, importación) en un solo paso:
    /// sub-cubos (diferencia, o la rejilla entera si cambia de tamaño), partes, interactuables y plantas.
    pub fn apply_external(&mut self, label: &str, result: &Structure) {
        self.end_stroke();
        let mut steps = Vec::new();
        if result.grid.size == self.s.grid.size {
            let (old, new) = (&self.s.grid, &result.grid);
            let det = |g: &VoxelGrid, i: usize| g.detail.as_ref().map_or(0, |d| d[i]);
            let diffs: Vec<_> = (0..old.data.len())
                .filter(|&i| old.data[i] != new.data[i] || det(old, i) != det(new, i))
                .map(|i| (i as u32, old.data[i], new.data[i], det(old, i), det(new, i)))
                .collect();
            if !diffs.is_empty() {
                steps.push(Step::Voxels(diffs));
            }
        } else {
            steps.push(Step::Grid(Box::new(self.s.grid.clone()), Box::new(result.grid.clone())));
        }
        let before = self.extras();
        let after = Extras {
            parts: result.parts.iter().map(|p| Part { cell: None, ..p.clone() }).collect(),
            interactables: result.interactables.clone(),
            band_starts: result.band_starts.clone(),
            band_floors: result.band_floors.clone(),
            ..before.clone()
        };
        if after != before {
            steps.push(Step::Extras(Box::new(before), Box::new(after)));
        }
        if steps.is_empty() {
            return;
        }
        let step = Step::Many(steps);
        self.apply(&step, true);
        self.push(label.into(), step);
    }

    fn push(&mut self, label: String, step: Step) {
        self.undo.push((label, step));
        self.redo.clear();
        self.unsaved = true;
        let mut total: usize = self.undo.iter().map(|(_, s)| s.bytes()).sum();
        while total > HISTORY_BYTES && self.undo.len() > 1 {
            total -= self.undo.remove(0).1.bytes();
        }
    }

    fn apply(&mut self, step: &Step, forward: bool) {
        match step {
            Step::Voxels(diffs) => {
                for &(i, before, after, d_before, d_after) in diffs {
                    let (m, d) = if forward { (after, d_after) } else { (before, d_before) };
                    self.s.grid.data[i as usize] = m;
                    if d != 0 || self.s.grid.detail.is_some() {
                        let len = self.s.grid.data.len();
                        self.s.grid.detail.get_or_insert_with(|| vec![0; len])[i as usize] = d;
                    }
                }
            }
            Step::Extras(before, after) => self.set_extras(if forward { after } else { before }),
            Step::Grid(before, after) => self.s.grid = (**if forward { after } else { before }).clone(),
            Step::Many(steps) => {
                if forward {
                    steps.iter().for_each(|s| self.apply(s, true));
                } else {
                    steps.iter().rev().for_each(|s| self.apply(s, false));
                }
            }
        }
        self.unsaved = true;
        self.version += 1;
    }

    /// Deshace un paso; devuelve su nombre.
    pub fn undo(&mut self) -> Option<String> {
        self.end_stroke();
        let (label, step) = self.undo.pop()?;
        self.apply(&step, false);
        self.redo.push((label.clone(), step));
        Some(label)
    }

    pub fn redo(&mut self) -> Option<String> {
        self.end_stroke();
        let (label, step) = self.redo.pop()?;
        self.apply(&step, true);
        self.undo.push((label.clone(), step));
        Some(label)
    }

    /// Nombres del historial, del más viejo al más nuevo (pestaña «Historial»).
    pub fn history(&self) -> impl Iterator<Item = &str> {
        self.undo.iter().map(|(l, _)| l.as_str())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.stroke.as_ref().is_some_and(|s| !s.diffs.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Doc {
        Doc::new(Meta::default(), Structure { grid: VoxelGrid::new([8, 8, 8]), ..Default::default() }, None)
    }

    #[test]
    fn deshacer_por_trazo() {
        let mut d = doc();
        d.begin_stroke("Lápiz");
        d.paint(1, 1, 1, 3, false);
        d.paint(2, 1, 1, 3, true);
        d.paint(1, 1, 1, 4, false); // repintar dentro del trazo: un solo paso
        d.end_stroke();
        assert_eq!(d.s.grid.get(1, 1, 1), 4);
        assert!(d.s.grid.is_detail(2, 1, 1));
        assert_eq!(d.undo().as_deref(), Some("Lápiz"));
        assert_eq!(d.s.grid.get(1, 1, 1), 0);
        assert!(!d.s.grid.is_detail(2, 1, 1));
        assert!(!d.can_undo());
        d.redo();
        assert_eq!(d.s.grid.get(1, 1, 1), 4);
    }

    #[test]
    fn cambio_externo_en_un_paso() {
        let mut d = doc();
        let mut r = d.s.clone();
        r.grid.set(1, 1, 1, 5);
        r.band_starts = vec![8];
        r.band_floors = vec![1];
        d.apply_external("Habitación", &r);
        assert_eq!(d.s.grid.get(1, 1, 1), 5);
        assert_eq!(d.s.band_floors, vec![1]);
        // Otra que además crece en altura (rejilla nueva entera).
        let mut taller = d.s.clone();
        taller.grid = VoxelGrid::new([8, 16, 8]);
        taller.grid.set(0, 15, 0, 2);
        d.apply_external("Planta nueva", &taller);
        assert_eq!(d.s.grid.size[1], 16);
        assert_eq!(d.history().count(), 2);
        d.undo();
        assert_eq!(d.s.grid.size[1], 8);
        assert_eq!(d.s.grid.get(1, 1, 1), 5);
        d.undo();
        assert_eq!(d.s.grid.get(1, 1, 1), 0);
        assert!(d.s.band_floors.is_empty());
    }

    #[test]
    fn extras_y_rejilla() {
        let mut d = doc();
        d.edit_extras("Entrada", |e| e.entrance = [3, -1]);
        d.replace_grid("Tamaño", VoxelGrid::new([16, 8, 8]));
        assert_eq!(d.s.grid.size[0], 16);
        d.undo();
        assert_eq!(d.s.grid.size[0], 8);
        d.undo();
        assert_eq!(d.s.entrance, [0, 0]);
        d.edit_extras("Nada", |_| {});
        assert_eq!(d.history().count(), 0);
        d.edit_extras("Entrada", |e| e.entrance = [1, -1]);
        d.edit_extras("Entrada", |e| e.entrance = [2, -1]);
        assert_eq!(d.history().count(), 1, "arrastre fundido en un paso");
        d.undo();
        assert_eq!(d.s.entrance, [0, 0]);
        assert!(d.can_redo());
    }
}
