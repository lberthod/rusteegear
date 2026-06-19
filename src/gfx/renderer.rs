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
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ModelUniform {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    params: [f32; 4], // x = surbrillance (sélection)
    color: [f32; 4],  // teinte (albédo) de l'objet
}

/// Éclairage de la scène (groupe 0, binding 1).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SceneUniform {
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4], // x = intensité ambiante
    light_vp: [[f32; 4]; 4],
}

const SHADOW_SIZE: u32 = 1024;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Ressources GPU par objet de la scène (une matrice modèle propre).
struct ObjectGpu {
    model_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Dernier état téléversé : évite de réécrire le buffer d'un objet immobile.
    last_model: Option<glam::Mat4>,
    last_highlight: f32,
    last_color: [f32; 3],
}

pub struct Renderer {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    pipeline: wgpu::RenderPipeline,
    depth_view: wgpu::TextureView,
    model_layout: wgpu::BindGroupLayout,

    camera_buf: wgpu::Buffer,
    light_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    meshes: HashMap<MeshKind, GpuMesh>,
    imported_gpu: Vec<GpuMesh>,
    objects_gpu: Vec<ObjectGpu>,

    gizmo_pipeline: wgpu::RenderPipeline,
    gizmo_vbuf: wgpu::Buffer,

    // --- ombres (shadow mapping) ---
    shadow_view: wgpu::TextureView,
    shadow_bind_group: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,

    // --- textures (groupe 3) ---
    tex_layout: wgpu::BindGroupLayout,
    tex_sampler: wgpu::Sampler,
    /// Bind groups de texture par chemin ; "" = texture blanche par défaut.
    textures: HashMap<String, wgpu::BindGroup>,

