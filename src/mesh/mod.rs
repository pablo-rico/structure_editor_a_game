//! Mallado greedy de la rejilla: port de `StructureMeshBuilder.Build` del juego (con el suelo de fuera en la UV 2).
//!
//! - Cara solo donde un sub-cubo sólido toca aire o el borde de la rejilla; `Hidden` no se malla pero tapa.
//! - Por corte, eje y sentido, las caras contiguas con la misma clave `material | detalle << 8` se funden en un
//!   rectángulo.
//! - Vértice `(x / 8, y / 8 · LEVEL_HEIGHT, z / 8)`, normal de la cara, color de la paleta (sRGB; alfa 250 en el
//!   detalle), UV 0 = sub-cubos en el plano de la cara, UV 1 = `(plano · 6 + eje · 2 + (sentido > 0), semilla & 0xFFFF)`,
//!   UV 2 = `(filas sobre el suelo de fuera, 1)`. El tono por sub-cubo lo pone el shader (`voxel_tone`).

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::bridge::proto::{Catalog, Rgb};
use crate::model::{LEVEL_HEIGHT, SUB, Structure};

/// Alfa del color de vértice de los sub-cubos de detalle (`StructureMeshBuilder.DetailAlpha`).
pub const DETAIL_ALPHA: u8 = 250;

const DETAIL_KEY: i32 = 256;

/// Vértice de la malla de estructuras (52 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// Color de la paleta en sRGB y alfa ([`DETAIL_ALPHA`] en el detalle).
    pub color: [u8; 4],
    /// UV 0: sub-cubos en el plano de la cara.
    pub uv: [f32; 2],
    /// UV 1: id de la cara y semilla del tono.
    pub face: [f32; 2],
    /// UV 2: filas de sub-cubo sobre el suelo de fuera y marca de la oclusión de contacto.
    pub ground: [f32; 2],
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Con qué se pinta una estructura: la paleta final (tema y color del reino ya aplicados) y el material `Hidden`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paint {
    pub palette: Vec<Rgb>,
    pub hidden: Option<u8>,
}

/// Lo que la vela toma del color del reino (`StructurePalette.SailTeamTint`).
const SAIL_TEAM_TINT: f32 = 0.35;

impl Paint {
    /// Paleta del tema `theme` (escuelas) con el color del reino `team` en `SailCloth` (en parte) y `TeamCloth` (entero),
    /// como `StructurePalette.Color(material, theme, team)`.
    pub fn new(catalog: &Catalog, theme: Option<usize>, team: Option<Rgb>) -> Self {
        let mut palette = catalog.palette_for(theme).to_vec();
        let index = |name: &str| catalog.materials.iter().position(|m| m == name);
        if let Some(team) = team {
            if let Some(i) = index("TeamCloth").filter(|&i| i < palette.len()) {
                palette[i] = team;
            }
            if let Some(i) = index("SailCloth").filter(|&i| i < palette.len()) {
                // Color32.Lerp: trunca como el cast a byte de C#.
                let base = catalog.palette.get(i).copied().unwrap_or(palette[i]);
                palette[i] = std::array::from_fn(|k| {
                    (base[k] as f32 + (team[k] as f32 - base[k] as f32) * SAIL_TEAM_TINT) as u8
                });
            }
        }
        Self {
            palette,
            hidden: index("Hidden").map(|i| i as u8),
        }
    }

    fn color(&self, material: i32) -> Rgb {
        // Magenta si no hay color, como StructurePalette.
        self.palette
            .get(material as usize)
            .copied()
            .unwrap_or([255, 0, 255])
    }
}

/// Esquina del sub-cubo `(x, y, z)` en el espacio de la estructura (`StructureMeshBuilder.SubToLocal`).
pub fn sub_to_local(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(
        x / SUB as f32,
        y / SUB as f32 * LEVEL_HEIGHT,
        z / SUB as f32,
    )
}

