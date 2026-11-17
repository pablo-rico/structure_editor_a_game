//! Recetas de `cantero` (`docs/recetas.md`): un script Rhai que construye o retoca un modelo o una estructura con
//! formas nativas (cajas, cilindros, tejados…) y las herramientas de construcción del juego. Los bucles del script
//! colocan formas, no sub-cubos: lo pesado corre aquí.
//!
//! Dos clases de receta: la *generativa* (`new_model` / `new_structure`) rehace el documento entero y deja en él su
//! huella; la de *parche* (`open`) retoca lo que hay. La huella dice si alguien ha tocado el fichero después.

mod api;
mod sprite;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, bail, ensure};
use rhai::{Dynamic, Engine};
use serde_json::{Value, json};

use crate::agvox::RecipeNote;
use crate::bridge::proto::{Check, Rgb};
use crate::cli::{Game, open_doc};
use crate::edit::Doc;
use crate::edit::ops::{self, Clip, V};
use crate::editor;
use crate::model::VoxelGrid;
use crate::vmodel::{self, Model, Tones};

pub use api::{API, reference};

/// Tope de operaciones del script (una forma nativa cuenta como una): corta los bucles desbocados.
const MAX_OPERATIONS: u64 = 50_000_000;
/// Líneas de `print` que se devuelven.
const MAX_LOG: usize = 40;

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Escribir aquí (borrador) y no en el juego.
    pub out_dir: Option<PathBuf>,
    /// Pisar aunque el fichero se haya retocado desde que la receta lo generó.
    pub force: bool,
}

/// Lo que deja una receta.
pub struct Outcome {
    pub game: Game,
    pub doc: Doc,
    /// Fichero escrito.
    pub target: PathBuf,
    /// Resumen para quien la lanzó: qué cambió, avisos y lo que el script imprimió.
    pub summary: Value,
}

/// Estado de una receta en marcha: el documento y con qué se pinta.
pub(crate) struct Canvas {
    game: Game,
    opts: Options,
    /// Receta, tal como se apunta en el documento (relativa al juego si está dentro).
    recipe: String,
    doc: Option<Doc>,
    target: Option<PathBuf>,
    generative: bool,
    /// Lo que había antes (para el resumen); `None` si el documento es nuevo.
    before: Option<Before>,
    opened_size: Option<[i32; 3]>,
    is_new: bool,
    /// Byte con que se pinta (0 = aire: las formas borran).
    brush: u8,
    detail: bool,
    mirror: (bool, bool),
    /// Materiales sombreados de un modelo: nombre → byte del lateral.
    named: HashMap<String, u8>,
    tones: HashMap<u8, Tones>,
    /// Estructuras: altura donde se pisa en la planta de trabajo y papeles de material de las herramientas.
    floor_y: Option<i32>,
    roles: serde_json::Map<String, Value>,
    clipped: u64,
    log: Vec<String>,
    /// Forrados con un sprite pendientes: se aplican al acabar, sobre el sombreado.
    wraps: Vec<sprite::Wrap>,
}

fn hex(c: Rgb) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn parse_hex(text: &str) -> Option<Rgb> {
    let t = text.trim_start_matches('#');
    (t.len() == 6 && t.bytes().all(|b| b.is_ascii_hexdigit())).then(|| {
        let n = u32::from_str_radix(t, 16).unwrap_or(0);
        [(n >> 16) as u8, (n >> 8) as u8, n as u8]
    })
}

/// Aclara (`k` > 0) u oscurece (`k` < 0) un color, de −1 a 1.
fn shift(c: Rgb, k: f32) -> Rgb {
    c.map(|v| {
        let v = v as f32;
        (if k >= 0.0 { v + (255.0 - v) * k } else { v * (1.0 + k) }).round().clamp(0.0, 255.0) as u8
    })
}

/// Huella apuntada en la nota de un modelo generado por una receta.
fn noted_fingerprint(note: &str) -> Option<&str> {
    let rest = &note[note.find("huella ")? + 7..];
    let hex = &rest[..rest.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(rest.len())];
    (hex.len() == 16).then_some(hex)
}

