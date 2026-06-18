//! État applicatif **sans dépendance GPU** : scène, sélection, caméra, mode Play,
//! interaction pointeur. Le `Renderer` consomme cet état pour dessiner.

pub mod input;

use std::time::Instant;

use glam::{Quat, Vec3, Vec4};

use crate::gfx::camera::OrbitCamera;
use crate::scene::{MeshKind, Scene};
use input::InputEvent;

pub struct AppState {
    pub scene: Scene,
    pub selection: Option<usize>,
    pub playing: bool,
    pub camera: OrbitCamera,

    viewport: (f32, f32),
    last_frame: Instant,

    // --- état d'interaction pointeur ---
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    press_cursor: Option<(f64, f64)>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            scene: Scene::demo(),
            selection: None,
            playing: false,
            camera: OrbitCamera::new(1.0),
            viewport: (1.0, 1.0),
            last_frame: Instant::now(),
            dragging: false,
            last_cursor: None,
            press_cursor: None,
        }
    }

    pub fn set_viewport(&mut self, width: u32, height: u32) {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;
        self.viewport = (w, h);
        self.camera.aspect = w / h;
    }

    /// Traite un événement d'entrée agnostique (orbit, zoom, sélection par picking).
    pub fn handle_input(&mut self, event: InputEvent) {
        match event {
            InputEvent::PointerDown => {
                self.dragging = true;
                self.press_cursor = self.last_cursor;
            }
            InputEvent::PointerUp => {
                self.dragging = false;
                // appui sans déplacement notable = sélection
                if let (Some((px, py)), Some((cx, cy))) = (self.press_cursor, self.last_cursor) {
                    if (px - cx).hypot(py - cy) < 4.0 {
                        self.selection = self.pick(cx, cy);
                    }
                }
                self.press_cursor = None;
            }
            InputEvent::PointerMove { x, y } => {
                if self.dragging {
                    if let Some((lx, ly)) = self.last_cursor {
                        self.camera.yaw -= (x - lx) as f32 * 0.005;
                        self.camera.pitch += (y - ly) as f32 * 0.005;
                    }
                }
                self.last_cursor = Some((x, y));
            }
            InputEvent::Scroll { delta } => {
                self.camera.distance = (self.camera.distance - delta * 0.5).clamp(1.5, 50.0);
            }
        }
    }

    /// Applique les comportements du mode Play (rotation simple) en delta-time.
    pub fn advance_play(&mut self) {
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

    pub fn save(&self) {
        let path = scene_path();
        match self.scene.save(&path) {
            Ok(()) => log::info!("Scène sauvegardée dans {path}"),
            Err(e) => log::error!("Échec sauvegarde : {e}"),
        }
    }

    pub fn load(&mut self) {
        match Scene::load(&scene_path()) {
            Ok(s) => {
                self.scene = s;
                self.selection = None;
            }
            Err(e) => log::error!("Échec chargement : {e}"),
        }
    }

    /// Lance un rayon depuis le curseur et renvoie l'objet le plus proche touché.
    fn pick(&self, px: f64, py: f64) -> Option<usize> {
        let (w, h) = self.viewport;
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
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
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
