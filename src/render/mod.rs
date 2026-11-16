//! Visor wgpu con el aspecto del juego (estilo «pixel 3D» de a_game) para egui.
//!
//! - Se dibuja a una textura de baja resolución (un texel = un píxel de arte, `VIEW_PIXELS_PER_UNIT` por unidad) y se
//!   escala por entero con Point (`PixelUpscaler` sin `FullResolution`). En Iso la cámara es ortográfica
//!   `Euler(elevación, yaw, 0)` con el foco encajado al texel (`OrbitCameraRig.SnapToTexel`) y el resto se recupera
//!   desplazando la imagen píxeles enteros de pantalla. La Vista libre es en perspectiva, sin encaje.
//! - Luz de mediodía del juego (ver [`Light`]), toon de 3 bandas, sombra dura con tinte frío, contorno de un píxel de
//!   arte, LUT de la paleta, tono por sub-cubo y oclusión de contacto (shaders en `scene.wgsl` y `post.wgsl`).
//! - Corte por planta como el cutaway del juego (descarte en el shader, sin remallar); capas y gizmos en la textura de
//!   arte (`overlay.rs`); chinchetas y etiquetas con egui, nítidas.

mod gpu;
mod overlay;

pub use gpu::set_agame_root;

use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2 as EVec2};
use eframe::egui_wgpu;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::mesh::{self, Paint};
use crate::model::{LEVEL_HEIGHT, SUB, Structure};
use crate::pick::{self, Cut, Hit};
use crate::theme::{self, overlay as ov};
use gpu::{Frame, Globals, Gpu, OvUniform, Post, Shared};

/// Píxeles de arte por unidad de mundo (`WorldConstants.ViewPixelsPerUnit`): una celda = 16 px a 45°.
pub const VIEW_PIXELS_PER_UNIT: f32 = 22.627_417;
/// Escala entera máxima (píxeles físicos por píxel de arte).
pub const MAX_SCALE: u32 = 16;
/// Elevación de la cámara en Iso (`WorldConstants.ViewElevationDeg` ± `ViewElevationRangeDeg`).
pub const ISO_ELEVATION: std::ops::RangeInclusive<f32> = 15.0..=45.0;
const FREE_ELEVATION: std::ops::RangeInclusive<f32> = 5.0..=85.0;
/// Campo de visión vertical de la Vista libre.
const FREE_FOV_DEG: f32 = 30.0;
/// Margen de la textura de arte, en píxeles de arte por borde, para el deslizamiento.
const MARGIN: u32 = 1;
/// Opacidad de las plantas de arriba en fantasma (§7).
const GHOST_ALPHA: f32 = 0.12;

/// Capas del visor (§7, teclas 1–8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layers {
    pub grid: bool,
    pub nav: bool,
    pub parts: bool,
    pub interactables: bool,
    pub entrance: bool,
    pub footprint: bool,
    pub detail: bool,
    pub bands: bool,
}

impl Layers {
    /// Ninguna capa (miniaturas, muestrario, capturas limpias).
    pub fn none() -> Self {
        Self {
            grid: false,
            nav: false,
            parts: false,
            interactables: false,
            entrance: false,
            footprint: false,
            detail: false,
            bands: false,
        }
    }
}

impl Default for Layers {
    /// Las de Explorar: rejilla, partes y entrada.
    fn default() -> Self {
        Self {
            grid: true,
            nav: false,
            parts: true,
            interactables: false,
            entrance: true,
            footprint: false,
            detail: false,
            bands: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Projection {
    /// Ortográfica encajada al píxel, como el juego.
    #[default]
    Iso,
    /// Perspectiva orbital, sin encaje.
    Free,
}

/// Gizmos del modo Editar que la app rellena cada fotograma (§7, «Gizmos»). Coordenadas en sub-cubos.
#[derive(Clone, Debug, PartialEq)]
pub enum Gizmo {
    /// Pincel o caja/línea en vista previa: cubos fantasma del material al 55 % con contorno crema (borrar: contorno
    /// rojo y rayado también por lo tapado). Con más de 512 cubos se dibuja su caja envolvente.
    Ghost {
        cells: Vec<[i32; 3]>,
        material: u8,
        erase: bool,
    },
    /// Selección: caja `[min, max]` incluida, hormigas doradas/oscuras a 4 fps y tinte dorado del 20 %.
    Selection { min: [i32; 3], max: [i32; 3] },
    /// Plano bloqueado de edición a la altura `y`: lámina crema al 8 % con rejilla de sub-cubo.
    Plane { y: i32 },
}

/// Estado de la cámara y de lo que se ve; lo guarda la app (uno por visor o compartido con «Cámara enlazada»).
#[derive(Clone, Debug, PartialEq)]
pub struct ViewState {
    pub yaw_deg: f32,
    pub elevation_deg: f32,
    /// Zoom entero: píxeles físicos por píxel de arte.
    pub scale: u32,
    /// Foco en unidades de mundo (las de la malla).
    pub focus: Vec3,
    pub projection: Projection,
    /// Corte por planta: nivel del suelo de la planta (los pies); `None` = todo.
    pub cut_level: Option<i32>,
    pub layers: Layers,
    /// Lo que quita el corte, en fantasma desaturado al 12 %.
    pub ghost_upper: bool,
    /// «Localizar»: caja `[x0, y0, z0, x1, y1, z1)` en sub-cubos que parpadea en rojo 3 veces y se borra sola.
    pub highlight: Option<[i32; 6]>,
    /// Encuadrar en el próximo dibujo (lo pone [`ViewState::frame`]; se elige el zoom que cabe).
    pub fit: bool,
    pub gizmos: Vec<Gizmo>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            yaw_deg: 45.0,
            elevation_deg: 30.0,
            scale: 3,
            focus: Vec3::ZERO,
            projection: Projection::Iso,
            cut_level: None,
            layers: Layers::default(),
            ghost_upper: false,
            highlight: None,
            fit: true,
            gizmos: Vec::new(),
        }
    }
}

impl ViewState {
    /// Encuadrar (F): foco en el centro de la rejilla y zoom que cabe en el visor.
    pub fn frame(&mut self, s: &Structure) {
        let (lo, hi) = self.visible_box(s);
        self.focus = (lo + hi) * 0.5;
        self.fit = true;
    }

