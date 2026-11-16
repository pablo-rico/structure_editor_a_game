//! Modo Editar (fase 2): el documento que se edita, su historial (deshacer infinito con diferencias dispersas
//! agrupadas por trazo) y las operaciones de las herramientas (`ops`).

pub mod ops;
pub mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::agvox::Meta;
use crate::bridge::proto::{Interactable, Part};
use crate::model::{FaceColors, Structure, VoxelGrid, face_key, face_unkey};
use crate::vmodel::Model;

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
    /// Modelos: `(clave de cara, color propio antes, después)`; 0 = sin color propio.
    Faces(Vec<(u32, u8, u8)>),
    /// Modelos: paleta o punto de apoyo.
    Model(Box<Model>, Box<Model>),
    /// Modelos: todos los colores propios de cara de golpe (cambio de tamaño).
    AllFaces(Box<FaceColors>, Box<FaceColors>),
}

impl Step {
    fn bytes(&self) -> usize {
        match self {
            Step::Voxels(v) => v.len() * 8,
            Step::Extras(..) => 1024,
            Step::Grid(a, b) => a.data.len() * 2 + b.data.len() * 2,
            Step::Many(steps) => steps.iter().map(Step::bytes).sum(),
            Step::Faces(v) => v.len() * 8,
            Step::Model(..) => 64 << 10,
            Step::AllFaces(a, b) => (a.len() + b.len()) * 8,
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
    /// `Some` si lo abierto es un modelo de vóxeles del juego (`vmodel`) y no una estructura.
    pub model: Option<Model>,
}

/// Trazo en curso: diferencias y dónde está la de cada sub-cubo (repintar dentro del trazo no crece la lista).
struct Stroke {
    label: String,
    diffs: Vec<(u32, u8, u8, u8, u8)>,
    at: HashMap<u32, usize>,
    /// Colores propios de cara tocados en el trazo (modelos) y dónde está cada uno.
    faces: Vec<(u32, u8, u8)>,
    face_at: HashMap<u32, usize>,
}

impl Doc {
    pub fn new(meta: Meta, s: Structure, path: Option<PathBuf>) -> Self {
        Self {
            meta,
            s,
            path,
            unsaved: false,
            version: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_extras: None,
            stroke: None,
            model: None,
        }
    }

    /// Documento de un modelo de vóxeles del juego; se llama como su fichero sin `_model.json`.
    pub fn new_model(model: Model, s: Structure, path: Option<PathBuf>) -> Self {
        let name = path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().trim_end_matches(".json").trim_end_matches("_model").to_string())
            .unwrap_or_else(|| "Modelo nuevo".into());
        let mut doc = Self::new(Meta { name, ..Default::default() }, s, path);
        doc.model = Some(model);
        doc
    }

    pub fn is_model(&self) -> bool {
        self.model.is_some()
    }

    /// Color (byte de rejilla) de la cara `dir` del vóxel `v`: el propio o el de su vóxel.
    pub fn face_color(&self, v: [i32; 3], dir: u8) -> u8 {
        self.s.face_colors.get(&face_key(v, dir)).copied().unwrap_or_else(|| self.s.grid.get(v[0], v[1], v[2]))
    }

    /// Pone (o quita, con 0) el color propio de una cara dentro del trazo en curso.
    fn set_face(&mut self, key: u32, color: u8) {
        let old = self.s.face_colors.get(&key).copied().unwrap_or(0);
        if old == color {
            return;
        }
        let Some(st) = self.stroke.as_mut() else { return };
        match st.face_at.get(&key) {
            Some(&k) => st.faces[k].2 = color,
            None => {
                st.face_at.insert(key, st.faces.len());
                st.faces.push((key, old, color));
            }
        }
        if color == 0 {
            self.s.face_colors.remove(&key);
        } else {
            self.s.face_colors.insert(key, color);
        }
        self.version += 1;
        self.unsaved = true;
    }

    /// Modelos: pinta solo la cara `dir` del vóxel `v` (sombreado dibujado). Con el color de su vóxel, o 0, la cara
    /// deja de tener color propio. Dentro del trazo en curso (abre uno si no hay).
    pub fn paint_face(&mut self, v: [i32; 3], dir: u8, color: u8) {
        let base = self.s.grid.get(v[0], v[1], v[2]);
        if base == 0 || dir > 5 {
            return;
        }
        if self.stroke.is_none() {
            self.begin_stroke("Pintar");
        }
        self.set_face(face_key(v, dir), if color == base { 0 } else { color });
    }

    /// Modelos: cambia la paleta o el punto de apoyo con un paso de historial.
    pub fn edit_model(&mut self, label: &str, f: impl FnOnce(&mut Model)) {
        self.end_stroke();
        let Some(before) = self.model.clone() else { return };
        let mut after = before.clone();
        f(&mut after);
        if after == before {
            return;
        }
        self.model = Some(after.clone());
        self.version += 1;
        self.push(label.into(), Step::Model(Box::new(before), Box::new(after)));
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
        self.stroke = Some(Stroke {
            label: label.to_string(),
            diffs: Vec::new(),
            at: HashMap::new(),
            faces: Vec::new(),
            face_at: HashMap::new(),
        });
    }

    /// Pinta un sub-cubo dentro del trazo en curso (abre uno si no hay). `detail` marca o desmarca el detalle.
    pub fn paint(&mut self, x: i32, y: i32, z: i32, material: u8, detail: bool) {
        if !self.s.grid.in_bounds(x, y, z) {
            return;
        }
        if self.stroke.is_none() {
            self.begin_stroke("Pintar");
        }
        // Pintar o borrar un vóxel de un modelo le quita el sombreado dibujado: sus caras vuelven a ser de su color.
        if !self.s.face_colors.is_empty() {
            for d in 0..6 {
                self.set_face(face_key([x, y, z], d), 0);
            }
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
        let Some(Stroke { label, mut diffs, mut faces, .. }) = self.stroke.take() else { return };
        diffs.retain(|d| d.1 != d.2 || d.3 != d.4);
        faces.retain(|f| f.1 != f.2);
        match (diffs.is_empty(), faces.is_empty()) {
            (true, true) => {}
            (false, true) => self.push(label, Step::Voxels(diffs)),
            (true, false) => self.push(label, Step::Faces(faces)),
            (false, false) => self.push(label, Step::Many(vec![Step::Voxels(diffs), Step::Faces(faces)])),
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

    /// Modelos: cambia el tamaño creciendo o encogiendo por +x, +y, +z, sin mover nada de lo que hay (el juego tiene
    /// posiciones en vóxeles apuntadas fuera del fichero). `false` si no cabe lo ocupado o el tamaño no vale.
    pub fn resize_model(&mut self, size: [i32; 3]) -> bool {
        self.end_stroke();
        let old = &self.s.grid;
        let mut used = [0; 3];
        for (i, _) in old.data.iter().enumerate().filter(|(_, b)| **b != 0) {
            let i = i as i32;
            let v = [i % old.size[0], i / (old.size[0] * old.size[2]), i / old.size[0] % old.size[2]];
            used = std::array::from_fn(|k| used[k].max(v[k] + 1));
        }
        let fits = (0..3).all(|k| size[k] >= used[k].max(1) && size[k] <= crate::vmodel::MAX_SIDE);
        if !fits || size == old.size {
            return false;
        }
        let mut grid = VoxelGrid::new(size);
        for y in 0..used[1] {
            for z in 0..used[2] {
                for x in 0..used[0] {
                    grid.set(x, y, z, old.get(x, y, z));
                }
            }
        }
        let mut faces = self.s.face_colors.clone();
        faces.retain(|&k, _| {
            let v = face_unkey(k).0;
            grid.in_bounds(v[0], v[1], v[2])
        });
        let mut steps = vec![Step::Grid(Box::new(old.clone()), Box::new(grid))];
        if faces != self.s.face_colors {
            steps.push(Step::AllFaces(Box::new(self.s.face_colors.clone()), Box::new(faces)));
        }
        let step = Step::Many(steps);
        self.apply(&step, true);
        self.push("Tamaño".into(), step);
        true
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
            Step::Faces(faces) => {
                for &(key, before, after) in faces {
                    match if forward { after } else { before } {
                        0 => self.s.face_colors.remove(&key),
                        c => self.s.face_colors.insert(key, c),
                    };
                }
            }
            Step::Model(before, after) => self.model = Some((**if forward { after } else { before }).clone()),
            Step::AllFaces(before, after) => self.s.face_colors = (**if forward { after } else { before }).clone(),
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

    fn chest() -> Doc {
        let (m, s) = crate::vmodel::load(&crate::vmodel::tests::lf(crate::vmodel::tests::CHEST)).unwrap();
        Doc::new_model(m, s, None)
    }

    /// Un vóxel del cofre con alguna cara de color propio.
    fn own_voxel(d: &Doc) -> [i32; 3] {
        face_unkey(*d.s.face_colors.keys().min().unwrap()).0
    }

    fn own_of(d: &Doc, v: [i32; 3]) -> usize {
        d.s.face_colors.keys().filter(|&&k| face_unkey(k).0 == v).count()
    }

    #[test]
    fn pintar_voxel_borra_sus_caras_propias() {
        let mut d = chest();
        let v = own_voxel(&d);
        let (total, mine) = (d.s.face_colors.len(), own_of(&d, v));
        assert!(mine > 0);
        // Pintar con su mismo color también lo deja liso.
        d.paint(v[0], v[1], v[2], d.s.grid.get(v[0], v[1], v[2]), false);
        d.end_stroke();
        assert_eq!(own_of(&d, v), 0);
        assert_eq!(d.s.face_colors.len(), total - mine, "las de los vecinos siguen");
    }

    #[test]
    fn deshacer_restaura_colores_de_cara() {
        let mut d = chest();
        let before = d.s.face_colors.clone();
        let v = own_voxel(&d);
        d.begin_stroke("Pintar");
        d.paint(v[0], v[1], v[2], 1, false);
        d.end_stroke();
        assert_ne!(d.s.face_colors, before);
        assert_eq!(d.undo().as_deref(), Some("Pintar"));
        assert_eq!(d.s.face_colors, before);
        d.redo();
        assert_eq!(own_of(&d, v), 0);
        // Borrar el vóxel y deshacer: vuelve con su sombreado.
        d.undo();
        d.paint(v[0], v[1], v[2], 0, false);
        d.end_stroke();
        d.undo();
        assert_eq!(d.s.face_colors, before);
        assert!(!d.can_undo());
    }

    #[test]
    fn pintar_cara_y_deshacer() {
        let mut d = chest();
        let v = own_voxel(&d);
        let base = d.s.grid.get(v[0], v[1], v[2]);
        let dir = (0..6u8).find(|&x| !d.s.face_colors.contains_key(&face_key(v, x))).unwrap();
        let other = if base == 1 { 2 } else { 1 };
        assert_eq!(d.face_color(v, dir), base);
        d.paint_face(v, dir, other);
        d.end_stroke();
        assert_eq!(d.face_color(v, dir), other);
        assert_eq!(d.s.grid.get(v[0], v[1], v[2]), base, "el vóxel no cambia");
        d.undo();
        assert_eq!(d.face_color(v, dir), base);
        // Pintar una cara con el color de su vóxel le quita el color propio.
        let own = (0..6u8).find(|&x| d.s.face_colors.contains_key(&face_key(v, x))).unwrap();
        d.paint_face(v, own, base);
        d.end_stroke();
        assert!(!d.s.face_colors.contains_key(&face_key(v, own)));
        // En el aire no pinta nada.
        let n = d.history().count();
        d.paint_face([0, d.s.grid.size[1] - 1, 0], 2, other);
        d.end_stroke();
        assert_eq!(d.history().count(), n);
    }

    #[test]
    fn encoger_no_deja_caras_huerfanas() {
        let mut d = chest();
        let size = d.s.grid.size;
        let (grid, faces) = (d.s.grid.clone(), d.s.face_colors.clone());
        assert!(!d.resize_model([size[0] - 1, size[1], size[2]]), "el cofre ocupa todo su ancho");
        assert!(!d.resize_model([size[0], 300, size[2]]));
        assert!(!d.resize_model(size), "el mismo tamaño no es un cambio");
        assert!(!d.can_undo());
        // Una cara con color propio en una zona que luego se recorta (el vóxel ya no está).
        assert!(d.resize_model([size[0] + 4, size[1], size[2]]));
        d.s.face_colors.insert(face_key([size[0] + 2, 0, 0], 2), 1);
        assert!(d.resize_model(size));
        assert!(d.s.face_colors.keys().all(|&k| {
            let v = face_unkey(k).0;
            d.s.grid.in_bounds(v[0], v[1], v[2])
        }));
        assert_eq!(d.s.grid, grid);
        assert_eq!(d.s.face_colors, faces);
        d.undo();
        assert_eq!(d.s.grid.size[0], size[0] + 4);
        d.undo();
        assert_eq!(d.s.grid.size, size);
    }

    #[test]
    fn crecer_no_mueve_nada() {
        let mut d = chest();
        let old = d.s.grid.clone();
        let size = old.size;
        assert!(d.resize_model([size[0] + 3, size[1] + 2, size[2] + 5]));
        for y in 0..size[1] + 2 {
            for z in 0..size[2] + 5 {
                for x in 0..size[0] + 3 {
                    assert_eq!(d.s.grid.get(x, y, z), old.get(x, y, z));
                }
            }
        }
        assert_eq!(d.undo().as_deref(), Some("Tamaño"));
        assert_eq!(d.s.grid, old);
    }

    #[test]
    fn editar_paleta_se_deshace() {
        let mut d = chest();
        let old = d.model.as_ref().unwrap().palette[0];
        d.edit_model("Cambiar color", |m| m.palette[0] = [1, 2, 3]);
        assert_eq!(d.model.as_ref().unwrap().palette[0], [1, 2, 3]);
        assert!(d.unsaved);
        assert_eq!(d.undo().as_deref(), Some("Cambiar color"));
        assert_eq!(d.model.as_ref().unwrap().palette[0], old);
        d.edit_model("Nada", |_| {});
        assert!(!d.can_undo());
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
