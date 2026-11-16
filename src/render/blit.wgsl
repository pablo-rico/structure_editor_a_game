// Escalado entero con Point de la textura de arte al rectángulo del visor (en la pasada de egui).

struct Blit {
    // xy: centro del visor en píxeles físicos (con el deslizamiento ya sumado); z: escala; w: 1 si el destino es sRGB.
    place: vec4<f32>,
    // rgb: fondo fuera de la textura (sRGB).
    bg: vec4<f32>,
};

@group(0) @binding(0) var art: texture_2d<f32>;
@group(0) @binding(1) var<uniform> b: Blit;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_blit(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(art));
    let t = floor((frag.xy - b.place.xy) / b.place.z + size * 0.5);
    var c = b.bg.rgb;
    if (all(t >= vec2<f32>(0.0)) && all(t < size)) {
        c = textureLoad(art, vec2<i32>(t), 0).rgb;
    }
    if (b.place.w > 0.5) {
        c = srgb_to_linear(c);
    }
    return vec4<f32>(c, 1.0);
}
