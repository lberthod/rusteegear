//! État applicatif **sans dépendance GPU** : scène, sélection, caméra, mode Play,
//! interaction pointeur. Le `Renderer` consomme cet état pour dessiner.

pub mod ai;
pub mod build_config;
mod combat;
pub mod input;
pub mod multiplayer;
pub mod settings;

use combat::{AttackCharge, AttackProjectile};

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use mlua::Lua;

use crate::gfx::camera::OrbitCamera;
use crate::gfx::mesh::MeshData;
use crate::scene::{
    GameCamera, ImportedMesh, MeshKind, MobileControls, PointLight, Scene, SceneObject, Transform,
};

/// Instantané léger de la scène pour l'undo/redo (sans les meshes importés, lourds
/// et rarement modifiés) : objets + lumières + caméra de jeu + contrôles + groupes.
#[derive(Clone)]
struct SceneSnapshot {
    objects: Vec<SceneObject>,
    groups: Vec<String>,
    point_lights: Vec<PointLight>,
    mobile: MobileControls,
    camera_follow: bool,
    game_camera: Option<GameCamera>,
}

impl SceneSnapshot {
    fn capture(s: &Scene) -> Self {
        Self {
            objects: s.objects.clone(),
            groups: s.groups.clone(),
            point_lights: s.point_lights.clone(),
            mobile: s.mobile.clone(),
            camera_follow: s.camera_follow,
            game_camera: s.game_camera,
        }
    }
    fn restore(self, s: &mut Scene) {
        s.objects = self.objects;
        s.groups = self.groups;
        s.point_lights = self.point_lights;
        s.mobile = self.mobile;
        s.camera_follow = self.camera_follow;
        s.game_camera = self.game_camera;
    }
}
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
    /// Inclinaison (gyroscope/accéléromètre), chaque composante dans [-1, 1].
    /// Desktop : simulée aux flèches du clavier ; mobile : capteur natif (à brancher).
    pub tilt: (f32, f32),
    /// Déplacement clavier (ordinateur) : flèches / WASD ; chaque composante dans [-1, 1].
    pub key_move: (f32, f32),
    /// Saut clavier (Espace) maintenu enfoncé.
    pub jump: bool,
    /// Attaque clavier (J) maintenue enfoncée.
    pub attack: bool,
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
    /// Accumulateur de temps réel pour la simulation à **pas fixe** (découplée du rendu).
    sim_accumulator: f32,
    /// Temps de jeu (s) auquel tous les collectibles ont été ramassés (figé pour le HUD).
    win_time: Option<f32>,
    /// Partie perdue : le joueur a touché une zone mortelle (fige le jeu jusqu'au Stop).
    lost: bool,
    /// Score : nombre total de pièces ramassées dans la partie (bonus respawn inclus).
    score: u32,
    /// File de réapparition : (index de pièce, temps de jeu auquel la rendre visible).
    respawn_queue: Vec<(usize, f32)>,
    /// Niveau courant de la démo contrôleur (1-based).
    level: u32,
    /// « Aperçu mobile » : restreint la vue 3D à un écran de téléphone (letterbox).
    pub device_preview: bool,
    /// Orientation de l'aperçu mobile (portrait par défaut).
    pub device_portrait: bool,
    /// Région centrale 3D (hors panneaux) en pixels physiques `(x, y, w, h)`,
    /// remontée par l'éditeur ; base de l'aperçu mobile. `(0,0,0,0)` = plein écran.
    pub view_rect_px: (f32, f32, f32, f32),
    /// Barre de vie du HUD (0..1) pilotée par `set_health` ; `None` = pas de barre.
    pub hud_health: Option<f32>,
    /// Qualité de rendu visée (cf. `build_config::RenderQuality`) : relue depuis la
    /// config persistée à chaque entrée en Play, pilote le nombre de lumières
    /// ponctuelles envoyées au shader (perf en mode interactif « Basse » qualité).
    pub render_quality: crate::app::build_config::RenderQuality,
    /// Intensité (1 = pic, décroît vers 0) du flash de dégâts (vignette rouge HUD),
    /// déclenché quand `hud_health` baisse. Purement cosmétique (retour de coup).
    pub damage_flash: f32,
    /// Intensité (1 = pic, décroît vers 0) de l'effet 3D d'attaque : téléporte et affiche
    /// brièvement l'objet `is_attack_fx` sur la cible touchée (rend le coup lisible).
    pub attack_flash: f32,
    /// Manche courante (1-based) d'un système de vagues (cf. `Combat::wave`) ; 0 = pas
    /// de système de manches dans la scène courante (les autres démos). Toutes les
    /// cibles de la manche courante vaincues ⇒ manche suivante révélée ; dernière
    /// manche vaincue ⇒ victoire (cf. `update_waves` dans `advance_play`).
    pub wave: u32,
    /// La scène courante est-elle la démo contrôleur à niveaux (`Scene::controller_level`) ?
    /// Seule cette famille de scènes a un « niveau suivant » (cf. `next_level`) ; toute
    /// autre victoire (course infinie, tour, manches de zombies...) doit juste relancer
    /// la même scène au clic sur « Rejouer », pas basculer vers l'arène de combat.
    pub is_leveled_demo: bool,
    /// Temps restant (s) avant la prochaine attaque possible (cf. `Controller::attack_cooldown`).
    /// Sans ce temporisateur, maintenir le bouton défait instantanément tout ce qui entre
    /// en portée, sans le moindre risque — verrouillé par un test dédié.
    attack_cooldown_remaining: f32,
    /// Missile d'attaque en vol (cf. `Scene::attack_at` → tir à distance) : `None` = pas
    /// de tir en cours. L'impact réel (mise à mort) n'est résolu qu'à l'arrivée, pas au
    /// moment du tir — laisse le temps à la cible de continuer d'approcher, donc de
    /// mordre avant que le coup ne porte (le vrai risque qu'une résolution instantanée
    /// ne pouvait pas garantir, cf. audit_sprint.md).
    attack_projectile: Option<AttackProjectile>,
    /// Préparation d'attaque en cours (cf. `Controller::attack_windup`) : `None` = pas de
    /// tir en préparation. La cible est verrouillée dès l'appui, mais le missile ne part
    /// qu'une fois le temps de préparation écoulé — le joueur reste exposé pendant ce
    /// temps (aucune invulnérabilité), créant enfin un vrai risque en 1 contre 1 (cf.
    /// audit_sprint.md : le temps de vol du missile seul ne suffisait pas).
    attack_charge: Option<AttackCharge>,
    /// Reculs (knockback) en cours : (indice d'objet, vitesse horizontale, temps restant
    /// en s) — cf. `KNOCKBACK_SPEED`/`KNOCKBACK_DURATION`. Prioritaire sur le pilotage
    /// IA tant que le temps n'est pas écoulé (sinon la poursuite écraserait le recul dès
    /// la frame suivante).
    stagger: Vec<(usize, Vec3, f32)>,
    /// Joueurs réseau connectés (cf. `multiplayer.rs`, Sprint 55) : indice de
    /// l'objet de scène que chacun pilote, dans `scene.objects`.
    network_players: HashMap<crate::net::protocol::PlayerId, usize>,
    /// Dernier `Input` reçu de chaque joueur réseau (remplacé, pas cumulé : le
    /// client renvoie son état complet à chaque message).
    network_inputs: HashMap<crate::net::protocol::PlayerId, multiplayer::NetworkInput>,
    /// Temps de recharge (s) restant avant la prochaine attaque possible de
    /// chaque joueur réseau (cf. `multiplayer::update_network_attacks`, Sprint 60).
    network_attack_cooldowns: HashMap<crate::net::protocol::PlayerId, f32>,
    /// Grille de référence au sol affichée en mode édition.
    pub show_grid: bool,
    /// Aimantation : les translations au gizmo s'alignent sur la grille (pas de 0.5).
    pub snap: bool,
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
    /// Transforms d'origine de la sélection (gizmo rotate/scale multi, autour d'un pivot).
    drag_orig_transforms: Vec<(usize, Transform)>,
    /// Pivot commun (centroïde de la sélection) pour rotate/scale multi.
    drag_pivot: Vec3,
    /// Le prochain clic ajoute/retire de la sélection (Cmd/Maj enfoncé).
    additive: bool,
    /// Lumière ponctuelle sélectionnée (déplaçable au gizmo) ; exclusif avec `selection`.
    pub selected_light: Option<usize>,
    /// Lumière en cours de déplacement au gizmo (avec `active_axis`).
    drag_light: Option<usize>,

    // --- historique (snapshots de la liste d'objets) ---
    undo_stack: VecDeque<SceneSnapshot>,
    redo_stack: Vec<SceneSnapshot>,

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
    // --- génération de scène entière par IA (asynchrone) ---
    ai_scene_tx: Sender<Result<Scene, String>>,
    ai_scene_rx: Receiver<Result<Scene, String>>,
    /// Mode de la génération de scène en cours : `true` = remplacer, `false` = ajouter.
    ai_scene_replace: bool,
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
        let (ai_scene_tx, ai_scene_rx) = channel();
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
            sim_accumulator: 0.0,
            win_time: None,
            lost: false,
            score: 0,
            respawn_queue: Vec::new(),
            level: 1,
            device_preview: false,
            device_portrait: true,
            view_rect_px: (0.0, 0.0, 0.0, 0.0),
            hud_health: None,
            render_quality: crate::app::build_config::BuildConfig::load().render_quality,
            damage_flash: 0.0,
            attack_flash: 0.0,
            wave: 0,
            is_leveled_demo: false,
            attack_cooldown_remaining: 0.0,
            attack_projectile: None,
            attack_charge: None,
            stagger: Vec::new(),
            network_players: HashMap::new(),
            network_inputs: HashMap::new(),
            network_attack_cooldowns: HashMap::new(),
            show_grid: true,
            snap: false,
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
            drag_orig_transforms: Vec::new(),
            drag_pivot: Vec3::ZERO,
            additive: false,
            selected_light: None,
            drag_light: None,
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
            ai_scene_tx,
            ai_scene_rx,
            ai_scene_replace: true,
        }
    }

    /// Lance une génération de scène par IA (thread de fond). `replace` = remplace la
    /// scène ; sinon ajoute les objets générés à la scène actuelle.
    pub fn request_ai_scene(&mut self, req: ai::AiRequest, replace: bool) {
        if self.ai_busy {
            return;
        }
        self.ai_busy = true;
        self.ai_scene_replace = replace;
        let tx = self.ai_scene_tx.clone();
        std::thread::spawn(move || {
            let result = ai::generate_scene_json(&req).and_then(|j| Scene::from_ai_json(&j));
            let _ = tx.send(result);
        });
    }

    /// Lance une génération de script Lua par IA (thread de fond) pour l'objet `idx`.
    pub fn request_ai_script(&mut self, idx: usize, req: ai::AiRequest) {
        if self.ai_busy {
            return;
        }
        self.ai_busy = true;
        let tx = self.ai_tx.clone();
        std::thread::spawn(move || {
            let result = ai::generate_lua(&req);
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
        while let Ok(result) = self.ai_scene_rx.try_recv() {
            self.ai_busy = false;
            match result {
                Ok(mut scene) => {
                    self.push_undo();
                    if self.ai_scene_replace {
                        self.scene = scene;
                        log::info!("Scène générée par IA appliquée");
                    } else {
                        // Ajout : on intègre les objets et lumières générés à la scène actuelle.
                        let n = scene.objects.len();
                        self.scene.objects.append(&mut scene.objects);
                        self.scene.point_lights.append(&mut scene.point_lights);
                        log::info!("{n} objet(s) ajouté(s) par IA à la scène");
                    }
                    self.imported_dirty = true;
                    self.clear_selection();
                }
                Err(e) => log::error!("Génération de scène IA : {e}"),
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

    /// Déplace l'objet `from` juste avant l'objet `to` dans l'ordre global
    /// (glisser-déposer de réordonnancement dans la hiérarchie). Passe par l'historique.
    pub fn reorder_object(&mut self, from: usize, to: usize) {
        let n = self.scene.objects.len();
        if from >= n || to >= n || from == to {
            return;
        }
        self.push_undo();
        let obj = self.scene.objects.remove(from);
        // Après le retrait, l'index cible se décale si `from` était avant lui.
        let dest = if from < to { to - 1 } else { to };
        self.scene.objects.insert(dest, obj);
        self.select_single(dest);
    }

    // --- sélection (primaire + ensemble) ---

    /// Mémorise les transforms d'origine de la sélection + leur centroïde (pivot),
    /// pour les manipulations multi-objets rotate/scale.
    fn capture_drag_selection(&mut self) {
        self.drag_orig_transforms = self
            .selected
            .iter()
            .filter_map(|&i| self.scene.objects.get(i).map(|o| (i, o.transform)))
            .collect();
        let n = self.drag_orig_transforms.len().max(1) as f32;
        let sum: Vec3 = self
            .drag_orig_transforms
            .iter()
            .map(|(_, t)| t.position)
            .sum();
        self.drag_pivot = sum / n;
    }

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

    /// Couper : copie la sélection puis la supprime.
    pub fn cut_selected(&mut self) {
        self.copy_selected();
        self.delete_selected();
    }

    /// Sélectionne tous les objets de la scène.
    pub fn select_all(&mut self) {
        self.selected = (0..self.scene.objects.len()).collect();
        self.selection = self.selected.last().copied();
    }

    /// Répartit les objets sélectionnés à intervalles égaux le long d'un axe
    /// (extrémités conservées). Nécessite au moins 3 objets.
    pub fn distribute_selection_axis(&mut self, axis: usize) {
        let comp = |p: Vec3| match axis {
            0 => p.x,
            1 => p.y,
            _ => p.z,
        };
        // (index, valeur sur l'axe), triés par valeur.
        let mut items: Vec<(usize, f32)> = self
            .selected
            .iter()
            .filter_map(|&i| {
                self.scene
                    .objects
                    .get(i)
                    .map(|o| (i, comp(o.transform.position)))
            })
            .collect();
        if items.len() < 3 {
            return;
        }
        items.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (min, max) = (items[0].1, items[items.len() - 1].1);
        let step = (max - min) / (items.len() - 1) as f32;
        self.push_undo();
        for (rank, (idx, _)) in items.iter().enumerate() {
            let v = min + step * rank as f32;
            if let Some(o) = self.scene.objects.get_mut(*idx) {
                match axis {
                    0 => o.transform.position.x = v,
                    1 => o.transform.position.y = v,
                    _ => o.transform.position.z = v,
                }
            }
        }
    }

    /// Aligne la position des objets sélectionnés sur celle de la primaire, le long
    /// d'un axe (0 = X, 1 = Y, 2 = Z).
    pub fn align_selection_axis(&mut self, axis: usize) {
        let Some(primary) = self.selection else {
            return;
        };
        if self.selected.len() < 2 {
            return;
        }
        let Some(target) = self
            .scene
            .objects
            .get(primary)
            .map(|o| o.transform.position)
        else {
            return;
        };
        self.push_undo();
        for &i in &self.selected {
            if let Some(o) = self.scene.objects.get_mut(i) {
                match axis {
                    0 => o.transform.position.x = target.x,
                    1 => o.transform.position.y = target.y,
                    _ => o.transform.position.z = target.z,
                }
            }
        }
    }

    /// Regroupe les objets sélectionnés dans un nouveau groupe nommé automatiquement.
    pub fn group_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let name = format!("Groupe {}", self.scene.groups.len() + 1);
        for &i in &self.selected {
            if let Some(o) = self.scene.objects.get_mut(i) {
                o.group = name.clone();
            }
        }
        if !self.scene.groups.contains(&name) {
            self.scene.groups.push(name);
        }
    }

    /// Retire les objets sélectionnés de leur groupe (« Sans groupe »).
    pub fn ungroup_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        for &i in &self.selected {
            if let Some(o) = self.scene.objects.get_mut(i) {
                o.group.clear();
            }
        }
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

    /// Capture l'état courant de la scène avant une modification (vide la pile redo).
    pub fn push_undo(&mut self) {
        self.undo_stack
            .push_back(SceneSnapshot::capture(&self.scene));
        if self.undo_stack.len() > 50 {
            self.undo_stack.pop_front(); // O(1), contrairement à Vec::remove(0)
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop_back() {
            self.redo_stack.push(SceneSnapshot::capture(&self.scene));
            prev.restore(&mut self.scene);
            self.clear_selection();
            self.selected_light = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack
                .push_back(SceneSnapshot::capture(&self.scene));
            next.restore(&mut self.scene);
            self.clear_selection();
            self.selected_light = None;
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
            collider_shape: crate::runtime::physics::ColliderShape::Auto,
            group: String::new(),
            color: [1.0, 1.0, 1.0],
            texture: String::new(),
            tappable: false,
            metallic: 0.0,
            roughness: 0.6,
            emissive: 0.0,
            trigger: false,
            ..Default::default()
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
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Définit la caméra de jeu depuis le point de vue actuel (orbite éditeur).
    pub fn set_game_camera(&mut self) {
        self.push_undo();
        self.scene.game_camera = Some(GameCamera {
            target: self.camera.target.to_array(),
            yaw: self.camera.yaw,
            pitch: self.camera.pitch,
            distance: self.camera.distance,
        });
        log::info!("Caméra de jeu définie sur la vue actuelle");
    }

    /// Retire la caméra de jeu (la vue Play repart de l'orbite éditeur).
    pub fn clear_game_camera(&mut self) {
        self.push_undo();
        self.scene.game_camera = None;
    }

    /// Réduit sur disque les textures dépassant `max_px` (côté le plus long), écrit
    /// une copie `…_opt.png` et met à jour les objets. Renvoie le nombre de textures réduites.
    pub fn optimize_textures(&mut self, max_px: u32) -> usize {
        use std::collections::HashMap;
        // chemins uniques utilisés par la scène
        let mut paths: Vec<String> = self
            .scene
            .objects
            .iter()
            .map(|o| o.texture.clone())
            .filter(|t| !t.is_empty())
            .collect();
        paths.sort();
        paths.dedup();

        let mut remap: HashMap<String, String> = HashMap::new();
        for path in paths {
            let Some(bytes) = crate::assets::read_bytes(&path) else {
                log::error!("Texture illisible {path}");
                continue;
            };
            let Ok(img) = image::load_from_memory(&bytes) else {
                log::error!("Texture non décodable {path}");
                continue;
            };
            let (w, h) = (img.width(), img.height());
            if w <= max_px && h <= max_px {
                continue;
            }
            let scale = max_px as f32 / w.max(h) as f32;
            let (nw, nh) = (
                ((w as f32 * scale) as u32).max(1),
                ((h as f32 * scale) as u32).max(1),
            );
            let resized = img.resize(nw, nh, image::imageops::FilterType::Lanczos3);
            let out = optimized_path(&path, max_px);
            // `asset://x.png` → écrit dans le dossier d'assets ; chemin disque → à côté.
            let write_path = match crate::assets::assets_dir() {
                Some(dir) if out.starts_with(crate::assets::ASSET_SCHEME) => dir
                    .join(out.trim_start_matches(crate::assets::ASSET_SCHEME))
                    .to_string_lossy()
                    .into_owned(),
                _ => out.clone(),
            };
            if let Err(e) = resized.save(&write_path) {
                log::error!("Échec écriture texture optimisée {write_path} : {e}");
                continue;
            }
            log::info!("Texture {path} ({w}×{h}) → {out} ({nw}×{nh})");
            remap.insert(path, out);
        }
        if remap.is_empty() {
            return 0;
        }
        self.push_undo();
        for o in &mut self.scene.objects {
            if let Some(new) = remap.get(&o.texture) {
                o.texture = new.clone();
            }
        }
        remap.len()
    }

    /// Rassemble les assets externes (textures, sons, modèles) dans le dossier de
    /// projet et réécrit les chemins en `asset://…` (portable). Renvoie le nombre réécrit.
    pub fn collect_assets(&mut self) -> usize {
        let is_external = |p: &str| {
            !p.is_empty()
                && !p.starts_with(crate::assets::ASSET_SCHEME)
                && !p.starts_with(crate::assets::SCHEME)
        };
        let any = self.scene.objects.iter().any(|o| {
            is_external(&o.texture) || o.audio.as_ref().is_some_and(|a| is_external(&a.clip))
        }) || self.scene.imported.iter().any(|m| is_external(&m.path));
        if !any {
            return 0;
        }
        self.push_undo();
        let mut changed = 0;
        let mut import = |p: &mut String| {
            if is_external(p)
                && let Some(a) = crate::assets::import_to_assets(p)
            {
                *p = a;
                changed += 1;
            }
        };
        for o in &mut self.scene.objects {
            import(&mut o.texture);
            if let Some(a) = &mut o.audio {
                import(&mut a.clip);
            }
        }
        for m in &mut self.scene.imported {
            import(&mut m.path);
        }
        changed
    }

    /// Limite le nombre de lumières ponctuelles (optimisation mobile).
    pub fn limit_point_lights(&mut self, max: usize) {
        if self.scene.point_lights.len() > max {
            self.push_undo();
            self.scene.point_lights.truncate(max);
        }
    }

    /// Convertisseur de textures : redimensionne chaque texture aux **puissances de 2**
    /// inférieures (mip-mapping/compression GPU mobile). Écrit des copies `…_pot.png`
    /// et met à jour les objets. Renvoie le nombre de textures converties.
    pub fn convert_textures_pot(&mut self) -> usize {
        use std::collections::HashMap;
        let mut paths: Vec<String> = self
            .scene
            .objects
            .iter()
            .map(|o| o.texture.clone())
            .filter(|t| !t.is_empty())
            .collect();
        paths.sort();
        paths.dedup();

        // Plus grande puissance de 2 ≤ v (bornée à [1, 4096]).
        let pot = |v: u32| -> u32 {
            if v < 2 {
                return 1;
            }
            (1u32 << (31 - v.leading_zeros())).clamp(1, 4096)
        };

        let mut remap: HashMap<String, String> = HashMap::new();
        for path in paths {
            let Some(bytes) = crate::assets::read_bytes(&path) else {
                log::error!("Texture illisible {path}");
                continue;
            };
            let Ok(img) = image::load_from_memory(&bytes) else {
                log::error!("Texture non décodable {path}");
                continue;
            };
            let (w, h) = (img.width(), img.height());
            let (nw, nh) = (pot(w), pot(h));
            if nw == w && nh == h {
                continue; // déjà en puissances de 2
            }
            let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
            let out = format!("{path}_pot.png");
            let write_path = match crate::assets::assets_dir() {
                Some(dir) if out.starts_with(crate::assets::ASSET_SCHEME) => dir
                    .join(out.trim_start_matches(crate::assets::ASSET_SCHEME))
                    .to_string_lossy()
                    .into_owned(),
                _ => out.clone(),
            };
            if let Err(e) = resized.save(&write_path) {
                log::error!("Échec écriture texture POT {write_path} : {e}");
                continue;
            }
            log::info!("Texture {path} ({w}×{h}) → {out} ({nw}×{nh}) [POT]");
            remap.insert(path, out);
        }
        if remap.is_empty() {
            return 0;
        }
        self.push_undo();
        for o in &mut self.scene.objects {
            if let Some(new) = remap.get(&o.texture) {
                o.texture = new.clone();
            }
        }
        remap.len()
    }

    /// Bake lighting : fige la contribution des lumières **ponctuelles** dans l'émission
    /// statique de chaque objet (selon distance/portée), puis les supprime. Réduit le
    /// nombre de lumières dynamiques (coût GPU mobile). Renvoie le nombre de lumières figées.
    pub fn bake_lighting(&mut self) -> usize {
        let lights = self.scene.point_lights.clone();
        if lights.is_empty() {
            return 0;
        }
        self.push_undo();
        for o in &mut self.scene.objects {
            let p = o.transform.position;
            let mut add = 0.0f32;
            for l in &lights {
                let lp = glam::Vec3::from(l.position);
                let d = (lp - p).length();
                if d < l.range {
                    let falloff = 1.0 - d / l.range; // atténuation linéaire simple
                    // Luminance approximative de la lumière.
                    let lum = (l.color[0] + l.color[1] + l.color[2]) / 3.0;
                    add += l.intensity * falloff * lum;
                }
            }
            o.emissive = (o.emissive + add * 0.6).clamp(0.0, 3.0);
        }
        let n = lights.len();
        self.scene.point_lights.clear();
        n
    }

    /// Recentre la caméra sur l'objet (ou la lumière) sélectionné (« frame selected », touche F).
    pub fn frame_selected(&mut self) {
        let target = self
            .selection
            .and_then(|i| self.scene.objects.get(i))
            .map(|o| o.transform.position)
            .or_else(|| {
                self.selected_light
                    .and_then(|i| self.scene.point_lights.get(i))
                    .map(|pl| Vec3::from_array(pl.position))
            });
        if let Some(t) = target {
            self.camera.target = t;
        }
    }

    /// Charge la démo gameplay complète (joystick/gyro/saut/zone/vie/tap).
    pub fn load_gameplay_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::gameplay_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Recommence la partie en cours (mode Play) : restaure la scène d'origine,
    /// reconstruit la physique et remet à zéro chrono/victoire/défaite. Permet de
    /// « Rejouer » depuis le jeu lui-même (essentiel sur APK, sans bouton Stop éditeur).
    pub fn restart_game(&mut self) {
        if self.play_snapshot.is_empty() {
            return;
        }
        self.scene.objects = self.play_snapshot.clone();
        // cf. AUDIT_MMORPG.md §4.2 : `play_snapshot` ne connaît pas les objets
        // ajoutés en cours de partie par `spawn_network_player` — sans ce
        // nettoyage, `network_players` pointerait vers des indices obsolètes
        // après la restauration.
        self.clear_network_players();
        self.time = 0.0;
        self.sim_accumulator = 0.0;
        self.win_time = None;
        self.lost = false;
        self.score = 0;
        self.respawn_queue.clear();
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.attack_cooldown_remaining = 0.0;
        self.attack_projectile = None;
        self.attack_charge = None;
        self.stagger.clear();
        self.tapped_obj = None;
        // Remet la manche 1 (révèle ses monstres, masque les suivantes) *avant* de
        // reconstruire la physique, pour que les corps rigides des monstres masqués ne
        // soient pas créés (cf. `init_waves`).
        self.init_waves();
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        if self.scene.camera_follow
            && let Some(p) = self.player_position()
        {
            self.camera.target = p + Vec3::new(0.0, 0.8, 0.0);
            if self.scene.game_camera.is_none() {
                self.camera.pitch = 0.62;
                self.camera.distance = 11.0;
            }
        }
    }

    /// A-t-on gagné le niveau (toutes les pièces-objectif ramassées) ?
    pub fn has_won(&self) -> bool {
        self.win_time.is_some()
    }

    /// Score courant (pièces ramassées) — affiché au HUD.
    pub fn score(&self) -> u32 {
        self.score
    }

    /// Passe au niveau suivant (boucle au niveau 1 après le dernier) et le charge en Play.
    pub fn next_level(&mut self) {
        self.level = self.level % crate::scene::CONTROLLER_LEVELS + 1;
        self.scene = crate::scene::Scene::controller_level(self.level);
        self.imported_dirty = true;
        self.is_leveled_demo = true;
        // Repart « en jeu » sur le nouveau niveau.
        self.play_snapshot = self.scene.objects.clone();
        self.restart_game();
    }

    /// Charge la démo « contrôleur » : joueur pilotable au joystick + saut, sans script.
    pub fn load_controller_demo(&mut self) {
        self.level = 1;
        self.push_undo();
        self.scene = Scene::controller_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = true;
        self.clear_selection();
    }

    /// Charge la démo « Tour d'ascension » (cf. `Scene::tower_demo`) : style de jeu
    /// différent de la démo contrôleur — platforming vertical pur, sans combat.
    pub fn load_tower_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::tower_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Course infinie » (cf. `Scene::temple_run_demo`) : 3ᵉ style de jeu
    /// — course automatique, changement de voie, obstacles à esquiver/sauter.
    pub fn load_temple_run_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::temple_run_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Vagues de zombies » (cf. `Scene::zombies_demo`) : jeu local
    /// contre l'ordinateur, sans réseau — manches de monstres poursuivant le joueur.
    pub fn load_zombies_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::zombies_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Donjon » façon roguelike (cf. `Scene::roguelike_demo`) : 3 salles
    /// à vider une à une (portes fermées jusqu'à la manche suivante), arme de départ
    /// tirée au sort parmi 3 profils à chaque chargement.
    pub fn load_roguelike_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::roguelike_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Duel » façon Tekken/Smash Bros (cf. `Scene::brawl_demo`) : arène
    /// flottante, un seul rival à plusieurs points de vie, à achever ou à sortir de
    /// l'arène (ring out).
    pub fn load_brawl_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::brawl_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la scène **exemple** des composants optionnels (cf. `Scene::components_demo`) :
    /// Controller/AudioSource/Combat, un seul chacun, pour référence rapide (pas un niveau).
    pub fn load_components_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::components_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
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
                // Gizmo de translation d'une lumière sélectionnée.
                if let (Some(li), Some((cx, cy))) = (self.selected_light, self.last_cursor)
                    && let Some(pl) = self.scene.point_lights.get(li)
                {
                    let origin = Vec3::from_array(pl.position);
                    if let Some(axis) = self.pick_axis_at(origin, cx, cy) {
                        if let Some(p) = self.axis_drag_param(origin, axis_dir(axis), cx, cy) {
                            self.push_undo(); // déplacement de lumière annulable
                            self.active_axis = Some(axis);
                            self.drag_light = Some(li);
                            self.drag_start_t = p;
                            self.drag_orig_pos = origin;
                        }
                        return;
                    }
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
                                    self.capture_drag_selection();
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
                                    self.capture_drag_selection();
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
                    self.drag_light = None;
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
                    // Priorité au marqueur de lumière (petit), sinon objet 3D.
                    if let Some(li) = self.pick_light(cx, cy) {
                        self.selected_light = Some(li);
                        self.clear_selection();
                    } else {
                        self.selected_light = None;
                        match self.pick(cx, cy) {
                            // Cmd/Maj : ajoute/retire de la sélection ; sinon sélection simple.
                            Some(i) if self.additive => self.toggle_select(i),
                            Some(i) => self.select_single(i),
                            None if !self.additive => self.clear_selection(),
                            None => {}
                        }
                    }
                }
                self.press_cursor = None;
            }
            InputEvent::PointerMove { x, y } => {
                // Déplacement d'une lumière sélectionnée (translate uniquement).
                if let (Some(axis), Some(li)) = (self.active_axis, self.drag_light) {
                    let a = axis_dir(axis);
                    if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y)
                        && let Some(pl) = self.scene.point_lights.get_mut(li)
                    {
                        let delta = a * (t - self.drag_start_t);
                        pl.position = maybe_snap(self.drag_orig_pos + delta, self.snap).to_array();
                    }
                    self.last_cursor = Some((x, y));
                    return;
                }
                // manipulation via la poignée active
                if let (Some(axis), Some(sel)) = (self.active_axis, self.selection) {
                    let a = axis_dir(axis);
                    match self.gizmo_mode {
                        GizmoMode::Translate => {
                            if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y) {
                                let delta = a * (t - self.drag_start_t);
                                let snap = self.snap;
                                if self.drag_orig_positions.len() > 1 {
                                    // déplace toute la sélection en bloc
                                    for (i, orig) in &self.drag_orig_positions {
                                        if let Some(o) = self.scene.objects.get_mut(*i) {
                                            o.transform.position = maybe_snap(*orig + delta, snap);
                                        }
                                    }
                                } else {
                                    self.scene.objects[sel].transform.position =
                                        maybe_snap(self.drag_orig_pos + delta, snap);
                                }
                            }
                        }
                        GizmoMode::Scale => {
                            if let Some(t) = self.axis_drag_param(self.drag_orig_pos, a, x, y) {
                                let d = t - self.drag_start_t;
                                // Même delta appliqué à chaque objet sélectionné (multi-scale).
                                for (i, t0) in &self.drag_orig_transforms {
                                    if let Some(o) = self.scene.objects.get_mut(*i) {
                                        let mut s = t0.scale;
                                        match axis {
                                            0 => s.x = (s.x + d).max(0.05),
                                            1 => s.y = (s.y + d).max(0.05),
                                            _ => s.z = (s.z + d).max(0.05),
                                        }
                                        o.transform.scale = s;
                                    }
                                }
                            }
                        }
                        GizmoMode::Rotate => {
                            if let Some(ang) = self.ring_drag_angle(self.drag_orig_pos, a, x, y) {
                                let delta = ang - self.drag_start_angle;
                                let rot = Quat::from_axis_angle(a, delta);
                                // Rotation autour du pivot commun (position + orientation).
                                let pivot = self.drag_pivot;
                                for (i, t0) in &self.drag_orig_transforms {
                                    if let Some(o) = self.scene.objects.get_mut(*i) {
                                        o.transform.rotation = rot * t0.rotation;
                                        o.transform.position = pivot + rot * (t0.position - pivot);
                                    }
                                }
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
        self.pick_axis_at(self.scene.objects[sel].transform.position, px, py)
    }

    /// Axe du gizmo de translation sous le curseur, pour une origine quelconque (~10 px).
    fn pick_axis_at(&self, origin: Vec3, px: f64, py: f64) -> Option<usize> {
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

    /// Lumière ponctuelle dont le marqueur est sous le curseur (~14 px), ou None.
    fn pick_light(&self, px: f64, py: f64) -> Option<usize> {
        let vp = self.camera.view_proj();
        let mut best: Option<(f64, usize)> = None;
        for (i, pl) in self.scene.point_lights.iter().enumerate() {
            if let Some((sx, sy)) = self.project_with(vp, Vec3::from_array(pl.position)) {
                let d = (px - sx).hypot(py - sy);
                if d < 14.0 && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
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
            // Manche 1 révélée, suivantes masquées, *avant* de construire la physique
            // (cf. `init_waves` : les monstres masqués n'ont pas de corps rigide).
            self.init_waves();
            self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
            // sons en autoplay (gain atténué par la distance à la caméra si spatialisé)
            let listener = self.camera.target;
            let clips: Vec<(String, f32)> = self
                .scene
                .objects
                .iter()
                .filter_map(|o| {
                    let a = o.audio.as_ref()?;
                    if !a.autoplay || a.clip.is_empty() {
                        return None;
                    }
                    let gain = if a.spatial {
                        let dist = (o.transform.position - listener).length();
                        (1.0 - dist / 20.0).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    Some((a.clip.clone(), gain))
                })
                .collect();
            for (c, gain) in clips {
                self.audio.play_gain(&c, gain);
            }
            // Caméra de suivi : se cale d'emblée sur le joueur + adopte un bon angle de
            // jeu 3ᵉ personne (plongée douce + recul confortable) si aucune caméra de jeu
            // n'est définie.
            if self.scene.camera_follow
                && let Some(p) = self.player_position()
            {
                self.camera.target = p + Vec3::new(0.0, 0.8, 0.0);
                if self.scene.game_camera.is_none() {
                    self.camera.pitch = 0.62; // ~35° de plongée
                    self.camera.distance = 11.0;
                }
            }
            // Caméra de jeu : applique le point de vue défini pour la scène.
            if let Some(gc) = self.scene.game_camera {
                self.camera.yaw = gc.yaw;
                self.camera.pitch = gc.pitch;
                self.camera.distance = gc.distance;
                if !self.scene.camera_follow {
                    self.camera.target = Vec3::from_array(gc.target);
                }
            }
        } else if !self.playing && self.was_playing {
            self.scene.objects = self.play_snapshot.clone();
            // cf. AUDIT_MMORPG.md §4.2 : même raison qu'à `restart_game`.
            self.clear_network_players();
            self.physics = None;
            self.paused = false;
            self.hud_health = None;
            self.damage_flash = 0.0;
            self.attack_flash = 0.0;
            self.attack_cooldown_remaining = 0.0;
            self.attack_projectile = None;
            self.attack_charge = None;
            self.stagger.clear();
            self.wave = 0;
            self.win_time = None;
            self.lost = false;
            self.clear_selection();
            self.audio.stop_all();
        }
        if self.playing && !self.was_playing {
            // Démarrage de Play : repart d'un accumulateur vide (pas de rafale initiale).
            self.sim_accumulator = 0.0;
            self.win_time = None;
            self.lost = false;
            self.score = 0;
            self.respawn_queue.clear();
            self.time = 0.0;
            // Relit la qualité visée (modifiable dans le panneau Export sans redémarrer
            // l'app) : s'applique dès ce lancement de Play, pas seulement au build exporté.
            self.render_quality = crate::app::build_config::BuildConfig::load().render_quality;
        }
        self.was_playing = self.playing;

        // En pause : on reste en mode Play (snapshot conservé) mais on gèle la
        // simulation (ni scripts, ni physique, ni avance du temps).
        if !self.playing || self.paused {
            self.sim_accumulator = 0.0;
            return;
        }

        // --- Simulation découplée du rendu : pas de temps FIXE (Sprint 45) ---
        // On accumule le temps réel écoulé et on simule par incréments fixes, quel que
        // soit le framerate → physique et scripts déterministes, indépendants du FPS.
        const FIXED_DT: f32 = 1.0 / 60.0;
        const MAX_SUBSTEPS: u32 = 5;
        // Jeu figé une fois gagné ou perdu (l'écran de fin attend « Rejouer »).
        if !self.lost && self.win_time.is_none() {
            let (steps, acc) = fixed_substeps(self.sim_accumulator, dt, FIXED_DT, MAX_SUBSTEPS);
            self.sim_accumulator = acc;
            for _ in 0..steps {
                self.sim_step(FIXED_DT);
            }

            // Ramassage par contact : le joueur récupère les pièces qu'il traverse.
            // Score +1 par pièce ; les pièces bonus (respawn_delay>0) réapparaissent.
            if let Some(p) = self.player_position() {
                let now = self.time;
                let hit = self.scene.collect_at(p, 0.7);
                if !hit.is_empty() {
                    self.score += hit.len() as u32;
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Pickup);
                    for i in hit {
                        let d = self.scene.objects[i].respawn_delay;
                        if d > 0.0 {
                            self.respawn_queue.push((i, now + d));
                        }
                    }
                }
            }
            // Ramassage d'arme par contact (cf. `WeaponPickup`, donjon roguelike) :
            // équipe le nouveau profil sur le joueur et score +1, comme une pièce —
            // mais **natif** (pas un script Lua, qui ne peut pas modifier `Controller`).
            if let Some(pi) = self.player_index() {
                let p = self.scene.objects[pi].transform.position;
                if let Some(w) = self.scene.weapon_pickup_at(p, 0.9) {
                    if let Some(ctrl) = self.scene.objects[pi].controller.as_mut() {
                        ctrl.attack_range = w.range;
                        ctrl.attack_cooldown = w.cooldown;
                        ctrl.attack_windup = w.windup;
                        ctrl.attack_mode = w.mode;
                    }
                    self.score += 1;
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Pickup);
                    log::info!(
                        "Arme trouvée : « {} » équipée (portée {:.1} m, recharge {:.2} s, préparation {:.2} s)",
                        w.label,
                        w.range,
                        w.cooldown,
                        w.windup
                    );
                }
            }
            self.update_attack(dt);
            self.update_network_attacks(dt);
            // Réapparition des pièces bonus dont le délai est écoulé.
            let now = self.time;
            self.respawn_queue.retain(|&(i, at)| {
                if now >= at {
                    if let Some(o) = self.scene.objects.get_mut(i) {
                        o.visible = true;
                    }
                    false
                } else {
                    true
                }
            });
            // Défaite : le joueur a touché une zone mortelle (mort instantanée, ex. lave).
            if !self.lost
                && let Some(p) = self.player_position()
                && self.scene.deadly_at(p)
            {
                self.lost = true;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
            }
            self.check_ring_outs();
            // Défaite : la vie (dégâts cumulés des ennemis via `damage()`) est tombée à 0.
            // Contrairement aux zones mortelles, les ennemis punissent par usure (dégâts
            // progressifs + régénération hors contact), plus indulgent qu'une mort au tap.
            if !self.lost
                && let Some(h) = self.hud_health
                && h <= 0.0
            {
                self.lost = true;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
            }
            // Victoire : fige le chrono quand toutes les pièces-objectif sont ramassées.
            if self.win_time.is_none()
                && let Some((c, t)) = self.scene.collectibles()
                && c == t
            {
                self.win_time = Some(self.time);
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Win);
            }
            // Système de manches (cf. `Combat::wave`) : révèle la manche suivante une
            // fois la courante vidée, ou déclenche la victoire à la dernière. N'a aucun
            // effet si la scène n'a pas de monstres à manches (self.wave == 0).
            self.update_waves();
        }

        // Caméra qui suit le joueur — au niveau frame (lissage visuel), avec le dt réel.
        // Cible légèrement au-dessus du joueur (regarde le buste, voit plus loin devant).
        if self.scene.camera_follow
            && let Some(p) = self.player_position()
        {
            let t = (dt * 6.0).min(1.0);
            self.camera.target = self.camera.target.lerp(p + Vec3::new(0.0, 0.8, 0.0), t);
        }

        // Décroissance du flash de dégâts (~0,4 s), au niveau frame comme la caméra.
        if self.damage_flash > 0.0 {
            self.damage_flash = (self.damage_flash - dt * 2.5).max(0.0);
        }
        // Décroissance de l'effet d'attaque (~0,33 s) : rétrécit l'ancre `is_attack_fx`
        // jusqu'à disparition, puis la remasque pour ne pas polluer le prochain coup.
        if self.attack_flash > 0.0 {
            self.attack_flash = (self.attack_flash - dt * 3.0).max(0.0);
            if let Some(fx) = self.attack_fx_index()
                && let Some(o) = self.scene.objects.get_mut(fx)
            {
                if self.attack_flash <= 0.0 {
                    o.visible = false;
                } else {
                    o.transform.scale = Vec3::splat(0.25 + 0.95 * self.attack_flash);
                }
            }
        }
    }

    /// Un pas de simulation à **dt fixe** : scripts Lua, actions au tap, pilotage des
    /// objets pilotables et pas de physique. Appelé 0..N fois par frame (cf. `advance_play`).
    fn sim_step(&mut self, dt: f32) {
        // 1. scripts
        self.time += dt;
        let time = self.time;
        // Zones de déclenchement : objets `trigger` visibles dont l'AABB monde touche
        // celui du joueur. Test d'*intersection* de volumes (et non « centre du joueur
        // dans la zone ») : quand la zone est un ennemi doté d'un corps physique, les
        // colliders empêchent le centre du joueur d'entrer dans son AABB — le contact
        // doit suffire pour qu'un monstre au corps-à-corps puisse mordre. `visible`
        // exclut les ennemis vaincus (masqués par l'attaque, cf. `Scene::attack_at`) :
        // un ennemi caché ne doit plus pouvoir infliger de dégâts.
        let triggered: std::collections::HashSet<usize> = match self.player_index() {
            Some(pi) => {
                let player = &self.scene.objects[pi];
                self.scene
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(i, o)| {
                        *i != pi
                            && o.trigger
                            && o.visible
                            && self.scene.world_aabb_intersects(o, player)
                    })
                    .map(|(i, _)| i)
                    .collect()
            }
            None => std::collections::HashSet::new(),
        };
        let mut vibrations: Vec<f32> = Vec::new();
        // Régénération passive de la vie (hors contact) : appliquée avant les scripts pour
        // que les appels `damage()` de cette frame s'appliquent après, sans s'annuler.
        const HEALTH_REGEN_PER_S: f32 = 0.25;
        let mut health = self
            .hud_health
            .map(|h| (h + HEALTH_REGEN_PER_S * dt).min(1.0));
        // Positions de départ (snapshot d'entrée en Play) pour l'action « Respawn ».
        let start_pos: Vec<Vec3> = self
            .play_snapshot
            .iter()
            .map(|o| o.transform.position)
            .collect();
        for (idx, obj) in self.scene.objects.iter_mut().enumerate() {
            let just_tapped = self.tapped_obj == Some(idx);
            // Vibration Feedback : retour haptique quand l'objet est tapé.
            if obj.vibrate_on_tap > 0 && just_tapped {
                vibrations.push(obj.vibrate_on_tap as f32);
            }
            // Action au tap sans script (couleur / masquer / grandir / respawn).
            if just_tapped {
                let start = start_pos
                    .get(idx)
                    .copied()
                    .unwrap_or(obj.transform.position);
                crate::scene::apply_tap_action(obj, start, time);
            }
            // Game feel : les collectibles encore visibles tournent sur eux-mêmes.
            crate::scene::animate_collectible(obj, time);
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
                triggered.contains(&idx),
                &mut vibrations,
                &mut health,
            ) {
                log::error!("Script '{}' : {e}", obj.name);
            }
        }
        // Détecte un coup encaissé (vie en baisse) pour le retour visuel/sonore (vignette
        // rouge + bip) : déclenché une fois par « coup », pas en continu tant que le
        // contact dure (sinon le son saturerait pendant qu'un ennemi colle au joueur).
        if let (Some(prev), Some(cur)) = (self.hud_health, health)
            && cur < prev - 1e-4
        {
            self.damage_flash = 1.0;
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
        }
        self.hud_health = health;
        // Le tap n'est exposé qu'une frame.
        self.tapped_obj = None;
        // Retour haptique demandé par les scripts (natif sur mobile, log sur desktop).
        for ms in vibrations {
            crate::runtime::vibrate(ms);
        }

        // 2. physique (écrase les poses des corps dynamiques)
        let chase_target = self.player_position();
        if let Some(phys) = &mut self.physics {
            // Pilotage des objets « pilotables » : vitesse horizontale (joystick + clavier
            // + gyro) et saut (bouton tactile ou Espace). Appliqué avant le pas de simulation.
            let inp = &self.input_state;
            // Mouvement combiné joystick + clavier (flèches/WASD), borné à 1 par axe.
            let mx = (inp.joy.0 + inp.key_move.0).clamp(-1.0, 1.0);
            let my = (inp.joy.1 + inp.key_move.1).clamp(-1.0, 1.0);
            let (tilt, space) = (inp.tilt, inp.jump);
            let mut any_jump = false;
            // Objets pilotés par un joueur réseau (cf. `multiplayer.rs`, Sprint 55) :
            // chacun a son propre `NetworkInput`, distinct de `self.input_state`
            // (qui ne pilote que l'objet « joueur local », clavier/tactile/gyro de
            // cette instance — ex. l'éditeur desktop, ou un client sans réseau).
            let network_by_index: HashMap<usize, multiplayer::NetworkInput> = self
                .network_players
                .iter()
                .filter_map(|(id, &idx)| self.network_inputs.get(id).map(|inp| (idx, *inp)))
                .collect();
            for (idx, obj) in self.scene.objects.iter().enumerate() {
                let Some(ctrl) = &obj.controller else {
                    continue;
                };
                if !ctrl.input && !ctrl.gyro {
                    continue;
                }
                let net_input = network_by_index.get(&idx);
                let (mx, my, space) = match net_input {
                    Some(n) => (n.move_x.clamp(-1.0, 1.0), n.move_y.clamp(-1.0, 1.0), n.jump),
                    None => (mx, my, space),
                };
                let mut vx = 0.0;
                let mut vz = 0.0;
                if ctrl.input {
                    vx += mx * ctrl.move_speed;
                    if ctrl.auto_run_speed > 0.0 {
                        // Course automatique (endless runner) : avance en continu en +Z ;
                        // l'entrée verticale du joystick ne fait rien (seul X = voie compte).
                        vz += ctrl.auto_run_speed;
                    } else {
                        vz += -my * ctrl.move_speed;
                    }
                }
                if ctrl.gyro && net_input.is_none() {
                    vx += tilt.0 * ctrl.move_speed;
                    vz += -tilt.1 * ctrl.move_speed;
                }
                // Saut : bouton tactile nommé (joueur local), ou Espace au clavier
                // (joueur local), ou demandé par l'`Input` réseau de ce joueur.
                let jump = (!ctrl.jump_button.is_empty()
                    && self.input_state.buttons.contains(&ctrl.jump_button))
                    || (space && ctrl.input);
                let jump_speed = (2.0 * 9.81 * ctrl.jump_height.max(0.0)).sqrt();
                any_jump |= phys.control(idx, vx, vz, jump, jump_speed);
            }
            // Pilotage des « chasseurs » IA (cf. `AiChaser`) : direction directe vers la
            // position courante du joueur, recalculée chaque frame — une vraie poursuite
            // réactive (jeu local vs IA), pas une trajectoire fixe scriptée à l'avance.
            if let Some(target) = chase_target {
                for (idx, obj) in self.scene.objects.iter().enumerate() {
                    let Some(ai) = &obj.ai_chaser else { continue };
                    // Un monstre vaincu (invisible) ou d'une manche pas encore révélée
                    // ne poursuit pas (et n'a de toute façon pas de corps physique tant
                    // qu'il est masqué, cf. le filtre `visible` dans `Physics::build`).
                    if !obj.visible {
                        continue;
                    }
                    let to_target = target - obj.transform.position;
                    let dir = Vec3::new(to_target.x, 0.0, to_target.z);
                    let (vx, vz) = if dir.length_squared() > 1e-6 {
                        let d = dir.normalize() * ai.speed;
                        (d.x, d.z)
                    } else {
                        (0.0, 0.0)
                    };
                    phys.control(idx, vx, vz, false, 0.0);
                }
            }
            // Recul (knockback, cf. `AppState::stagger`) : appliqué en dernier, après le
            // pilotage joystick/IA ci-dessus, pour qu'un coup encaissé cette frame ne soit
            // pas immédiatement écrasé par la vitesse que le joystick ou la poursuite
            // viennent de recalculer.
            self.stagger.retain_mut(|(idx, vel, remaining)| {
                phys.control(*idx, vel.x, vel.z, false, 0.0);
                *remaining -= dt;
                *remaining > 0.0
            });
            phys.step(dt, &mut self.scene);
            if any_jump {
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Jump);
            }
        }
    }

    /// La partie est-elle perdue (joueur entré dans une zone mortelle) ?
    pub fn is_lost(&self) -> bool {
        self.lost
    }

    /// Temps à afficher au HUD chrono : figé à la victoire, sinon temps de jeu courant.
    /// `None` si la scène n'a pas de collectibles ou si on n'est pas en Play.
    pub fn hud_timer(&self) -> Option<f32> {
        if !self.playing || self.scene.collectibles().is_none() {
            return None;
        }
        Some(self.win_time.unwrap_or(self.time))
    }

    /// Objet « joueur » : pilotable (joystick/gyro) en priorité, sinon premier objet
    /// scripté, sinon premier objet. Base commune à `player_position` et à la résolution
    /// d'attaque (bouton/portée propres à cet objet).
    fn player_object(&self) -> Option<&SceneObject> {
        self.player_index().map(|i| &self.scene.objects[i])
    }

    /// Indice de l'objet « joueur » : cf. `player_object`.
    fn player_index(&self) -> Option<usize> {
        self.scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input || c.gyro))
            .or_else(|| {
                self.scene
                    .objects
                    .iter()
                    .position(|o| !o.script.trim().is_empty())
            })
            .or_else(|| (!self.scene.objects.is_empty()).then_some(0))
    }

    /// Position du « joueur » : cf. `player_object`.
    fn player_position(&self) -> Option<Vec3> {
        self.player_object().map(|o| o.transform.position)
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
            collider_shape: crate::runtime::physics::ColliderShape::Auto,
            group: String::new(),
            color: [1.0, 1.0, 1.0],
            texture: String::new(),
            tappable: false,
            metallic: 0.0,
            roughness: 0.6,
            emissive: 0.0,
            trigger: false,
            ..Default::default()
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

/// Chemin de la copie optimisée d'une texture (`foo.png` → `foo_opt2048.png`).
/// Conserve le schéma `asset://`/`bundle://` éventuel ; sinon écrit à côté du fichier.
fn optimized_path(path: &str, max_px: u32) -> String {
    for scheme in [crate::assets::ASSET_SCHEME, crate::assets::SCHEME] {
        if let Some(key) = path.strip_prefix(scheme) {
            let stem = std::path::Path::new(key)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("texture");
            // Une copie optimisée d'un asset devient elle-même un asset de projet.
            return format!("{}{stem}_opt{max_px}.png", crate::assets::ASSET_SCHEME);
        }
    }
    let p = std::path::Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("texture");
    let parent = p.parent().and_then(|s| s.to_str()).unwrap_or("");
    let name = format!("{stem}_opt{max_px}.png");
    if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    }
}

/// Aligne une position sur la grille (pas de 0.5) si `snap` est actif.
fn maybe_snap(p: Vec3, snap: bool) -> Vec3 {
    if !snap {
        return p;
    }
    const STEP: f32 = 0.5;
    Vec3::new(
        (p.x / STEP).round() * STEP,
        (p.y / STEP).round() * STEP,
        (p.z / STEP).round() * STEP,
    )
}

/// Hash stable d'une source de script, clé du cache de chunks compilés.
fn script_key(src: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// Cadence à pas fixe : ajoute le temps de la frame à l'accumulateur (borné contre la
/// « spirale de la mort »), puis renvoie le nombre de sous-pas de `fixed_dt` à exécuter
/// et l'accumulateur restant. Au-delà de `max` sous-pas, le reliquat est jeté (pas de
/// retard accumulé sur une machine trop lente).
fn fixed_substeps(accumulator: f32, frame_dt: f32, fixed_dt: f32, max: u32) -> (u32, f32) {
    let mut acc = accumulator + frame_dt.min(0.25);
    let mut steps = 0;
    while acc >= fixed_dt && steps < max {
        acc -= fixed_dt;
        steps += 1;
    }
    if steps == max {
        acc = 0.0;
    }
    (steps, acc)
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
    triggered: bool,
    vib_out: &mut Vec<f32>,
    health_out: &mut Option<f32>,
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
    obj.set("triggered", triggered)?;

    // Contrôles tactiles : `input.jx`, `input.jy` (joystick) et `input.btn.<nom>` (booléens).
    let input_tbl = lua.create_table()?;
    input_tbl.set("jx", input.joy.0)?;
    input_tbl.set("jy", input.joy.1)?;
    let btns = lua.create_table()?;
    for name in &input.buttons {
        btns.set(name.as_str(), true)?;
    }
    input_tbl.set("btn", btns)?;

    // `vibrate(ms)` : empile les durées de vibration demandées par le script.
    let vib = lua.create_table()?;
    let vib_ref = vib.clone();
    let vibrate = lua.create_function(move |_, ms: f32| {
        vib_ref.push(ms)?;
        Ok(())
    })?;

    // Inclinaison (gyroscope) : `tilt.x`, `tilt.y`.
    let tilt = lua.create_table()?;
    tilt.set("x", input.tilt.0)?;
    tilt.set("y", input.tilt.1)?;

    // `set_health(v)` : pilote la barre de vie du HUD (0..1), valeur absolue.
    // La table `hud` reste vide tant qu'aucun script n'y touche (opt-in : les scripts
    // sans rapport avec la vie — décor animé, etc. — ne font pas apparaître la barre).
    let hud = lua.create_table()?;
    let hud_ref = hud.clone();
    let set_health = lua.create_function(move |_, v: f32| {
        hud_ref.set("h", v.clamp(0.0, 1.0))?;
        Ok(())
    })?;
    // `damage(v)` : soustrait `v` à la vie courante (accumulée depuis le début de la
    // frame, entre objets inclus) plutôt que de l'écraser — plusieurs ennemis peuvent
    // infliger des dégâts la même frame sans s'annuler mutuellement comme le ferait
    // `set_health` (valeur absolue). Base = vie déjà régénérée/endommagée cette frame,
    // ou pleine vie par défaut si le système de vie n'a jamais démarré.
    let base_health = health_out.unwrap_or(1.0);
    let hud_ref_dmg = hud.clone();
    let damage = lua.create_function(move |_, v: f32| {
        let cur: f32 = hud_ref_dmg.get("h").unwrap_or(base_health);
        hud_ref_dmg.set("h", (cur - v).clamp(0.0, 1.0))?;
        Ok(())
    })?;

    let g = lua.globals();
    g.set("obj", &obj)?;
    g.set("dt", dt)?;
    g.set("time", time)?;
    g.set("input", input_tbl)?;
    g.set("tilt", tilt)?;
    g.set("vibrate", vibrate)?;
    g.set("set_health", set_health)?;
    g.set("damage", damage)?;
    func.call::<()>(())?;

    for v in vib.sequence_values::<f32>().flatten() {
        vib_out.push(v);
    }
    if let Ok(h) = hud.get::<f32>("h") {
        *health_out = Some(h);
    }

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

    #[test]
    fn fixed_substeps_is_framerate_independent() {
        let fixed = 1.0 / 60.0;
        // 60 FPS : 1 frame = 1 pas, reliquat ~0.
        let (n, acc) = fixed_substeps(0.0, fixed, fixed, 5);
        assert_eq!(n, 1);
        assert!(acc.abs() < 1e-6);
        // 30 FPS : une frame longue = 2 pas fixes (rattrapage).
        let (n, _) = fixed_substeps(0.0, 1.0 / 30.0, fixed, 5);
        assert_eq!(n, 2);
        // 120 FPS : frame trop courte → 0 pas, le temps s'accumule.
        let (n, acc) = fixed_substeps(0.0, 1.0 / 120.0, fixed, 5);
        assert_eq!(n, 0);
        assert!(acc > 0.0);
        // Deux frames à 120 FPS finissent par produire un pas.
        let (n2, _) = fixed_substeps(acc, 1.0 / 120.0, fixed, 5);
        assert_eq!(n2, 1);
        // Gel long : borné par le cap (pas de spirale), accumulateur remis à 0.
        let (n, acc) = fixed_substeps(0.0, 5.0, fixed, 5);
        assert_eq!(n, 5);
        assert_eq!(acc, 0.0);
    }

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
            ..Default::default()
        };
        input.buttons.insert("B1".into());
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert!((t.position.x - 0.5).abs() < 1e-5);
        assert!((t.position.y - 5.0).abs() < 1e-5);

        // Sans bouton ni joystick : aucun mouvement.
        let mut t2 = Transform::from_pos(Vec3::ZERO);
        let empty = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t2,
            &mut col,
            0.016,
            0.0,
            &empty,
            false,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
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
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert_eq!(col, [0.5, 0.5, 0.5]);
        // tap : passe au rouge
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            true,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert_eq!(col, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn script_reacts_to_trigger() {
        // obj.y monte quand le joueur entre dans la zone.
        let lua = Lua::new();
        let src = "if obj.triggered then obj.y = 9.0 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let input = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert_eq!(t.position.y, 0.0);
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            false,
            true,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert_eq!(t.position.y, 9.0);
    }

    #[test]
    fn script_reads_tilt() {
        let lua = Lua::new();
        let func = lua
            .load("obj.x = obj.x + tilt.x; obj.z = obj.z + tilt.y")
            .into_function()
            .unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput {
            tilt: (1.0, -1.0),
            ..Default::default()
        };
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert!((t.position.x - 1.0).abs() < 1e-5);
        assert!((t.position.z + 1.0).abs() < 1e-5);
    }

    #[test]
    fn script_sets_health() {
        let lua = Lua::new();
        let func = lua.load("set_health(0.5)").into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput::default();
        let mut health = None;
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut health,
        )
        .unwrap();
        assert_eq!(health, Some(0.5));
    }

    #[test]
    fn script_damage_is_relative_and_stacks_across_objects_same_frame() {
        // `damage(v)` doit soustraire de la vie déjà accumulée cette frame (par d'autres
        // objets), contrairement à `set_health` (valeur absolue) qui écraserait les dégâts
        // d'un ennemi précédent si un autre script s'exécutait après lui sans le vouloir.
        let lua = Lua::new();
        let func = lua.load("damage(0.3)").into_function().unwrap();
        let input = PlayerInput::default();
        // Aucun système de vie démarré : la base par défaut est pleine vie (1.0).
        let mut health = None;
        run_script(
            &lua,
            &func,
            &mut Transform::from_pos(Vec3::ZERO),
            &mut [1.0; 3],
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut health,
        )
        .unwrap();
        assert_eq!(health, Some(0.7));
        // Un deuxième objet inflige des dégâts la même frame : doit partir de 0.7, pas de 1.0.
        run_script(
            &lua,
            &func,
            &mut Transform::from_pos(Vec3::ZERO),
            &mut [1.0; 3],
            0.016,
            0.0,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut health,
        )
        .unwrap();
        assert!(
            (health.unwrap() - 0.4).abs() < 1e-5,
            "les dégâts de deux objets la même frame doivent s'additionner : {health:?}"
        );
        // Clampé à 0, ne descend pas en négatif.
        for _ in 0..10 {
            run_script(
                &lua,
                &func,
                &mut Transform::from_pos(Vec3::ZERO),
                &mut [1.0; 3],
                0.016,
                0.0,
                &input,
                false,
                false,
                &mut Vec::new(),
                &mut health,
            )
            .unwrap();
        }
        assert_eq!(health, Some(0.0));
    }

    #[test]
    fn controller_demo_enemy_scripts_compile_and_patrol() {
        // Les ennemis de la démo contrôleur sont scriptés (patrouille + pulsation rouge) :
        // vérifie que leurs scripts compilent et déplacent réellement l'objet dans le temps
        // (sinon un ennemi "mort" resterait immobile, silencieusement cassé).
        let scene = crate::scene::Scene::controller_demo();
        let enemies: Vec<_> = scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Ennemi"))
            .collect();
        assert!(enemies.len() >= 3, "au moins 3 ennemis dans la démo");
        let lua = Lua::new();
        for e in enemies {
            assert!(
                e.trigger && !e.deadly,
                "un ennemi doit infliger des dégâts progressifs (trigger), pas tuer \
                 instantanément (deadly) : {}",
                e.name
            );
            let func = lua.load(&e.script).into_function().unwrap();
            let mut t0 = e.transform;
            let mut col = e.color;
            let input = PlayerInput::default();
            run_script(
                &lua,
                &func,
                &mut t0,
                &mut col,
                0.016,
                0.0,
                &input,
                false,
                false,
                &mut Vec::new(),
                &mut None,
            )
            .unwrap();
            let mut t1 = e.transform;
            let mut col1 = e.color;
            run_script(
                &lua,
                &func,
                &mut t1,
                &mut col1,
                0.016,
                1.0,
                &input,
                false,
                false,
                &mut Vec::new(),
                &mut None,
            )
            .unwrap();
            assert!(
                (t0.position - t1.position).length() > 0.01,
                "l'ennemi {} doit bouger avec le temps",
                e.name
            );
        }
    }

    /// Scène synthétique minimale (sol + joueur + un danger `trigger`+`damage()` couvrant
    /// tout le sol) : isole la mécanique vie/dégâts de l'équilibrage d'un niveau réel.
    /// La démo contrôleur n'est pas réutilisée ici : sa patrouille est conçue pour un
    /// contact *intermittent* (l'ennemi s'éloigne), ce qui ne conviendrait pas pour
    /// tester un contact permanent sans coupler le test à ce détail d'équilibrage.
    fn synthetic_damage_scene() -> crate::scene::Scene {
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(16.0, 1.0, 16.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut danger = crate::scene::SceneObject {
            name: "Danger".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, 0.0))
                .with_scale(Vec3::splat(3.0)),
            trigger: true,
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            respawn_delay: 100.0,
            script: "if obj.triggered then damage(2.0 * dt) end".into(),
            ..Default::default()
        };
        danger.color = [1.0; 3];
        let mut fx = crate::scene::SceneObject {
            name: "FX Attaque".into(),
            mesh: crate::scene::MeshKind::Sphere,
            combat: Some(crate::scene::Combat {
                is_attack_fx: true,
                ..Default::default()
            }),
            visible: false,
            ..Default::default()
        };
        fx.color = [1.0; 3];
        crate::scene::Scene {
            objects: vec![sol, joueur, danger, fx],
            ..Default::default()
        }
    }

    #[test]
    fn sustained_enemy_contact_drains_health_and_ends_the_game() {
        // Bout en bout (App réel, pas juste `run_script`) : un contact **permanent** avec
        // un danger `trigger` + `damage()` doit finir par vaincre le joueur via le nouveau
        // check de défaite sur `hud_health <= 0`, malgré la régénération passive.
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        app.playing = true;
        let mut ended = false;
        for _ in 0..80 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.lost {
                ended = true;
                break;
            }
        }
        assert!(
            ended,
            "un contact soutenu doit finir par vaincre le joueur (vie = {:?})",
            app.hud_health
        );
    }

    #[test]
    fn attacking_defeats_enemy_and_stops_further_damage() {
        // Bout en bout : appuyer sur « Attaque » (bouton nommé) alors qu'un ennemi
        // `attackable` est à portée doit le vaincre (masquer) et augmenter le score.
        // Verrouille aussi la correction du filtre `triggered` (doit exclure les objets
        // invisibles) : un ennemi vaincu ne doit plus pouvoir blesser le joueur ensuite.
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        app.playing = true;
        app.input_state.buttons.insert("Attaque".into());
        // Laisse le temps à la préparation (attack_windup) puis au missile d'arriver
        // (l'attaque n'est plus instantanée, cf. `AttackCharge`/`AttackProjectile`).
        for _ in 0..10 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert_eq!(
            app.score, 1,
            "l'attaque doit vaincre l'ennemi à portée (score += 1)"
        );
        assert!(
            !app.scene
                .objects
                .iter()
                .find(|o| o.name == "Danger")
                .unwrap()
                .visible,
            "l'ennemi vaincu doit devenir invisible"
        );
        // Le joueur ne prend plus de dégâts une fois l'ennemi vaincu, même en restant
        // dessus (sans la correction du filtre `triggered`, le script du danger continuerait
        // à appeler `damage()` malgré `visible = false`).
        app.input_state.buttons.clear();
        let health_after_defeat = app.hud_health;
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            !app.lost,
            "un ennemi vaincu ne doit plus pouvoir vaincre le joueur (vie = {:?} → {:?})",
            health_after_defeat, app.hud_health
        );
    }

    #[test]
    fn attack_cooldown_blocks_rapid_refire_but_allows_it_once_expired() {
        // Trouvaille de l'audit gameplay : sans temps de recharge, maintenir le bouton
        // d'attaque défaisait instantanément tout ce qui entrait en portée, sans le
        // moindre risque — le combat était trivial. Verrouille le correctif : une
        // deuxième cible à portée n'est PAS vaincue dans la fenêtre de recharge, mais
        // l'est une fois celle-ci expirée.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 50.0,
                attack_cooldown: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut cible1 = crate::scene::SceneObject {
            name: "Cible 1".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        cible1.color = [1.0; 3];
        // Hors de portée au départ : n'est PAS touchée par la première attaque (portée
        // 50 mais la cible 2 démarre à 100 unités). Téléportée à portée juste après.
        let mut cible2 = crate::scene::SceneObject {
            name: "Cible 2".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(100.0, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        cible2.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![joueur, cible1, cible2],
            ..Default::default()
        };
        app.playing = true;
        app.input_state.buttons.insert("Attaque".into());

        // Tir sur la cible 1 (seule à portée), puis laisse le temps à la préparation
        // (attack_windup, défaut 0,25 s) et au missile d'arriver (le coup n'est plus
        // instantané, cf. `AttackCharge`/`AttackProjectile`) — sans dépasser la fenêtre
        // de recharge (0,5 s), sans quoi l'assertion suivante (cible 2 protégée par la
        // recharge) ne serait plus valide.
        for _ in 0..8 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            !app.scene.objects[1].visible,
            "cible 1 vaincue après l'arrivée du missile"
        );
        assert!(
            app.scene.objects[2].visible,
            "cible 2 encore debout (hors de portée)"
        );

        // La cible 2 entre à portée juste après (ex. un monstre qui s'approche) — toujours
        // dans la fenêtre de recharge de 0,5 s : le bouton reste enfoncé mais ne doit PAS
        // tirer un nouveau missile sur elle à cet instant.
        app.scene.objects[2].transform.position = Vec3::new(1.0, 0.5, 0.0);
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(
            app.scene.objects[2].visible,
            "sans recharge écoulée, aucun missile ne doit être tiré sur la cible 2"
        );

        // Laisse la recharge s'écouler (0,5 s) puis le missile arriver : l'attaque
        // suivante doit alors porter.
        for _ in 0..15 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            !app.scene.objects[2].visible,
            "la recharge écoulée et le missile arrivé, la cible 2 doit finir par être vaincue"
        );
    }

    #[test]
    fn attack_shows_and_hides_the_visual_fx_anchor() {
        // Une attaque qui porte doit rendre visible l'ancre `is_attack_fx`, la téléporter
        // sur la cible touchée, puis la faire disparaître une fois `attack_flash` retombé
        // à 0 — sinon l'effet resterait affiché indéfiniment après un coup.
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        let target_pos = app
            .scene
            .objects
            .iter()
            .find(|o| o.name == "Danger")
            .unwrap()
            .transform
            .position;
        app.playing = true;
        app.input_state.buttons.insert("Attaque".into());
        // Laisse le temps à la préparation puis au missile d'arriver (le coup n'est plus
        // instantané, cf. `AttackCharge`/`AttackProjectile`).
        for _ in 0..10 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }

        fn fx(app: &AppState) -> crate::scene::SceneObject {
            app.scene
                .objects
                .iter()
                .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
                .unwrap()
                .clone()
        }
        assert!(
            fx(&app).visible,
            "l'ancre FX doit être visible après un coup"
        );
        assert!(
            (fx(&app).transform.position - target_pos).length() < 1e-4,
            "l'ancre FX doit être téléportée sur la cible touchée"
        );
        assert!(app.attack_flash > 0.0);

        app.input_state.buttons.clear();
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.attack_flash <= 0.0 {
                break;
            }
        }
        assert_eq!(
            app.attack_flash, 0.0,
            "le flash d'attaque doit finir par retomber à 0"
        );
        assert!(
            !fx(&app).visible,
            "l'ancre FX doit disparaître une fois le flash retombé"
        );
    }

    #[test]
    fn auto_run_speed_advances_the_player_with_zero_input() {
        // Cœur du style « Temple Run » : un joueur `auto_run_speed > 0` doit avancer en +Z
        // même sans la moindre entrée (ni joystick, ni clavier) — contrairement au
        // déplacement classique (`move_speed` seul), purement piloté par l'entrée.
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::temple_run_demo();
        app.playing = true;
        // `input_state` reste à ses valeurs par défaut (aucune entrée).
        let z0 = app.player_position().unwrap().z;
        for _ in 0..40 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let z1 = app.player_position().unwrap().z;
        assert!(
            z1 > z0 + 1.0,
            "la course automatique doit avancer le joueur sans entrée (z0={z0}, z1={z1})"
        );
    }

    #[test]
    fn ai_chaser_actively_closes_distance_to_the_player() {
        // Cœur du « jeu local vs IA » : contrairement aux patrouilles scriptées à
        // trajectoire fixe (prévisibles, évitables par pattern), un `AiChaser` doit
        // se rapprocher réellement de la position courante du joueur, recalculée
        // chaque frame — une poursuite réactive.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut chaser = crate::scene::SceneObject {
            name: "Chasseur".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(10.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 3.0 }),
            ..Default::default()
        };
        chaser.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, chaser],
            ..Default::default()
        };
        app.playing = true;
        let dist0 = (app.scene.objects[2].transform.position - Vec3::new(0.0, 1.0, 0.0)).length();
        for _ in 0..60 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let player_pos = app.player_position().unwrap();
        let dist1 = (app.scene.objects[2].transform.position - player_pos).length();
        assert!(
            dist1 < dist0 - 1.0,
            "le chasseur doit se rapprocher du joueur (dist0={dist0}, dist1={dist1})"
        );
    }

    #[test]
    fn wave_system_reveals_next_wave_then_wins_on_the_last() {
        // 2 manches synthétiques d'un seul monstre chacune : ne doit révéler la manche 2
        // qu'une fois la manche 1 vidée, et gagner une fois la manche 2 vidée à son tour.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 50.0, // portée large : le test cible la logique de manches, pas la précision d'attaque.
                attack_cooldown: 0.0, // pas de recharge : le test cible les manches, pas le rythme de combat.
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        let mut m1 = crate::scene::SceneObject {
            name: "Monstre Vague1".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 1.0 }),
            combat: Some(crate::scene::Combat {
                attackable: true,
                wave: 1,
                ..Default::default()
            }),
            ..Default::default()
        };
        m1.color = [1.0; 3];
        let mut m2 = crate::scene::SceneObject {
            name: "Monstre Vague2".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(-5.0, 0.5, 0.0)),
            ai_chaser: Some(crate::scene::AiChaser { speed: 1.0 }),
            combat: Some(crate::scene::Combat {
                attackable: true,
                wave: 2,
                ..Default::default()
            }),
            ..Default::default()
        };
        m2.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, m1, m2],
            ..Default::default()
        };
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play(); // entrée en Play : `init_waves` doit s'exécuter.

        assert_eq!(app.wave, 1, "démarre à la manche 1");
        assert!(
            app.scene.objects[2].visible,
            "manche 1 : le monstre 1 est révélé"
        );
        assert!(
            !app.scene.objects[3].visible,
            "manche 1 : le monstre 2 reste masqué"
        );

        // Attaque : tire sur le monstre de la manche 1 (portée large, toujours à portée),
        // puis laisse le temps au missile d'arriver (le coup n'est plus instantané, cf.
        // `AttackProjectile`) et à `update_waves` de détecter la manche vidée.
        app.input_state.buttons.insert("Attaque".into());
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.wave == 2 {
                // S'arrête dès la révélation de la manche 2, avant qu'un nouveau missile
                // (bouton toujours enfoncé) n'ait le temps de la vaincre aussi.
                break;
            }
        }
        app.input_state.buttons.clear();

        assert_eq!(app.wave, 2, "la manche 1 vidée doit révéler la manche 2");
        assert!(
            app.scene.objects[3].visible,
            "manche 2 : le monstre 2 est révélé"
        );
        assert!(
            app.win_time.is_none(),
            "pas encore gagné, la manche 2 reste à vider"
        );

        // Vainc le monstre de la manche 2 : dernière manche ⇒ victoire.
        app.input_state.buttons.insert("Attaque".into());
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        app.input_state.buttons.clear();

        assert!(
            app.win_time.is_some(),
            "toutes les manches vidées ⇒ victoire"
        );
    }

    #[test]
    fn only_controller_demo_is_marked_as_leveled() {
        // `is_leveled_demo` pilote si le bouton de fin de partie appelle `next_level()`
        // (bascule vers `controller_level`) ou `restart_game()` (relance la même scène).
        // Une régression ici ferait basculer une victoire en course infinie / tour /
        // manches de zombies vers l'arène de combat au lieu de relancer la bonne scène.
        let mut app = AppState::new();
        app.load_controller_demo();
        assert!(app.is_leveled_demo, "démo contrôleur : à niveaux");

        app.load_tower_demo();
        assert!(!app.is_leveled_demo, "tour : pas de niveau suivant");

        app.load_temple_run_demo();
        assert!(
            !app.is_leveled_demo,
            "course infinie : pas de niveau suivant"
        );

        app.load_zombies_demo();
        assert!(
            !app.is_leveled_demo,
            "zombies : pas de niveau suivant (manches)"
        );

        app.load_gameplay_demo();
        assert!(!app.is_leveled_demo);

        app.load_components_demo();
        assert!(!app.is_leveled_demo);

        app.load_mobile_demo();
        assert!(!app.is_leveled_demo);

        app.load_roguelike_demo();
        assert!(
            !app.is_leveled_demo,
            "donjon : pas de niveau suivant (manches)"
        );

        app.load_brawl_demo();
        assert!(
            !app.is_leveled_demo,
            "duel : pas de niveau suivant (manches)"
        );
    }

    #[test]
    fn roguelike_demo_clears_rooms_one_at_a_time_to_victory() {
        // Bout en bout sur la vraie scène (pas une scène synthétique) : la salle 2 ne
        // doit pas être révélée avant que la salle 1 soit vidée, et ainsi de suite
        // jusqu'à la victoire — même mécanique que `wave_system_reveals_next_wave_...`
        // mais sur `Scene::roguelike_demo`, portée d'attaque élargie et préparation
        // nulle pour isoler la logique de manches de la précision de visée et de l'arme
        // tirée au sort (cf. commentaire similaire dans
        // `wave_system_reveals_next_wave_then_wins_on_the_last`). Le joueur ne bouge
        // jamais dans ce test (aucune entrée de mouvement) : le missile doit donc
        // parcourir toute la longueur du donjon pour la salle 3 (~20 m) — budget de
        // boucle large pour laisser le temps au missile homing d'arriver.
        let mut app = AppState::new();
        app.load_roguelike_demo();
        for o in &mut app.scene.objects {
            if let Some(c) = &mut o.controller
                && c.input
            {
                c.attack_range = 50.0;
                c.attack_cooldown = 0.0;
                c.attack_windup = 0.0;
            }
        }
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(app.wave, 1, "démarre à la salle 1");

        let monster_count_wave = |app: &AppState, w: u32| {
            app.scene
                .objects
                .iter()
                .filter(|o| o.visible && o.combat.as_ref().is_some_and(|c| c.wave == w))
                .count()
        };
        assert_eq!(
            monster_count_wave(&app, 1),
            1,
            "salle 1 : son monstre est visible"
        );
        assert_eq!(monster_count_wave(&app, 2), 0, "salle 2 : encore masquée");
        assert_eq!(monster_count_wave(&app, 3), 0, "salle 3 : encore masquée");

        app.input_state.attack = true;
        for wave in 1..=3u32 {
            for _ in 0..100 {
                app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
                app.advance_play();
                if app.wave > wave || app.has_won() {
                    break;
                }
            }
        }
        assert!(
            app.has_won(),
            "les 3 salles vidées doivent déclencher la victoire (wave={})",
            app.wave
        );
    }

    #[test]
    fn roguelike_demo_walking_onto_a_weapon_pickup_reequips_the_player() {
        // Le ramassage d'arme (donjon roguelike) est **natif** (pas un script Lua, qui ne
        // peut pas modifier `Controller`) : bout en bout via `advance_play`, pas
        // seulement au niveau `Scene::weapon_pickup_at` (déjà testé isolément côté scène).
        let mut app = AppState::new();
        app.load_roguelike_demo();
        let (loot_idx, loot_pos, expected) = app
            .scene
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| {
                o.weapon_pickup
                    .map(|wp| (i, o.transform.position, crate::scene::WEAPONS[wp.weapon]))
            })
            .expect("le donjon a au moins un butin d'arme");
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .unwrap();
        // Place le joueur exactement sur le butin (au lieu de simuler un déplacement) :
        // isole la résolution du ramassage de la logique de déplacement, déjà testée
        // ailleurs (`controller_demo_player_moves_with_joystick`).
        app.scene.objects[pi].transform.position = loot_pos;

        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();

        let ctrl = app.scene.objects[pi].controller.as_ref().unwrap();
        assert_eq!(
            (ctrl.attack_range, ctrl.attack_cooldown, ctrl.attack_windup),
            (expected.range, expected.cooldown, expected.windup),
            "le joueur doit être équipé du profil du butin ramassé"
        );
        assert!(
            !app.scene.objects[loot_idx].visible,
            "le butin ramassé doit disparaître"
        );
        assert_eq!(
            app.score(),
            1,
            "un butin ramassé doit compter au score, comme une pièce"
        );
    }

    #[test]
    fn brawl_demo_rival_survives_two_hits_then_falls_on_the_third() {
        // Le cœur du duel façon Tekken/Smash : le rival a plusieurs PV (cf.
        // `Combat::hp`), donc encaisse d'abord, ne meurt pas au premier coup. Portée
        // élargie et recharge/préparation nulles pour isoler la mécanique de PV de la
        // précision de visée et du timing (même convention que les tests de manches).
        let mut app = AppState::new();
        app.load_brawl_demo();
        let ri = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Rival")
            .unwrap();
        for o in &mut app.scene.objects {
            if let Some(c) = &mut o.controller
                && c.input
            {
                c.attack_range = 50.0;
                c.attack_cooldown = 0.0;
                c.attack_windup = 0.0;
            }
        }
        app.playing = true;
        app.input_state.attack = true;

        let mut hp_history = Vec::new();
        for _ in 0..1000 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if let Some(hp) = app.scene.objects[ri].combat.as_ref().map(|c| c.hp)
                && hp_history.last() != Some(&hp)
            {
                hp_history.push(hp);
            }
            if !app.scene.objects[ri].visible {
                break;
            }
        }
        assert_eq!(
            hp_history,
            vec![3, 2, 1, 0],
            "le rival doit encaisser 3 coups avant de tomber, pas mourir au premier"
        );
        assert!(
            !app.scene.objects[ri].visible,
            "invisible une fois achevé au 3e coup"
        );
        assert_eq!(
            app.score(),
            1,
            "le score ne doit compter que le coup qui achève, pas les coups intermédiaires"
        );
        assert!(
            app.has_won(),
            "achever l'unique rival doit déclencher la victoire (cf. Combat::wave = 1)"
        );
    }

    #[test]
    fn brawl_demo_non_lethal_hit_knocks_the_rival_away_from_the_player() {
        // Contrepoint « Smash » du coup qui achève : un coup qui blesse sans tuer doit
        // repousser la cible (cf. `AppState::stagger`/`KNOCKBACK_SPEED`), pas la laisser
        // reprendre aussitôt sa poursuite comme si de rien n'était — sinon aucun ring out
        // n'est jamais possible.
        let mut app = AppState::new();
        app.load_brawl_demo();
        let ri = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Rival")
            .unwrap();
        for o in &mut app.scene.objects {
            if let Some(c) = &mut o.controller
                && c.input
            {
                c.attack_range = 50.0;
                // Recharge énorme : un seul coup possible sur toute la durée du test,
                // pour observer le recul sans qu'un 2e coup n'interfère.
                c.attack_cooldown = 100.0;
                c.attack_windup = 0.0;
            }
        }
        app.playing = true;
        app.input_state.attack = true;

        let mut pos_at_impact = None;
        for _ in 0..200 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.scene.objects[ri]
                .combat
                .as_ref()
                .is_some_and(|c| c.hp == 2)
            {
                pos_at_impact = Some(app.scene.objects[ri].transform.position);
                break;
            }
        }
        let pos_at_impact = pos_at_impact.expect("le 1er coup (non-létal) doit atterrir");
        let player_pos = app
            .player_position()
            .expect("le joueur ne bouge pas dans ce test (aucune entrée de mouvement)");
        let dist0 = (pos_at_impact - player_pos).length();

        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let dist1 = (app.scene.objects[ri].transform.position - player_pos).length();
        assert!(
            dist1 > dist0,
            "le rival doit s'éloigner juste après un coup non-létal, pas continuer de \
             se rapprocher comme le ferait une poursuite ininterrompue (avant={dist0}, après={dist1})"
        );
    }

    #[test]
    fn falling_into_the_void_ring_outs_the_rival_and_counts_as_victory() {
        // Deuxième façon de gagner un duel façon Smash : sortir l'adversaire de l'arène,
        // pas seulement l'achever à coups de poing (cf. `Scene::brawl_demo`).
        let mut app = AppState::new();
        app.load_brawl_demo();
        let ri = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Rival")
            .unwrap();
        // Téléporte le rival dans le vide sous l'arène (au lieu de simuler un vrai
        // recul jusqu'au bord) : isole la détection du ring out de la mécanique de
        // recul, déjà testée ailleurs (`brawl_demo_non_lethal_hit_knocks_the_rival_away_from_the_player`).
        app.scene.objects[ri].transform.position = Vec3::new(0.0, -8.0, 0.0);
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();

        assert!(
            !app.scene.objects[ri].visible,
            "le rival doit être vaincu en tombant dans le vide"
        );
        assert!(
            app.has_won(),
            "un ring out doit compter comme une victoire (adversaire unique, wave=1)"
        );
    }

    #[test]
    fn zombies_demo_attack_range_stays_close_to_monster_bite_reach() {
        // Audit gameplay : la portée d'attaque totale (attack_range + rayon du monstre)
        // est un cercle qui **contient toujours** la boîte de morsure du monstre (rayon
        // ≈ son propre rayon) dès que `attack_range > 0` — un joueur qui fonce droit sur
        // un monstre gagnera donc structurellement la course à l'engagement, quelle que
        // soit sa vitesse. `attack_range` ne peut pas éliminer ce biais en 1 contre 1
        // frontal, seulement en réduire la marge (le vrai risque vient d'affronter
        // plusieurs monstres à la fois pendant la recharge). L'ancienne valeur (1,5 m)
        // donnait une marge de sécurité énorme (jusqu'à 4-5× le rayon du plus petit
        // monstre) ; verrouille qu'elle reste modeste désormais.
        let s = crate::scene::Scene::zombies_demo();
        let ctrl = s
            .objects
            .iter()
            .find_map(|o| o.controller.as_ref())
            .expect("un joueur pilotable");
        let smallest_monster_r = s
            .objects
            .iter()
            .filter(|o| o.ai_chaser.is_some())
            .map(|o| o.transform.scale.max_element() * 0.5)
            .fold(f32::INFINITY, f32::min);
        assert!(
            ctrl.attack_range <= smallest_monster_r + 0.5,
            "marge de sécurité trop généreuse : attack_range={} vs rayon du plus petit \
             monstre={smallest_monster_r} (marge > 0,5 m)",
            ctrl.attack_range
        );
    }

    #[test]
    fn attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing() {
        // Suite de l'audit gameplay : `attack_at` vainquait TOUTES les cibles à portée en
        // un seul appel (balayage de zone). Une expérimentation poussée (3 archétypes
        // convergeant en cercle serré sur un joueur immobile qui attaque en continu) a
        // montré qu'ils entraient dans le rayon de mise à mort de façon quasi synchronisée
        // — leur taille (donc leur propre rayon, qui élargit d'autant le rayon de mise à
        // mort perçu) compense presque exactement leur différence de vitesse. Résultat :
        // un groupe entier disparaissait en un seul coup, sans qu'aucun n'ait jamais mordu.
        // `attack_at` ne vainc désormais que la cible la plus proche : un groupe de 3
        // exige donc 3 coups (et donc 3 fenêtres de recharge), pas un seul.
        //
        // Limite honnête, documentée plutôt que masquée par un test fragile : ceci ne
        // garantit pas qu'un joueur qui reste immobile et attaque prendra des dégâts —
        // sans temps de préparation sur l'attaque, la portée d'attaque englobera toujours
        // la portée de morsure d'un monstre qui approche en ligne droite (cf.
        // `zombies_demo_attack_range_stays_close_to_monster_bite_reach`), donc gagner la
        // course à l'engagement 1 contre 1 reste structurellement favorable au joueur.
        // Un vrai risque garanti demanderait un temps de préparation sur l'attaque
        // (fenêtre de vulnérabilité avant que le coup ne porte) — hors du périmètre de ce
        // sprint, noté dans audit_sprint.md pour une prochaine itération.
        let mut s = crate::scene::Scene::default();
        s.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        for n in 0..3 {
            let mut m = crate::scene::SceneObject {
                name: format!("Monstre {n}"),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::new(0.2 * n as f32, 0.5, 0.0)),
                combat: Some(crate::scene::Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            m.color = [1.0; 3];
            s.objects.push(m);
        }
        // Les 3 sont groupés à moins de 0,5 m les uns des autres, largement à portée
        // d'une seule attaque à grand rayon.
        let hit = s.attack_at(Vec3::new(0.2, 0.5, 0.0), 5.0);
        assert_eq!(
            hit.len(),
            1,
            "une attaque ne vainc qu'une seule cible, pas tout le groupe"
        );
        let still_visible = s.objects[1..].iter().filter(|o| o.visible).count();
        assert_eq!(
            still_visible, 2,
            "les 2 autres cibles du groupe doivent survivre à ce coup"
        );
    }

    #[test]
    fn attack_mode_zone_clears_a_whole_cluster_in_one_swing() {
        // Contrepoint direct de `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` :
        // ce dernier documente que le mode par défaut (`Single`) ne vainc qu'une cible à
        // la fois, précisément pour ne pas trivialiser un groupe convergent. Le mode
        // `AttackMode::Zone` (Marteau, cf. `Weapon::mode`) doit au contraire vaincre TOUT
        // le groupe d'un coup — c'est le point de payer une préparation/recharge plus
        // longues (cf. `WEAPONS`) : jamais d'état intermédiaire « 1 ou 2 des 3 vaincus ».
        let mut s = crate::scene::Scene::default();
        s.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.2, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 5.0,
                attack_cooldown: 1.0,
                attack_windup: 0.1,
                attack_mode: crate::scene::AttackMode::Zone,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        s.objects.push(joueur);
        for n in 0..3 {
            let mut m = crate::scene::SceneObject {
                name: format!("Monstre {n}"),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::new(0.2 * n as f32, 0.5, 0.0)),
                combat: Some(crate::scene::Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            m.color = [1.0; 3];
            s.objects.push(m);
        }

        let mut app = AppState::new();
        app.scene = s;
        app.playing = true;
        app.input_state.attack = true;
        let mut seen_counts: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            let visible = app.scene.objects[2..].iter().filter(|o| o.visible).count();
            seen_counts.insert(visible);
            if visible == 0 {
                break;
            }
        }
        assert!(
            seen_counts.contains(&0),
            "le mode Zone doit finir par vaincre tout le groupe"
        );
        assert!(
            !seen_counts.contains(&1) && !seen_counts.contains(&2),
            "jamais d'état intermédiaire \"1 ou 2 vaincus\" : la résolution doit toucher \
             les 3 cibles du groupe dans le même appel, pas une par une (vu={seen_counts:?})"
        );
    }

    /// Duel 1 contre 1 : sol statique, joueur pilotable (attaque à préparation) et un
    /// monstre-chasseur mordeur à 1 m. Le monstre a un **corps physique** (via
    /// `ai_chaser` + `visible`, cf. `Physics::build`) : contrairement aux dangers
    /// statiques de `synthetic_damage_scene`, sa collision solide repousse le joueur —
    /// c'est précisément la configuration où la morsure « centre dans l'AABB » échouait.
    fn duel_1v1_scene() -> crate::scene::Scene {
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 6.0,
                attack_cooldown: 0.5,
                attack_windup: 0.25,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        // À 1 m (rayon de morsure par défaut ≈ 0,5 m) et 4 m/s : atteint sa portée de
        // morsure en (1 - 0,5) / 4 = 0,125 s — avant la fin des 0,25 s de préparation,
        // donc avant même que le missile ne soit tiré.
        let mut monstre = crate::scene::SceneObject {
            name: "Monstre".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.5, 0.0)),
            trigger: true,
            ai_chaser: Some(crate::scene::AiChaser { speed: 4.0 }),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            script: "if obj.triggered then damage(5.0 * dt) end".into(),
            ..Default::default()
        };
        monstre.color = [1.0; 3];

        crate::scene::Scene {
            objects: vec![sol, joueur, monstre],
            ..Default::default()
        }
    }

    #[test]
    fn chasing_monster_with_solid_body_can_bite_the_player_on_contact() {
        // Régression du bug racine découvert par l'audit : la morsure testait « centre
        // du joueur dans l'AABB du monstre », or les colliders solides (joueur et
        // chasseur ont tous deux un corps rigide) empêchent toute interpénétration —
        // un monstre-chasseur ne mordait donc *jamais*, même en contact continu. Le
        // test de déclenchement est désormais une **intersection d'AABB** (cf.
        // `Scene::world_aabb_intersects`) : le contact suffit.
        let mut app = AppState::new();
        app.scene = duel_1v1_scene();
        app.playing = true;
        // Aucune attaque : on isole la collision physique pure (le joueur ne se défend
        // pas, le monstre doit finir par le mordre).
        app.input_state.attack = false;

        let mut took_damage = false;
        for _ in 0..40 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.hud_health.is_some() {
                took_damage = true;
                break;
            }
        }
        assert!(
            took_damage,
            "un monstre-chasseur au corps solide doit pouvoir mordre au contact, \
             malgré la répulsion physique qui interdit l'interpénétration des centres"
        );
    }

    #[test]
    fn attack_windup_finally_guarantees_risk_in_a_1v1() {
        // Clôt la limite documentée à répétition dans l'audit (le temps de vol du
        // missile seul ne suffisait pas à garantir un risque en 1 contre 1, cf.
        // `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` et
        // `attack_is_a_missile_with_travel_time_not_an_instant_hit`) : un temps de
        // préparation (`Controller::attack_windup`) *avant même que le missile ne
        // parte* fonctionne, lui, indépendamment de la vitesse du missile — un monstre
        // déjà proche de sa propre portée de morsure au moment du tir peut mordre
        // pendant la préparation, avant qu'aucun projectile n'existe.
        let mut app = AppState::new();
        app.scene = duel_1v1_scene();
        app.playing = true;
        // Attaque maintenue dès la première frame : la préparation démarre aussitôt.
        app.input_state.attack = true;

        let mut bitten_before_kill = false;
        for _ in 0..40 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.hud_health.is_some() && app.scene.objects[2].visible {
                bitten_before_kill = true;
            }
            if !app.scene.objects[2].visible {
                break;
            }
        }
        assert!(
            !app.scene.objects[2].visible,
            "le missile doit finir par vaincre le monstre (sinon le duel ne se résout pas)"
        );
        assert!(
            bitten_before_kill,
            "un monstre déjà proche de sa portée de morsure doit pouvoir mordre pendant \
             la préparation de l'attaque, avant que le missile ne le vainque — gagner \
             un 1 contre 1 doit coûter de la vie"
        );
    }

    #[test]
    fn attack_is_a_missile_with_travel_time_not_an_instant_hit() {
        // L'attaque est désormais un missile homing avec un temps de vol (cf.
        // `AttackProjectile`), pas une résolution instantanée au tir : rend le coup
        // lisible en 3D (le missile se voit voyager, pas juste « la cible disparaît »).
        //
        // Limite honnête, re-vérifiée ici plutôt que survendue : le temps de vol NE
        // garantit PAS à lui seul un risque en 1 contre 1 — un missile homing tiré dès
        // l'entrée en portée arrive quasi toujours avant qu'un monstre qui fonce en
        // ligne droite n'ait eu le temps d'atteindre sa propre (bien plus courte) portée
        // de morsure, sauf à rendre le missile déraisonnablement lent. Le vrai risque
        // reste celui déjà documenté : affronter plusieurs monstres à la fois pendant la
        // recharge (cf. `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing`).
        // Ce test vérifie donc uniquement ce que le missile change réellement : un vol
        // progressif et homing, pas un « tout ou rien » au moment du tir.
        let mut joueur = crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                attack_button: "Attaque".into(),
                attack_range: 6.0,
                attack_cooldown: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut sol = crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(30.0, 1.0, 30.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        };
        sol.color = [1.0; 3];
        // À 5 m : à portée du tir (6 m), le missile doit voyager plusieurs frames avant
        // d'arriver (pas de patrouille/chasse ici : isole le temps de vol lui-même).
        let mut monstre = crate::scene::SceneObject {
            name: "Monstre".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        monstre.color = [1.0; 3];
        let mut fx = crate::scene::SceneObject {
            name: "FX Attaque".into(),
            mesh: crate::scene::MeshKind::Sphere,
            combat: Some(crate::scene::Combat {
                is_attack_fx: true,
                ..Default::default()
            }),
            visible: false,
            ..Default::default()
        };
        fx.color = [1.0; 3];

        let mut app = AppState::new();
        app.scene = crate::scene::Scene {
            objects: vec![sol, joueur, monstre, fx],
            ..Default::default()
        };
        app.playing = true;
        app.input_state.attack = true;

        // Quelques pas : couvre la préparation (attack_windup, 0,25 s par défaut) sans
        // atteindre le temps de vol du missile (5 m à 10 m/s ≈ 0,5 s) — le monstre à 5 m
        // ne doit pas être vaincu si tôt.
        for _ in 0..6 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert!(
            app.scene.objects[2].visible,
            "le monstre à 5 m ne doit pas être vaincu dès la préparation/le tir : le \
             missile met du temps à arriver"
        );
        let fx_after_launch = app
            .scene
            .objects
            .iter()
            .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
            .map(|o| o.transform.position);

        // Quelques frames plus tard : le missile a progressé (pas téléporté).
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let fx_mid_flight = app
            .scene
            .objects
            .iter()
            .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
            .map(|o| o.transform.position);
        assert_ne!(
            fx_after_launch, fx_mid_flight,
            "l'ancre visuelle doit progresser vers la cible, pas rester figée"
        );

        // Laisse le temps au missile d'arriver.
        for _ in 0..20 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if !app.scene.objects[2].visible {
                break;
            }
        }
        assert!(
            !app.scene.objects[2].visible,
            "le missile doit finir par atteindre sa cible"
        );
    }

    #[test]
    fn damage_triggers_flash_that_fades_and_resets_on_stop() {
        // Retour visuel du coup : `damage_flash` doit monter à 1.0 dès la première baisse
        // de vie détectée, puis décroître frame après frame (pas rester bloqué au pic).
        let mut app = AppState::new();
        app.scene = synthetic_damage_scene();
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        // Le pic (1.0) est déclenché par le sim_step qui détecte le coup, mais cette même
        // frame applique déjà une frame de décroissance ensuite (comportement voulu : le
        // flash commence à s'estomper dès la frame du coup) — d'où la marge, pas `== 1.0`.
        let peak = app.damage_flash;
        assert!(
            peak > 0.8,
            "un coup doit déclencher un pic net du flash : {peak}"
        );
        // Sort du contact (sinon chaque frame retriggerait le pic à 1.0) pour vérifier la
        // décroissance en l'absence de nouveaux coups. Reconstruit le corps physique à sa
        // nouvelle position : sinon le pas de physique du même appel le ramènerait vers
        // l'ancienne pose (le corps rigide, lui, n'a pas bougé) et le contact reprendrait.
        if let Some(j) = app
            .scene
            .objects
            .iter_mut()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        {
            j.transform.position = Vec3::new(50.0, 0.5, 50.0);
        }
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(
            app.damage_flash < peak,
            "le flash doit continuer à décroître frame après frame hors contact"
        );
        // Sortir de Play remet tout à zéro (pas de flash résiduel visible en édition).
        app.playing = false;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(
            app.damage_flash, 0.0,
            "le flash est effacé à la sortie de Play"
        );
    }

    #[test]
    fn controller_demo_lava_boil_script_preserves_collision_scale() {
        // La lave a un script de « bouillonnement » (pulsation de couleur) ajouté après coup ;
        // il ne doit surtout pas toucher à l'échelle Y, qui encode l'épaisseur de collision
        // nécessaire pour que la zone mortelle détecte un joueur debout (cf. test dédié dans
        // scene::tests). Une régression ici rendrait la lave inoffensive en silence.
        let scene = crate::scene::Scene::controller_demo();
        let lave = scene
            .objects
            .iter()
            .find(|o| o.name == "Lave")
            .expect("la lave existe");
        assert!(!lave.script.trim().is_empty(), "la lave doit être animée");
        let lua = Lua::new();
        let func = lua.load(&lave.script).into_function().unwrap();
        let mut t = lave.transform;
        let mut col = lave.color;
        let input = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            0.016,
            3.7,
            &input,
            false,
            false,
            &mut Vec::new(),
            &mut None,
        )
        .unwrap();
        assert_eq!(
            t.scale, lave.transform.scale,
            "le script de la lave ne doit pas modifier l'échelle (collision)"
        );
        assert_eq!(
            t.position, lave.transform.position,
            "le script de la lave ne doit pas déplacer la mare"
        );
    }

    #[test]
    fn script_can_request_vibration() {
        let lua = Lua::new();
        let func = lua
            .load("if obj.tapped then vibrate(80) end")
            .into_function()
            .unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let input = PlayerInput::default();
        let mut vib = Vec::new();
        run_script(
            &lua, &func, &mut t, &mut col, 0.016, 0.0, &input, true, false, &mut vib, &mut None,
        )
        .unwrap();
        assert_eq!(vib, vec![80.0]);
    }

    #[test]
    fn restart_game_restores_scene_and_clears_flags() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::controller_demo();
        app.play_snapshot = app.scene.objects.clone();
        // Simule une partie en cours : une gemme ramassée, perdu, chrono figé.
        if let Some(g) = app
            .scene
            .objects
            .iter_mut()
            .find(|o| o.tap_action == crate::scene::TapAction::Hide)
        {
            g.visible = false;
        }
        app.lost = true;
        app.win_time = Some(5.0);
        app.time = 5.0;

        app.restart_game();

        assert!(!app.lost, "défaite remise à zéro");
        assert!(app.win_time.is_none(), "victoire remise à zéro");
        assert_eq!(app.time, 0.0, "chrono remis à zéro");
        // Scopé aux gemmes (Hide) : d'autres objets sont légitimement invisibles par défaut
        // dans cette démo (ex. l'ancre `is_attack_fx`, masquée tant qu'aucun coup ne porte).
        assert!(
            app.scene
                .objects
                .iter()
                .filter(|o| o.tap_action == crate::scene::TapAction::Hide)
                .all(|o| o.visible),
            "toutes les gemmes redeviennent visibles"
        );
    }

    #[test]
    fn undo_covers_point_lights() {
        let mut app = AppState::new();
        let n0 = app.scene.point_lights.len();
        app.push_undo();
        app.scene.point_lights.push(PointLight::default());
        assert_eq!(app.scene.point_lights.len(), n0 + 1);
        app.undo();
        assert_eq!(app.scene.point_lights.len(), n0); // lumière retirée par l'undo
        app.redo();
        assert_eq!(app.scene.point_lights.len(), n0 + 1); // ré-ajoutée
    }

    #[test]
    fn distribute_spaces_evenly() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        for x in [0.0, 1.0, 9.0] {
            app.scene.objects.push(SceneObject {
                name: "o".into(),
                transform: Transform::from_pos(Vec3::new(x, 0.0, 0.0)),
                mesh: MeshKind::Cube,
                script: String::new(),
                physics: crate::runtime::physics::PhysicsKind::None,
                collider_shape: crate::runtime::physics::ColliderShape::Auto,
                group: String::new(),
                color: [1.0; 3],
                texture: String::new(),
                tappable: false,
                metallic: 0.0,
                roughness: 0.6,
                emissive: 0.0,
                trigger: false,
                ..Default::default()
            });
        }
        app.selected = vec![0, 1, 2];
        app.distribute_selection_axis(0);
        // extrémités conservées (0 et 9), celui du milieu recalé à 4.5
        let xs: Vec<f32> = app
            .scene
            .objects
            .iter()
            .map(|o| o.transform.position.x)
            .collect();
        assert!((xs[0] - 0.0).abs() < 1e-5);
        assert!((xs[1] - 4.5).abs() < 1e-5);
        assert!((xs[2] - 9.0).abs() < 1e-5);
    }

    #[test]
    fn optimized_path_preserves_scheme() {
        // Un asset projet reste un asset projet ; un chemin disque écrit à côté.
        assert_eq!(
            optimized_path("asset://bois.png", 1024),
            "asset://bois_opt1024.png"
        );
        assert_eq!(
            optimized_path("/tmp/bois.jpg", 2048),
            "/tmp/bois_opt2048.png"
        );
        assert_eq!(optimized_path("bois.png", 512), "bois_opt512.png");
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
