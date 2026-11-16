//! Modelos de vóxeles del juego (`*_model.json`, `VoxelModel` de a_game): árboles, objetos, intro… El fichero del
//! juego es la única fuente: aquí se lee, se reconstruye el sólido a partir de sus caras y se vuelve a escribir.
//! Formato y reglas en `docs/contrato.md` («Modelos»); `VoxelModel.cs` no corre fuera de Unity, por eso vive en Rust.

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Result, ensure};

use crate::bridge::proto::Rgb;
use crate::mesh::Paint;
use crate::model::{FaceColors, Structure, VoxelGrid, face_key, face_unkey};

/// Lo que el editor antepone a `_` al guardar: los scripts de arte de a_game se saltan los modelos que la llevan.
pub const EDITED_MARK: &str = "A mano (editor de estructuras). ";
/// La rejilla guarda un byte por vóxel y el 0 es aire.
pub const MAX_COLORS: usize = 255;
/// Lado máximo en vóxeles (el de `vox::MAX`; `face_key` reserva 8 bits por eje).
pub const MAX_SIDE: i32 = 256;
/// Normal de cada dirección de cara (`VoxelModel.Normals`): `+x −x +y −y +z −z`.
pub const DIRS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// Lo que un modelo tiene además de sus vóxeles.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// Vóxeles por casilla y por nivel.
    pub cell: i32,
    pub level: i32,
    /// Paleta viva: el byte `b` de la rejilla es `palette[b - 1]`.
    pub palette: Vec<Rgb>,
    /// Punto del modelo que cae en la esquina de la huella (`origin`).
    pub origin: Option<[i32; 3]>,
    /// Texto original hasta `"faces"`, con `\n`: las claves que no tocamos (`pivot`, `hinge`…) se conservan tal cual.
    head: String,
    crlf: bool,
    /// `face_key` de las caras en el orden del fichero (no es uniforme entre scripts).
    order: Vec<u32>,
}

impl Model {
    /// Modelo nuevo y vacío, con un gris en la paleta.
    pub fn new(size: [i32; 3]) -> (Model, Structure) {
        let head = format!(
            "{{\n  \"_\": \"{EDITED_MARK}Modelo nuevo.\",\n  \"cell\": 16,\n  \"level\": 16,\n  \"size\": [{},{},{}],\n  \"palette\": [],\n  ",
            size[0], size[1], size[2]
        );
        let model = Model {
            cell: 16,
            level: 16,
            palette: vec![[128, 128, 128]],
            origin: None,
            head,
            crlf: false,
            order: Vec::new(),
        };
        let s = Structure {
            grid: VoxelGrid::new(size),
            model_cell: Some(16),
            ..Default::default()
        };
        (model, s)
    }

    fn raw_note(&self) -> &str {
        let Some(at) = self.head.find("\"_\": \"") else {
            return "";
        };
        let rest = &self.head[at + 6..];
        &rest[..rest.find("\",\n").unwrap_or(rest.len())]
    }

    /// Con qué se pinta: la paleta (el byte 0 es aire) y sin tono por sub-cubo.
    pub fn paint(&self) -> Paint {
        let palette = std::iter::once([255, 0, 255]).chain(self.palette.iter().copied()).collect();
        Paint { palette, hidden: None, flat: true }
    }

    /// Tras guardar: la cabecera y el orden de las caras pasan a ser los del texto escrito (la paleta viva no cambia).
    pub fn sync(&mut self, saved: &str) -> Result<()> {
        let (m, _) = load(saved)?;
        (self.head, self.order, self.crlf) = (m.head, m.order, m.crlf);
        Ok(())
    }

    /// Ya se guardó alguna vez desde el editor.
    pub fn edited(&self) -> bool {
        self.raw_note().starts_with(EDITED_MARK)
    }

    /// La nota `_` del fichero (de qué script salió), sin la marca.
    pub fn note(&self) -> &str {
        let n = self.raw_note();
        n.strip_prefix(EDITED_MARK).unwrap_or(n)
    }
}

fn ints<const N: usize>(v: &serde_json::Value, what: &str) -> Result<[i32; N]> {
    let a = v
        .as_array()
        .with_context(|| format!("al modelo le falta «{what}»"))?;
    ensure!(a.len() == N, "«{what}» debería tener {N} números");
    let mut out = [0; N];
    for (o, n) in out.iter_mut().zip(a) {
        *o = n
            .as_i64()
            .with_context(|| format!("«{what}» debería ser de enteros"))? as i32;
    }
    Ok(out)
}

