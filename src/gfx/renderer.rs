//! Couche **rendu pur** (wgpu + egui). Ne contient aucun état métier : la scène,
//! la caméra et la sélection vivent dans `AppState` et sont passées à `render`.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use super::mesh::{GpuMesh, Vertex};
use crate::app::{AppState, GIZMO_LEN, GizmoMode, RING_SEGMENTS, axis_basis, axis_dir};
use crate::editor::Editor;
use crate::scene::{MeshKind, Scene};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GizmoVertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl GizmoVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GizmoVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// Position de la caméra (xyz), pour le terme spéculaire. w inutilisé.
    eye: [f32; 4],
    /// Inverse de `view_proj` (Sprint 89) : déplie un point NDC du plan lointain en
    /// position monde, pour reconstruire la direction de vue dans `sky.wgsl` sans
    /// dépendre d'un dégradé fixe en espace écran (qui resterait immobile si la
    /// caméra pivote). Inutilisé par les autres shaders (`main.wgsl`/`skinned.wgsl`/
    /// `gizmo.wgsl` ne déclarent qu'un préfixe de cet uniform, WGSL l'autorise).
    inv_view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ModelUniform {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    params: [f32; 4], // x = surbrillance (sélection)
    color: [f32; 4],  // teinte (albédo) de l'objet
}

/// Une lumière ponctuelle côté GPU (std140 : deux vec4).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PointLightU {
    pos_range: [f32; 4], // xyz = position, w = portée
    color_int: [f32; 4], // rgb = couleur, w = intensité
    spot: [f32; 4],      // xyz = direction du cône, w = cos(demi-angle) ou -1 (point)
}

/// Éclairage de la scène (groupe 0, binding 1).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SceneUniform {
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4], // x = intensité ambiante
    light_vp: [[f32; 4]; 4],
    num_points: [f32; 4], // x = nombre de lumières ponctuelles actives
    points: [PointLightU; crate::scene::MAX_POINT_LIGHTS],
    /// Ciel + brouillard (Sprint 89) : ajoutés en fin de struct pour ne décaler aucun
    /// des offsets existants ci-dessus (moins de risque de désync avec les shaders qui
    /// ne déclarent qu'un préfixe de cet uniform).
    sky_horizon: [f32; 4], // rgb, w inutilisé
    sky_zenith: [f32; 4], // rgb, w inutilisé
    fog: [f32; 4],        // rgb = couleur, w = densité
}

/// Paramètre du bloom (Sprint 91, groupe dédié du `tonemap_pipeline`) : juste
/// l'intensité, dans son propre petit uniform plutôt que dans `SceneUniform` — le
/// tone mapping est une passe séparée avec son propre bind group, pas de raison de
/// lui faire porter tout `Light`/`Camera` pour un seul flottant.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BloomUniform {
    intensity: [f32; 4], // x = intensité, yzw inutilisés (alignement std140)
}

