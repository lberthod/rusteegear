//! **Ball — ordinateur** : rejoindre la partie du joueur VR (Meta Quest) par
//! le relais du VPS, se promener dans sa scène et poser des murs pour arrêter
//! ses tirs. `cargo run --release --bin ball_pc`.
//!
//! Commandes : ZQSD / WASD = se déplacer · clic droit maintenu (ou flèches)
//! = regarder · clic gauche ou Espace = poser un mur là où l'on vise (2 au
//! plus, 8 s chacun, 2 s de recharge) · Échap = quitter.
//!
//! Le casque est maître de la partie (physique, score) : ce programme affiche
//! ce qu'il lui décrit (`xr::ball::net::Snapshot`, interpolé pour rester
//! fluide) et lui renvoie sa position et ses murs. `BALL_URL` : autre relais
//! (ex. `ws://127.0.0.1:7790/ball` en local). `--snapshot <png>` : attend la
//! scène du casque, rend une image sans fenêtre et quitte (vérification).

use std::collections::HashSet;
use std::sync::Arc;

use glam::{EulerRot, Quat, Vec3, Vec4};
use motor3derust::time_compat::Instant;
use motor3derust::xr::ball::gfx::{Gfx, Instance};
use motor3derust::xr::ball::net::{Link, LinkStatus, Msg, NetBox, NetWall, PcState, Role, Snapshot};
use motor3derust::xr::ball::relay_url;
use motor3derust::xr::math::{EyeView, Fov};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const EYE_HEIGHT: f32 = 1.65;
const SPEED: f32 = 3.5;
const WALL_LIFETIME: f32 = 8.0;
const WALL_COOLDOWN: f32 = 2.0;
const MAX_WALLS: usize = 2;
/// Mur : mêmes dimensions que côté casque (`world::WALL_HALF`).
const WALL_HALF: Vec3 = Vec3::new(0.3, 0.8, 0.03);
const INPUT_PERIOD: f32 = 1.0 / 30.0;
/// Retard d'affichage pour interpoler entre deux états reçus (20 par s).
const INTERP_DELAY: f32 = 0.06;

/// État du joueur PC et de ce qu'il sait de la partie.
struct Player {
    link: Link,
    /// Deux derniers états reçus (précédent, courant) et quand.
    snaps: Option<(Snapshot, Snapshot, f32)>,
    statics: Vec<NetBox>,
    origin: Vec3,
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    /// Murs posés : (mur, instant de pose).
    walls: Vec<(NetWall, f32)>,
    last_wall: f32,
    clock: f32,
    next_send: f32,
    keys: HashSet<KeyCode>,
}

impl Player {
    fn new(url: &str) -> Self {
        Self {
            link: Link::start(url, Role::Pc),
            snaps: None,
            statics: Vec::new(),
            origin: Vec3::ZERO,
            // Entre les cibles et le joueur VR, tourné vers lui.
            pos: Vec3::new(1.2, 0.0, -1.3),
            yaw: 2.4,
            pitch: -0.15,
            walls: Vec::new(),
            last_wall: -10.0,
            clock: 0.0,
            next_send: 0.0,
            keys: HashSet::new(),
        }
    }

