//! Fonctions de création de ressources GPU (textures, buffers, bind groups) utilisées
//! par `renderer.rs` lors du setup et de la synchronisation des textures/uniforms.
//! Extrait de `renderer.rs` (Sprint 113a) — aucun changement de comportement, les
//! signatures/corps sont identiques à ceux d'origine.

use std::collections::HashMap;
use std::sync::Arc;

use winit::window::Window;

use super::mesh::{GpuMesh, Vertex};
use super::passes::build_grid_verts;
use super::renderer::{
    BLOOM_MIP_LEVELS, CameraUniform, DEPTH_FORMAT, GizmoVertex, HDR_FORMAT, JOINT_SLOT_BYTES,
    MAX_SKINNED_INSTANCES, ModelUniform, SHADOW_SIZE, SceneUniform,
};
use crate::app::RING_SEGMENTS;
use crate::editor::Editor;
use crate::scene::MeshKind;

pub(super) fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Décode une image (disque ou `bundle://`) en RGBA8 + dimensions. `pub(crate)` :
/// aussi utilisé par `editor::hud` pour les widgets HUD `Image` (cf. Sprint 109),
/// pas seulement les textures de mesh de ce module.
pub(crate) fn load_rgba(path: &str) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = crate::assets::read_bytes(path)?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Nombre de mips pour une texture `width`×`height` : `1 + log2(plus
/// grande dimension)`, la formule standard — 256 → 9 niveaux (256..1), 1×1 → 1 (rien
/// à générer). `leading_zeros` sur `u32` : direct, sans dépendance à une fonction
/// `log2` flottante (imprécisions d'arrondi à éviter sur un compte de niveaux entier).
pub(super) fn mip_count_for(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

/// Crée une texture RGBA8 + son bind group (groupe 3) prêt à lier, avec sa chaîne de
/// mips complète : sans elle, un objet texturé vu de loin agrège l'aliasing
/// du mip 0 au lieu de moyenner vers une version plus petite — c'est tout l'intérêt de
/// `mip_count_for`/de générer les niveaux suivants ici plutôt que de rester à 1 seul.
#[allow(clippy::too_many_arguments)]
pub(super) fn make_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    mipgen_pipeline: &wgpu::RenderPipeline,
    mipgen_layout: &wgpu::BindGroupLayout,
    mipgen_sampler: &wgpu::Sampler,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mip_count = mip_count_for(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("albedo"),
        size,
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        // `RENDER_ATTACHMENT` en plus de `TEXTURE_BINDING`/`COPY_DST` : chaque mip > 0
        // est rempli en le ciblant comme cible de rendu (blit), pas via `write_texture`
        // (qui n'a pas de filtre de réduction intégré).
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );

    if mip_count > 1 {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mipgen_encoder"),
        });
        let mut prev_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("mip_src"),
            base_mip_level: 0,
            mip_level_count: Some(1),
            ..Default::default()
        });
        for level in 1..mip_count {
            let target_view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("mip_dst"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mipgen_bg"),
                layout: mipgen_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&prev_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(mipgen_sampler),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mipgen_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(mipgen_pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            prev_view = target_view;
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    // Vue par défaut (tous les mips) : c'est celle-ci que le shader échantillonne,
    // le sampler choisit/mélange le niveau selon les dérivées d'écran (`mipmap_filter`
    // du sampler, cf. `tex_sampler`).
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tex_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Crée le buffer storage d'instances + son bind group (groupe 1) pour `capacity` objets.
pub(super) fn create_models_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: usize,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let size = (capacity * std::mem::size_of::<ModelUniform>()) as wgpu::BufferAddress;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("models_storage"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("models_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bg)
}

pub(super) fn create_uniform(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn create_depth_view(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Texture HDR intermédiaire : cible de la passe principale avant tone
/// mapping. `width`/`height` explicites plutôt qu'une `SurfaceConfiguration` : réutilisée
/// aussi bien par le chemin fenêtré (taille de la fenêtre) que par les rendus headless
/// (taille demandée par l'appelant, indépendante de toute fenêtre).
pub(super) fn create_hdr_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hdr_color"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Chaîne de mips du bloom : une texture à `BLOOM_MIP_LEVELS` niveaux,
/// démarrant à moitié de la résolution HDR (`width`/`height` = celles de `hdr_view`) —
/// une vue par niveau (`base_mip_level` fixé, `mip_level_count: 1`), utilisable aussi
/// bien comme cible de rendu que comme texture échantillonnée (jamais les deux à la
/// fois dans la même passe).
pub(super) fn create_bloom_mip_views(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Vec<wgpu::TextureView> {
    let base_width = (width / 2).max(2);
    let base_height = (height / 2).max(2);
    // `mip_level_count` ne peut pas dépasser ce que la taille de base permet
    // (log2(min(w,h)) + 1) — sinon `create_texture` échoue la validation WebGPU,
    // ce qui invalide la texture, son bind group, son pipeline, puis (même
    // command encoder) TOUT le rendu de la frame, écran noir y compris pour la
    // passe principale qui elle est saine. Se produit sur une fenêtre/canvas
    // minuscule (ex. avant le premier resize réel, où `base_width`/`base_height`
    // retombent sur le plancher `.max(2)`, insuffisant pour 4 niveaux).
    let max_mips = base_width.min(base_height).ilog2() + 1;
    let mip_level_count = BLOOM_MIP_LEVELS.min(max_mips);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bloom_chain"),
        size: wgpu::Extent3d {
            width: base_width,
            height: base_height,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    (0..mip_level_count)
        .map(|level| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("bloom_mip_view"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect()
}
pub(super) struct PipelineBundle {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) sky_pipeline: wgpu::RenderPipeline,
    pub(super) tonemap_pipeline: wgpu::RenderPipeline,
    pub(super) tonemap_layout: wgpu::BindGroupLayout,
    pub(super) tonemap_sampler: wgpu::Sampler,
    pub(super) hdr_view: wgpu::TextureView,
    pub(super) bloom_threshold_pipeline: wgpu::RenderPipeline,
    pub(super) bloom_downsample_pipeline: wgpu::RenderPipeline,
    pub(super) bloom_upsample_pipeline: wgpu::RenderPipeline,
    pub(super) bloom_sample_layout: wgpu::BindGroupLayout,
    pub(super) bloom_intensity_buf: wgpu::Buffer,
    pub(super) bloom_mip_views: Vec<wgpu::TextureView>,
    pub(super) depth_view: wgpu::TextureView,
    pub(super) model_layout: wgpu::BindGroupLayout,
    pub(super) camera_buf: wgpu::Buffer,
    pub(super) light_buf: wgpu::Buffer,
    pub(super) camera_bind_group: wgpu::BindGroup,
    pub(super) meshes: HashMap<MeshKind, GpuMesh>,
    pub(super) models_buf: wgpu::Buffer,
    pub(super) models_bind_group: wgpu::BindGroup,
    pub(super) models_capacity: usize,
    pub(super) gizmo_pipeline: wgpu::RenderPipeline,
    pub(super) gizmo_vbuf: wgpu::Buffer,
    pub(super) debug_vbuf: wgpu::Buffer,
    pub(super) debug_capacity: usize,
    pub(super) grid_pipeline: wgpu::RenderPipeline,
    pub(super) grid_vbuf: wgpu::Buffer,
    pub(super) grid_count: u32,
    pub(super) shadow_view: wgpu::TextureView,
    pub(super) shadow_bind_group: wgpu::BindGroup,
    pub(super) shadow_pipeline: wgpu::RenderPipeline,
    pub(super) tex_layout: wgpu::BindGroupLayout,
    pub(super) tex_sampler: wgpu::Sampler,
    pub(super) textures: HashMap<String, wgpu::BindGroup>,
    pub(super) mipgen_pipeline: wgpu::RenderPipeline,
    pub(super) mipgen_layout: wgpu::BindGroupLayout,
    pub(super) mipgen_sampler: wgpu::Sampler,
    pub(super) editor: Option<Editor>,
    pub(super) skinned_pipeline: wgpu::RenderPipeline,
    pub(super) joint_buf: wgpu::Buffer,
    pub(super) joint_bind_group: wgpu::BindGroup,
}

/// Construit toutes les pipelines/layouts/samplers/ressources GPU de départ (hors
/// device/queue/surface, déjà créés par l'appelant). Extrait de `Renderer::new_impl`
/// (Sprint 113a) — corps inchangé, seul le point de retour diffère (un `PipelineBundle`
/// au lieu d'alimenter directement le littéral `Renderer { .. }`).
pub(super) fn build(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: &wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Option<&Arc<Window>>,
) -> PipelineBundle {
    // --- Caméra + lumière (bind group 0) ---
    let camera_buf = create_uniform(device, "camera", std::mem::size_of::<CameraUniform>());
    let light_buf = create_uniform(device, "light", std::mem::size_of::<SceneUniform>());
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera_layout"),
        entries: &[uniform_entry(0), uniform_entry(1)],
    });
    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera_bg"),
        layout: &camera_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: light_buf.as_entire_binding(),
            },
        ],
    });

    // --- Layout des objets (bind group 1) : tableau d'instances (storage, lecture) ---
    let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("model_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let models_capacity = 64usize;
    let (models_buf, models_bind_group) =
        create_models_buffer(device, &model_layout, models_capacity);

    // --- Carte d'ombre (shadow map) ---
    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow_map"),
        size: wgpu::Extent3d {
            width: SHADOW_SIZE,
            height: SHADOW_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow_sampler"),
        compare: Some(wgpu::CompareFunction::LessEqual),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    });
    let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow_bg"),
        layout: &shadow_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&shadow_sampler),
            },
        ],
    });

    // --- Génération de mipmaps à l'import : chaîne de blits, un niveau
    // à la fois, chacun un simple échantillonnage bilinéaire du niveau précédent
    // (moitié résolution) — cf. `make_texture`/`mip_count_for`. Pipeline dédiée
    // (pas celle du bloom, format différent — `Rgba8UnormSrgb` des textures
    // albédo, pas `HDR_FORMAT`) mais même principe de triangle plein écran.
    let mipgen_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mipgen_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/mipgen.wgsl").into()),
    });
    let mipgen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mipgen_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let mipgen_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("mipgen_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let mipgen_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mipgen_pipeline_layout"),
        bind_group_layouts: &[Some(&mipgen_layout)],
        immediate_size: 0,
    });
    let mipgen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mipgen_pipeline"),
        layout: Some(&mipgen_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &mipgen_shader,
            entry_point: Some("vs_mipgen"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &mipgen_shader,
            entry_point: Some("fs_mipgen"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // --- Textures (bind group 3) ---
    let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("tex_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let tex_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("tex_sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        // Mipmaps : sans `mipmap_filter`, le sampler resterait bloqué
        // sur le mip 0 quelle que soit la chaîne générée par `make_texture` — la
        // sélection/mélange de mip selon la distance (dérivées d'écran) ne se
        // déclenche qu'avec ce filtre renseigné.
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let mut textures = HashMap::new();
    // texture blanche 1×1 par défaut (objets sans texture).
    let white = make_texture(
        device,
        queue,
        &tex_layout,
        &tex_sampler,
        &mipgen_pipeline,
        &mipgen_layout,
        &mipgen_sampler,
        &[255, 255, 255, 255],
        1,
        1,
    );
    textures.insert(String::new(), white);

    // --- Pipeline ---
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("main_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/main.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline_layout"),
        bind_group_layouts: &[
            Some(&camera_layout),
            Some(&model_layout),
            Some(&shadow_bgl),
            Some(&tex_layout),
        ],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // --- Ciel : triangle plein écran sans vertex buffer, dessiné en
    // premier dans la passe principale (avant la géométrie), profondeur à `Always`/
    // pas d'écriture pour ne jamais l'emporter sur un objet réel ni polluer le depth
    // buffer que la passe de géométrie s'apprête à remplir. Réutilise `camera_layout`
    // (groupe 0) : mêmes `camera`/`light` que `pipeline`, aucun bind group dédié.
    let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sky_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sky.wgsl").into()),
    });
    let sky_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky_pipeline_layout"),
        bind_group_layouts: &[Some(&camera_layout)],
        immediate_size: 0,
    });
    let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky_pipeline"),
        layout: Some(&sky_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &sky_shader,
            entry_point: Some("vs_sky"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &sky_shader,
            entry_point: Some("fs_sky"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // --- Tone mapping : passe plein écran qui convertit `HDR_FORMAT`
    // (rempli par `pipeline`/`sky_pipeline`/`grid_pipeline`/`gizmo_pipeline`/
    // `skinned_pipeline` ci-dessus) vers `config.format`, le format d'affichage réel
    // — c'est la seule pipeline de cette fonction qui cible encore `config.format`
    // directement, exprès : elle est le dernier maillon avant présentation/lecture.
    let tonemap_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("tonemap_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/tonemap.wgsl").into()),
    });
    // `bloom_sample_layout` : texture + sampler seuls, partagée par les
    // 3 passes de la chaîne de bloom (seuil, downsample, upsample) — plus légère que
    // `tonemap_layout` ci-dessous, qui porte en plus la texture de bloom déjà
    // remontée et son intensité.
    let bloom_sample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bloom_sample_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let tonemap_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("tonemap_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // Bloom : texture déjà remontée à sa taille pleine par le
            // filtrage bilinéaire du sampler (cf. `Renderer::render_bloom`).
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let tonemap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("tonemap_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let tonemap_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("tonemap_pipeline_layout"),
        bind_group_layouts: &[Some(&tonemap_layout)],
        immediate_size: 0,
    });
    let tonemap_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("tonemap_pipeline"),
        layout: Some(&tonemap_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &tonemap_shader,
            entry_point: Some("vs_tonemap"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &tonemap_shader,
            entry_point: Some("fs_tonemap"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let hdr_view = create_hdr_view(device, size.width, size.height);

    // --- Bloom : seuil + chaîne de mips down/upsample, cf.
    // `Renderer::render_bloom`. Les 3 passes partagent `bloom_sample_layout` (texture
    // + sampler) et le shader `bloom.wgsl` ; seul le blend state distingue
    // downsample (REPLACE) d'upsample (ADD, accumule sur le niveau déjà rempli par
    // la descente).
    let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bloom_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/bloom.wgsl").into()),
    });
    let bloom_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bloom_pipeline_layout"),
        bind_group_layouts: &[Some(&bloom_sample_layout)],
        immediate_size: 0,
    });
    fn make_bloom_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        label: &str,
        entry_point: &'static str,
        blend: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_bloom"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(entry_point),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }
    let bloom_threshold_pipeline = make_bloom_pipeline(
        device,
        &bloom_pipeline_layout,
        &bloom_shader,
        "bloom_threshold_pipeline",
        "fs_threshold",
        wgpu::BlendState::REPLACE,
    );
    let bloom_downsample_pipeline = make_bloom_pipeline(
        device,
        &bloom_pipeline_layout,
        &bloom_shader,
        "bloom_downsample_pipeline",
        "fs_sample",
        wgpu::BlendState::REPLACE,
    );
    let bloom_upsample_pipeline = make_bloom_pipeline(
        device,
        &bloom_pipeline_layout,
        &bloom_shader,
        "bloom_upsample_pipeline",
        "fs_sample",
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
    );
    let bloom_intensity_buf = create_uniform(device, "bloom_intensity", 16);
    let bloom_mip_views = create_bloom_mip_views(device, size.width, size.height);

    // --- Skinning GPU : palette de joints (groupe 4), pipeline vertex
    // dédié + fragment **partagée** avec `pipeline` ci-dessus (même module `shader`,
    // même `fs_main` : un seul endroit qui connaît l'éclairage).
    // Décalage dynamique : plusieurs objets skinnés
    // distincts peuvent être dessinés dans la même frame, chacun avec sa propre
    // palette de joints — un seul gros buffer, un « créneau » par instance, sélectionné
    // au dessin via un offset dynamique plutôt que de réécrire le buffer entre chaque
    // draw (ce qui ne fonctionnerait pas : `queue.write_buffer` n'est pas ordonné avec
    // les draw calls d'un encoder pas encore soumis).
    let joint_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("joint_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let joint_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("joint_buf"),
        size: JOINT_SLOT_BYTES * MAX_SKINNED_INSTANCES as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let joint_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("joint_bind_group"),
        layout: &joint_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &joint_buf,
                offset: 0,
                size: std::num::NonZeroU64::new(JOINT_SLOT_BYTES),
            }),
        }],
    });
    let skinned_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("skinned_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/skinned.wgsl").into()),
    });
    let skinned_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("skinned_pipeline_layout"),
        bind_group_layouts: &[
            Some(&camera_layout),
            Some(&model_layout),
            Some(&shadow_bgl),
            Some(&tex_layout),
            Some(&joint_layout),
        ],
        immediate_size: 0,
    });
    let skinned_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("skinned_pipeline"),
        layout: Some(&skinned_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &skinned_shader,
            entry_point: Some("vs_skinned_main"),
            buffers: &[crate::gfx::mesh::SkinnedVertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // --- Pipeline d'ombre (profondeur seule depuis la lumière) ---
    let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shadow.wgsl").into()),
    });
    let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow_pl"),
        bind_group_layouts: &[Some(&camera_layout), Some(&model_layout)],
        immediate_size: 0,
    });
    let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow_pipeline"),
        layout: Some(&shadow_pl),
        vertex: wgpu::VertexState {
            module: &shadow_shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // cull des faces avant : pousse l'acné d'ombre vers les faces arrière.
            cull_mode: Some(wgpu::Face::Front),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            // biais de profondeur pour réduire l'acné d'ombre.
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // --- Pipeline gizmo (lignes, par-dessus la scène) ---
    let gizmo_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gizmo_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/gizmo.wgsl").into()),
    });
    let gizmo_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gizmo_layout"),
        bind_group_layouts: &[Some(&camera_layout)],
        immediate_size: 0,
    });
    let gizmo_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gizmo_pipeline"),
        layout: Some(&gizmo_layout),
        vertex: wgpu::VertexState {
            module: &gizmo_shader,
            entry_point: Some("vs_main"),
            buffers: &[GizmoVertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &gizmo_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        // dessiné par-dessus : pas de test de profondeur
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    // capacité : 3 axes × RING_SEGMENTS segments × 2 sommets (anneaux de rotation)
    // + un marqueur 3 axes (6 sommets) par lumière ponctuelle + 6 pour la caméra de jeu.
    // 3 axes×anneaux + (croix 6 + ligne spot 2) par lumière + marqueur caméra 6
    // + gizmo translate d'une lumière sélectionnée (6).
    let gizmo_capacity = 3 * RING_SEGMENTS * 2 + crate::scene::MAX_POINT_LIGHTS * 8 + 12;
    let gizmo_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gizmo_vbuf"),
        size: (gizmo_capacity * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Debug drawing : capacité initiale modeste (256 segments), doublée à
    // la demande (cf. `ensure_debug_capacity`) — le volume dépend du gameplay, pas
    // connu à l'avance contrairement aux gizmos de manipulation.
    const INITIAL_DEBUG_CAPACITY: usize = 512; // 512 sommets = 256 segments
    let debug_capacity = INITIAL_DEBUG_CAPACITY;
    let debug_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("debug_vbuf"),
        size: (INITIAL_DEBUG_CAPACITY * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // --- Pipeline grille : même shader lignes, mais AVEC test de profondeur
    //     (la grille au sol passe correctement derrière les objets). ---
    let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid_pipeline"),
        layout: Some(&gizmo_layout),
        vertex: wgpu::VertexState {
            module: &gizmo_shader,
            entry_point: Some("vs_main"),
            buffers: &[GizmoVertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &gizmo_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    // Géométrie statique de la grille (plan XZ, -10..10, axes X/Z accentués).
    let grid_verts = build_grid_verts();
    let grid_count = grid_verts.len() as u32;
    let grid_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid_vbuf"),
        size: (grid_verts.len() * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&grid_vbuf, 0, bytemuck::cast_slice(&grid_verts));

    // --- Meshes (un GpuMesh par type) ---
    let mut meshes = HashMap::new();
    for kind in MeshKind::ALL {
        meshes.insert(kind, GpuMesh::new(device, &kind.mesh_data()));
    }

    let depth_view = create_depth_view(device, config);
    let editor = window.map(|w| Editor::new(device, config.format, w));

    PipelineBundle {
        pipeline,
        sky_pipeline,
        tonemap_pipeline,
        tonemap_layout,
        tonemap_sampler,
        hdr_view,
        bloom_threshold_pipeline,
        bloom_downsample_pipeline,
        bloom_upsample_pipeline,
        bloom_sample_layout,
        bloom_intensity_buf,
        bloom_mip_views,
        depth_view,
        model_layout,
        camera_buf,
        light_buf,
        camera_bind_group,
        meshes,
        models_buf,
        models_bind_group,
        models_capacity,
        gizmo_pipeline,
        gizmo_vbuf,
        debug_vbuf,
        debug_capacity,
        grid_pipeline,
        grid_vbuf,
        grid_count,
        shadow_view,
        shadow_bind_group,
        shadow_pipeline,
        tex_layout,
        tex_sampler,
        textures,
        mipgen_pipeline,
        mipgen_layout,
        mipgen_sampler,
        editor,
        skinned_pipeline,
        joint_buf,
        joint_bind_group,
    }
}
