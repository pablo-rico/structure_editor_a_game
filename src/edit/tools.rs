//! Herramientas del modo Editar (§8): qué hace cada una con el puntero ya resuelto a sub-cubos. Lógica pura; la UI
//! traduce el ratón a `Pointer` y dibuja lo que devuelve `preview`.

use super::Doc;
use super::ops::{self, Clip, V};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Erase,
    Box,
    Line,
    Fill,
    Extrude,
    Eyedropper,
    Select,
    Move,
}

impl Tool {
    pub const ALL: [Tool; 9] =
        [Tool::Pencil, Tool::Erase, Tool::Box, Tool::Line, Tool::Fill, Tool::Extrude, Tool::Eyedropper, Tool::Select, Tool::Move];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pencil => "Lápiz",
            Tool::Erase => "Borrar",
            Tool::Box => "Caja",
            Tool::Line => "Línea",
            Tool::Fill => "Relleno",
            Tool::Extrude => "Extruir cara",
            Tool::Eyedropper => "Cuentagotas",
            Tool::Select => "Selección",
            Tool::Move => "Mover",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Pencil => "B",
            Tool::Erase => "E",
            Tool::Box => "U",
            Tool::Line => "L",
            Tool::Fill => "G",
            Tool::Extrude => "X",
            Tool::Eyedropper => "I",
            Tool::Select => "M",
            Tool::Move => "V",
        }
    }

    pub fn icon(self) -> &'static str {
        use egui_phosphor::regular as i;
        match self {
            Tool::Pencil => i::PENCIL_SIMPLE,
            Tool::Erase => i::ERASER,
            Tool::Box => i::CUBE,
            Tool::Line => i::LINE_SEGMENT,
            Tool::Fill => i::PAINT_BUCKET,
            Tool::Extrude => i::ARROW_FAT_LINES_UP,
            Tool::Eyedropper => i::EYEDROPPER,
            Tool::Select => i::SELECTION,
            Tool::Move => i::ARROWS_OUT_CARDINAL,
        }
    }

    /// Pista para la barra de estado.
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pencil => "Clic o arrastra para pintar · Alt+clic: cuentagotas · Coma/punto: tamaño del pincel",
            Tool::Erase => "Clic o arrastra para borrar",
            Tool::Box => "Arrastra de una esquina a otra para rellenar una caja",
            Tool::Line => "Arrastra para trazar una línea",
            Tool::Fill => "Clic: rellena lo que tenga el mismo material (con «Solo plano», solo en esa altura)",
            Tool::Extrude => "Clic en una cara: añade una capa encima de toda la cara del mismo material",
            Tool::Eyedropper => "Clic: coge el material de un sub-cubo",
            Tool::Select => "Arrastra para seleccionar · Cmd+C/X/V/D: copiar, cortar, pegar, duplicar · Shift+X/Z: espejo · Cmd+R: girar",
            Tool::Move => "Arrastra la selección para moverla",
        }
    }
}

/// Puntero ya resuelto por el visor.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pointer {
    /// Sub-cubo señalado y la normal de la cara.
    pub hit: Option<(V, V)>,
    /// Dónde iría un sub-cubo nuevo: junto a la cara señalada, o en el plano bloqueado / el suelo si no hay nada.
    pub place: Option<V>,
    pub pressed: bool,
    pub down: bool,
    pub released: bool,
    pub alt: bool,
}

/// Lo que hay que dibujar encima de la estructura.
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    Ghost { cells: Vec<V>, erase: bool },
    Selection { min: V, max: V },
}

struct Moving {
    clip: Clip,
    from: V,
    origin: V,
}