impl Canvas {
    fn doc(&mut self) -> Result<&mut Doc> {
        self.doc.as_mut().context("la receta empieza creando o abriendo algo: new_model, new_structure u open")
    }

    fn model(&mut self) -> Result<&mut Model> {
        self.doc()?.model.as_mut().context("esto es solo para modelos")
    }

    fn structure_only(&mut self, what: &str) -> Result<()> {
        ensure!(!self.doc()?.is_model(), "{what} es solo para estructuras");
        Ok(())
    }

    fn fresh(&mut self) -> Result<()> {
        ensure!(self.doc.is_none(), "una receta trabaja un solo documento: ya hay uno abierto");
        Ok(())
    }

    /// Dónde se escribe `file` (el de verdad, o en la carpeta de borrador).
    fn place(&self, real: PathBuf) -> PathBuf {
        match (&self.opts.out_dir, real.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => real,
        }
    }

    /// Receta generativa: comprueba que lo que hay en `target` sigue siendo lo que una receta dejó y lo devuelve.
    fn guard(&mut self, target: &Path) -> Result<Option<Doc>> {
        if !target.is_file() {
            return Ok(None);
        }
        let old = open_doc(target, &mut self.game)?;
        let noted = match &old.model {
            Some(m) => noted_fingerprint(m.note()).map(str::to_string),
            None => old.meta.recipe.as_ref().map(|r| r.fingerprint.clone()),
        };
        if !self.opts.force && noted.as_deref() != Some(old.fingerprint().as_str()) {
            let why = if noted.is_some() { "se ha retocado desde que una receta lo generó" } else { "no lo generó una receta" };
            bail!(
                "{} ya existe y {why}. Para retocarlo usa una receta de parche (open); para rehacerlo entero perdiendo esos cambios, --forzar",
                target.display()
            );
        }
        Ok(Some(old))
    }

    fn new_model(&mut self, name: &str, size: [i32; 3]) -> Result<()> {
        self.fresh()?;
        ensure!(size.iter().all(|&n| (1..=vmodel::MAX_SIDE).contains(&n)), "tamaño no válido: {size:?} (de 1 a {} por lado)", vmodel::MAX_SIDE);
        let name = name.trim_end_matches(".json").trim_end_matches("_model");
        ensure!(!name.is_empty() && !name.ends_with('/'), "el modelo necesita un nombre, por ejemplo «Objects/Voxel/farol»");
        let real = editor::models_root(&self.game.root).join(format!("{name}_model.json"));
        // Nuevo es que aún no está en el juego, aunque ya haya un borrador.
        let in_game = real.is_file();
        let target = self.place(real);
        let old = self.guard(&target)?;
        let (mut model, s) = Model::new(size);
        model.palette.clear();
        self.is_new = !in_game;
        self.before = old.as_ref().map(Before::of);
        self.doc = Some(Doc::new_model(model, s, Some(target.clone())));
        self.target = Some(target);
        self.generative = true;
        Ok(())
    }

    fn new_structure(&mut self, name: &str, cells: [i32; 2], levels: i32) -> Result<()> {
        self.fresh()?;
        ensure!((1..=32).contains(&cells[0]) && (1..=32).contains(&cells[1]), "de 1 a 32 celdas por lado");
        ensure!((2..=63).contains(&levels), "de 2 a 63 niveles (8 sub-cubos cada uno)");
        ensure!(!name.trim().is_empty(), "la estructura necesita un nombre");
        let target = self.place(editor::source_dir(&self.game.root).join(format!("{}.agvox", editor::slug(name))));
        let old = self.guard(&target)?;
        let (mut meta, s) = editor::blank_structure(self.game.catalog()?, cells[0], cells[1], levels);
        meta.name = name.trim().to_string();
        // El id, el estilo y a qué diseño sustituye son del fichero: rehacerlo no los cambia.
        if let Some(old) = &old {
            (meta.id, meta.style, meta.replaces, meta.look) =
                (old.meta.id, old.meta.style.clone(), old.meta.replaces.clone(), old.meta.look.clone());
        }
        self.is_new = old.is_none();
        self.before = old.as_ref().map(Before::of);
        self.doc = Some(Doc::new(meta, s, Some(target.clone())));
        self.target = Some(target);
        self.generative = true;
        self.material("Stone")?;
        Ok(())
    }

