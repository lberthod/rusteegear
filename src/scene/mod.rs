//! Modèle de scène (sans ECS) : un Vec d'objets, chacun avec un Transform et un type de mesh.

pub mod import;

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::gfx::mesh::{self, MeshData};
use crate::runtime::physics::PhysicsKind;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::from_pos(Vec3::ZERO)
    }
}

impl Transform {
    pub fn from_pos(position: Vec3) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MeshKind {
    #[default]
    Cube,
    Sphere,
    Plane,
    Cylinder,
    Capsule,
    Terrain,
    /// Modèle glTF importé, index dans `Scene::imported`.
    Imported(u32),
}

impl MeshKind {
    /// Primitives générées par code (clés du cache de meshes GPU).
    pub const ALL: [MeshKind; 6] = [
        MeshKind::Cube,
        MeshKind::Sphere,
        MeshKind::Plane,
        MeshKind::Cylinder,
        MeshKind::Capsule,
        MeshKind::Terrain,
    ];

    /// Données CPU des primitives (pas valable pour `Imported`).
    pub fn mesh_data(self) -> MeshData {
        match self {
            MeshKind::Cube => mesh::cube([0.8, 0.45, 0.2]),
            MeshKind::Sphere => mesh::sphere([0.3, 0.55, 0.85]),
            MeshKind::Plane => mesh::plane([0.35, 0.4, 0.35]),
            MeshKind::Cylinder => mesh::cylinder([0.55, 0.45, 0.7]),
            MeshKind::Capsule => mesh::capsule([0.45, 0.7, 0.5]),
            MeshKind::Terrain => mesh::terrain([0.4, 0.55, 0.35]),
            MeshKind::Imported(_) => MeshData::default(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MeshKind::Cube => "Cube",
            MeshKind::Sphere => "Sphère",
            MeshKind::Plane => "Plan",
            MeshKind::Cylinder => "Cylindre",
            MeshKind::Capsule => "Capsule",
            MeshKind::Terrain => "Terrain",
            MeshKind::Imported(_) => "Modèle",
        }
    }
}

/// Géométrie importée d'un fichier glTF. `data`/`aabb` sont reconstruits au chargement.
#[derive(Serialize, Deserialize, Default)]
pub struct ImportedMesh {
    pub name: String,
    pub path: String,
    #[serde(skip)]
    pub data: MeshData,
    #[serde(skip)]
    pub aabb_min: Vec3,
    #[serde(skip)]
    pub aabb_max: Vec3,
}

/// Composant optionnel : fait d'un `SceneObject` un objet pilotable par le joueur
/// (joystick, gyroscope, saut, attaque). Regroupe des champs auparavant plats sur
/// `SceneObject` — un seul objet par scène en porte généralement un (le joueur), donc
/// les y laisser à plat aurait alourdi *tous* les objets (décor, ennemis, pièces...)
/// pour rien. Étape de migration « composants optionnels » (pas un ECS complet : pas
/// de requêtes génériques, juste un regroupement logique qui évite le bloat plat).
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Controller {
    /// Le joystick/clavier (X/Z, ou X seul si `auto_run_speed > 0`) pilote l'objet.
    #[serde(default)]
    pub input: bool,
    /// L'inclinaison (gyroscope/flèches simulées) pilote l'objet.
    #[serde(default)]
    pub gyro: bool,
    /// Vitesse appliquée par `input`/`gyro` (unités/seconde).
    #[serde(default = "default_move_speed")]
    pub move_speed: f32,
    /// Course automatique (m/s, +Z) : > 0 ⇒ avance en continu sans action du joueur
    /// (style « endless runner »), l'entrée horizontale ne pilotant plus que la voie (X).
    #[serde(default)]
    pub auto_run_speed: f32,
    /// Nom du bouton tactile qui fait sauter (vide = pas de saut).
    #[serde(default)]
    pub jump_button: String,
    /// Hauteur de saut (mètres).
    #[serde(default = "default_jump_height")]
    pub jump_height: f32,
    /// Nom du bouton tactile qui fait attaquer (vide = pas d'attaque). Combiné à la
    /// touche clavier Attaque (desktop) — cf. `PlayerInput::attack`.
    #[serde(default)]
    pub attack_button: String,
    /// Portée (mètres) de l'attaque, centrée sur la position de l'objet.
    #[serde(default = "default_attack_range")]
    pub attack_range: f32,
}

impl Controller {
    /// Pilotable « standard » (joystick + saut), le cas le plus courant. Les autres champs
    /// restent à leurs défauts (`Controller::default()` puis champs modifiés au besoin).
    pub fn input_only(move_speed: f32) -> Self {
        Self {
            input: true,
            move_speed,
            ..Default::default()
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SceneObject {
    pub name: String,
    pub transform: Transform,
    pub mesh: MeshKind,
    /// Script Lua exécuté chaque frame en mode Play (vide = aucun).
    #[serde(default)]
    pub script: String,
    /// Type de corps physique en mode Play.
    #[serde(default = "default_physics")]
    pub physics: PhysicsKind,
    /// Forme du collider (Auto = déduite du mesh).
    #[serde(default)]
    pub collider_shape: crate::runtime::physics::ColliderShape,
    /// Fichier son associé (vide = aucun).
    #[serde(default)]
    pub audio_clip: String,
    /// Joue le son au lancement du mode Play.
    #[serde(default)]
    pub audio_autoplay: bool,
    /// Groupe (dossier) défini par l'utilisateur ; vide = « Sans groupe ».
    #[serde(default)]
    pub group: String,
    /// Teinte (albédo) appliquée à l'objet ; blanc = couleur du mesh inchangée.
    #[serde(default = "white")]
    pub color: [f32; 3],
    /// Texture albédo (chemin disque ou `bundle://`) ; vide = aucune.
    #[serde(default)]
    pub texture: String,
    /// Objet « tactile » : un tap dessus en mode Play expose `obj.tapped = true`
    /// au script pendant une frame (interaction tactile, ex. changer de couleur).
    #[serde(default)]
    pub tappable: bool,
    /// Aspect métallique (0 = diélectrique, 1 = métal).
    #[serde(default)]
    pub metallic: f32,
    /// Rugosité de surface (0 = miroir, 1 = mat).
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    /// Intensité d'émission (0 = aucune ; l'objet « brille » de sa propre couleur).
    #[serde(default)]
    pub emissive: f32,
    /// Zone de déclenchement : en Play, expose `obj.triggered = true` au script quand
    /// le joueur (premier objet scripté) entre dans l'AABB de cet objet.
    #[serde(default)]
    pub trigger: bool,
    /// Son spatialisé : le volume au lancement décroît avec la distance à la caméra.
    #[serde(default)]
    pub audio_spatial: bool,

    // --- Composants mobiles Android (Sprint 41) ---
    /// Fait de cet objet un objet **pilotable** (joystick/gyroscope/saut/attaque) : `None`
    /// pour la grande majorité des objets d'une scène (décor, ennemis, pièces...), qui
    /// n'ont pas besoin de ces champs. Regroupe ce qui était 8 champs plats séparés
    /// (composant optionnel plutôt qu'un ECS complet — cf. discussion d'architecture).
    #[serde(default)]
    pub controller: Option<Controller>,
    /// Vibration Feedback : durée (ms) du retour haptique quand l'objet est tapé (0 = off).
    /// Reste hors de `Controller` : s'applique à tout objet tactile, pas seulement pilotable.
    #[serde(default)]
    pub vibrate_on_tap: u32,
    /// Cible valide pour l'attaque du joueur (cf. `Scene::attack_at`) : un ennemi vaincu
    /// devient invisible, puis réapparaît après `respawn_delay` (0 = ne réapparaît pas).
    #[serde(default)]
    pub attackable: bool,
    /// Ancre visuelle de l'effet d'attaque (au plus un objet par scène) : téléportée sur
    /// la cible touchée et affichée brièvement par `App` quand une attaque porte (cf.
    /// `AppState::attack_flash`). N'a aucun effet tant qu'aucune attaque ne porte.
    #[serde(default)]
    pub is_attack_fx: bool,
    /// Action déclenchée sans script quand l'objet est tapé (Touch Area requise).
    #[serde(default)]
    pub tap_action: TapAction,
    /// Objet visible au rendu (mis à false par l'action « Masquer » ; rétabli à l'arrêt).
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Zone mortelle : si le joueur entre dans son AABB en Play, la partie est perdue.
    #[serde(default)]
    pub deadly: bool,
    /// Délai de réapparition (s) d'un collectible après ramassage (0 = ne réapparaît pas).
    /// > 0 ⇒ pièce **bonus** (score continu), hors objectif de victoire.
    #[serde(default)]
    pub respawn_delay: f32,
}

fn default_true() -> bool {
    true
}

fn default_jump_height() -> f32 {
    1.5
}

fn default_attack_range() -> f32 {
    1.4
}

/// Action déclenchée sans script quand l'objet est tapé en mode Play.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum TapAction {
    #[default]
    None,
    /// Change la couleur de l'objet (teinte vive variant à chaque tap).
    ChangeColor,
    /// Masque l'objet (ramassage) ; il réapparaît à l'arrêt du mode Play.
    Hide,
    /// Grossit l'objet à chaque tap (plafonné).
    Grow,
    /// Replace l'objet à sa position de départ (respawn).
    Respawn,
}

impl TapAction {
    pub fn label(self) -> &'static str {
        match self {
            TapAction::None => "Aucune",
            TapAction::ChangeColor => "Changer de couleur",
            TapAction::Hide => "Masquer (ramasser)",
            TapAction::Grow => "Grandir",
            TapAction::Respawn => "Réapparaître au départ",
        }
    }

    /// Toutes les variantes, pour les menus déroulants.
    pub const ALL: [TapAction; 5] = [
        TapAction::None,
        TapAction::ChangeColor,
        TapAction::Hide,
        TapAction::Grow,
        TapAction::Respawn,
    ];
}

/// Anime les collectibles (objets « à ramasser » encore visibles) : rotation continue
/// autour de Y pour signaler qu'ils sont ramassables. Rotation absolue dérivée du temps
/// (déterministe, sans dérive).
pub fn animate_collectible(o: &mut SceneObject, time: f32) {
    if o.tap_action == TapAction::Hide && o.visible {
        o.transform.rotation = Quat::from_rotation_y(time * 2.0);
    }
}

/// Applique l'action au tap d'un objet (sans script), en mode Play. `start` = position
/// de départ (snapshot d'entrée en Play), `time` = temps de jeu écoulé.
pub fn apply_tap_action(o: &mut SceneObject, start: Vec3, time: f32) {
    match o.tap_action {
        TapAction::None => {}
        TapAction::ChangeColor => o.color = hue_to_rgb(time * 0.37),
        TapAction::Hide => o.visible = false,
        TapAction::Grow => o.transform.scale = (o.transform.scale * 1.25).min(Vec3::splat(4.0)),
        TapAction::Respawn => o.transform.position = start,
    }
}

fn default_move_speed() -> f32 {
    3.0
}

impl Default for SceneObject {
    fn default() -> Self {
        Self {
            name: "Objet".into(),
            transform: Transform::default(),
            mesh: MeshKind::Cube,
            script: String::new(),
            physics: PhysicsKind::None,
            collider_shape: crate::runtime::physics::ColliderShape::Auto,
            audio_clip: String::new(),
            audio_autoplay: false,
            group: String::new(),
            color: white(),
            texture: String::new(),
            tappable: false,
            metallic: 0.0,
            roughness: default_roughness(),
            emissive: 0.0,
            trigger: false,
            audio_spatial: false,
            controller: None,
            vibrate_on_tap: 0,
            attackable: false,
            is_attack_fx: false,
            tap_action: TapAction::None,
            visible: true,
            deadly: false,
            respawn_delay: 0.0,
        }
    }
}

/// Couleur vive (teinte → RGB, saturation/valeur max) pour l'action « changer de couleur ».
/// `h` est en tours (0..1) ; h=0 rouge, 1/3 vert, 2/3 bleu.
pub fn hue_to_rgb(h: f32) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [r, g, b]
}

fn default_roughness() -> f32 {
    0.6
}

fn white() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_physics() -> PhysicsKind {
    PhysicsKind::None
}

#[derive(Serialize, Deserialize, Default)]
pub struct Scene {
    pub objects: Vec<SceneObject>,
    #[serde(default)]
    pub imported: Vec<ImportedMesh>,
    /// Groupes (dossiers) créés par l'utilisateur, y compris vides (ordre conservé).
    #[serde(default)]
    pub groups: Vec<String>,
    /// Éclairage de la scène (direction, couleur, ambiante).
    #[serde(default)]
    pub light: Light,
    /// Lumières ponctuelles (position + couleur + intensité + portée).
    #[serde(default)]
    pub point_lights: Vec<PointLight>,
    /// Contrôles tactiles mobiles (joystick + boutons), exposés aux scripts Lua.
    #[serde(default)]
    pub mobile: MobileControls,
    /// En mode Play, la caméra suit le premier objet scripté (« joueur »).
    #[serde(default)]
    pub camera_follow: bool,
    /// Caméra de jeu : point de vue appliqué à l'entrée en mode Play (None = orbite éditeur).
    #[serde(default)]
    pub game_camera: Option<GameCamera>,
}

/// Point de vue de jeu (mêmes paramètres que la caméra orbitale), appliqué en Play.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct GameCamera {
    pub target: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

/// Configuration des contrôles tactiles affichés en mode Play / Player.
/// Le joystick et chaque bouton nommé sont lisibles depuis Lua via `input`.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct MobileControls {
    /// Affiche un joystick virtuel (coin bas-gauche).
    pub joystick: bool,
    /// Boutons tactiles nommés (coin bas-droite).
    pub buttons: Vec<String>,
    /// Zone tactile plein écran : un tap n'importe où expose `input.btn.touch` au script.
    #[serde(default)]
    pub touch_zone: bool,
    /// Affiche la barre de vie du HUD (pilotée par `set_health` côté script).
    #[serde(default)]
    pub health_bar: bool,
    /// Screen Safe Area : rentre les contrôles/HUD dans une marge sûre (encoche, bords arrondis).
    #[serde(default)]
    pub safe_area: bool,
}

impl MobileControls {
    /// Au moins un contrôle est-il actif ?
    pub fn any(&self) -> bool {
        self.joystick || !self.buttons.is_empty() || self.touch_zone || self.health_bar
    }
}

/// Schéma JSON simplifié produit par l'IA pour générer une scène entière.
#[derive(Deserialize)]
struct SceneSpec {
    #[serde(default)]
    objects: Vec<ObjSpec>,
    #[serde(default)]
    joystick: bool,
    #[serde(default)]
    buttons: Vec<String>,
    #[serde(default)]
    camera_follow: bool,
}

#[derive(Deserialize)]
struct ObjSpec {
    #[serde(default = "unnamed")]
    name: String,
    #[serde(default)]
    mesh: String,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    z: f32,
    #[serde(default = "white")]
    color: [f32; 3],
    #[serde(default)]
    script: String,
    #[serde(default)]
    physics: String,
    #[serde(default)]
    tappable: bool,
}

fn unnamed() -> String {
    "Objet".to_string()
}

/// Lumière directionnelle de la scène + lumière ambiante.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Light {
    pub dir: [f32; 3],
    pub color: [f32; 3],
    pub ambient: f32,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            dir: [0.5, 1.0, 0.3],
            color: [1.0, 1.0, 1.0],
            ambient: 0.25,
        }
    }
}

