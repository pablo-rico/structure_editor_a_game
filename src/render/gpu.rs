//! Recursos wgpu del visor: pipelines compartidos por dispositivo ([`Shared`]) y, por visor, la malla, las texturas
//! de arte y los uniformes ([`Gpu`]). Pasadas por imagen: sombra → escena → capas → contorno/LUT/composición.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

use crate::mesh::{MeshData, Vertex};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Textura de arte final y de las capas: sRGB codificado a mano en una Unorm (se mezcla en sRGB, como egui).
const ART_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(crate) const SHADOW_SIZE: u32 = 2048;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub light_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub forward: [f32; 4],
    pub sun: [f32; 4],
    pub sun_color: [f32; 4],
    pub ambient: [f32; 4],
    pub shadow_tint: [f32; 4],
    pub cut: [f32; 4],
    pub cut_view: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Post {
    pub bg: [f32; 4],
    pub outline: [f32; 4],
    pub params: [f32; 4],
    pub depth: [f32; 4],
    pub hatch: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct OvUniform {
    pub view_proj: [[f32; 4]; 4],
    pub params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct BlitUniform {
    place: [f32; 4],
    bg: [f32; 4],
}

/// Vértice de las capas: posición de mundo, color sRGB con opacidad y máscara (rgb = contorno, a = lo lleva).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct OvVertex {
    pub pos: [f32; 3],
    pub color: [u8; 4],
    pub mask: [u8; 4],
}

/// Primitivas de las capas: líneas de 1 píxel de arte y triángulos.
#[derive(Clone, Debug, Default)]
pub(crate) struct Prims {
    pub lines: Vec<OvVertex>,
    pub tris: Vec<OvVertex>,
}

/// Capas de una imagen: `plain` solo donde se ven; `xray` también lo tapado, a rayas.
#[derive(Clone, Debug, Default)]
pub(crate) struct Overlay {
    pub plain: Prims,
    pub xray: Prims,
}

/// Todo lo que cambia de una imagen a otra.
pub(crate) struct Frame<'a> {
    pub globals: Globals,
    pub post: Post,
    pub overlay_uniform: OvUniform,
    pub overlay: &'a Overlay,
    pub ghost: bool,
}

/// Pipelines y recursos fijos, uno por dispositivo (en los `callback_resources` de egui_wgpu).
pub(crate) struct Shared {
    scene_bgl: wgpu::BindGroupLayout,
    shadow_bgl: wgpu::BindGroupLayout,
    post_bgl: wgpu::BindGroupLayout,
    uniform_bgl: wgpu::BindGroupLayout,
    blit_bgl: wgpu::BindGroupLayout,
    main: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    ghost: wgpu::RenderPipeline,
    post: wgpu::RenderPipeline,
    lines_visible: wgpu::RenderPipeline,
    tris_visible: wgpu::RenderPipeline,
    lines_occluded: wgpu::RenderPipeline,
    tris_occluded: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
    shadow_sampler: wgpu::Sampler,
    lut_sampler: wgpu::Sampler,
    lut: wgpu::TextureView,
    /// Hay LUT de a_game (si no, sin paleta).
    pub lut_on: bool,
    target_is_srgb: bool,
}

impl Shared {
    /// Los de este dispositivo (se crean la primera vez).
    pub fn get(rs: &egui_wgpu::RenderState) -> Arc<Shared> {
        let mut renderer = rs.renderer.write();
        if let Some(shared) = renderer.callback_resources.get::<Arc<Shared>>() {
            return shared.clone();
        }
        let shared = Arc::new(Shared::new(&rs.device, &rs.queue, rs.target_format));
        renderer.callback_resources.insert(shared.clone());
        shared
    }

    fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: wgpu::TextureFormat) -> Self {
        use wgpu::ShaderStages as S;
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: S::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let scene_bgl = layout(
            "visor escena",
            &[
                uniform(0, S::VERTEX_FRAGMENT),
                texture(1, wgpu::TextureSampleType::Depth),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        );
        let shadow_bgl = layout("visor sombra", &[uniform(0, S::VERTEX_FRAGMENT)]);
        let post_bgl = layout(
            "visor post",
            &[
                texture(0, float),
                texture(1, float),
                texture(2, wgpu::TextureSampleType::Depth),
                texture(3, float),
                texture(4, float),
                texture(5, wgpu::TextureSampleType::Float { filterable: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: S::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform(7, S::FRAGMENT),
            ],
        );
        let uniform_bgl = layout("visor capas", &[uniform(0, S::VERTEX_FRAGMENT)]);
        let blit_bgl = layout("visor blit", &[texture(0, float), uniform(1, S::FRAGMENT)]);

        let module = |label, src: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        };
        let scene = module("visor escena", include_str!("scene.wgsl"));
        let post_module = module("visor post", include_str!("post.wgsl"));
        let overlay = module("visor capas", include_str!("overlay.wgsl"));
        let blit_module = module("visor blit", include_str!("blit.wgsl"));
        let pipeline_layout = |bgl: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(bgl)],
                immediate_size: 0,
            })
        };
        let scene_layout = pipeline_layout(&scene_bgl);
        let shadow_layout = pipeline_layout(&shadow_bgl);
        let post_layout = pipeline_layout(&post_bgl);
        let overlay_layout = pipeline_layout(&uniform_bgl);
        let blit_layout = pipeline_layout(&blit_bgl);

        let mesh_attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Float32x2, 4 => Float32x2, 5 => Float32x2
        ];
        let mesh_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attrs,
        })];
        let ov_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4];
        let ov_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<OvVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ov_attrs,
        })];
        let premultiplied = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let art = |blend, write_mask| {
            Some(wgpu::ColorTargetState {
                format: ART_FORMAT,
                blend,
                write_mask,
            })
        };
        let depth = |write, compare| {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        struct Desc<'a> {
            label: &'a str,
            layout: &'a wgpu::PipelineLayout,
            module: &'a wgpu::ShaderModule,
            vs: &'a str,
            fs: Option<&'a str>,
            buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
            targets: &'a [Option<wgpu::ColorTargetState>],
            topology: wgpu::PrimitiveTopology,
            depth: Option<wgpu::DepthStencilState>,
        }
        let pipeline = |d: Desc| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(d.label),
                layout: Some(d.layout),
                vertex: wgpu::VertexState {
                    module: d.module,
                    entry_point: Some(d.vs),
                    compilation_options: Default::default(),
                    buffers: d.buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: d.topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: d.depth,
                multisample: Default::default(),
                fragment: d.fs.map(|fs| wgpu::FragmentState {
                    module: d.module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: d.targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        use wgpu::{ColorWrites as W, CompareFunction as C, PrimitiveTopology as T};
        let main = pipeline(Desc {
            label: "visor escena",
            layout: &scene_layout,
            module: &scene,
            vs: "vs_main",
            fs: Some("fs_main"),
            buffers: &mesh_buffers,
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: W::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: W::ALL,
                }),
            ],
            topology: T::TriangleList,
            depth: depth(true, C::Less),
        });
        let shadow = pipeline(Desc {
            label: "visor sombra",
            layout: &shadow_layout,
            module: &scene,
            vs: "vs_shadow",
            fs: Some("fs_shadow"),
            buffers: &mesh_buffers,
            targets: &[],
            topology: T::TriangleList,
            depth: depth(true, C::LessEqual),
        });
        let overlay_targets_visible = [art(premultiplied, W::ALL), art(None, W::ALL)];
        let overlay_targets_occluded = [art(premultiplied, W::ALL), art(None, W::empty())];
        let ghost = pipeline(Desc {
            label: "visor fantasma",
            layout: &scene_layout,
            module: &scene,
            vs: "vs_main",
            fs: Some("fs_ghost"),
            buffers: &mesh_buffers,
            targets: &overlay_targets_occluded,
            topology: T::TriangleList,
            depth: depth(false, C::LessEqual),
        });
        let post = pipeline(Desc {
            label: "visor post",
            layout: &post_layout,
            module: &post_module,
            vs: "vs_full",
            fs: Some("fs_post"),
            buffers: &[],
            targets: &[art(None, W::ALL)],
            topology: T::TriangleList,
            depth: None,
        });
        let overlay_pipeline = |label, fs, topology, visible: bool| {
            pipeline(Desc {
                label,
                layout: &overlay_layout,
                module: &overlay,
                vs: "vs_overlay",
                fs: Some(fs),
                buffers: &ov_buffers,
                targets: if visible {
                    &overlay_targets_visible
                } else {
                    &overlay_targets_occluded
                },
                topology,
                depth: depth(false, if visible { C::LessEqual } else { C::Greater }),
            })
        };
        let lines_visible = overlay_pipeline("visor líneas", "fs_visible", T::LineList, true);
        let tris_visible = overlay_pipeline("visor rellenos", "fs_visible", T::TriangleList, true);
        let lines_occluded =
            overlay_pipeline("visor líneas tapadas", "fs_occluded", T::LineList, false);
        let tris_occluded = overlay_pipeline(
            "visor rellenos tapados",
            "fs_occluded",
            T::TriangleList,
            false,
        );
        let blit = pipeline(Desc {
            label: "visor blit",
            layout: &blit_layout,
            module: &blit_module,
            vs: "vs_full",
            fs: Some("fs_blit"),
            buffers: &[],
            targets: &[Some(wgpu::ColorTargetState {
                format: target,
                blend: None,
                write_mask: W::ALL,
            })],
            topology: T::TriangleList,
            depth: None,
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("visor sombra"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(C::LessEqual),
            ..Default::default()
        });
        let lut_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("visor LUT"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let lut_image = load_lut();
        let lut_on = lut_image.is_some();
        let lut_image =
            lut_image.unwrap_or_else(|| image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])));
        let lut = device
            .create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("visor LUT"),
                    size: wgpu::Extent3d {
                        width: lut_image.width(),
                        height: lut_image.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    // Sin conversión sRGB, como el importador de Unity (sRGBTexture: 0).
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                lut_image.as_raw(),
            )
            .create_view(&Default::default());
        Self {
            scene_bgl,
            shadow_bgl,
            post_bgl,
            uniform_bgl,
            blit_bgl,
            main,
            shadow,
            ghost,
            post,
            lines_visible,
            tris_visible,
            lines_occluded,
            tris_occluded,
            blit,
            shadow_sampler,
            lut_sampler,
            lut,
            lut_on,
            target_is_srgb: target.is_srgb(),
        }
    }
}