    fn open(&mut self, path: &str, base: Option<&str>) -> Result<()> {
        self.fresh()?;
        let given = PathBuf::from(path);
        let real = if given.is_absolute() || given.is_file() { given } else { self.game.root.join(&given) };
        let target = self.place(real.clone());
        // Con borrador, se sigue trabajando sobre lo que ya hay en él.
        let source = if target.is_file() { target.clone() } else { real };
        let doc = open_doc(&source, &mut self.game)?;
        if let Some(base) = base {
            let now = doc.fingerprint();
            ensure!(
                base == now,
                "{} no es la base de este parche (huella {now}, la receta espera {base}): ya está aplicado o el fichero ha cambiado",
                source.display()
            );
        }
        self.before = Some(Before::of(&doc));
        self.opened_size = Some(doc.s.grid.size);
        self.doc = Some(doc);
        self.target = Some(target);
        Ok(())
    }

    /// Byte de un color de modelo (lo añade a la paleta si no está).
    fn color_byte(&mut self, rgb: Rgb) -> Result<u8> {
        let m = self.model()?;
        let i = match m.palette.iter().position(|c| *c == rgb) {
            Some(i) => i,
            None => {
                ensure!(m.palette.len() < vmodel::MAX_COLORS, "un modelo tiene como mucho {} colores", vmodel::MAX_COLORS);
                m.palette.push(rgb);
                m.palette.len() - 1
            }
        };
        Ok(i as u8 + 1)
    }

    fn color(&mut self, name: &str) -> Result<()> {
        self.brush = match (self.named.get(name), parse_hex(name)) {
            (Some(&b), _) => b,
            (None, Some(rgb)) => self.color_byte(rgb)?,
            _ => bail!("«{name}» no es un color rrggbb ni un material definido con tones()"),
        };
        Ok(())
    }

    fn define_tones(&mut self, name: &str, top: Rgb, rim: Rgb, side: Rgb, under: Rgb) -> Result<()> {
        let b = self.color_byte(side)?;
        let t = Tones { top: self.color_byte(top)?, rim: self.color_byte(rim)?, under: self.color_byte(under)? };
        self.tones.insert(b, t);
        self.named.insert(name.to_string(), b);
        self.brush = b;
        Ok(())
    }

    fn material(&mut self, name: &str) -> Result<()> {
        self.structure_only("material()")?;
        let cat = self.game.catalog()?;
        let i = cat.materials.iter().position(|m| m == name);
        self.brush = i.filter(|&i| i > 0).with_context(|| format!("el juego no tiene el material «{name}» (mira `cantero catalog materiales`)"))? as u8;
        Ok(())
    }

    /// Nombre de lo que hay en un sub-cubo: color `rrggbb`, material, o vacío si es aire.
    fn name_of(&mut self, b: u8) -> Result<String> {
        if b == 0 {
            return Ok(String::new());
        }
        if let Some(m) = &self.doc()?.model {
            return Ok(m.palette.get(b as usize - 1).copied().map(hex).unwrap_or_default());
        }
        Ok(self.game.catalog()?.materials.get(b as usize).cloned().unwrap_or_default())
    }

    /// Byte de un nombre (color, material sombreado o material del juego).
    fn byte_of(&mut self, name: &str) -> Result<u8> {
        let keep = self.brush;
        if self.doc()?.is_model() { self.color(name)? } else { self.material(name)? }
        Ok(std::mem::replace(&mut self.brush, keep))
    }

    fn put(&mut self, v: V) -> Result<()> {
        let (brush, detail, (mx, mz)) = (self.brush, self.detail, self.mirror);
        let doc = self.doc()?;
        let size = doc.s.grid.size;
        let mut out = 0;
        let mut paint = |w: V| {
            if doc.s.grid.in_bounds(w[0], w[1], w[2]) {
                doc.paint(w[0], w[1], w[2], brush, detail);
            } else {
                out += 1;
            }
        };
        if mx || mz {
            ops::mirrored(v, size, mx, mz).into_iter().for_each(&mut paint);
        } else {
            paint(v);
        }
        self.clipped += out;
        Ok(())
    }