/// Lumière ponctuelle, ou **spot** (cône) si `spot_angle > 0`.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PointLight {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    /// Direction du cône (spot). Ignorée si `spot_angle == 0`.
    #[serde(default = "spot_dir_default")]
    pub spot_dir: [f32; 3],
    /// Demi-angle du cône en degrés (0 = lumière ponctuelle omnidirectionnelle).
    #[serde(default)]
    pub spot_angle: f32,
}

fn spot_dir_default() -> [f32; 3] {
    [0.0, -1.0, 0.0]
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            position: [0.0, 2.0, 0.0],
            color: [1.0, 0.9, 0.7],
            intensity: 1.0,
            range: 8.0,
            spot_dir: spot_dir_default(),
            spot_angle: 0.0,
        }
    }
}

/// Nombre maximal de lumières ponctuelles prises en compte par le shader.
pub const MAX_POINT_LIGHTS: usize = 8;

/// Constructeur d'objet aux valeurs par défaut (réduit le boilerplate des démos).
fn demo_obj(name: &str, mesh: MeshKind, pos: Vec3) -> SceneObject {
    SceneObject {
        name: name.into(),
        transform: Transform::from_pos(pos),
        mesh,
        script: String::new(),
        physics: PhysicsKind::None,
        collider_shape: crate::runtime::physics::ColliderShape::Auto,
        audio_clip: String::new(),
        audio_autoplay: false,
        group: String::new(),
        color: white(),
        texture: String::new(),
        tappable: false,
        metallic: 0.0,
        roughness: 0.6,
        emissive: 0.0,
        trigger: false,
        audio_spatial: false,
        ..Default::default()
    }
}

/// Nombre de niveaux de la démo contrôleur (cf. `Scene::controller_level`).
pub const CONTROLLER_LEVELS: u32 = 2;

impl Scene {
    /// Démo « contrôleur » **sans script** (niveau 1) : joueur pilotable au joystick,
    /// saut, collisions, pièces à ramasser, lave à éviter.
    pub fn controller_demo() -> Self {
        Self::controller_level(1)
    }

    /// Niveau `level` (1-based) de la démo contrôleur. Les niveaux supérieurs sont plus
    /// grands/chargés (plus de pièces, lave plus large, bonus plus fréquents).
    pub fn controller_level(level: u32) -> Self {
        let lvl = level.max(1);
        let hard = (lvl - 1) as f32; // 0 au niveau 1, 1 au niveau 2, …

        // Sol statique (teinte qui varie par niveau pour les distinguer).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.30 + 0.12 * hard, 0.5 - 0.08 * hard, 0.42];

        // Joueur pilotable : Input Receiver + saut sur le bouton « Saut ».
        // Démarre au bord (pas sur la lave centrale).
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, -6.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Attaque au corps-à-corps : vainc les ennemis `attackable` à portée (cf.
        // `Scene::attack_at`), sur pression du bouton tactile « Attaque » ou de la
        // touche J (desktop, cf. `PlayerInput::attack`).
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.6,
            attack_button: "Attaque".into(),
            attack_range: 1.5,
            ..Default::default()
        });

        // Effet visuel du coup : sphère blanche invisible par défaut, téléportée sur la
        // cible et affichée brièvement par `App` quand une attaque porte (cf.
        // `AppState::attack_flash`) — rend le coup lisible, pas juste sonore.
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.is_attack_fx = true;
        fx.visible = false;
        let mut objects = vec![sol, joueur, fx];

