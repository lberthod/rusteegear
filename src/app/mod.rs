//! État applicatif **sans dépendance GPU** : scène, sélection, caméra, mode Play,
//! interaction pointeur. Le `Renderer` consomme cet état pour dessiner.

pub mod ai;
pub mod build_config;
pub mod input;
pub mod settings;

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use mlua::Lua;

use crate::gfx::camera::OrbitCamera;
use crate::gfx::mesh::MeshData;
use crate::scene::{ImportedMesh, MeshKind, Scene, SceneObject, Transform};
use input::InputEvent;

/// Résultat d'un import glTF effectué en thread de fond.
type ImportResult = Result<(String, MeshData, Vec3, Vec3), String>;

/// Rectangle `(x, y, largeur, hauteur)` d'un écran de téléphone (ratio 1080×2340,
/// ≈ 19.5:9) centré dans une zone `width × height`, avec une petite marge.
/// Sert à l'« Aperçu mobile » : même calcul en pixels (viewport GPU) et en points (UI egui).
pub fn device_rect(width: f32, height: f32, portrait: bool) -> (f32, f32, f32, f32) {
    let ar = if portrait {
        1080.0 / 2340.0
    } else {
        2340.0 / 1080.0
    };
    let margin = 0.94;
    let mut w = width * margin;
    let mut h = w / ar;
    if h > height * margin {
        h = height * margin;
        w = h * ar;
    }
    ((width - w) * 0.5, (height - h) * 0.5, w, h)
}

/// État des contrôles tactiles produit par l'overlay UI et lu par les scripts Lua.
#[derive(Default)]
pub struct PlayerInput {
    /// Axe du joystick virtuel, chaque composante dans [-1, 1].
    pub joy: (f32, f32),
    /// Boutons actuellement pressés (par nom).
    pub buttons: std::collections::HashSet<String>,
}

pub struct AppState {
    pub scene: Scene,
    /// Sélection « primaire » (gizmo, inspecteur, surbrillance forte).
    pub selection: Option<usize>,
    /// Ensemble sélectionné (inclut la primaire) pour les opérations groupées.
    pub selected: Vec<usize>,
    /// Presse-papiers d'objets (copier/coller).
    clipboard: Vec<SceneObject>,
    pub playing: bool,
    /// En pause : reste en mode Play mais gèle la simulation (scripts, physique, temps).
    pub paused: bool,
    /// Demande de fermeture de l'application (menu Fichier → Quitter).
    pub should_quit: bool,
    /// Mode « player » : pas d'éditeur (panneaux egui), démarre en Play.
    pub player: bool,
    /// État courant des contrôles tactiles (joystick + boutons), lu par les scripts.
    pub input_state: PlayerInput,
    /// Objet « tactile » touché cette frame (exposé une frame à son script via `obj.tapped`).
    tapped_obj: Option<usize>,
    /// « Aperçu mobile » : restreint la vue 3D à un écran de téléphone (letterbox).
    pub device_preview: bool,
    /// Orientation de l'aperçu mobile (portrait par défaut).
    pub device_portrait: bool,
    /// Région centrale 3D (hors panneaux) en pixels physiques `(x, y, w, h)`,
    /// remontée par l'éditeur ; base de l'aperçu mobile. `(0,0,0,0)` = plein écran.
    pub view_rect_px: (f32, f32, f32, f32),
    pub camera: OrbitCamera,

    viewport: (f32, f32),
    last_frame: Instant,
    /// Images par seconde lissées (moyenne mobile exponentielle), pour le bandeau d'état.
    fps: f32,

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
    /// Positions d'origine de tous les objets sélectionnés (gizmo translate multi).
    drag_orig_positions: Vec<(usize, Vec3)>,
    /// Le prochain clic ajoute/retire de la sélection (Cmd/Maj enfoncé).
    additive: bool,

    // --- historique (snapshots de la liste d'objets) ---
    undo_stack: VecDeque<Vec<SceneObject>>,
    redo_stack: Vec<Vec<SceneObject>>,

    // --- scripting ---
    lua: Lua,
    /// Chunks Lua déjà compilés, indexés par hash de la source (évite de re-parser
    /// le même script à chaque frame).
    script_cache: HashMap<u64, mlua::Function>,
    time: f32,

    // --- runtime Play ---
    was_playing: bool,
    play_snapshot: Vec<SceneObject>,
    physics: Option<crate::runtime::physics::Physics>,
    audio: crate::runtime::audio::Audio,

    // --- import glTF asynchrone ---
    import_tx: Sender<ImportResult>,
    import_rx: Receiver<ImportResult>,