    /// Corte del visor con este estado (el de los shaders y el picking).
    pub fn cut(&self, s: &Structure) -> Option<Cut> {
        let (_, _, fwd) = basis(self.yaw_deg, self.elevation_deg);
        let ground = Vec2::new(fwd.x, fwd.z).try_normalize().unwrap_or(Vec2::Y);
        self.cut_level.map(|feet| Cut {
            feet,
            forward: ground.into(),
            size: [s.grid.size[0], s.grid.size[2]],
        })
    }

    /// Caja de lo sólido que queda con el corte (lo de encima no cuenta para encuadrar).
    fn visible_box(&self, s: &Structure) -> (Vec3, Vec3) {
        let (lo, mut hi) = solid_box(s);
        if let Some(feet) = self.cut_level {
            hi.y =
                hi.y.min((feet as f32 + pick::CUT_BACK / SUB as f32) * LEVEL_HEIGHT)
                    .max(lo.y);
        }
        (lo, hi)
    }

    pub fn rotate(&mut self, degrees: f32) {
        self.yaw_deg = (self.yaw_deg + degrees).rem_euclid(360.0);
    }

    /// Iso ⇄ vista libre (P); al volver a Iso, imán de yaw y elevación en rango.
    pub fn toggle_projection(&mut self) {
        self.projection = if self.projection == Projection::Iso { Projection::Free } else { Projection::Iso };
        if self.projection == Projection::Iso {
            self.snap_yaw();
        }
        self.clamp_elevation();
    }

    /// Imán de Iso: 45° + múltiplos de 90°.
    fn snap_yaw(&mut self) {
        self.yaw_deg = (45.0 + ((self.yaw_deg - 45.0) / 90.0).round() * 90.0).rem_euclid(360.0);
    }

    fn clamp_elevation(&mut self) {
        let r = if self.projection == Projection::Iso {
            ISO_ELEVATION
        } else {
            FREE_ELEVATION
        };
        self.elevation_deg = self.elevation_deg.clamp(*r.start(), *r.end());
    }

    pub fn zoom(&mut self, steps: i32) {
        self.scale = (self.scale as i32 + steps).clamp(1, MAX_SCALE as i32) as u32;
    }
}

/// Esquina máxima de la rejilla en unidades de mundo (la mínima es el origen).
fn extent(s: &Structure) -> Vec3 {
    let [sx, sy, sz] = s.grid.size;
    mesh::sub_to_local(sx as f32, sy as f32, sz as f32)
}

/// Caja de lo sólido en unidades de mundo (la rejilla entera si está vacía).
fn solid_box(s: &Structure) -> (Vec3, Vec3) {
    let g = &s.grid;
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for (i, &m) in g.data.iter().enumerate() {
        if m != 0 {
            let i = i as i32;
            let (x, z, y) = (
                i % g.size[0],
                i / g.size[0] % g.size[2],
                i / (g.size[0] * g.size[2]),
            );
            lo = [lo[0].min(x), lo[1].min(y), lo[2].min(z)];
            hi = [hi[0].max(x + 1), hi[1].max(y + 1), hi[2].max(z + 1)];
        }
    }
    if lo[0] > hi[0] {
        return (Vec3::ZERO, extent(s));
    }
    let w = |p: [i32; 3]| mesh::sub_to_local(p[0] as f32, p[1] as f32, p[2] as f32);
    (w(lo), w(hi))
}

fn box_corners(lo: Vec3, hi: Vec3) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    })
}

fn corners(s: &Structure) -> [Vec3; 8] {
    box_corners(Vec3::ZERO, extent(s))
}

/// Ejes de la cámara `Euler(elevación, yaw, 0)` de Unity: derecha, arriba y hacia delante.
fn basis(yaw_deg: f32, elevation_deg: f32) -> (Vec3, Vec3, Vec3) {
    let (sy, cy) = yaw_deg.to_radians().sin_cos();
    let (se, ce) = elevation_deg.to_radians().sin_cos();
    (
        Vec3::new(cy, 0.0, -sy),
        Vec3::new(se * sy, ce, se * cy),
        Vec3::new(sy * ce, -se, cy * ce),
    )
}

/// Mundo ↔ sub-cubos.
pub fn world_to_sub(p: Vec3) -> Vec3 {
    Vec3::new(
        p.x * SUB as f32,
        p.y / LEVEL_HEIGHT * SUB as f32,
        p.z * SUB as f32,
    )
}