    fn update(&mut self, dt: f32) {
        self.clock += dt;
        for msg in self.link.poll() {
            if let Msg::Snapshot(mut s) = msg {
                if let Some(st) = s.statics.take() {
                    self.statics = st;
                }
                let origin = Vec3::from(s.origin);
                if origin != self.origin {
                    // Le joueur VR a changé de trou : on le suit, même place relative.
                    self.pos += origin - self.origin;
                    self.walls.clear();
                    self.origin = origin;
                    self.snaps = None;
                }
                self.snaps = Some(match self.snaps.take() {
                    Some((_, cur, _)) => (cur, s, self.clock),
                    None => (s.clone(), s, self.clock),
                });
            }
        }
        if self.link.status() != LinkStatus::Paired {
            self.snaps = None;
        }

        // Déplacements (au sol, dans la zone du trou, jamais sur le joueur VR).
        let held = |k: &[KeyCode]| k.iter().any(|k| self.keys.contains(k));
        let turn = held(&[KeyCode::ArrowLeft]) as i32 - held(&[KeyCode::ArrowRight]) as i32;
        self.yaw += turn as f32 * 1.8 * dt;
        let look = held(&[KeyCode::ArrowUp]) as i32 - held(&[KeyCode::ArrowDown]) as i32;
        self.pitch = (self.pitch + look as f32 * 1.2 * dt).clamp(-1.2, 1.2);
        let fwd = held(&[KeyCode::KeyW, KeyCode::KeyZ]) as i32 - held(&[KeyCode::KeyS]) as i32;
        let side = held(&[KeyCode::KeyD]) as i32 - held(&[KeyCode::KeyA, KeyCode::KeyQ]) as i32;
        let rot = Quat::from_rotation_y(self.yaw);
        let dir = rot * Vec3::new(side as f32, 0.0, -(fwd as f32));
        self.pos += dir.normalize_or_zero() * SPEED * dt;
        let o = self.origin;
        self.pos.x = self.pos.x.clamp(o.x - 4.0, o.x + 4.0);
        self.pos.z = self.pos.z.clamp(o.z - 7.0, o.z + 2.0);
        self.pos.y = o.y;
        let from_vr = glam::Vec2::new(self.pos.x - o.x, self.pos.z - o.z);
        if from_vr.length() < 0.7 {
            let push = from_vr.normalize_or(glam::Vec2::NEG_Y) * 0.7;
            self.pos.x = o.x + push.x;
            self.pos.z = o.z + push.y;
        }

        let now = self.clock;
        self.walls.retain(|(_, t)| now - t < WALL_LIFETIME);
        if held(&[KeyCode::Space]) {
            self.place_wall();
        }
        if self.clock >= self.next_send {
            self.next_send = self.clock + INPUT_PERIOD;
            self.link.send(&Msg::Pc(PcState {
                pos: self.pos.to_array(),
                yaw: self.yaw,
                walls: self.walls.iter().map(|(w, _)| *w).collect(),
            }));
        }
    }