/// LUT de la paleta del estilo pixel 3D de a_game (tira 1024×32), de `AGAME_ROOT` o `../a_game`.
fn load_lut() -> Option<image::RgbaImage> {
    const LUT: &str = "Assets/_Game/Rendering/Resources/pixel3d_lut.png";
    let roots = [
        std::env::var_os("AGAME_ROOT").map(std::path::PathBuf::from),
        Some("../a_game".into()),
        Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../a_game")),
    ];
    let path = roots
        .into_iter()
        .flatten()
        .map(|r| r.join(LUT))
        .find(|p| p.is_file())?;
    match image::open(&path) {
        Ok(img) if img.width() == 1024 && img.height() == 32 => Some(img.to_rgba8()),
        Ok(img) => {
            eprintln!(
                "[visor] LUT de tamaño inesperado {}×{}: sin paleta",
                img.width(),
                img.height()
            );
            None
        }
        Err(e) => {
            eprintln!("[visor] no se pudo leer la LUT {}: {e}", path.display());
            None
        }
    }
}

struct Targets {
    size: [u32; 2],
    color: wgpu::TextureView,
    normal: wgpu::TextureView,
    depth: wgpu::TextureView,
    overlay_color: wgpu::TextureView,
    overlay_mask: wgpu::TextureView,
    art: wgpu::Texture,
    art_view: wgpu::TextureView,
    post_bg: wgpu::BindGroup,
    blit_bg: wgpu::BindGroup,
}

/// Lo de un visor: malla, texturas de arte (se rehacen al cambiar de tamaño), mapa de sombras y uniformes.
pub(crate) struct Gpu {
    mesh: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    targets: Option<Targets>,
    shadow: wgpu::TextureView,
    globals: wgpu::Buffer,
    post: wgpu::Buffer,
    overlay: wgpu::Buffer,
    blit: wgpu::Buffer,
    scene_bg: wgpu::BindGroup,
    shadow_bg: wgpu::BindGroup,
    overlay_bg: wgpu::BindGroup,
}