fn rows(r: [Vec4; 4]) -> Mat4 {
    Mat4::from_cols(r[0], r[1], r[2], r[3]).transpose()
}

/// Cámara de una imagen de `art` píxeles de arte.
#[derive(Clone, Copy, Debug)]
struct Camera {
    view_proj: Mat4,
    right: Vec3,
    up: Vec3,
    fwd: Vec3,
    /// Foco encajado (Iso) o foco (libre).
    center: Vec3,
    eye: Vec3,
    perspective: bool,
    near: f32,
    far: f32,
    tan_half: f32,
    art: [u32; 2],
    /// Lo que el encaje deja, en píxeles de arte sobre derecha y arriba.
    remainder: Vec2,
}

impl Camera {
    fn new(view: &ViewState, art: [u32; 2], corners: &[Vec3; 8]) -> Self {
        let (right, up, fwd) = basis(view.yaw_deg, view.elevation_deg);
        let (w, h) = (art[0] as f32, art[1] as f32);
        let ppu = VIEW_PIXELS_PER_UNIT;
        let tan_half = (FREE_FOV_DEG.to_radians() * 0.5).tan();
        match view.projection {
            Projection::Iso => {
                let f = view.focus;
                let snap = |axis: Vec3| (f.dot(axis) * ppu).round() / ppu;
                let center = right * snap(right) + up * snap(up) + fwd * f.dot(fwd);
                let remainder = Vec2::new((f - center).dot(right), (f - center).dot(up)) * ppu;
                let depths = corners.map(|c| (c - center).dot(fwd));
                let near = depths.iter().copied().fold(f32::MAX, f32::min) - 2.0;
                let far = depths.iter().copied().fold(f32::MIN, f32::max) + 2.0;
                let (hw, hh) = (w * 0.5 / ppu, h * 0.5 / ppu);
                let view_proj = rows([
                    (right / hw).extend(-right.dot(center) / hw),
                    (up / hh).extend(-up.dot(center) / hh),
                    (fwd / (far - near)).extend((-fwd.dot(center) - near) / (far - near)),
                    Vec4::W,
                ]);
                let eye = center + fwd * near;
                Self {
                    view_proj,
                    right,
                    up,
                    fwd,
                    center,
                    eye,
                    perspective: false,
                    near,
                    far,
                    tan_half,
                    art,
                    remainder,
                }
            }
            Projection::Free => {
                // La misma escala que Iso en el foco.
                let dist = h * 0.5 / ppu / tan_half;
                let eye = view.focus - fwd * dist;
                let depths = corners.map(|c| (c - eye).dot(fwd));
                let near = (depths.iter().copied().fold(f32::MAX, f32::min) - 2.0).max(0.05);
                let far = (depths.iter().copied().fold(f32::MIN, f32::max) + 2.0).max(near + 1.0);
                let (sx, sy) = (1.0 / (tan_half * w / h), 1.0 / tan_half);
                let (a, b) = (far / (far - near), -far * near / (far - near));
                let view_proj = rows([
                    (right * sx).extend(-right.dot(eye) * sx),
                    (up * sy).extend(-up.dot(eye) * sy),
                    (fwd * a).extend(-fwd.dot(eye) * a + b),
                    fwd.extend(-fwd.dot(eye)),
                ]);
                let center = view.focus;
                Self {
                    view_proj,
                    right,
                    up,
                    fwd,
                    center,
                    eye,
                    perspective: true,
                    near,
                    far,
                    tan_half,
                    art,
                    remainder: Vec2::ZERO,
                }
            }
        }
    }

    /// Píxel de arte (x a la derecha, y hacia abajo) de un punto de mundo.
    fn project(&self, p: Vec3) -> Option<Vec2> {
        let c = self.view_proj * p.extend(1.0);
        (c.w > 1e-4).then(|| {
            let n = c.truncate() / c.w;
            Vec2::new(
                (n.x + 1.0) * 0.5 * self.art[0] as f32,
                (1.0 - n.y) * 0.5 * self.art[1] as f32,
            )
        })
    }

    /// Rayo de mundo por un punto de la textura de arte.
    fn ray(&self, px: Vec2) -> (Vec3, Vec3) {
        let (w, h) = (self.art[0] as f32, self.art[1] as f32);
        let n = Vec2::new(px.x / w * 2.0 - 1.0, 1.0 - px.y / h * 2.0);
        if self.perspective {
            let d = self.fwd
                + self.right * (n.x * self.tan_half * w / h)
                + self.up * (n.y * self.tan_half);
            (self.eye, d.normalize())
        } else {
            let half = Vec2::new(w, h) * 0.5 / VIEW_PIXELS_PER_UNIT;
            (
                self.center
                    + self.right * (n.x * half.x)
                    + self.up * (n.y * half.y)
                    + self.fwd * self.near,
                self.fwd,
            )
        }
    }
}