pub struct Tools {
    pub tool: Tool,
    pub material: u8,
    /// Lado del pincel: 1, 2, 4 u 8 (celda).
    pub brush: i32,
    pub mirror_x: bool,
    pub mirror_z: bool,
    pub detail: bool,
    pub selection: Option<(V, V)>,
    pub clipboard: Option<Clip>,
    anchor: Option<V>,
    moving: Option<Moving>,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            tool: Tool::Pencil,
            material: 1,
            brush: 1,
            mirror_x: false,
            mirror_z: false,
            detail: false,
            selection: None,
            clipboard: None,
            anchor: None,
            moving: None,
        }
    }
}

fn bounds(a: V, b: V) -> (V, V) {
    ([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])])
}

impl Tools {
    pub fn brush_up(&mut self) {
        self.brush = (self.brush * 2).min(8);
    }

    pub fn brush_down(&mut self) {
        self.brush = (self.brush / 2).max(1);
    }

    fn with_mirror(&self, doc: &Doc, cells: Vec<V>) -> Vec<V> {
        if !self.mirror_x && !self.mirror_z {
            return cells;
        }
        let mut out: Vec<V> =
            cells.iter().flat_map(|&v| ops::mirrored(v, doc.s.grid.size, self.mirror_x, self.mirror_z)).collect();
        out.sort();
        out.dedup();
        out
    }

    fn paint_all(&self, doc: &mut Doc, cells: &[V], material: u8) {
        for v in cells {
            doc.paint(v[0], v[1], v[2], material, self.detail);
        }
    }

