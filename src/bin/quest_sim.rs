//! Simulateur de Meta Quest 3 sur desktop — `cargo run --release --bin quest_sim`.
//!
//! Rend la scène VR **en stéréo, à la résolution réelle par œil du casque**
//! (2064×2208), avec les FOV asymétriques et l'écart interpupillaire du
//! profil (`xr::sim::QuestProfile`), exactement comme l'APK le fait dans la
//! swapchain OpenXR ; puis affiche les deux yeux côte à côte. Même code de
//! projection (`xr::math::EyeView`) et de scène (`xr::test_scene`) que l'APK :
//! on développe et on vérifie les phases 1 à 6 de la roadmap VR ici, le casque
//! ne sert plus qu'aux tests de validation.
//!
//! `--bench <secondes>` : banc d'essai hors écran, sans vsync (débit, CPU,
//! draw calls). `QUEST_SIM_SCALE=0.7` : résolution de rendu par œil × 0,7 ;
//! `QUEST_SIM_DRAW_DISTANCE=1` : distances d'affichage du jeu desktop au lieu
//! du profil VR (mesures de la phase 2).
//!
//! Scène : `--scene menu` (défaut : sélecteur de niveaux, Rivière en fond),
//! `riviere`, `hameau`, `herroad`, `ragequit`, `reeduc`, `embedded` ou `cubes`
//! (scène de test de la phase 0).
//!
//! Commandes : clic gauche glissé = tourner la tête · flèches = marcher dans la
//! pièce · Page↑ / Page↓ = se lever / s'accroupir · R = recentrer · 2 = profil
//! Quest 2 / 3 · Échap = quitter. Manettes Touch simulées (ZQSD/WASD = stick
//! gauche, Espace = A, F ou clic droit = gâchette…) : cf. `Sim::sim_input`.
//! `QUEST_SIM_HANDS=1` : suivi des mains à la place des manettes (P / O =
//! pincement / poing droits, L / M = gauches), pour la rééducation
//! (`--scene reeduc`).
//! Titre de la fenêtre : temps CPU+GPU par image comparé au budget du casque —
//! indicatif seulement (GPU du Mac ≠ Adreno du Quest).

use std::collections::HashSet;
use std::sync::Arc;

use motor3derust::time_compat::Instant;
use motor3derust::xr::content::{SceneChoice, XrContent};
use motor3derust::xr::input::{HandInput, LEFT, RIGHT, XrInput};
use motor3derust::xr::sim::{QuestProfile, SimHead};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Format des « swapchains » simulées : celui que l'APK choisit en priorité.
const EYE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Sensibilité du regard à la souris (rad / pixel).
const LOOK_SENSITIVITY: f32 = 0.004;

const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var eye: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    // Triangle plein écran (3 sommets, pas de tampon).
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: Out;
    out.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(eye, samp, in.uv);
}
"#;

/// Échelle de la résolution de rendu par œil (`QUEST_SIM_SCALE=0.7`, défaut 1) :
/// mesure ce que rapporte un rendu sous la résolution recommandée (phase 2).
fn render_scale() -> f32 {
    std::env::var("QUEST_SIM_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0)
}

/// Features du device VR : `MULTIVIEW` si l'adaptateur l'expose (les deux
/// yeux en une passe, cf. `gfx::multiview`), sauf `QUEST_SIM_MULTIVIEW=0`
/// (comparaison avec le rendu œil par œil).
fn vr_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let off = std::env::var("QUEST_SIM_MULTIVIEW").is_ok_and(|v| v == "0");
    if off {
        wgpu::Features::empty()
    } else {
        adapter.features() & wgpu::Features::MULTIVIEW
    }
}

/// Scène depuis la ligne de commande : `--scene cubes|riviere` (défaut Rivière).
fn scene_from_args(args: &[String]) -> SceneChoice {
    args.iter()
        .position(|a| a == "--scene")
        .and_then(|i| args.get(i + 1))
        .map_or(SceneChoice::Launcher, |s| SceneChoice::parse(s))
}

/// Les deux « swapchains » d'œil simulées (une texture à 2 couches, comme
/// celle du runtime XR) et ce qu'il faut pour les recopier dans la fenêtre.
struct Eyes {
    profile: QuestProfile,
    views: [wgpu::TextureView; 2],
    blit_groups: [wgpu::BindGroup; 2],
}