    fn put_all(&mut self, cells: Vec<V>) -> Result<()> {
        cells.into_iter().try_for_each(|v| self.put(v))
    }

    /// Esquinas incluidas de una caja semiabierta `[x0, y0, z0, x1, y1, z1)`.
    fn corners(b: [i32; 6]) -> Result<(V, V)> {
        ensure!((0..3).all(|k| b[k] < b[k + 3]), "caja vacía: va de la esquina menor a la mayor, sin incluir la mayor ({b:?})");
        Ok(([b[0], b[1], b[2]], [b[3] - 1, b[4] - 1, b[5] - 1]))
    }

    fn shaped(&mut self, b: [i32; 6], shape: ops::BoxShape) -> Result<()> {
        let (lo, hi) = Self::corners(b)?;
        self.put_all(ops::shaped_box(lo, hi, shape))
    }

    fn fill(&mut self, at: V) -> Result<()> {
        let cells = ops::flood(&self.doc()?.s.grid, at, None, 4_000_000);
        // Rellenar no se refleja: es una región, no una forma.
        let keep = std::mem::take(&mut self.mirror);
        let done = self.put_all(cells);
        self.mirror = keep;
        done
    }

    fn replace(&mut self, from: &str, to: &str, b: Option<[i32; 6]>) -> Result<i64> {
        let (from, to) = (self.byte_of(from)?, self.byte_of(to)?);
        let size = self.doc()?.s.grid.size;
        let (lo, hi) = Self::corners(b.unwrap_or([0, 0, 0, size[0], size[1], size[2]]))?;
        let detail = self.detail;
        let doc = self.doc()?;
        let mut n = 0;
        for v in ops::cuboid(lo, hi) {
            if doc.s.grid.get(v[0], v[1], v[2]) == from {
                doc.paint(v[0], v[1], v[2], to, detail || doc.s.grid.is_detail(v[0], v[1], v[2]));
                n += 1;
            }
        }
        Ok(n)
    }

    /// Repinta al azar con el pincel una parte de lo sólido de la caja (vetas, desgaste, hojas distintas).
    fn speckle(&mut self, b: [i32; 6], percent: i32, seed: i32, only: Option<&str>) -> Result<i64> {
        ensure!((1..=100).contains(&percent), "el porcentaje va de 1 a 100");
        ensure!(self.brush != 0, "speckle repinta lo sólido: elige un color, no aire");
        let only = only.map(|n| self.byte_of(n)).transpose()?;
        let (lo, hi) = Self::corners(b)?;
        let (brush, detail) = (self.brush, self.detail);
        let doc = self.doc()?;
        let mut n = 0;
        for v in ops::cuboid(lo, hi) {
            let m = doc.s.grid.get(v[0], v[1], v[2]);
            if m != 0 && only.is_none_or(|o| o == m) && ops::noise(seed as u32, v) % 100 < percent as u32 {
                doc.paint(v[0], v[1], v[2], brush, detail || doc.s.grid.is_detail(v[0], v[1], v[2]));
                n += 1;
            }
        }
        Ok(n)
    }

    fn copy(&mut self, b: [i32; 6]) -> Result<Clip> {
        let (lo, hi) = Self::corners(b)?;
        Ok(Clip::copy_from(&self.doc()?.s, lo, hi))
    }

    fn paste(&mut self, clip: &Clip, at: V) -> Result<()> {
        let doc = self.doc()?;
        let mut out = 0;
        for (v, m, detail) in clip.solids() {
            let w = [at[0] + v[0], at[1] + v[1], at[2] + v[2]];
            if doc.s.grid.in_bounds(w[0], w[1], w[2]) {
                doc.paint(w[0], w[1], w[2], m, detail);
            } else {
                out += 1;
            }
        }
        for (&key, &color) in &clip.faces {
            let (v, d) = crate::model::face_unkey(key);
            doc.paint_face([at[0] + v[0], at[1] + v[1], at[2] + v[2]], d, color);
        }
        self.clipped += out;
        Ok(())
    }

