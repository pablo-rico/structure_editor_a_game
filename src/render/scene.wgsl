// Estructura de sub-cubos con el aspecto del juego: port de AGame/World3DLit con _VOXEL y _STYLE_PIXEL3D
// (World3DLit.shader, PixelArt.hlsl, StructureCutaway.hlsl). Pasadas: principal (color lineal + normal y marca de
// detalle), sombra (profundidad desde el sol) y fantasma (lo que quita el corte, desaturado).

struct Globals {
    view_proj: mat4x4<f32>,
    light_proj: mat4x4<f32>,
    // xyz: posición de la cámara (perspectiva); w: 1 en perspectiva.
    eye: vec4<f32>,
    // xyz: hacia dónde mira la cámara; w: texel del mapa de sombras en unidades de mundo.
    forward: vec4<f32>,
    // xyz: hacia el sol; w: fuerza de la sombra.
    sun: vec4<f32>,
    sun_color: vec4<f32>,
    ambient: vec4<f32>,
    shadow_tint: vec4<f32>,
    // x: y de mundo de los pies; y: 1 si hay corte; z: y de mundo del corte del detalle; w: opacidad del fantasma.
    cut: vec4<f32>,
    // xy: vista sobre el suelo (x, z) unitaria; zw: medio tamaño de la región (la rejilla) en mundo.
    cut_view: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_cmp: sampler_comparison;

const LH: f32 = 0.81649658;
// Sub-cubo en mundo (StructureCutaway: _CutView.zw) y píxel de arte del mundo (_ArtTexel.xy: 1/16 de celda y nivel).
const SUB_XZ: f32 = 0.125;
const SUB_Y: f32 = 0.10206207;
const ART: vec3<f32> = vec3<f32>(0.0625, 0.051031036, 0.0625);
// StructureCutaway.FrontHeight / BackHeight, en niveles.
const CUT_FRONT: f32 = 0.25;
const CUT_BACK: f32 = 1.875;
// World3DLit: tapa del corte, borde bajo el corte, oclusión de contacto y desplazamiento de la sombra.
const CAP_DARKEN: f32 = 0.45;
const RIM_BOOST: f32 = 1.25;
const CONTACT_AO: f32 = 0.68;
const CONTACT_AO_ROWS: f32 = 4.0;
const SHADOW_RECEIVER_OFFSET: f32 = 0.06;
// PixelStyle: umbrales de las bandas toon y luz de la banda media.
const TOON_LOW: f32 = 0.2;
const TOON_HIGH: f32 = 0.6;
const TOON_MID: f32 = 0.55;
// SunLight3D: sesgos del proyector de sombra, en texels del mapa (URP ApplyShadowBias).
const DEPTH_BIAS: f32 = 2.0;
const NORMAL_BIAS: f32 = 1.0;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) face: vec2<f32>,
    @location(5) ground: vec2<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat) face: vec2<f32>,
    @location(5) ground: vec2<f32>,
};

fn pass_through(v: VIn, clip: vec4<f32>) -> VOut {
    var o: VOut;
    o.clip = clip;
    o.pos = v.pos;
    o.normal = v.normal;
    o.color = v.color;
    o.uv = v.uv;
    o.face = v.face;
    o.ground = v.ground;
    return o;
}

@vertex
fn vs_main(v: VIn) -> VOut {
    return pass_through(v, g.view_proj * vec4<f32>(v.pos, 1.0));
}

