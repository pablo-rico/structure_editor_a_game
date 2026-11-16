//! MagicaVoxel (`.vox`): importar modelos (asociando cada color a un material del juego) y exportar estructuras.
//!
//! Ejes: MagicaVoxel es Z arriba y diestro; el juego es Y arriba con +Z al norte. Para no reflejar el modelo, la Y del
//! `.vox` es el −Z del juego y la Z del `.vox` es la Y del juego (docs/contrato.md no lo necesita: es solo del editor).

use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, anyhow, bail};
use dot_vox::{Color, DotVoxData, Model, SceneNode, Size, Voxel};

use crate::bridge::proto::Rgb;
use crate::model::{SUB, Structure, VoxelGrid};

/// Máximo por eje de un modelo de MagicaVoxel.
pub const MAX: i32 = 256;

/// Un color del `.vox` y cuántos sub-cubos lo usan.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxColor {
    pub index: u8,
    pub rgb: Rgb,
    pub count: usize,
}

/// Lo leído de un `.vox`, ya en ejes del juego.
#[derive(Clone, Debug)]
pub struct VoxFile {
    pub size: [i32; 3],
    pub cells: Vec<([i32; 3], u8)>,
    /// Colores por cantidad de sub-cubos, de más a menos.
    pub colors: Vec<VoxColor>,
    pub models: usize,
    /// Algún modelo estaba girado en la escena (se importa sin el giro).
    pub rotated: bool,
}

/// Recorre la escena y devuelve cada modelo con su esquina (en ejes del `.vox`).
fn placed(data: &DotVoxData) -> (Vec<(usize, [i32; 3])>, bool) {
    if data.scenes.is_empty() {
        return ((0..data.models.len()).map(|m| (m, [0; 3])).collect(), false);
    }
    let mut out = Vec::new();
    let mut rotated = false;
    let mut stack = vec![(0u32, [0i32; 3])];
    while let Some((node, off)) = stack.pop() {
        match data.scenes.get(node as usize) {
            Some(SceneNode::Transform { frames, child, .. }) => {
                let frame = frames.first();
                // `_r` = 4 es la identidad (`Rotation::IDENTITY`); cualquier otro valor es un giro que no se importa.
                rotated |= frame.and_then(|f| f.attributes.get("_r")).is_some_and(|r| r.trim() != "4");
                let t = frame.and_then(|f| f.position()).map_or([0; 3], |p| [p.x, p.y, p.z]);
                stack.push((*child, [off[0] + t[0], off[1] + t[1], off[2] + t[2]]));
            }
            Some(SceneNode::Group { children, .. }) => stack.extend(children.iter().map(|c| (*c, off))),
            Some(SceneNode::Shape { models, .. }) => {
                for m in models {
                    // La traslación es el centro del modelo: la esquina queda a media medida.
                    if let Some(model) = data.models.get(m.model_id as usize) {
                        let s = model.size;
                        let corner = [off[0] - s.x as i32 / 2, off[1] - s.y as i32 / 2, off[2] - s.z as i32 / 2];
                        out.push((m.model_id as usize, corner));
                    }
                }
            }
            None => {}
        }
    }
    (out, rotated)
}