        // --- Murs de pourtour : enferment l'aire de jeu (le joueur ne tombe plus) ---
        // Le sol (plan unité × 16) couvre [-8, 8] ; on pose 4 murs statiques aux bords.
        let half = 7.5_f32;
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.45, 0.5, 0.62];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.6, -half),
            Vec3::new(16.0, 1.2, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.6, half),
            Vec3::new(16.0, 1.2, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.6, 0.0),
            Vec3::new(0.5, 1.2, 16.0),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.6, 0.0),
            Vec3::new(0.5, 1.2, 16.0),
        );

        // Mare de lave **au centre** (plus large aux niveaux supérieurs) : à contourner.
        // Note : le mesh Plane a une épaisseur visuelle nulle (y=0 pour tous les sommets),
        // donc l'échelle Y ne change rien au rendu — on s'en sert pour épaissir l'AABB de
        // collision verticalement (≈0.6 m autour du sol) afin que la zone mortelle détecte
        // fiablement un joueur qui marche dessus (capsule au repos ~y=0.5), tout en restant
        // franchissable en sautant par-dessus (le pic du saut dépasse cette plage).
        let lave_s = 3.0 + hard;
        let mut lave = demo_obj("Lave", MeshKind::Plane, Vec3::new(0.0, 0.02, 0.0));
        lave.transform = lave
            .transform
            .with_scale(Vec3::new(lave_s, 30.0, lave_s));
        lave.color = [0.95, 0.3, 0.1];
        lave.emissive = 0.7;
        lave.deadly = true;
        // Bouillonnement : la teinte pulse (deux fréquences superposées) sans toucher à
        // l'échelle Y (réservée à l'épaisseur de collision, cf. note ci-dessus).
        lave.script = "\
local b = 0.5 + 0.5 * math.sin(time * 2.2) + 0.25 * math.sin(time * 5.3)\n\
obj.r = 0.85 + 0.15 * b; obj.g = 0.22 + 0.18 * b; obj.b = 0.05 + 0.1 * b"
            .into();
        objects.push(lave);

        // Bulles de lave décoratives : jaillissent puis retombent en boucle, déphasées,
        // pour animer la surface (aucune collision/danger propre : la mare mère suffit).
        let bub_r = (lave_s * 0.5 - 0.4).max(0.3);
        for (n, (bx, bz, ph)) in [
            (0.5_f32, -0.3_f32, 0.0_f32),
            (-0.4, 0.4, 1.1),
            (0.1, 0.6, 2.3),
            (-0.5, -0.5, 3.6),
            (0.6, 0.1, 4.8),
        ]
        .into_iter()
        .enumerate()
        {
            let pos = Vec3::new(bx * bub_r, 0.05, bz * bub_r);
            let mut bubble = demo_obj(&format!("Bulle Lave {}", n + 1), MeshKind::Sphere, pos);
            bubble.color = [1.0, 0.5, 0.15];
            bubble.emissive = 1.0;
            bubble.script = format!(
                "local cyc = (time * 0.6 + {ph}) % 2.0\n\
                 local h = math.max(0.0, math.sin(cyc * math.pi))\n\
                 obj.y = 0.02 + h * 0.4\n\
                 obj.sx = 0.12 + h * 0.28; obj.sy = 0.12 + h * 0.28; obj.sz = 0.12 + h * 0.28"
            );
            objects.push(bubble);
        }

        // --- Pont surélevé traversant la lave (axe Z) : raccourci risqué mais direct.
        // Reste hors de portée verticale de la lave (marge ≈0.23 m) — sûr tant qu'on ne
        // tombe pas sur les côtés, ce qui ramène au niveau du sol au-dessus de la lave
        // (mort instantanée). Récompensé par une gemme suprême flottant en son centre.
        let bridge_half = lave_s * 0.5 + 0.8;
        let mut bridge = demo_obj("Pont", MeshKind::Cube, Vec3::new(0.0, 1.0, 0.0));
        bridge.transform = bridge
            .transform
            .with_scale(Vec3::new(0.9, 0.3, bridge_half * 2.0));
        bridge.physics = PhysicsKind::Static;
        bridge.color = [0.4, 0.36, 0.42];
        bridge.metallic = 0.25;
        bridge.roughness = 0.5;
        objects.push(bridge);

        let mut supreme = demo_obj("Gemme Suprême", MeshKind::Sphere, Vec3::new(0.0, 1.75, 0.0));
        supreme.transform = supreme.transform.with_scale(Vec3::splat(0.5));
        supreme.color = [0.85, 0.3, 0.95];
        supreme.emissive = 1.1;
        supreme.metallic = 0.5;
        supreme.tappable = true;
        supreme.tap_action = TapAction::Hide;
        supreme.respawn_delay = 7.0 - hard;
        objects.push(supreme);

        // Piliers-obstacles aux diagonales, surmontés d'une **étoile bonus** (en hauteur,
        // atteignable au saut ; réapparaît → score continu).
        for (n, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let base = Vec3::new(sx * 4.3, 0.0, sz * 4.3);
            let mut pil = demo_obj(
                &format!("Pilier {}", n + 1),
                MeshKind::Cube,
                base + Vec3::Y * 0.7,
            );
            pil.transform = pil.transform.with_scale(Vec3::new(0.8, 1.4, 0.8));
            pil.physics = PhysicsKind::Static;
            pil.color = [0.5, 0.52, 0.6];
            objects.push(pil);

            let mut star = demo_obj(
                &format!("Étoile {}", n + 1),
                MeshKind::Sphere,
                base + Vec3::Y * 1.9,
            );
            star.transform = star.transform.with_scale(Vec3::splat(0.4));
            star.color = [0.55, 0.85, 1.0];
            star.emissive = 0.8;
            star.tappable = true;
            star.tap_action = TapAction::Hide;
            star.respawn_delay = 4.0 - hard; // réapparition plus rapide au niveau 2
            objects.push(star);
        }

        // --- Pièces-objectif : anneaux générés automatiquement autour de la lave ---
        let rings: &[(u32, f32)] = if hard > 0.5 {
            &[(6, 3.8), (8, 6.4)]
        } else {
            &[(6, 3.4), (6, 6.2)]
        };
        let mut p = 0;
        for &(ring, radius) in rings {
            for k in 0..ring {
                // anneau extérieur décalé d'un demi-pas (disposition en quinconce).
                let off = if radius > 5.0 { 0.5 } else { 0.0 };
                let angle = (k as f32 + off) / ring as f32 * std::f32::consts::TAU;
                let pos = Vec3::new(angle.cos() * radius, 0.5, angle.sin() * radius);
                p += 1;
                let mut gem = demo_obj(&format!("Pièce {p}"), MeshKind::Sphere, pos);
                gem.transform = gem.transform.with_scale(Vec3::splat(0.45));
                gem.color = [1.0, 0.85, 0.2];
                gem.emissive = 0.5;
                gem.metallic = 0.6;
                gem.roughness = 0.25;
                gem.tappable = true;
                gem.tap_action = TapAction::Hide;
                objects.push(gem);
            }
        }

        // --- Escalier + plateforme surélevée côté ouest : défi de plateforme optionnel,
        // récompensé par des pièces bonus et un trophée (ne bloque pas la victoire).
        for i in 0..3u32 {
            let sy = 0.3 + i as f32 * 0.3;
            let sx = -7.0 + i as f32 * 0.65;
            let mut step = demo_obj(
                &format!("Marche {}", i + 1),
                MeshKind::Cube,
                Vec3::new(sx, sy * 0.5, 0.0),
            );
            step.transform = step.transform.with_scale(Vec3::new(0.75, sy, 2.2));
            step.physics = PhysicsKind::Static;
            step.color = [0.55, 0.5, 0.4];
            objects.push(step);
        }
        let mut podium = demo_obj("Plateforme", MeshKind::Cube, Vec3::new(-5.0, 0.95, 0.0));
        podium.transform = podium.transform.with_scale(Vec3::new(1.7, 0.3, 2.6));
        podium.physics = PhysicsKind::Static;
        podium.color = [0.52, 0.48, 0.58];
        podium.metallic = 0.35;
        podium.roughness = 0.35;
        objects.push(podium);

        // Deux pièces bonus flanquant le trophée, en hauteur sur la plateforme.
        for (n, dz) in [(1, -0.8), (2, 0.8)] {
            let mut bonus = demo_obj(
                &format!("Pièce Bonus {n}"),
                MeshKind::Sphere,
                Vec3::new(-5.0, 1.5, dz),
            );
            bonus.transform = bonus.transform.with_scale(Vec3::splat(0.4));
            bonus.color = [0.4, 0.9, 0.6];
            bonus.emissive = 0.7;
            bonus.tappable = true;
            bonus.tap_action = TapAction::Hide;
            bonus.respawn_delay = 6.0 - hard;
            objects.push(bonus);
        }
        // Trophée : bonus le plus précieux (score continu), au sommet de la plateforme.
        let mut trophy = demo_obj("Étoile Trophée", MeshKind::Sphere, Vec3::new(-5.0, 2.1, 0.0));
        trophy.transform = trophy.transform.with_scale(Vec3::splat(0.55));
        trophy.color = [1.0, 0.75, 0.25];
        trophy.emissive = 1.0;
        trophy.metallic = 0.4;
        trophy.tappable = true;
        trophy.tap_action = TapAction::Hide;
        trophy.respawn_delay = 5.0 - hard;
        objects.push(trophy);

        // --- Portique décoratif encadrant l'entrée côté sud (lisibilité + ambiance) ---
        for sx in [-1.6_f32, 1.6] {
            let mut post = demo_obj("Pilier Portique", MeshKind::Cube, Vec3::new(sx, 1.1, -5.6));
            post.transform = post.transform.with_scale(Vec3::new(0.5, 2.2, 0.5));
            post.physics = PhysicsKind::Static;
            post.color = [0.45, 0.4, 0.5];
            post.metallic = 0.5;
            post.roughness = 0.3;
            objects.push(post);
        }
        let mut lintel = demo_obj("Linteau Portique", MeshKind::Cube, Vec3::new(0.0, 2.35, -5.6));
        lintel.transform = lintel.transform.with_scale(Vec3::new(3.6, 0.4, 0.5));
        lintel.physics = PhysicsKind::Static;
        lintel.color = [0.45, 0.4, 0.5];
        lintel.metallic = 0.5;
        lintel.roughness = 0.3;
        objects.push(lintel);

        // --- Ennemis patrouilleurs : hazards mobiles (scriptés), infligent des **dégâts
        // progressifs** au contact (via `damage()`) plutôt qu'une mort instantanée comme
        // la lave — plus indulgent, encourage à esquiver/se replier plutôt qu'à figer la
        // partie au premier effleurement. Plus rapides et plus punitifs au niveau 2 (`hard`).
        // Pulsent en rouge (menace visuelle). Vaincus par l'attaque du joueur (à portée) :
        // disparaissent puis réapparaissent après un répit, plutôt que d'être éliminés
        // définitivement (le niveau reste tendu même après un bon coup).
        let enemy_speed = 1.0 + 0.4 * hard;
        let dmg_rate = 0.9 + 0.3 * hard;
        let mut enemy = |name: &str, pos: Vec3, script: String| {
            let mut e = demo_obj(name, MeshKind::Sphere, pos);
            e.transform = e.transform.with_scale(Vec3::new(0.7, 0.6, 0.7));
            e.color = [0.85, 0.08, 0.08];
            e.emissive = 0.5;
            e.trigger = true;
            e.attackable = true;
            e.respawn_delay = 8.0 - hard;
            e.script = script;
            objects.push(e);
        };
        // Sentinelle sud : va-et-vient devant l'entrée, le long du mur sud.
        enemy(
            "Ennemi Sentinelle",
            Vec3::new(0.0, 0.5, -7.0),
            format!(
                "local s = {enemy_speed}\n\
                 obj.x = math.sin(time * s) * 3.0\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );
        // Rôdeur est : va-et-vient le long du couloir est, entre le mur et les piliers.
        enemy(
            "Ennemi Rôdeur",
            Vec3::new(5.6, 0.5, 0.0),
            format!(
                "local s = {enemy_speed}\n\
                 obj.z = math.sin(time * s * 0.8) * 3.0\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );
        // Gardien du trésor : tourne en orbite près de la gemme suprême / du pont.
        enemy(
            "Ennemi Gardien",
            Vec3::new(2.2, 0.5, -2.2),
            format!(
                "local s = {enemy_speed}\n\
                 obj.x = 2.2 + math.cos(time * s * 0.9) * 1.1\n\
                 obj.z = -2.2 + math.sin(time * s * 0.9) * 1.1\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );

        // --- Torches aux 4 coins de l'arène (flamme émissive + halo de lumière chaude) ---
        let mut lights = vec![PointLight {
            // Lumière ponctuelle chaude au-dessus de l'arène (ambiance + lisibilité).
            position: [0.0, 6.0, 0.0],
            color: [1.0, 0.92, 0.78],
            intensity: 1.4,
            range: 16.0,
            ..PointLight::default()
        }];
        for (n, (cx, cz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let base = Vec3::new(cx * 6.9, 0.0, cz * 6.9);
            let mut torch = demo_obj(&format!("Torche {}", n + 1), MeshKind::Cube, base + Vec3::Y * 0.8);
            torch.transform = torch.transform.with_scale(Vec3::new(0.3, 1.6, 0.3));
            torch.physics = PhysicsKind::Static;
            torch.color = [0.3, 0.28, 0.3];
            objects.push(torch);

            let mut flame = demo_obj(&format!("Flamme {}", n + 1), MeshKind::Sphere, base + Vec3::Y * 1.7);
            flame.transform = flame.transform.with_scale(Vec3::splat(0.3));
            flame.color = [1.0, 0.55, 0.15];
            flame.emissive = 1.2;
            // Vacillement (déphasé par torche) : taille + teinte fluctuent, deux fréquences
            // superposées pour un scintillement moins mécanique qu'une simple sinusoïde.
            let phase = n as f32 * 1.7;
            flame.script = format!(
                "local f = 0.75 + 0.15 * math.sin(time * 9.0 + {phase}) \
                 + 0.10 * math.sin(time * 23.0 + {phase} * 2.0)\n\
                 obj.sx = 0.3 * f; obj.sy = 0.3 * f; obj.sz = 0.3 * f\n\
                 obj.r = 1.0; obj.g = 0.45 + 0.2 * f; obj.b = 0.1 + 0.15 * f"
            );
            objects.push(flame);

            lights.push(PointLight {
                position: (base + Vec3::Y * 1.7).into(),
                color: [1.0, 0.6, 0.25],
                intensity: 0.9,
                range: 6.0,
                ..PointLight::default()
            });
        }
        // Lueur rouge au ras de la lave : renforce le danger visuel de la zone mortelle.
        lights.push(PointLight {
            position: [0.0, 0.6, 0.0],
            color: [1.0, 0.35, 0.1],
            intensity: 1.1,
            range: 7.0,
            ..PointLight::default()
        });
        // Lueur violette autour de la gemme suprême, sur le pont : signale la récompense
        // la plus prestigieuse du niveau, visible de loin par contraste avec la lave.
        lights.push(PointLight {
            position: [0.0, 2.0, 0.0],
            color: [0.85, 0.4, 1.0],
            intensity: 0.8,
            range: 5.0,
            ..PointLight::default()
        });

        Scene {
            objects,
            camera_follow: true,
            point_lights: lights,
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Tour d'ascension » : style de jeu très différent de la démo contrôleur
    /// (arène de combat) — pur platforming vertical, sans ennemi ni combat. Plateformes
    /// en spirale à gravir jusqu'au sommet ; une chute hors des plateformes est une mort
    /// instantanée (vide en contrebas), ce qui remplace la lave comme unique danger.
    pub fn tower_demo() -> Self {
        let mut objects = Vec::new();

        // Sol de départ (petit, juste pour l'atterrissage initial — pas d'arène close ici,
        // le style est vertical, pas horizontal).
        let mut sol = demo_obj("Socle", MeshKind::Cylinder, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(4.0, 0.6, 4.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.4, 0.5];
        objects.push(sol);

        // Joueur pilotable : mêmes contrôles que la démo contrôleur (joystick + saut),
        // mais ici la précision de saut est ce qui compte, pas le combat.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.7,
            ..Default::default()
        });
        objects.push(joueur);

        // Vide mortel loin en contrebas : toute chute hors des plateformes est une mort
        // instantanée (remplace la lave comme unique danger de ce style de niveau). Même
        // technique que la lave : l'échelle Y épaissit l'AABB de collision (le mesh Plane
        // a une épaisseur visuelle nulle, cf. note dans `controller_level`) pour détecter
        // fiablement le joueur en chute malgré le pas de simulation fixe.
        let mut vide = demo_obj("Vide", MeshKind::Plane, Vec3::new(0.0, -4.0, 0.0));
        vide.transform = vide.transform.with_scale(Vec3::new(80.0, 60.0, 80.0));
        vide.color = [0.05, 0.05, 0.12];
        vide.deadly = true;
        objects.push(vide);

        // --- Plateformes en spirale ascendante : 4 positions en rotation (avant/droite/
        // arrière/gauche), qui montent d'un cran à chaque tour. Chaque plateforme porte une
        // gemme-objectif (obligatoire pour gagner) légèrement au-dessus, au centre.
        const N: usize = 16;
        for i in 0..N {
            let angle_step = (i % 4) as f32;
            let (dx, dz) = match angle_step as u32 {
                0 => (0.0, -2.6),
                1 => (2.6, 0.0),
                2 => (0.0, 2.6),
                _ => (-2.6, 0.0),
            };
            let y = 1.4 + i as f32 * 1.75;
            let pos = Vec3::new(dx, y, dz);

            let mut plat = demo_obj(&format!("Plateforme {}", i + 1), MeshKind::Cylinder, pos);
            plat.transform = plat.transform.with_scale(Vec3::new(1.6, 0.35, 1.6));
            plat.physics = PhysicsKind::Static;
            // Dégradé froid (bleu nuit → cyan clair) à mesure qu'on grimpe : lisibilité de
            // la progression même sans HUD de score consulté.
            let t = i as f32 / (N - 1) as f32;
            plat.color = [0.25 + 0.15 * t, 0.4 + 0.35 * t, 0.55 + 0.35 * t];
            plat.metallic = 0.3;
            plat.roughness = 0.35;
            objects.push(plat);

            let mut gem = demo_obj(&format!("Gemme {}", i + 1), MeshKind::Sphere, pos + Vec3::Y * 0.85);
            gem.transform = gem.transform.with_scale(Vec3::splat(0.4));
            gem.color = [0.6, 0.9, 1.0];
            gem.emissive = 0.7;
            gem.tappable = true;
            gem.tap_action = TapAction::Hide;
            objects.push(gem);
        }

        // Trophée décoratif au sommet, au-dessus de la dernière plateforme : bonus (score
        // continu, ne bloque pas la victoire — gagner = avoir gravi toute la tour).
        let top = Vec3::new(0.0, 1.4 + (N - 1) as f32 * 1.75, 0.0)
            + match ((N - 1) % 4) as u32 {
                0 => Vec3::new(0.0, 0.0, -2.6),
                1 => Vec3::new(2.6, 0.0, 0.0),
                2 => Vec3::new(0.0, 0.0, 2.6),
                _ => Vec3::new(-2.6, 0.0, 0.0),
            };
        let mut trophy = demo_obj("Étoile Sommet", MeshKind::Sphere, top + Vec3::Y * 1.6);
        trophy.transform = trophy.transform.with_scale(Vec3::splat(0.55));
        trophy.color = [1.0, 0.85, 0.3];
        trophy.emissive = 1.1;
        trophy.tappable = true;
        trophy.tap_action = TapAction::Hide;
        trophy.respawn_delay = 6.0;
        objects.push(trophy);

        // Étoiles décoratives (ciel nocturne) : petits points statiques loin en hauteur,
        // pure ambiance — contraste avec les torches chaudes de la démo contrôleur.
        for i in 0..24 {
            let a = i as f32 * 2.399963; // angle doré : répartition sans motif visible
            let r = 6.0 + (i % 5) as f32 * 3.0;
            let h = 4.0 + (i * 7 % 40) as f32;
            let mut star = demo_obj(
                &format!("Étoile Ciel {}", i + 1),
                MeshKind::Sphere,
                Vec3::new(a.cos() * r, h, a.sin() * r),
            );
            star.transform = star.transform.with_scale(Vec3::splat(0.12));
            star.color = [0.85, 0.9, 1.0];
            star.emissive = 1.0;
            objects.push(star);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 6.0, 0.0],
                    color: [0.75, 0.85, 1.0],
                    intensity: 1.2,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: top.into(),
                    color: [1.0, 0.9, 0.7],
                    intensity: 1.3,
                    range: 10.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Course infinie » (style Temple Run) : 3ᵉ style de jeu, distinct de l'arène
    /// de combat et de la tour de platforming — course automatique en avant, le joueur ne
    /// contrôle que le changement de voie (gauche/centre/droite) et le saut. Obstacles à
    /// esquiver (voie) ou à sauter, pièces à ramasser, ligne d'arrivée obligatoire.
    /// (Piste longue et procédurale plutôt que réellement infinie : le moteur n'a pas de
    /// génération/dé-spawn à la volée — cf. `Scene::temple_run_demo` pour le détail.)
    pub fn temple_run_demo() -> Self {
        const LANES: [f32; 3] = [-2.2, 0.0, 2.2];
        const TRACK_LEN: f32 = 190.0;

        let mut objects = Vec::new();

        // Sol unique sur toute la longueur de la piste.
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, TRACK_LEN * 0.5));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(8.0, 1.0, TRACK_LEN + 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.42, 0.38, 0.5];
        objects.push(sol);

        // Murs latéraux : bloquent (sans tuer) toute sortie des 3 voies.
        for sx in [-3.6_f32, 3.6] {
            let mut wall = demo_obj(
                "Mur Voie",
                MeshKind::Cube,
                Vec3::new(sx, 0.9, TRACK_LEN * 0.5),
            );
            wall.transform = wall
                .transform
                .with_scale(Vec3::new(0.4, 1.8, TRACK_LEN + 10.0));
            wall.physics = PhysicsKind::Static;
            wall.color = [0.3, 0.28, 0.4];
            objects.push(wall);
        }

        // Joueur : course automatique en +Z, le joystick/clavier X ne pilote que la voie.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, -3.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 7.0,
            auto_run_speed: 5.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });
        objects.push(joueur);

        // --- Génération procédurale de la piste : motifs répétés tous les 6 m, densité
        // croissante avec la distance (difficulté progressive, comme un vrai endless runner).
        let seg_len = 6.0_f32;
        let n_segments = (TRACK_LEN / seg_len) as u32;
        for seg in 0..n_segments {
            let z = 8.0 + seg as f32 * seg_len;
            // Les 2 premiers segments sont un couloir d'échauffement (aucun obstacle).
            if seg < 2 {
                continue;
            }
            match seg % 5 {
                0 => {
                    // Haie à sauter (barre pleine largeur, franchissable au saut : sa
                    // hauteur réelle de 1,1 m couvre naturellement l'AABB nécessaire pour
                    // détecter un joueur debout sans la traverser en l'air — contrairement
                    // à un mesh Plane plat, un Cube épais n'a pas besoin de l'astuce
                    // d'échelle Y utilisée pour la lave (cf. `controller_level`).
                    let mut haie = demo_obj("Haie", MeshKind::Cube, Vec3::new(0.0, 0.55, z));
                    haie.transform = haie.transform.with_scale(Vec3::new(7.0, 1.1, 0.6));
                    haie.color = [0.75, 0.35, 0.2];
                    haie.deadly = true;
                    objects.push(haie);
                }
                1 => {
                    // Barrage : 2 des 3 voies bloquées (hauteur non franchissable au saut),
                    // la voie ouverte tourne à chaque occurrence pour ne pas être mémorisable.
                    let open = (seg / 5) % 3;
                    for (lane, &lx) in LANES.iter().enumerate() {
                        if lane as u32 == open {
                            continue;
                        }
                        let mut bar = demo_obj("Barrage", MeshKind::Cube, Vec3::new(lx, 1.0, z));
                        bar.transform = bar.transform.with_scale(Vec3::new(1.8, 2.0, 0.6));
                        bar.color = [0.6, 0.2, 0.2];
                        bar.deadly = true;
                        objects.push(bar);
                    }
                }
                2 => {
                    // Arc de pièces sur les 3 voies : encourage à zigzaguer.
                    for &lx in &LANES {
                        let mut coin =
                            demo_obj("Pièce", MeshKind::Sphere, Vec3::new(lx, 1.0, z));
                        coin.transform = coin.transform.with_scale(Vec3::splat(0.4));
                        coin.color = [1.0, 0.85, 0.2];
                        coin.emissive = 0.6;
                        coin.tappable = true;
                        coin.tap_action = TapAction::Hide;
                        // Bonus (score continu) : ne bloque pas la victoire, seule la
                        // ligne d'arrivée (plus bas) compte comme objectif obligatoire.
                        coin.respawn_delay = 999.0;
                        objects.push(coin);
                    }
                }
                3 => {
                    // Ligne de pièces dans une seule voie (récompense un choix de trajectoire).
                    let lane = (seg / 3) % 3;
                    let mut coin = demo_obj(
                        "Pièce",
                        MeshKind::Sphere,
                        Vec3::new(LANES[lane as usize], 1.0, z),
                    );
                    coin.transform = coin.transform.with_scale(Vec3::splat(0.4));
                    coin.color = [1.0, 0.85, 0.2];
                    coin.emissive = 0.6;
                    coin.tappable = true;
                    coin.tap_action = TapAction::Hide;
                    coin.respawn_delay = 999.0;
                    objects.push(coin);
                }
                _ => {} // couloir de respiration : pas d'obstacle
            }
        }

        // Ligne d'arrivée : seul objectif obligatoire (victoire = l'atteindre), un portique
        // lumineux bien visible + une étoile à ramasser.
        let finish_z = 8.0 + n_segments as f32 * seg_len + 4.0;
        for sx in [-3.2_f32, 3.2] {
            let mut post = demo_obj("Pilier Arrivée", MeshKind::Cube, Vec3::new(sx, 1.4, finish_z));
            post.transform = post.transform.with_scale(Vec3::new(0.5, 2.8, 0.5));
            post.physics = PhysicsKind::Static;
            post.color = [0.9, 0.75, 0.2];
            post.metallic = 0.5;
            objects.push(post);
        }
        let mut lintel = demo_obj(
            "Linteau Arrivée",
            MeshKind::Cube,
            Vec3::new(0.0, 3.0, finish_z),
        );
        lintel.transform = lintel.transform.with_scale(Vec3::new(6.9, 0.4, 0.5));
        lintel.physics = PhysicsKind::Static;
        lintel.color = [0.9, 0.75, 0.2];
        lintel.metallic = 0.5;
        objects.push(lintel);

        let mut finish = demo_obj("Étoile Arrivée", MeshKind::Sphere, Vec3::new(0.0, 1.5, finish_z));
        finish.transform = finish.transform.with_scale(Vec3::splat(0.6));
        finish.color = [1.0, 0.9, 0.3];
        finish.emissive = 1.2;
        finish.tappable = true;
        finish.tap_action = TapAction::Hide;
        // respawn_delay = 0 (défaut) ⇒ objectif obligatoire : seule pièce dont la victoire dépend.
        objects.push(finish);

        Scene {
            objects,
            camera_follow: true,
            // Éclairage réparti le long de la piste (190 m) : la lumière directionnelle
            // par défaut (`light`) couvre l'ambiance générale, ces points ponctuels
            // renforcent la lisibilité aux endroits clés (départ, milieu, arrivée).
            point_lights: [10.0, 70.0, 130.0, finish_z]
                .into_iter()
                .map(|z| PointLight {
                    position: [0.0, 8.0, z],
                    color: [1.0, 0.95, 0.85],
                    intensity: 1.2,
                    range: 26.0,
                    ..PointLight::default()
                })
                .collect(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « gameplay complet » : joueur (joystick + gyroscope + saut + vibration),
    /// zone de danger qui retire de la vie (HUD), et cube tactile qui change de couleur.
    /// Montre toute l'API de script en une scène jouable.
    pub fn gameplay_demo() -> Self {
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.5, 0.4];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 0.5, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.script = "\
local s = 4.0
obj.x = obj.x + (input.jx + tilt.x) * s * dt
obj.z = obj.z - (input.jy + tilt.y) * s * dt
if input.btn.Saut then obj.y = 1.4; vibrate(40) else obj.y = 0.5 end"
            .into();

        let mut danger = demo_obj("Zone danger", MeshKind::Cube, Vec3::new(3.0, 0.5, 0.0));
        danger.color = [0.8, 0.2, 0.2];
        danger.emissive = 0.3;
        danger.trigger = true;
        danger.script = "\
if obj.triggered then set_health(0.25); vibrate(120) else set_health(1.0) end"
            .into();

        let mut bouton = demo_obj("Cube couleur", MeshKind::Cube, Vec3::new(-3.0, 0.5, 0.0));
        bouton.color = [0.3, 0.6, 0.9];
        bouton.tappable = true;
        bouton.script = "\
if obj.tapped then
  obj.r = (time * 0.7) % 1.0; obj.g = (time * 1.3) % 1.0; obj.b = (time * 1.9) % 1.0
end"
        .into();

        Scene {
            objects: vec![sol, joueur, danger, bouton],
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
        }
    }
}

impl Scene {
    /// Estimation grossière de l'occupation mémoire (octets) : `(objets, meshes importés,
    /// nombre de textures uniques)`. Pour le profiler mémoire (ordre de grandeur).
    pub fn memory_estimate(&self) -> (usize, usize, usize) {
        let mut obj_bytes = self.objects.len() * std::mem::size_of::<SceneObject>();
        let mut textures = std::collections::BTreeSet::new();
        for o in &self.objects {
            obj_bytes += o.name.len() + o.script.len() + o.texture.len() + o.audio_clip.len();
            if !o.texture.is_empty() {
                textures.insert(o.texture.as_str());
            }
        }
        let vsize = std::mem::size_of::<crate::gfx::mesh::Vertex>();
        let mesh_bytes: usize = self
            .imported
            .iter()
            .map(|m| m.data.vertices.len() * vsize + m.data.indices.len() * 4)
            .sum();
        (obj_bytes, mesh_bytes, textures.len())
    }

    /// AABB local d'un objet (primitive codée ou mesh importé).
    pub fn local_aabb(&self, mesh: MeshKind) -> (Vec3, Vec3) {
        match mesh {
            MeshKind::Cube | MeshKind::Sphere => (Vec3::splat(-0.5), Vec3::splat(0.5)),
            MeshKind::Plane => (Vec3::new(-0.5, -0.02, -0.5), Vec3::new(0.5, 0.02, 0.5)),
            MeshKind::Cylinder => (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)),
            MeshKind::Capsule => (Vec3::new(-0.25, -0.5, -0.25), Vec3::new(0.25, 0.5, 0.25)),
            MeshKind::Terrain => (Vec3::new(-0.5, -0.1, -0.5), Vec3::new(0.5, 0.1, 0.5)),
            MeshKind::Imported(i) => {
                let m = &self.imported[i as usize];
                (m.aabb_min, m.aabb_max)
            }
        }
    }

    /// Le point monde `p` est-il dans l'AABB monde de l'objet `o` ?
    pub fn world_aabb_contains(&self, o: &SceneObject, p: Vec3) -> bool {
        let (lmin, lmax) = self.local_aabb(o.mesh);
        let m = o.transform.matrix();
        let mut wmin = Vec3::splat(f32::INFINITY);
        let mut wmax = Vec3::splat(f32::NEG_INFINITY);
        for sx in [lmin.x, lmax.x] {
            for sy in [lmin.y, lmax.y] {
                for sz in [lmin.z, lmax.z] {
                    let q = (m * Vec3::new(sx, sy, sz).extend(1.0)).truncate();
                    wmin = wmin.min(q);
                    wmax = wmax.max(q);
                }
            }
        }
        p.cmpge(wmin).all() && p.cmple(wmax).all()
    }

    /// Le point `p` (position du joueur) touche-t-il une zone mortelle ?
    pub fn deadly_at(&self, p: Vec3) -> bool {
        self.objects
            .iter()
            .filter(|o| o.deadly)
            .any(|o| self.world_aabb_contains(o, p))
    }

    /// Ramassage par contact : masque (collecte) les collectibles encore visibles dont
    /// le centre est à moins de `radius` (+ leur rayon) du point `p` (position du joueur).
    /// Renvoie les **indices** des pièces ramassées cette frame (pour score + respawn).
    pub fn collect_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        let mut hit = Vec::new();
        for (i, o) in self.objects.iter_mut().enumerate() {
            if o.tap_action == TapAction::Hide && o.visible {
                let piece_r = o.transform.scale.max_element() * 0.5;
                if (o.transform.position - p).length() <= radius + piece_r {
                    o.visible = false;
                    hit.push(i);
                }
            }
        }
        hit
    }

    /// Résout une attaque du joueur en `p` (portée `radius`) : vainc (masque) les ennemis
    /// `attackable` encore visibles à portée. Renvoie les indices vaincus (pour score,
    /// son, et mise en file de réapparition côté `App`, comme les bonus).
    pub fn attack_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        let mut hit = Vec::new();
        for (i, o) in self.objects.iter_mut().enumerate() {
            if o.attackable && o.visible {
                let enemy_r = o.transform.scale.max_element() * 0.5;
                if (o.transform.position - p).length() <= radius + enemy_r {
                    o.visible = false;
                    hit.push(i);
                }
            }
        }
        hit
    }

    /// État des **pièces-objectif** (action « Masquer », **non** réapparaissantes) :
    /// `Some((ramassées, total))` si la scène en contient, sinon `None`. Les pièces bonus
    /// (`respawn_delay > 0`) ne comptent pas. `ramassées == total` ⇒ niveau gagné.
    pub fn collectibles(&self) -> Option<(usize, usize)> {
        let goal = |o: &&SceneObject| o.tap_action == TapAction::Hide && o.respawn_delay == 0.0;
        let total = self.objects.iter().filter(goal).count();
        if total == 0 {
            return None;
        }
        let collected = self
            .objects
            .iter()
            .filter(goal)
            .filter(|o| !o.visible)
            .count();
        Some((collected, total))
    }

    /// Sélectionne les indices des `max` lumières ponctuelles les **plus proches** de
    /// `cam` (culling/LOD de lumières : seules les plus pertinentes sont envoyées au
    /// shader quand la scène en compte plus que la limite). Ordre : de la plus proche
    /// à la plus éloignée. Si le nombre de lumières ≤ `max`, les renvoie toutes dans
    /// l'ordre d'origine (aucun tri).
    pub fn nearest_point_lights(&self, cam: Vec3, max: usize) -> Vec<usize> {
        let n = self.point_lights.len();
        if n <= max {
            return (0..n).collect();
        }
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            let da = (Vec3::from(self.point_lights[a].position) - cam).length_squared();
            let db = (Vec3::from(self.point_lights[b].position) - cam).length_squared();
            da.total_cmp(&db)
        });
        idx.truncate(max);
        idx
    }

    /// Recharge la géométrie des meshes importés depuis leurs fichiers (après désérialisation).
    pub fn reload_imported(&mut self) {
        for m in &mut self.imported {
            match import::load_gltf(&m.path) {
                Ok((data, min, max)) => {
                    m.data = data;
                    m.aabb_min = min;
                    m.aabb_max = max;
                }
                Err(e) => log::error!("Rechargement de {} échoué : {e}", m.path),
            }
        }
    }

    /// Scène **embarquée dans le binaire** (figée à la compilation depuis
    /// `assets/player_scene.json`, réécrite à chaque export). C'est le jeu que joue
    /// le mode Player d'un `.dmg`/`.apk`/`.ipa` exporté.
    pub fn embedded_player() -> Self {
        const JSON: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/player_scene.json"
        ));
        match serde_json::from_str::<Scene>(JSON) {
            Ok(mut s) => {
                s.reload_imported();
                s
            }
            Err(e) => {
                log::error!("Scène embarquée invalide ({e}) — retour à la démo.");
                Scene::demo()
            }
        }
    }

    /// Scène de démonstration : un sol, un cube, une sphère.
    pub fn demo() -> Self {
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls::default(),
            camera_follow: false,
            game_camera: None,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                        .with_scale(Vec3::new(10.0, 1.0, 10.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    audio_clip: String::new(),
                    audio_autoplay: false,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    audio_spatial: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Cube".into(),
                    transform: Transform::from_pos(Vec3::new(-1.2, -0.5, 0.0)),
                    mesh: MeshKind::Cube,
                    // exemple : tourne autour de Y à 60°/s en mode Play
                    script: "obj.ry = obj.ry + dt * 60.0".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    audio_clip: String::new(),
                    audio_autoplay: false,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    audio_spatial: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Sphère".into(),
                    transform: Transform::from_pos(Vec3::new(1.2, 2.5, 0.0)),
                    mesh: MeshKind::Sphere,
                    script: String::new(),
                    // tombe et rebondit sur le sol en mode Play
                    physics: PhysicsKind::Dynamic,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    audio_clip: String::new(),
                    audio_autoplay: false,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    audio_spatial: false,
                    ..Default::default()
                },
            ],
        }
    }

    /// Démo mobile « prête à jouer » : un sol, un personnage piloté au joystick
    /// (avec saut au bouton) et contrôles tactiles activés. Démontre toute la
    /// boucle joystick → script → rendu en mode Play.
    pub fn mobile_demo() -> Self {
        let player_script = "\
local speed = 4.0
obj.x = obj.x + input.jx * speed * dt
obj.z = obj.z - input.jy * speed * dt
if input.btn.Saut then obj.y = 1.4 else obj.y = 0.5 end";
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0))
                        .with_scale(Vec3::new(14.0, 1.0, 14.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    audio_clip: String::new(),
                    audio_autoplay: false,
                    group: String::new(),
                    color: [0.4, 0.5, 0.45],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    audio_spatial: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Joueur".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
                    mesh: MeshKind::Capsule,
                    script: player_script.into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    audio_clip: String::new(),
                    audio_autoplay: false,
                    group: String::new(),
                    color: [0.95, 0.6, 0.25],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    audio_spatial: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Bouton couleur".into(),
                    transform: Transform::from_pos(Vec3::new(2.5, 0.5, -1.0)),
                    mesh: MeshKind::Cube,
                    // Tap → couleur aléatoire (changeante) via le temps.
                    script: "if obj.tapped then\n  obj.r = (time * 0.7) % 1.0\n  obj.g = (time * 1.3) % 1.0\n  obj.b = (time * 1.9) % 1.0\nend".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    audio_clip: String::new(),
                    audio_autoplay: false,
                    group: String::new(),
                    color: [0.3, 0.6, 0.9],
                    texture: String::new(),
                    tappable: true,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    audio_spatial: false,
                    ..Default::default()
                },
            ],
        }
    }

    /// Construit une scène depuis le JSON contraint produit par l'IA (cf. `app::ai`).
    pub fn from_ai_json(json: &str) -> Result<Scene, String> {
        let spec: SceneSpec =
            serde_json::from_str(json).map_err(|e| format!("JSON de scène invalide : {e}"))?;
        let objects: Vec<SceneObject> = spec
            .objects
            .into_iter()
            .map(|o| SceneObject {
                name: o.name,
                transform: Transform::from_pos(Vec3::new(o.x, o.y, o.z)),
                mesh: match o.mesh.as_str() {
                    "sphere" => MeshKind::Sphere,
                    "plane" => MeshKind::Plane,
                    "cylinder" => MeshKind::Cylinder,
                    "capsule" => MeshKind::Capsule,
                    _ => MeshKind::Cube,
                },
                script: o.script,
                physics: match o.physics.as_str() {
                    "static" => PhysicsKind::Static,
                    "dynamic" => PhysicsKind::Dynamic,
                    _ => PhysicsKind::None,
                },
                collider_shape: crate::runtime::physics::ColliderShape::Auto,
                audio_clip: String::new(),
                audio_autoplay: false,
                group: String::new(),
                color: o.color,
                texture: String::new(),
                tappable: o.tappable,
                metallic: 0.0,
                roughness: 0.6,
                emissive: 0.0,
                trigger: false,
                audio_spatial: false,
                ..Default::default()
            })
            .collect();
        if objects.is_empty() {
            return Err("La scène générée ne contient aucun objet".into());
        }
        Ok(Scene {
            objects,
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: spec.joystick,
                buttons: spec.buttons,
                ..Default::default()
            },
            camera_follow: spec.camera_follow,
            game_camera: None,
        })
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, json)
    }

    pub fn load(path: &str) -> std::io::Result<Scene> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_to_rgb_primary_colors() {
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3);
        assert!(close(hue_to_rgb(0.0), [1.0, 0.0, 0.0]), "rouge");
        assert!(close(hue_to_rgb(1.0 / 3.0), [0.0, 1.0, 0.0]), "vert");
        assert!(close(hue_to_rgb(2.0 / 3.0), [0.0, 0.0, 1.0]), "bleu");
        // Périodicité : h et h+1 donnent la même couleur.
        assert!(close(hue_to_rgb(0.2), hue_to_rgb(1.2)), "période");
    }

    #[test]
    fn nearest_point_lights_picks_closest_to_camera() {
        let mut s = Scene::default();
        // 3 lumières à x = 0, 5, 10 ; caméra à l'origine.
        for x in [0.0, 5.0, 10.0] {
            s.point_lights.push(PointLight {
                position: [x, 0.0, 0.0],
                ..PointLight::default()
            });
        }
        // Limite 2 → garde les deux plus proches (x=0 puis x=5), dans l'ordre.
        let chosen = s.nearest_point_lights(Vec3::ZERO, 2);
        assert_eq!(chosen, vec![0, 1]);
        // Caméra près de la 3ᵉ → garde x=10 puis x=5.
        let chosen = s.nearest_point_lights(Vec3::new(10.0, 0.0, 0.0), 2);
        assert_eq!(chosen, vec![2, 1]);
        // Sous la limite → toutes, ordre d'origine (pas de tri).
        assert_eq!(s.nearest_point_lights(Vec3::ZERO, 8), vec![0, 1, 2]);
    }

    #[test]
    fn transform_matrix_translates_point() {
        let t = Transform::from_pos(Vec3::new(1.0, 2.0, 3.0));
        let p = t.matrix() * Vec3::ZERO.extend(1.0);
        assert!((p.truncate() - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-6);
    }

    #[test]
    fn transform_matrix_applies_scale() {
        let t = Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(2.0));
        let p = t.matrix() * Vec3::new(1.0, 0.0, 0.0).extend(1.0);
        assert!((p.truncate() - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn mobile_demo_is_playable() {
        let s = Scene::mobile_demo();
        // contrôles tactiles présents
        assert!(s.mobile.joystick);
        assert!(s.mobile.buttons.iter().any(|b| b == "Saut"));
        // un personnage scripté qui lit le joystick
        let player = s.objects.iter().find(|o| o.name == "Joueur").unwrap();
        assert!(player.script.contains("input.jx"));
        assert!(player.script.contains("input.btn.Saut"));
        // et un sol
        assert!(s.objects.iter().any(|o| matches!(o.mesh, MeshKind::Plane)));
    }

    #[test]
    fn tower_demo_is_a_distinct_no_combat_climbing_level() {
        let s = Scene::tower_demo();
        // Contrôles : joystick + saut, comme la démo contrôleur, mais pas d'attaque
        // (aucun combat dans ce style de niveau).
        assert!(s.mobile.joystick);
        assert!(s.mobile.buttons.iter().any(|b| b == "Saut"));
        assert!(!s.mobile.buttons.iter().any(|b| b == "Attaque"));
        let player = s
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable");
        assert!(
            player.controller.as_ref().unwrap().attack_button.is_empty(),
            "pas de bouton d'attaque dans ce niveau"
        );
        // Aucun ennemi, aucune lave (contrairement à la démo contrôleur) : le seul danger
        // est la chute (zone `deadly` unique).
        assert!(!s.objects.iter().any(|o| o.name.starts_with("Ennemi")));
        assert!(!s.objects.iter().any(|o| o.name == "Lave"));
        let deadly: Vec<_> = s.objects.iter().filter(|o| o.deadly).collect();
        assert_eq!(deadly.len(), 1, "un seul danger : le vide en contrebas");
        assert_eq!(deadly[0].name, "Vide");
        // Au moins une plateforme non triviale au-dessus du socle de départ, et une
        // gemme-objectif obligatoire par plateforme (collectibles => victoire en gravissant).
        let platforms = s
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Plateforme"))
            .count();
        assert!(platforms >= 10, "une vraie tour à gravir, pas un décor minimal");
        let (collected, total) = s.collectibles().expect("des gemmes-objectif");
        assert_eq!(collected, 0);
        assert_eq!(total, platforms, "une gemme obligatoire par plateforme");
    }

    #[test]
    fn tower_demo_lava_style_void_kills_a_falling_player() {
        // Même piège que pour la lave (cf. `controller_demo_lava_kills_standing_player`) :
        // le mesh Plane a une AABB locale quasi nulle en Y, donc sans épaississement de
        // l'échelle Y à la génération, le vide ne détecterait jamais un joueur en chute.
        let s = Scene::tower_demo();
        let vide = s.objects.iter().find(|o| o.name == "Vide").unwrap();
        assert!(
            vide.transform.scale.y > 1.0,
            "l'échelle Y du vide doit être épaissie pour détecter la chute"
        );
        assert!(
            s.deadly_at(vide.transform.position),
            "un joueur en chute au niveau du vide doit mourir"
        );
        // Loin au-dessus (sur une plateforme), on est en sécurité.
        assert!(!s.deadly_at(Vec3::new(0.0, 5.0, 0.0)));
    }

    #[test]
    fn temple_run_demo_is_a_distinct_endless_runner_style() {
        let s = Scene::temple_run_demo();
        // Joueur : course automatique, pas de bouton d'attaque (3ᵉ style, encore différent
        // des deux précédents : ni combat, ni pur platforming vertical).
        let player = s
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable");
        let ctrl = player.controller.as_ref().unwrap();
        assert!(ctrl.auto_run_speed > 0.0, "la course doit être automatique");
        assert!(ctrl.attack_button.is_empty(), "pas de combat dans ce style de niveau");
        assert!(!s.objects.iter().any(|o| o.name.starts_with("Ennemi")));

        // Des obstacles mortels (haies/barrages) et des pièces existent.
        assert!(s.objects.iter().any(|o| o.name == "Haie" && o.deadly));
        assert!(s.objects.iter().any(|o| o.name == "Barrage" && o.deadly));
        let coins = s.objects.iter().filter(|o| o.name == "Pièce").count();
        assert!(coins >= 10, "un vrai parcours, pas un décor minimal");

        // Un seul objectif obligatoire : la ligne d'arrivée (les pièces sont des bonus,
        // respawn_delay élevé ⇒ exclues du calcul de victoire).
        let (collected, total) = s.collectibles().expect("un objectif de victoire");
        assert_eq!(collected, 0);
        assert_eq!(total, 1, "seule l'étoile d'arrivée doit être obligatoire");
        assert!(
            s.objects
                .iter()
                .any(|o| o.name == "Étoile Arrivée" && o.respawn_delay == 0.0)
        );
    }

    #[test]
    fn scene_json_round_trip_preserves_objects() {
        let scene = Scene::demo();
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!(scene.objects.len(), back.objects.len());
        assert_eq!(back.objects[1].name, "Cube");
        assert_eq!(back.objects[1].physics, PhysicsKind::None);
        let p0 = scene.objects[0].transform.position;
        let p1 = back.objects[0].transform.position;
        assert!((p0 - p1).length() < 1e-6);
    }

    #[test]
    fn scene_round_trip_preserves_groups_color_light() {
        let mut scene = Scene::demo();
        scene.groups = vec!["Décor".into(), "Acteurs".into()];
        scene.objects[0].group = "Décor".into();
        scene.objects[1].color = [0.2, 0.4, 0.8];
        scene.light.ambient = 0.5;
        scene.light.color = [1.0, 0.5, 0.25];

        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.groups,
            vec!["Décor".to_string(), "Acteurs".to_string()]
        );
        assert_eq!(back.objects[0].group, "Décor");
        assert_eq!(back.objects[1].color, [0.2, 0.4, 0.8]);
        assert!((back.light.ambient - 0.5).abs() < 1e-6);
        assert_eq!(back.light.color, [1.0, 0.5, 0.25]);
    }

    #[test]
    fn old_scene_without_new_fields_loads_with_defaults() {
        // Scène d'une version antérieure : ni group, ni color, ni light, ni groups.
        let json = r#"{"objects":[{"name":"X","transform":{"position":[0,0,0],
            "rotation":[0,0,0,1],"scale":[1,1,1]},"mesh":"Cube"}]}"#;
        let s: Scene = serde_json::from_str(json).unwrap();
        assert_eq!(s.objects.len(), 1);
        assert_eq!(s.objects[0].color, [1.0, 1.0, 1.0]);
        assert_eq!(s.objects[0].group, "");
        assert!(s.groups.is_empty());
        assert!((s.light.ambient - 0.25).abs() < 1e-6);
        // Composants récents : valeurs par défaut sûres sur une vieille scène.
        assert!(s.objects[0].controller.is_none(), "pas pilotable par défaut");
        assert!(
            s.objects[0].visible,
            "visible doit défauter à true (sinon invisible)"
        );
        assert_eq!(s.objects[0].tap_action, TapAction::None);
    }

    #[test]
    fn deadly_zone_detects_player() {
        let mut zone = SceneObject {
            mesh: MeshKind::Cube,
            transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0)).with_scale(Vec3::splat(2.0)),
            deadly: true,
            ..Default::default()
        };
        zone.name = "Piège".into();
        let s = Scene {
            objects: vec![zone],
            ..Default::default()
        };
        assert!(s.deadly_at(Vec3::ZERO), "le centre touche la zone");
        assert!(
            !s.deadly_at(Vec3::new(10.0, 0.0, 0.0)),
            "loin = pas de contact"
        );
        // La démo contrôleur a bien une zone mortelle.
        assert!(Scene::controller_demo().objects.iter().any(|o| o.deadly));
    }

    #[test]
    fn collectible_spins_only_while_visible() {
        // Collectible visible : il tourne (rotation ≠ identité après animation).
        let angle = |o: &SceneObject| o.transform.rotation.to_axis_angle().1.abs();
        let mut o = SceneObject {
            tap_action: TapAction::Hide,
            ..Default::default()
        };
        animate_collectible(&mut o, 1.0);
        assert!(angle(&o) > 0.1, "doit tourner si visible");
        // Une fois ramassé (invisible), on ne touche plus à sa rotation.
        let mut o2 = SceneObject {
            tap_action: TapAction::Hide,
            visible: false,
            ..Default::default()
        };
        animate_collectible(&mut o2, 1.0);
        assert!(angle(&o2) < 1e-6, "figé une fois ramassé");
        // Un objet normal (pas un collectible) n'est pas animé.
        let mut n = SceneObject::default();
        animate_collectible(&mut n, 1.0);
        assert!(angle(&n) < 1e-6);
    }

    #[test]
    fn collect_at_picks_up_touched_pieces() {
        let mut s = Scene::controller_demo();
        assert_eq!(s.collectibles().unwrap().0, 0, "rien au départ");
        // On se place exactement sur une pièce (position trouvée dynamiquement).
        let piece_pos = s
            .objects
            .iter()
            .find(|o| o.tap_action == TapAction::Hide && o.visible)
            .map(|o| o.transform.position)
            .unwrap();
        let n = s.collect_at(piece_pos, 0.7).len();
        assert!(n >= 1, "doit ramasser la pièce touchée");
        // Très loin de l'arène : rien ramassé.
        assert!(s.collect_at(Vec3::new(100.0, 0.5, 100.0), 0.7).is_empty());
    }

    #[test]
    fn attack_at_defeats_only_attackable_enemies_in_range() {
        let mut s = Scene::controller_demo();
        let enemies: Vec<_> = s
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name.starts_with("Ennemi"))
            .map(|(i, o)| (i, o.transform.position))
            .collect();
        assert!(enemies.len() >= 3, "au moins 3 ennemis dans la démo");
        for (i, o) in s.objects.iter().enumerate() {
            if o.name.starts_with("Ennemi") {
                assert!(o.attackable, "un ennemi doit être une cible d'attaque valide : {i}");
            }
        }
        // Loin de tout ennemi : aucune attaque ne touche.
        assert!(s.attack_at(Vec3::new(100.0, 0.5, 100.0), 1.5).is_empty());
        // Sur le premier ennemi : il est vaincu (masqué), et une deuxième attaque au même
        // endroit ne le retouche pas (déjà invisible).
        let (idx, pos) = enemies[0];
        let hit = s.attack_at(pos, 1.5);
        assert_eq!(hit, vec![idx]);
        assert!(!s.objects[idx].visible, "l'ennemi vaincu devient invisible");
        assert!(s.attack_at(pos, 1.5).is_empty(), "un ennemi déjà vaincu n'est pas retouché");
    }

    #[test]
    fn controller_demo_lava_kills_standing_player() {
        // Le mesh Plane a une AABB locale très fine (±0.02 en Y) ; sans épaississement de
        // l'échelle Y à la génération, la lave ne recouperait jamais la hauteur réelle d'un
        // joueur debout (~0.5) et ne tuerait donc jamais personne. Verrouille la correction.
        let s = Scene::controller_demo();
        let lava_top = s
            .objects
            .iter()
            .find(|o| o.name == "Lave")
            .expect("la lave existe");
        assert!(
            lava_top.transform.scale.y > 1.0,
            "l'échelle Y de la lave doit être épaissie pour détecter un joueur debout"
        );
        // Un joueur debout au centre de la lave (hauteur de repos typique d'une capsule).
        assert!(
            s.deadly_at(Vec3::new(0.0, 0.5, 0.0)),
            "un joueur debout sur la lave doit mourir"
        );
        // Mais un joueur en plein saut au-dessus (loin dans les airs) doit pouvoir franchir.
        assert!(
            !s.deadly_at(Vec3::new(0.0, 2.5, 0.0)),
            "un joueur qui saute par-dessus la lave ne doit pas mourir"
        );
    }

    #[test]
    fn collectibles_count_and_win() {
        let mut s = Scene::controller_demo();
        let (collected, total) = s.collectibles().expect("la démo a des collectibles");
        assert!(total >= 3, "au moins 3 gemmes");
        assert_eq!(collected, 0, "rien ramassé au départ");
        // Ramasse tout : chaque collectible devient invisible.
        for o in s
            .objects
            .iter_mut()
            .filter(|o| o.tap_action == TapAction::Hide)
        {
            o.visible = false;
        }
        let (collected, total2) = s.collectibles().unwrap();
        assert_eq!(collected, total2, "tout ramassé = gagné");
        // Une scène sans collectible renvoie None.
        let empty = Scene::default();
        assert!(empty.collectibles().is_none());
    }

    #[test]
    fn tap_actions_apply_correctly() {
        let start = Vec3::new(0.0, 1.0, 0.0);
        // Hide : devient invisible.
        let mut o = SceneObject {
            tap_action: TapAction::Hide,
            ..Default::default()
        };
        apply_tap_action(&mut o, start, 0.0);
        assert!(!o.visible);
        // Grow : grossit mais reste plafonné à 4.
        let mut o = SceneObject {
            tap_action: TapAction::Grow,
            ..Default::default()
        };
        apply_tap_action(&mut o, start, 0.0);
        assert!(o.transform.scale.x > 1.0);
        for _ in 0..50 {
            apply_tap_action(&mut o, start, 0.0);
        }
        assert!(o.transform.scale.x <= 4.0 + 1e-3, "plafonné à 4");
        // Respawn : revient à la position de départ.
        let mut o = SceneObject {
            tap_action: TapAction::Respawn,
            transform: Transform::from_pos(Vec3::new(5.0, 5.0, 5.0)),
            ..Default::default()
        };
        apply_tap_action(&mut o, start, 0.0);
        assert!((o.transform.position - start).length() < 1e-6);
    }

    #[test]
    fn controller_fields_survive_round_trip() {
        let mut o = SceneObject {
            name: "Joueur".into(),
            ..Default::default()
        };
        o.controller = Some(Controller {
            input: true,
            jump_button: "Saut".into(),
            jump_height: 2.2,
            ..Default::default()
        });
        o.tap_action = TapAction::Hide;
        o.visible = false;
        let scene = Scene {
            objects: vec![o],
            ..Default::default()
        };
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        let b = &back.objects[0];
        let ctrl = b.controller.as_ref().expect("controller round-trip");
        assert!(ctrl.input);
        assert_eq!(ctrl.jump_button, "Saut");
        assert!((ctrl.jump_height - 2.2).abs() < 1e-6);
        assert_eq!(b.tap_action, TapAction::Hide);
        assert!(!b.visible);
    }
}