    /// Qué sub-cubos tocaría la herramienta ahora (vista previa).
    pub fn preview(&self, doc: &Doc, p: &Pointer) -> Vec<Preview> {
        let mut out = Vec::new();
        if let Some((a, b)) = self.selection {
            let (min, max) = bounds(a, b);
            out.push(Preview::Selection { min, max });
        }
        let place = p.place;
        let hit = p.hit.map(|h| h.0);
        let ghost = match self.tool {
            Tool::Pencil => place.map(|v| ops::brush(v, self.brush)),
            Tool::Erase => hit.map(|v| ops::brush(v, self.brush)),
            Tool::Box => place.map(|v| ops::cuboid(self.anchor.unwrap_or(v), v)),
            Tool::Line => place.map(|v| ops::line(self.anchor.unwrap_or(v), v)),
            Tool::Extrude => p.hit.map(|(v, n)| ops::extrude(&doc.s.grid, v, n)),
            Tool::Select => match (self.anchor, hit.or(place)) {
                (Some(a), Some(v)) => {
                    let (min, max) = bounds(a, v);
                    out.push(Preview::Selection { min, max });
                    None
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(cells) = ghost.filter(|c| !c.is_empty()) {
            let erase = self.tool == Tool::Erase;
            out.push(Preview::Ghost { cells: self.with_mirror(doc, cells), erase });
        }
        out
    }

    /// Aplica la herramienta a este fotograma del puntero. Devuelve el material cogido con el cuentagotas, si lo hay.
    pub fn step(&mut self, doc: &mut Doc, p: &Pointer) -> Option<u8> {
        let tool = if p.alt { Tool::Eyedropper } else { self.tool };
        match tool {
            Tool::Eyedropper => {
                if p.pressed
                    && let Some((v, _)) = p.hit
                {
                    let m = doc.s.grid.get(v[0], v[1], v[2]);
                    self.material = m;
                    return Some(m);
                }
            }
            Tool::Pencil | Tool::Erase => {
                let (target, material, label) = match tool {
                    Tool::Pencil => (p.place, self.material, "Lápiz"),
                    _ => (p.hit.map(|h| h.0), 0, "Borrar"),
                };
                if p.pressed {
                    doc.begin_stroke(label);
                }
                if p.down
                    && let Some(v) = target
                {
                    let cells = self.with_mirror(doc, ops::brush(v, self.brush));
                    self.paint_all(doc, &cells, material);
                }
                if p.released {
                    doc.end_stroke();
                }
            }
            Tool::Box | Tool::Line => {
                if p.pressed {
                    self.anchor = p.place;
                }
                if p.released {
                    if let (Some(a), Some(b)) = (self.anchor, p.place) {
                        let cells = if tool == Tool::Box { ops::cuboid(a, b) } else { ops::line(a, b) };
                        let cells = self.with_mirror(doc, cells);
                        doc.begin_stroke(tool.label());
                        self.paint_all(doc, &cells, self.material);
                        doc.end_stroke();
                    }
                    self.anchor = None;
                }
            }
            Tool::Fill => {
                if p.pressed
                    && let Some((v, _)) = p.hit
                {
                    let cells = ops::flood(&doc.s.grid, v, None, 2_000_000);
                    doc.begin_stroke("Relleno");
                    self.paint_all(doc, &cells, self.material);
                    doc.end_stroke();
                }
            }
            Tool::Extrude => {
                if p.pressed
                    && let Some((v, n)) = p.hit
                {
                    let m = doc.s.grid.get(v[0], v[1], v[2]);
                    let cells = ops::extrude(&doc.s.grid, v, n);
                    doc.begin_stroke("Extruir cara");
                    for c in cells {
                        doc.paint(c[0], c[1], c[2], m, doc.s.grid.is_detail(v[0], v[1], v[2]));
                    }
                    doc.end_stroke();
                }
            }
            Tool::Select => {
                let at = p.hit.map(|h| h.0).or(p.place);
                if p.pressed {
                    self.anchor = at;
                }
                if p.released {
                    self.selection = match (self.anchor, at) {
                        (Some(a), Some(b)) => Some(bounds(a, b)),
                        _ => None,
                    };
                    self.anchor = None;
                }
            }
            Tool::Move => self.step_move(doc, p),
        }
        None
    }

    fn step_move(&mut self, doc: &mut Doc, p: &Pointer) {
        let at = p.hit.map(|h| h.0).or(p.place);
        if p.pressed
            && let (Some((min, max)), Some(from)) = (self.selection, at)
        {
            let clip = Clip::copy(&doc.s.grid, min, max);
            doc.begin_stroke("Mover");
            self.paint_all(doc, &ops::cuboid(min, max), 0);
            self.moving = Some(Moving { clip, from, origin: min });
        }
        if p.released
            && let Some(m) = self.moving.take()
        {
            let to = at.unwrap_or(m.from);
            // Se mueve en el plano: la altura solo cambia con las flechas (como en el juego, el suelo manda).
            let delta = [to[0] - m.from[0], 0, to[2] - m.from[2]];
            let dest = [m.origin[0] + delta[0], m.origin[1], m.origin[2] + delta[2]];
            self.paste_clip(doc, &m.clip, dest);
            doc.end_stroke();
            let size = [m.clip.size[0] - 1, m.clip.size[1] - 1, m.clip.size[2] - 1];
            self.selection = Some((dest, [dest[0] + size[0], dest[1] + size[1], dest[2] + size[2]]));
        }
    }

    fn paste_clip(&self, doc: &mut Doc, clip: &Clip, at: V) {
        for (v, m, detail) in clip.solids() {
            doc.paint(at[0] + v[0], at[1] + v[1], at[2] + v[2], m, detail);
        }
    }

    pub fn copy(&mut self, doc: &Doc) -> bool {
        let Some((a, b)) = self.selection else { return false };
        self.clipboard = Some(Clip::copy(&doc.s.grid, a, b));
        true
    }

    pub fn cut(&mut self, doc: &mut Doc) -> bool {
        if !self.copy(doc) {
            return false;
        }
        let (a, b) = self.selection.unwrap();
        doc.begin_stroke("Cortar");
        self.paint_all(doc, &ops::cuboid(a, b), 0);
        doc.end_stroke();
        true
    }

    /// Pega el portapapeles con su esquina en `at` y lo deja seleccionado.
    pub fn paste(&mut self, doc: &mut Doc, at: V) -> bool {
        let Some(clip) = self.clipboard.clone() else { return false };
        doc.begin_stroke("Pegar");
        self.paste_clip(doc, &clip, at);
        doc.end_stroke();
        self.selection = Some((at, [at[0] + clip.size[0] - 1, at[1] + clip.size[1] - 1, at[2] + clip.size[2] - 1]));
        true
    }

    /// Transforma la selección en su sitio (espejo o giro), anclada en su esquina mínima.
    pub fn transform(&mut self, doc: &mut Doc, label: &str, f: impl Fn(&Clip) -> Clip) -> bool {
        let Some((a, b)) = self.selection else { return false };
        let clip = f(&Clip::copy(&doc.s.grid, a, b));
        doc.begin_stroke(label);
        self.paint_all(doc, &ops::cuboid(a, b), 0);
        self.paste_clip(doc, &clip, a);
        doc.end_stroke();
        self.selection = Some((a, [a[0] + clip.size[0] - 1, a[1] + clip.size[1] - 1, a[2] + clip.size[2] - 1]));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agvox::Meta;
    use crate::model::{Structure, VoxelGrid};

    fn doc() -> Doc {
        Doc::new(Meta::default(), Structure { grid: VoxelGrid::new([16, 16, 16]), ..Default::default() }, None)
    }

    fn click(place: V) -> Pointer {
        Pointer { place: Some(place), hit: Some((place, [0, 1, 0])), pressed: true, down: true, released: true, alt: false }
    }

    #[test]
    fn lapiz_con_espejo_y_deshacer() {
        let mut d = doc();
        let mut t = Tools { material: 3, mirror_x: true, ..Default::default() };
        t.step(&mut d, &click([1, 0, 2]));
        assert_eq!(d.s.grid.get(1, 0, 2), 3);
        assert_eq!(d.s.grid.get(14, 0, 2), 3);
        d.undo();
        assert_eq!(d.s.grid.get(14, 0, 2), 0);
    }

    #[test]
    fn caja_seleccion_y_mover() {
        let mut d = doc();
        let mut t = Tools { tool: Tool::Box, material: 2, ..Default::default() };
        t.step(&mut d, &Pointer { place: Some([0, 0, 0]), pressed: true, down: true, ..Default::default() });
        t.step(&mut d, &Pointer { place: Some([1, 0, 1]), down: true, released: true, ..Default::default() });
        assert_eq!(d.s.grid.get(1, 0, 1), 2);
        t.tool = Tool::Select;
        t.step(&mut d, &Pointer { hit: Some(([0, 0, 0], [0, 1, 0])), pressed: true, ..Default::default() });
        t.step(&mut d, &Pointer { hit: Some(([1, 0, 1], [0, 1, 0])), released: true, ..Default::default() });
        assert_eq!(t.selection, Some(([0, 0, 0], [1, 0, 1])));
        t.tool = Tool::Move;
        t.step(&mut d, &Pointer { hit: Some(([0, 0, 0], [0, 1, 0])), pressed: true, ..Default::default() });
        t.step(&mut d, &Pointer { place: Some([5, 0, 3]), released: true, ..Default::default() });
        assert_eq!(d.s.grid.get(0, 0, 0), 0);
        assert_eq!(d.s.grid.get(6, 0, 4), 2);
        assert_eq!(t.selection, Some(([5, 0, 3], [6, 0, 4])));
        d.undo(); // mover se deshace de una vez
        assert_eq!(d.s.grid.get(0, 0, 0), 2);
        assert_eq!(d.s.grid.get(6, 0, 4), 0);
    }

    #[test]
    fn cuentagotas_con_alt() {
        let mut d = doc();
        d.paint(2, 2, 2, 9, false);
        let mut t = Tools::default();
        let p = Pointer { hit: Some(([2, 2, 2], [0, 1, 0])), pressed: true, alt: true, ..Default::default() };
        assert_eq!(t.step(&mut d, &p), Some(9));
        assert_eq!(t.material, 9);
        assert_eq!(d.history().count(), 0);
    }
}
