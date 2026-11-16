// Pasada a pantalla completa en la textura de arte (un texel = un píxel de arte): contorno de PixelOutline.shader,
// paleta LUT de URP (Color Lookup, en sRGB), fondo, rayado del detalle y las capas (con su contorno de 1 píxel).
// Escribe sRGB en una Rgba8Unorm.

struct Post {
    // rgb: fondo del visor (sRGB).
    bg: vec4<f32>,
    // rgb: tinte del contorno (lineal); w: salto de profundidad del borde (u).
    outline: vec4<f32>,
    // x: oscurecido del borde; y: aclarado de la arista; z: 1 con LUT; w: 1 en perspectiva.
    params: vec4<f32>,
    // x: plano cercano; y: lejano.
    depth: vec4<f32>,
    // rgb: color del rayado del detalle (sRGB); w: su opacidad (0: capa apagada).
    hatch: vec4<f32>,
};

@group(0) @binding(0) var color_tex: texture_2d<f32>;
@group(0) @binding(1) var normal_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
@group(0) @binding(3) var overlay_color: texture_2d<f32>;
@group(0) @binding(4) var overlay_mask: texture_2d<f32>;
@group(0) @binding(5) var lut_tex: texture_2d<f32>;
@group(0) @binding(6) var lut_smp: sampler;
@group(0) @binding(7) var<uniform> p: Post;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn size() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth_tex));
}

fn raw_depth(px: vec2<i32>) -> f32 {
    return textureLoad(depth_tex, clamp(px, vec2<i32>(0), size() - 1), 0);
}

// Profundidad de vista lineal (u).
fn eye_depth(px: vec2<i32>) -> f32 {
    let d = raw_depth(px);
    let n = p.depth.x;
    let f = p.depth.y;
    if (p.params.w > 0.5) {
        return n * f / (f - d * (f - n));
    }
    return n + d * (f - n);
}

fn normal_at(px: vec2<i32>) -> vec3<f32> {
    return textureLoad(normal_tex, clamp(px, vec2<i32>(0), size() - 1), 0).xyz;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// ApplyLut2D de URP sobre la tira 1024×32 (rojo en x, verde en y, azul por tira). La v de Unity crece hacia arriba.
fn apply_lut(c: vec3<f32>) -> vec3<f32> {
    let so = vec3<f32>(1.0 / 1024.0, 1.0 / 32.0, 31.0);
    let z = c.z * so.z;
    let shift = floor(z);
    var uv = c.xy * so.z * so.xy + so.xy * 0.5;
    uv.x = uv.x + shift * so.y;
    uv.y = 1.0 - uv.y;
    let a = textureSampleLevel(lut_tex, lut_smp, uv, 0.0).rgb;
    let b = textureSampleLevel(lut_tex, lut_smp, uv + vec2<f32>(so.y, 0.0), 0.0).rgb;
    return mix(a, b, z - shift);
}

@fragment
fn fs_post(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(frag.xy);
    var c = p.bg.rgb;
    if (raw_depth(px) < 1.0) {
        var lin = textureLoad(color_tex, px, 0).rgb;
        // PixelOutline: borde exterior hacia el tinte; si no, arista convexa más clara.
        let depth = eye_depth(px);
        let normal = normal_at(px);
        var outer = 0.0;
        var crease = 0.0;
        let dirs = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
        for (var i = 0; i < 4; i = i + 1) {
            let q = px + dirs[i];
            let diff = eye_depth(q) - depth;
            outer = max(outer, step(p.outline.w, diff));
            if (abs(diff) < p.outline.w) {
                let n = normal_at(q);
                let open = dot(normal - n, vec3<f32>(1.0));
                let convex = smoothstep(-0.01, 0.01, open) * step(-0.0025, diff * 0.25);
                crease = max(crease, (1.0 - dot(normal, n)) * convex);
            }
        }
        if (outer > 0.0) {
            lin = mix(lin, lin * p.outline.rgb, p.params.x);
        } else if (crease > 0.15) {
            lin = lin * (1.0 + p.params.y);
        }
        c = linear_to_srgb(saturate(lin));
        if (p.params.z > 0.5) {
            c = apply_lut(c);
        }
        // Capa Detalle: rayado a 45° sobre las caras de detalle.
        let detail = textureLoad(normal_tex, px, 0).w > 0.5;
        if (detail && p.hatch.w > 0.0 && ((px.x + px.y) & 3) == 0) {
            c = mix(c, p.hatch.rgb, p.hatch.w);
        }
    }
    // Capas: color premultiplicado y, alrededor de su máscara, su contorno de 1 píxel.
    let ov = textureLoad(overlay_color, px, 0);
    c = ov.rgb + c * (1.0 - ov.a);
    if (textureLoad(overlay_mask, px, 0).a < 0.5) {
        let dirs = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
        for (var i = 0; i < 4; i = i + 1) {
            let m = textureLoad(overlay_mask, clamp(px + dirs[i], vec2<i32>(0), size() - 1), 0);
            if (m.a > 0.5) {
                c = m.rgb;
                break;
            }
        }
    }
    return vec4<f32>(c, 1.0);
}