/// Luz del juego a mediodía (12:00, `DayNightProfile.asset` y `SunLight3D`), en lineal como la recibe el shader.
struct Light {
    /// Hacia el sol: azimut 51° y elevación 65° tras `SunLight3D.Quantize` (`SunMath` a t = 0,5), fijo en el mundo.
    sun: Vec3,
    sun_color: Vec3,
    ambient: Vec3,
    shadow_tint: Vec3,
    shadow_strength: f32,
    outline: Vec3,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl Light {
    fn noon() -> Self {
        let (az, el) = (51f32.to_radians(), 65f32.to_radians());
        // (gx, gy, arriba) → mundo (gx, arriba, gy) con el yaw isométrico (SunLight3D.DirectionToLight).
        let sun = Vec3::new(az.cos() * el.cos(), el.sin(), az.sin() * el.cos());
        // Color del sol: gradiente del perfil en Progress(0,5) = 0,463 (entre las claves 0,15 y 0,5); URP lo pasa a lineal.
        let t = (0.25 / 0.54 - 0.15) / 0.35;
        let sun_srgb = Vec3::new(0.32, 0.26, 0.18).lerp(Vec3::new(0.28, 0.27, 0.25), t);
        // Ambiente plano: AddAmbientLight(0,78) en los armónicos, que se usan tal cual (ya lineales).
        let ambient = Vec3::splat(0.78);
        // PixelStyle.ShadowTint(DayNightProfile.shadowColor) por SetGlobalColor (sRGB → lineal).
        let shadow = Vec3::new(0.10, 0.08, 0.22);
        let tint = Vec3::splat(0.3).lerp(shadow / shadow.max_element(), 0.35);
        let lin = |c: Vec3| {
            Vec3::new(
                srgb_to_linear(c.x),
                srgb_to_linear(c.y),
                srgb_to_linear(c.z),
            )
        };
        Self {
            sun,
            sun_color: lin(sun_srgb),
            ambient,
            shadow_tint: lin(tint),
            // SunMath.ShadowAlpha a mediodía = DayNightProfile.shadowAlpha.
            shadow_strength: 0.35,
            // PixelStyle.OutlineColor por SetGlobalColor.
            outline: lin(Vec3::new(0.32, 0.22, 0.36)),
        }
    }
}

/// Matriz ortográfica del sol que abarca la rejilla y el tamaño de su texel en mundo.
fn light_proj(sun: Vec3, corners: &[Vec3; 8]) -> (Mat4, f32) {
    let lf = -sun;
    let r = Vec3::Y.cross(lf).normalize();
    let u = lf.cross(r);
    let proj = |axis: Vec3| {
        let d = corners.map(|c| c.dot(axis));
        (
            d.iter().copied().fold(f32::MAX, f32::min),
            d.iter().copied().fold(f32::MIN, f32::max),
        )
    };
    let ((x0, x1), (y0, y1), (z0, z1)) = (proj(r), proj(u), proj(lf));
    let half = ((x1 - x0).max(y1 - y0) * 0.5 + 0.5).max(0.5);
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let (near, far) = (z0 - 2.0, z1 + 2.0);
    let m = rows([
        (r / half).extend(-cx / half),
        (u / half).extend(-cy / half),
        (lf / (far - near)).extend(-near / (far - near)),
        Vec4::W,
    ]);
    (m, 2.0 * half / gpu::SHADOW_SIZE as f32)
}

fn color4(c: Color32, a: f32) -> [f32; 4] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        a,
    ]
}

/// Uniformes de una imagen.
fn frame_uniforms(
    s: &Structure,
    view: &ViewState,
    cam: &Camera,
    lut_on: bool,
) -> (Globals, Post, OvUniform) {
    let light = Light::noon();
    let corners = corners(s);
    let (light_proj, texel) = light_proj(light.sun, &corners);
    let cut = view.cut(s);
    let half = extent(s) * 0.5;
    let globals = Globals {
        view_proj: cam.view_proj.to_cols_array_2d(),
        light_proj: light_proj.to_cols_array_2d(),
        eye: cam
            .eye
            .extend(if cam.perspective { 1.0 } else { 0.0 })
            .into(),
        forward: cam.fwd.extend(texel).into(),
        sun: light.sun.extend(light.shadow_strength).into(),
        sun_color: light.sun_color.extend(1.0).into(),
        ambient: light.ambient.extend(1.0).into(),
        shadow_tint: light.shadow_tint.extend(1.0).into(),
        cut: match &cut {
            Some(c) => {
                let feet = c.feet as f32 * LEVEL_HEIGHT;
                [
                    feet,
                    1.0,
                    feet + pick::CUT_BACK * LEVEL_HEIGHT / SUB as f32,
                    GHOST_ALPHA,
                ]
            }
            None => [0.0, 0.0, 0.0, GHOST_ALPHA],
        },
        cut_view: cut.map_or([0.0, 1.0, half.x, half.z], |c| {
            [c.forward[0], c.forward[1], half.x, half.z]
        }),
    };
    let post = Post {
        bg: color4(theme::VIEW_BG, 1.0),
        // PixelStyle: salto de 0,45 u, oscurecido 0,6 hacia el tinte y aristas convexas ×1,3.
        outline: light.outline.extend(0.45).into(),
        params: [
            0.6,
            0.3,
            if lut_on { 1.0 } else { 0.0 },
            if cam.perspective { 1.0 } else { 0.0 },
        ],
        depth: [cam.near, cam.far, 0.0, 0.0],
        hatch: color4(
            ov::DETAIL_HATCH,
            if view.layers.detail { 0.35 } else { 0.0 },
        ),
    };
    // Las capas se acercan 0,03 u a la cámara para no pelear con la cara sobre la que van.
    let bias = 0.03 / (cam.far - cam.near);
    let overlay = OvUniform {
        view_proj: cam.view_proj.to_cols_array_2d(),
        params: [bias, ov::OCCLUDED_ALPHA, 0.0, 0.0],
    };
    (globals, post, overlay)
}