    fn eye(&self) -> (Vec3, Quat) {
        (
            self.pos + Vec3::Y * EYE_HEIGHT,
            Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0),
        )
    }

    /// Où irait un mur maintenant : point du sol visé (entre 1 et 6 m).
    fn wall_preview(&self) -> Option<NetWall> {
        let (eye, rot) = self.eye();
        let dir = rot * Vec3::NEG_Z;
        let ground = self.origin.y;
        let t = if dir.y < -0.02 {
            ((ground - eye.y) / dir.y).clamp(1.0, 6.0)
        } else {
            3.0
        };
        let p = eye + dir * t;
        let o = self.origin;
        if glam::Vec2::new(p.x - o.x, p.z - o.z).length() < 0.9 {
            return None; // pas sur le joueur VR
        }
        Some(NetWall {
            x: p.x,
            z: p.z,
            yaw: self.yaw,
        })
    }

    fn place_wall(&mut self) {
        if self.clock - self.last_wall < WALL_COOLDOWN || self.snaps.is_none() {
            return;
        }
        let Some(w) = self.wall_preview() else {
            return;
        };
        if self.walls.len() >= MAX_WALLS {
            self.walls.remove(0);
        }
        self.walls.push((w, self.clock));
        self.last_wall = self.clock;
    }

    fn title(&self) -> String {
        let status = match self.link.status() {
            LinkStatus::Offline => "relais injoignable, nouvelle tentative…".to_string(),
            LinkStatus::Waiting => "en attente du joueur VR (lance Ball sur le casque)".to_string(),
            LinkStatus::Paired => match &self.snaps {
                Some((_, s, _)) => s.hud.join(" · "),
                None => "connecté, réception de la scène…".to_string(),
            },
        };
        let recharge = (WALL_COOLDOWN - (self.clock - self.last_wall)).max(0.0);
        let walls = if recharge > 0.0 {
            format!("murs {}/{MAX_WALLS} (recharge {recharge:.1} s)", self.walls.len())
        } else {
            format!("murs {}/{MAX_WALLS} — clic gauche pour poser", self.walls.len())
        };
        format!("Ball — ordinateur · {status} · {walls}")
    }

    /// Scène à dessiner : décor, état du casque interpolé, joueur VR, murs.
    fn instances(&self) -> (Vec<Instance>, Vec<Instance>) {
        let bx = |b: &NetBox| {
            Instance::boxed(
                Vec3::from(b.p),
                Vec3::from(b.half),
                Quat::from_array(b.q),
                Vec4::from(b.color),
            )
        };
        let mut cubes: Vec<Instance> = self.statics.iter().map(bx).collect();
        let mut spheres = Vec::new();
        if self.statics.is_empty() {
            cubes.push(Instance::boxed(
                Vec3::new(0.0, -0.05, 0.0),
                Vec3::new(30.0, 0.05, 30.0),
                Quat::IDENTITY,
                Vec4::new(0.42, 0.62, 0.36, 2.0),
            ));
        }
        if let Some((prev, cur, t)) = &self.snaps {
            let k = ((self.clock - t) / 0.05 - (INTERP_DELAY / 0.05 - 1.0)).clamp(0.0, 1.0);
            let same = prev.boxes.len() == cur.boxes.len();
            for (i, b) in cur.boxes.iter().enumerate() {
                let (p, q) = match (same, prev.boxes.get(i)) {
                    (true, Some(a)) => (
                        Vec3::from(a.p).lerp(Vec3::from(b.p), k),
                        Quat::from_array(a.q).slerp(Quat::from_array(b.q), k),
                    ),
                    _ => (Vec3::from(b.p), Quat::from_array(b.q)),
                };
                cubes.push(Instance::boxed(p, Vec3::from(b.half), q, Vec4::from(b.color)));
            }
            let same = prev.spheres.len() == cur.spheres.len();
            for (i, s) in cur.spheres.iter().enumerate() {
                let p = match (same, prev.spheres.get(i)) {
                    (true, Some(a)) => Vec3::from(a.p).lerp(Vec3::from(s.p), k),
                    _ => Vec3::from(s.p),
                };
                spheres.push(Instance::boxed(
                    p,
                    Vec3::splat(s.r),
                    Quat::IDENTITY,
                    Vec4::from(s.color),
                ));
            }
            // Joueur VR : tête (et visière), corps sous la tête.
            let (hp, hq) = (
                Vec3::from(prev.head.0).lerp(Vec3::from(cur.head.0), k),
                Quat::from_array(prev.head.1).slerp(Quat::from_array(cur.head.1), k),
            );
            let body = Vec4::new(0.95, 0.55, 0.2, 1.0);
            spheres.push(Instance::boxed(hp, Vec3::splat(0.13), hq, body));
            cubes.push(Instance::boxed(
                hp + hq * Vec3::new(0.0, 0.0, -0.1),
                Vec3::new(0.1, 0.04, 0.04),
                hq,
                Vec4::new(0.1, 0.1, 0.12, 1.0),
            ));
            let yaw = Quat::from_rotation_y({
                let f = hq * Vec3::NEG_Z;
                (-f.x).atan2(-f.z)
            });
            let torso_h = ((hp.y - self.origin.y) - 0.25).max(0.4) * 0.5;
            cubes.push(Instance::boxed(
                Vec3::new(hp.x, self.origin.y + torso_h, hp.z),
                Vec3::new(0.2, torso_h, 0.12),
                yaw,
                body,
            ));
        }
        // Mes murs (tout de suite, sans attendre l'aller-retour) et l'aperçu.
        let wall_color = Vec4::new(0.35, 0.62, 0.98, 1.0);
        for (w, _) in &self.walls {
            cubes.push(Instance::boxed(
                Vec3::new(w.x, self.origin.y + WALL_HALF.y, w.z),
                WALL_HALF,
                Quat::from_rotation_y(w.yaw),
                wall_color,
            ));
        }
        if let Some(w) = self.wall_preview() {
            let ready = self.clock - self.last_wall >= WALL_COOLDOWN;
            let c = if ready {
                Vec4::new(0.5, 0.8, 1.0, 0.5)
            } else {
                Vec4::new(0.6, 0.6, 0.65, 0.5)
            };
            let r = Quat::from_rotation_y(w.yaw);
            let base = Vec3::new(w.x, self.origin.y, w.z);
            // Contour : deux montants et deux traverses.
            for (off, half) in [
                (Vec3::new(-WALL_HALF.x, WALL_HALF.y, 0.0), Vec3::new(0.012, WALL_HALF.y, 0.012)),
                (Vec3::new(WALL_HALF.x, WALL_HALF.y, 0.0), Vec3::new(0.012, WALL_HALF.y, 0.012)),
                (Vec3::new(0.0, 0.01, 0.0), Vec3::new(WALL_HALF.x, 0.012, 0.012)),
                (Vec3::new(0.0, WALL_HALF.y * 2.0, 0.0), Vec3::new(WALL_HALF.x, 0.012, 0.012)),
            ] {
                cubes.push(Instance::boxed(base + r * off, half, r, c));
            }
        }
        (cubes, spheres)
    }

    fn eye_view(&self, width: u32, height: u32) -> EyeView {
        let (position, orientation) = self.eye();
        let v = 35f32.to_radians();
        let h = (v.tan() * width as f32 / height.max(1) as f32).atan();
        EyeView {
            orientation,
            position,
            fov: Fov {
                left: -h,
                right: h,
                up: v,
                down: -v,
            },
        }
    }
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    gfx: Gfx,
}

