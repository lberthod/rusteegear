//! Couche rendu wgpu : surface, pipeline, depth buffer, dessin de plusieurs objets.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3, Vec4};
use winit::window::Window;

use super::camera::OrbitCamera;
use super::mesh::{GpuMesh, Vertex};
use crate::editor::Editor;
use crate::scene::{MeshKind, Scene};

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
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Ressources GPU par objet de la scène (une matrice modèle propre).
struct ObjectGpu {
    model_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct State {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    pipeline: wgpu::RenderPipeline,
    depth_view: wgpu::TextureView,
    model_layout: wgpu::BindGroupLayout,

    camera: OrbitCamera,
    camera_buf: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    meshes: HashMap<MeshKind, GpuMesh>,
    scene: Scene,
    objects_gpu: Vec<ObjectGpu>,
    selection: Option<usize>,

    editor: Editor,
    playing: bool,
    last_frame: Instant,

    // --- état d'interaction souris ---
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    press_cursor: Option<(f64, f64)>,
}

impl State {
    pub async fn new(window: Arc<Window>) -> State {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Aucun adaptateur GPU trouvé");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("Échec création du device");

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
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // --- Caméra (bind group 0) ---
        let camera = OrbitCamera::new(config.width as f32 / config.height as f32);
        let camera_buf = create_uniform(&device, "camera", std::mem::size_of::<CameraUniform>());
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera_layout"),
            entries: &[uniform_entry(0)],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // --- Layout des objets (bind group 1) ---
        let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model_layout"),
            entries: &[uniform_entry(0)],
        });

        // --- Pipeline ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("main_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/main.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&model_layout)],
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
                cull_mode: None, // robuste pour toutes les primitives générées
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

        // --- Meshes (un GpuMesh par type) ---
        let mut meshes = HashMap::new();
        for kind in MeshKind::ALL {
            meshes.insert(kind, GpuMesh::new(&device, &kind.mesh_data()));
        }