fn uniform_buffer<T: Pod>(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size_of::<T>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    copy: bool,
) -> wgpu::Texture {
    let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    if copy {
        usage |= wgpu::TextureUsages::COPY_SRC;
    }
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    label: &str,
    colors: &[Option<wgpu::RenderPassColorAttachment>],
    depth: Option<wgpu::RenderPassDepthStencilAttachment>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: colors,
        depth_stencil_attachment: depth,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

impl Gpu {
    pub fn new(device: &wgpu::Device, shared: &Shared) -> Self {
        let globals = uniform_buffer::<Globals>(device, "visor globales");
        let post = uniform_buffer::<Post>(device, "visor post");
        let overlay = uniform_buffer::<OvUniform>(device, "visor capas");
        let blit = uniform_buffer::<BlitUniform>(device, "visor blit");
        let shadow = texture(
            device,
            "visor sombra",
            [SHADOW_SIZE; 2],
            DEPTH_FORMAT,
            false,
        )
        .create_view(&Default::default());
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("visor escena"),
            layout: &shared.scene_bgl,
            entries: &[
                entry(0, globals.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&shadow)),
                entry(2, wgpu::BindingResource::Sampler(&shared.shadow_sampler)),
            ],
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("visor sombra"),
            layout: &shared.shadow_bgl,
            entries: &[entry(0, globals.as_entire_binding())],
        });
        let overlay_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("visor capas"),
            layout: &shared.uniform_bgl,
            entries: &[entry(0, overlay.as_entire_binding())],
        });
        Self {
            mesh: None,
            targets: None,
            shadow,
            globals,
            post,
            overlay,
            blit,
            scene_bg,
            shadow_bg,
            overlay_bg,
        }
    }

    pub fn upload_mesh(&mut self, device: &wgpu::Device, mesh: &MeshData) {
        self.mesh = (!mesh.indices.is_empty()).then(|| {
            let init = |label, contents, usage| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
            };
            (
                init(
                    "visor vértices",
                    bytemuck::cast_slice(&mesh.vertices),
                    wgpu::BufferUsages::VERTEX,
                ),
                init(
                    "visor índices",
                    bytemuck::cast_slice(&mesh.indices),
                    wgpu::BufferUsages::INDEX,
                ),
                mesh.indices.len() as u32,
            )
        });
    }

    fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        shared: &Shared,
        size: [u32; 2],
    ) -> &Targets {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let view = |t: wgpu::Texture| t.create_view(&Default::default());
            let color = view(texture(device, "visor color", size, COLOR_FORMAT, false));
            let normal = view(texture(device, "visor normales", size, COLOR_FORMAT, false));
            let depth = view(texture(
                device,
                "visor profundidad",
                size,
                DEPTH_FORMAT,
                false,
            ));
            let overlay_color = view(texture(device, "visor capas", size, ART_FORMAT, false));
            let overlay_mask = view(texture(device, "visor máscara", size, ART_FORMAT, false));
            let art = texture(device, "visor arte", size, ART_FORMAT, true);
            let art_view = art.create_view(&Default::default());
            let tv = wgpu::BindingResource::TextureView;
            let post_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("visor post"),
                layout: &shared.post_bgl,
                entries: &[
                    entry(0, tv(&color)),
                    entry(1, tv(&normal)),
                    entry(2, tv(&depth)),
                    entry(3, tv(&overlay_color)),
                    entry(4, tv(&overlay_mask)),
                    entry(5, tv(&shared.lut)),
                    entry(6, wgpu::BindingResource::Sampler(&shared.lut_sampler)),
                    entry(7, self.post.as_entire_binding()),
                ],
            });
            let blit_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("visor blit"),
                layout: &shared.blit_bgl,
                entries: &[
                    entry(0, tv(&art_view)),
                    entry(1, self.blit.as_entire_binding()),
                ],
            });
            self.targets = Some(Targets {
                size,
                color,
                normal,
                depth,
                overlay_color,
                overlay_mask,
                art,
                art_view,
                post_bg,
                blit_bg,
            });
        }
        self.targets.as_ref().unwrap()
    }

    /// Dibuja una imagen de `size` píxeles de arte en la textura de arte.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shared: &Shared,
        size: [u32; 2],
        f: &Frame,
    ) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&f.globals));
        queue.write_buffer(&self.post, 0, bytemuck::bytes_of(&f.post));
        queue.write_buffer(&self.overlay, 0, bytemuck::bytes_of(&f.overlay_uniform));
        self.ensure_targets(device, shared, size);
        let t = self.targets.as_ref().unwrap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("visor"),
        });
        let clear_depth = |view| {
            Some(wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            })
        };
        let color = |view, load| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        {
            let mut rp = pass(&mut encoder, "visor sombra", &[], clear_depth(&self.shadow));
            if let Some((vb, ib, count)) = &self.mesh {
                rp.set_pipeline(&shared.shadow);
                rp.set_bind_group(0, &self.shadow_bg, &[]);
                rp.set_vertex_buffer(0, vb.slice(..));
                rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..*count, 0, 0..1);
            }
        }
        {
            let mut rp = pass(
                &mut encoder,
                "visor escena",
                &[color(&t.color, clear), color(&t.normal, clear)],
                clear_depth(&t.depth),
            );
            if let Some((vb, ib, count)) = &self.mesh {
                rp.set_pipeline(&shared.main);
                rp.set_bind_group(0, &self.scene_bg, &[]);
                rp.set_vertex_buffer(0, vb.slice(..));
                rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..*count, 0, 0..1);
            }
        }
        let buffers: Vec<_> = [
            &f.overlay.plain.lines,
            &f.overlay.plain.tris,
            &f.overlay.xray.lines,
            &f.overlay.xray.tris,
        ]
        .map(|v| {
            (!v.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("visor capas"),
                    contents: bytemuck::cast_slice(v),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        })
        .into_iter()
        .zip([
            f.overlay.plain.lines.len(),
            f.overlay.plain.tris.len(),
            f.overlay.xray.lines.len(),
            f.overlay.xray.tris.len(),
        ])
        .collect();
        {
            let depth = Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            });
            let mut rp = pass(
                &mut encoder,
                "visor capas",
                &[
                    color(&t.overlay_color, clear),
                    color(&t.overlay_mask, clear),
                ],
                depth,
            );
            if f.ghost
                && let Some((vb, ib, count)) = &self.mesh
            {
                rp.set_pipeline(&shared.ghost);
                rp.set_bind_group(0, &self.scene_bg, &[]);
                rp.set_vertex_buffer(0, vb.slice(..));
                rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..*count, 0, 0..1);
            }
            rp.set_bind_group(0, &self.overlay_bg, &[]);
            // Orden: lo tapado a rayas primero, luego lo visible encima.
            let draws = [
                (2, &shared.lines_occluded),
                (3, &shared.tris_occluded),
                (1, &shared.tris_visible),
                (3, &shared.tris_visible),
                (0, &shared.lines_visible),
                (2, &shared.lines_visible),
            ];
            for (i, pipeline) in draws {
                if let (Some(buffer), n) = &buffers[i] {
                    rp.set_pipeline(pipeline);
                    rp.set_vertex_buffer(0, buffer.slice(..));
                    rp.draw(0..*n as u32, 0..1);
                }
            }
        }
        {
            let mut rp = pass(
                &mut encoder,
                "visor post",
                &[color(&t.art_view, clear)],
                None,
            );
            rp.set_pipeline(&shared.post);
            rp.set_bind_group(0, &t.post_bg, &[]);
            rp.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }

    /// Callback de egui que escala la textura de arte al visor: `center` en píxeles físicos (ya con el deslizamiento).
    pub fn blit_callback(
        &self,
        queue: &wgpu::Queue,
        shared: &Shared,
        center: [f32; 2],
        scale: u32,
        bg: [f32; 4],
    ) -> Option<BlitCallback> {
        let t = self.targets.as_ref()?;
        let uniform = BlitUniform {
            place: [
                center[0],
                center[1],
                scale as f32,
                if shared.target_is_srgb { 1.0 } else { 0.0 },
            ],
            bg,
        };
        queue.write_buffer(&self.blit, 0, bytemuck::bytes_of(&uniform));
        Some(BlitCallback {
            pipeline: shared.blit.clone(),
            bind_group: t.blit_bg.clone(),
        })
    }

    /// Copia la textura de arte a la CPU (RGBA sRGB, sin relleno de fila).
    pub fn read_art(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<([u32; 2], Vec<u8>)> {
        let t = self.targets.as_ref()?;
        let [w, h] = t.size;
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("visor lectura"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            t.art.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            out.extend_from_slice(&data[start..start + (w * 4) as usize]);
        }
        Some(([w, h], out))
    }
}

/// Dibuja la textura de arte escalada dentro de la pasada de egui.
pub(crate) struct BlitCallback {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl egui_wgpu::CallbackTrait for BlitCallback {
    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _resources: &egui_wgpu::CallbackResources,
    ) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