/// Lee un `.vox`. Falla con un mensaje para artistas si pasa de 256 por eje o está vacío.
pub fn read(bytes: &[u8]) -> Result<VoxFile> {
    let data = dot_vox::load_bytes(bytes).map_err(|e| anyhow!("no es un .vox válido: {e}"))?;
    let (models, rotated) = placed(&data);
    let mut raw = Vec::new();
    for (m, c) in &models {
        for v in &data.models[*m].voxels {
            raw.push(([c[0] + v.x as i32, c[1] + v.y as i32, c[2] + v.z as i32], v.i));
        }
    }
    if raw.is_empty() {
        bail!("el .vox no tiene ningún vóxel");
    }
    let lo = |k: usize| raw.iter().map(|(p, _)| p[k]).min().unwrap();
    let hi = |k: usize| raw.iter().map(|(p, _)| p[k]).max().unwrap();
    let (lo, hi) = ([lo(0), lo(1), lo(2)], [hi(0), hi(1), hi(2)]);
    // Ejes del juego: x = x, y = z del .vox, z = −y del .vox.
    let size = [hi[0] - lo[0] + 1, hi[2] - lo[2] + 1, hi[1] - lo[1] + 1];
    for (axis, n) in ["de ancho", "de fondo", "de alto"].iter().zip([size[0], size[2], size[1]]) {
        if n > MAX {
            bail!("Mide {n} sub-cubos {axis}; el máximo es {MAX}. Recórtalo en MagicaVoxel.");
        }
    }
    let mut counts: HashMap<u8, usize> = HashMap::new();
    let cells = raw
        .iter()
        .map(|(p, i)| {
            *counts.entry(*i).or_default() += 1;
            ([p[0] - lo[0], p[2] - lo[2], hi[1] - p[1]], *i)
        })
        .collect();
    let mut colors: Vec<VoxColor> = counts
        .into_iter()
        .map(|(index, count)| {
            let c = data.palette.get(index as usize).copied().unwrap_or(Color { r: 255, g: 0, b: 255, a: 255 });
            VoxColor { index, rgb: [c.r, c.g, c.b], count }
        })
        .collect();
    colors.sort_by(|a, b| b.count.cmp(&a.count).then(a.index.cmp(&b.index)));
    Ok(VoxFile { size, cells, colors, models: models.len(), rotated })
}

/// Distancia de color percibida («redmean»): barata y suficiente para asociar colores a materiales.
pub fn distance(a: Rgb, b: Rgb) -> f32 {
    let rm = (a[0] as f32 + b[0] as f32) / 2.0;
    let d = |k: usize| a[k] as f32 - b[k] as f32;
    ((2.0 + rm / 256.0) * d(0) * d(0) + 4.0 * d(1) * d(1) + (2.0 + (255.0 - rm) / 256.0) * d(2) * d(2)).sqrt()
}

/// Por encima de esta distancia se avisa de que «ninguno se parece».
pub const FAR: f32 = 90.0;

/// Paleta de exportación: la del juego, con los colores repetidos desplazados ±1 para que la vuelta sea exacta.
pub fn export_palette(palette: &[Rgb]) -> Vec<Rgb> {
    let mut out: Vec<Rgb> = Vec::with_capacity(palette.len());
    for &c in palette {
        let mut c = c;
        let mut k = 0u8;
        while out.contains(&c) {
            k = k.wrapping_add(1);
            c = [c[0], c[1].wrapping_add(if k % 2 == 1 { 1 } else { 254 }), c[2].wrapping_add(k / 2)];
        }
        out.push(c);
    }
    out
}

/// Asociación automática color → material: exacta si el color sale de una exportación nuestra; si no, el material
/// usable más parecido. `usable[m]` dice qué materiales se proponen.
pub fn auto_map(file: &VoxFile, palette: &[Rgb], usable: &[bool]) -> BTreeMap<u8, Option<u8>> {
    let exact = export_palette(palette);
    file.colors
        .iter()
        .map(|c| {
            let m = exact.iter().position(|e| *e == c.rgb).filter(|&m| m != 0).or_else(|| {
                (1..palette.len())
                    .filter(|&m| usable.get(m).copied().unwrap_or(false))
                    .min_by(|&a, &b| distance(c.rgb, palette[a]).total_cmp(&distance(c.rgb, palette[b])))
            });
            (c.index, m.map(|m| m as u8))
        })
        .collect()
}

