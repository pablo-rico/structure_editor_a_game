//! Tipos del contrato con el puente (`docs/contrato.md`) y decodificación de la carga binaria de `build`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::model::{Nav, Structure, VoxelGrid};

pub type Rgb = [u8; 3];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub message: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol: u32,
    pub agame_root: String,
    pub unity_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub name: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub name: String,
    pub label: String,
    pub palette: Vec<Rgb>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Facing {
    pub value: i32,
    pub label: String,
    pub turns: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SizeRange {
    pub min: [i32; 2],
    pub max: [i32; 2],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opt {
    pub value: i64,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bit {
    pub bit: u32,
    pub label: String,
}

/// Tipo de un campo de variante; el control que le toca está en `docs/diseno-gui.md`.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FieldKind {
    #[serde(rename_all = "camelCase")]
    Index {
        min: i64,
        max: i64,
        #[serde(default)]
        max_by_style: HashMap<String, i64>,
    },
    Choice {
        options: Vec<Opt>,
    },
    Mask {
        bits: Vec<Bit>,
    },
    Bool,
    Element,
    Range {
        min: i64,
        max: i64,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    pub label: String,
    pub default: i64,
    #[serde(flatten)]
    pub kind: FieldKind,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindInfo {
    pub id: i32,
    pub name: String,
    pub label: String,
    pub group: String,
    #[serde(default)]
    pub legacy: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub styles: Vec<String>,
    pub size: Option<SizeRange>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub part_states: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub materials: Vec<String>,
    pub palette: Vec<Rgb>,
    pub elements: Vec<Element>,
    #[serde(default)]
    pub team_materials: Vec<String>,
    pub styles: Vec<Named>,
    pub facings: Vec<Facing>,
    pub damages: Vec<Named>,
    pub groups: Vec<Group>,
    pub kinds: Vec<KindInfo>,
    /// Barrios y clases (nombres de `CityDistrict`/`DistrictTier`) para el Aspecto y las sustituciones.
    #[serde(default)]
    pub districts: Vec<Named>,
    #[serde(default)]
    pub tiers: Vec<Named>,
}

impl Catalog {
    pub fn kind(&self, name: &str) -> Option<&KindInfo> {
        self.kinds.iter().find(|k| k.name == name)
    }

    /// Paleta con la que se pinta una estructura: la del elemento si la tiñe un tema (escuelas), la base si no.
    pub fn palette_for(&self, theme: Option<usize>) -> &[Rgb] {
        theme.and_then(|t| self.elements.get(t)).map_or(&self.palette, |e| &e.palette)
    }
}

/// Petición `build` (sin `id` ni `op`, que pone el cliente).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuildRequest {
    pub kind: String,
    pub style: String,
    pub fields: HashMap<String, i64>,
    pub seed: i32,
    pub width: i32,
    pub depth: i32,
    pub facing: i32,
    pub foundation: i32,
    pub damage: String,
    pub dynamic: u32,
    pub party: u32,
    /// Construir el plano del juego saltándose las sustituciones hechas a mano (ver el original).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub procedural: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub kind: String,
    pub pattern: String,
    pub material: String,
    #[serde(rename = "box")]
    pub bbox: [i32; 6],
    pub swing: i32,
    pub detail: bool,
    pub starts_open: bool,
    pub locked: bool,
    /// Celda de la parte (`PartCells`); no va en `.agvox`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<[i32; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Interactable {
    pub kind: String,
    #[serde(rename = "box")]
    pub bbox: [i32; 6],
    pub arg: i32,
    pub facing: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum CheckAt {
    Cell { cell: [i32; 2], level: i32 },
    Voxel { voxel: [i32; 3] },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub at: Option<CheckAt>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavHeader {
    pub levels: i32,
    pub cells_x: i32,
    pub cells_z: i32,
    pub dirs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadLens {
    pub grid: usize,
    pub detail: usize,
    pub nav_floors: usize,
    pub nav_slabs: usize,
    pub nav_edges: usize,
}

/// Cabecera de una respuesta `build` o `validate`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildHeader {
    #[serde(default)]
    pub ms: f64,
    pub size: [i32; 3],
    pub ground_level: i32,
    pub band_starts: Vec<i32>,
    pub band_floors: Vec<i32>,
    pub entrance: [i32; 2],
    #[serde(default)]
    pub courtyard: bool,
    #[serde(default)]
    pub theme: Option<usize>,
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default)]
    pub interactables: Vec<Interactable>,
    #[serde(default)]
    pub footprint: Option<String>,
    pub nav: NavHeader,
    #[serde(default)]
    pub checks: Vec<Check>,
    pub payload: PayloadLens,
    /// Lo construido es una sustitución hecha a mano de este diseño del juego.
    #[serde(default, rename = "override")]
    pub override_by: Option<OverrideInfo>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverrideInfo {
    pub file: String,
    pub name: String,
}

/// Herramienta de construcción del generador (`tools`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: String,
    /// `rect`, `line`, `face` o `cell`.
    pub target: String,
    #[serde(default)]
    pub params: Vec<Field>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct WidthRange {
    pub min: i32,
    pub max: i32,
    pub default: i32,
}

/// Mueble del juego (`furniture`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    pub id: String,
    pub label: String,
    pub category: String,
    #[serde(default)]
    pub size: [i32; 3],
    #[serde(default)]
    pub width: Option<WidthRange>,
    #[serde(default)]
    pub interacts: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FurnitureCatalog {
    pub categories: Vec<Group>,
    pub pieces: Vec<Piece>,
}

/// Pieza recortada de `stamp`: rejilla, interactuables y el sub-cubo de apoyo.
#[derive(Clone, Debug)]
pub struct Stamp {
    pub grid: VoxelGrid,
    pub interactables: Vec<Interactable>,
    pub anchor: [i32; 3],
}

/// Decodifica la respuesta de `stamp` (sin navegación).
pub fn decode_stamp(h: &serde_json::Value, payload: &[u8]) -> Result<Stamp> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct H {
        size: [i32; 3],
        #[serde(default)]
        interactables: Vec<Interactable>,
        #[serde(default)]
        anchor: [i32; 3],
        payload: PayloadLens,
    }
    let h: H = serde_json::from_value(h.clone()).context("cabecera de stamp")?;
    let n = (h.size[0] * h.size[1] * h.size[2]) as usize;
    ensure!(h.payload.grid == n && payload.len() >= n + h.payload.detail, "pieza de tamaño inesperado");
    let detail = (h.payload.detail == n).then(|| payload[n..2 * n].to_vec());
    Ok(Stamp {
        grid: VoxelGrid { size: h.size, data: payload[..n].to_vec(), detail },
        interactables: h.interactables,
        anchor: h.anchor,
    })
}

fn u64s(bytes: &[u8]) -> Vec<u64> {
    bytes.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c)).collect()
}

/// Monta la estructura a partir de la cabecera y la carga de `build`. `grid` es la rejilla si la carga no la trae
/// (`validate`).
pub fn decode_build(h: BuildHeader, payload: &[u8], seed: i32, grid: Option<VoxelGrid>) -> Result<Structure> {
    use base64::Engine as _;
    let p = h.payload;
    let total = p.grid + p.detail + p.nav_floors + p.nav_slabs + p.nav_edges;
    ensure!(payload.len() == total, "carga de {} bytes; la cabecera dice {total}", payload.len());
    let voxels = (h.size[0] * h.size[1] * h.size[2]) as usize;
    let cells = (h.nav.cells_x * h.nav.cells_z) as usize;
    ensure!(p.nav_floors == cells * 8 && p.nav_slabs == cells * 8, "navegación de tamaño inesperado");
    ensure!(p.nav_edges == cells * h.nav.levels as usize, "aristas de tamaño inesperado");
    let (grid_b, rest) = payload.split_at(p.grid);
    let (detail_b, rest) = rest.split_at(p.detail);
    let (floors_b, rest) = rest.split_at(p.nav_floors);
    let (slabs_b, edges_b) = rest.split_at(p.nav_slabs);
    let grid = match grid {
        Some(g) if p.grid == 0 => g,
        _ => {
            ensure!(p.grid == voxels, "rejilla de {} bytes; el tamaño pide {voxels}", p.grid);
            let detail = match p.detail {
                0 => None,
                n if n == voxels => Some(detail_b.to_vec()),
                n => bail!("detalle de {n} bytes; el tamaño pide {voxels}"),
            };
            VoxelGrid { size: h.size, data: grid_b.to_vec(), detail }
        }
    };
    let override_by = h.override_by.map(|o| o.name);
    let footprint = match &h.footprint {
        Some(b64) => Some(base64::engine::general_purpose::STANDARD.decode(b64).context("huella en base64")?),
        None => None,
    };
    Ok(Structure {
        grid,
        seed,
        ground_level: h.ground_level,
        band_starts: h.band_starts,
        band_floors: h.band_floors,
        entrance: h.entrance,
        courtyard: h.courtyard,
        theme: h.theme,
        parts: h.parts,
        interactables: h.interactables,
        footprint,
        nav: Nav {
            levels: h.nav.levels,
            cells_x: h.nav.cells_x,
            cells_z: h.nav.cells_z,
            dirs: h.nav.dirs,
            floors: u64s(floors_b),
            slabs: u64s(slabs_b),
            edges: edges_b.to_vec(),
        },
        checks: h.checks,
        override_by,
    })
}

/// Fixture escrita por `dump`: `<dir>/<name>.json` (cabecera + `request`) y `<dir>/<name>.bin` (carga).
pub fn load_fixture(dir: &Path, name: &str) -> Result<(BuildRequest, Structure)> {
    #[derive(Deserialize)]
    struct Fixture {
        request: BuildRequest,
        #[serde(flatten)]
        header: BuildHeader,
    }
    let json = std::fs::read(dir.join(format!("{name}.json"))).with_context(|| format!("fixture {name}.json"))?;
    let f: Fixture = serde_json::from_slice(&json).with_context(|| format!("fixture {name}.json"))?;
    let bin = std::fs::read(dir.join(format!("{name}.bin"))).with_context(|| format!("fixture {name}.bin"))?;
    let seed = f.request.seed;
    Ok((f.request, decode_build(f.header, &bin, seed, None)?))
}

/// Catálogo de las fixtures (`fixtures/catalog.json`).
pub fn load_catalog_fixture(dir: &Path) -> Result<Catalog> {
    let json = std::fs::read(dir.join("catalog.json")).context("fixtures/catalog.json")?;
    Ok(serde_json::from_slice(&json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifica_carga_minima() {
        let h: BuildHeader = serde_json::from_value(serde_json::json!({
            "size": [8, 8, 8], "groundLevel": 1, "bandStarts": [8], "bandFloors": [1], "entrance": [0, -1],
            "nav": {"levels": 2, "cellsX": 1, "cellsZ": 1, "dirs": ["+X", "+Z", "-X", "-Z"]},
            "checks": [{"severity": "error", "code": "x", "message": "m", "at": {"cell": [0, 0], "level": 1}}],
            "payload": {"grid": 512, "detail": 0, "navFloors": 8, "navSlabs": 8, "navEdges": 2}
        }))
        .unwrap();
        let mut payload = vec![1u8; 512];
        payload.extend(2u64.to_le_bytes());
        payload.extend(2u64.to_le_bytes());
        payload.extend([0b0001, 0]);
        let s = decode_build(h, &payload, 5, None).unwrap();
        assert_eq!(s.grid.get(7, 7, 7), 1);
        assert!(s.nav.has_floor(0, 0, 1) && !s.nav.has_floor(0, 0, 0));
        assert_eq!(s.nav.edges_at(0, 0, 0), 1);
        assert_eq!(s.checks[0].at, Some(CheckAt::Cell { cell: [0, 0], level: 1 }));
    }

    #[test]
    fn campos_de_variante() {
        let f: Field = serde_json::from_value(serde_json::json!(
            {"key": "design", "label": "Diseño", "type": "index", "min": 0, "max": 5, "default": 0,
             "maxByStyle": {"Arabic": 3}}
        ))
        .unwrap();
        assert!(matches!(f.kind, FieldKind::Index { max: 5, ref max_by_style, .. } if max_by_style["Arabic"] == 3));
        let b: Field = serde_json::from_value(serde_json::json!(
            {"key": "scaffold", "label": "En obra", "type": "bool", "default": 0}
        ))
        .unwrap();
        assert!(matches!(b.kind, FieldKind::Bool));
    }
}
