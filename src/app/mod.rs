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

    // --- gizmo ---
    pub gizmo_mode: GizmoMode,
    /// Axe en cours de manipulation (0 = X, 1 = Y, 2 = Z).
    pub active_axis: Option<usize>,
    drag_start_t: f32,
    drag_start_angle: f32,
    drag_orig_pos: Vec3,
    drag_orig_rot: Quat,
    drag_orig_scale: Vec3,
}

/// Mode de manipulation du gizmo (touches W / E / R).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

/// Longueur (monde) des axes / rayon des anneaux du gizmo. Partagée picking ↔ rendu.
pub const GIZMO_LEN: f32 = 1.0;

/// Direction unitaire d'un axe du gizmo.
pub fn axis_dir(axis: usize) -> Vec3 {
    match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

/// Base orthonormée (u, w) du plan perpendiculaire à un axe.
pub fn axis_basis(a: Vec3) -> (Vec3, Vec3) {
    let reference = if a.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = a.cross(reference).normalize();
    let w = a.cross(u).normalize();
    (u, w)
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
            gizmo_mode: GizmoMode::Translate,
            active_axis: None,
            drag_start_t: 0.0,
            drag_start_angle: 0.0,
            drag_orig_pos: Vec3::ZERO,
            drag_orig_rot: Quat::IDENTITY,
            drag_orig_scale: Vec3::ONE,
        }
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.gizmo_mode = mode;
    }

    pub fn set_viewport(&mut self, width: u32, height: u32) {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;
        self.viewport = (w, h);
        self.camera.aspect = w / h;
    }

    /// Traite un événement d'entrée agnostique (gizmo, orbit, zoom, sélection).
    pub fn handle_input(&mut self, event: InputEvent) {
        match event {
            InputEvent::PointerDown => {
                self.press_cursor = self.last_cursor;
                // priorité au gizmo : si une poignée est cliquée, on démarre la manipulation.
                if let (Some(sel), Some((cx, cy))) = (self.selection, self.last_cursor) {
                    let origin = self.scene.objects[sel].transform.position;
                    let t = &self.scene.objects[sel].transform;
                    let (orig_rot, orig_scale) = (t.rotation, t.scale);
                    match self.gizmo_mode {
                        GizmoMode::Rotate => {
                            if let Some(axis) = self.pick_ring(sel, cx, cy) {
                                if let Some(ang) =
                                    self.ring_drag_angle(origin, axis_dir(axis), cx, cy)
                                {
                                    self.active_axis = Some(axis);
                                    self.drag_start_angle = ang;
                                    self.drag_orig_pos = origin;
                                    self.drag_orig_rot = orig_rot;
                                }
                                return;
                            }
                        }
                        _ => {
                            if let Some(axis) = self.pick_axis(sel, cx, cy) {
                                if let Some(p) = self.axis_drag_param(origin, axis_dir(axis), cx, cy)
                                {
                                    self.active_axis = Some(axis);
                                    self.drag_start_t = p;
                                    self.drag_orig_pos = origin;
                                    self.drag_orig_scale = orig_scale;
                                }
                                return;
                            }
                        }
                    }
                }
                self.dragging = true;
            }
            InputEvent::PointerUp => {
                if self.active_axis.take().is_some() {
                    self.press_cursor = None;
                    return;
                }
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
                // manipulation via la poignée active
                if let (Some(axis), Some(sel)) = (self.active_axis, self.selection) {
                    let a = axis_dir(axis);
                    match self.gizmo_mode {
                        GizmoMode::Translate => {
                            if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y) {
                                self.scene.objects[sel].transform.position =
                                    self.drag_orig_pos + a * (t - self.drag_start_t);
                            }
                        }
                        GizmoMode::Scale => {
                            if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y) {
                                let d = t - self.drag_start_t;
                                let mut s = self.drag_orig_scale;
                                match axis {
                                    0 => s.x = (s.x + d).max(0.05),
                                    1 => s.y = (s.y + d).max(0.05),
                                    _ => s.z = (s.z + d).max(0.05),
                                }
                                self.scene.objects[sel].transform.scale = s;
                            }
                        }
                        GizmoMode::Rotate => {
                            if let Some(ang) = self.ring_drag_angle(self.drag_orig_pos, a, x, y) {
                                let delta = ang - self.drag_start_angle;
                                self.scene.objects[sel].transform.rotation =
                                    Quat::from_axis_angle(a, delta) * self.drag_orig_rot;
                            }
                        }
                    }
                    self.last_cursor = Some((x, y));
                    return;
                } else if self.dragging {
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

    /// Rayon monde (origine, direction) issu d'un point écran en pixels.
    fn ray(&self, px: f64, py: f64) -> (Vec3, Vec3) {
        let (w, h) = self.viewport;
        let ndc_x = 2.0 * px as f32 / w - 1.0;
        let ndc_y = 1.0 - 2.0 * py as f32 / h;
        let inv = self.camera.view_proj().inverse();
        let near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let origin = near.truncate() / near.w;
        let dir = (far.truncate() / far.w - origin).normalize();
        (origin, dir)
    }

    /// Projette un point monde vers les coordonnées écran (pixels), si devant la caméra.
    fn project(&self, world: Vec3) -> Option<(f64, f64)> {
        let clip = self.camera.view_proj() * world.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let (w, h) = self.viewport;
        Some((
            ((ndc.x * 0.5 + 0.5) * w) as f64,
            ((1.0 - (ndc.y * 0.5 + 0.5)) * h) as f64,
        ))
    }

    /// Renvoie l'axe du gizmo sous le curseur (test écran à ~10 px), ou None.
    fn pick_axis(&self, sel: usize, px: f64, py: f64) -> Option<usize> {
        let origin = self.scene.objects[sel].transform.position;
        let mut best: Option<(f64, usize)> = None;
        for axis in 0..3 {
            let (Some(p0), Some(p1)) =
                (self.project(origin), self.project(origin + axis_dir(axis) * GIZMO_LEN))
            else {
                continue;
            };
            let d = point_segment_dist((px, py), p0, p1);
            if d < 10.0 && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, axis));
            }
        }
        best.map(|(_, a)| a)
    }

    /// Paramètre `t` du point du curseur projeté sur l'axe (via le plan le plus face caméra).
    fn axis_drag_param(&self, origin: Vec3, a: Vec3, px: f64, py: f64) -> Option<f32> {
        let (ro, rd) = self.ray(px, py);
        // plan contenant l'axe, de normale perpendiculaire à l'axe et tournée vers la vue
        let n = a.cross(rd.cross(a));
        if n.length_squared() < 1e-8 {
            return None;
        }
        let n = n.normalize();
        let denom = rd.dot(n);
        if denom.abs() < 1e-6 {
            return None;
        }
        let t_ray = (origin - ro).dot(n) / denom;
        let p = ro + rd * t_ray;
        Some((p - origin).dot(a))
    }

    /// Renvoie l'axe dont l'anneau de rotation est sous le curseur (~10 px), ou None.
    fn pick_ring(&self, sel: usize, px: f64, py: f64) -> Option<usize> {
        const N: usize = 48;
        let origin = self.scene.objects[sel].transform.position;
        let mut best: Option<(f64, usize)> = None;
        for axis in 0..3 {
            let (u, w) = axis_basis(axis_dir(axis));
            let mut prev: Option<(f64, f64)> = None;
            let mut first: Option<(f64, f64)> = None;
            let mut min_d = f64::INFINITY;
            for j in 0..=N {
                let ang = std::f32::consts::TAU * j as f32 / N as f32;
                let pt = origin + (u * ang.cos() + w * ang.sin()) * GIZMO_LEN;
                let Some(sp) = self.project(pt) else { continue };
                if first.is_none() {
                    first = Some(sp);
                }
                if let Some(pp) = prev {
                    min_d = min_d.min(point_segment_dist((px, py), pp, sp));
                }
                prev = Some(sp);
            }
            if min_d < 10.0 && best.map_or(true, |(bd, _)| min_d < bd) {
                best = Some((min_d, axis));
            }
        }
        best.map(|(_, a)| a)
    }

    /// Angle (radians) du curseur autour de l'axe, dans le plan perpendiculaire à `a`.
    fn ring_drag_angle(&self, origin: Vec3, a: Vec3, px: f64, py: f64) -> Option<f32> {
        let (ro, rd) = self.ray(px, py);
        let denom = rd.dot(a);
        if denom.abs() < 1e-6 {
            return None;
        }
        let t = (origin - ro).dot(a) / denom;
        let p = ro + rd * t;
        let v = p - origin;
        let (u, w) = axis_basis(a);
        Some(v.dot(w).atan2(v.dot(u)))
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
            Ok(mut s) => {
                s.reload_imported();
                self.scene = s;
                self.selection = None;
            }
            Err(e) => log::error!("Échec chargement : {e}"),
        }
    }

    /// Importe un modèle glTF/GLB et ajoute un objet le référençant.
    pub fn import_gltf(&mut self, path: &str) {
        match crate::scene::import::load_gltf(path) {
            Ok((data, min, max)) => {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Modèle")
                    .to_string();
                let idx = self.scene.imported.len() as u32;
                self.scene.imported.push(crate::scene::ImportedMesh {
                    name: name.clone(),
                    path: path.to_string(),
                    data,
                    aabb_min: min,
                    aabb_max: max,
                });
                // Recadrage auto : centrer le modèle à l'origine et le mettre à l'échelle ~2 u.
                let size = max - min;
                let s = 2.0 / size.max_element().max(1e-3);
                let center = (min + max) * 0.5;
                self.scene.objects.push(crate::scene::SceneObject {
                    name,
                    transform: crate::scene::Transform {
                        position: -center * s,
                        rotation: Quat::IDENTITY,
                        scale: Vec3::splat(s),
                    },
                    mesh: MeshKind::Imported(idx),
                });
                self.selection = Some(self.scene.objects.len() - 1);
                log::info!("Modèle importé : {path}");
            }
            Err(e) => log::error!("Import glTF échoué : {e}"),
        }
    }

    /// Lance un rayon depuis le curseur et renvoie l'objet le plus proche touché.
    fn pick(&self, px: f64, py: f64) -> Option<usize> {
        let (origin, dir) = self.ray(px, py);

        let mut best: Option<(f32, usize)> = None;
        for (i, obj) in self.scene.objects.iter().enumerate() {
            let (lmin, lmax) = self.scene.local_aabb(obj.mesh);
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

/// Distance 2D (pixels) entre un point et un segment.
fn point_segment_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (px, py) = p;
    let (ax, ay) = a;
    let (bx, by) = b;
    let abx = bx - ax;
    let aby = by - ay;
    let len2 = abx * abx + aby * aby;
    let t = if len2 < 1e-9 {
        0.0
    } else {
        (((px - ax) * abx + (py - ay) * aby) / len2).clamp(0.0, 1.0)
    };
    let cx = ax + t * abx;
    let cy = ay + t * aby;
    (px - cx).hypot(py - cy)
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
