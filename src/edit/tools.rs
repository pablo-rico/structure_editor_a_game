//! Herramientas del modo Editar (docs/diseno-gui.md §8 y §10): qué hace cada una con el puntero ya resuelto a
//! sub-cubos. Lógica pura; la UI traduce el ratón a `Pointer` y dibuja lo que devuelve `preview`. Las herramientas de
//! construcción (Habitación, Tejado…) las hace el generador del juego: aquí solo se marca que hay una activa.

use super::Doc;
use super::ops::{self, BoxShape, Clip, V};
use crate::bridge::proto::Interactable;
use crate::model::Structure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Box,
    Line,
    Fill,
    Extrude,
    Eyedropper,
    Select,
    Move,
    /// Colocar lo que hay en la mano (mueble o sello).
    Place,
    /// Herramienta de construcción del juego (`Tools::build`); la aplica la app por el generador.
    Build,
}

/// Qué hacen Lápiz, Caja, Línea, Relleno y Extruir: poner, recolorear o quitar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Add,
    Paint,
    Erase,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Add => "Añadir",
            Mode::Paint => "Pintar",
            Mode::Erase => "Borrar",
        }
    }

    /// Lo que se ve en la barra de estado mientras está activo.
    pub fn status(self) -> &'static str {
        match self {
            Mode::Add => "Añadiendo",
            Mode::Paint => "Pintando",
            Mode::Erase => "Borrando",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            Mode::Add => "Pone sub-cubos junto a la cara que señalas (A).",
            Mode::Paint => "Cambia el material de lo que ya hay, sin añadir ni quitar (N).",
            Mode::Erase => "Quita sub-cubos (E).",
        }
    }
}

impl Tool {
    /// Las básicas, en el orden de la tira (§10).
    pub const BASIC: [Tool; 8] =
        [Tool::Pencil, Tool::Box, Tool::Line, Tool::Fill, Tool::Extrude, Tool::Eyedropper, Tool::Select, Tool::Move];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pencil => "Lápiz",
            Tool::Box => "Caja",
            Tool::Line => "Línea",
            Tool::Fill => "Relleno",
            Tool::Extrude => "Extruir cara",
            Tool::Eyedropper => "Cuentagotas",
            Tool::Select => "Selección",
            Tool::Move => "Mover",
            Tool::Place => "Colocar",
            Tool::Build => "Construir",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Pencil => "B",
            Tool::Box => "U",
            Tool::Line => "L",
            Tool::Fill => "G",
            Tool::Extrude => "X",
            Tool::Eyedropper => "I",
            Tool::Select => "M",
            Tool::Move => "V",
            Tool::Place | Tool::Build => "",
        }
    }

    pub fn icon(self) -> &'static str {
        use egui_phosphor::regular as i;
        match self {
            Tool::Pencil => i::PENCIL_SIMPLE,
            Tool::Box => i::CUBE,
            Tool::Line => i::LINE_SEGMENT,
            Tool::Fill => i::PAINT_BUCKET,
            Tool::Extrude => i::ARROW_FAT_LINES_UP,
            Tool::Eyedropper => i::EYEDROPPER,
            Tool::Select => i::SELECTION,
            Tool::Move => i::ARROWS_OUT_CARDINAL,
            Tool::Place => i::COUCH,
            Tool::Build => i::HOUSE_LINE,
        }
    }

    /// Las que obedecen al modo Añadir / Pintar / Borrar.
    pub fn uses_mode(self) -> bool {
        matches!(self, Tool::Pencil | Tool::Box | Tool::Line | Tool::Fill | Tool::Extrude)
    }

    /// Pista para la barra de estado.
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pencil => "Clic o arrastra · Alt+clic: cuentagotas · Coma/punto: tamaño del pincel",
            Tool::Box => "Arrastra de una esquina a otra · Llena, hueca o solo paredes",
            Tool::Line => "Arrastra para trazar una línea",
            Tool::Fill => "Clic: todo lo unido del mismo material (hasta esta planta o toda)",
            Tool::Extrude => "Clic en una cara: una capa más (o menos) en toda la cara del mismo material",
            Tool::Eyedropper => "Clic: coge el material de un sub-cubo",
            Tool::Select => "Arrastra para seleccionar · Flechas: mover · Cmd+C/X/V/D · Shift+X/Z: espejo · Cmd+R: girar",
            Tool::Move => "Arrastra la selección para moverla · Flechas: de sub-cubo en sub-cubo (Shift: celda)",
            Tool::Place => "Clic donde va · O: girar · Esc: soltar la pieza",
            Tool::Build => "Clic o arrastra según la herramienta · Suelta para construir · Esc cancela",
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

