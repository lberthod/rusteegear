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
    /// Input Receiver : l'objet se déplace avec le joystick virtuel en Play (X/Z).
    #[serde(default)]
    pub input_receiver: bool,
    /// Gyroscope Controller : l'objet se déplace selon l'inclinaison (tilt) en Play.
    #[serde(default)]
    pub gyro_control: bool,
    /// Vitesse appliquée par Input Receiver / Gyroscope (unités/seconde).
    #[serde(default = "default_move_speed")]
    pub move_speed: f32,
    /// Vibration Feedback : durée (ms) du retour haptique quand l'objet est tapé (0 = off).
    #[serde(default)]
    pub vibrate_on_tap: u32,
    /// Nom du bouton tactile qui fait sauter l'objet pilotable (vide = pas de saut).
    #[serde(default)]
    pub jump_button: String,
    /// Hauteur de saut (mètres) de l'objet pilotable.
    #[serde(default = "default_jump_height")]
    pub jump_height: f32,
    /// Action déclenchée sans script quand l'objet est tapé (Touch Area requise).
    #[serde(default)]
    pub tap_action: TapAction,
    /// Objet visible au rendu (mis à false par l'action « Masquer » ; rétabli à l'arrêt).
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_true() -> bool {
    true
}

fn default_jump_height() -> f32 {
    1.5
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
            input_receiver: false,
            gyro_control: false,
            move_speed: default_move_speed(),
            vibrate_on_tap: 0,
            jump_button: String::new(),
            jump_height: default_jump_height(),
            tap_action: TapAction::None,
            visible: true,
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

impl Scene {
    /// Démo « contrôleur » **sans script** : un joueur pilotable au joystick (composant
    /// Input Receiver) qui saute via un bouton tactile et entre en collision avec des
    /// obstacles statiques. Montre le contrôleur de personnage intégré.
    pub fn controller_demo() -> Self {
        // Sol statique.
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.5, 0.4];

        // Joueur pilotable : Input Receiver + saut sur le bouton « Saut ».
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.input_receiver = true;
        joueur.move_speed = 4.0;
        joueur.jump_button = "Saut".into();
        joueur.jump_height = 1.6;

        // Obstacles statiques : le joueur bute dessus.
        let mut mur = demo_obj("Mur", MeshKind::Cube, Vec3::new(3.0, 0.5, 0.0));
        mur.transform = mur.transform.with_scale(Vec3::new(1.0, 1.0, 4.0));
        mur.physics = PhysicsKind::Static;
        mur.color = [0.5, 0.55, 0.7];

        let mut caisse = demo_obj("Caisse", MeshKind::Cube, Vec3::new(-2.5, 0.5, 1.5));
        caisse.physics = PhysicsKind::Static;
        caisse.color = [0.7, 0.5, 0.3];

        // Collectibles : petites sphères jaunes à ramasser (tap → masquer).
        let mut objects = vec![sol, joueur, mur, caisse];
        for (n, pos) in [
            Vec3::new(2.0, 0.4, 2.0),
            Vec3::new(-2.0, 0.4, -2.0),
            Vec3::new(2.5, 0.4, -1.5),
        ]
        .into_iter()
        .enumerate()
        {
            let mut gem = demo_obj(&format!("Gemme {}", n + 1), MeshKind::Sphere, pos);
            gem.transform = gem.transform.with_scale(Vec3::splat(0.5));
            gem.color = [1.0, 0.85, 0.2];
            gem.emissive = 0.4;
            gem.tappable = true;
            gem.tap_action = TapAction::Hide;
            objects.push(gem);
        }

        Scene {
            objects,
            camera_follow: true,
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

    /// État des collectibles (objets à ramasser = action au tap « Masquer ») :
    /// `Some((ramassés, total))` si la scène en contient, sinon `None`. Un objet est
    /// « ramassé » quand il est devenu invisible. `ramassés == total` ⇒ niveau gagné.
    pub fn collectibles(&self) -> Option<(usize, usize)> {
        let total = self
            .objects
            .iter()
            .filter(|o| o.tap_action == TapAction::Hide)
            .count();
        if total == 0 {
            return None;
        }
        let collected = self
            .objects
            .iter()
            .filter(|o| o.tap_action == TapAction::Hide && !o.visible)
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
        assert!(!s.objects[0].input_receiver);
        assert!(
            s.objects[0].visible,
            "visible doit défauter à true (sinon invisible)"
        );
        assert_eq!(s.objects[0].tap_action, TapAction::None);
        assert!((s.objects[0].jump_height - 1.5).abs() < 1e-6);
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
        o.input_receiver = true;
        o.jump_button = "Saut".into();
        o.jump_height = 2.2;
        o.tap_action = TapAction::Hide;
        o.visible = false;
        let scene = Scene {
            objects: vec![o],
            ..Default::default()
        };
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        let b = &back.objects[0];
        assert!(b.input_receiver);
        assert_eq!(b.jump_button, "Saut");
        assert!((b.jump_height - 2.2).abs() < 1e-6);
        assert_eq!(b.tap_action, TapAction::Hide);
        assert!(!b.visible);
    }
}