/// Escala (puede ser < 1) a la que la rejilla cabe en `px` píxeles con un 10 % de aire.
fn fit_scale(view: &ViewState, s: &Structure, px: [f32; 2]) -> f32 {
    let (right, up, _) = basis(view.yaw_deg, view.elevation_deg);
    let (lo, hi) = view.visible_box(s);
    let c = box_corners(lo, hi);
    let extent = |axis: Vec3| {
        let d = c.map(|p| p.dot(axis));
        (d.iter().copied().fold(f32::MIN, f32::max) - d.iter().copied().fold(f32::MAX, f32::min))
            * VIEW_PIXELS_PER_UNIT
    };
    (px[0] * 0.9 / extent(right).max(1.0)).min(px[1] * 0.9 / extent(up).max(1.0))
}

/// Tamaño par (el centro cae en una esquina de texel) de la textura de arte para `px` píxeles a escala `scale`.
fn art_size(px: [f32; 2], scale: u32) -> [u32; 2] {
    px.map(|v| {
        let n = (v / scale as f32).ceil().max(2.0) as u32 + 2 * MARGIN;
        n + n % 2
    })
}

/// Respuesta del visor: la de egui y lo que hay bajo el puntero.
pub struct ViewportResponse {
    pub response: egui::Response,
    pub hovered: Option<Hit>,
    pub clicked: Option<Hit>,
    /// Rayo bajo el puntero en coordenadas de sub-cubo (origen, dirección), para pintar en el aire o en un plano.
    pub ray: Option<(Vec3, Vec3)>,
}

/// Lo que decide si hay que volver a dibujar.
#[derive(Clone, PartialEq)]
struct Key {
    view: ViewState,
    art: [u32; 2],
    version: u64,
    hover: Option<Hit>,
    highlight: Option<[i32; 6]>,
    ants: i64,
}

/// Un visor (principal, comparar A/B, muestrario): malla, texturas y entrada de cámara.
pub struct Viewport {
    rs: egui_wgpu::RenderState,
    shared: Arc<Shared>,
    gpu: Gpu,
    structure: Option<Arc<Structure>>,
    paint: Paint,
    version: u64,
    last: Option<Key>,
    /// Caja de «Localizar» y cuándo empezó a parpadear.
    blink: Option<([i32; 6], f64)>,
    zoom_acc: f32,
}

/// Parpadeo de «Localizar»: 3 veces en 600 ms cada una.
const BLINK_PERIOD: f64 = 0.6;
const BLINKS: f64 = 3.0;