    // --- chargement de scène asynchrone (Load) ---
    scene_load_tx: Sender<Result<Scene, String>>,
    scene_load_rx: Receiver<Result<Scene, String>>,
    /// Vrai après remplacement de la scène : le renderer doit reconstruire les meshes GPU importés.
    imported_dirty: bool,

    // --- génération de script par IA (asynchrone) ---
    ai_tx: Sender<(usize, Result<String, String>)>,
    ai_rx: Receiver<(usize, Result<String, String>)>,
    /// Une génération IA est en cours (désactive le bouton, affiche l'état).
    pub ai_busy: bool,
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

/// Nombre de segments par anneau de rotation du gizmo. Partagé picking ↔ rendu
/// pour garantir une géométrie identique des deux côtés.
pub const RING_SEGMENTS: usize = 48;

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
        let (tx, rx) = channel();
        let (scene_tx, scene_rx) = channel();
        let (ai_tx, ai_rx) = channel();
        AppState {
            scene: Scene::demo(),
            selection: None,
            selected: Vec::new(),
            clipboard: Vec::new(),
            playing: false,
            paused: false,
            should_quit: false,
            player: false,
            input_state: PlayerInput::default(),
            tapped_obj: None,
            device_preview: false,
            device_portrait: true,
            view_rect_px: (0.0, 0.0, 0.0, 0.0),
            camera: OrbitCamera::new(1.0),
            viewport: (1.0, 1.0),
            last_frame: Instant::now(),
            fps: 0.0,
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
            drag_orig_positions: Vec::new(),
            additive: false,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            lua: Lua::new(),
            script_cache: HashMap::new(),
            time: 0.0,
            was_playing: false,
            play_snapshot: Vec::new(),
            physics: None,
            audio: crate::runtime::audio::Audio::new(),
            import_tx: tx,
            import_rx: rx,
            scene_load_tx: scene_tx,
            scene_load_rx: scene_rx,
            imported_dirty: false,
            ai_tx,
            ai_rx,
            ai_busy: false,
        }
    }

    /// Lance une génération de script Lua par IA (thread de fond) pour l'objet `idx`.
    pub fn request_ai_script(&mut self, idx: usize, prompt: String, api_key: String) {
        if self.ai_busy {
            return;
        }
        self.ai_busy = true;
        let tx = self.ai_tx.clone();
        std::thread::spawn(move || {
            let result = ai::generate_lua(&api_key, &prompt);
            let _ = tx.send((idx, result));
        });
    }

    /// Applique un script généré par IA s'il est prêt (à appeler chaque frame).
    fn poll_ai(&mut self) {
        while let Ok((idx, result)) = self.ai_rx.try_recv() {
            self.ai_busy = false;
            match result {
                Ok(script) if idx < self.scene.objects.len() => {
                    self.push_undo();
                    self.scene.objects[idx].script = script;
                    log::info!("Script généré par IA appliqué à l'objet {idx}");
                }
                Ok(_) => {} // l'objet a disparu entre-temps
                Err(e) => log::error!("Génération IA : {e}"),
            }
        }
    }

    /// Indique (et réinitialise) si la scène vient d'être remplacée par un Load :
    /// le renderer s'en sert pour reconstruire ses meshes GPU importés.
    pub fn take_imported_dirty(&mut self) -> bool {
        std::mem::take(&mut self.imported_dirty)
    }

    /// Images par seconde lissées, pour le bandeau d'état de l'éditeur.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Vrai quand l'app doit rendre en continu (animation Play ou interaction en cours) :
    /// la boucle d'événements reste en `Poll`. Sinon elle peut throttler (économie CPU).
    pub fn is_active(&self) -> bool {
        (self.playing && !self.paused) || self.dragging || self.active_axis.is_some()
    }

    /// Charge la scène embarquée (jeu exporté) à la place de la démo : appelé en mode Player.
    pub fn use_embedded_scene(&mut self) {
        self.scene = Scene::embedded_player();
        self.selection = None;
    }

    /// Joue immédiatement un fichier son (bouton de test / scripts).
    pub fn play_audio(&mut self, path: &str) {
        self.audio.play(path);
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.gizmo_mode = mode;
    }

    /// Le prochain clic de sélection sera additif (Cmd/Maj enfoncé), positionné par la plateforme.
    pub fn set_additive(&mut self, additive: bool) {
        self.additive = additive;
    }

    /// Décale tous les objets sélectionnés (échange d'ordre) — réordonnancement simple.
    pub fn move_selected_in_list(&mut self, down: bool) {
        let Some(i) = self.selection else { return };
        let n = self.scene.objects.len();
        let j = if down {
            if i + 1 >= n {
                return;
            }
            i + 1
        } else {
            if i == 0 {
                return;
            }
            i - 1
        };
        self.push_undo();
        self.scene.objects.swap(i, j);
        self.select_single(j);
    }

    // --- sélection (primaire + ensemble) ---

    /// Sélectionne un seul objet (remplace l'ensemble).
    pub fn select_single(&mut self, i: usize) {
        self.selection = Some(i);
        self.selected = vec![i];
    }

    /// Vide toute la sélection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selected.clear();
    }

    /// Ajoute/retire un objet de l'ensemble sélectionné (clic Cmd/Maj).
    pub fn toggle_select(&mut self, i: usize) {
        if let Some(pos) = self.selected.iter().position(|&x| x == i) {
            self.selected.remove(pos);
            self.selection = self.selected.last().copied();
        } else {
            self.selected.push(i);
            self.selection = Some(i);
        }
    }

    /// Facteur de surbrillance d'un objet : primaire = 1.0, autre sélectionné = 0.55.
    pub fn highlight_of(&self, i: usize) -> f32 {
        if self.selection == Some(i) {
            1.0
        } else if self.selected.contains(&i) {
            0.55
        } else {
            0.0
        }
    }

    /// Copie les objets sélectionnés dans le presse-papiers.
    pub fn copy_selected(&mut self) {
        self.clipboard = self
            .selected
            .iter()
            .filter_map(|&i| self.scene.objects.get(i).cloned())
            .collect();
    }

    /// Colle le presse-papiers (décalé), et sélectionne les nouveaux objets.
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.push_undo();
        let start = self.scene.objects.len();
        let clips = self.clipboard.clone();
        for o in clips {
            let mut c = o.clone();
            c.name = format!("{} (copie)", c.name);
            c.transform.position += Vec3::new(0.6, 0.0, 0.6);
            self.scene.objects.push(c);
        }
        self.selected = (start..self.scene.objects.len()).collect();
        self.selection = self.selected.last().copied();
    }

    /// Supprime tous les objets sélectionnés (indices décroissants).
    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut idx = self.selected.clone();
        idx.sort_unstable();
        idx.dedup();
        for &i in idx.iter().rev() {
            if i < self.scene.objects.len() {
                self.scene.objects.remove(i);
            }
        }
        self.clear_selection();
    }

    // --- historique ---

    /// Capture l'état courant des objets avant une modification (vide la pile redo).
    pub fn push_undo(&mut self) {
        self.undo_stack.push_back(self.scene.objects.clone());
        if self.undo_stack.len() > 50 {
            self.undo_stack.pop_front(); // O(1), contrairement à Vec::remove(0)
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop_back() {
            self.redo_stack.push(self.scene.objects.clone());
            self.scene.objects = prev;
            self.clear_selection();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push_back(self.scene.objects.clone());
            self.scene.objects = next;
            self.clear_selection();
        }
    }

    // --- édition d'objets (avec historique) ---

    pub fn add_object(&mut self, kind: MeshKind) {
        self.push_undo();
        let name = format!("{} {}", kind.label(), self.scene.objects.len());
        self.scene.objects.push(SceneObject {
            name,
            transform: Transform::from_pos(Vec3::ZERO),
            mesh: kind,
            script: String::new(),
            physics: crate::runtime::physics::PhysicsKind::None,
            audio_clip: String::new(),
            audio_autoplay: false,
            group: String::new(),
            color: [1.0, 1.0, 1.0],
            texture: String::new(),
            tappable: false,
        });
        self.select_single(self.scene.objects.len() - 1);
    }

    /// Demande la fermeture de l'application (traitée par la boucle d'événements).
    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    /// Charge la démo mobile prête à jouer (avec historique pour annuler).
    pub fn load_mobile_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::mobile_demo();
        self.imported_dirty = true;
        self.clear_selection();
    }

    /// Nouveau projet : vide la scène (avec historique pour pouvoir annuler).
    pub fn new_scene(&mut self) {
        self.push_undo();
        self.scene.objects.clear();
        self.scene.imported.clear();
        self.scene.groups.clear();
        self.clear_selection();
    }

    /// Pose la base des objets sélectionnés sur le plan du sol (y = 0).
    pub fn align_to_ground(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        for &i in &self.selected.clone() {
            if let Some(o) = self.scene.objects.get(i) {
                let (lmin, _) = self.scene.local_aabb(o.mesh);
                let base_offset = lmin.y * o.transform.scale.y;
                if let Some(o) = self.scene.objects.get_mut(i) {
                    o.transform.position.y = -base_offset;
                }
            }
        }
    }

    /// Réinitialise rotation et échelle des objets sélectionnés (position conservée).
    pub fn reset_transform(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        for &i in &self.selected.clone() {
            if let Some(o) = self.scene.objects.get_mut(i) {
                o.transform.rotation = Quat::IDENTITY;
                o.transform.scale = Vec3::ONE;
            }
        }
    }

    pub fn delete_object(&mut self, i: usize) {
        if i < self.scene.objects.len() {
            self.push_undo();
            self.scene.objects.remove(i);
            self.clear_selection();
        }
    }

    pub fn duplicate_selected(&mut self) {
        let mut idx = self.selected.clone();
        idx.sort_unstable();
        idx.dedup();
        idx.retain(|&i| i < self.scene.objects.len());
        if idx.is_empty() {
            return;
        }
        self.push_undo();
        let start = self.scene.objects.len();
        for i in idx {
            let mut copy = self.scene.objects[i].clone();
            copy.name = format!("{} (copie)", copy.name);
            copy.transform.position += Vec3::new(0.6, 0.0, 0.6);
            self.scene.objects.push(copy);
        }
        self.selected = (start..self.scene.objects.len()).collect();
        self.selection = self.selected.last().copied();
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
                // Aperçu mobile : on joue au tactile, pas d'édition (ni gizmo, ni sélection).
                if self.device_preview {
                    self.dragging = true;
                    return;
                }
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
                                    self.push_undo(); // un seul snapshot par manipulation
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
                                if let Some(p) =
                                    self.axis_drag_param(origin, axis_dir(axis), cx, cy)
                                {
                                    self.push_undo();
                                    self.active_axis = Some(axis);
                                    self.drag_start_t = p;
                                    self.drag_orig_pos = origin;
                                    self.drag_orig_scale = orig_scale;
                                    // mémorise les positions de toute la sélection (translate multi)
                                    self.drag_orig_positions = self
                                        .selected
                                        .iter()
                                        .filter_map(|&i| {
                                            self.scene
                                                .objects
                                                .get(i)
                                                .map(|o| (i, o.transform.position))
                                        })
                                        .collect();
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
                // Tap (appui sans déplacement notable) ?
                let tap = matches!(
                    (self.press_cursor, self.last_cursor),
                    (Some((px, py)), Some((cx, cy))) if (px - cx).hypot(py - cy) < 4.0
                );
                // En mode Play : un tap sur un objet « tactile » le notifie à son script.
                if self.playing
                    && !self.paused
                    && tap
                    && let Some((cx, cy)) = self.last_cursor
                    && let Some(i) = self.pick(cx, cy)
                    && self.scene.objects[i].tappable
                {
                    self.tapped_obj = Some(i);
                }
                // Aperçu mobile : pas de sélection éditeur au clic (on joue, on n'édite pas).
                if self.device_preview {
                    self.press_cursor = None;
                    return;
                }
                // appui sans déplacement notable = sélection éditeur
                if let (Some((px, py)), Some((cx, cy))) = (self.press_cursor, self.last_cursor)
                    && (px - cx).hypot(py - cy) < 4.0
                {
                    match self.pick(cx, cy) {
                        // Cmd/Maj : ajoute/retire de la sélection ; sinon sélection simple.
                        Some(i) if self.additive => self.toggle_select(i),
                        Some(i) => self.select_single(i),
                        None if !self.additive => self.clear_selection(),
                        None => {}
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
                                let delta = a * (t - self.drag_start_t);
                                if self.drag_orig_positions.len() > 1 {
                                    // déplace toute la sélection en bloc
                                    for (i, orig) in &self.drag_orig_positions {
                                        if let Some(o) = self.scene.objects.get_mut(*i) {
                                            o.transform.position = *orig + delta;
                                        }
                                    }
                                } else {
                                    self.scene.objects[sel].transform.position =
                                        self.drag_orig_pos + delta;
                                }
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
                } else if self.dragging
                    && !self.device_preview // en aperçu mobile : pas d'orbite souris (simule le tactile)
                    && let Some((lx, ly)) = self.last_cursor
                {
                    self.camera.yaw -= (x - lx) as f32 * 0.005;
                    self.camera.pitch += (y - ly) as f32 * 0.005;
                }
                self.last_cursor = Some((x, y));
            }
            InputEvent::Scroll { delta } => {
                // En aperçu mobile, la molette ne zoome pas (un téléphone n'a pas de molette).
                if !self.device_preview {
                    self.camera.distance = (self.camera.distance - delta * 0.5).clamp(1.5, 50.0);
                }
            }
        }
    }

    /// Rayon monde (origine, direction) issu d'un point écran en pixels.
    /// `vp_inv` = inverse de la view-projection (calculée une fois par l'appelant).
    /// Convertit un point écran (pixels) en NDC, en tenant compte du rectangle
    /// letterboxé de l'aperçu mobile (sinon : tout le viewport).
    fn screen_to_ndc(&self, px: f64, py: f64) -> (f32, f32) {
        let (ox, oy, w, h) = if self.device_preview {
            let (bx, by, bw, bh) = if self.view_rect_px.2 > 1.0 {
                self.view_rect_px
            } else {
                (0.0, 0.0, self.viewport.0, self.viewport.1)
            };
            let (rx, ry, rw, rh) = device_rect(bw, bh, self.device_portrait);
            (bx + rx, by + ry, rw, rh)
        } else {
            (0.0, 0.0, self.viewport.0, self.viewport.1)
        };
        (
            2.0 * (px as f32 - ox) / w - 1.0,
            1.0 - 2.0 * (py as f32 - oy) / h,
        )
    }

    fn ray_with(&self, vp_inv: Mat4, px: f64, py: f64) -> (Vec3, Vec3) {
        let (ndc_x, ndc_y) = self.screen_to_ndc(px, py);
        let near = vp_inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = vp_inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let origin = near.truncate() / near.w;
        let dir = (far.truncate() / far.w - origin).normalize();
        (origin, dir)
    }

    /// Variante pratique : recalcule l'inverse de la view-projection à la volée.
    fn ray(&self, px: f64, py: f64) -> (Vec3, Vec3) {
        self.ray_with(self.camera.view_proj().inverse(), px, py)
    }

    /// Projette un point monde vers les coordonnées écran (pixels), si devant la caméra.
    /// `vp` = view-projection (calculée une fois par l'appelant).
    fn project_with(&self, vp: Mat4, world: Vec3) -> Option<(f64, f64)> {
        let clip = vp * world.extend(1.0);
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
        let vp = self.camera.view_proj();
        let mut best: Option<(f64, usize)> = None;
        for axis in 0..3 {
            let (Some(p0), Some(p1)) = (
                self.project_with(vp, origin),
                self.project_with(vp, origin + axis_dir(axis) * GIZMO_LEN),
            ) else {
                continue;
            };
            let d = point_segment_dist((px, py), p0, p1);
            if d < 10.0 && best.is_none_or(|(bd, _)| d < bd) {
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
        const N: usize = RING_SEGMENTS;
        let origin = self.scene.objects[sel].transform.position;
        let vp = self.camera.view_proj();
        let mut best: Option<(f64, usize)> = None;
        for axis in 0..3 {
            let (u, w) = axis_basis(axis_dir(axis));
            let mut prev: Option<(f64, f64)> = None;
            let mut first: Option<(f64, f64)> = None;
            let mut min_d = f64::INFINITY;
            for j in 0..=N {
                let ang = std::f32::consts::TAU * j as f32 / N as f32;
                let pt = origin + (u * ang.cos() + w * ang.sin()) * GIZMO_LEN;
                let Some(sp) = self.project_with(vp, pt) else {
                    continue;
                };
                if first.is_none() {
                    first = Some(sp);
                }
                if let Some(pp) = prev {
                    min_d = min_d.min(point_segment_dist((px, py), pp, sp));
                }
                prev = Some(sp);
            }
            if min_d < 10.0 && best.is_none_or(|(bd, _)| min_d < bd) {
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

    /// En mode Play : scripts Lua + simulation physique (delta-time).
    /// Au démarrage de Play, capture l'état ; à l'arrêt, le restaure.
    pub fn advance_play(&mut self) {
        // chargements asynchrones (imports glTF, sons décodés, script IA) prêts cette frame
        self.poll_imports();
        self.poll_ai();
        self.audio.update();

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        // FPS lissé (EMA) ; ignore les dt aberrants (première frame, throttle au repos).
        if dt > 1e-4 && dt < 0.5 {
            let inst = 1.0 / dt;
            self.fps = if self.fps == 0.0 {
                inst
            } else {
                self.fps * 0.9 + inst * 0.1
            };
        }

        // transitions Edit <-> Play
        if self.playing && !self.was_playing {
            self.play_snapshot = self.scene.objects.clone();
            self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
            // sons en autoplay
            let clips: Vec<String> = self
                .scene
                .objects
                .iter()
                .filter(|o| o.audio_autoplay && !o.audio_clip.is_empty())
                .map(|o| o.audio_clip.clone())
                .collect();
            for c in clips {
                self.audio.play(&c);
            }
            // Caméra de suivi : se cale d'emblée sur le joueur (pas de panoramique initial).
            if self.scene.camera_follow
                && let Some(p) = self.player_position()
            {
                self.camera.target = p;
            }
        } else if !self.playing && self.was_playing {
            self.scene.objects = self.play_snapshot.clone();
            self.physics = None;
            self.paused = false;
            self.clear_selection();
            self.audio.stop_all();
        }
        self.was_playing = self.playing;

        // En pause : on reste en mode Play (snapshot conservé) mais on gèle la
        // simulation (ni scripts, ni physique, ni avance du temps).
        if !self.playing || self.paused {
            return;
        }

        // 1. scripts
        self.time += dt;
        let time = self.time;
        for (idx, obj) in self.scene.objects.iter_mut().enumerate() {
            if obj.script.trim().is_empty() {
                continue;
            }
            // Récupère (ou compile une seule fois) le chunk associé à cette source.
            let key = script_key(&obj.script);
            let func = match self.script_cache.get(&key) {
                Some(f) => f.clone(),
                None => match self.lua.load(&obj.script).into_function() {
                    Ok(f) => {
                        self.script_cache.insert(key, f.clone());
                        f
                    }
                    Err(e) => {
                        log::error!("Compilation du script '{}' : {e}", obj.name);
                        continue;
                    }
                },
            };
            let tapped = self.tapped_obj == Some(idx);
            if let Err(e) = run_script(
                &self.lua,
                &func,
                &mut obj.transform,
                &mut obj.color,
                dt,
                time,
                &self.input_state,
                tapped,
            ) {
                log::error!("Script '{}' : {e}", obj.name);
            }
        }
        // Le tap n'est exposé qu'une frame.
        self.tapped_obj = None;

        // 2. physique (écrase les poses des corps dynamiques)
        if let Some(phys) = &mut self.physics {
            phys.step(dt, &mut self.scene);
        }

        // 3. caméra qui suit le joueur (premier objet scripté) en douceur.
        if self.scene.camera_follow
            && let Some(p) = self.player_position()
        {
            let t = (dt * 6.0).min(1.0);
            self.camera.target = self.camera.target.lerp(p, t);
        }
    }

    /// Position du « joueur » : premier objet scripté, sinon premier objet.
    fn player_position(&self) -> Option<Vec3> {
        self.scene
            .objects
            .iter()
            .find(|o| !o.script.trim().is_empty())
            .or_else(|| self.scene.objects.first())
            .map(|o| o.transform.position)
    }

    /// Sauvegarde rapide vers l'emplacement par défaut (`~/motor3derust_scene.json`).
    pub fn save(&self) {
        self.save_to(&scene_path());
    }

    /// Sauvegarde la scène en JSON vers un chemin donné (« Enregistrer sous »).
    pub fn save_to(&self, path: &str) {
        match self.scene.save(path) {
            Ok(()) => log::info!("Scène sauvegardée dans {path}"),
            Err(e) => log::error!("Échec sauvegarde : {e}"),
        }
    }

    /// Charge la scène depuis l'emplacement par défaut.
    pub fn load(&mut self) {
        self.load_from(&scene_path());
    }

    /// Charge une scène depuis un chemin JSON donné, en thread de fond (sans bloquer
    /// le rendu). Le résultat est appliqué dans `poll_imports`.
    pub fn load_from(&mut self, path: &str) {
        let tx = self.scene_load_tx.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            let res = Scene::load(&path).map_err(|e| e.to_string()).map(|mut s| {
                s.reload_imported();
                s
            });
            let _ = tx.send(res);
        });
    }

    /// Lance l'import d'un modèle glTF/GLB en thread de fond (sans bloquer le rendu).
    pub fn import_gltf(&mut self, path: &str) {
        let tx = self.import_tx.clone();
        let p = path.to_string();
        std::thread::spawn(move || {
            let res = crate::scene::import::load_gltf(&p).map(|(d, mn, mx)| (p.clone(), d, mn, mx));
            let _ = tx.send(res);
        });
    }

    /// Récupère les imports terminés et les ajoute à la scène (appelé chaque frame).
    fn poll_imports(&mut self) {
        while let Ok(res) = self.import_rx.try_recv() {
            match res {
                Ok((path, data, min, max)) => self.finish_import(path, data, min, max),
                Err(e) => log::error!("Import glTF échoué : {e}"),
            }
        }
        // scènes chargées en arrière-plan (Load) prêtes cette frame
        while let Ok(res) = self.scene_load_rx.try_recv() {
            match res {
                Ok(s) => {
                    self.scene = s;
                    self.clear_selection();
                    self.imported_dirty = true;
                }
                Err(e) => log::error!("Échec chargement : {e}"),
            }
        }
    }

    fn finish_import(&mut self, path: String, data: MeshData, min: Vec3, max: Vec3) {
        let name = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Modèle")
            .to_string();
        let idx = self.scene.imported.len() as u32;
        self.scene.imported.push(ImportedMesh {
            name: name.clone(),
            path,
            data,
            aabb_min: min,
            aabb_max: max,
        });
        // Recadrage auto : centrer à l'origine, mise à l'échelle ~2 u.
        let size = max - min;
        let s = 2.0 / size.max_element().max(1e-3);
        let center = (min + max) * 0.5;
        self.scene.objects.push(SceneObject {
            name,
            transform: Transform {
                position: -center * s,
                rotation: Quat::IDENTITY,
                scale: Vec3::splat(s),
            },
            mesh: MeshKind::Imported(idx),
            script: String::new(),
            physics: crate::runtime::physics::PhysicsKind::None,
            audio_clip: String::new(),
            audio_autoplay: false,
            group: String::new(),
            color: [1.0, 1.0, 1.0],
            texture: String::new(),
            tappable: false,
        });
        self.select_single(self.scene.objects.len() - 1);
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
            if let Some(t) = ray_aabb(origin, dir, wmin, wmax)
                && best.is_none_or(|(bt, _)| t < bt)
            {
                best = Some((t, i));
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

/// Hash stable d'une source de script, clé du cache de chunks compilés.
fn script_key(src: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// Exécute le chunk Lua **déjà compilé** d'un objet : expose `obj` (x,y,z,
/// rx,ry,rz en °, sx,sy,sz, r,g,b, tapped), `dt`, `time` et `input`, puis relit
/// les champs modifiés.
#[allow(clippy::too_many_arguments)] // contexte d'exécution d'un script : champs distincts
fn run_script(
    lua: &Lua,
    func: &mlua::Function,
    t: &mut Transform,
    color: &mut [f32; 3],
    dt: f32,
    time: f32,
    input: &PlayerInput,
    tapped: bool,
) -> mlua::Result<()> {
    let (rx, ry, rz) = t.rotation.to_euler(EulerRot::XYZ);
    let obj = lua.create_table()?;
    obj.set("x", t.position.x)?;
    obj.set("y", t.position.y)?;
    obj.set("z", t.position.z)?;
    obj.set("rx", rx.to_degrees())?;
    obj.set("ry", ry.to_degrees())?;
    obj.set("rz", rz.to_degrees())?;
    obj.set("sx", t.scale.x)?;
    obj.set("sy", t.scale.y)?;
    obj.set("sz", t.scale.z)?;
    obj.set("r", color[0])?;
    obj.set("g", color[1])?;
    obj.set("b", color[2])?;
    obj.set("tapped", tapped)?;

    // Contrôles tactiles : `input.jx`, `input.jy` (joystick) et `input.btn.<nom>` (booléens).
    let input_tbl = lua.create_table()?;
    input_tbl.set("jx", input.joy.0)?;
    input_tbl.set("jy", input.joy.1)?;
    let btns = lua.create_table()?;
    for name in &input.buttons {
        btns.set(name.as_str(), true)?;
    }
    input_tbl.set("btn", btns)?;

    let g = lua.globals();
    g.set("obj", &obj)?;
    g.set("dt", dt)?;
    g.set("time", time)?;
    g.set("input", input_tbl)?;
    func.call::<()>(())?;

    t.position = Vec3::new(obj.get("x")?, obj.get("y")?, obj.get("z")?);
    let (rx, ry, rz): (f32, f32, f32) = (obj.get("rx")?, obj.get("ry")?, obj.get("rz")?);
    t.rotation = Quat::from_euler(
        EulerRot::XYZ,
        rx.to_radians(),
        ry.to_radians(),
        rz.to_radians(),
    );
    t.scale = Vec3::new(obj.get("sx")?, obj.get("sy")?, obj.get("sz")?);
    *color = [obj.get("r")?, obj.get("g")?, obj.get("b")?];
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Invariant : la primaire (si présente) appartient toujours à l'ensemble sélectionné.
    fn assert_selection_invariant(app: &AppState) {
        if let Some(p) = app.selection {
            assert!(
                app.selected.contains(&p),
                "primaire {p} absente de selected {:?}",
                app.selected
            );
        } else {
            assert!(
                app.selected.is_empty(),
                "selection None mais selected non vide"
            );
        }
    }

    #[test]
    fn selection_helpers_keep_invariant() {
        let mut app = AppState::new();
        app.select_single(2);
        assert_eq!(app.selection, Some(2));
        assert_eq!(app.selected, vec![2]);
        assert_selection_invariant(&app);

        app.toggle_select(5); // ajoute
        assert_eq!(app.selection, Some(5));
        assert!(app.selected.contains(&2) && app.selected.contains(&5));
        assert_selection_invariant(&app);

        app.toggle_select(5); // retire → primaire repasse au dernier restant
        assert!(!app.selected.contains(&5));
        assert_eq!(app.selection, Some(2));
        assert_selection_invariant(&app);

        app.toggle_select(2); // retire le dernier → plus rien
        assert_eq!(app.selection, None);
        assert!(app.selected.is_empty());
        assert_selection_invariant(&app);

        app.select_single(0);
        app.clear_selection();
        assert_selection_invariant(&app);
    }

    #[test]
    fn highlight_levels() {
        let mut app = AppState::new();
        app.select_single(0);
        app.toggle_select(1);
        assert_eq!(app.highlight_of(1), 1.0); // primaire
        assert_eq!(app.highlight_of(0), 0.55); // autre sélectionné
        assert_eq!(app.highlight_of(2), 0.0); // non sélectionné
    }

    #[test]
    fn script_reads_mobile_input() {
        // Le script déplace l'objet selon le joystick et saute si le bouton « B1 » est pressé.
        let lua = Lua::new();
        let src = "obj.x = obj.x + input.jx; if input.btn.B1 then obj.y = 5 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let mut input = PlayerInput {
            joy: (0.5, 0.0),
            buttons: std::collections::HashSet::new(),
        };
        input.buttons.insert("B1".into());
        run_script(&lua, &func, &mut t, &mut col, 0.016, 0.0, &input, false).unwrap();
        assert!((t.position.x - 0.5).abs() < 1e-5);
        assert!((t.position.y - 5.0).abs() < 1e-5);

        // Sans bouton ni joystick : aucun mouvement.
        let mut t2 = Transform::from_pos(Vec3::ZERO);
        let empty = PlayerInput::default();
        run_script(&lua, &func, &mut t2, &mut col, 0.016, 0.0, &empty, false).unwrap();
        assert!((t2.position.x).abs() < 1e-5);
        assert!((t2.position.y).abs() < 1e-5);
    }

    #[test]
    fn script_reacts_to_tap_and_changes_color() {
        // Au tap, l'objet vire au rouge.
        let lua = Lua::new();
        let src = "if obj.tapped then obj.r = 1.0; obj.g = 0.0; obj.b = 0.0 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [0.5, 0.5, 0.5];
        let input = PlayerInput::default();
        // pas de tap : couleur inchangée
        run_script(&lua, &func, &mut t, &mut col, 0.016, 0.0, &input, false).unwrap();
        assert_eq!(col, [0.5, 0.5, 0.5]);
        // tap : passe au rouge
        run_script(&lua, &func, &mut t, &mut col, 0.016, 0.0, &input, true).unwrap();
        assert_eq!(col, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn ray_aabb_hit_in_front() {
        // rayon partant de -10 sur Z+, visant le cube unité à l'origine
        let t = ray_aabb(
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::Z,
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
        );
        assert!(t.is_some());
        assert!((t.unwrap() - 9.5).abs() < 1e-3);
    }

    #[test]
    fn ray_aabb_miss_to_the_side() {
        let t = ray_aabb(
            Vec3::new(5.0, 0.0, -10.0),
            Vec3::Z,
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
        );
        assert!(t.is_none());
    }

    #[test]
    fn ray_aabb_behind_returns_none() {
        // box derrière l'origine du rayon (qui regarde Z+)
        let t = ray_aabb(
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::Z,
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
        );
        assert!(t.is_none());
    }

    #[test]
    fn point_segment_dist_basics() {
        // distance d'un point au milieu d'un segment horizontal
        let d = point_segment_dist((1.0, 2.0), (0.0, 0.0), (2.0, 0.0));
        assert!((d - 2.0).abs() < 1e-9);
        // projection au-delà de l'extrémité => distance à l'extrémité
        let d2 = point_segment_dist((5.0, 0.0), (0.0, 0.0), (2.0, 0.0));
        assert!((d2 - 3.0).abs() < 1e-9);
        // segment dégénéré (longueur nulle)
        let d3 = point_segment_dist((3.0, 4.0), (0.0, 0.0), (0.0, 0.0));
        assert!((d3 - 5.0).abs() < 1e-9);
    }

    #[test]
    fn axis_basis_is_orthonormal() {
        for axis in 0..3 {
            let a = axis_dir(axis);
            let (u, w) = axis_basis(a);
            assert!((u.length() - 1.0).abs() < 1e-5);
            assert!((w.length() - 1.0).abs() < 1e-5);
            assert!(u.dot(a).abs() < 1e-5);
            assert!(w.dot(a).abs() < 1e-5);
            assert!(u.dot(w).abs() < 1e-5);
        }
    }

    #[test]
    fn script_key_stable_and_distinct() {
        assert_eq!(script_key("obj.x = 1"), script_key("obj.x = 1"));
        assert_ne!(script_key("obj.x = 1"), script_key("obj.x = 2"));
    }
}