/// Pieza en la mano (mueble del juego o sello propio): sub-cubos, interactuables relativos a la pieza y el sub-cubo
/// de apoyo (el que cae en el sitio señalado).
#[derive(Clone, Debug, PartialEq)]
pub struct Hand {
    pub name: String,
    pub clip: Clip,
    pub interactables: Vec<Interactable>,
    pub anchor: V,
    /// Mueble del juego (se pide girado al generador) o sello (se gira aquí).
    pub furniture: Option<String>,
    pub facing: i32,
}

impl Hand {
    /// Un cuarto de vuelta, con la regla del juego (sub-cubos, cajas e interactuables).
    pub fn rotate(&mut self) {
        let sz = self.clip.size[2];
        self.clip = self.clip.rotate_quarter();
        for it in &mut self.interactables {
            it.bbox = ops::rotate_box(it.bbox, sz);
            it.facing = ops::rotate_dir(it.facing);
        }
        self.anchor = [sz - 1 - self.anchor[2], self.anchor[1], self.anchor[0]];
        self.facing = ops::rotate_dir(self.facing);
    }

    /// Dónde cae la esquina de la pieza si su apoyo va en `at`.
    pub fn origin(&self, at: V) -> V {
        [at[0] - self.anchor[0], at[1] - self.anchor[1], at[2] - self.anchor[2]]
    }

    /// La estructura con la pieza puesta en `at` (para `Doc::apply_external`: un solo paso de deshacer).
    pub fn placed(&self, s: &Structure, at: V, detail: bool) -> Structure {
        let mut out = s.clone();
        let o = self.origin(at);
        for (v, m, d) in self.clip.solids() {
            let (x, y, z) = (o[0] + v[0], o[1] + v[1], o[2] + v[2]);
            if out.grid.in_bounds(x, y, z) {
                out.grid.set(x, y, z, m);
                let i = out.grid.index(x, y, z);
                if d || detail || out.grid.detail.is_some() {
                    let len = out.grid.data.len();
                    out.grid.detail.get_or_insert_with(|| vec![0; len])[i] = (d || detail) as u8;
                }
            }
        }
        for it in &self.interactables {
            let b = it.bbox;
            let moved = [b[0] + o[0], b[1] + o[1], b[2] + o[2], b[3] + o[0], b[4] + o[1], b[5] + o[2]];
            out.interactables.push(Interactable { bbox: moved, ..it.clone() });
        }
        out
    }

    /// Sub-cubos que ocuparía (vista previa).
    pub fn cells(&self, at: V) -> Vec<V> {
        let o = self.origin(at);
        self.clip.solids().map(|(v, _, _)| [o[0] + v[0], o[1] + v[1], o[2] + v[2]]).collect()
    }
}

struct Moving {
    clip: Clip,
    from: V,
    origin: V,
}

pub struct Tools {
    pub tool: Tool,
    pub mode: Mode,
    pub material: u8,
    /// Lado del pincel: 1, 2, 4 u 8 (celda).
    pub brush: i32,
    pub mirror_x: bool,
    pub mirror_z: bool,
    pub detail: bool,
    pub box_shape: BoxShape,
    /// Relleno hasta esta planta (alturas incluidas); `None` = toda la estructura.
    pub fill_range: Option<(i32, i32)>,
    pub selection: Option<(V, V)>,
    pub clipboard: Option<Clip>,
    pub hand: Option<Hand>,
    /// Herramienta de construcción activa (id del catálogo `tools` del generador).
    pub build: String,
    anchor: Option<V>,
    moving: Option<Moving>,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            tool: Tool::Pencil,
            mode: Mode::Add,
            material: 1,
            brush: 1,
            mirror_x: false,
            mirror_z: false,
            detail: false,
            box_shape: BoxShape::Solid,
            fill_range: None,
            selection: None,
            clipboard: None,
            hand: None,
            build: String::new(),
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