        // --- Scène + ressources GPU par objet ---
        let scene = Scene::demo();
        let objects_gpu = scene
            .objects
            .iter()
            .map(|_| {
                let model_buf =
                    create_uniform(&device, "model", std::mem::size_of::<ModelUniform>());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("model_bg"),
                    layout: &model_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: model_buf.as_entire_binding(),
                    }],
                });
                ObjectGpu { model_buf, bind_group }
            })
            .collect();

        let depth_view = create_depth_view(&device, &config);
        let editor = Editor::new(&device, config.format, &window);

        State {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            depth_view,
            model_layout,
            camera,
            camera_buf,
            camera_bind_group,
            meshes,
            scene,
            objects_gpu,
            selection: None,
            editor,
            playing: false,
            last_frame: Instant::now(),
            dragging: false,
            last_cursor: None,
            press_cursor: None,
        }
    }

    /// Ajuste le nombre de ressources GPU par objet pour qu'il corresponde à la scène.
    fn sync_objects(&mut self) {
        let n = self.scene.objects.len();
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
            self.objects_gpu.push(ObjectGpu { model_buf, bind_group });
        }
        self.objects_gpu.truncate(n);
    }

    /// Transmet l'événement à l'UI. Retourne `true` s'il a été consommé par egui.
    pub fn on_ui_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        self.editor.on_window_event(&self.window, event)
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
        self.camera.aspect = new_size.width as f32 / new_size.height as f32;
    }

    // --- interaction souris ---

    pub fn on_mouse_button(&mut self, pressed: bool) {
        if pressed {
            self.dragging = true;
            self.press_cursor = self.last_cursor;
        } else {
            self.dragging = false;
            // clic sans déplacement notable = sélection par picking
            if let (Some((px, py)), Some((cx, cy))) = (self.press_cursor, self.last_cursor) {
                if (px - cx).hypot(py - cy) < 4.0 {
                    self.selection = self.pick(cx, cy);
                }
            }
            self.press_cursor = None;
        }
    }

    /// Lance un rayon depuis le curseur et renvoie l'objet le plus proche touché.
    fn pick(&self, px: f64, py: f64) -> Option<usize> {
        let w = self.config.width as f32;
        let h = self.config.height as f32;
        let ndc_x = 2.0 * px as f32 / w - 1.0;
        let ndc_y = 1.0 - 2.0 * py as f32 / h;

        let inv = self.camera.view_proj().inverse();
        let near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let origin = near.truncate() / near.w;
        let dir = (far.truncate() / far.w - origin).normalize();

        let mut best: Option<(f32, usize)> = None;
        for (i, obj) in self.scene.objects.iter().enumerate() {
            let (lmin, lmax) = obj.mesh.local_aabb();
            let m = obj.transform.matrix();
            // AABB monde via les 8 coins transformés
            let mut wmin = Vec3::splat(f32::INFINITY);
            let mut wmax = Vec3::splat(f32::NEG_INFINITY);
            for sx in [lmin.x, lmax.x] {
                for sy in [lmin.y, lmax.y] {
                    for sz in [lmin.z, lmax.z] {
                        let p = (m * Vec3::new(sx, sy, sz).extend(1.0)).truncate();
                        wmin = wmin.min(p);
                        wmax = wmax.max(p);
                    }
                }
            }
            if let Some(t) = ray_aabb(origin, dir, wmin, wmax) {
                if best.map_or(true, |(bt, _)| t < bt) {
                    best = Some((t, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Applique les comportements du mode Play (rotation simple) selon le delta-time.
    fn advance_play(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        if self.playing {
            for obj in &mut self.scene.objects {
                if obj.mesh != MeshKind::Plane {
                    obj.transform.rotation =
                        Quat::from_rotation_y(dt * 1.2) * obj.transform.rotation;
                }
            }
        }
    }

    pub fn on_cursor_moved(&mut self, x: f64, y: f64) {
        if self.dragging {
            if let Some((lx, ly)) = self.last_cursor {
                let dx = (x - lx) as f32;
                let dy = (y - ly) as f32;
                self.camera.yaw -= dx * 0.005;
                self.camera.pitch += dy * 0.005;
            }
        }
        self.last_cursor = Some((x, y));
    }

    pub fn on_scroll(&mut self, delta: f32) {
        self.camera.distance = (self.camera.distance - delta * 0.5).clamp(1.5, 50.0);
    }

    pub fn update(&mut self) {
        let camera_uniform = CameraUniform {
            view_proj: self.camera.view_proj().to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera_uniform));

        for (i, (obj, gpu)) in self.scene.objects.iter().zip(&self.objects_gpu).enumerate() {
            let model = obj.transform.matrix();
            let highlight = if self.selection == Some(i) { 1.0 } else { 0.0 };
            let model_uniform = ModelUniform {
                model: model.to_cols_array_2d(),
                normal: model.inverse().transpose().to_cols_array_2d(),
                params: [highlight, 0.0, 0.0, 0.0],
            };
            self.queue
                .write_buffer(&gpu.model_buf, 0, bytemuck::bytes_of(&model_uniform));
        }
    }

    pub fn render(&mut self) {
        // 0. Acquérir la surface EN PREMIER. Si elle n'est pas disponible, on sort
        //    avant de lancer egui : sinon on jetterait le `textures_delta` de la frame
        //    (atlas de police), ce qui désynchronise le renderer egui (panic).
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

        // 1. Construire l'UI (peut modifier la scène / la sélection).
        let (full_output, actions) = self.editor.run(
            &self.window,
            &mut self.scene,
            &mut self.selection,
            &mut self.playing,
        );
        if actions.save {
            let path = scene_path();
            match self.scene.save(&path) {
                Ok(()) => log::info!("Scène sauvegardée dans {path}"),
                Err(e) => log::error!("Échec sauvegarde : {e}"),
            }
        }
        if actions.load {
            match Scene::load(&scene_path()) {
                Ok(s) => {
                    self.scene = s;
                    self.selection = None;
                }
                Err(e) => log::error!("Échec chargement : {e}"),
            }
        }
        // 2. Comportements (Play), sync GPU, puis push des uniforms.
        self.advance_play();
        self.sync_objects();
        self.update();

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("encoder") });

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

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            for (obj, gpu) in self.scene.objects.iter().zip(&self.objects_gpu) {
                let mesh = &self.meshes[&obj.mesh];
                pass.set_bind_group(1, &gpu.bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }
        }

        // 3. Peindre l'UI egui par-dessus la scène.
        let extra = self.editor.paint(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            [self.config.width, self.config.height],
            full_output,
        );

        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        frame.present();
    }
}

/// Chemin du fichier de scène, dans le dossier personnel (cwd vaut "/" en mode .app).
fn scene_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{home}/motor3derust_scene.json")
}

/// Intersection rayon / AABB (méthode des slabs). Renvoie le t d'entrée si touché devant.
fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let o = origin.to_array();
    let d = dir.to_array();
    let mn = min.to_array();
    let mx = max.to_array();
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    for i in 0..3 {
        if d[i].abs() < 1e-8 {
            if o[i] < mn[i] || o[i] > mx[i] {
                return None;
            }
        } else {
            let t1 = (mn[i] - o[i]) / d[i];
            let t2 = (mx[i] - o[i]) / d[i];
            let (t1, t2) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
        }
    }
    if tmax >= tmin && tmax >= 0.0 {
        Some(tmin.max(0.0))
    } else {
        None
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
