//! Formato `.agvox` (docs/contrato.md): fuente de una estructura hecha a mano. JSON con la rejilla en RLE + base64 y
//! los materiales por *nombre*, para que reordenar el enum del juego no cambie lo pintado.

use anyhow::{Context, Result, bail, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::bridge::proto::{Interactable, Part};
use crate::model::{Structure, VoxelGrid};

pub const VERSION: u32 = 1;

/// Lo que se guarda además de la rejilla.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    /// Id estable en el juego; `None` hasta el primer export.
    pub id: Option<u32>,
    pub name: String,
    pub style: String,
    /// Diseño del juego al que sustituye (docs/contrato.md, «`.agvox`»); `None` = estructura propia.
    pub replaces: Option<Replaces>,
    /// Aspecto de las herramientas de construcción.
    pub look: Option<Look>,
    /// Receta de `cantero` que generó la estructura y la huella de lo que escribió (docs/recetas.md).
    pub recipe: Option<RecipeNote>,
}

/// De qué receta salió un documento y cómo quedó: si la huella ya no casa, alguien lo ha retocado después.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeNote {
    pub file: String,
    pub fingerprint: String,
}

/// Diseño procedural del juego que esta estructura sustituye. `rank`, `district` y `tier` a `None` = cualquiera.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replaces {
    pub kind: String,
    pub style: String,
    pub design: i64,
    pub size: [i32; 2],
    /// Lados medianeros sin girar (bit `i` = `dirs[i]`).
    pub party: u32,
    #[serde(default)]
    pub rank: Option<i64>,
    #[serde(default)]
    pub district: Option<String>,
    #[serde(default)]
    pub tier: Option<String>,
}

/// Aspecto: barrio y clase de los que salen los materiales por papel, y los papeles cambiados a mano.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub district: String,
    pub tier: String,
    #[serde(default)]
    pub roles: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    format: String,
    version: u32,
    id: Option<u32>,
    name: String,
    style: String,
    size: [i32; 3],
    ground_level: i32,
    bands: Vec<[i32; 2]>,
    entrance_x: i32,
    courtyard: bool,
    materials: Vec<String>,
    voxels: String,
    detail: Option<String>,
    parts: Vec<Part>,
    interactables: Vec<Interactable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replaces: Option<Replaces>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    look: Option<Look>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recipe: Option<RecipeNote>,
}

/// Tramos `(valor u8, largo u16 LE)`, largo 1–65535.
fn rle(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let v = data[i];
        let n = data[i..].iter().take(u16::MAX as usize).take_while(|&&b| b == v).count();
        out.push(v);
        out.extend((n as u16).to_le_bytes());
        i += n;
    }
    out
}

fn unrle(bytes: &[u8], len: usize) -> Result<Vec<u8>> {
    let (runs, rest) = bytes.as_chunks::<3>();
    ensure!(rest.is_empty(), "RLE cortado");
    let mut out = Vec::with_capacity(len);
    for run in runs {
        let n = u16::from_le_bytes([run[1], run[2]]) as usize;
        ensure!(n > 0, "RLE con un tramo vacío");
        out.extend(std::iter::repeat_n(run[0], n));
    }
    ensure!(out.len() == len, "la rejilla tiene {} sub-cubos; el tamaño pide {len}", out.len());
    Ok(out)
}

/// Serializa a JSON. `materials` = nombres del enum por valor (los del catálogo).
pub fn save(meta: &Meta, s: &Structure, materials: &[String]) -> Result<String> {
    // Tabla local: solo los materiales usados, en orden de primera aparición (aire siempre el 0).
    let mut local: Vec<u8> = vec![0];
    let mut map = [u8::MAX; 256];
    map[0] = 0;
    let mut data = Vec::with_capacity(s.grid.data.len());
    for &m in &s.grid.data {
        if map[m as usize] == u8::MAX {
            map[m as usize] = local.len() as u8;
            local.push(m);
        }
        data.push(map[m as usize]);
    }
    let names = local
        .iter()
        .map(|&m| materials.get(m as usize).cloned().with_context(|| format!("material {m} desconocido")))
        .collect::<Result<Vec<_>>>()?;
    let file = File {
        format: "agvox".into(),
        version: VERSION,
        id: meta.id,
        name: meta.name.clone(),
        style: meta.style.clone(),
        size: s.grid.size,
        ground_level: s.ground_level,
        bands: s.band_starts.iter().zip(&s.band_floors).map(|(a, b)| [*a, *b]).collect(),
        entrance_x: s.entrance[0],
        courtyard: s.courtyard,
        materials: names,
        voxels: STANDARD.encode(rle(&data)),
        detail: s.grid.detail.as_ref().filter(|d| d.iter().any(|&b| b != 0)).map(|d| STANDARD.encode(rle(d))),
        parts: s.parts.iter().map(|p| Part { cell: None, ..p.clone() }).collect(),
        interactables: s.interactables.clone(),
        replaces: meta.replaces.clone(),
        look: meta.look.clone(),
        recipe: meta.recipe.clone(),
    };
    Ok(serde_json::to_string_pretty(&file)? + "\n")
}