    /// Cambia de modo; con una herramienta que no lo usa, pasa al Lápiz (§10, «A / N / E»).
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if !self.tool.uses_mode() {
            self.tool = Tool::Pencil;
        }
    }

    pub fn cancel(&mut self) {
        self.anchor = None;
        self.moving = None;
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

    /// Material con que pinta el modo: el elegido, o aire al borrar.
    fn ink(&self) -> u8 {
        if self.mode == Mode::Erase { 0 } else { self.material }
    }

    /// Aplica el modo a unos sub-cubos: Añadir y Borrar los ponen tal cual; Pintar solo cambia los que no son aire.
    fn apply_cells(&self, doc: &mut Doc, cells: &[V]) {
        let m = self.ink();
        for v in cells {
            if self.mode == Mode::Paint && doc.s.grid.get(v[0], v[1], v[2]) == 0 {
                continue;
            }
            let detail = if self.mode == Mode::Paint { doc.s.grid.is_detail(v[0], v[1], v[2]) || self.detail } else { self.detail };
            doc.paint(v[0], v[1], v[2], m, detail);
        }
    }

    /// Sitio de trabajo según el modo: Añadir pone junto a la cara; Pintar y Borrar actúan sobre lo señalado.
    fn target(&self, p: &Pointer) -> Option<V> {
        match self.mode {
            Mode::Add => p.place,
            Mode::Paint | Mode::Erase => p.hit.map(|h| h.0),
        }
    }

    fn shape_cells(&self, a: V, b: V) -> Vec<V> {
        match self.tool {
            Tool::Line => ops::line(a, b),
            _ => ops::shaped_box(a, b, self.box_shape),
        }
    }

    /// Qué sub-cubos tocaría la herramienta ahora (vista previa).
    pub fn preview(&self, doc: &Doc, p: &Pointer) -> Vec<Preview> {
        let mut out = Vec::new();
        if let Some((a, b)) = self.selection {
            let (min, max) = bounds(a, b);
            out.push(Preview::Selection { min, max });
        }
        let erase = self.mode == Mode::Erase;
        let ghost = match self.tool {
            Tool::Pencil => self.target(p).map(|v| ops::brush(v, self.brush)),
            Tool::Box | Tool::Line => self.target(p).map(|v| self.shape_cells(self.anchor.unwrap_or(v), v)),
            Tool::Extrude => p.hit.map(|(v, n)| match self.mode {
                Mode::Add => ops::extrude(&doc.s.grid, v, n),
                _ => ops::extrude(&doc.s.grid, v, n).iter().map(|c| [c[0] - n[0], c[1] - n[1], c[2] - n[2]]).collect(),
            }),
            Tool::Place => match (&self.hand, p.place) {
                (Some(h), Some(at)) => Some(h.cells(at)),
                _ => None,
            },
            Tool::Select => {
                if let (Some(a), Some(v)) = (self.anchor, p.hit.map(|h| h.0).or(p.place)) {
                    let (min, max) = bounds(a, v);
                    out.push(Preview::Selection { min, max });
                }
                None
            }
            _ => None,
        };
        if let Some(cells) = ghost.filter(|c| !c.is_empty()) {
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
            Tool::Pencil => {
                if p.pressed {
                    doc.begin_stroke(self.mode.label());
                }
                if p.down
                    && let Some(v) = self.target(p)
                {
                    let cells = self.with_mirror(doc, ops::brush(v, self.brush));
                    self.apply_cells(doc, &cells);
                }
                if p.released {
                    doc.end_stroke();
                }
            }
            Tool::Box | Tool::Line => {
                if p.pressed {
                    self.anchor = self.target(p);
                }
                if p.released {
                    if let (Some(a), Some(b)) = (self.anchor, self.target(p)) {
                        let cells = self.with_mirror(doc, self.shape_cells(a, b));
                        doc.begin_stroke(tool.label());
                        self.apply_cells(doc, &cells);
                        doc.end_stroke();
                    }
                    self.anchor = None;
                }
            }
            Tool::Fill => {
                if p.pressed
                    && let Some((v, _)) = p.hit
                {
                    let cells = ops::flood(&doc.s.grid, v, self.fill_range, 2_000_000);
                    doc.begin_stroke("Relleno");
                    // En el relleno, Añadir funciona como Pintar: cambia lo unido del mismo material.
                    let m = self.ink();
                    for c in cells {
                        let d = m != 0 && (doc.s.grid.is_detail(c[0], c[1], c[2]) || self.detail);
                        doc.paint(c[0], c[1], c[2], m, d);
                    }
                    doc.end_stroke();
                }
            }
            Tool::Extrude => {
                if p.pressed
                    && let Some((v, n)) = p.hit
                {
                    let m = doc.s.grid.get(v[0], v[1], v[2]);
                    let detail = doc.s.grid.is_detail(v[0], v[1], v[2]);
                    let layer = ops::extrude(&doc.s.grid, v, n);
                    doc.begin_stroke("Extruir cara");
                    for c in layer {
                        match self.mode {
                            Mode::Add => doc.paint(c[0], c[1], c[2], m, detail),
                            // Pintar y Borrar actúan sobre la propia cara (la capa de debajo de la nueva).
                            Mode::Paint => doc.paint(c[0] - n[0], c[1] - n[1], c[2] - n[2], self.material, detail),
                            Mode::Erase => doc.paint(c[0] - n[0], c[1] - n[1], c[2] - n[2], 0, false),
                        }
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
            Tool::Place => {
                if p.pressed
                    && let (Some(h), Some(at)) = (&self.hand, p.place)
                {
                    let result = h.placed(&doc.s, at, self.detail);
                    let label = format!("Colocar {}", h.name.to_lowercase());
                    doc.apply_external(&label, &result);
                }
            }
            Tool::Build => {}
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
            for v in ops::cuboid(min, max) {
                doc.paint(v[0], v[1], v[2], 0, false);
            }
            self.moving = Some(Moving { clip, from, origin: min });
        }
        if p.released
            && let Some(m) = self.moving.take()
        {
            let to = at.unwrap_or(m.from);
            // Se mueve en el plano: la altura solo cambia con Alt+flechas.
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

    /// Mueve el contenido de la selección `delta` sub-cubos (flechas; un paso de deshacer cada vez).
    pub fn nudge(&mut self, doc: &mut Doc, delta: V) -> bool {
        let Some((a, b)) = self.selection else { return false };
        let clip = Clip::copy(&doc.s.grid, a, b);
        doc.begin_stroke("Mover selección");
        for v in ops::cuboid(a, b) {
            doc.paint(v[0], v[1], v[2], 0, false);
        }
        let to = [a[0] + delta[0], a[1] + delta[1], a[2] + delta[2]];
        self.paste_clip(doc, &clip, to);
        doc.end_stroke();
        self.selection = Some((to, [b[0] + delta[0], b[1] + delta[1], b[2] + delta[2]]));
        true
    }

    /// Vacía la selección (Supr).
    pub fn clear_selection(&mut self, doc: &mut Doc) -> bool {
        let Some((a, b)) = self.selection else { return false };
        doc.begin_stroke("Vaciar selección");
        for v in ops::cuboid(a, b) {
            doc.paint(v[0], v[1], v[2], 0, false);
        }
        doc.end_stroke();
        true
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
        self.clear_selection(doc)
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
        for v in ops::cuboid(a, b) {
            doc.paint(v[0], v[1], v[2], 0, false);
        }
        self.paste_clip(doc, &clip, a);
        doc.end_stroke();
        self.selection = Some((a, [a[0] + clip.size[0] - 1, a[1] + clip.size[1] - 1, a[2] + clip.size[2] - 1]));
        true
    }

    /// La selección como pieza para «Guardar como sello»: sub-cubos e interactuables que caen dentro, apoyada en su
    /// esquina inferior.
    pub fn selection_as_hand(&self, doc: &Doc, name: &str) -> Option<Hand> {
        let (a, b) = self.selection?;
        let clip = Clip::copy(&doc.s.grid, a, b);
        let inside = |bb: &[i32; 6]| (0..3).all(|k| bb[k] >= a[k] && bb[k + 3] <= b[k] + 1);
        let interactables = doc
            .s
            .interactables
            .iter()
            .filter(|it| inside(&it.bbox))
            .map(|it| {
                let x = it.bbox;
                Interactable { bbox: [x[0] - a[0], x[1] - a[1], x[2] - a[2], x[3] - a[0], x[4] - a[1], x[5] - a[2]], ..it.clone() }
            })
            .collect();
        Some(Hand { name: name.into(), clip, interactables, anchor: [0, 0, 0], furniture: None, facing: 3 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agvox::Meta;
    use crate::model::VoxelGrid;

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
    fn modos_pintar_y_borrar() {
        let mut d = doc();
        let mut t = Tools { material: 3, ..Default::default() };
        t.step(&mut d, &click([2, 0, 2]));
        t.mode = Mode::Paint;
        t.material = 7;
        t.step(&mut d, &click([2, 0, 2])); // recolorea lo señalado
        t.step(&mut d, &click([5, 0, 5])); // aire: pintar no añade
        assert_eq!(d.s.grid.get(2, 0, 2), 7);
        assert_eq!(d.s.grid.get(5, 0, 5), 0);
        t.set_mode(Mode::Erase);
        t.step(&mut d, &click([2, 0, 2]));
        assert_eq!(d.s.grid.get(2, 0, 2), 0);
        t.tool = Tool::Select;
        t.set_mode(Mode::Add); // con una herramienta sin modo, vuelve al lápiz
        assert_eq!(t.tool, Tool::Pencil);
    }

    #[test]
    fn caja_hueca_seleccion_y_mover() {
        let mut d = doc();
        let mut t = Tools { tool: Tool::Box, material: 2, box_shape: BoxShape::Hollow, ..Default::default() };
        t.step(&mut d, &Pointer { place: Some([0, 0, 0]), pressed: true, down: true, ..Default::default() });
        t.step(&mut d, &Pointer { place: Some([2, 2, 2]), down: true, released: true, ..Default::default() });
        assert_eq!(d.s.grid.get(0, 0, 0), 2);
        assert_eq!(d.s.grid.get(1, 1, 1), 0, "hueca");
        t.tool = Tool::Select;
        t.step(&mut d, &Pointer { hit: Some(([0, 0, 0], [0, 1, 0])), pressed: true, ..Default::default() });
        t.step(&mut d, &Pointer { hit: Some(([2, 2, 2], [0, 1, 0])), released: true, ..Default::default() });
        assert_eq!(t.selection, Some(([0, 0, 0], [2, 2, 2])));
        assert!(t.nudge(&mut d, [3, 0, 0]));
        assert_eq!(d.s.grid.get(0, 0, 0), 0);
        assert_eq!(d.s.grid.get(3, 0, 0), 2);
        assert_eq!(t.selection, Some(([3, 0, 0], [5, 2, 2])));
        d.undo(); // mover con flechas se deshace de una vez
        assert_eq!(d.s.grid.get(0, 0, 0), 2);
    }

    #[test]
    fn colocar_y_girar_una_pieza() {
        let mut d = doc();
        let mut src = doc();
        src.s.grid.set(0, 0, 0, 4);
        src.s.grid.set(2, 0, 0, 4);
        src.s.interactables.push(Interactable { kind: "Bed".into(), bbox: [0, 0, 0, 3, 1, 1], arg: 0, facing: 3 });
        let t0 = Tools { selection: Some(([0, 0, 0], [2, 0, 0])), ..Default::default() };
        let mut hand = t0.selection_as_hand(&src, "Cama").unwrap();
        assert_eq!(hand.interactables.len(), 1);
        let mut t = Tools { tool: Tool::Place, hand: Some(hand.clone()), ..Default::default() };
        t.step(&mut d, &click([5, 1, 5]));
        assert_eq!(d.s.grid.get(5, 1, 5), 4);
        assert_eq!(d.s.grid.get(7, 1, 5), 4);
        assert_eq!(d.s.interactables[0].bbox, [5, 1, 5, 8, 2, 6]);
        d.undo();
        assert!(d.s.interactables.is_empty());
        // Girada: la fila en X pasa a ser columna en Z.
        hand.rotate();
        assert_eq!(hand.clip.size, [1, 1, 3]);
        assert_eq!(hand.interactables[0].bbox, [0, 0, 0, 1, 1, 3]);
        assert_eq!(hand.interactables[0].facing, 0);
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