/// Estructura nueva con lo importado: medidas redondeadas a celdas enteras y, si `foundation` es `Some(material)`, un
/// nivel de cimientos de ese material debajo de todo (el modelo empieza en el nivel 1).
pub fn to_structure(file: &VoxFile, map: &BTreeMap<u8, Option<u8>>, foundation: Option<u8>) -> Structure {
    let lift = if foundation.is_some() { SUB } else { 0 };
    let up = |n: i32| (n + SUB - 1) / SUB * SUB;
    let size = [up(file.size[0]), up(file.size[1] + lift), up(file.size[2])];
    let mut grid = VoxelGrid::new(size);
    if let Some(m) = foundation {
        for y in 0..lift {
            for z in 0..size[2] {
                for x in 0..size[0] {
                    grid.set(x, y, z, m);
                }
            }
        }
    }
    for (p, i) in &file.cells {
        if let Some(Some(m)) = map.get(i) {
            grid.set(p[0], p[1] + lift, p[2], *m);
        }
    }
    // Una planta cada 2 niveles por encima del suelo (se pueden cambiar en Propiedades).
    let ground = lift / SUB;
    let floors: Vec<i32> = (ground..size[1] / SUB).step_by(2).collect();
    Structure {
        band_starts: floors.iter().map(|f| f * SUB).collect(),
        band_floors: floors,
        ground_level: ground.max(1),
        entrance: [size[0] / SUB / 2, -1],
        grid,
        ..Default::default()
    }
}

/// Modelo del juego nuevo con lo importado: los colores del `.vox` pasan tal cual a su paleta.
pub fn to_model(file: &VoxFile) -> Result<(crate::vmodel::Model, Structure)> {
    let max = crate::vmodel::MAX_COLORS;
    if file.colors.len() > max {
        bail!("El .vox usa {} colores y un modelo del juego admite {max} como mucho.", file.colors.len());
    }
    let (mut model, mut s) = crate::vmodel::Model::new(file.size);
    model.palette = file.colors.iter().map(|c| c.rgb).collect();
    for (p, i) in &file.cells {
        if let Some(at) = file.colors.iter().position(|c| c.index == *i) {
            s.grid.set(p[0], p[1], p[2], at as u8 + 1);
        }
    }
    if model.palette.is_empty() {
        model.palette.push([128, 128, 128]);
    }
    Ok((model, s))
}

/// Escribe un modelo del juego como `.vox`, con su paleta. El sombreado por cara no cabe en un `.vox`.
pub fn write_model(model: &crate::vmodel::Model, s: &Structure) -> Result<Vec<u8>> {
    write(s, &model.paint().palette, None)
}