impl Eyes {
    fn new(
        device: &wgpu::Device,
        profile: QuestProfile,
        blit_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("quest_sim_eyes"),
            size: wgpu::Extent3d {
                width: profile.eye_width,
                height: profile.eye_height,
                depth_or_array_layers: 2,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: EYE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let views = [0, 1].map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("quest_sim_eye"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        let blit_groups = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("quest_sim_blit"),
                layout: blit_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&views[i]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        });
        Self {
            profile,
            views,
            blit_groups,
        }
    }
}

struct Gpu {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blit_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    eyes: Eyes,
    adapter: wgpu::Adapter,
    choice: SceneChoice,
    content: XrContent,
}

impl Gpu {
    async fn new(
        window: Arc<Window>,
        profile: QuestProfile,
        choice: SceneChoice,
    ) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("surface : {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("adaptateur GPU : {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("quest_sim"),
                required_features: vr_features(&adapter),
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("device : {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        // Surface sRGB : les yeux sont en sRGB, l'échantillonnage les rend
        // linéaires, l'écriture les ré-encode — couleurs identiques au casque.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or("aucun format de surface")?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Pas de vsync : le titre mesure alors le vrai coût d'une image.
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or_default(),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quest_sim_blit"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quest_sim_blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quest_sim_blit"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quest_sim_blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("quest_sim_blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let eyes = Eyes::new(&device, profile, &blit_layout, &sampler);
        let mut content = XrContent::new(
            choice,
            &adapter,
            &device,
            &queue,
            EYE_FORMAT,
            profile.eye_width,
            profile.eye_height,
        );
        // `QUEST_SIM_DRAW_DISTANCE=1` : distances d'affichage du jeu desktop
        // (le profil VR les réduit, cf. `VR_DRAW_DISTANCE_SCALE`).
        if let (XrContent::Game(game), Some(scale)) = (
            &mut content,
            std::env::var("QUEST_SIM_DRAW_DISTANCE")
                .ok()
                .and_then(|v| v.parse::<f32>().ok()),
        ) {
            game.renderer.set_draw_distance_scale(scale);
        }
        Ok(Self {
            window,
            device,
            queue,
            surface,
            config,
            blit_pipeline,
            blit_layout,
            sampler,
            eyes,
            adapter,
            choice,
            content,
        })
    }

    fn set_profile(&mut self, profile: QuestProfile) {
        self.eyes = Eyes::new(&self.device, profile, &self.blit_layout, &self.sampler);
        // La scène de cubes a une profondeur à la taille de l'œil ; le renderer
        // du jeu, lui, suit la taille à chaque `render_views`.
        if self.choice == SceneChoice::Cubes {
            self.content = XrContent::new(
                self.choice,
                &self.adapter,
                &self.device,
                &self.queue,
                EYE_FORMAT,
                profile.eye_width,
                profile.eye_height,
            );
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Rend les deux yeux puis les recopie côte à côte, proportions conservées.
    /// `None` si rien n'a été rendu (fenêtre masquée, surface à refaire),
    /// sinon `Some(quitter)` — `true` si le menu VR a demandé à quitter.
    fn render(&mut self, head: &SimHead, input: &XrInput) -> Option<bool> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return None;
            }
            _ => return None,
        };
        let p = self.eyes.profile;
        let out = self.content.render(
            &self.device,
            &self.queue,
            head.eye_views(&p),
            input,
            [&self.eyes.views[0], &self.eyes.views[1]],
            p.eye_width,
            p.eye_height,
        );
        // Pas de moteur de vibration sur un Mac : on le dit.
        for (hand, h) in ["gauche", "droite"].iter().zip(out.haptics) {
            if let Some(h) = h {
                log::info!(
                    "Vibration manette {hand} : {:.0} % pendant {:.0} ms",
                    h.amplitude * 100.0,
                    h.seconds * 1000.0
                );
            }
        }

        let target = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("quest_sim_blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
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
            pass.set_pipeline(&self.blit_pipeline);
            let (w, h) = (self.config.width as f32, self.config.height as f32);
            let p = &self.eyes.profile;
            let rects = side_by_side(w, h, p.eye_width as f32 / p.eye_height as f32);
            for (group, (x, y, vw, vh)) in self.eyes.blit_groups.iter().zip(rects) {
                pass.set_viewport(x, y, vw, vh, 0.0, 1.0);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
        Some(out.quit)
    }
}

/// Deux rectangles (x, y, largeur, hauteur) côte à côte, chacun au format
/// `eye_aspect` (largeur / hauteur) et centré dans sa moitié de fenêtre.
fn side_by_side(w: f32, h: f32, eye_aspect: f32) -> [(f32, f32, f32, f32); 2] {
    let half = w / 2.0;
    let (vw, vh) = if half / h > eye_aspect {
        (h * eye_aspect, h)
    } else {
        (half, half / eye_aspect)
    };
    let y = (h - vh) / 2.0;
    [0.0, 1.0].map(|i| (i * half + (half - vw) / 2.0, y, vw, vh))
}

#[derive(Default)]
struct Sim {
    gpu: Option<Gpu>,
    head: SimHead,
    keys: HashSet<KeyCode>,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    last_frame: Option<Instant>,
    /// Somme des durées d'image et nombre d'images depuis la dernière mise à
    /// jour du titre (moyenne sur ~0,5 s, lisible).
    acc_ms: f32,
    acc_frames: u32,
    quest2: bool,
    choice: Option<SceneChoice>,
    title_updates: u32,
    right_trigger_mouse: bool,
}

impl Sim {
    fn profile(&self) -> QuestProfile {
        let p = if self.quest2 {
            QuestProfile::QUEST2
        } else {
            QuestProfile::QUEST3
        };
        p.scaled(render_scale())
    }

    /// Manettes Touch simulées (phase 3) : poses devant le corps
    /// (`SimHead::controller_pose`), boutons et sticks au clavier :
    ///
    /// | Touche | Manette |
    /// |---|---|
    /// | ZQSD / WASD | stick gauche (déplacement du personnage) |
    /// | A / E (Q / E en QWERTY) | stick droit ← → (rotation par crans) |
    /// | Espace | A (saut) · Maj : B (ruée) |
    /// | F ou clic droit | gâchette droite (attaque) |
    /// | T | gâchette gauche (boule de feu) · G : grip gauche (bouclier) |
    /// | H | X (soin) · Y : Y (arme) |
    /// | V | clic du stick droit (vue 1re personne ⇄ spectateur) |
    /// | Tab | menu |
    fn sim_input(&self) -> XrInput {
        input_from_keys(&self.keys, &self.head, self.right_trigger_mouse)
    }

    fn axis(&self, pos: &[KeyCode], neg: &[KeyCode]) -> f32 {
        let held = |ks: &[KeyCode]| ks.iter().any(|k| self.keys.contains(k));
        (held(pos) as i32 - held(neg) as i32) as f32
    }
}

impl ApplicationHandler for Sim {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("RusteeGear — simulateur Meta Quest 3")
            .with_inner_size(winit::dpi::LogicalSize::new(1400.0, 760.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("fenêtre : {e}");
                event_loop.exit();
                return;
            }
        };
        let choice = self.choice.unwrap_or(SceneChoice::Riviere);
        match pollster::block_on(Gpu::new(window, self.profile(), choice)) {
            Ok(gpu) => {
                log::info!(
                    "Simulateur {} : {}×{} px par œil, budget {:.1} ms",
                    gpu.eyes.profile.name,
                    gpu.eyes.profile.eye_width,
                    gpu.eyes.profile.eye_height,
                    gpu.eyes.profile.frame_budget_ms()
                );
                gpu.window.request_redraw();
                self.gpu = Some(gpu);
            }
            Err(e) => {
                log::error!("GPU : {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                if event.state == ElementState::Pressed {
                    match code {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::KeyR => self.head = SimHead::default(),
                        KeyCode::Digit2 if !event.repeat => {
                            self.quest2 = !self.quest2;
                            let profile = self.profile();
                            if let Some(gpu) = self.gpu.as_mut() {
                                gpu.set_profile(profile);
                                gpu.window.set_title(&format!(
                                    "RusteeGear — simulateur {}",
                                    profile.name
                                ));
                            }
                        }
                        _ => {}
                    }
                    self.keys.insert(code);
                } else {
                    self.keys.remove(&code);
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                // Clic droit = gâchette droite (attaque), comme F.
                self.right_trigger_mouse = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (true, Some((x, y))) = (self.dragging, self.last_cursor) {
                    // Glisser vers la droite = tourner la tête à droite (lacet −).
                    self.head.look(
                        -((position.x - x) as f32) * LOOK_SENSITIVITY,
                        -((position.y - y) as f32) * LOOK_SENSITIVITY,
                    );
                }
                self.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = self
                    .last_frame
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .unwrap_or(0.0)
                    .min(0.1);
                self.last_frame = Some(now);
                // ZQSD (AZERTY) et WASD (QWERTY) : codes physiques, donc les
                // deux dispositions tombent sur les mêmes touches.
                // Marche **physique** dans la pièce (flèches) et hauteur de la
                // tête (Page↑/Page↓) — le déplacement du personnage, lui, est au
                // stick gauche simulé (ZQSD/WASD), cf. `sim_input`.
                let forward = self.axis(&[KeyCode::ArrowUp], &[KeyCode::ArrowDown]);
                let strafe = self.axis(&[KeyCode::ArrowRight], &[KeyCode::ArrowLeft]);
                let rise = self.axis(&[KeyCode::PageUp], &[KeyCode::PageDown]);
                self.head.walk(forward, strafe, rise, dt);
                let input = self.sim_input();

                let Some(gpu) = self.gpu.as_mut() else {
                    return;
                };
                match gpu.render(&self.head, &input) {
                    Some(true) => {
                        event_loop.exit();
                        return;
                    }
                    Some(false) => {}
                    None => {
                        // Fenêtre masquée : rien de rendu, donc rien à mesurer —
                        // et une courte pause plutôt que de tourner à vide.
                        std::thread::sleep(std::time::Duration::from_millis(16));
                        return;
                    }
                }
                // Pas d'attente du GPU : comme dans un casque, CPU (image N+1) et
                // GPU (image N) travaillent en parallèle, `get_current_texture`
                // freine quand le GPU prend du retard. Le temps par image est donc
                // l'intervalle entre deux images (débit réel). Mesure CPU/GPU
                // précise, sans vsync : `--bench`.
                self.acc_ms += dt * 1000.0;
                self.acc_frames += 1;
                if self.acc_ms >= 500.0 {
                    let avg = self.acc_ms / self.acc_frames as f32;
                    let p = gpu.eyes.profile;
                    gpu.window.set_title(&format!(
                        "RusteeGear — simulateur {} · {avg:.2} ms/image (budget {:.1} ms à {} Hz, GPU du Mac) · tête ({:.2}, {:.2}, {:.2})",
                        p.name,
                        p.frame_budget_ms(),
                        p.refresh_hz,
                        self.head.position.x,
                        self.head.position.y,
                        self.head.position.z,
                    ));
                    // Aussi dans le journal toutes les ~5 s : lisible par un script
                    // ou un agent, qui ne voit pas le titre de la fenêtre.
                    self.title_updates += 1;
                    if self.title_updates % 10 == 0 {
                        let detail = match &gpu.content {
                            XrContent::Game(g) => format!(
                                " · dernière image : simulation {:.2} ms, rendu CPU {:.2} ms, scripts {:.2} ms, physique {:.2} ms",
                                g.last_sim_ms,
                                g.last_render_ms,
                                g.app.sim_perf_ms().0,
                                g.app.sim_perf_ms().1
                            ),
                            XrContent::Cubes(_) | XrContent::Balls(_) => String::new(),
                        };
                        log::info!(
                            "{avg:.2} ms/image ({:.0} img/s, 2 yeux) — budget {:.1} ms ({} Hz){detail}",
                            1000.0 / avg.max(0.01),
                            p.frame_budget_ms(),
                            p.refresh_hz
                        );
                    }
                    self.acc_ms = 0.0;
                    self.acc_frames = 0;
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }
}

/// Manettes simulées depuis les touches tenues (cf. la table de `Sim::sim_input`).
fn input_from_keys(keys: &HashSet<KeyCode>, head: &SimHead, right_mouse: bool) -> XrInput {
    let held = |k: KeyCode| keys.contains(&k);
    let axis = |pos: KeyCode, neg: KeyCode| (held(pos) as i32 - held(neg) as i32) as f32;
    let pose = |hand: usize| Some(head.controller_pose(hand));
    let left = HandInput {
        grip: pose(LEFT),
        aim: pose(LEFT),
        trigger: if held(KeyCode::KeyT) { 1.0 } else { 0.0 },
        squeeze: if held(KeyCode::KeyG) { 1.0 } else { 0.0 },
        stick: (
            axis(KeyCode::KeyD, KeyCode::KeyA),
            axis(KeyCode::KeyW, KeyCode::KeyS),
        ),
        stick_click: false,
        primary: held(KeyCode::KeyH),
        secondary: held(KeyCode::KeyY),
        menu: held(KeyCode::Tab),
    };
    let right = HandInput {
        grip: pose(RIGHT),
        aim: pose(RIGHT),
        trigger: if held(KeyCode::KeyF) || right_mouse {
            1.0
        } else {
            0.0
        },
        squeeze: 0.0,
        stick: (axis(KeyCode::KeyE, KeyCode::KeyQ), 0.0),
        stick_click: held(KeyCode::KeyV),
        primary: held(KeyCode::Space),
        secondary: held(KeyCode::ShiftLeft) || held(KeyCode::ShiftRight),
        menu: false,
    };
    // Suivi des mains simulé (`QUEST_SIM_HANDS=1`, phase 8) : plus de
    // manettes (ni boutons ni sticks), des mains synthétiques aux mêmes poses —
    // P / O = pincement / poing droits, L / M = pincement / poing gauches.
    if std::env::var("QUEST_SIM_HANDS").is_ok_and(|v| v == "1") {
        let hand = |i: usize, pinch: KeyCode, fist: KeyCode| {
            let (pos, rot) = head.controller_pose(i);
            let amount = |k: KeyCode| if held(k) { 1.0 } else { 0.0 };
            motor3derust::xr::hands::synthetic_hand(
                pos,
                rot,
                i == RIGHT,
                amount(pinch),
                amount(fist),
            )
        };
        return XrInput {
            hands: [
                HandInput {
                    menu: held(KeyCode::Tab),
                    ..Default::default()
                },
                HandInput::default(),
            ],
            hand_joints: [
                Some(hand(LEFT, KeyCode::KeyL, KeyCode::Semicolon)),
                Some(hand(RIGHT, KeyCode::KeyP, KeyCode::KeyO)),
            ],
        };
    }
    XrInput {
        hands: [left, right],
        hand_joints: [None, None],
    }
}

/// Touches tenues pendant une capture (`QUEST_SIM_HOLD=W,F`) : noms des
/// touches de la table de `Sim::sim_input` (lettres, `Space`, `Shift`, `Tab`).
fn held_keys_from_env() -> HashSet<KeyCode> {
    let Ok(list) = std::env::var("QUEST_SIM_HOLD") else {
        return HashSet::new();
    };
    list.split(',')
        .filter_map(|k| {
            Some(match k.trim().to_ascii_uppercase().as_str() {
                "W" => KeyCode::KeyW,
                "A" => KeyCode::KeyA,
                "S" => KeyCode::KeyS,
                "D" => KeyCode::KeyD,
                "Q" => KeyCode::KeyQ,
                "E" => KeyCode::KeyE,
                "F" => KeyCode::KeyF,
                "T" => KeyCode::KeyT,
                "G" => KeyCode::KeyG,
                "H" => KeyCode::KeyH,
                "Y" => KeyCode::KeyY,
                "V" => KeyCode::KeyV,
                "SPACE" => KeyCode::Space,
                "SHIFT" => KeyCode::ShiftLeft,
                "TAB" => KeyCode::Tab,
                "P" => KeyCode::KeyP,
                "O" => KeyCode::KeyO,
                "L" => KeyCode::KeyL,
                "M" => KeyCode::Semicolon,
                other => {
                    eprintln!("QUEST_SIM_HOLD : touche inconnue « {other} »");
                    return None;
                }
            })
        })
        .collect()
}

/// `--snapshot <fichier.png> [--quest2]` : une image stéréo rendue hors écran
/// (sans fenêtre), les deux yeux côte à côte réduits de moitié — vérification
/// scriptable (CI, agent) de ce que verrait le casque depuis la pose de départ.
async fn snapshot(path: &str, profile: QuestProfile, choice: SceneChoice) -> Result<(), String> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("adaptateur GPU : {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("quest_sim_snapshot"),
            required_features: vr_features(&adapter),
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("device : {e}"))?;
    let (w, h) = (profile.eye_width, profile.eye_height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("quest_sim_snapshot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: EYE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let views = [0, 1].map(|layer| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });
    let mut content = XrContent::new(
        choice,
        &adapter,
        &device,
        &queue,
        EYE_FORMAT,
        profile.eye_width,
        profile.eye_height,
    );
    // Une partie : ~1 s de jeu avant la capture (physique posée, créatures en
    // mouvement, rig placé sur le terrain), rendue à chaque pas comme en direct.
    // `QUEST_SIM_FRAMES=300` : jouer plus longtemps avant la capture ;
    // `QUEST_SIM_SWITCH=herroad` : changer de niveau (comme le menu « Choisir
    // un niveau ») à la première image.
    let frames: u32 = std::env::var("QUEST_SIM_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if choice == SceneChoice::Cubes { 1 } else { 60 });
    if let Ok(level) = std::env::var("QUEST_SIM_SWITCH") {
        content.load_level(SceneChoice::parse(&level));
    }
    // Touches tenues pendant la seconde de jeu (`QUEST_SIM_HOLD=W,F`) :
    // vérifie déplacement, rotation, actions sans fenêtre.
    let head = SimHead::default();
    let input = input_from_keys(&held_keys_from_env(), &head, false);
    for _ in 0..frames {
        content.render(
            &device,
            &queue,
            head.eye_views(&profile),
            &input,
            [&views[0], &views[1]],
            w,
            h,
        );
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    if let XrContent::Game(g) = &content
        && let (Some((pos, _, _, kmh)), Some(rig)) = (g.app.race_car_view(), g.rig)
    {
        println!(
            "Voiture en ({:.1}, {:.1}, {:.1}) à {kmh:.0} km/h · rig ({:.1}, {:.1}, {:.1}), lacet {:.0}°",
            pos.x,
            pos.y,
            pos.z,
            rig.origin.x,
            rig.origin.y,
            rig.origin.z,
            rig.yaw.to_degrees()
        );
    } else if let XrContent::Game(g) = &content
        && let (Some(i), Some(rig)) = (g.app.player_index(), g.rig)
    {
        let p = g.app.scene.objects[i].transform.position;
        println!(
            "Personnage en ({:.2}, {:.2}, {:.2}) · rig ({:.2}, {:.2}, {:.2}), lacet {:.0}° · vue {:?}",
            p.x,
            p.y,
            p.z,
            rig.origin.x,
            rig.origin.y,
            rig.origin.z,
            rig.yaw.to_degrees(),
            g.comfort.view
        );
    }

    // Relecture des deux couches (lignes alignées sur 256 octets, contrainte wgpu).
    let row = w * 4;
    let padded =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quest_sim_snapshot"),
        size: u64::from(padded) * u64::from(h) * 2,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 2,
        },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("GPU : {e}"))?;
    let data = buffer.slice(..).get_mapped_range();

    // Côte à côte, réduit de moitié (un pixel sur deux) : ~2064×1104.
    let (ow, oh) = (w, h / 2);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for eye in 0..2u32 {
        for y in 0..oh {
            for x in 0..w / 2 {
                let src = (eye * padded * h + (y * 2) * padded + (x * 2) * 4) as usize;
                let dst = ((y * ow + eye * (w / 2) + x) * 4) as usize;
                out[dst..dst + 4].copy_from_slice(&data[src..src + 4]);
            }
        }
    }
    image::save_buffer(path, &out, ow, oh, image::ColorType::Rgba8)
        .map_err(|e| format!("écriture de {path} : {e}"))
}

/// `--bench <secondes>` : banc d'essai hors écran (sans vsync), CPU et GPU en
/// parallèle avec une image d'avance au plus — comme un casque, où
/// `xrWaitFrame` laisse le CPU préparer l'image N+1 pendant que le GPU rend la
/// N. Affiche le débit (images/s), le temps CPU par image (simulation, rendu)
/// et le nombre de draw calls. Même scène, même pose que `--snapshot`.
async fn bench(seconds: f32, profile: QuestProfile, choice: SceneChoice) -> Result<(), String> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("adaptateur GPU : {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("quest_sim_bench"),
            required_features: vr_features(&adapter),
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("device : {e}"))?;
    let (w, h) = (profile.eye_width, profile.eye_height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("quest_sim_bench"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: EYE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let views = [0, 1].map(|layer| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });
    let mut content = XrContent::new(choice, &adapter, &device, &queue, EYE_FORMAT, w, h);
    if let (XrContent::Game(game), Some(scale)) = (
        &mut content,
        std::env::var("QUEST_SIM_DRAW_DISTANCE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok()),
    ) {
        game.renderer.set_draw_distance_scale(scale);
    }
    let eyes = SimHead::default().eye_views(&profile);
    // Échauffement : pipelines compilés, rig posé, caches remplis.
    for _ in 0..30 {
        content.render(
            &device,
            &queue,
            eyes,
            &XrInput::default(),
            [&views[0], &views[1]],
            w,
            h,
        );
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    }
    let started = Instant::now();
    let (mut frames, mut cpu_ms, mut sim_ms, mut render_ms) = (0u32, 0f32, 0f32, 0f32);
    let mut previous: Option<wgpu::SubmissionIndex> = None;
    while started.elapsed().as_secs_f32() < seconds {
        let t = Instant::now();
        content.render(
            &device,
            &queue,
            eyes,
            &XrInput::default(),
            [&views[0], &views[1]],
            w,
            h,
        );
        cpu_ms += t.elapsed().as_secs_f32() * 1000.0;
        if let XrContent::Game(g) = &content {
            sim_ms += g.last_sim_ms;
            render_ms += g.last_render_ms;
        }
        // Une image d'avance au plus : attend la fin GPU de l'image précédente.
        let current = queue.submit([]);
        if let Some(prev) = previous.replace(current) {
            let _ = device.poll(wgpu::PollType::Wait {
                submission_index: Some(prev),
                timeout: None,
            });
        }
        frames += 1;
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let total = started.elapsed().as_secs_f32() * 1000.0;
    let n = frames.max(1) as f32;
    let draw_calls = match &content {
        XrContent::Game(g) => g.renderer.gpu_profiler_info().1,
        XrContent::Cubes(_) => 12,
        // Balles & cubes : deux appels (cubes, sphères) par œil.
        XrContent::Balls(_) => 4,
    };
    println!(
        "{} {}×{}/œil : {:.2} ms/image ({:.0} img/s, budget {:.1} ms) · CPU {:.2} ms (simulation {:.2}, rendu {:.2}) · {} draw calls",
        profile.name,
        w,
        h,
        total / n,
        1000.0 * n / total,
        profile.frame_budget_ms(),
        cpu_ms / n,
        sim_ms / n,
        render_ms / n,
        draw_calls
    );
    Ok(())
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--bench") {
        let seconds = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(10.0);
        let profile = if args.iter().any(|a| a == "--quest2") {
            QuestProfile::QUEST2
        } else {
            QuestProfile::QUEST3
        }
        .scaled(render_scale());
        if let Err(e) = pollster::block_on(bench(seconds, profile, scene_from_args(&args))) {
            eprintln!("simulateur : {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        let Some(path) = args.get(i + 1) else {
            eprintln!(
                "usage : quest_sim --snapshot <fichier.png> [--quest2] [--scene riviere|cubes]"
            );
            std::process::exit(2);
        };
        let profile = if args.iter().any(|a| a == "--quest2") {
            QuestProfile::QUEST2
        } else {
            QuestProfile::QUEST3
        }
        .scaled(render_scale());
        match pollster::block_on(snapshot(path, profile, scene_from_args(&args))) {
            Ok(()) => println!("{path} ({} — les deux yeux côte à côte)", profile.name),
            Err(e) => {
                eprintln!("simulateur : {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("boucle d'événements : {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut sim = Sim {
        choice: Some(scene_from_args(&args)),
        ..Default::default()
    };
    if let Err(e) = event_loop.run_app(&mut sim) {
        eprintln!("simulateur : {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::side_by_side;

    #[test]
    fn eyes_fill_their_half_without_distortion() {
        let aspect = 2064.0 / 2208.0;
        for (w, h) in [(1400.0, 760.0), (800.0, 1200.0), (3000.0, 800.0)] {
            let [l, r] = side_by_side(w, h, aspect);
            for (x, y, vw, vh) in [l, r] {
                assert!((vw / vh - aspect).abs() < 1e-4);
                assert!(x >= 0.0 && y >= 0.0 && x + vw <= w + 1e-3 && y + vh <= h + 1e-3);
            }
            assert!(l.0 + l.2 <= r.0 + 1e-3, "l'œil gauche reste à gauche");
        }
    }
}
