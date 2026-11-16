// Capas y gizmos (docs/diseno-gui.md §7): en la textura de arte, sin luz ni contorno del shader. Color en sRGB con
// opacidad (se premultiplica aquí) y máscara: rgb = color de su contorno, a = 1 si lo lleva. Lo tapado va a rayas.

struct Ov {
    view_proj: mat4x4<f32>,
    // x: lo que se acerca a la cámara (NDC) para no pelear con la cara sobre la que se dibuja; y: opacidad de lo tapado.
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> o: Ov;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) mask: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) mask: vec4<f32>,
};

@vertex
fn vs_overlay(v: VIn) -> VOut {
    var out: VOut;
    var clip = o.view_proj * vec4<f32>(v.pos, 1.0);
    clip.z = clip.z - o.params.x * clip.w;
    out.clip = clip;
    out.color = v.color;
    out.mask = v.mask;
    return out;
}

struct FOut {
    @location(0) color: vec4<f32>,
    @location(1) mask: vec4<f32>,
};

@fragment
fn fs_visible(in: VOut) -> FOut {
    var out: FOut;
    out.color = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
    out.mask = in.mask;
    return out;
}

// Lo tapado: rayas a 45° de 2 píxeles de arte a la opacidad de lo tapado (sin máscara).
@fragment
fn fs_occluded(in: VOut) -> FOut {
    let q = vec2<i32>(in.clip.xy);
    if (((q.x + q.y) & 3) >= 2) {
        discard;
    }
    let a = in.color.a * o.params.y;
    var out: FOut;
    out.color = vec4<f32>(in.color.rgb * a, a);
    out.mask = vec4<f32>(0.0);
    return out;
}