// Proyector de sombra: lejos del sol DEPTH_BIAS texels y hacia dentro NORMAL_BIAS por la normal (URP).
@vertex
fn vs_shadow(v: VIn) -> VOut {
    let l = g.sun.xyz;
    let texel = g.forward.w;
    let inset = (1.0 - saturate(dot(l, v.normal))) * NORMAL_BIAS * texel;
    let p = v.pos - l * (DEPTH_BIAS * texel) - v.normal * inset;
    return pass_through(v, g.light_proj * vec4<f32>(p, 1.0));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// CutHeight (StructureCutaway.hlsl) con la región = la rejilla, cuantizada a sub-cubos.
fn cut_height(xz: vec2<f32>) -> f32 {
    let half_size = g.cut_view.zw;
    let f = g.cut_view.xy;
    let extent = max(abs(f.x) * half_size.x + abs(f.y) * half_size.y, 1e-3);
    let u = dot(xz - half_size, f) / extent;
    let h = g.cut.x + mix(CUT_FRONT * LH, CUT_BACK * LH, smoothstep(-0.5, 0.5, u));
    return floor(h / SUB_Y + 0.5) * SUB_Y;
}

// CutawayDiscard: −1 si el sub-cubo del fragmento queda por encima del corte; si no, 1 en la última fila (borde).
fn cutaway(p: vec3<f32>, n: vec3<f32>, alpha: f32) -> f32 {
    if (g.cut.y < 0.5) {
        return 0.0;
    }
    let size = vec3<f32>(SUB_XZ, SUB_Y, SUB_XZ);
    let cell = floor((p - n * (0.5 * size)) / size);
    let centre = (cell + 0.5) * size;
    var cut = cut_height(centre.xz);
    // El detalle de la planta (muebles, escaleras) se queda entero.
    if (alpha < 0.99) {
        cut = max(cut, g.cut.z);
    }
    let top = (cell.y + 1.0) * size.y;
    if (top > cut + 0.5 * size.y) {
        return -1.0;
    }
    return select(0.0, 1.0, top > cut - 0.5 * size.y);
}

// VoxelTone: hash u32 del sub-cubo y la cara.
fn voxel_tone(uv: vec2<f32>, face: vec2<f32>) -> f32 {
    // Semilla negativa: modelo, sin tono por sub-cubo (el juego los pinta lisos).
    if (face.y < 0.0) { return 1.0; }
    let cell = vec2<u32>(floor(uv));
    var h = (cell.x * 0x9E3779B1u) ^ (cell.y * 0x85EBCA77u) ^ (u32(face.x) * 0xC2B2AE3Du) ^ (u32(face.y) * 0x27D4EB2Fu);
    h = h ^ (h >> 16u);
    h = h * 0x7FEB352Du;
    h = h ^ (h >> 15u);
    h = h * 0x846CA68Bu;
    h = h ^ (h >> 16u);
    let r = h & 7u;
    if (r < 4u) { return 1.0; }
    if (r == 4u) { return 0.92; }
    if (r == 5u) { return 1.07; }
    if (r == 6u) { return 0.86; }
    return 1.12;
}

fn wrap(d: f32) -> f32 {
    let w = saturate(d * 0.5 + 0.5);
    return w * w;
}

fn toon_band(x: f32) -> f32 {
    if (x >= TOON_HIGH) { return 1.0; }
    if (x >= TOON_LOW) { return TOON_MID; }
    return 0.0;
}

// ArtTexelPos: centro del píxel de arte de la cara que contiene p; por el eje de la normal no se encaja.
fn art_texel_pos(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let snapped = (floor(p / ART) + 0.5) * ART;
    let a = abs(n);
    var along = vec3<f32>(0.0, 0.0, 1.0);
    if (a.x >= a.y && a.x >= a.z) {
        along = vec3<f32>(1.0, 0.0, 0.0);
    } else if (a.y >= a.z) {
        along = vec3<f32>(0.0, 1.0, 0.0);
    }
    return mix(snapped, p, along);
}

// 1 al sol, 0 en la sombra (comparación bilineal como la de URP; luego se umbraliza).
fn shadow_raw(p: vec3<f32>) -> f32 {
    let c = g.light_proj * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || c.z < 0.0 || c.z > 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_map, shadow_cmp, uv, c.z);
}

fn is_front(p: vec3<f32>, n: vec3<f32>) -> bool {
    if (g.eye.w > 0.5) {
        return dot(n, g.eye.xyz - p) > 0.0;
    }
    return dot(n, g.forward.xyz) < 0.0;
}

// WorldLight con _STYLE_PIXEL3D y sin nubes: SH plano + sol en 3 bandas, sombra a todo o nada tintada de frío.
fn world_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let strength = g.sun.w;
    let lit = dot(n, g.sun.xyz);
    var mask = shadow_raw(p + n * SHADOW_RECEIVER_OFFSET);
    mask = mix(1.0, mask, saturate(lit * 4.0));
    mask = step(0.5, mask);
    let band = toon_band(wrap(lit));
    let sun_lit = mask * step(0.001, band);
    let shade = mix(mix(vec3<f32>(1.0), g.shadow_tint.rgb, strength), vec3<f32>(1.0), sun_lit);
    return (g.ambient.rgb + g.sun_color.rgb * band * mask) * shade;
}

struct MainOut {
    @location(0) color: vec4<f32>,
    // xyz: normal de mundo (la de la malla, también en las tapas); w: 1 en el detalle.
    @location(1) normal: vec4<f32>,
};

@fragment
fn fs_main(in: VOut) -> MainOut {
    let n = normalize(in.normal);
    let rim = cutaway(in.pos, n, in.color.a);
    if (rim < 0.0) {
        discard;
    }
    var o: MainOut;
    o.normal = vec4<f32>(n, select(0.0, 1.0, in.color.a < 0.99));
    let base = srgb_to_linear(in.color.rgb);
    if (!is_front(in.pos, n)) {
        // Solo se ve por el corte: luz de arriba, sin sombra (está dentro del muro).
        let cap = g.ambient.rgb + g.sun_color.rgb * wrap(g.sun.y);
        o.color = vec4<f32>(base * (CAP_DARKEN * cap), 1.0);
        return o;
    }
    var albedo = base * voxel_tone(in.uv, in.face) * mix(1.0, RIM_BOOST, rim);
    // Oclusión de contacto: las paredes, en sus primeras filas de sub-cubos sobre el suelo de fuera.
    if (in.ground.y > 0.5 && abs(n.y) < 0.5) {
        albedo = albedo * mix(CONTACT_AO, 1.0, saturate(floor(in.ground.x) / CONTACT_AO_ROWS));
    }
    o.color = vec4<f32>(albedo * world_light(art_texel_pos(in.pos, n), n), 1.0);
    return o;
}

@fragment
fn fs_shadow(in: VOut) {
    if (cutaway(in.pos, normalize(in.normal), in.color.a) < 0.0) {
        discard;
    }
}

struct GhostOut {
    @location(0) color: vec4<f32>,
    @location(1) mask: vec4<f32>,
};

// Plantas de arriba en fantasma: solo lo que el corte quita, en gris, premultiplicado.
@fragment
fn fs_ghost(in: VOut) -> GhostOut {
    if (cutaway(in.pos, normalize(in.normal), in.color.a) >= 0.0) {
        discard;
    }
    let gray = dot(in.color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let a = g.cut.w;
    var o: GhostOut;
    o.color = vec4<f32>(vec3<f32>(gray) * a, a);
    o.mask = vec4<f32>(0.0);
    return o;
}