/// Malla de toda la rejilla de `s`, con el suelo de fuera a `s.ground_level` niveles de la base.
pub fn build(s: &Structure, paint: &Paint) -> MeshData {
    let g = &s.grid;
    let mut out = MeshData::default();
    if g.data.is_empty() {
        return out;
    }
    let size = g.size;
    // Recorrido sobre los datos en crudo con zancadas por eje (x, y, z).
    let stride = [1, size[0] * size[2], size[0]];
    let data = &g.data;
    let detail = g.detail.as_deref();
    let hidden = paint.hidden.map_or(-1, i32::from);
    let seed = (s.seed & 0xFFFF) as f32;
    let ground_rows = (s.ground_level * SUB) as f32;
    let mut mask: Vec<i32> = Vec::new();
    for d in 0..3 {
        let (u, v) = ((d + 1) % 3, (d + 2) % 3);
        let (du, dv) = (size[u], size[v]);
        mask.resize((du * dv) as usize, 0);
        let (su, sv) = (stride[u], stride[v]);
        for sign in [-1, 1] {
            let neighbour_offset = sign * stride[d];
            for i in 0..size[d] {
                // Fuera de la rejilla, el vecino es aire.
                let neighbour_inside = (0..size[d]).contains(&(i + sign));
                let slice_base = i * stride[d];
                let mut any = false;
                for b in 0..dv {
                    let mut index = slice_base + b * sv;
                    let row = b * du;
                    for a in 0..du {
                        let m = data[index as usize] as i32;
                        // El suelo invisible no se malla, pero tapa las caras de lo que tiene al lado.
                        let open =
                            !neighbour_inside || data[(index + neighbour_offset) as usize] == 0;
                        let mut key = if m != 0 && m != hidden && open { m } else { 0 };
                        if key != 0 && detail.is_some_and(|dt| dt[index as usize] != 0) {
                            key |= DETAIL_KEY;
                        }
                        mask[(row + a) as usize] = key;
                        any |= key != 0;
                        index += su;
                    }
                }
                if !any {
                    continue;
                }
                let plane = i + i32::from(sign > 0);
                let face = [
                    (plane * 6 + d as i32 * 2 + i32::from(sign > 0)) as f32,
                    seed,
                ];
                for b in 0..dv {
                    let mut a = 0;
                    while a < du {
                        let key = mask[(b * du + a) as usize];
                        if key == 0 {
                            a += 1;
                            continue;
                        }
                        let mut w = 1;
                        while a + w < du && mask[(b * du + a + w) as usize] == key {
                            w += 1;
                        }
                        let mut h = 1;
                        while b + h < dv
                            && (0..w).all(|t| mask[((b + h) * du + a + t) as usize] == key)
                        {
                            h += 1;
                        }
                        for r in 0..h {
                            let start = ((b + r) * du + a) as usize;
                            mask[start..start + w as usize].fill(0);
                        }
                        let rgb = paint.color(key & 0xFF);
                        let alpha = if key & DETAIL_KEY != 0 {
                            DETAIL_ALPHA
                        } else {
                            255
                        };
                        let quad = Quad {
                            d,
                            u,
                            v,
                            sign,
                            plane,
                            a,
                            b,
                            w,
                            h,
                        };
                        add_face(
                            &mut out,
                            &quad,
                            [rgb[0], rgb[1], rgb[2], alpha],
                            face,
                            ground_rows,
                        );
                        a += w;
                    }
                }
            }
        }
    }
    out
}

struct Quad {
    d: usize,
    u: usize,
    v: usize,
    sign: i32,
    plane: i32,
    a: i32,
    b: i32,
    w: i32,
    h: i32,
}

fn add_face(o: &mut MeshData, q: &Quad, color: [u8; 4], face: [f32; 2], ground_rows: f32) {
    let corner = |a: i32, b: i32| {
        let mut p = [0i32; 3];
        p[q.d] = q.plane;
        p[q.u] = a;
        p[q.v] = b;
        (sub_to_local(p[0] as f32, p[1] as f32, p[2] as f32), p[1])
    };
    let (a, b, w, h) = (q.a, q.b, q.w, q.h);
    let c = [
        corner(a, b),
        corner(a + w, b),
        corner(a + w, b + h),
        corner(a, b + h),
    ];
    let uv = [[a, b], [a + w, b], [a + w, b + h], [a, b + h]];
    let mut normal = [0.0f32; 3];
    normal[q.d] = q.sign as f32;
    let n = Vec3::from(normal);
    // Unity toma como cara frontal la que recorre sus vértices en sentido horario vista desde fuera.
    let clockwise = (c[1].0 - c[0].0).cross(c[2].0 - c[0].0).dot(n) > 0.0;
    let order = if clockwise {
        [0, 1, 2, 3]
    } else {
        [0, 3, 2, 1]
    };
    let start = o.vertices.len() as u32;
    for k in order {
        let (pos, y) = c[k];
        o.vertices.push(Vertex {
            pos: pos.into(),
            normal,
            color,
            uv: [uv[k][0] as f32, uv[k][1] as f32],
            face,
            // Filas de sub-cubo sobre el suelo (RowsAbove): enteras en las esquinas.
            ground: [y as f32 - ground_rows, 1.0],
        });
    }
    o.indices
        .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
}