/// Lee un `.agvox`. Un material que el juego ya no tiene es un error que dice cuál.
pub fn load(json: &str, materials: &[String]) -> Result<(Meta, Structure)> {
    let f: File = serde_json::from_str(json).context("no es un .agvox válido")?;
    ensure!(f.format == "agvox", "no es un .agvox (format = {})", f.format);
    ensure!(f.version <= VERSION, "este .agvox es de una versión más nueva del editor ({})", f.version);
    ensure!(f.size.iter().all(|&n| n > 0), "tamaño no válido");
    let missing: Vec<&str> =
        f.materials.iter().filter(|n| !materials.contains(n)).map(String::as_str).collect();
    if !missing.is_empty() {
        bail!("el juego ya no tiene estos materiales: {}", missing.join(", "));
    }
    let global: Vec<u8> = f.materials.iter().map(|n| materials.iter().position(|m| m == n).unwrap() as u8).collect();
    let len = (f.size[0] * f.size[1] * f.size[2]) as usize;
    let local = unrle(&STANDARD.decode(&f.voxels).context("rejilla en base64")?, len)?;
    let data = local
        .iter()
        .map(|&i| global.get(i as usize).copied().context("índice de material fuera de la tabla"))
        .collect::<Result<Vec<_>>>()?;
    let detail = match &f.detail {
        Some(b64) => Some(unrle(&STANDARD.decode(b64).context("detalle en base64")?, len)?),
        None => None,
    };
    let s = Structure {
        grid: VoxelGrid { size: f.size, data, detail },
        ground_level: f.ground_level,
        band_starts: f.bands.iter().map(|b| b[0]).collect(),
        band_floors: f.bands.iter().map(|b| b[1]).collect(),
        entrance: [f.entrance_x, -1],
        courtyard: f.courtyard,
        parts: f.parts,
        interactables: f.interactables,
        ..Default::default()
    };
    Ok((Meta { id: f.id, name: f.name, style: f.style, replaces: f.replaces, look: f.look, recipe: f.recipe }, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["Air", "Stone", "Plank", "Glass"].iter().map(|s| s.to_string()).collect()
    }

    fn sample() -> Structure {
        let mut g = VoxelGrid::new([16, 16, 8]);
        for x in 0..16 {
            for z in 0..8 {
                g.set(x, 0, z, 1);
            }
        }
        g.set(3, 5, 2, 3);
        let mut detail = vec![0; g.data.len()];
        detail[g.index(3, 5, 2)] = 1;
        g.detail = Some(detail);
        Structure {
            grid: g,
            ground_level: 1,
            band_starts: vec![8],
            band_floors: vec![1],
            entrance: [1, -1],
            parts: vec![Part {
                kind: "Door".into(),
                pattern: "Solid".into(),
                material: "Plank".into(),
                bbox: [7, 8, 0, 9, 20, 1],
                swing: 1,
                detail: true,
                starts_open: false,
                locked: false,
                cell: Some([0, 0]),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn ida_y_vuelta_da_los_mismos_bytes() {
        let meta = Meta {
            id: Some(7),
            name: "Torre".into(),
            style: "Northern".into(),
            replaces: Some(Replaces {
                kind: "TownHall".into(),
                style: "Northern".into(),
                design: 1,
                size: [10, 10],
                party: 0,
                district: Some("Noble".into()),
                ..Default::default()
            }),
            look: Some(Look { district: "Noble".into(), tier: "Rich".into(), roles: [("wall".into(), "Stone".into())].into() }),
            recipe: Some(RecipeNote { file: "ArtSource/Structures/torre.rhai".into(), fingerprint: "00ff".into() }),
        };
        let json = save(&meta, &sample(), &names()).unwrap();
        let (m2, s2) = load(&json, &names()).unwrap();
        assert_eq!(m2, meta);
        assert_eq!(s2.grid, sample().grid);
        assert_eq!(s2.parts[0].cell, None);
        assert_eq!(save(&m2, &s2, &names()).unwrap(), json);
    }

    #[test]
    fn materiales_por_nombre() {
        let json = save(&Meta::default(), &sample(), &names()).unwrap();
        // El juego reordena su enum: se repinta con los índices nuevos.
        let moved: Vec<String> = ["Air", "Glass", "Stone", "Plank"].iter().map(|s| s.to_string()).collect();
        let (_, s) = load(&json, &moved).unwrap();
        assert_eq!(s.grid.get(0, 0, 0), 2);
        assert_eq!(s.grid.get(3, 5, 2), 1);
        // Y si desaparece uno, se dice cuál.
        let err = load(&json, &names()[..2]).unwrap_err().to_string();
        assert!(err.contains("Glass"), "{err}");
    }

    #[test]
    fn rle_tramos_largos() {
        let data = vec![5u8; 70_000];
        let r = rle(&data);
        assert_eq!(r.len(), 6); // 65535 + 4465
        assert_eq!(unrle(&r, 70_000).unwrap(), data);
        assert!(unrle(&r, 10).is_err());
    }
}