const SHADOW_SIZE: u32 = 1024;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Cible de rendu HDR (Sprint 90) : la scène (ciel, grille, objets, gizmos, debug
/// drawing, skinning) est dessinée dans cette texture intermédiaire — pas directement
/// dans le format d'affichage final — pour que les valeurs > 1 (émissifs, spéculaire
/// fort) restent représentables au lieu d'être écrêtées avant même le tone mapping.
/// `Rgba16Float` : suffisant pour la plage dynamique visée ici (contrairement à
/// `Rgba32Float`, filtrable nativement sans extension GPU supplémentaire).
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Nombre de niveaux de la chaîne de mips du bloom (Sprint 91) : mip 0 = moitié de la
/// résolution HDR, chaque niveau suivant moitié du précédent. 4 est un compromis
/// raisonnable — assez pour un halo doux qui s'étend sur plusieurs pixels, sans
/// multiplier les passes plein écran par frame (2×(N-1) + 1 = 7 passes ici).
const BLOOM_MIP_LEVELS: u32 = 4;

/// Skinning GPU (Sprint 86-87) : matrices par instance skinnée dans la palette de
/// joints — généreux pour un rig réel (Mixamo : ~50-65 os).
const JOINT_CAPACITY: usize = 128;
/// Nombre d'objets skinnés distincts dessinables dans une même frame (Sprint 87) : un
/// créneau par instance dans `Renderer::joint_buf`, sélectionné au dessin par offset
/// dynamique. Augmenter est un changement d'une ligne si besoin.
const MAX_SKINNED_INSTANCES: usize = 8;
/// Taille en octets d'un créneau de la palette de joints — un objet skinné à la fois.
const JOINT_SLOT_BYTES: wgpu::BufferAddress =
    (JOINT_CAPACITY * std::mem::size_of::<[[f32; 4]; 4]>()) as wgpu::BufferAddress;

/// Descripteur d'une instance dans le plan de rendu (ordre = index dans le buffer storage).
struct InstanceDraw {
    /// Index de l'objet dans `scene.objects` (mesh/texture relus au draw, sans clone).
    /// La scène n'est pas mutée entre la construction du plan et les passes de dessin.
    obj: usize,
    /// Visible par la caméra (frustum culling) — la passe d'ombre l'ignore.
    visible: bool,
}

pub struct Renderer {
    /// `None` en rendu headless (Sprint 80 : tests de non-régression visuelle) — pas de
    /// fenêtre, pas de surface d'écran, pas d'UI egui.
    pub window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    pipeline: wgpu::RenderPipeline,
    /// Fond de ciel (Sprint 89), dessiné en premier dans la passe principale.
    sky_pipeline: wgpu::RenderPipeline,
    /// Tone mapping HDR → LDR (Sprint 90), dessiné après la passe principale.
    tonemap_pipeline: wgpu::RenderPipeline,
    tonemap_layout: wgpu::BindGroupLayout,
    tonemap_sampler: wgpu::Sampler,
    /// Cible HDR (Sprint 90) de la passe principale en mode fenêtré — redimensionnée
    /// dans `resize()`, comme `depth_view`. Les chemins headless/test créent la leur en
    /// local (taille demandée par l'appelant, indépendante de la fenêtre).
    hdr_view: wgpu::TextureView,
    /// Chaîne de bloom (Sprint 91), cf. `render_bloom` — trois pipelines partageant
    /// `bloom_sample_layout` (seuil, downsample, upsample) et une petite texture à
    /// plusieurs mips en mode fenêtré (`bloom_mip_views`, redimensionnée dans
    /// `resize()` comme `hdr_view`).
    bloom_threshold_pipeline: wgpu::RenderPipeline,
    bloom_downsample_pipeline: wgpu::RenderPipeline,
    bloom_upsample_pipeline: wgpu::RenderPipeline,
    bloom_sample_layout: wgpu::BindGroupLayout,
    bloom_intensity_buf: wgpu::Buffer,
    bloom_mip_views: Vec<wgpu::TextureView>,
    depth_view: wgpu::TextureView,
    model_layout: wgpu::BindGroupLayout,

    camera_buf: wgpu::Buffer,
    light_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    meshes: HashMap<MeshKind, GpuMesh>,
    imported_gpu: Vec<GpuMesh>,
    /// Mesh GPU skinné (Sprint 87), aligné avec `imported_gpu`/`Scene::imported` :
    /// `None` pour un import statique (pas de skin), `Some` sinon. Séparé de
    /// `imported_gpu` plutôt qu'un enum : le mesh statique reste disponible même pour un
    /// objet skinné (utile si un jour un LOD non skinné est voulu), et la grande majorité
    /// des entrées n'ont simplement rien ici.
    imported_gpu_skinned: Vec<Option<GpuMesh>>,
    /// Données d'instances de tous les objets (groupe 1, storage), indexées par `instance_index`.
    models_buf: wgpu::Buffer,
    models_bind_group: wgpu::BindGroup,
    models_capacity: usize,
    /// Plan de rendu de la frame : un descripteur par objet, dans l'ordre du buffer d'instances.
    draw_plan: Vec<InstanceDraw>,
    /// Objets skinnés (Sprint 87) : (indice scène, instance_index dans `models_buf`),
    /// hors du batching de `draw_plan` (chaque objet a sa propre palette de joints,
    /// dessiné individuellement par `draw_skinned_objects`). Leurs `ModelUniform` occupent
    /// la queue de `models_buf`, après les objets statiques de `draw_plan`.
    draw_plan_skinned: Vec<(usize, u32)>,
    /// Tampons réutilisés chaque frame (évite deux allocations par frame).
    order_scratch: Vec<usize>,
    models_scratch: Vec<ModelUniform>,
    /// Nombre d'objets au dernier tri de `order_scratch` (re-tri paresseux).
    last_sort_len: usize,
    /// Hash des entrées de rendu (objets + caméra) à la dernière reconstruction du plan
    /// de dessin : si inchangé, on saute le rebuild (skip au repos, sûr par construction).
    last_render_hash: u64,

    gizmo_pipeline: wgpu::RenderPipeline,
    gizmo_vbuf: wgpu::Buffer,

    // --- debug drawing (Sprint 83) : mêmes pipeline/format que les gizmos, buffer
    //     séparé et redimensionnable (le nombre de segments n'est pas borné à l'avance,
    //     contrairement aux gizmos de manipulation). Vidé (`AppState::debug_lines`)
    //     après chaque frame de rendu.
    debug_vbuf: wgpu::Buffer,
    debug_capacity: usize,

    // --- grille de référence au sol (depth-testée, dans la passe principale) ---
    grid_pipeline: wgpu::RenderPipeline,
    grid_vbuf: wgpu::Buffer,
    grid_count: u32,

    // --- ombres (shadow mapping) ---
    shadow_view: wgpu::TextureView,
    shadow_bind_group: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,

    // --- textures (groupe 3) ---
    tex_layout: wgpu::BindGroupLayout,
    tex_sampler: wgpu::Sampler,
    /// Bind groups de texture par chemin ; "" = texture blanche par défaut.
    textures: HashMap<String, wgpu::BindGroup>,

    editor: Option<Editor>,
    /// Nom du backend GPU réel (Metal / Vulkan / …), pour le bandeau d'état.
    backend: String,

    // --- skinning GPU (Sprint 86) : palette de matrices de joints (groupe 4) + pipeline
    //     dédié (vertex `skinned.wgsl`, fragment `fs_main` de main.wgsl **partagée**, même
    //     éclairage que le chemin statique). Pas encore branché sur la boucle de rendu de
    //     scène générale (`render`/`render_scene_headless`) — capacité vérifiée par un
    //     rendu headless dédié, `render_skinned_test` (tests). L'intégration éditeur
    //     (SceneObject animé, Play) est un chantier séparé, délibérément hors de ce sprint.
    skinned_pipeline: wgpu::RenderPipeline,
    joint_buf: wgpu::Buffer,
    joint_bind_group: wgpu::BindGroup,
    joint_capacity: usize,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Renderer, String> {
        let size = window.inner_size();
        Self::new_impl(Some(window), size).await
    }

    /// Rendu headless : pas de fenêtre ni de surface d'écran (Sprint 80, golden tests).
    /// `compatible_surface: None` à la création de l'adaptateur ; format fixe
    /// (`Rgba8UnormSrgb`) puisqu'il n'y a pas de surface pour en dicter un.
    pub async fn new_headless(width: u32, height: u32) -> Result<Renderer, String> {
        let size = winit::dpi::PhysicalSize::new(width.max(1), height.max(1));
        Self::new_impl(None, size).await
    }

    async fn new_impl(
        window: Option<Arc<Window>>,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Result<Renderer, String> {
        let instance = wgpu::Instance::default();
        let surface = match &window {
            Some(w) => Some(
                instance
                    .create_surface(w.clone())
                    .map_err(|e| format!("Création de la surface impossible : {e}"))?,
            ),
            None => None,
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("Aucun adaptateur GPU trouvé : {e}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                // Limites du GPU réel (iOS/mobile en ont de plus basses que les défauts).
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("Échec création du device : {e}"))?;

        let backend = format!("{:?}", adapter.get_info().backend);

        let (format, alpha_mode) = match &surface {
            Some(s) => {
                let caps = s.get_capabilities(&adapter);
                // GPU dégénéré (surface incompatible) : `caps.formats`/`alpha_modes` peuvent
                // être vides → on remonte une erreur claire au lieu de paniquer en indexant `[0]`.
                let format = caps
                    .formats
                    .iter()
                    .copied()
                    .find(|f| f.is_srgb())
                    .or_else(|| caps.formats.first().copied())
                    .ok_or_else(|| "Aucun format de surface supporté par le GPU".to_string())?;
                let alpha_mode =
                    caps.alpha_modes.first().copied().ok_or_else(|| {
                        "Aucun mode alpha de surface supporté par le GPU".to_string()
                    })?;
                (format, alpha_mode)
            }
            // Rendu headless : pas de surface pour dicter un format → fixe, stable d'une
            // machine à l'autre (comparaison de pixels des golden tests).
            None => (
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::CompositeAlphaMode::Opaque,
            ),
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Fifo (vsync) : cale le rendu sur l'écran, fluide et peu gourmand.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        if let Some(s) = &surface {
            s.configure(&device, &config);
        }

        // --- Caméra + lumière (bind group 0) ---
        let camera_buf = create_uniform(&device, "camera", std::mem::size_of::<CameraUniform>());
        let light_buf = create_uniform(&device, "light", std::mem::size_of::<SceneUniform>());
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
            create_models_buffer(&device, &model_layout, models_capacity);

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
            ..Default::default()
        });
        let mut textures = HashMap::new();
        // texture blanche 1×1 par défaut (objets sans texture).
        let white = make_texture(
            &device,
            &queue,
            &tex_layout,
            &tex_sampler,
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

        // --- Ciel (Sprint 89) : triangle plein écran sans vertex buffer, dessiné en
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

        // --- Tone mapping (Sprint 90) : passe plein écran qui convertit `HDR_FORMAT`
        // (rempli par `pipeline`/`sky_pipeline`/`grid_pipeline`/`gizmo_pipeline`/
        // `skinned_pipeline` ci-dessus) vers `config.format`, le format d'affichage réel
        // — c'est la seule pipeline de cette fonction qui cible encore `config.format`
        // directement, exprès : elle est le dernier maillon avant présentation/lecture.
        let tonemap_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tonemap_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/tonemap.wgsl").into()),
        });
        // `bloom_sample_layout` (Sprint 91) : texture + sampler seuls, partagée par les
        // 3 passes de la chaîne de bloom (seuil, downsample, upsample) — plus légère que
        // `tonemap_layout` ci-dessous, qui porte en plus la texture de bloom déjà
        // remontée et son intensité.
        let bloom_sample_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                // Bloom (Sprint 91) : texture déjà remontée à sa taille pleine par le
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
        let tonemap_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
        let hdr_view = create_hdr_view(&device, size.width, size.height);

        // --- Bloom (Sprint 91) : seuil + chaîne de mips down/upsample, cf.
        // `Renderer::render_bloom`. Les 3 passes partagent `bloom_sample_layout` (texture
        // + sampler) et le shader `bloom.wgsl` ; seul le blend state distingue
        // downsample (REPLACE) d'upsample (ADD, accumule sur le niveau déjà rempli par
        // la descente).
        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/bloom.wgsl").into()),
        });
        let bloom_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
            &device,
            &bloom_pipeline_layout,
            &bloom_shader,
            "bloom_threshold_pipeline",
            "fs_threshold",
            wgpu::BlendState::REPLACE,
        );
        let bloom_downsample_pipeline = make_bloom_pipeline(
            &device,
            &bloom_pipeline_layout,
            &bloom_shader,
            "bloom_downsample_pipeline",
            "fs_sample",
            wgpu::BlendState::REPLACE,
        );
        let bloom_upsample_pipeline = make_bloom_pipeline(
            &device,
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
        let bloom_intensity_buf = create_uniform(&device, "bloom_intensity", 16);
        let bloom_mip_views = create_bloom_mip_views(&device, size.width, size.height);

        // --- Skinning GPU (Sprint 86) : palette de joints (groupe 4), pipeline vertex
        // dédié + fragment **partagée** avec `pipeline` ci-dessus (même module `shader`,
        // même `fs_main` : un seul endroit qui connaît l'éclairage).
        // Décalage dynamique (Sprint 87, intégration Play) : plusieurs objets skinnés
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
        let skinned_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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

        // Debug drawing (Sprint 83) : capacité initiale modeste (256 segments), doublée à
        // la demande (cf. `ensure_debug_capacity`) — le volume dépend du gameplay, pas
        // connu à l'avance contrairement aux gizmos de manipulation.
        const INITIAL_DEBUG_CAPACITY: usize = 512; // 512 sommets = 256 segments
        let debug_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("debug_vbuf"),
            size: (INITIAL_DEBUG_CAPACITY * std::mem::size_of::<GizmoVertex>())
                as wgpu::BufferAddress,
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
            meshes.insert(kind, GpuMesh::new(&device, &kind.mesh_data()));
        }

        let depth_view = create_depth_view(&device, &config);
        let editor = window
            .as_ref()
            .map(|w| Editor::new(&device, config.format, w));

        Ok(Renderer {
            window,
            surface,
            device,
            queue,
            config,
            size,
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
            imported_gpu: Vec::new(),
            imported_gpu_skinned: Vec::new(),
            models_buf,
            models_bind_group,
            models_capacity,
            draw_plan: Vec::new(),
            draw_plan_skinned: Vec::new(),
            order_scratch: Vec::new(),
            models_scratch: Vec::new(),
            last_sort_len: usize::MAX,
            last_render_hash: 0,
            gizmo_pipeline,
            gizmo_vbuf,
            debug_vbuf,
            debug_capacity: INITIAL_DEBUG_CAPACITY,
            grid_pipeline,
            grid_vbuf,
            grid_count,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            tex_layout,
            tex_sampler,
            textures,
            editor,
            backend,
            skinned_pipeline,
            joint_buf,
            joint_bind_group,
            joint_capacity: JOINT_CAPACITY,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let Some(surface) = self.surface.as_ref() else {
            return; // rendu headless : pas de surface à reconfigurer
        };
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config);
        self.hdr_view = create_hdr_view(&self.device, new_size.width, new_size.height);
        self.bloom_mip_views =
            create_bloom_mip_views(&self.device, new_size.width, new_size.height);
    }

    /// Recrée `debug_vbuf` en le doublant tant qu'il ne peut pas contenir `n` sommets
    /// (Sprint 83), même politique de croissance que `create_models_buffer`.
    fn ensure_debug_capacity(&mut self, n: usize) {
        if n <= self.debug_capacity {
            return;
        }
        let cap = n.next_power_of_two().max(64);
        self.debug_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("debug_vbuf"),
            size: (cap * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.debug_capacity = cap;
    }

    /// Envoie la palette de matrices de joints d'**une** instance skinnée au GPU, dans son
    /// créneau `slot` du buffer partagé (Sprint 86-87 : offset dynamique, cf. commentaire
    /// sur `JOINT_SLOT_BYTES`). Tronque silencieusement (`log::warn!`) au-delà de
    /// `joint_capacity` plutôt que de paniquer ou d'écrire hors créneau — un rig
    /// anormalement gros dégraderait l'anim plutôt que de planter le rendu. `slot` au-delà
    /// de `MAX_SKINNED_INSTANCES` est ignoré (même logique).
    ///
    /// Renvoie l'offset dynamique (octets) à passer à `set_bind_group(4, .., &[offset])`.
    fn write_joint_matrices(&mut self, slot: usize, matrices: &[glam::Mat4]) -> u32 {
        if slot >= MAX_SKINNED_INSTANCES {
            log::warn!(
                "skinning : créneau {slot} au-delà de la capacité ({MAX_SKINNED_INSTANCES}) — objet ignoré"
            );
            return 0;
        }
        let n = matrices.len().min(self.joint_capacity);
        if matrices.len() > self.joint_capacity {
            log::warn!(
                "skinning : {} joints, capacité {} — le reste est ignoré",
                matrices.len(),
                self.joint_capacity
            );
        }
        let raw: Vec<[[f32; 4]; 4]> = matrices[..n].iter().map(|m| m.to_cols_array_2d()).collect();
        let offset = slot as wgpu::BufferAddress * JOINT_SLOT_BYTES;
        self.queue
            .write_buffer(&self.joint_buf, offset, bytemuck::cast_slice(&raw));
        offset as u32
    }

    /// Calcule et envoie au GPU la palette de joints de chaque objet skinné visible de la
    /// frame (Sprint 87 — `self.draw_plan_skinned`, déjà construit par `write_uniforms`),
    /// **avant** toute passe de rendu (cf. commentaire aux sites d'appel : `write_buffer`
    /// n'est pas ordonné avec les draw calls d'un encoder pas encore soumis). Renvoie les
    /// offsets dynamiques, dans l'ordre de `draw_plan_skinned`, à passer à
    /// `set_bind_group(4, .., &[offset])` lors du dessin réel dans la passe.
    fn prepare_skinned_draws(&mut self, scene: &Scene) -> Vec<u32> {
        let mut offsets = Vec::with_capacity(self.draw_plan_skinned.len());
        for (slot, &(obj_idx, _instance)) in self.draw_plan_skinned.clone().iter().enumerate() {
            let obj = &scene.objects[obj_idx];
            let MeshKind::Imported(mesh_idx) = obj.mesh else {
                offsets.push(0);
                continue;
            };
            let Some(imported) = scene.imported.get(mesh_idx as usize) else {
                offsets.push(0);
                continue;
            };
            let Some(skeleton) = &imported.skeleton else {
                offsets.push(0);
                continue;
            };
            // Sans `AnimationState` (ou clip introuvable/vide) : pose de liaison figée,
            // pas une erreur — un mesh skinné a le droit de rester immobile (décor posé).
            let find_clip = |name: &str| imported.clips.iter().find(|c| c.name == name);
            let anim = obj.animation.as_ref();
            let clip = anim
                .filter(|a| !a.clip.is_empty())
                .and_then(|a| find_clip(&a.clip));
            let time = anim.map(|a| a.time).unwrap_or(0.0);
            // Fondu enchaîné (Sprint 87) : `blend < 1.0` tant qu'une transition est en
            // cours (cf. `AppState::sim_step`) — mélange avec le clip quitté au niveau
            // des poses locales, pas des matrices monde (`compute_joint_matrices_blended`).
            let matrices = match anim.filter(|a| a.blend < 1.0 && !a.prev_clip.is_empty()) {
                Some(a) => crate::scene::import::compute_joint_matrices_blended(
                    skeleton,
                    find_clip(&a.prev_clip),
                    a.prev_time,
                    clip,
                    time,
                    a.blend,
                ),
                None => crate::scene::import::compute_joint_matrices(skeleton, clip, time),
            };
            offsets.push(self.write_joint_matrices(slot, &matrices));
        }
        offsets
    }

    /// Dessine les objets skinnés de `self.draw_plan_skinned`, un draw individuel par
    /// objet (chacun avec sa propre palette de joints — pas de batching possible ici,
    /// contrairement aux objets statiques). `offsets` doit venir de
    /// `prepare_skinned_draws` sur la même frame, dans le même ordre.
    fn draw_skinned_objects<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        scene: &Scene,
        offsets: &[u32],
    ) {
        for (&(obj_idx, instance_index), &offset) in self.draw_plan_skinned.iter().zip(offsets) {
            let obj = &scene.objects[obj_idx];
            let MeshKind::Imported(mesh_idx) = obj.mesh else {
                continue;
            };
            let Some(Some(gpu_mesh)) = self.imported_gpu_skinned.get(mesh_idx as usize) else {
                continue;
            };
            let tex = self
                .textures
                .get(&obj.texture)
                .unwrap_or(&self.textures[""]);
            pass.set_pipeline(&self.skinned_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(3, tex, &[]);
            pass.set_bind_group(4, &self.joint_bind_group, &[offset]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..gpu_mesh.num_indices,
                0,
                instance_index..instance_index + 1,
            );
        }
    }

    /// Rendu headless d'**un** mesh skinné, en une seule instance (Sprint 86, chemin de
    /// test/vérification dédié — pas piloté par `draw_plan_skinned`). `app` ne sert qu'à
    /// fournir caméra + lumière (`write_uniforms`) ; sa scène n'est pas dessinée ici.
    pub fn render_skinned_test(
        &mut self,
        app: &mut AppState,
        mesh: &crate::gfx::mesh::SkinnedMeshData,
        joint_matrices: &[glam::Mat4],
        model_transform: glam::Mat4,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        app.camera.aspect = width as f32 / (height as f32).max(1.0);
        self.write_uniforms(app);
        let joint_offset = self.write_joint_matrices(0, joint_matrices);

        let gpu_mesh = crate::gfx::mesh::GpuMesh::new_skinned(&self.device, mesh);

        let model_uniform = ModelUniform {
            model: model_transform.to_cols_array_2d(),
            normal: glam::Mat4::from_mat3(
                glam::Mat3::from_mat4(model_transform).inverse().transpose(),
            )
            .to_cols_array_2d(),
            params: [0.0, 0.0, 0.6, 0.0], // pas de surbrillance ; roughness 0.6, reste par défaut
            color: [1.0, 1.0, 1.0, 1.0],
        };
        self.queue
            .write_buffer(&self.models_buf, 0, bytemuck::bytes_of(&model_uniform));

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skinned_test_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skinned_test_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // Cible HDR (Sprint 90), locale à cet appel — cf. `hdr_view` de `render()`.
        let hdr_view = create_hdr_view(&self.device, width, height);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("skinned_test_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("skinned_test_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
            pass.set_pipeline(&self.skinned_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(3, &self.textures[""], &[]);
            pass.set_bind_group(4, &self.joint_bind_group, &[joint_offset]);
            pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..gpu_mesh.num_indices, 0, 0..1);
        }

        // Tone mapping (Sprint 90) : HDR → `view` (le format lu par `finish_and_read_rgba`).
        // Pas de bloom ici (Sprint 91) : ce chemin sert uniquement au golden test de
        // skinning, qui n'a pas besoin du post-effet — `hdr_view` réutilisée comme
        // source de bloom factice, neutralisée par une intensité à 0.
        self.tonemap(&mut encoder, &hdr_view, &hdr_view, 0.0, &view);

        self.finish_and_read_rgba(encoder, &target, width, height)
    }

    /// Transmet l'événement à l'UI. Retourne `true` s'il a été consommé par egui.
    pub fn on_ui_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        let (Some(window), Some(editor)) = (self.window.as_ref(), self.editor.as_mut()) else {
            return false; // rendu headless : pas d'UI
        };
        editor.on_window_event(window, event)
    }

    /// Garantit que le buffer d'instances peut contenir `n` objets (le recrée s'il faut).
    fn sync_objects(&mut self, scene: &Scene) {
        let n = scene.objects.len();
        if n > self.models_capacity {
            let cap = n.next_power_of_two().max(64);
            let (buf, bg) = create_models_buffer(&self.device, &self.model_layout, cap);
            self.models_buf = buf;
            self.models_bind_group = bg;
            self.models_capacity = cap;
        }
    }

    /// Résout le `GpuMesh` d'un type de mesh (None si un modèle importé n'est pas encore chargé).
    fn resolve_mesh(&self, mesh: MeshKind) -> Option<&GpuMesh> {
        match mesh {
            MeshKind::Imported(i) => self.imported_gpu.get(i as usize),
            k => self.meshes.get(&k),
        }
    }

    /// Construit les `GpuMesh` des modèles importés pas encore chargés sur GPU.
    fn sync_imported(&mut self, scene: &Scene) {
        while self.imported_gpu.len() < scene.imported.len() {
            let m = &scene.imported[self.imported_gpu.len()];
            self.imported_gpu.push(GpuMesh::new(&self.device, &m.data));
            // Skinning GPU (Sprint 87) : mesh skinné en plus du statique si le glTF a un
            // skin (`ImportedMesh::skeleton`) — `None` sinon, la grande majorité des imports.
            let skinned = m
                .skinned_mesh_data()
                .map(|d| GpuMesh::new_skinned(&self.device, &d));
            self.imported_gpu_skinned.push(skinned);
        }
    }

    /// Charge les textures référencées par la scène pas encore en cache.
    fn sync_textures(&mut self, scene: &Scene) {
        for obj in &scene.objects {
            if obj.texture.is_empty() || self.textures.contains_key(&obj.texture) {
                continue;
            }
            let bg = match load_rgba(&obj.texture) {
                Some((rgba, w, h)) => make_texture(
                    &self.device,
                    &self.queue,
                    &self.tex_layout,
                    &self.tex_sampler,
                    &rgba,
                    w,
                    h,
                ),
                None => {
                    log::error!("Texture illisible : {}", obj.texture);
                    // repli : réutilise la blanche pour ne pas réessayer en boucle
                    make_texture(
                        &self.device,
                        &self.queue,
                        &self.tex_layout,
                        &self.tex_sampler,
                        &[255, 255, 255, 255],
                        1,
                        1,
                    )
                }
            };
            self.textures.insert(obj.texture.clone(), bg);
        }
    }

    /// Pousse les uniforms (caméra + matrices modèle + surbrillance) depuis l'état.
    /// N'écrit le buffer d'un objet que si sa pose ou sa surbrillance a changé.
    fn write_uniforms(&mut self, app: &AppState) {
        let eye = app.camera.eye();
        let view_proj = app.camera.view_proj();
        let camera_uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            // `view_proj` est toujours inversible (projection perspective + vue
            // rigide, jamais dégénérée) : pas de garde-fou nécessaire ici.
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera_uniform));

        // Éclairage de la scène + matrice de la carte d'ombre.
        let l = &app.scene.light;
        let mut dir = glam::Vec3::from_array(l.dir);
        if dir.length_squared() < 1e-6 {
            dir = glam::Vec3::Y;
        }
        dir = dir.normalize();
        // caméra orthographique placée « au niveau de la lumière », regardant l'origine.
        let up = if dir.x.abs() < 1e-3 && dir.z.abs() < 1e-3 {
            glam::Vec3::Z
        } else {
            glam::Vec3::Y
        };
        let view = glam::Mat4::look_at_rh(dir * 20.0, glam::Vec3::ZERO, up);
        let proj = glam::Mat4::orthographic_rh(-12.0, 12.0, -12.0, 12.0, 0.1, 60.0);
        let light_vp = proj * view;
        let mut points = [PointLightU {
            pos_range: [0.0; 4],
            color_int: [0.0; 4],
            spot: [0.0, -1.0, 0.0, -1.0],
        }; crate::scene::MAX_POINT_LIGHTS];
        // Culling/LOD : au-delà de la limite, on garde les lumières les plus proches
        // de la caméra (les plus visibles) plutôt que les premières de la liste. Le
        // plafond dépend de la qualité de rendu visée (perf en mode interactif « Basse »).
        let chosen = app
            .scene
            .nearest_point_lights(eye, app.render_quality.light_budget());
        let count = chosen.len();
        for (slot, &li) in points.iter_mut().zip(&chosen) {
            let pl = &app.scene.point_lights[li];
            slot.pos_range = [
                pl.position[0],
                pl.position[1],
                pl.position[2],
                pl.range.max(0.01),
            ];
            slot.color_int = [pl.color[0], pl.color[1], pl.color[2], pl.intensity];
            // Spot : direction normalisée + cos(demi-angle) ; w = -1 → lumière ponctuelle.
            let d = glam::Vec3::from_array(pl.spot_dir);
            let dir = if d.length_squared() > 1e-6 {
                d.normalize()
            } else {
                glam::Vec3::NEG_Y
            };
            let cos_cut = if pl.spot_angle > 0.0 {
                pl.spot_angle.to_radians().cos()
            } else {
                -1.0
            };
            slot.spot = [dir.x, dir.y, dir.z, cos_cut];
        }
        let scene_uniform = SceneUniform {
            light_dir: [l.dir[0], l.dir[1], l.dir[2], 0.0],
            light_color: [l.color[0], l.color[1], l.color[2], 0.0],
            // .y : vue de debug (Sprint 83) — canal inutilisé jusqu'ici, réutilisé plutôt
            // que d'agrandir l'uniform. Décodé dans `main.wgsl`.
            ambient: [l.ambient, app.debug_view.as_uniform(), 0.0, 0.0],
            light_vp: light_vp.to_cols_array_2d(),
            num_points: [count as f32, 0.0, 0.0, 0.0],
            points,
            sky_horizon: [
                app.scene.sky.horizon_color[0],
                app.scene.sky.horizon_color[1],
                app.scene.sky.horizon_color[2],
                0.0,
            ],
            sky_zenith: [
                app.scene.sky.zenith_color[0],
                app.scene.sky.zenith_color[1],
                app.scene.sky.zenith_color[2],
                0.0,
            ],
            fog: [
                app.scene.sky.fog_color[0],
                app.scene.sky.fog_color[1],
                app.scene.sky.fog_color[2],
                app.scene.sky.fog_density.max(0.0),
            ],
        };
        self.queue
            .write_buffer(&self.light_buf, 0, bytemuck::bytes_of(&scene_uniform));

        // Skip-rebuild : si les entrées de rendu (transforms/couleurs/sélection + caméra)
        // sont identiques à la frame précédente, le plan de dessin et le buffer d'instances
        // sont déjà à jour. Le hash capte TOUT changement pertinent → pas d'affichage figé.
        // (Les uniforms caméra/lumière ci-dessus sont toujours réécrits, ils sont bon marché.)
        let hash = render_input_hash(app);
        if hash == self.last_render_hash && !self.draw_plan.is_empty() {
            return;
        }
        self.last_render_hash = hash;

        // Instances ordonnées par (mesh, texture) pour permettre des draws groupés.
        // On bâtit en parallèle le buffer storage et le plan de rendu (même ordre).
        let planes = frustum_planes(app.camera.view_proj());
        let n = app.scene.objects.len();
        let order = &mut self.order_scratch;
        // Re-tri paresseux : l'ordre (groupé par mesh/texture pour le batching) ne dépend
        // pas des transforms ; on ne le recalcule que quand le nombre d'objets change.
        // Un ordre « périmé » reste une permutation valide de 0..n → rendu correct, au pire
        // batching sous-optimal jusqu'au prochain ajout/retrait.
        if self.last_sort_len != n {
            order.clear();
            order.extend(0..n);
            order.sort_by(|&a, &b| {
                let oa = &app.scene.objects[a];
                let ob = &app.scene.objects[b];
                mesh_key(oa.mesh)
                    .cmp(&mesh_key(ob.mesh))
                    .then_with(|| oa.texture.cmp(&ob.texture))
            });
            self.last_sort_len = n;
        }

        let models = &mut self.models_scratch;
        models.clear();
        self.draw_plan.clear();
        for &i in order.iter() {
            let obj = &app.scene.objects[i];
            // Skinning GPU (Sprint 87) : un objet skinné a sa propre palette de joints,
            // incompatible avec le batching par instances de ce plan — dessiné à part par
            // `draw_skinned_objects`, jamais ici (sinon il apparaîtrait deux fois).
            if is_skinned(&app.scene, obj.mesh) {
                continue;
            }
            let model = obj.transform.matrix();
            let highlight = app.highlight_of(i);
            // Matrice normale = inverse-transposée du bloc 3×3 (correct en scale non uniforme).
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, obj.metallic, obj.roughness, obj.emissive],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            });
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            self.draw_plan.push(InstanceDraw {
                obj: i,
                visible: obj.visible && aabb_visible(&planes, model, lmin, lmax),
            });
        }

        // Objets skinnés (Sprint 87) : leur ModelUniform occupe la queue de `models`,
        // après tous les objets statiques ci-dessus — `draw_skinned_objects` s'en sert
        // comme `base_instance` pour un draw individuel par objet (chacun avec sa propre
        // palette de joints, incompatible avec le batching des statiques).
        self.draw_plan_skinned.clear();
        for &i in order.iter() {
            let obj = &app.scene.objects[i];
            if !is_skinned(&app.scene, obj.mesh) || !obj.visible {
                continue;
            }
            let model = obj.transform.matrix();
            // Culling AABB approximatif : basé sur la pose de liaison (`aabb_min/max` de
            // l'import), pas sur l'enveloppe réelle de la pose animée — simplification
            // assumée (déplacement des os hors de cette boîte possible sur une anim
            // ample), commune même dans des moteurs de production comme premier jet.
            let (lmin, lmax) = app.scene.local_aabb(obj.mesh);
            if !aabb_visible(&planes, model, lmin, lmax) {
                continue;
            }
            let highlight = app.highlight_of(i);
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            let instance_index = models.len() as u32;
            models.push(ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, obj.metallic, obj.roughness, obj.emissive],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            });
            self.draw_plan_skinned.push((i, instance_index));
        }

        if !models.is_empty() {
            self.queue
                .write_buffer(&self.models_buf, 0, bytemuck::cast_slice(models));
        }
    }

    /// Chaîne de bloom (Sprint 91) : seuil (`hdr_source` → `mip_views[0]`), descente
    /// (`mip_views[i]` → `mip_views[i+1]`, remplace), puis remontée (`mip_views[i+1]` →
    /// `mip_views[i]`, additionne) — `mip_views[0]` porte le résultat final en sortie,
    /// à moitié résolution HDR, remonté à pleine taille par le filtrage bilinéaire du
    /// sampler quand `tonemap()` l'échantillonne.
    fn render_bloom(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_source: &wgpu::TextureView,
        mip_views: &[wgpu::TextureView],
    ) {
        let bind = |src: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom_bg"),
                layout: &self.bloom_sample_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.tonemap_sampler),
                    },
                ],
            })
        };
        let draw_into = |encoder: &mut wgpu::CommandEncoder,
                         pipeline: &wgpu::RenderPipeline,
                         bind_group: &wgpu::BindGroup,
                         target: &wgpu::TextureView| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        };

        let threshold_bg = bind(hdr_source);
        draw_into(
            encoder,
            &self.bloom_threshold_pipeline,
            &threshold_bg,
            &mip_views[0],
        );
        for i in 0..mip_views.len() - 1 {
            let bg = bind(&mip_views[i]);
            draw_into(
                encoder,
                &self.bloom_downsample_pipeline,
                &bg,
                &mip_views[i + 1],
            );
        }
        for i in (0..mip_views.len() - 1).rev() {
            let bg = bind(&mip_views[i + 1]);
            draw_into(encoder, &self.bloom_upsample_pipeline, &bg, &mip_views[i]);
        }
    }

    /// Passe de tone mapping (Sprint 90) + composition du bloom (Sprint 91) : lit
    /// `hdr_source` (`HDR_FORMAT`, rempli par la passe principale) et `bloom_source`
    /// (résultat de `render_bloom`, `mip_views[0]`), écrit le résultat dans `output`
    /// (format d'affichage final, `config.format`). Partagée par les trois chemins de
    /// rendu (`render`, `render_scene_headless`, `render_skinned_test`) : un seul
    /// endroit qui connaît la courbe ACES. `bloom_intensity` à 0 (opt-out mobile, cf.
    /// `RenderQuality::bloom_enabled`) neutralise `bloom_source` quel que soit son
    /// contenu — pas besoin d'une texture noire dédiée.
    fn tonemap(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_source: &wgpu::TextureView,
        bloom_source: &wgpu::TextureView,
        bloom_intensity: f32,
        output: &wgpu::TextureView,
    ) {
        self.queue.write_buffer(
            &self.bloom_intensity_buf,
            0,
            bytemuck::bytes_of(&BloomUniform {
                intensity: [bloom_intensity, 0.0, 0.0, 0.0],
            }),
        );
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap_bg"),
            layout: &self.tonemap_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.tonemap_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom_source),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.bloom_intensity_buf.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
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
        pass.set_pipeline(&self.tonemap_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    pub fn render(&mut self, app: &mut AppState) {
        // 0. Acquérir la surface EN PREMIER. Si indisponible, on sort avant de lancer
        //    egui : sinon on jetterait le `textures_delta` de la frame (atlas de police),
        //    ce qui désynchronise le renderer egui (panic).
        let Some(surface) = self.surface.as_ref() else {
            return; // rendu headless : `render_scene_headless` est le chemin utilisé
        };
        use wgpu::CurrentSurfaceTexture as C;
        let frame = match surface.get_current_texture() {
            C::Success(t) | C::Suboptimal(t) => t,
            C::Outdated | C::Lost => {
                surface.configure(&self.device, &self.config);
                return;
            }
            C::Timeout | C::Occluded => return,
            C::Validation => {
                log::error!("surface validation error");
                return;
            }
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(mut editor) = self.editor.take() else {
            return;
        };

        // 1. Construire l'UI éditeur. En mode player : pas de panneaux, mais on
        //    dessine quand même les contrôles tactiles (joystick + boutons).
        // Calculé avant les appels mutant `app` (évite un conflit d'emprunt au site d'appel).
        let game_time = app.hud_timer();
        let score = app.score();
        let lost = app.is_lost();
        let won = app.has_won();
        let wave = app.wave;
        let mut restart = false;
        let mut player_net_actions = None;
        let full_output = if app.player {
            if app.scene.mobile.any() {
                let net_status = app.net_status.clone();
                let net_connected = app.is_connected();
                let weapon_label = app.selected_weapon_label();
                let defeated = app.is_locally_defeated();
                let kills = app.displayed_kill_count();
                let (output, actions) = editor.run_player_overlay(
                    &window,
                    &app.scene,
                    &mut app.input_state,
                    app.device_preview,
                    app.device_portrait,
                    app.hud_health,
                    app.damage_flash,
                    game_time,
                    score,
                    lost,
                    won,
                    wave,
                    &mut restart,
                    &net_status,
                    net_connected,
                    weapon_label,
                    defeated,
                    kills,
                );
                player_net_actions = Some(actions);
                Some(output)
            } else {
                None
            }
        } else {
            let status = crate::editor::StatusInfo {
                fps: app.fps(),
                backend: &self.backend,
                ai_busy: app.ai_busy,
                grid: app.show_grid,
                snap: app.snap,
                debug_view: app.debug_view,
            };
            let net_status = app.net_status.clone();
            let net_connected = app.is_connected();
            let has_firebase_account = app.has_firebase_account();
            let weapon_label = app.selected_weapon_label();
            let defeated = app.is_locally_defeated();
            let kills = app.displayed_kill_count();
            let (full_output, actions) = editor.run(
                &window,
                &mut app.scene,
                &mut app.selection,
                &mut app.selected,
                &mut app.selected_light,
                &mut app.playing,
                &mut app.paused,
                &mut app.time_scale,
                &mut app.gizmo_mode,
                &mut app.input_state,
                &mut app.device_preview,
                &mut app.device_portrait,
                &mut app.view_rect_px,
                app.hud_health,
                app.damage_flash,
                game_time,
                score,
                lost,
                won,
                wave,
                status,
                &net_status,
                net_connected,
                &app.chat_messages,
                has_firebase_account,
                &app.leaderboard,
                weapon_label,
                defeated,
                kills,
            );
            if actions.save {
                app.save();
            }
            if let Some(path) = actions.save_path {
                app.save_to(&path);
            }
            if actions.load {
                app.load(); // asynchrone : la scène est remplacée plus tard (cf. take_imported_dirty)
            }
            if let Some(path) = actions.load_path {
                app.load_from(&path);
            }
            if let Some(path) = actions.import {
                app.import_gltf(&path);
            }
            if let Some(kind) = actions.add {
                app.add_object(kind);
            }
            if let Some(i) = actions.delete {
                app.delete_object(i);
            }
            if actions.duplicate {
                app.duplicate_selected();
            }
            if actions.new_scene {
                app.new_scene();
            }
            if actions.load_demo {
                app.load_mobile_demo();
            }
            if actions.load_gameplay {
                app.load_gameplay_demo();
            }
            if actions.load_controller {
                app.load_controller_demo();
            }
            if actions.load_tower {
                app.load_tower_demo();
            }
            if actions.load_temple_run {
                app.load_temple_run_demo();
            }
            if actions.load_components_demo {
                app.load_components_demo();
            }
            if actions.load_ai_duel {
                app.load_zombies_demo();
            }
            if actions.load_mmorpg {
                app.load_mmorpg_demo();
            }
            if actions.load_roguelike {
                app.load_roguelike_demo();
            }
            if actions.load_brawl {
                app.load_brawl_demo();
            }
            if actions.restart {
                restart = true;
            }
            if let Some((url, name)) = actions.connect_to_server {
                app.connect_to_server(&url, &name);
            }
            if actions.disconnect_from_server {
                app.disconnect_from_server();
            }
            if let Some((email, password)) = actions.firebase_sign_in {
                let settings = editor.settings();
                app.request_firebase_sign_in(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    email,
                    password,
                );
            }
            if let Some((email, password)) = actions.firebase_sign_up {
                let settings = editor.settings();
                app.request_firebase_sign_up(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    email,
                    password,
                );
            }
            if let Some((lobby_code, sender_name, text)) = actions.send_chat_message {
                let settings = editor.settings();
                app.request_send_chat_message(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    lobby_code,
                    sender_name,
                    text,
                );
            }
            if let Some(lobby_code) = actions.refresh_chat {
                let settings = editor.settings();
                app.request_refresh_chat(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    lobby_code,
                );
            }
            if actions.refresh_leaderboard {
                let settings = editor.settings();
                app.request_refresh_leaderboard(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    10,
                );
            }
            if actions.align_ground {
                app.align_to_ground();
            }
            if actions.reset_transform {
                app.reset_transform();
            }
            if actions.quit {
                app.request_quit();
            }
            if actions.undo {
                app.undo();
            }
            if actions.redo {
                app.redo();
            }
            if actions.step_frame {
                app.request_step();
            }
            if let Some(cmd) = actions.console_command {
                let result = app.run_console_command(&cmd);
                log::info!("> {cmd}\n{result}");
            }
            if let Some(clip) = actions.play_audio {
                app.play_audio(&clip);
            }
            if let Some(down) = actions.move_in_list {
                app.move_selected_in_list(down);
            }
            if let Some((from, to)) = actions.reorder {
                app.reorder_object(from, to);
            }
            if let Some((idx, req)) = actions.ai_generate {
                app.request_ai_script(idx, req);
            }
            if let Some((req, replace)) = actions.ai_generate_scene {
                app.request_ai_scene(req, replace);
            }
            if actions.set_game_camera {
                app.set_game_camera();
            }
            if actions.clear_game_camera {
                app.clear_game_camera();
            }
            if let Some(max) = actions.optimize_textures {
                let n = app.optimize_textures(max);
                log::info!("Optimisation : {n} texture(s) réduite(s) à ≤ {max} px");
            }
            if let Some(max) = actions.limit_lights {
                app.limit_point_lights(max);
            }
            if actions.convert_textures_pot {
                let n = app.convert_textures_pot();
                log::info!("Convertisseur : {n} texture(s) en puissances de 2");
            }
            if actions.bake_lighting {
                let n = app.bake_lighting();
                log::info!("Bake lighting : {n} lumière(s) ponctuelle(s) figée(s) en émission");
            }
            if actions.perf_mode {
                let t = app.optimize_textures(1024);
                app.limit_point_lights(4);
                log::info!("Mode performance Android : {t} texture(s) réduite(s), ≤ 4 lumières");
            }
            if actions.collect_assets {
                let n = app.collect_assets();
                log::info!("Assets rassemblés : {n} chemin(s) → asset://");
            }
            if actions.cut {
                app.cut_selected();
            }
            if actions.copy {
                app.copy_selected();
            }
            if actions.paste {
                app.paste();
            }
            if actions.select_all {
                app.select_all();
            }
            if actions.group {
                app.group_selected();
            }
            if actions.ungroup {
                app.ungroup_selected();
            }
            if let Some(axis) = actions.align_axis {
                app.align_selection_axis(axis);
            }
            if let Some(axis) = actions.distribute_axis {
                app.distribute_selection_axis(axis);
            }
            if actions.toggle_grid {
                app.show_grid = !app.show_grid;
            }
            if actions.toggle_snap {
                app.snap = !app.snap;
            }
            if let Some(view) = actions.set_debug_view {
                app.debug_view = view;
            }
            Some(full_output)
        };

        if let Some(actions) = player_net_actions {
            if let Some((url, name)) = actions.connect_to_server {
                app.connect_to_server(&url, &name);
            }
            if actions.disconnect_from_server {
                app.disconnect_from_server();
            }
        }

        // Bouton de fin de partie : « Niveau suivant » uniquement pour la démo contrôleur
        // à niveaux ; sinon « Rejouer » — y compris une victoire par manches (zombies) ou
        // par ligne d'arrivée (course infinie/tour), qui doivent juste relancer la scène.
        if restart {
            if app.has_won() && app.is_leveled_demo {
                app.next_level();
            } else {
                app.restart_game();
            }
        }

        // 2. Comportements (Play), sync GPU, push des uniforms.
        app.advance_play();
        // Une scène chargée en fond vient peut-être de remplacer l'actuelle :
        // reconstruire les meshes GPU importés depuis les nouvelles données.
        if app.take_imported_dirty() {
            self.imported_gpu.clear();
        }
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);

        // Aperçu mobile : restreint la vue 3D à un écran de téléphone (letterbox).
        // L'aspect caméra doit suivre ce rectangle (sinon l'image serait étirée).
        let sw = self.config.width as f32;
        let sh = self.config.height as f32;
        let (dx, dy, dw, dh) = if app.device_preview {
            // Base : région centrale (hors panneaux) remontée par l'éditeur ; sinon plein écran.
            let (bx, by, bw, bh) = app.view_rect_px;
            let (bx, by, bw, bh) = if bw > 1.0 && bh > 1.0 {
                (bx, by, bw, bh)
            } else {
                (0.0, 0.0, sw, sh)
            };
            // Le viewport GPU est en Y depuis le haut, comme les coordonnées egui : pas d'inversion.
            let (rx, ry, rw, rh) = crate::app::device_rect(bw, bh, app.device_portrait);
            (bx + rx, by + ry, rw, rh)
        } else {
            (0.0, 0.0, sw, sh)
        };
        app.camera.aspect = dw / dh.max(1.0);
        self.write_uniforms(app);
        // Skinning GPU (Sprint 87) : joint_buf entièrement rempli AVANT la passe (comme
        // les lignes de debug ci-dessous) — `queue.write_buffer` n'est pas ordonné avec
        // les draw calls d'un encoder pas encore soumis, donc rien de tout ça ne peut
        // être fait entre deux `draw_indexed` de la passe principale plus bas.
        let skinned_offsets = self.prepare_skinned_draws(&app.scene);

        // Préparer les lignes du gizmo + marqueurs de lumières (jamais en player/aperçu mobile).
        let gizmo_count = if app.player || app.device_preview {
            0
        } else {
            let mut verts: Vec<GizmoVertex> = Vec::new();
            // Marqueur en croix 3D à chaque lumière ponctuelle, teinté par sa couleur.
            for pl in &app.scene.point_lights {
                let c = pl.position;
                let col = pl.color;
                let s = 0.3;
                for axis in 0..3 {
                    let d = axis_dir(axis) * s;
                    verts.push(GizmoVertex {
                        position: [c[0] - d.x, c[1] - d.y, c[2] - d.z],
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: [c[0] + d.x, c[1] + d.y, c[2] + d.z],
                        color: col,
                    });
                }
                // Spot : ligne depuis la lumière le long du cône (visualise l'orientation).
                if pl.spot_angle > 0.0 {
                    let dir = glam::Vec3::from_array(pl.spot_dir);
                    let dir = if dir.length_squared() > 1e-6 {
                        dir.normalize()
                    } else {
                        glam::Vec3::NEG_Y
                    };
                    let end = glam::Vec3::from_array(c) + dir * (pl.range * 0.4).max(0.5);
                    verts.push(GizmoVertex {
                        position: c,
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: end.to_array(),
                        color: col,
                    });
                }
            }
            // Marqueur cyan à la position de la caméra de jeu (si définie).
            if let Some(gc) = app.scene.game_camera {
                let pitch = gc.pitch.clamp(-1.54, 1.54);
                let eye = glam::Vec3::from_array(gc.target)
                    + glam::Vec3::new(
                        gc.distance * pitch.cos() * gc.yaw.sin(),
                        gc.distance * pitch.sin(),
                        gc.distance * pitch.cos() * gc.yaw.cos(),
                    );
                let col = [0.2, 0.85, 0.95];
                let s = 0.4;
                for axis in 0..3 {
                    let d = axis_dir(axis) * s;
                    verts.push(GizmoVertex {
                        position: [eye.x - d.x, eye.y - d.y, eye.z - d.z],
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: [eye.x + d.x, eye.y + d.y, eye.z + d.z],
                        color: col,
                    });
                }
            }
            // Gizmo de translation d'une lumière sélectionnée (3 axes colorés).
            if let Some(li) = app.selected_light
                && let Some(pl) = app.scene.point_lights.get(li)
            {
                let o = glam::Vec3::from_array(pl.position);
                let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
                for (axis, color) in colors.iter().enumerate() {
                    let end = o + axis_dir(axis) * GIZMO_LEN;
                    verts.push(GizmoVertex {
                        position: o.to_array(),
                        color: *color,
                    });
                    verts.push(GizmoVertex {
                        position: end.to_array(),
                        color: *color,
                    });
                }
            }
            // Gizmo de manipulation de l'objet sélectionné.
            if let Some(sel) = app.selection {
                let o = app.scene.objects[sel].transform.position;
                let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
                match app.gizmo_mode {
                    // Déplacer / Redimensionner : 3 segments d'axe.
                    GizmoMode::Translate | GizmoMode::Scale => {
                        for (axis, color) in colors.iter().enumerate() {
                            let end = o + axis_dir(axis) * GIZMO_LEN;
                            verts.push(GizmoVertex {
                                position: o.to_array(),
                                color: *color,
                            });
                            verts.push(GizmoVertex {
                                position: end.to_array(),
                                color: *color,
                            });
                        }
                    }
                    // Tourner : 3 anneaux (un par axe).
                    GizmoMode::Rotate => {
                        const N: usize = RING_SEGMENTS;
                        for (axis, color) in colors.iter().enumerate() {
                            let (u, w) = axis_basis(axis_dir(axis));
                            for j in 0..N {
                                let a0 = std::f32::consts::TAU * j as f32 / N as f32;
                                let a1 = std::f32::consts::TAU * (j + 1) as f32 / N as f32;
                                let p0 = o + (u * a0.cos() + w * a0.sin()) * GIZMO_LEN;
                                let p1 = o + (u * a1.cos() + w * a1.sin()) * GIZMO_LEN;
                                verts.push(GizmoVertex {
                                    position: p0.to_array(),
                                    color: *color,
                                });
                                verts.push(GizmoVertex {
                                    position: p1.to_array(),
                                    color: *color,
                                });
                            }
                        }
                    }
                }
            }
            self.queue
                .write_buffer(&self.gizmo_vbuf, 0, bytemuck::cast_slice(&verts));
            verts.len() as u32
        };

        // Debug drawing (Sprint 83) : segments accumulés pendant la frame (picking,
        // gameplay), dessinés une fois puis vidés — jamais persistants d'une frame à
        // l'autre, contrairement aux gizmos de manipulation ci-dessus.
        let debug_count = {
            let verts: Vec<GizmoVertex> = app
                .debug_lines
                .iter()
                .flat_map(|&(a, b, color)| {
                    [
                        GizmoVertex {
                            position: a.to_array(),
                            color,
                        },
                        GizmoVertex {
                            position: b.to_array(),
                            color,
                        },
                    ]
                })
                .collect();
            app.debug_lines.clear();
            if !verts.is_empty() {
                self.ensure_debug_capacity(verts.len());
                self.queue
                    .write_buffer(&self.debug_vbuf, 0, bytemuck::cast_slice(&verts));
            }
            verts.len() as u32
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });

        // Passe d'ombre : profondeur de la scène depuis la lumière → carte d'ombre.
        {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.camera_bind_group, &[]);
            spass.set_bind_group(1, &self.models_bind_group, &[]);
            // Passe d'ombre : rend les objets hors champ (pas de frustum culling), mais
            // **ignore les objets invisibles** (ex. pièce ramassée) pour ne pas laisser
            // d'ombre fantôme. Groupé par mesh, scindé en plages de visibles consécutifs.
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = objs[plan[i].obj].mesh;
                let mut j = i + 1;
                while j < plan.len() && objs[plan[j].obj].mesh == mi {
                    j += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < j {
                        if !objs[plan[k].obj].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < j && objs[plan[k].obj].visible {
                            k += 1;
                        }
                        spass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                    }
                }
                i = j;
            }
        }

        {
            // Sprint 90 : la passe principale dessine dans `hdr_view` (HDR_FORMAT),
            // pas directement dans `view` — `self.tonemap()` fait le dernier maillon
            // vers le format d'affichage, après cette passe.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Aperçu mobile : la scène ne se dessine que dans le rectangle « téléphone ».
            // (Le clear remplit toute la surface → bandes sombres autour = letterbox.)
            pass.set_viewport(dx, dy, dw, dh, 0.0, 1.0);
            pass.set_scissor_rect(dx as u32, dy as u32, dw as u32, dh as u32);

            // Ciel (Sprint 89) : dessiné en premier, derrière tout le reste.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.draw(0..3, 0..1);

            // Grille de référence au sol (depth-testée), en mode édition uniquement.
            if app.show_grid && !app.player && !app.device_preview {
                pass.set_pipeline(&self.grid_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
                pass.draw(0..self.grid_count, 0..1);
            }

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);

            // Rendu instancié : un draw par lot (mesh+texture), scindé en sous-plages
            // d'instances visibles consécutives (frustum culling).
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = objs[plan[i].obj].mesh;
                let tex_key = &objs[plan[i].obj].texture;
                let mut group_end = i + 1;
                while group_end < plan.len()
                    && objs[plan[group_end].obj].mesh == mi
                    && &objs[plan[group_end].obj].texture == tex_key
                {
                    group_end += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    let tex = self
                        .textures
                        .get(tex_key)
                        .unwrap_or_else(|| &self.textures[""]);
                    pass.set_bind_group(3, tex, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    // Plages contiguës d'instances visibles dans le lot.
                    let mut k = i;
                    while k < group_end {
                        if !plan[k].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < group_end && plan[k].visible {
                            k += 1;
                        }
                        pass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                    }
                }
                i = group_end;
            }

            // Gizmo de l'objet sélectionné, par-dessus.
            if gizmo_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.gizmo_vbuf.slice(..));
                pass.draw(0..gizmo_count, 0..1);
            }

            // Debug drawing (Sprint 83) : même pipeline lignes, buffer dédié.
            if debug_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vbuf.slice(..));
                pass.draw(0..debug_count, 0..1);
            }

            // Objets skinnés (Sprint 87) : un draw individuel par objet, palettes déjà
            // envoyées au GPU par `prepare_skinned_draws` avant cette passe.
            self.draw_skinned_objects(&mut pass, &app.scene, &skinned_offsets);
        }

        // Bloom (Sprint 91) : passes de seuil/downsample/upsample sautées entièrement
        // si désactivé (opt-out mobile, `RenderQuality::bloom_enabled`) — pas seulement
        // neutralisées côté shader, un vrai gain de perf sur le palier visé.
        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        if bloom_intensity > 0.0 {
            self.render_bloom(&mut encoder, &self.hdr_view, &self.bloom_mip_views);
        }
        // Tone mapping (Sprint 90) : HDR → `view` (format d'affichage réel), avant l'UI
        // (l'UI egui reste en LDR, peinte par-dessus l'image déjà tonemappée).
        self.tonemap(
            &mut encoder,
            &self.hdr_view,
            &self.bloom_mip_views[0],
            bloom_intensity,
            &view,
        );

        // 3. Peindre l'UI egui par-dessus la scène (sauf en mode player).
        let extra = match full_output {
            Some(output) => editor.paint(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                [self.config.width, self.config.height],
                output,
            ),
            None => Vec::new(),
        };
        self.editor = Some(editor);

        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        frame.present();
    }

    /// Rendu headless d'une scène dans une texture hors-écran : passe d'ombre + passe
    /// principale, **sans** grille, gizmos ni UI egui (Sprint 80 : golden tests de
    /// non-régression visuelle). Le pipeline utilisé — mêmes shaders, mêmes bind groups —
    /// est celui de [`Renderer::render`] : un shader qui dérive fait dériver les deux.
    /// Retourne les pixels RGBA8 (`width`×`height`, 4 octets/pixel, sans padding de ligne).
    pub fn render_scene_headless(
        &mut self,
        app: &mut AppState,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);
        app.camera.aspect = width as f32 / (height as f32).max(1.0);
        self.write_uniforms(app);
        // Skinning GPU (Sprint 87) : cf. commentaire équivalent dans `render()`.
        let skinned_offsets = self.prepare_skinned_draws(&app.scene);

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        // Depth dédiée à la taille demandée (peut différer de `self.depth_view`, qui suit
        // la taille de la fenêtre en mode interactif).
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // Cible HDR (Sprint 90), locale à cet appel — cf. `hdr_view` de `render()`.
        let hdr_view = create_hdr_view(&self.device, width, height);
        // Chaîne de bloom (Sprint 91), locale à cet appel — cf. `bloom_mip_views` de
        // `render()`.
        let bloom_mip_views = create_bloom_mip_views(&self.device, width, height);

        // Debug drawing (Sprint 83) : même logique que `render()` (préparer + vider avant
        // les passes, dessiner après les meshes texturés dans la passe principale).
        let debug_count = {
            let verts: Vec<GizmoVertex> = app
                .debug_lines
                .iter()
                .flat_map(|&(a, b, color)| {
                    [
                        GizmoVertex {
                            position: a.to_array(),
                            color,
                        },
                        GizmoVertex {
                            position: b.to_array(),
                            color,
                        },
                    ]
                })
                .collect();
            app.debug_lines.clear();
            if !verts.is_empty() {
                self.ensure_debug_capacity(verts.len());
                self.queue
                    .write_buffer(&self.debug_vbuf, 0, bytemuck::cast_slice(&verts));
            }
            verts.len() as u32
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("headless_encoder"),
            });

        // Passe d'ombre — identique à celle de `render()`, sans les gizmos ni l'UI.
        {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.camera_bind_group, &[]);
            spass.set_bind_group(1, &self.models_bind_group, &[]);
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = objs[plan[i].obj].mesh;
                let mut j = i + 1;
                while j < plan.len() && objs[plan[j].obj].mesh == mi {
                    j += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < j {
                        if !objs[plan[k].obj].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < j && objs[plan[k].obj].visible {
                            k += 1;
                        }
                        spass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                    }
                }
                i = j;
            }
        }

        // Passe principale — identique à celle de `render()`, sans grille ni gizmos.
        // Dessine dans `hdr_view` (Sprint 90) ; `self.tonemap()` fait le dernier pas
        // vers `view`, juste avant la lecture des pixels.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);

            // Ciel (Sprint 89) : même geste que dans `render()`.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.draw(0..3, 0..1);

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);

            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = objs[plan[i].obj].mesh;
                let tex_key = &objs[plan[i].obj].texture;
                let mut group_end = i + 1;
                while group_end < plan.len()
                    && objs[plan[group_end].obj].mesh == mi
                    && &objs[plan[group_end].obj].texture == tex_key
                {
                    group_end += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    let tex = self
                        .textures
                        .get(tex_key)
                        .unwrap_or_else(|| &self.textures[""]);
                    pass.set_bind_group(3, tex, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < group_end {
                        if !plan[k].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < group_end && plan[k].visible {
                            k += 1;
                        }
                        pass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                    }
                }
                i = group_end;
            }

            // Debug drawing (Sprint 83).
            if debug_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vbuf.slice(..));
                pass.draw(0..debug_count, 0..1);
            }

            // Objets skinnés (Sprint 87) : cf. commentaire équivalent dans `render()`.
            self.draw_skinned_objects(&mut pass, &app.scene, &skinned_offsets);
        }

        // Bloom (Sprint 91) : cf. commentaire équivalent dans `render()`.
        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        if bloom_intensity > 0.0 {
            self.render_bloom(&mut encoder, &hdr_view, &bloom_mip_views);
        }
        // Tone mapping (Sprint 90) : HDR → `view` (le format lu par `finish_and_read_rgba`).
        self.tonemap(
            &mut encoder,
            &hdr_view,
            &bloom_mip_views[0],
            bloom_intensity,
            &view,
        );

        self.finish_and_read_rgba(encoder, &target, width, height)
    }

    /// Copie `target` vers un buffer lisible CPU, soumet `encoder` et attend le résultat —
    /// partagé par tous les rendus headless (`render_scene_headless`, `render_skinned_test`
    /// (Sprint 86)). `encoder` doit déjà contenir toutes les passes de dessin dans `target` ;
    /// cette méthode ne fait que la copie finale + lecture.
    fn finish_and_read_rgba(
        &self,
        mut encoder: wgpu::CommandEncoder,
        target: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        // wgpu impose que `bytes_per_row` soit un multiple de
        // `COPY_BYTES_PER_ROW_ALIGNMENT` (256) → on copie avec ce padding puis on le retire.
        let bytes_per_pixel = 4u32;
        let unpadded = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless_readback"),
            size: (padded * height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        // Le callback de `map_async` est invoqué par `poll` ci-dessus : à ce stade le
        // résultat est déjà dans le canal.
        let _ = rx.recv();
        let mapped = slice.get_mapped_range();
        let mut out = vec![0u8; (unpadded * height) as usize];
        for y in 0..height {
            let src_start = (y * padded) as usize;
            let dst_start = (y * unpadded) as usize;
            out[dst_start..dst_start + unpadded as usize]
                .copy_from_slice(&mapped[src_start..src_start + unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();
        out
    }
}

/// `true` si `mesh` référence un import glTF skinné (Sprint 87) — c'est-à-dire dont
/// `ImportedMesh::skeleton` est renseigné. Toujours `false` pour les primitives, qui ne
/// sont jamais skinnées.
fn is_skinned(scene: &Scene, mesh: MeshKind) -> bool {
    match mesh {
        MeshKind::Imported(i) => scene
            .imported
            .get(i as usize)
            .is_some_and(|m| m.skeleton.is_some()),
        _ => false,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

/// Décode une image (disque ou `bundle://`) en RGBA8 + dimensions.
fn load_rgba(path: &str) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = crate::assets::read_bytes(path)?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Crée une texture RGBA8 + son bind group (groupe 3) prêt à lier.
fn make_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("albedo"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
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

/// Les 6 plans du frustum (méthode de Gribb-Hartmann) extraits de la view-projection.
/// Chaque plan `(a,b,c,d)` : un point `p` est dans le frustum si `a·px+b·py+c·pz+d ≥ 0`.
fn frustum_planes(vp: glam::Mat4) -> [glam::Vec4; 6] {
    let m = vp.to_cols_array_2d(); // m[col][row]
    let row = |r: usize| glam::Vec4::new(m[0][r], m[1][r], m[2][r], m[3][r]);
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    [
        r3 + r0, // gauche
        r3 - r0, // droite
        r3 + r1, // bas
        r3 - r1, // haut
        r3 + r2, // près
        r3 - r2, // loin
    ]
}

/// Teste si l'AABB locale `[lmin, lmax]` (transformée par `model`) est au moins
/// partiellement dans le frustum. Conservateur : peut garder un objet juste hors champ.
fn aabb_visible(
    planes: &[glam::Vec4; 6],
    model: glam::Mat4,
    lmin: glam::Vec3,
    lmax: glam::Vec3,
) -> bool {
    // AABB monde à partir des 8 coins transformés.
    let mut wmin = glam::Vec3::splat(f32::INFINITY);
    let mut wmax = glam::Vec3::splat(f32::NEG_INFINITY);
    for sx in [lmin.x, lmax.x] {
        for sy in [lmin.y, lmax.y] {
            for sz in [lmin.z, lmax.z] {
                let p = (model * glam::Vec3::new(sx, sy, sz).extend(1.0)).truncate();
                wmin = wmin.min(p);
                wmax = wmax.max(p);
            }
        }
    }
    // Pour chaque plan, on teste le coin « positif » (le plus avancé vers le plan).
    for pl in planes {
        let n = pl.truncate();
        let positive = glam::Vec3::new(
            if n.x >= 0.0 { wmax.x } else { wmin.x },
            if n.y >= 0.0 { wmax.y } else { wmin.y },
            if n.z >= 0.0 { wmax.z } else { wmin.z },
        );
        if n.dot(positive) + pl.w < 0.0 {
            return false; // entièrement du mauvais côté d'un plan → hors champ
        }
    }
    true
}

/// Géométrie statique de la grille de référence (plan XZ, -10..10).
/// Axes X (rougeâtre) et Z (bleuté) accentués, lignes secondaires grises.
fn build_grid_verts() -> Vec<GizmoVertex> {
    const N: i32 = 10;
    let mut v = Vec::new();
    for i in -N..=N {
        let f = i as f32;
        let cx = if i == 0 {
            [0.6, 0.3, 0.3]
        } else {
            [0.26, 0.26, 0.3]
        };
        let cz = if i == 0 {
            [0.3, 0.3, 0.6]
        } else {
            [0.26, 0.26, 0.3]
        };
        v.push(GizmoVertex {
            position: [f, 0.0, -N as f32],
            color: cx,
        });
        v.push(GizmoVertex {
            position: [f, 0.0, N as f32],
            color: cx,
        });
        v.push(GizmoVertex {
            position: [-N as f32, 0.0, f],
            color: cz,
        });
        v.push(GizmoVertex {
            position: [N as f32, 0.0, f],
            color: cz,
        });
    }
    v
}

/// Clé d'ordonnancement stable d'un type de mesh (pour grouper les instances).
fn mesh_key(m: MeshKind) -> u32 {
    match m {
        MeshKind::Cube => 0,
        MeshKind::Sphere => 1,
        MeshKind::Plane => 2,
        MeshKind::Cylinder => 3,
        MeshKind::Capsule => 4,
        MeshKind::Terrain => 5,
        MeshKind::Imported(i) => 100 + i,
    }
}

/// Empreinte de **toutes** les entrées qui déterminent le buffer d'instances et le plan
/// de dessin : matrice caméra (frustum) + par objet (transform, couleur, matériau,
/// surbrillance, mesh, texture, visibilité). Sert au skip-rebuild : hash identique ⇒
/// sortie identique ⇒ rien à reconstruire. Capte tout changement → pas de frame périmée.
fn render_input_hash(app: &AppState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in app.camera.view_proj().to_cols_array() {
        h.write_u32(v.to_bits());
    }
    h.write_usize(app.scene.objects.len());
    for (i, o) in app.scene.objects.iter().enumerate() {
        let t = &o.transform;
        let floats = [
            t.position.x,
            t.position.y,
            t.position.z,
            t.rotation.x,
            t.rotation.y,
            t.rotation.z,
            t.rotation.w,
            t.scale.x,
            t.scale.y,
            t.scale.z,
            o.color[0],
            o.color[1],
            o.color[2],
            o.metallic,
            o.roughness,
            o.emissive,
            app.highlight_of(i),
        ];
        for v in floats {
            h.write_u32(v.to_bits());
        }
        o.mesh.hash(&mut h);
        h.write(o.texture.as_bytes());
        h.write_u8(o.visible as u8);
    }
    h.finish()
}

/// Crée le buffer storage d'instances + son bind group (groupe 1) pour `capacity` objets.
fn create_models_buffer(
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

fn create_uniform(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_depth_view(
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

/// Texture HDR intermédiaire (Sprint 90) : cible de la passe principale avant tone
/// mapping. `width`/`height` explicites plutôt qu'une `SurfaceConfiguration` : réutilisée
/// aussi bien par le chemin fenêtré (taille de la fenêtre) que par les rendus headless
/// (taille demandée par l'appelant, indépendante de toute fenêtre).
fn create_hdr_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
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

/// Chaîne de mips du bloom (Sprint 91) : une texture à `BLOOM_MIP_LEVELS` niveaux,
/// démarrant à moitié de la résolution HDR (`width`/`height` = celles de `hdr_view`) —
/// une vue par niveau (`base_mip_level` fixé, `mip_level_count: 1`), utilisable aussi
/// bien comme cible de rendu que comme texture échantillonnée (jamais les deux à la
/// fois dans la même passe).
fn create_bloom_mip_views(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Vec<wgpu::TextureView> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bloom_chain"),
        size: wgpu::Extent3d {
            width: (width / 2).max(2),
            height: (height / 2).max(2),
            depth_or_array_layers: 1,
        },
        mip_level_count: BLOOM_MIP_LEVELS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    (0..BLOOM_MIP_LEVELS)
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