/// Escribe la estructura como `.vox` (un modelo). Falla si pasa de 256 por eje.
pub fn write(s: &Structure, palette: &[Rgb], hidden: Option<u8>) -> Result<Vec<u8>> {
    let [sx, sy, sz] = s.grid.size;
    if sx > MAX || sy > MAX || sz > MAX {
        bail!("Mide {sx}×{sz}×{sy} sub-cubos; MagicaVoxel admite hasta {MAX} por lado (32 celdas).");
    }
    let mut voxels = Vec::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let m = s.grid.get(x, y, z);
                if m == 0 || Some(m) == hidden || m as usize > 255 {
                    continue;
                }
                voxels.push(Voxel { x: x as u8, y: (sz - 1 - z) as u8, z: y as u8, i: m - 1 });
            }
        }
    }
    let mut colors: Vec<Color> = export_palette(palette)
        .iter()
        .skip(1)
        .map(|c| Color { r: c[0], g: c[1], b: c[2], a: 255 })
        .collect();
    colors.resize(256, Color { r: 0, g: 0, b: 0, a: 255 });
    let data = DotVoxData {
        version: 150,
        index_map: dot_vox::DEFAULT_INDEX_MAP.to_vec(),
        models: vec![Model { size: Size { x: sx as u32, y: sz as u32, z: sy as u32 }, voxels }],
        palette: colors,
        materials: Vec::new(),
        scenes: Vec::new(),
        layers: Vec::new(),
    };
    let mut out = Vec::new();
    data.write_vox(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Vec<Rgb> {
        // Aire, piedra, teja y un material repetido con el color de la piedra.
        vec![[255, 0, 255], [120, 120, 120], [200, 60, 40], [120, 120, 120]]
    }

    #[test]
    fn ida_y_vuelta_exacta_y_sin_reflejar() {
        let mut s = Structure { grid: VoxelGrid::new([8, 8, 16]), ..Default::default() };
        s.grid.set(1, 2, 3, 2); // teja
        s.grid.set(7, 0, 15, 3); // el repetido: tiene que volver como 3, no como 1
        s.grid.set(0, 0, 0, 1);
        let bytes = write(&s, &palette(), None).unwrap();
        let f = read(&bytes).unwrap();
        assert_eq!(f.size, [8, 3, 16]); // recortado a lo que hay
        let map = auto_map(&f, &palette(), &[false, true, true, true]);
        let back = to_structure(&f, &map, None);
        assert_eq!(back.grid.size, [8, 8, 16]);
        assert_eq!(back.grid.get(1, 2, 3), 2);
        assert_eq!(back.grid.get(7, 0, 15), 3);
        assert_eq!(back.grid.get(0, 0, 0), 1);
        assert_eq!(back.grid.data.iter().filter(|&&m| m != 0).count(), 3);
    }

    #[test]
    fn colores_desconocidos_al_mas_parecido_y_cimientos() {
        let f = VoxFile {
            size: [2, 1, 2],
            cells: vec![([0, 0, 0], 5), ([1, 0, 1], 9)],
            colors: vec![VoxColor { index: 5, rgb: [210, 70, 50], count: 1 }, VoxColor { index: 9, rgb: [0, 0, 250], count: 1 }],
            models: 1,
            rotated: false,
        };
        let map = auto_map(&f, &palette(), &[false, true, true, false]);
        assert_eq!(map[&5], Some(2)); // rojizo → teja
        let mut map2 = map.clone();
        map2.insert(9, None); // «No importar este color»
        let s = to_structure(&f, &map2, Some(1));
        assert_eq!(s.grid.size, [8, 16, 8], "1 de alto + 8 de cimientos, redondeado a niveles");
        assert_eq!(s.grid.get(0, 0, 0), 1, "cimientos de piedra");
        assert_eq!(s.grid.get(0, 8, 0), 2, "el modelo empieza en el nivel 1");
        assert_eq!(s.grid.get(1, 8, 1), 0, "color descartado");
        assert_eq!(s.ground_level, 1);
    }

    #[test]
    fn modelo_vox_ida_y_vuelta() {
        let text = crate::vmodel::tests::lf(crate::vmodel::tests::MINERAL);
        let (m, s) = crate::vmodel::load(&text).unwrap();
        let colors = |m: &crate::vmodel::Model, s: &Structure| {
            let mut c: Vec<Rgb> = s.grid.data.iter().filter(|&&b| b != 0).map(|&b| m.palette[b as usize - 1]).collect();
            c.sort();
            c
        };
        let (m2, s2) = to_model(&read(&write_model(&m, &s).unwrap()).unwrap()).unwrap();
        assert_eq!(colors(&m2, &s2), colors(&m, &s));
        assert_eq!(s2.model_cell, Some(16));
        assert!(m2.palette.len() <= m.palette.len());
        // Lo importado se puede guardar como modelo del juego.
        assert!(crate::vmodel::save(&m2, &s2).is_ok());
    }

    #[test]
    fn modelo_vox_con_demasiados_colores() {
        let f = VoxFile {
            size: [16, 16, 1],
            cells: (0..256).map(|i| ([i % 16, i / 16, 0], i as u8)).collect(),
            colors: (0..256).map(|i| VoxColor { index: i as u8, rgb: [i as u8, 0, 0], count: 1 }).collect(),
            models: 1,
            rotated: false,
        };
        assert!(to_model(&f).unwrap_err().to_string().contains("255"));
    }

    #[test]
    fn demasiado_grande() {
        let s = Structure { grid: VoxelGrid::new([264, 8, 8]), ..Default::default() };
        assert!(write(&s, &palette(), None).unwrap_err().to_string().contains("256"));
    }
}