impl Viewport {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let shared = Shared::get(rs);
        let gpu = Gpu::new(&rs.device, &shared);
        Self {
            rs: rs.clone(),
            shared,
            gpu,
            structure: None,
            paint: Paint::default(),
            version: 0,
            last: None,
            blink: None,
            zoom_acc: 1.0,
        }
    }

    /// Malla y sube la estructura (con su paleta: [`Paint::new`]).
    pub fn set_structure(&mut self, s: Arc<Structure>, paint: &Paint) {
        let t = std::time::Instant::now();
        let mesh = mesh::build(&s, paint);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        if ms > 50.0 {
            eprintln!(
                "[visor] mallado de {:?}: {ms:.1} ms, {} vértices",
                s.grid.size,
                mesh.vertices.len()
            );
        }
        self.gpu.upload_mesh(&self.rs.device, &mesh);
        self.structure = Some(s);
        self.paint = paint.clone();
        self.version += 1;
    }

    pub fn structure(&self) -> Option<&Arc<Structure>> {
        self.structure.as_ref()
    }

    /// Dibuja el visor en todo el espacio disponible y gestiona la cámara (§8) con el puntero encima.
    pub fn ui(&mut self, ui: &mut egui::Ui, view: &mut ViewState) -> ViewportResponse {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let px = [rect.width() * ppp, rect.height() * ppp];
        let Some(s) = self.structure.clone() else {
            ui.painter().rect_filled(rect, 0.0, theme::VIEW_BG);
            return ViewportResponse {
                response,
                hovered: None,
                clicked: None,
                ray: None,
            };
        };
        if view.fit && px[0] > 1.0 && px[1] > 1.0 {
            view.scale = (fit_scale(view, &s, px).floor() as u32).clamp(1, MAX_SCALE);
            view.fit = false;
        }
        self.input(ui, &response, view, &s, ppp);

        let art = art_size(px, view.scale);
        let cam = Camera::new(view, art, &corners(&s));
        let scale = view.scale as f32;
        // Centro del visor en píxeles físicos, con el resto del encaje en píxeles enteros de pantalla.
        let shift = Vec2::new(
            -(cam.remainder.x * scale).round(),
            (cam.remainder.y * scale).round(),
        );
        let center = (Vec2::new(rect.center().x, rect.center().y) * ppp).round() + shift;
        let half_art = Vec2::new(art[0] as f32, art[1] as f32) * 0.5;
        let to_art = |p: Pos2| (Vec2::new(p.x, p.y) * ppp - center) / scale + half_art;
        let to_screen = |a: Vec2| {
            let p = ((a - half_art) * scale + center) / ppp;
            Pos2::new(p.x, p.y)
        };

        let cut = view.cut(&s);
        let ray = response.hover_pos().map(|p| {
            let (o, d) = cam.ray(to_art(p));
            (world_to_sub(o), world_to_sub(d))
        });
        let hovered =
            ray.and_then(|(o, d)| pick::raycast(&s.grid, o, d, cut.as_ref(), self.paint.hidden));
        let clicked = if response.clicked() { hovered } else { None };

        // «Localizar»: tres parpadeos y se borra.
        let now = ui.input(|i| i.time);
        if let Some(h) = view.highlight
            && self.blink.is_none_or(|(b, _)| b != h)
        {
            self.blink = Some((h, now));
        }
        let mut highlight = None;
        if let Some((h, start)) = self.blink {
            let t = now - start;
            if t >= BLINK_PERIOD * BLINKS || view.highlight.is_none() {
                self.blink = None;
                view.highlight = None;
            } else {
                if (t % BLINK_PERIOD) < BLINK_PERIOD * 0.5 {
                    highlight = Some(h);
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        let ants = view
            .gizmos
            .iter()
            .any(|g| matches!(g, Gizmo::Selection { .. }));
        if ants {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        let key = Key {
            view: view.clone(),
            art,
            version: self.version,
            hover: hovered,
            highlight,
            ants: if ants { (now * 4.0) as i64 } else { 0 },
        };
        if self.last.as_ref() != Some(&key) {
            let geom = overlay::build(&overlay::Inputs {
                structure: &s,
                paint: &self.paint,
                view,
                cut: cut.as_ref(),
                hover: hovered,
                highlight,
                time: now,
            });
            let (globals, post, overlay_uniform) =
                frame_uniforms(&s, view, &cam, self.shared.lut_on);
            let ghost = view.ghost_upper && cut.is_some();
            let frame = Frame {
                globals,
                post,
                overlay_uniform,
                overlay: &geom,
                ghost,
            };
            self.gpu
                .render(&self.rs.device, &self.rs.queue, &self.shared, art, &frame);
            self.last = Some(key);
        }
        if let Some(cb) = self.gpu.blit_callback(
            &self.rs.queue,
            &self.shared,
            center.into(),
            view.scale,
            color4(theme::VIEW_BG, 1.0),
        ) {
            ui.painter()
                .add(egui_wgpu::Callback::new_paint_callback(rect, cb));
        }

        let painter = ui.painter_at(rect);
        let world_screen = |p: Vec3| cam.project(p).map(to_screen);
        if view.layers.entrance {
            let [ex, ez] = s.entrance;
            let p = mesh::sub_to_local(
                (ex * SUB) as f32 + 4.0,
                (s.ground_level * SUB) as f32,
                (ez * SUB) as f32 + 4.0,
            );
            if let Some(at) = world_screen(p) {
                label(
                    &painter,
                    at + EVec2::new(0.0, 14.0),
                    "Entrada",
                    ov::ENTRANCE,
                );
            }
        }
        if view.layers.interactables {
            pins(&painter, &s, cut.as_ref(), world_screen);
        }
        ViewportResponse {
            response,
            hovered,
            clicked,
            ray,
        }
    }

    /// Cámara con ratón, trackpad y teclas (§8) mientras el puntero está sobre el visor.
    fn input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        view: &mut ViewState,
        s: &Structure,
        ppp: f32,
    ) {
        let hovered = response.contains_pointer() || response.dragged();
        if !hovered {
            return;
        }
        let (right, up, _) = basis(view.yaw_deg, view.elevation_deg);
        // Desplazar: el contenido sigue al puntero.
        let pan = |view: &mut ViewState, delta: EVec2| {
            let k = ppp / view.scale as f32 / VIEW_PIXELS_PER_UNIT;
            view.focus += (-right * delta.x + up * delta.y) * k;
        };
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        if response.dragged_by(egui::PointerButton::Middle)
            || (space && response.dragged_by(egui::PointerButton::Primary))
        {
            pan(view, response.drag_delta());
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let d = response.drag_delta();
            view.rotate(d.x * 0.4);
            if view.projection == Projection::Free {
                view.elevation_deg += d.y * 0.3;
                view.clamp_elevation();
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Secondary)
            && view.projection == Projection::Iso
        {
            view.snap_yaw();
        }

        let (events, zoom) = ui.input(|i| (i.events.clone(), i.zoom_delta()));
        let keys_free = !ui.ctx().egui_wants_keyboard_input();
        for e in &events {
            if let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = e
            {
                if modifiers.command || modifiers.ctrl {
                    continue; // ya va en zoom_delta
                }
                let notch = delta.y + delta.x;
                if modifiers.shift {
                    view.elevation_deg += notch.signum() * 2.5;
                    view.clamp_elevation();
                } else if *unit == egui::MouseWheelUnit::Line || *unit == egui::MouseWheelUnit::Page
                {
                    view.zoom(notch.signum() as i32);
                } else if modifiers.alt {
                    view.rotate(delta.x * 0.3);
                    view.elevation_deg += delta.y * 0.2;
                    view.clamp_elevation();
                } else {
                    pan(view, *delta);
                }
            }
        }
        // Pellizcar: un paso de zoom entero cada 25 %.
        if zoom != 1.0 {
            self.zoom_acc *= zoom;
            if self.zoom_acc > 1.25 || self.zoom_acc < 0.8 {
                view.zoom(if self.zoom_acc > 1.0 { 1 } else { -1 });
                self.zoom_acc = 1.0;
            }
        }
        if keys_free {
            let pressed = |k| ui.input(|i| i.key_pressed(k));
            // Con Cmd son otros atajos (Cmd+R gira la selección, Cmd+Q sale, Cmd±: zoom de la interfaz).
            let command = ui.input(|i| i.modifiers.command);
            let plain = |k| !command && pressed(k);
            if plain(egui::Key::Q) {
                view.rotate(-90.0);
            }
            if plain(egui::Key::R) {
                view.rotate(90.0);
            }
            if plain(egui::Key::F) {
                view.frame(s);
            }
            if plain(egui::Key::P) {
                view.toggle_projection();
            }
            if plain(egui::Key::Plus) || plain(egui::Key::Equals) {
                view.zoom(1);
            }
            if plain(egui::Key::Minus) {
                view.zoom(-1);
            }
            if command && pressed(egui::Key::Num0) {
                view.fit = true;
            }
        }
    }
}

/// Etiqueta en pantalla con sombra (§7: nítida, con egui).
fn label(painter: &egui::Painter, at: Pos2, text: &str, color: Color32) {
    let font = egui::FontId::proportional(12.0);
    painter.text(
        at + EVec2::new(1.0, 1.0),
        egui::Align2::CENTER_TOP,
        text,
        font.clone(),
        theme::TEXT_SHADOW,
    );
    painter.text(at, egui::Align2::CENTER_TOP, text, font, color);
}

/// Glifo de la chincheta de un interactuable (`InteractKind`).
fn pin_glyph(kind: &str) -> &'static str {
    use egui_phosphor::regular as i;
    match kind {
        "Door" => i::DOOR,
        "Bed" => i::BED,
        "Chair" => i::CHAIR,
        "Chest" => i::PACKAGE,
        "Counter" => i::STOREFRONT,
        "HarbourDesk" => i::DESK,
        "Helm" => i::STEERING_WHEEL,
        "Cannon" => i::CROSSHAIR,
        "CargoHatch" => i::TRAY,
        "Anchor" | "Mooring" => i::ANCHOR,
        "Gangway" => i::LADDER,
        _ => i::HAND_POINTING,
    }
}

/// Chinchetas de los interactuables: placa de 20 pt con glifo, palito a un punto dorado; las solapadas, «+n».
fn pins(
    painter: &egui::Painter,
    s: &Structure,
    cut: Option<&Cut>,
    project: impl Fn(Vec3) -> Option<Pos2>,
) {
    const PLATE: f32 = 20.0;
    const STICK: f32 = 12.0;
    let mut items: Vec<(Pos2, &str)> = s
        .interactables
        .iter()
        .filter(|it| cut.is_none_or(|c| it.bbox[1] < c.feet * SUB + pick::CUT_BACK as i32))
        .filter_map(|it| {
            let [x0, _, z0, x1, y1, z1] = it.bbox.map(|v| v as f32);
            project(mesh::sub_to_local((x0 + x1) * 0.5, y1, (z0 + z1) * 0.5))
                .map(|p| (p, it.kind.as_str()))
        })
        .collect();
    items.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    let mut groups: Vec<(Pos2, &str, usize)> = Vec::new();
    for (p, kind) in items {
        match groups.iter_mut().find(|g| g.0.distance(p) < PLATE) {
            Some(g) => g.2 += 1,
            None => groups.push((p, kind, 1)),
        }
    }
    for (p, kind, n) in groups {
        let plate = Rect::from_center_size(
            p - EVec2::new(0.0, STICK + PLATE * 0.5),
            EVec2::splat(PLATE),
        );
        painter.line_segment(
            [p, plate.center_bottom()],
            egui::Stroke::new(1.0, ov::PIN_BORDER),
        );
        painter.circle_filled(p, 2.5, ov::PIN_DOT);
        painter.rect(
            plate,
            3.0,
            ov::PIN_PLATE,
            egui::Stroke::new(1.0, ov::PIN_BORDER),
            egui::StrokeKind::Inside,
        );
        painter.text(
            plate.center(),
            egui::Align2::CENTER_CENTER,
            pin_glyph(kind),
            egui::FontId::proportional(14.0),
            ov::PIN_GLYPH,
        );
        if n > 1 {
            // Con sombra (`label`): sobre revoque claro también se lee.
            label(painter, plate.right_top() + EVec2::new(8.0, -14.0), &format!("+{}", n - 1), ov::PIN_PLATE);
        }
    }
}

/// Imagen fuera de pantalla (miniaturas, muestrario, capturas) de `size_px` píxeles, sin etiquetas ni chinchetas de
/// egui (y `highlight` fijo, sin parpadeo). Con `view.fit`, encuadra; si la estructura no cabe ni a escala 1, se dibuja más grande y se reduce.
pub fn render_image(
    rs: &egui_wgpu::RenderState,
    s: &Structure,
    paint: &Paint,
    view: &ViewState,
    size_px: [u32; 2],
) -> egui::ColorImage {
    let shared = Shared::get(rs);
    let mut gpu = Gpu::new(&rs.device, &shared);
    gpu.upload_mesh(&rs.device, &mesh::build(s, paint));
    let mut view = view.clone();
    let px = size_px.map(|v| v.max(1) as f32);
    let mut shrink = 1.0;
    if view.fit {
        view.frame(s);
        let f = fit_scale(&view, s, px);
        view.scale = (f.floor() as u32).clamp(1, MAX_SCALE);
        if f < 1.0 {
            shrink = f.max(0.05);
        }
        view.fit = false;
    }
    let canvas = px.map(|v| v / shrink);
    let art = canvas.map(|v| {
        let n = (v / view.scale as f32).ceil() as u32;
        n + n % 2
    });
    let cam = Camera::new(&view, art, &corners(s));
    let cut = view.cut(s);
    let geom = overlay::build(&overlay::Inputs {
        structure: s,
        paint,
        view: &view,
        cut: cut.as_ref(),
        hover: None,
        highlight: view.highlight,
        time: 0.0,
    });
    let (globals, post, overlay_uniform) = frame_uniforms(s, &view, &cam, shared.lut_on);
    let ghost = view.ghost_upper && cut.is_some();
    gpu.render(
        &rs.device,
        &rs.queue,
        &shared,
        art,
        &Frame {
            globals,
            post,
            overlay_uniform,
            overlay: &geom,
            ghost,
        },
    );
    let Some(([w, h], rgba)) = gpu.read_art(&rs.device, &rs.queue) else {
        return egui::ColorImage::filled(
            [size_px[0] as usize, size_px[1] as usize],
            theme::VIEW_BG,
        );
    };
    let img = image::RgbaImage::from_raw(w, h, rgba).expect("tamaño de la lectura");
    let img = if shrink < 1.0 {
        image::imageops::resize(
            &img,
            size_px[0],
            size_px[1],
            image::imageops::FilterType::Triangle,
        )
    } else {
        // Escalado entero con Point, centrado y recortado al tamaño pedido.
        let k = view.scale;
        let (ox, oy) = (
            (w * k) as i64 - size_px[0] as i64,
            (h * k) as i64 - size_px[1] as i64,
        );
        image::RgbaImage::from_fn(size_px[0], size_px[1], |x, y| {
            let sx = ((x as i64 + ox / 2) / k as i64).clamp(0, w as i64 - 1) as u32;
            let sy = ((y as i64 + oy / 2) / k as i64).clamp(0, h as i64 - 1) as u32;
            *img.get_pixel(sx, sy)
        })
    };
    egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    )
}

/// `RenderState` sin ventana (tests, capturas por lotes).
pub fn headless_render_state() -> anyhow::Result<egui_wgpu::RenderState> {
    let config = egui_wgpu::WgpuConfiguration::default();
    block_on(async {
        let instance = config.wgpu_setup.new_instance().await;
        Ok(egui_wgpu::RenderState::create(
            &config,
            &instance,
            None,
            egui_wgpu::RendererOptions::default(),
        )
        .await?)
    })
}

/// Ejecutor mínimo: las promesas de wgpu en nativo se resuelven sin despertar a nadie.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camara_iso_a_45_grados_da_el_rombo_2_1() {
        let view = ViewState::default();
        let (right, up, fwd) = basis(view.yaw_deg, view.elevation_deg);
        assert!(
            right.dot(up).abs() < 1e-6 && right.dot(fwd).abs() < 1e-6 && up.dot(fwd).abs() < 1e-6
        );
        // Una celda: 16 px de arte en horizontal por lado y un nivel, 16 px de alto.
        let px = |p: Vec3| Vec2::new(p.dot(right), p.dot(up)) * VIEW_PIXELS_PER_UNIT;
        assert!((px(Vec3::X).x - 16.0).abs() < 1e-3 && (px(Vec3::X).y - 8.0).abs() < 1e-3);
        assert!((px(Vec3::Y * LEVEL_HEIGHT).y - 16.0).abs() < 1e-3);
        // Mira hacia +X +Z y hacia abajo.
        assert!(fwd.x > 0.0 && fwd.z > 0.0 && fwd.y < 0.0);
    }

    #[test]
    fn rayo_y_proyeccion_casan() {
        let s = Structure {
            grid: crate::model::VoxelGrid::new([16, 16, 16]),
            ..Default::default()
        };
        let mut view = ViewState::default();
        view.frame(&s);
        for projection in [Projection::Iso, Projection::Free] {
            view.projection = projection;
            let cam = Camera::new(&view, [200, 120], &corners(&s));
            let p = Vec3::new(1.2, 0.7, 0.4);
            let a = cam.project(p).unwrap();
            let (o, d) = cam.ray(a);
            let closest = o + d * (p - o).dot(d);
            assert!(
                closest.distance(p) < 1e-3,
                "{projection:?}: {closest} vs {p}"
            );
        }
    }

    #[test]
    fn imanes_y_limites() {
        let mut v = ViewState {
            yaw_deg: 100.0,
            ..Default::default()
        };
        v.snap_yaw();
        assert_eq!(v.yaw_deg, 135.0);
        v.rotate(-180.0);
        v.snap_yaw();
        assert_eq!(v.yaw_deg, 315.0);
        v.elevation_deg = 80.0;
        v.clamp_elevation();
        assert_eq!(v.elevation_deg, 45.0);
        assert_eq!(art_size([301.0, 200.0], 3), [104, 70]);
    }
}