    editor: Editor,
    /// Nom du backend GPU réel (Metal / Vulkan / …), pour le bandeau d'état.
    backend: String,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Renderer, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("Création de la surface impossible : {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
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

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Fifo (vsync) : cale le rendu sur l'écran, fluide et peu gourmand.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

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

        // --- Layout des objets (bind group 1) ---
        let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model_layout"),
            entries: &[uniform_entry(0)],
        });

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
                    format: config.format,
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
        // capacité : 3 axes × RING_SEGMENTS segments × 2 sommets (anneaux de rotation).
        let gizmo_capacity = 3 * RING_SEGMENTS * 2;
        let gizmo_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gizmo_vbuf"),
            size: (gizmo_capacity * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Meshes (un GpuMesh par type) ---
        let mut meshes = HashMap::new();
        for kind in MeshKind::ALL {
            meshes.insert(kind, GpuMesh::new(&device, &kind.mesh_data()));
        }

        let depth_view = create_depth_view(&device, &config);
        let editor = Editor::new(&device, config.format, &window);

        Ok(Renderer {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            depth_view,
            model_layout,
            camera_buf,
            light_buf,
            camera_bind_group,
            meshes,
            imported_gpu: Vec::new(),
            objects_gpu: Vec::new(),
            gizmo_pipeline,
            gizmo_vbuf,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            tex_layout,
            tex_sampler,
            textures,
            editor,
            backend,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config);
    }

    /// Transmet l'événement à l'UI. Retourne `true` s'il a été consommé par egui.
    pub fn on_ui_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        self.editor.on_window_event(&self.window, event)
    }

    /// Ajuste le nombre de ressources GPU par objet pour qu'il corresponde à la scène.
    fn sync_objects(&mut self, scene: &Scene) {
        let n = scene.objects.len();
        while self.objects_gpu.len() < n {
            let model_buf =
                create_uniform(&self.device, "model", std::mem::size_of::<ModelUniform>());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("model_bg"),
                layout: &self.model_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: model_buf.as_entire_binding(),
                }],
            });
            self.objects_gpu.push(ObjectGpu {
                model_buf,
                bind_group,
                last_model: None,
                last_highlight: -1.0,
                last_color: [-1.0, -1.0, -1.0],
            });
        }
        self.objects_gpu.truncate(n);
    }

    /// Construit les `GpuMesh` des modèles importés pas encore chargés sur GPU.
    fn sync_imported(&mut self, scene: &Scene) {
        while self.imported_gpu.len() < scene.imported.len() {
            let data = &scene.imported[self.imported_gpu.len()].data;
            self.imported_gpu.push(GpuMesh::new(&self.device, data));
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
        let camera_uniform = CameraUniform {
            view_proj: app.camera.view_proj().to_cols_array_2d(),
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
        let scene_uniform = SceneUniform {
            light_dir: [l.dir[0], l.dir[1], l.dir[2], 0.0],
            light_color: [l.color[0], l.color[1], l.color[2], 0.0],
            ambient: [l.ambient, 0.0, 0.0, 0.0],
            light_vp: light_vp.to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.light_buf, 0, bytemuck::bytes_of(&scene_uniform));

        for (i, (obj, gpu)) in app
            .scene
            .objects
            .iter()
            .zip(&mut self.objects_gpu)
            .enumerate()
        {
            let model = obj.transform.matrix();
            let highlight = app.highlight_of(i);
            // Rien n'a changé depuis la dernière frame : on saute l'upload.
            if gpu.last_model == Some(model)
                && gpu.last_highlight == highlight
                && gpu.last_color == obj.color
            {
                continue;
            }
            // Matrice normale = inverse-transposée du bloc 3×3 (Mat3 bien moins
            // coûteux qu'une inversion de Mat4, et correct même en scale non uniforme).
            let normal3 = glam::Mat3::from_mat4(model).inverse().transpose();
            let model_uniform = ModelUniform {
                model: model.to_cols_array_2d(),
                normal: glam::Mat4::from_mat3(normal3).to_cols_array_2d(),
                params: [highlight, 0.0, 0.0, 0.0],
                color: [obj.color[0], obj.color[1], obj.color[2], 1.0],
            };
            self.queue
                .write_buffer(&gpu.model_buf, 0, bytemuck::bytes_of(&model_uniform));
            gpu.last_model = Some(model);
            gpu.last_highlight = highlight;
            gpu.last_color = obj.color;
        }
    }

    pub fn render(&mut self, app: &mut AppState) {
        // 0. Acquérir la surface EN PREMIER. Si indisponible, on sort avant de lancer
        //    egui : sinon on jetterait le `textures_delta` de la frame (atlas de police),
        //    ce qui désynchronise le renderer egui (panic).
        use wgpu::CurrentSurfaceTexture as C;
        let frame = match self.surface.get_current_texture() {
            C::Success(t) | C::Suboptimal(t) => t,
            C::Outdated | C::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            C::Timeout | C::Occluded => return,
            C::Validation => {
                log::error!("surface validation error");
                return;
            }
        };

        // 1. Construire l'UI éditeur. En mode player : pas de panneaux, mais on
        //    dessine quand même les contrôles tactiles (joystick + boutons).
        let full_output = if app.player {
            if app.scene.mobile.any() {
                Some(self.editor.run_player_overlay(
                    &self.window,
                    &app.scene,
                    &mut app.input_state,
                    app.device_preview,
                    app.device_portrait,
                ))
            } else {
                None
            }
        } else {
            let status = crate::editor::StatusInfo {
                fps: app.fps(),
                backend: &self.backend,
                ai_busy: app.ai_busy,
            };
            let (full_output, actions) = self.editor.run(
                &self.window,
                &mut app.scene,
                &mut app.selection,
                &mut app.selected,
                &mut app.playing,
                &mut app.paused,
                &mut app.gizmo_mode,
                &mut app.input_state,
                &mut app.device_preview,
                &mut app.device_portrait,
                &mut app.view_rect_px,
                status,
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
            if let Some(clip) = actions.play_audio {
                app.play_audio(&clip);
            }
            if let Some(down) = actions.move_in_list {
                app.move_selected_in_list(down);
            }
            if let Some((idx, req)) = actions.ai_generate {
                app.request_ai_script(idx, req);
            }
            Some(full_output)
        };

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

        // Préparer le gizmo de l'objet sélectionné (jamais en mode player ni en aperçu mobile).
        let gizmo_count = if app.player || app.device_preview {
            0
        } else if let Some(sel) = app.selection {
            let o = app.scene.objects[sel].transform.position;
            let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
            let mut verts: Vec<GizmoVertex> = Vec::new();
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
            self.queue
                .write_buffer(&self.gizmo_vbuf, 0, bytemuck::cast_slice(&verts));
            verts.len() as u32
        } else {
            0
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
            for (obj, gpu) in app.scene.objects.iter().zip(&self.objects_gpu) {
                let mesh = match obj.mesh {
                    MeshKind::Imported(i) => match self.imported_gpu.get(i as usize) {
                        Some(m) => m,
                        None => continue,
                    },
                    k => &self.meshes[&k],
                };
                spass.set_bind_group(1, &gpu.bind_group, &[]);
                spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                spass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);

            for (obj, gpu) in app.scene.objects.iter().zip(&self.objects_gpu) {
                let mesh = match obj.mesh {
                    MeshKind::Imported(i) => match self.imported_gpu.get(i as usize) {
                        Some(m) => m,
                        None => continue, // mesh importé pas encore (ou plus) chargé
                    },
                    k => &self.meshes[&k],
                };
                let tex = self
                    .textures
                    .get(&obj.texture)
                    .unwrap_or_else(|| &self.textures[""]);
                pass.set_bind_group(1, &gpu.bind_group, &[]);
                pass.set_bind_group(3, tex, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }

            // Gizmo de l'objet sélectionné, par-dessus.
            if gizmo_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.gizmo_vbuf.slice(..));
                pass.draw(0..gizmo_count, 0..1);
            }
        }

        // 3. Peindre l'UI egui par-dessus la scène (sauf en mode player).
        let extra = match full_output {
            Some(output) => self.editor.paint(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                [self.config.width, self.config.height],
                output,
            ),
            None => Vec::new(),
        };

        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        frame.present();
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
    let bytes: Vec<u8> = if let Some(key) = crate::assets::strip_scheme(path) {
        crate::assets::bundle_bytes(key)?.to_vec()
    } else {
        std::fs::read(path).ok()?
    };
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