    fn resize(&mut self, size: [i32; 3]) -> Result<()> {
        ensure!(self.doc()?.is_model(), "resize es para modelos: una estructura crece con las herramientas del juego");
        let doc = self.doc()?;
        ensure!(size == doc.s.grid.size || doc.resize_model(size), "el tamaño {size:?} no vale: no cabe lo que hay o se sale de 1–{}", vmodel::MAX_SIDE);
        Ok(())
    }

    /// Herramienta de construcción del juego sobre el documento entero.
    fn tool(&mut self, name: &str, target: Value, params: Value) -> Result<()> {
        self.structure_only("tool()")?;
        let doc = self.doc.as_mut().expect("comprobado");
        doc.end_stroke();
        let floor_y = self.floor_y.unwrap_or(doc.s.ground_level * crate::model::SUB);
        let mut roles = doc.meta.look.as_ref().map_or_else(serde_json::Map::new, |l| l.roles.iter().map(|(k, v)| (k.clone(), json!(v))).collect());
        roles.extend(self.roles.clone());
        let extra = json!({ "name": name, "params": params, "roles": roles, "target": target, "floorY": floor_y, "style": doc.meta.style });
        let result = self
            .game
            .client()?
            .tool(&doc.s, doc.meta.replaces.as_ref(), extra)
            .with_context(|| format!("tool(\"{name}\")"))?;
        doc.apply_external(name, &result);
        if result.entrance != doc.s.entrance {
            doc.edit_extras("Entrada", |e| e.entrance = result.entrance);
        }
        Ok(())
    }

    /// Mueble del juego apoyado en `at` (el suelo donde se pisa), mirando a `facing` (`dirs`: 0 +X, 1 −X, 2 +Z, 3 −Z).
    fn stamp(&mut self, piece: &str, at: V, facing: i32, width: Option<i32>, seed: i32) -> Result<()> {
        self.structure_only("stamp()")?;
        ensure!((0..4).contains(&facing), "facing va de 0 a 3 (+X, −X, +Z, −Z)");
        let st = self.game.client()?.stamp(piece, facing, width, seed).with_context(|| format!("stamp(\"{piece}\")"))?;
        let len = st.grid.data.len();
        let hand = crate::edit::tools::Hand {
            name: piece.to_string(),
            clip: Clip { size: st.grid.size, data: st.grid.data, detail: st.grid.detail.unwrap_or_else(|| vec![0; len]), faces: Default::default() },
            interactables: st.interactables,
            anchor: st.anchor,
            furniture: Some(piece.to_string()),
            facing,
        };
        let doc = self.doc.as_mut().expect("comprobado");
        let result = hand.placed(&doc.s, at, true);
        doc.apply_external(piece, &result);
        Ok(())
    }

    /// Sprite del juego (ruta absoluta o relativa a su carpeta).
    fn sprite(&mut self, path: &str) -> Result<sprite::Sprite> {
        self.model()?;
        let given = PathBuf::from(path);
        sprite::Sprite::load(&if given.is_absolute() || given.is_file() { given } else { self.game.root.join(&given) })
    }

    fn sprite_radii(&mut self, path: &str, axis: f32, ground: i32, y0: i32, y1: i32) -> Result<Vec<f32>> {
        let s = self.sprite(path)?;
        Ok((y0..y1).map(|y| s.radius(axis, ground, y)).collect())
    }

    fn sprite_wrap(&mut self, path: &str, axis: f32, ground: i32, bounds: [i32; 6]) -> Result<()> {
        let sprite = self.sprite(path)?;
        self.wraps.push(sprite::Wrap { sprite, axis, ground, bounds });
        Ok(())
    }

    fn look(&mut self, district: &str, tier: &str) -> Result<()> {
        self.structure_only("look()")?;
        let style = self.doc()?.meta.style.clone();
        let roles = self.game.client()?.look(&style, district, tier)?;
        self.roles = roles.as_object().cloned().context("el generador no ha devuelto papeles")?;
        Ok(())
    }