/// Caras expuestas de lo sólido de `g`, salvo las de abajo en y = 0 (el formato nunca las lleva).
fn exposed(g: &VoxelGrid) -> Vec<([i32; 3], u8)> {
    let [sx, sy, sz] = g.size;
    let mut out = Vec::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                if g.get(x, y, z) == 0 {
                    continue;
                }
                for (d, n) in DIRS.iter().enumerate() {
                    if g.get(x + n[0], y + n[1], z + n[2]) == 0 && !(y == 0 && d == 3) {
                        out.push(([x, y, z], d as u8));
                    }
                }
            }
        }
    }
    out
}

/// Lee un `_model.json` y reconstruye el sólido. No abre nada «a medias»: si las caras no encierran un sólido, error.
pub fn load(text: &str) -> Result<(Model, Structure)> {
    let crlf = text.contains("\r\n");
    let text = text.replace("\r\n", "\n");
    let json: serde_json::Value = serde_json::from_str(&text)
        .context("no es un modelo de vóxeles (no se puede leer como JSON)")?;
    let cell = json["cell"].as_i64().unwrap_or(0) as i32;
    let level = json["level"].as_i64().unwrap_or(0) as i32;
    ensure!(
        cell > 0 && level > 0,
        "al modelo le falta cuántos vóxeles tiene por casilla y por nivel"
    );
    let size: [i32; 3] = ints(&json["size"], "size")?;
    ensure!(
        size.iter().all(|&s| (1..=MAX_SIDE).contains(&s)),
        "tamaño no válido: {size:?} (de 1 a {MAX_SIDE} por lado)"
    );
    let colors = json["palette"]
        .as_array()
        .context("el modelo no tiene paleta")?;
    ensure!(
        colors.len() <= MAX_COLORS,
        "el modelo tiene {} colores y el máximo es {MAX_COLORS}",
        colors.len()
    );
    let mut palette = Vec::with_capacity(colors.len());
    for c in colors {
        let hex = c.as_str().unwrap_or("");
        ensure!(
            hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
            "color no válido en la paleta: «{hex}»"
        );
        let n = u32::from_str_radix(hex, 16)?;
        palette.push([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    let origin = match &json["origin"] {
        v if v.as_array().is_some_and(|a| a.len() == 3) => Some(ints(v, "origin")?),
        _ => None,
    };
    let raw = json["faces"]
        .as_array()
        .context("el modelo no tiene caras")?;
    ensure!(
        raw.len() % 5 == 0,
        "las caras del modelo no van de cinco en cinco números"
    );
    let head_len = text.find("\"faces\"").context("el modelo no tiene caras")?;

    // Caras: clave → índice de color, y el orden en que venían.
    let mut faces: HashMap<u32, u8> = HashMap::with_capacity(raw.len() / 5);
    let mut order = Vec::with_capacity(raw.len() / 5);
    for f in raw.chunks(5) {
        let n: Vec<i64> = f.iter().map(|v| v.as_i64().unwrap_or(-1)).collect();
        let v = [n[0] as i32, n[1] as i32, n[2] as i32];
        ensure!(
            (0..3).all(|i| (0..size[i] as i64).contains(&n[i])),
            "hay una cara fuera del modelo: {v:?}"
        );
        ensure!(
            (0..6).contains(&n[3]),
            "hay una cara con una dirección que no existe: {}",
            n[3]
        );
        ensure!(
            (0..palette.len() as i64).contains(&n[4]),
            "hay una cara con un color que no está en la paleta: {}",
            n[4]
        );
        let key = face_key(v, n[3] as u8);
        ensure!(
            faces.insert(key, n[4] as u8).is_none(),
            "hay una cara repetida en {v:?}"
        );
        order.push(key);
    }

    // Sólido: en cada fila (y, z), una cara −x abre y una +x cierra.
    const OPEN: &str = "las caras del modelo no encierran un sólido";
    let mut solid = VoxelGrid::new(size);
    let mut rows: HashMap<(i32, i32), (Vec<i32>, Vec<i32>)> = HashMap::new();
    for &key in &order {
        let (v, d) = face_unkey(key);
        if d < 2 {
            let row = rows.entry((v[1], v[2])).or_default();
            if d == 1 {
                row.0.push(v[0])
            } else {
                row.1.push(v[0])
            }
        }
    }
    for ((y, z), (mut open, mut close)) in rows {
        open.sort_unstable();
        close.sort_unstable();
        ensure!(open.len() == close.len(), "{OPEN} (fila y = {y}, z = {z})");
        for (&a, &b) in open.iter().zip(&close) {
            ensure!(a <= b, "{OPEN} (fila y = {y}, z = {z})");
            for x in a..=b {
                solid.set(x, y, z, 1);
            }
        }
    }
    // Las caras del sólido tienen que ser exactamente las del fichero (las de abajo en y = 0 no cuentan).
    let shell = exposed(&solid);
    for (v, d) in &shell {
        ensure!(
            faces.contains_key(&face_key(*v, *d)),
            "{OPEN}: falta una cara en {v:?}"
        );
    }
    let bottoms = order
        .iter()
        .filter(|&&k| matches!(face_unkey(k), (v, 3) if v[1] == 0))
        .count();
    ensure!(shell.len() == order.len() - bottoms, "{OPEN}: sobran caras");

    // Color de cada vóxel con caras: el más frecuente; en empate, el de arriba si empata y si no el de índice menor.
    let mut by_voxel: HashMap<[i32; 3], [Option<u8>; 6]> = HashMap::new();
    for (&key, &c) in &faces {
        let (v, d) = face_unkey(key);
        by_voxel.entry(v).or_insert([None; 6])[d as usize] = Some(c);
    }
    let mut voxels: Vec<_> = by_voxel.into_iter().collect();
    voxels.sort_unstable_by_key(|(v, _)| (v[1], v[2], v[0]));
    let mut grid = VoxelGrid::new(size);
    let mut face_colors = FaceColors::new();
    let mut queue = VecDeque::with_capacity(voxels.len());
    for (v, cs) in &voxels {
        let count = |c: u8| cs.iter().filter(|&&x| x == Some(c)).count();
        let best = cs.iter().flatten().map(|&c| count(c)).max().unwrap_or(0);
        let color = match cs[2] {
            Some(top) if count(top) == best => top,
            _ => cs
                .iter()
                .flatten()
                .copied()
                .filter(|&c| count(c) == best)
                .min()
                .unwrap_or(0),
        };
        grid.set(v[0], v[1], v[2], color + 1);
        for (d, c) in cs.iter().enumerate() {
            if let Some(c) = c.filter(|&c| c != color) {
                face_colors.insert(face_key(*v, d as u8), c + 1);
            }
        }
        queue.push_back(*v);
    }
    // Los enterrados no tienen color en el fichero: heredan el del vecino por el que se llega desde la superficie.
    while let Some(v) = queue.pop_front() {
        let c = grid.get(v[0], v[1], v[2]);
        for n in DIRS {
            let w = [v[0] + n[0], v[1] + n[1], v[2] + n[2]];
            if solid.get(w[0], w[1], w[2]) != 0 && grid.get(w[0], w[1], w[2]) == 0 {
                grid.set(w[0], w[1], w[2], c);
                queue.push_back(w);
            }
        }
    }
    if palette.is_empty() {
        palette.push([128, 128, 128]); // siempre hay un color con el que pintar; sin usar, no se escribe
    }
    let model = Model {
        cell,
        level,
        palette,
        origin,
        head: text[..head_len].to_string(),
        crlf,
        order,
    };
    let s = Structure {
        grid,
        face_colors,
        model_cell: Some(cell),
        ..Default::default()
    };
    Ok((model, s))
}

/// Cambia en la cabecera el valor (una lista en una línea) de `key`.
fn set_array(head: &mut String, key: &str, value: &str) -> Result<()> {
    let at = head
        .find(&format!("\"{key}\":"))
        .with_context(|| format!("al modelo le falta «{key}»"))?;
    let open = at
        + head[at..]
            .find('[')
            .with_context(|| format!("«{key}» no es una lista"))?;
    let close = open
        + head[open..]
            .find(']')
            .with_context(|| format!("«{key}» no se cierra"))?;
    head.replace_range(open..=close, value);
    Ok(())
}

/// Texto del `_model.json` de `s`, con el formato de los scripts de arte de a_game: la cabecera original, las caras
/// que siguen existiendo en su orden y las nuevas al final por (y, z, x, dirección).
pub fn save(model: &Model, s: &Structure) -> Result<String> {
    let g = &s.grid;
    ensure!(
        g.size.iter().all(|&n| (1..=MAX_SIDE).contains(&n)),
        "tamaño no válido: {:?}",
        g.size
    );
    let mut colors: HashMap<u32, u8> = HashMap::new();
    for (v, d) in exposed(g) {
        let key = face_key(v, d);
        let b = s
            .face_colors
            .get(&key)
            .copied()
            .unwrap_or_else(|| g.get(v[0], v[1], v[2]));
        ensure!(
            (1..=model.palette.len()).contains(&(b as usize)),
            "hay un vóxel con un color que no está en la paleta"
        );
        colors.insert(key, b);
    }
    let mut keys: Vec<u32> = model
        .order
        .iter()
        .copied()
        .filter(|k| colors.contains_key(k))
        .collect();
    let kept: HashSet<u32> = keys.iter().copied().collect();
    let mut new: Vec<u32> = colors
        .keys()
        .copied()
        .filter(|k| !kept.contains(k))
        .collect();
    new.sort_unstable_by_key(|&k| {
        let (v, d) = face_unkey(k);
        (v[1], v[2], v[0], d)
    });
    keys.extend(new);

    // Paleta del fichero: solo los colores que lleva alguna cara, en el orden de la paleta viva.
    let used: HashSet<u8> = colors.values().copied().collect();
    let mut index = [0usize; 256];
    let mut hexes = Vec::new();
    for (i, c) in model.palette.iter().enumerate() {
        if used.contains(&(i as u8 + 1)) {
            index[i + 1] = hexes.len();
            hexes.push(format!("\"{:02x}{:02x}{:02x}\"", c[0], c[1], c[2]));
        }
    }

    let mut head = model.head.clone();
    set_array(
        &mut head,
        "size",
        &format!("[{},{},{}]", g.size[0], g.size[1], g.size[2]),
    )?;
    set_array(&mut head, "palette", &format!("[{}]", hexes.join(",")))?;
    if let Some(o) = model.origin {
        let value = format!("[{},{},{}]", o[0], o[1], o[2]);
        if head.contains("\"origin\":") {
            set_array(&mut head, "origin", &value)?;
        } else {
            head = head.replacen(
                "  \"palette\":",
                &format!("  \"origin\": {value},\n  \"palette\":"),
                1,
            );
        }
    }
    if !model.edited() {
        match head.find("\"_\": \"") {
            Some(at) => head.insert_str(at + 6, EDITED_MARK),
            None => head = head.replacen("{\n", &format!("{{\n  \"_\": \"{EDITED_MARK}\",\n"), 1),
        }
    }
    let lines: Vec<String> = keys
        .iter()
        .map(|&k| {
            let (v, d) = face_unkey(k);
            format!(
                "    {}, {}, {}, {}, {}",
                v[0], v[1], v[2], d, index[colors[&k] as usize]
            )
        })
        .collect();
    let mut out = head;
    out.push_str("\"faces\": [\n");
    if !lines.is_empty() {
        out.push_str(&lines.join(",\n"));
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    Ok(if model.crlf {
        out.replace('\n', "\r\n")
    } else {
        out
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const MINERAL: &str =
        include_str!("../fixtures/models/mineral_nickel_grass_small_model.json");
    pub(crate) const CHEST: &str = include_str!("../fixtures/models/chest_0_model.json");
    const INKWELL: &str = include_str!("../fixtures/models/pbk_inkwell_model.json");
    const COVER: &str = include_str!("../fixtures/models/pbk_cover_l_model.json");
    const TABLE: &str = include_str!("../fixtures/models/pbk_table_model.json");

    /// Las fixtures con `\n` aunque git las haya sacado con `\r\n`.
    pub(crate) fn lf(t: &str) -> String {
        t.replace("\r\n", "\n")
    }

    fn marked(t: &str) -> String {
        t.replacen("\"_\": \"", &format!("\"_\": \"{EDITED_MARK}"), 1)
    }

    /// Un modelo mínimo con el formato de los scripts.
    fn mk(size: [i32; 3], palette: &[&str], faces: &[[i32; 5]]) -> String {
        let pal: Vec<String> = palette.iter().map(|c| format!("\"{c}\"")).collect();
        let fs: Vec<String> = faces
            .iter()
            .map(|f| format!("    {}, {}, {}, {}, {}", f[0], f[1], f[2], f[3], f[4]))
            .collect();
        format!(
            "{{\n  \"_\": \"prueba\",\n  \"cell\": 16,\n  \"level\": 16,\n  \"size\": [{},{},{}],\n  \"palette\": [{}],\n  \"faces\": [\n{}\n  ]\n}}\n",
            size[0],
            size[1],
            size[2],
            pal.join(","),
            fs.join(",\n")
        )
    }

    /// Las caras de un texto de modelo, de 5 en 5.
    fn faces_of(text: &str) -> Vec<[i32; 5]> {
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        let f: Vec<i32> = v["faces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap() as i32)
            .collect();
        f.chunks(5)
            .map(|c| [c[0], c[1], c[2], c[3], c[4]])
            .collect()
    }

    fn palette_of(text: &str) -> Vec<String> {
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        v["palette"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect()
    }

    fn solids(s: &Structure) -> usize {
        s.grid.data.iter().filter(|&&b| b != 0).count()
    }

    #[test]
    fn ida_y_vuelta_identica() {
        for t in [MINERAL, CHEST, INKWELL, COVER, TABLE] {
            let t = lf(t);
            let (m, s) = load(&t).unwrap();
            assert_eq!(save(&m, &s).unwrap(), marked(&t));
            assert_eq!(s.model_cell, Some(m.cell));
        }
    }

    #[test]
    fn guardar_dos_veces_no_repite_la_marca() {
        let (m, s) = load(&lf(CHEST)).unwrap();
        let once = save(&m, &s).unwrap();
        let (m2, s2) = load(&once).unwrap();
        assert!(m2.edited() && !m.edited());
        assert_eq!(m2.note(), m.note());
        assert_eq!(save(&m2, &s2).unwrap(), once);
    }

    #[test]
    fn reconstruye_el_solido() {
        let t = lf(CHEST);
        let (_, s) = load(&t).unwrap();
        let faces = faces_of(&t);
        // Sólido esperado, por paridad en cada fila: −x abre, +x cierra.
        let mut rows: HashMap<(i32, i32), (Vec<i32>, Vec<i32>)> = HashMap::new();
        for f in &faces {
            let r = rows.entry((f[1], f[2])).or_default();
            match f[3] {
                1 => r.0.push(f[0]),
                0 => r.1.push(f[0]),
                _ => {}
            }
        }
        let mut expected = 0;
        for (open, close) in rows.values_mut() {
            open.sort();
            close.sort();
            expected += open
                .iter()
                .zip(close.iter())
                .map(|(a, b)| (b - a + 1) as usize)
                .sum::<usize>();
        }
        assert_eq!(solids(&s), expected);
        let faced: HashSet<[i32; 3]> = faces.iter().map(|f| [f[0], f[1], f[2]]).collect();
        assert!(
            solids(&s) > faced.len(),
            "el cofre tiene vóxeles enterrados"
        );
        for f in &faces {
            let n = DIRS[f[3] as usize];
            assert_ne!(s.grid.get(f[0], f[1], f[2]), 0);
            assert_eq!(s.grid.get(f[0] + n[0], f[1] + n[1], f[2] + n[2]), 0);
        }
    }

    #[test]
    fn color_de_voxel_y_caras_propias() {
        assert!(!load(&lf(CHEST)).unwrap().1.face_colors.is_empty());
        assert!(load(&lf(MINERAL)).unwrap().1.face_colors.is_empty());
        // Cinco caras del color 0 y la de arriba del 1: manda el más frecuente y la de arriba queda como propia.
        let t = mk(
            [1, 2, 1],
            &["101010", "202020"],
            &[
                [0, 1, 0, 0, 0],
                [0, 1, 0, 1, 0],
                [0, 1, 0, 2, 1],
                [0, 1, 0, 3, 0],
                [0, 1, 0, 4, 0],
                [0, 1, 0, 5, 0],
            ],
        );
        let (_, s) = load(&t).unwrap();
        assert_eq!(s.grid.get(0, 1, 0), 1);
        assert_eq!(
            s.face_colors,
            FaceColors::from([(face_key([0, 1, 0], 2), 2)])
        );
        // Empate a tres: gana el de la cara de arriba.
        let t = mk(
            [1, 2, 1],
            &["101010", "202020", "303030"],
            &[
                [0, 1, 0, 0, 0],
                [0, 1, 0, 1, 0],
                [0, 1, 0, 2, 1],
                [0, 1, 0, 3, 1],
                [0, 1, 0, 4, 2],
                [0, 1, 0, 5, 2],
            ],
        );
        assert_eq!(load(&t).unwrap().1.grid.get(0, 1, 0), 2);
        // Empate sin la de arriba entre los empatados: el de índice menor.
        let t = mk(
            [1, 1, 1],
            &["101010", "202020", "303030"],
            &[
                [0, 0, 0, 0, 1],
                [0, 0, 0, 1, 1],
                [0, 0, 0, 2, 0],
                [0, 0, 0, 4, 2],
                [0, 0, 0, 5, 2],
            ],
        );
        assert_eq!(load(&t).unwrap().1.grid.get(0, 0, 0), 2);
    }

    #[test]
    fn claves_extra_intactas() {
        let (m, s) = load(&lf(COVER)).unwrap();
        let out = save(&m, &s).unwrap();
        assert!(out.contains("\n  \"pivot\": [25.5,0,14],\n  \"hinge\": [25.5,6.5,14],\n"));
        let (m, s) = load(&lf(TABLE)).unwrap();
        assert_eq!(m.cell, 8);
        assert!(save(&m, &s).unwrap().contains("\n  \"cell\": 8,\n"));
        let (m, _) = load(&lf(MINERAL)).unwrap();
        assert_eq!(m.origin, Some([-3, 0, -1]));
    }

    #[test]
    fn cambiar_el_punto_de_apoyo() {
        let (mut m, s) = load(&lf(MINERAL)).unwrap();
        m.origin = Some([1, 0, 2]);
        assert!(save(&m, &s).unwrap().contains("\n  \"origin\": [1,0,2],\n"));
        // Uno que no lo tenía: la línea va antes de la paleta.
        let (mut m, s) = load(&lf(CHEST)).unwrap();
        m.origin = Some([8, 0, 8]);
        let out = save(&m, &s).unwrap();
        assert!(out.contains("\n  \"origin\": [8,0,8],\n  \"palette\": ["));
        assert_eq!(load(&out).unwrap().0.origin, Some([8, 0, 8]));
    }

    #[test]
    fn conserva_crlf() {
        let t = lf(CHEST).replace('\n', "\r\n");
        let (m, s) = load(&t).unwrap();
        let out = save(&m, &s).unwrap();
        assert_eq!(out, marked(&t));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn paleta_compacta_al_guardar() {
        let t = lf(MINERAL);
        let (m, mut s) = load(&t).unwrap();
        let gone = s.grid.data.iter().copied().find(|&b| b != 0).unwrap();
        let rgb = m.palette[gone as usize - 1];
        for b in &mut s.grid.data {
            if *b == gone {
                *b = 0;
            }
        }
        let out = save(&m, &s).unwrap();
        let hex = format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        let pal = palette_of(&out);
        assert!(!pal.contains(&hex));
        assert!(faces_of(&out).iter().all(|f| (f[4] as usize) < pal.len()));
        // Lo guardado se vuelve a abrir y da lo mismo.
        let (m2, s2) = load(&out).unwrap();
        assert_eq!(save(&m2, &s2).unwrap(), out);
        assert_eq!(solids(&s2), solids(&s));
    }

    #[test]
    fn caras_nuevas_al_final() {
        let t = lf(CHEST);
        let (m, mut s) = load(&t).unwrap();
        let before = faces_of(&t);
        // Un vóxel de superficie con un vecino sólido debajo.
        let v = before
            .iter()
            .find(|f| f[3] == 2 && s.grid.get(f[0], f[1] - 1, f[2]) != 0)
            .map(|f| [f[0], f[1], f[2]])
            .unwrap();
        s.grid.set(v[0], v[1], v[2], 0);
        s.face_colors.retain(|k, _| face_unkey(*k).0 != v);
        let out = save(&m, &s).unwrap();
        let after = faces_of(&out);
        let pal_before = palette_of(&t);
        let pal_after = palette_of(&out);
        let rgb = |f: &[i32; 5], pal: &[String]| pal[f[4] as usize].clone();
        let key = |f: &[i32; 5]| (f[0], f[1], f[2], f[3]);
        let kept: Vec<_> = before.iter().filter(|f| [f[0], f[1], f[2]] != v).collect();
        assert!(
            after.len() > kept.len(),
            "al quitarlo quedan caras nuevas al aire"
        );
        for (a, b) in after.iter().zip(&kept) {
            assert_eq!(key(a), key(b));
            assert_eq!(rgb(a, &pal_after), rgb(b, &pal_before));
        }
        let new = &after[kept.len()..];
        assert!(
            new.windows(2).all(
                |w| (w[0][1], w[0][2], w[0][0], w[0][3]) < (w[1][1], w[1][2], w[1][0], w[1][3])
            )
        );
        for f in new {
            let b = s.grid.get(f[0], f[1], f[2]);
            let c = m.palette[b as usize - 1];
            assert_eq!(
                rgb(f, &pal_after),
                format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
            );
        }
    }

    #[test]
    fn modelo_nuevo_y_vacio() {
        let (m, mut s) = Model::new([4, 4, 4]);
        assert!(m.edited());
        let out = save(&m, &s).unwrap();
        let (m2, s2) = load(&out).unwrap();
        assert_eq!(s2.grid.size, [4, 4, 4]);
        assert_eq!(solids(&s2), 0);
        assert!(
            !m2.palette.is_empty(),
            "siempre hay un color con el que pintar"
        );
        s.grid.set(1, 0, 1, 1);
        let out = save(&m, &s).unwrap();
        assert_eq!(
            faces_of(&out).len(),
            5,
            "un vóxel en el suelo: cinco caras, sin la de abajo"
        );
        assert_eq!(palette_of(&out), ["808080"]);
    }

    #[test]
    fn rechaza_lo_que_no_cuadra() {
        let cube = [
            [0, 0, 0, 0, 0],
            [0, 0, 0, 1, 0],
            [0, 0, 0, 2, 0],
            [0, 0, 0, 4, 0],
            [0, 0, 0, 5, 0],
        ];
        let ok = mk([1, 1, 1], &["101010"], &cube);
        assert!(load(&ok).is_ok());
        let mut dup = cube.to_vec();
        dup.push(cube[0]);
        let many: Vec<String> = (0..256).map(|i| format!("{i:06x}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let bad = [
            ("no es JSON", "hola".to_string()),
            ("sin faces", "{\n  \"cell\": 16,\n  \"level\": 16,\n  \"size\": [1,1,1],\n  \"palette\": []\n}\n".to_string()),
            ("faces de 5 en 5", ok.replace("0, 0, 0, 5, 0", "0, 0, 0, 5")),
            ("cara fuera del tamaño", mk([1, 1, 1], &["101010"], &[[3, 0, 0, 0, 0]])),
            ("dirección 6", mk([1, 1, 1], &["101010"], &[[0, 0, 0, 6, 0]])),
            ("color fuera de la paleta", mk([1, 1, 1], &["101010"], &[[0, 0, 0, 0, 4]])),
            ("color que no es hex", mk([1, 1, 1], &["zzzzzz"], &cube)),
            ("cara repetida", mk([1, 1, 1], &["101010"], &dup)),
            ("cara suelta", mk([1, 1, 1], &["101010"], &[[0, 0, 0, 0, 0]])),
            ("falta una cara expuesta", mk([1, 1, 1], &["101010"], &cube[..4])),
            ("lado enorme", mk([300, 1, 1], &["101010"], &cube)),
            ("demasiados colores", mk([1, 1, 1], &many, &cube)),
            ("cell 0", ok.replace("\"cell\": 16", "\"cell\": 0")),
        ];
        for (what, text) in bad {
            let e = load(&text)
                .err()
                .unwrap_or_else(|| panic!("debería rechazar: {what}"));
            assert!(!e.to_string().is_empty());
        }
    }
}