impl Gpu {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
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
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("device : {e}"))?;
        let caps = surface.get_capabilities(&adapter);
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
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or_default(),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let gfx = Gfx::new(&device, format, config.width, config.height);
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            gfx,
        })
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
        self.gfx.resize(&self.device, self.config.width, self.config.height);
    }

    fn render(&mut self, player: &Player) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let (cubes, spheres) = player.instances();
        let eye = player.eye_view(self.config.width, self.config.height);
        self.gfx.draw(
            &self.device,
            &self.queue,
            &[eye],
            &[&view],
            &cubes,
            &spheres,
            player.origin + Vec3::new(0.0, 0.0, -2.5),
            1.0,
        );
        frame.present();
    }
}

struct App {
    player: Player,
    gpu: Option<Gpu>,
    last: Option<Instant>,
    looking: bool,
    cursor: Option<(f64, f64)>,
    title_at: f32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Ball — ordinateur")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("fenêtre : {e}");
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Gpu::new(window)) {
            Ok(gpu) => {
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
                if code == KeyCode::Escape {
                    event_loop.exit();
                    return;
                }
                if event.state == ElementState::Pressed {
                    self.player.keys.insert(code);
                } else {
                    self.player.keys.remove(&code);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Right => self.looking = state == ElementState::Pressed,
                MouseButton::Left if state == ElementState::Pressed => self.player.place_wall(),
                _ => {}
            },
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x, position.y);
                if let (true, Some(prev)) = (self.looking, self.cursor) {
                    self.player.yaw -= (p.0 - prev.0) as f32 * 0.004;
                    self.player.pitch =
                        (self.player.pitch - (p.1 - prev.1) as f32 * 0.004).clamp(-1.2, 1.2);
                }
                self.cursor = Some(p);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = self
                    .last
                    .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32())
                    .min(0.1);
                self.last = Some(now);
                self.player.update(dt);
                if let Some(gpu) = self.gpu.as_mut() {
                    if self.player.clock >= self.title_at {
                        self.title_at = self.player.clock + 0.25;
                        gpu.window.set_title(&self.player.title());
                    }
                    gpu.render(&self.player);
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// `--snapshot <png>` : attend la scène du casque (15 s au plus), rend une
/// image hors écran à 1280×800 et quitte. `--wall` : pose un mur d'abord.
fn snapshot(path: &str, wall: bool) -> Result<(), String> {
    let mut player = Player::new(&relay_url());
    let started = std::time::Instant::now();
    let dt = 1.0 / 60.0;
    while player.snaps.is_none() || player.statics.is_empty() {
        if started.elapsed().as_secs() > 15 {
            return Err(format!("pas de scène reçue ({:?})", player.link.status()));
        }
        player.update(dt);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    for i in 0..90 {
        if wall && i == 10 {
            player.pitch = -0.35;
            player.place_wall();
            player.pitch = -0.15;
        }
        player.update(dt);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    println!("{}", player.title());
    let (cubes, spheres) = player.instances();
    let eye = player.eye_view(1280, 800);
    let shadow_center = player.origin + Vec3::new(0.0, 0.0, -2.5);
    // `--linger <s>` : rester connecté (le casque simulé voit l'avatar et le mur).
    let linger: f32 = std::env::args()
        .skip_while(|a| a != "--linger")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let linger_thread = if linger > 0.0 {
        Some(std::thread::spawn(move || {
            let start = std::time::Instant::now();
            while start.elapsed().as_secs_f32() < linger {
                player.update(1.0 / 60.0);
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }))
    } else {
        None
    };
    let (w, h) = (1280u32, 800u32);
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&Default::default())
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&Default::default())
            .await
            .map_err(|e| e.to_string())?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ball_pc_snapshot"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let gfx = Gfx::new(&device, format, w, h);
        gfx.draw(
            &device,
            &queue,
            &[eye],
            &[&view],
            &cubes,
            &spheres,
            shadow_center,
            1.0,
        );
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(w * 4 * h),
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
                    bytes_per_row: Some(w * 4),
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
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        let data = buffer.slice(..).get_mapped_range().to_vec();
        image::save_buffer(path, &data, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())
    })?;
    if let Some(t) = linger_thread {
        let _ = t.join();
    }
    Ok(())
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| "ball_pc.png".into());
        match snapshot(&path, args.iter().any(|a| a == "--wall")) {
            Ok(()) => println!("{path}"),
            Err(e) => {
                eprintln!("ball_pc --snapshot : {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let event_loop = EventLoop::new().expect("boucle d'événements");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        player: Player::new(&relay_url()),
        gpu: None,
        last: None,
        looking: false,
        cursor: None,
        title_at: 0.0,
    };
    event_loop.run_app(&mut app).expect("boucle");
}