    /// Cierra la receta: sombreado, huella, guardar y resumen.
    fn finish(mut self) -> Result<Outcome> {
        let target = self.target.clone().context("la receta no ha creado ni abierto nada")?;
        let mut doc = self.doc.take().context("la receta no ha creado ni abierto nada")?;
        if !self.tones.is_empty() {
            for (v, d, color) in vmodel::shade(&doc.s, &self.tones) {
                doc.paint_face(v, d, color);
            }
        }
        for w in std::mem::take(&mut self.wraps) {
            for (v, d, rgb) in sprite::wrap(&doc.s, &w) {
                let m = doc.model.as_mut().context("sprite_wrap() es para modelos")?;
                let i = match m.palette.iter().position(|c| *c == rgb) {
                    Some(i) => i,
                    None => {
                        ensure!(m.palette.len() < vmodel::MAX_COLORS, "un modelo tiene como mucho {} colores", vmodel::MAX_COLORS);
                        m.palette.push(rgb);
                        m.palette.len() - 1
                    }
                };
                doc.paint_face(v, d, i as u8 + 1);
            }
        }
        doc.end_stroke();
        let fingerprint = doc.fingerprint();
        let materials = if doc.is_model() { Vec::new() } else { self.game.catalog()?.materials.clone() };
        let mut checks: Vec<Check> = Vec::new();
        let text = match doc.model.as_mut() {
            Some(m) => {
                if self.generative {
                    m.set_note(&format!("Receta: {}; huella {fingerprint}.", self.recipe));
                }
                checks.extend(vmodel::checks(&doc.s, self.opened_size, self.is_new));
                vmodel::save(m, &doc.s)?
            }
            None => {
                if self.generative {
                    doc.meta.recipe = Some(RecipeNote { file: self.recipe.clone(), fingerprint: fingerprint.clone() });
                }
                checks.extend(self.game.client()?.validate(&doc.s, doc.meta.replaces.as_ref())?.checks);
                crate::agvox::save(&doc.meta, &doc.s, &materials)?
            }
        };
        let unchanged = std::fs::read_to_string(&target).is_ok_and(|old| old == text);
        if !unchanged {
            editor::write_atomic(&target, &text)?;
            // Un modelo nuevo dentro del juego necesita su `.meta` de Unity.
            let meta = PathBuf::from(format!("{}.meta", target.display()));
            if doc.is_model() && !meta.exists() && target.starts_with(self.game.root.join("Assets")) {
                std::fs::write(&meta, editor::unity_meta())?;
            }
        }
        let mut summary = json!({
            "written": target,
            "unchanged": unchanged,
            "kind": if doc.is_model() { "model" } else { "structure" },
            "size": doc.s.grid.size,
            "solidBox": doc.s.grid.solid_box().map(|(lo, hi)| [lo, hi]),
            "solids": doc.s.grid.counts()[1..].iter().sum::<u32>(),
            "fingerprint": fingerprint,
            "change": change(self.before.as_ref(), &doc),
            "checks": checks,
        });
        if self.clipped > 0 {
            summary["outOfBounds"] = json!(format!("{} sub-cubos caían fuera de la rejilla y no se han puesto", self.clipped));
        }
        if !self.log.is_empty() {
            summary["log"] = json!(self.log);
        }
        doc.unsaved = false;
        Ok(Outcome { game: self.game, doc, target, summary })
    }
}

/// Lo que había antes de la receta, para decir qué ha cambiado.
struct Before {
    grid: VoxelGrid,
    /// Modelos: color de cada cara expuesta. El byte de un vóxel no sirve para comparar: la paleta se compacta al
    /// guardar y el color de un vóxel leído es el más frecuente de sus caras.
    faces: Option<HashMap<u32, Rgb>>,
}

impl Before {
    fn of(doc: &Doc) -> Self {
        let faces = doc.model.as_ref().map(|m| vmodel::face_colors(m, &doc.s).into_iter().collect());
        Self { grid: doc.s.grid.clone(), faces }
    }
}