/// Tono de un sub-cubo (`VoxelTone` de `World3DLit.shader`): la mitad de las veces el base y el resto algo más claro u
/// oscuro. Aritmética u32 con desbordamiento, como en HLSL. El shader hace lo mismo; esta versión es para los tests.
pub fn voxel_tone(cell: [u32; 2], face: [u32; 2]) -> f32 {
    let mut h = cell[0].wrapping_mul(0x9E37_79B1)
        ^ cell[1].wrapping_mul(0x85EB_CA77)
        ^ face[0].wrapping_mul(0xC2B2_AE3D)
        ^ face[1].wrapping_mul(0x27D4_EB2F);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    match h & 7 {
        0..=3 => 1.0,
        4 => 0.92,
        5 => 1.07,
        6 => 0.86,
        _ => 1.12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VoxelGrid;

    const STONE: u8 = 1;
    const PLANK: u8 = 3;
    const HIDDEN: u8 = 61;

    fn paint() -> Paint {
        let mut palette = vec![[255, 0, 255]; 70];
        palette[STONE as usize] = [0x7c, 0x7c, 0x84];
        palette[PLANK as usize] = [0x9a, 0x6a, 0x3c];
        Paint {
            palette,
            hidden: Some(HIDDEN),
        }
    }

    fn boxed(x: i32, y: i32, z: i32) -> Structure {
        let mut grid = VoxelGrid::new([x, y, z]);
        grid.data.fill(STONE);
        Structure {
            grid,
            ..Default::default()
        }
    }

    fn to_sub(p: [f32; 3]) -> Vec3 {
        Vec3::new(p[0] * 8.0, p[1] / LEVEL_HEIGHT * 8.0, p[2] * 8.0)
    }

    /// Área total de las caras, en caras de sub-cubo.
    fn sub_area(m: &MeshData) -> f32 {
        m.vertices
            .chunks(4)
            .map(|q| {
                (to_sub(q[1].pos) - to_sub(q[0].pos))
                    .cross(to_sub(q[2].pos) - to_sub(q[0].pos))
                    .length()
            })
            .sum()
    }

    fn quads(m: &MeshData) -> usize {
        m.vertices.len() / 4
    }

    #[test]
    fn un_sub_cubo_tiene_seis_caras() {
        let m = build(&boxed(1, 1, 1), &paint());
        assert_eq!(quads(&m), 6);
        assert!((sub_area(&m) - 6.0).abs() < 1e-3);
        assert_eq!(m.indices.len(), 36);
    }

    #[test]
    fn culling_solo_la_superficie() {
        let m = build(&boxed(3, 4, 5), &paint());
        assert!((sub_area(&m) - 2.0 * (3.0 * 4.0 + 4.0 * 5.0 + 3.0 * 5.0)).abs() < 1e-2);
    }

    #[test]
    fn caja_hueca_tambien_emite_las_caras_de_dentro() {
        let mut s = boxed(5, 5, 5);
        for y in 1..4 {
            for z in 1..4 {
                for x in 1..4 {
                    s.grid.set(x, y, z, 0);
                }
            }
        }
        let m = build(&s, &paint());
        assert!((sub_area(&m) - (6.0 * 25.0 + 6.0 * 9.0)).abs() < 1e-2);
    }

    #[test]
    fn greedy_funde_el_mismo_material() {
        let m = build(&boxed(16, 16, 16), &paint());
        assert_eq!(quads(&m), 6, "un cubo de un solo material son seis caras");
        assert!((sub_area(&m) - 6.0 * 256.0).abs() < 1e-2);
    }

    #[test]
    fn greedy_no_funde_materiales_distintos() {
        let mut s = boxed(4, 1, 1);
        s.grid.set(3, 0, 0, PLANK);
        let m = build(&s, &paint());
        // Arriba, abajo, delante y detrás: dos quads por material; más los dos extremos.
        assert_eq!(quads(&m), 4 * 2 + 2);
    }

    #[test]
    fn el_detalle_no_se_funde_con_lo_normal() {
        let mut s = boxed(4, 1, 1);
        let mut detail = vec![0u8; 4];
        detail[3] = 1;
        s.grid.detail = Some(detail);
        let m = build(&s, &paint());
        assert_eq!(quads(&m), 4 * 2 + 2);
        let alphas: Vec<u8> = m.vertices.iter().map(|v| v.color[3]).collect();
        assert!(alphas.contains(&DETAIL_ALPHA) && alphas.contains(&255));
    }

    #[test]
    fn hidden_tapa_sin_mallarse() {
        // Columna de piedra sobre un suelo Hidden: el suelo no se ve y la cara de abajo de la piedra no existe.
        let mut s = boxed(1, 2, 1);
        s.grid.set(0, 0, 0, HIDDEN);
        let m = build(&s, &paint());
        assert_eq!(quads(&m), 5);
        assert!(m.vertices.iter().all(|v| to_sub(v.pos).y >= 1.0 - 1e-4));
        assert!(m.vertices.iter().all(|v| v.normal != [0.0, -1.0, 0.0]));
    }

    #[test]
    fn caras_en_sentido_horario_desde_fuera() {
        let mut s = boxed(4, 3, 2);
        s.grid.set(1, 1, 0, 0);
        let m = build(&s, &paint());
        for t in m.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(m.vertices[t[k] as usize].pos));
            let n = Vec3::from(m.vertices[t[0] as usize].normal);
            assert!((b - a).cross(c - a).dot(n) > 0.0);
        }
    }

    #[test]
    fn uv_cuentan_sub_cubos_e_identifican_la_cara() {
        let mut s = boxed(3, 4, 5);
        s.seed = 0x12345;
        let m = build(&s, &paint());
        let mut faces = Vec::new();
        for q in m.vertices.chunks(4) {
            let (a, b, c) = (to_sub(q[0].pos), to_sub(q[1].pos), to_sub(q[2].pos));
            let uv = |k: usize| glam::Vec2::from(q[k].uv);
            assert!(((b - a).length() - (uv(1) - uv(0)).length()).abs() < 1e-3);
            assert!(((c - b).length() - (uv(2) - uv(1)).length()).abs() < 1e-3);
            assert!(q.iter().all(|v| v.face == q[0].face));
            assert_eq!(q[0].face[1] as i32, 0x2345, "semilla en los 16 bits bajos");
            faces.push(q[0].face.map(f32::to_bits));
        }
        faces.sort();
        faces.dedup();
        assert_eq!(faces.len(), 6);
    }

    #[test]
    fn filas_sobre_el_suelo_en_la_uv2() {
        let mut s = boxed(1, 16, 1);
        s.ground_level = 1;
        let m = build(&s, &paint());
        for v in &m.vertices {
            assert_eq!(v.ground[0], (to_sub(v.pos).y - 8.0).round());
            assert_eq!(v.ground[1], 1.0);
        }
    }

    #[test]
    fn paleta_con_color_del_reino() {
        let catalog: Catalog = serde_json::from_value(serde_json::json!({
            "materials": ["Air", "SailCloth", "TeamCloth", "Hidden"],
            "palette": [[255, 0, 255], [0xe8, 0xe0, 0xc8], [0xa8, 0x35, 0x2e], [255, 0, 255]],
            "elements": [], "styles": [], "facings": [], "damages": [], "groups": [], "kinds": []
        }))
        .unwrap();
        let p = Paint::new(&catalog, None, Some([0, 0, 255]));
        assert_eq!(p.hidden, Some(3));
        assert_eq!(p.palette[2], [0, 0, 255]);
        // 232 + (0 − 232) · 0,35 = 150,8 → 150; 200 + (255 − 200) · 0,35 = 219,25 → 219.
        assert_eq!(p.palette[1], [150, 145, 219]);
        assert_eq!(Paint::new(&catalog, None, None).palette, catalog.palette);
    }

    #[test]
    fn tono_como_el_hash_hlsl() {
        // Valores calculados aparte con la fórmula de World3DLit.shader (u32 con desbordamiento, h & 7).
        assert_eq!(voxel_tone([0, 0], [0, 0]), 1.0); // h = 0
        assert_eq!(voxel_tone([0, 0], [25, 4660]), 1.0); // h = 0x576569b0, r = 0
        assert_eq!(voxel_tone([0, 4], [25, 4660]), 1.0); // h = 0x2533e19a, r = 2
        assert_eq!(voxel_tone([0, 10], [25, 4660]), 0.92); // h = 0x82de206c, r = 4
        assert_eq!(voxel_tone([0, 1], [25, 4660]), 1.07); // h = 0x58caaf0d, r = 5
        assert_eq!(voxel_tone([0, 18], [25, 4660]), 0.86); // h = 0x8c945776, r = 6
        assert_eq!(voxel_tone([0, 2], [25, 4660]), 1.12); // h = 0xac3eeeaf, r = 7
        assert_eq!(voxel_tone([12, 40], [601, 1234]), 1.12); // h = 0x624fb927, r = 7
    }
}