/// Qué ha cambiado en los sub-cubos respecto a lo que había.
fn change(before: Option<&Before>, doc: &Doc) -> Value {
    let Some(old) = before else { return json!({ "new": true }) };
    let s = &doc.s;
    if old.grid.size != s.grid.size {
        return json!({ "resized": [old.grid.size, s.grid.size] });
    }
    // Vóxeles de un modelo con alguna cara que ha cambiado de color.
    let repainted: std::collections::HashSet<u32> = match (&old.faces, &doc.model) {
        (Some(faces), Some(m)) => vmodel::face_colors(m, s)
            .into_iter()
            .filter(|(key, c)| faces.get(key).is_some_and(|o| o != c))
            .map(|(key, _)| key & 0x00ff_ffff)
            .collect(),
        _ => Default::default(),
    };
    let (mut added, mut removed, mut recolored) = (0u32, 0u32, 0u32);
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    let [sx, _, sz] = old.grid.size;
    for (i, (&a, &b)) in old.grid.data.iter().zip(&s.grid.data).enumerate() {
        let i = i as i32;
        let v = [i % sx, i / (sx * sz), i / sx % sz];
        match (a, b) {
            (0, 0) => continue,
            (0, _) => added += 1,
            (_, 0) => removed += 1,
            _ if doc.is_model() && !repainted.contains(&crate::model::face_key(v, 0)) => continue,
            _ if !doc.is_model() && a == b => continue,
            _ => recolored += 1,
        }
        lo = std::array::from_fn(|k| lo[k].min(v[k]));
        hi = std::array::from_fn(|k| hi[k].max(v[k] + 1));
    }
    let bbox = (added + removed + recolored > 0).then_some([lo, hi]);
    json!({ "added": added, "removed": removed, "recolored": recolored, "box": bbox })
}

/// Ejecuta una receta y guarda lo que deja.
pub fn run(recipe: &Path, game: Game, opts: Options) -> Result<Outcome> {
    let script = std::fs::read_to_string(recipe).with_context(|| format!("no puedo leer {}", recipe.display()))?;
    let full = std::path::absolute(recipe).unwrap_or_else(|_| recipe.to_path_buf());
    let root = std::path::absolute(&game.root).unwrap_or_else(|_| game.root.clone());
    let noted = full.strip_prefix(&root).unwrap_or(Path::new(recipe.file_name().unwrap_or_default()));
    let canvas = Canvas {
        game,
        opts,
        recipe: noted.to_string_lossy().replace('\\', "/"),
        doc: None,
        target: None,
        generative: false,
        before: None,
        opened_size: None,
        is_new: false,
        brush: 0,
        detail: false,
        mirror: (false, false),
        named: HashMap::new(),
        tones: HashMap::new(),
        floor_y: None,
        roles: serde_json::Map::new(),
        clipped: 0,
        log: Vec::new(),
        wraps: Vec::new(),
    };
    let shared = Rc::new(RefCell::new(canvas));
    let mut engine = Engine::new();
    engine.set_max_operations(MAX_OPERATIONS);
    api::register(&mut engine, &shared);
    let log = shared.clone();
    engine.on_print(move |line| {
        let c = &mut log.borrow_mut();
        if c.log.len() < MAX_LOG {
            c.log.push(line.to_string());
        }
    });
    let ran = engine.run(&script);
    drop(engine);
    if let Err(e) = ran {
        bail!("{}: {e}", recipe.display());
    }
    let canvas = Rc::try_unwrap(shared).ok().context("la receta sigue en uso")?.into_inner();
    canvas.finish()
}

/// Valor de Rhai como JSON (parámetros de las herramientas del juego).
pub(crate) fn to_json(d: &Dynamic) -> Result<Value> {
    if d.is_unit() {
        return Ok(Value::Null);
    }
    if let Ok(b) = d.as_bool() {
        return Ok(json!(b));
    }
    if let Ok(i) = d.as_int() {
        return Ok(json!(i));
    }
    if let Ok(f) = d.as_float() {
        return Ok(json!(f));
    }
    if d.is_string() {
        return Ok(json!(d.clone().into_string().unwrap_or_default()));
    }
    if d.is_array() {
        let items = d.clone().into_array().map_err(anyhow::Error::msg)?;
        return items.iter().map(to_json).collect::<Result<Vec<_>>>().map(Value::Array);
    }
    if let Some(map) = d.clone().try_cast::<rhai::Map>() {
        return map.iter().map(|(k, v)| Ok((k.to_string(), to_json(v)?))).collect::<Result<serde_json::Map<_, _>>>().map(Value::Object);
    }
    bail!("no sé pasar al juego un valor de tipo {}", d.type_name())
}

#[cfg(test)]
mod tests;
