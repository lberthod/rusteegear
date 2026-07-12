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

/// Composant optionnel : son associé à un `SceneObject` (clip, autoplay, spatialisation).
/// `None` = aucun son — la grande majorité des objets d'une scène n'en ont pas ; les y
/// laisser à plat (3 champs) aurait alourdi tous les objets pour rien. Même logique de
/// migration que `Controller`.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct AudioSource {
    /// Fichier son (chemin disque ou `bundle://`).
    #[serde(default)]
    pub clip: String,
    /// Joue le son au lancement du mode Play.
    #[serde(default)]
    pub autoplay: bool,
    /// Volume au lancement décroissant avec la distance à la caméra.
    #[serde(default)]
    pub spatial: bool,
}

/// Mécanique de résolution de l'attaque du joueur (cf. `Controller::attack_mode`).
/// `Single` reste le comportement par défaut de toutes les démos existantes — un audit
/// antérieur a justement retiré l'attaque en zone du comportement par défaut (un swing
/// qui vainc tout un groupe convergent avant qu'aucun monstre n'ait pu mordre triviale
/// le combat, cf. commit « attack_at cible désormais une seule cible, pas la zone »).
/// `Zone` redevient disponible ici en **opt-in par arme** (cf. `Weapon::mode`, le
/// Marteau) : le coût (préparation/recharge longues) compense le fait de vaincre tout
/// un groupe d'un coup, ce que l'ancien comportement par défaut ne compensait pas.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum AttackMode {
    /// Missile unique verrouillé sur la cible `attackable` la plus proche à portée au
    /// moment du tir (cf. `AppState::attack_projectile`).
    #[default]
    Single,
    /// Frappe de zone résolue instantanément à la fin de la préparation (pas de missile
    /// à temps de vol) : vainc TOUTES les cibles `attackable` visibles à portée d'un
    /// coup (cf. `Scene::attack_zone_at`).
    Zone,
}

/// Composant optionnel : fait d'un `SceneObject` un objet pilotable par le joueur
/// (joystick, gyroscope, saut, attaque). Regroupe des champs auparavant plats sur
/// `SceneObject` — un seul objet par scène en porte généralement un (le joueur), donc
/// les y laisser à plat aurait alourdi *tous* les objets (décor, ennemis, pièces...)
/// pour rien. Étape de migration « composants optionnels » (pas un ECS complet : pas
/// de requêtes génériques, juste un regroupement logique qui évite le bloat plat).
#[derive(Clone, Serialize, Deserialize)]
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
    /// Accélération/décélération (m/s²) appliquée à la vitesse horizontale de `input` :
    /// 0 = vitesse imposée instantanément (comportement historique, robotique). Une
    /// valeur positive lisse les départs/arrêts façon jeu d'action moderne (cf.
    /// `Physics::control`) — demandé le 2026-07-12 pour un déplacement moins abrupt.
    #[serde(default = "default_acceleration")]
    pub acceleration: f32,
    /// Vitesse de rotation (rad/s) à laquelle le personnage tourne progressivement
    /// pour faire face à sa direction de déplacement (`input` uniquement — pas les
    /// chasseurs IA ni le recul). 0 = pas de rotation automatique (comportement
    /// historique). Cf. `Physics::face_direction`.
    #[serde(default = "default_turn_speed")]
    pub turn_speed: f32,
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
    /// Temps de recharge (s) entre deux attaques (0 = pas de limite — à éviter : sans
    /// recharge, maintenir le bouton défait instantanément tout ce qui entre en portée,
    /// sans risque). Cf. `AppState::attack_cooldown_remaining`.
    #[serde(default = "default_attack_cooldown")]
    pub attack_cooldown: f32,
    /// Temps de préparation (s) entre l'appui et le départ du missile (0 = tir immédiat).
    /// Audit gameplay : le temps de vol du missile ne suffit pas à garantir un risque
    /// en 1 contre 1 (un missile homing tiré dès l'entrée en portée arrive presque
    /// toujours avant qu'un monstre en approche directe n'atteigne sa propre portée de
    /// morsure) — un temps de préparation, lui, laisse la cible continuer d'approcher
    /// *avant même que le missile ne parte*, créant une vraie fenêtre de vulnérabilité.
    /// Cf. `AppState::attack_charge`.
    #[serde(default = "default_attack_windup")]
    pub attack_windup: f32,
    /// Mécanique de résolution de l'attaque (cf. `AttackMode`) : `Single` par défaut
    /// (comportement historique de toutes les démos), `Zone` pour les armes qui
    /// l'assument explicitement (cf. `Weapon::mode`, le Marteau du donjon roguelike).
    #[serde(default)]
    pub attack_mode: AttackMode,
}

// Implémentation manuelle (pas `#[derive(Default)]`) : `derive` donnerait 0.0/vide à
// chaque champ, alors que plusieurs ont un défaut serde non trivial (`move_speed` = 3.0,
// `attack_cooldown` = 0.5, etc.) — un piège classique où `..Default::default()` en Rust
// diverge silencieusement des défauts appliqués à la désérialisation JSON.
impl Default for Controller {
    fn default() -> Self {
        Self {
            input: false,
            gyro: false,
            move_speed: default_move_speed(),
            auto_run_speed: 0.0,
            acceleration: default_acceleration(),
            turn_speed: default_turn_speed(),
            jump_button: String::new(),
            jump_height: default_jump_height(),
            attack_button: String::new(),
            attack_range: default_attack_range(),
            attack_cooldown: default_attack_cooldown(),
            attack_windup: default_attack_windup(),
            attack_mode: AttackMode::Single,
        }
    }
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

/// Composant optionnel : rôle d'un `SceneObject` dans le combat — cible d'attaque
/// (`attackable`) et/ou ancre visuelle de l'effet d'impact (`is_attack_fx`). `None`
/// pour la grande majorité des objets d'une scène (décor, collectibles, joueur...).
/// Les deux champs se cumulent rarement sur le même objet (l'ancre FX n'est
/// généralement pas elle-même attaquable) mais partagent le même domaine « combat ».
#[derive(Clone, Serialize, Deserialize)]
pub struct Combat {
    /// Cible valide pour l'attaque du joueur (cf. `Scene::attack_at`) : un ennemi vaincu
    /// devient invisible, puis réapparaît après `respawn_delay` (0 = ne réapparaît pas).
    #[serde(default)]
    pub attackable: bool,
    /// Ancre visuelle de l'effet d'attaque (au plus un objet par scène) : téléportée sur
    /// la cible touchée et affichée brièvement par `App` quand une attaque porte (cf.
    /// `AppState::attack_flash`). N'a aucun effet tant qu'aucune attaque ne porte.
    #[serde(default)]
    pub is_attack_fx: bool,
    /// Numéro de manche (1-based) auquel appartient cet ennemi ; 0 = pas de système de
    /// manches (visible/actif dès le départ, comme les autres démos). Les ennemis d'une
    /// manche > 1 sont masqués jusqu'à ce que `App` révèle leur manche (cf.
    /// `AppState::wave` : toutes les cibles de la manche courante vaincues ⇒ manche
    /// suivante révélée, jusqu'à la dernière ⇒ victoire).
    #[serde(default)]
    pub wave: u32,
    /// Points de vie : nombre de coups nécessaires pour vaincre cette cible. 1 par défaut
    /// (mise à mort en un coup, comportement historique de toutes les démos existantes).
    /// Une valeur plus grande décrit un adversaire qui encaisse plusieurs coups avant de
    /// tomber (cf. `Scene::brawl_demo`, un duel façon Tekken/Smash) — décompté par
    /// `Scene::damage_attackable`, qui ne masque la cible que si ce coup l'achève.
    /// Limite connue : un ennemi qui réapparaît (`respawn_delay` positif) revient avec
    /// les PV où il les a laissés (0), pas remis à son maximum — sans effet sur les
    /// démos actuelles (aucune ne combine plusieurs PV et réapparition).
    #[serde(default = "default_combat_hp")]
    pub hp: u32,
}

// Manuel comme `Controller`/`AiChaser` : `derive(Default)` donnerait hp=0 (cible déjà
// vaincue avant le moindre coup), pas 1 (une cible naît vivante par défaut) — même piège
// que documenté sur `impl Default for Controller`.
impl Default for Combat {
    fn default() -> Self {
        Self {
            attackable: false,
            is_attack_fx: false,
            wave: 0,
            hp: default_combat_hp(),
        }
    }
}

fn default_combat_hp() -> u32 {
    1
}

/// Composant optionnel : IA qui **poursuit activement le joueur** (contrairement aux
/// patrouilles scriptées à trajectoire fixe/sinusoïdale, prévisibles) — se déplace en
/// ligne droite vers la position courante du joueur chaque frame, via le moteur physique
/// (collisions avec le décor respectées, comme le joueur). L'attaque au contact reste
/// gérée séparément par `trigger` + un script `damage()` (cf. `controller_level`) :
/// `AiChaser` ne fait que le déplacement, pas les dégâts.
#[derive(Clone, Serialize, Deserialize)]
pub struct AiChaser {
    /// Vitesse de poursuite (unités/seconde).
    #[serde(default = "default_move_speed")]
    pub speed: f32,
}

// Manuel comme `Controller` : `derive(Default)` donnerait speed=0.0 (immobile), pas la
// vitesse par défaut serde — mêmes raisons, cf. le commentaire sur `impl Default for Controller`.
impl Default for AiChaser {
    fn default() -> Self {
        Self {
            speed: default_move_speed(),
        }
    }
}

/// Profil d'arme (portée/recharge/préparation) appliqué au `Controller` du joueur — pas
/// de dégât différent d'un profil à l'autre : une attaque vainc toujours sa cible en un
/// coup (cf. `Scene::attack_at`), le choix change le *style* de jeu (risque/portée), pas
/// la puissance brute. Utilisé par `Scene::roguelike_demo` : une arme de départ tirée au
/// sort, et d'autres à trouver en jeu (cf. `WeaponPickup`).
#[derive(Clone, Copy, Debug)]
pub struct Weapon {
    pub label: &'static str,
    pub range: f32,
    pub cooldown: f32,
    pub windup: f32,
    /// Mécanique de résolution (cf. `AttackMode`) : `Zone` pour le Marteau seulement —
    /// la contrepartie (préparation et recharge les plus longues de la table) doit
    /// compenser le fait de vaincre tout un groupe à portée d'un coup.
    pub mode: AttackMode,
}

/// Les 5 profils d'arme connus, du plus risqué (corps-à-corps rapide) au plus prudent
/// (portée longue, lent à préparer). Table publique : partagée entre la génération de
/// la démo (tirage de l'arme de départ + placement des butins) et la résolution du
/// ramassage en jeu (cf. `Scene::weapon_pickup_at`), pour n'avoir qu'une seule source
/// de vérité sur les profils.
pub const WEAPONS: [Weapon; 5] = [
    Weapon {
        label: "Dague",
        range: 0.9,
        cooldown: 0.3,
        windup: 0.12,
        mode: AttackMode::Single,
    },
    Weapon {
        label: "Épée",
        range: 1.6,
        cooldown: 0.5,
        windup: 0.25,
        mode: AttackMode::Single,
    },
    Weapon {
        label: "Lance",
        range: 2.4,
        cooldown: 0.65,
        windup: 0.3,
        mode: AttackMode::Single,
    },
    Weapon {
        label: "Marteau",
        range: 1.2,
        cooldown: 0.9,
        windup: 0.5,
        mode: AttackMode::Zone,
    },
    Weapon {
        label: "Arc",
        range: 4.0,
        cooldown: 1.0,
        windup: 0.45,
        mode: AttackMode::Single,
    },
];

/// Composant optionnel : butin à ramasser au contact (cf. `Scene::weapon_pickup_at`) qui
/// équipe l'arme `WEAPONS[weapon]` sur le joueur. Distinct des pièces (`tap_action ==
/// Hide`, cf. `collect_at`) : une pièce compte comme pièce-objectif (`collectibles()`),
/// ce qu'un butin d'arme ne doit **pas** faire — sinon ramasser du matériel déclencherait
/// une victoire prématurée, à la place de vider les salles (cf. `Combat::wave`).
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct WeaponPickup {
    /// Indice dans `WEAPONS`.
    pub weapon: usize,
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
    /// Son associé à l'objet (clip, autoplay, spatialisation) : `None` = aucun son —
    /// la grande majorité des objets d'une scène n'en ont pas. Composant optionnel
    /// (regroupe 3 champs auparavant plats), même logique que `controller`.
    #[serde(default)]
    pub audio: Option<AudioSource>,
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
    /// Combat : cible d'attaque et/ou ancre visuelle d'effet (cf. `Combat`). `None` pour
    /// la grande majorité des objets — décor, collectibles, etc. n'ont rien à voir avec
    /// le combat. `respawn_delay` (plus bas) reste hors de ce composant : partagé avec les
    /// collectibles bonus, il n'est pas propre au combat.
    #[serde(default)]
    pub combat: Option<Combat>,
    /// IA de poursuite active du joueur (cf. `AiChaser`) : `None` pour la grande
    /// majorité des objets — seuls les ennemis « chasseurs » (jeu local vs IA) en ont.
    #[serde(default)]
    pub ai_chaser: Option<AiChaser>,
    /// Butin d'arme à ramasser au contact (cf. `WeaponPickup`) : `None` pour la grande
    /// majorité des objets — seuls les butins du donjon roguelike en ont.
    #[serde(default)]
    pub weapon_pickup: Option<WeaponPickup>,
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

fn default_attack_cooldown() -> f32 {
    0.5
}

fn default_attack_windup() -> f32 {
    0.25
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

fn default_acceleration() -> f32 {
    20.0
}

fn default_turn_speed() -> f32 {
    10.0
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
            audio: None,
            group: String::new(),
            color: white(),
            texture: String::new(),
            tappable: false,
            metallic: 0.0,
            roughness: default_roughness(),
            emissive: 0.0,
            trigger: false,
            controller: None,
            vibrate_on_tap: 0,
            combat: None,
            ai_chaser: None,
            weapon_pickup: None,
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
    /// Affiche une croix directionnelle (4 boutons haut/bas/gauche/droite,
    /// coin bas-gauche) à la place du joystick — plus précis pour un
    /// déplacement à 4 (ou 8) directions qu'un joystick analogique, au prix
    /// de la finesse d'un angle libre. Prioritaire sur `joystick` si les deux
    /// sont actifs (cf. `mobile_overlay`), pour ne jamais superposer les deux
    /// dans le même coin de l'écran.
    #[serde(default)]
    pub dpad: bool,
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
        self.joystick || self.dpad || !self.buttons.is_empty() || self.touch_zone || self.health_bar
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
        group: String::new(),
        color: white(),
        texture: String::new(),
        tappable: false,
        metallic: 0.0,
        roughness: 0.6,
        emissive: 0.0,
        trigger: false,
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
        // touche J (desktop, cf. `PlayerInput::attack`). Portée courte (0,7 m) : au-delà
        // de `attack_range`, ce qui compte c'est l'écart avec la portée de morsure de la
        // cible (son propre rayon) — un écart de 1,5 m rendait le combat sans risque
        // (audit gameplay : un bot qui approche puis attaque ne prenait jamais de dégâts).
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.6,
            attack_button: "Attaque".into(),
            attack_range: 0.7,
            ..Default::default()
        });

        // Effet visuel du coup : sphère blanche invisible par défaut, téléportée sur la
        // cible et affichée brièvement par `App` quand une attaque porte (cf.
        // `AppState::attack_flash`) — rend le coup lisible, pas juste sonore.
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
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
        lave.transform = lave.transform.with_scale(Vec3::new(lave_s, 30.0, lave_s));
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
        let mut trophy = demo_obj(
            "Étoile Trophée",
            MeshKind::Sphere,
            Vec3::new(-5.0, 2.1, 0.0),
        );
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
        let mut lintel = demo_obj(
            "Linteau Portique",
            MeshKind::Cube,
            Vec3::new(0.0, 2.35, -5.6),
        );
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
            e.combat = Some(Combat {
                attackable: true,
                ..Default::default()
            });
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
            let mut torch = demo_obj(
                &format!("Torche {}", n + 1),
                MeshKind::Cube,
                base + Vec3::Y * 0.8,
            );
            torch.transform = torch.transform.with_scale(Vec3::new(0.3, 1.6, 0.3));
            torch.physics = PhysicsKind::Static;
            torch.color = [0.3, 0.28, 0.3];
            objects.push(torch);

            let mut flame = demo_obj(
                &format!("Flamme {}", n + 1),
                MeshKind::Sphere,
                base + Vec3::Y * 1.7,
            );
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

            let mut gem = demo_obj(
                &format!("Gemme {}", i + 1),
                MeshKind::Sphere,
                pos + Vec3::Y * 0.85,
            );
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
                        let mut coin = demo_obj("Pièce", MeshKind::Sphere, Vec3::new(lx, 1.0, z));
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
            let mut post = demo_obj(
                "Pilier Arrivée",
                MeshKind::Cube,
                Vec3::new(sx, 1.4, finish_z),
            );
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

        let mut finish = demo_obj(
            "Étoile Arrivée",
            MeshKind::Sphere,
            Vec3::new(0.0, 1.5, finish_z),
        );
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

    /// Scène **exemple**, minimale et commentée : montre les 3 composants optionnels
    /// (`Controller`, `AudioSource`, `Combat`) chacun sur un seul objet, sans le décor
    /// dense d'un vrai niveau. Sert de référence rapide pour qui étend le moteur — pas
    /// une démo de gameplay comme les autres (arène/tour/course).
    pub fn components_demo() -> Self {
        // Sol minimal (juste assez pour marcher/sauter).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(10.0, 1.0, 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.4, 0.45, 0.5];

        // --- Controller : rend un objet pilotable (joystick + saut + attaque). `None`
        // pour tous les autres objets de cette scène — un seul joueur en a besoin.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-2.5, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 1.5,
            ..Default::default()
        });

        // --- AudioSource : son associé à un objet (clip/autoplay/spatialisation). Le
        // clip est vide ici (aucun fichier son fourni avec l'exemple) — assigne-en un
        // via l'inspecteur (panneau Audio › Choisir un son…) pour l'entendre en Play.
        let mut boite = demo_obj("Boîte à musique", MeshKind::Cube, Vec3::new(0.0, 0.5, 2.0));
        boite.color = [0.6, 0.4, 0.8];
        boite.audio = Some(AudioSource {
            clip: String::new(),
            autoplay: true,
            spatial: true,
        });

        // --- Combat : cible d'attaque (`attackable`) et ancre visuelle de l'effet
        // d'impact (`is_attack_fx`), rarement sur le même objet (ici, deux objets
        // séparés). Approche le joueur et appuie sur Attaque (ou touche J) pour tester.
        let mut cible = demo_obj(
            "Cible d'entraînement",
            MeshKind::Sphere,
            Vec3::new(2.5, 1.0, 0.0),
        );
        cible.color = [0.85, 0.15, 0.15];
        cible.emissive = 0.4;
        cible.combat = Some(Combat {
            attackable: true,
            ..Default::default()
        });
        cible.respawn_delay = 3.0;

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        Scene {
            objects: vec![sol, joueur, boite, cible, fx],
            camera_follow: true,
            point_lights: vec![PointLight {
                position: [0.0, 5.0, 0.0],
                color: [1.0, 0.95, 0.85],
                intensity: 1.2,
                range: 14.0,
                ..PointLight::default()
            }],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Vagues de zombies » : jeu **local contre l'ordinateur**, sans réseau, en
    /// **manches** (style Call of Duty Zombies) — 3 archétypes de monstres (`AiChaser`,
    /// poursuite active, pas de patrouille scriptée), de plus en plus nombreux et variés
    /// à chaque manche. Vaincre tous les monstres d'une manche révèle la suivante ; la
    /// dernière vaincue ⇒ victoire (`App` pilote la progression, cf. `AppState::wave`).
    pub fn zombies_demo() -> Self {
        let half = 10.0_f32;
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.22, 0.24, 0.28];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Portée courte (0,7 m, pas 1,5) : audit gameplay — un bot qui approche puis
        // attaque au cooldown ne prenait jamais un seul point de dégâts sur les 4 manches,
        // la portée dépassant bien trop largement le rayon de morsure des monstres.
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 0.7,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour.
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.3, 0.32, 0.38];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );

        // Piliers de couverture : obstacles pour casser une poursuite (les monstres ne
        // les contournent pas intelligemment, ils foncent tout droit vers le joueur).
        for (sx, sz) in [
            (-3.0_f32, 2.0),
            (3.0, -2.0),
            (0.0, 5.5),
            (-4.0, -5.0),
            (4.5, 4.5),
        ] {
            let mut pilier = demo_obj("Pilier", MeshKind::Cylinder, Vec3::new(sx, 0.9, sz));
            pilier.transform = pilier.transform.with_scale(Vec3::new(1.4, 1.8, 1.4));
            pilier.physics = PhysicsKind::Static;
            pilier.color = [0.4, 0.4, 0.45];
            objects.push(pilier);
        }

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        objects.push(fx);

        // --- 3 archétypes de monstres, de plus en plus présents/variés à chaque manche
        // (comme les vagues d'un mode zombies) : Rôdeur (basique), Coureur (rapide et
        // fragile), Brute (lente mais très punitive et plus difficile à esquiver).
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
        }
        const RODEUR: Kind = Kind {
            label: "Rôdeur",
            speed: 2.6,
            dmg: 0.8,
            scale: 0.7,
            color: [0.35, 0.55, 0.25],
        };
        const COUREUR: Kind = Kind {
            label: "Coureur",
            speed: 4.6,
            dmg: 0.5,
            scale: 0.55,
            color: [0.75, 0.8, 0.2],
        };
        const BRUTE: Kind = Kind {
            label: "Brute",
            speed: 1.8,
            dmg: 2.2,
            scale: 1.3,
            color: [0.45, 0.08, 0.25],
        };
        // (manche, archétypes de cette manche) — la difficulté monte : plus de monstres,
        // puis des archétypes plus dangereux introduits progressivement.
        let waves: &[(u32, &[&Kind])] = &[
            (1, &[&RODEUR, &RODEUR, &RODEUR]),
            (2, &[&RODEUR, &RODEUR, &RODEUR, &COUREUR, &COUREUR]),
            (
                3,
                &[
                    &RODEUR, &RODEUR, &COUREUR, &COUREUR, &COUREUR, &BRUTE, &BRUTE,
                ],
            ),
            (
                4,
                &[&RODEUR, &RODEUR, &COUREUR, &COUREUR, &BRUTE, &BRUTE, &BRUTE],
            ),
        ];
        let total: usize = waves.iter().map(|(_, ks)| ks.len()).sum();
        let mut spawned = 0usize;
        for &(wave, kinds) in waves {
            for (n, k) in kinds.iter().enumerate() {
                // Répartis en cercle sur tout le pourtour (indice global, pas par manche) :
                // les manches suivantes n'occupent pas les mêmes points que la précédente.
                let angle = spawned as f32 / total as f32 * std::f32::consts::TAU;
                let radius = half - 1.4;
                let pos = Vec3::new(
                    angle.cos() * radius,
                    k.scale.max(0.5) * 0.5,
                    angle.sin() * radius,
                );
                spawned += 1;

                let mut m = demo_obj(&format!("{} {}", k.label, n + 1), MeshKind::Sphere, pos);
                m.transform = m.transform.with_scale(Vec3::splat(k.scale));
                m.color = k.color;
                m.emissive = 0.5;
                m.trigger = true;
                m.ai_chaser = Some(AiChaser { speed: k.speed });
                m.combat = Some(Combat {
                    attackable: true,
                    wave,
                    ..Default::default()
                });
                // Pas de réapparition : un monstre vaincu reste mort pour la manche
                // (contrairement aux ennemis de l'arène de combat, qui reviennent).
                m.respawn_delay = 0.0;
                m.script = format!(
                    "if obj.triggered then damage({} * dt) end\n\
                     local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                     obj.r = {} + {} * p; obj.g = {}; obj.b = {}",
                    k.dmg,
                    k.color[0] * 0.7,
                    k.color[0] * 0.3,
                    k.color[1] * 0.6,
                    k.color[2] * 0.6,
                );
                objects.push(m);
            }
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 9.0, 0.0],
                    color: [0.75, 0.85, 1.0],
                    intensity: 1.3,
                    range: 24.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 3.0, 0.0],
                    color: [1.0, 0.5, 0.3],
                    intensity: 0.7,
                    range: 10.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « MMORPG » (Sprint 65) : arène minimale dédiée au test multijoueur
    /// PC ↔ mobile — pas de monstres ni de manches (contrairement à
    /// `zombies_demo`), juste un joueur pilotable (joystick + saut) sur une
    /// carte simple avec quelques repères visuels statiques, pour voir
    /// clairement un joueur desktop et un joueur APK se déplacer l'un par
    /// rapport à l'autre (fantômes réseau, cf. `app::network_client`).
    pub fn mmorpg_demo() -> Self {
        let half = 12.0_f32;
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.28, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour (enferment l'aire de jeu, ne servent qu'à ne pas tomber).
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.3, 0.32, 0.38];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );

        // Repères visuels statiques (juste pour situer les déplacements, sans danger).
        for (n, (x, z)) in [(-6.0_f32, -6.0), (6.0, -6.0), (-6.0, 6.0), (6.0, 6.0)]
            .into_iter()
            .enumerate()
        {
            let mut repere = demo_obj(
                &format!("Repère {}", n + 1),
                MeshKind::Cylinder,
                Vec3::new(x, 0.9, z),
            );
            repere.transform = repere.transform.with_scale(Vec3::new(1.0, 1.8, 1.0));
            repere.physics = PhysicsKind::Static;
            repere.color = [0.5, 0.45, 0.62];
            objects.push(repere);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![PointLight {
                position: [0.0, 10.0, 0.0],
                color: [0.9, 0.95, 1.0],
                intensity: 1.2,
                range: 30.0,
                ..PointLight::default()
            }],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Donjon » façon roguelike : 3 salles reliées par des portes (une salle à la
    /// fois, comme un couloir de progression), chacune gardée par un monstre — réutilise
    /// le système de manches (`Combat::wave`) de `zombies_demo` : un monstre par manche,
    /// la salle suivante ne se révèle (et n'obtient de corps physique, cf.
    /// `Physics::build`) qu'une fois la précédente vidée. Particularité roguelike : à
    /// chaque chargement, 3 armes **distinctes** sont tirées au sort parmi les 5 profils
    /// connus (cf. `WEAPONS`) — une équipée au départ, les 2 autres cachées en butin
    /// dans les salles 1 et 2 (cf. `WeaponPickup`) à trouver en explorant avant d'arriver
    /// à la salle 3 (l'Ogre). Score +1 par monstre vaincu *et* par arme trouvée (cf.
    /// `AppState::advance_play`) : un vrai objectif d'exploration, pas juste un combat.
    pub fn roguelike_demo() -> Self {
        // Salles carrées de 9 m de côté, alignées le long de +Z, séparées par une porte
        // (mur avec une ouverture centrale de 3 m) plutôt que par un couloir séparé —
        // plus compact qu'un vrai couloir, mais tout aussi lisible comme 3 pièces
        // distinctes (ligne de vue coupée hors de l'ouverture).
        let half_x = 4.5_f32;
        let room_depth = 9.0_f32;
        let room_z = [-room_depth, 0.0, room_depth]; // centres des 3 salles
        let total_half_z = 1.5 * room_depth;

        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half_x, 1.0, 2.0 * total_half_z));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.18, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, room_z[0]));
        joueur.color = [0.95, 0.6, 0.25];

        // --- Tirage de 3 armes DISTINCTES parmi les 5 profils connus (`WEAPONS`) : une
        // pour l'équipement de départ, les 2 autres cachées en butin plus bas (cf.
        // `WeaponPickup`). Mélange de Fisher-Yates sur un petit xorshift maison (pas de
        // dépendance `rand` pour un tirage aussi simple, cf. philosophie du projet —
        // dépendances choisies pour des besoins délimités, jamais pour la structure du
        // moteur) : l'horloge système sert de graine.
        let mut rng_state = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1; // xorshift dégénère à 0 si la graine est 0 : jamais nulle.
        let mut next_rand = move || {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            rng_state
        };
        let mut order: [usize; WEAPONS.len()] = std::array::from_fn(|i| i);
        for i in (1..order.len()).rev() {
            let j = (next_rand() as usize) % (i + 1);
            order.swap(i, j);
        }
        let (starting_idx, found_idx) = (order[0], [order[1], order[2]]);
        let weapon = WEAPONS[starting_idx];
        log::info!(
            "Donjon : arme de départ « {} » (portée {:.1} m, recharge {:.2} s, préparation {:.2} s) — à trouver : {}, {}",
            weapon.label,
            weapon.range,
            weapon.cooldown,
            weapon.windup,
            WEAPONS[found_idx[0]].label,
            WEAPONS[found_idx[1]].label,
        );
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            attack_button: "Attaque".into(),
            attack_range: weapon.range,
            attack_cooldown: weapon.cooldown,
            attack_windup: weapon.windup,
            attack_mode: weapon.mode,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour de tout le donjon (une seule enveloppe extérieure).
        let wall = |name: &str, pos: Vec3, scale: Vec3, objects: &mut Vec<SceneObject>| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.32, 0.28, 0.35];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -total_half_z - 0.25),
            Vec3::new(2.0 * half_x + 0.5, 1.8, 0.5),
            &mut objects,
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, total_half_z + 0.25),
            Vec3::new(2.0 * half_x + 0.5, 1.8, 0.5),
            &mut objects,
        );
        wall(
            "Mur Est",
            Vec3::new(half_x + 0.25, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * total_half_z + 0.5),
            &mut objects,
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half_x - 0.25, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * total_half_z + 0.5),
            &mut objects,
        );

        // Portes entre les salles : mur transversal avec une ouverture centrale de 3 m
        // (deux segments latéraux), à mi-chemin entre chaque paire de salles.
        let door_gap_half = 1.5_f32;
        for (n, z) in [room_z[0], room_z[1]]
            .iter()
            .map(|z| z + room_depth * 0.5)
            .enumerate()
        {
            wall(
                &format!("Porte {} (gauche)", n + 1),
                Vec3::new(-(half_x + door_gap_half) * 0.5, 0.9, z),
                Vec3::new(half_x - door_gap_half, 1.8, 0.4),
                &mut objects,
            );
            wall(
                &format!("Porte {} (droite)", n + 1),
                Vec3::new((half_x + door_gap_half) * 0.5, 0.9, z),
                Vec3::new(half_x - door_gap_half, 1.8, 0.4),
                &mut objects,
            );
        }

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        objects.push(fx);

        // --- Un monstre par salle (une manche chacun, cf. `Combat::wave`) : la salle 2
        // ne se révèle qu'une fois le monstre de la salle 1 vaincu, etc. — progression
        // « une salle à la fois » typique d'un roguelike, sans script de porte à part.
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
        }
        const GOBELIN: Kind = Kind {
            label: "Gobelin",
            speed: 3.2,
            dmg: 0.6,
            scale: 0.6,
            color: [0.35, 0.6, 0.3],
        };
        const SQUELETTE: Kind = Kind {
            label: "Squelette",
            speed: 2.4,
            dmg: 1.0,
            scale: 0.85,
            color: [0.75, 0.72, 0.65],
        };
        const OGRE: Kind = Kind {
            label: "Ogre",
            speed: 1.6,
            dmg: 2.4,
            scale: 1.4,
            color: [0.4, 0.15, 0.15],
        };
        // Décalage du monstre par rapport au centre de sa salle : loin du point d'entrée
        // du joueur — son spawn pour la salle 1 (sinon le Gobelin apparaissait pile sur
        // le joueur et mordait avant même qu'il ait pu bouger), la porte d'entrée pour
        // les salles 2 et 3 (sinon le monstre suivant mord dès le franchissement de la
        // porte, sans le moindre temps de réaction).
        const MONSTER_Z_OFFSET: [f32; 3] = [-3.0, 3.0, 3.0];
        for (wave, ((k, z), z_offset)) in [GOBELIN, SQUELETTE, OGRE]
            .into_iter()
            .zip(room_z)
            .zip(MONSTER_Z_OFFSET)
            .enumerate()
        {
            let wave = wave as u32 + 1;
            let mut m = demo_obj(
                k.label,
                MeshKind::Sphere,
                Vec3::new(0.0, k.scale.max(0.5) * 0.5, z + z_offset),
            );
            m.transform = m.transform.with_scale(Vec3::splat(k.scale));
            m.color = k.color;
            m.emissive = 0.5;
            m.trigger = true;
            m.ai_chaser = Some(AiChaser { speed: k.speed });
            m.combat = Some(Combat {
                attackable: true,
                wave,
                ..Default::default()
            });
            m.respawn_delay = 0.0;
            m.script = format!(
                "if obj.triggered then damage({} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = {} + {} * p; obj.g = {}; obj.b = {}",
                k.dmg,
                k.color[0] * 0.7,
                k.color[0] * 0.3,
                k.color[1] * 0.6,
                k.color[2] * 0.6,
            );
            objects.push(m);
        }

        // Butins d'arme (cf. `WeaponPickup`) : un dans la salle 1, un dans la salle 2 —
        // la salle 3 (l'Ogre, le combat le plus dur) doit pouvoir être abordée avec la
        // meilleure arme déjà trouvée en explorant, pas en découvrir une nouvelle en
        // pleine bagarre. Coin de salle opposé au monstre (au centre), pour ne pas
        // forcer le joueur à passer devant le monstre juste pour le voir.
        for (n, &weapon_idx) in found_idx.iter().enumerate() {
            let w = WEAPONS[weapon_idx];
            let side = if n == 0 { 1.0 } else { -1.0 };
            let mut loot = demo_obj(
                &format!("Butin: {}", w.label),
                MeshKind::Cube,
                Vec3::new(side * 3.0, 0.4, room_z[n] + side * 3.0),
            );
            loot.transform = loot.transform.with_scale(Vec3::splat(0.4));
            loot.color = [1.0, 0.85, 0.2];
            loot.emissive = 1.2;
            loot.weapon_pickup = Some(WeaponPickup { weapon: weapon_idx });
            objects.push(loot);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 6.0, room_z[0]],
                    color: [0.4, 0.8, 0.5],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 6.0, room_z[1]],
                    color: [0.9, 0.85, 0.7],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 6.0, room_z[2]],
                    color: [0.9, 0.3, 0.3],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Duel » façon Tekken/Smash Bros : arène compacte flottant au-dessus du
    /// vide, joueur contre un unique rival qui encaisse plusieurs coups (cf.
    /// `Combat::hp`) avant de tomber — un vrai combat, pas une mise à mort au premier
    /// coup. Deux façons de gagner, comme dans un vrai jeu de combat : l'achever à coups
    /// de poing (hp à 0, cf. `Scene::damage_attackable`), ou le faire sortir de l'arène
    /// d'un coup de recul (« ring out », cf. `AppState::stagger` — le vide sous la scène
    /// est une zone mortelle, cf. `deadly`, réutilisée pour l'IA comme pour le joueur).
    /// Réutilise le système de manches (`Combat::wave = 1`, un seul adversaire) plutôt
    /// qu'un mécanisme de victoire dédié : dès que le rival est invisible (achevé ou
    /// sorti de l'arène), `AppState::update_waves` déclenche la victoire tout seul.
    pub fn brawl_demo() -> Self {
        let half = 7.0_f32;

        let mut sol = demo_obj("Arène", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.18, 0.16, 0.22];
        sol.metallic = 0.5;
        sol.roughness = 0.3;

        // Le vide sous l'arène : aucun mur, aucun sol au-delà du bord — tomber suffit à
        // perdre (joueur) ou à être vaincu (rival, cf. la vérification de ring out dans
        // `AppState::advance_play`). Invisible : la chute elle-même (rien sous les
        // pieds) suffit à faire comprendre le danger, pas besoin d'un aplat coloré.
        let mut vide = demo_obj("Vide", MeshKind::Cube, Vec3::new(0.0, -8.0, 0.0));
        vide.transform = vide.transform.with_scale(Vec3::new(60.0, 10.0, 60.0));
        vide.deadly = true;
        vide.visible = false;

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-4.0, 1.0, 0.0));
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            // Portée courte et préparation vive : des coups qui se rapprochent d'un jab
            // de jeu de combat, pas d'un missile à distance.
            attack_range: 1.3,
            attack_cooldown: 0.45,
            attack_windup: 0.15,
            ..Default::default()
        });

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.9, 0.6];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        let mut rival = demo_obj("Rival", MeshKind::Capsule, Vec3::new(4.0, 1.0, 0.0));
        rival.transform = rival.transform.with_scale(Vec3::splat(1.05));
        rival.color = [0.55, 0.08, 0.12];
        rival.emissive = 0.35;
        rival.trigger = true;
        rival.ai_chaser = Some(AiChaser { speed: 2.8 });
        rival.combat = Some(Combat {
            attackable: true,
            // Une seule « manche » (cf. `Combat::wave`) : un adversaire unique, pas des
            // vagues — juste pour déclencher la victoire via `AppState::update_waves`
            // une fois qu'il est invisible (achevé ou sorti de l'arène), sans avoir à
            // écrire une condition de victoire dédiée à cette démo.
            wave: 1,
            // 3 coups pour l'achever : un vrai duel, pas une mise à mort au premier
            // coup (`Combat::hp` par défaut ailleurs). Reste vainquable par ring out
            // avant d'y arriver (cf. la vérification dans `AppState::advance_play`).
            hp: 3,
            ..Default::default()
        });
        rival.respawn_delay = 0.0;
        rival.script = "if obj.triggered then damage(0.9 * dt) end\n\
             local p = 0.5 + 0.5 * math.sin(time * 6.0)\n\
             obj.r = 0.55 + 0.35 * p; obj.g = 0.08; obj.b = 0.12"
            .into();

        Scene {
            objects: vec![sol, vide, joueur, fx, rival],
            camera_follow: true,
            // Angle plus bas et plus horizontal que les autres démos (pitch ~0,35 contre
            // ~0,62) : cadrage de profil façon jeu de combat plutôt qu'une vue plongeante
            // de action-aventure — le point de vue précis se règle facilement dans
            // l'éditeur (`Vue → Définir la caméra de jeu`) si besoin d'un angle différent.
            game_camera: Some(GameCamera {
                target: [0.0, 1.0, 0.0],
                yaw: 0.0,
                pitch: 0.35,
                distance: 9.0,
            }),
            point_lights: vec![
                // Lumière chaude du côté du joueur, froide du côté du rival — cadrage
                // « vs » à deux couleurs typique des jeux de combat.
                PointLight {
                    position: [-4.0, 4.0, 2.0],
                    color: [1.0, 0.65, 0.3],
                    intensity: 1.1,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [4.0, 4.0, -2.0],
                    color: [0.3, 0.55, 1.0],
                    intensity: 1.1,
                    range: 14.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
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
            let audio_len = o.audio.as_ref().map_or(0, |a| a.clip.len());
            obj_bytes += o.name.len() + o.script.len() + o.texture.len() + audio_len;
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

    /// AABB monde de l'objet `o` (AABB local transformé, ré-englobé axe-aligné).
    pub fn world_aabb(&self, o: &SceneObject) -> (Vec3, Vec3) {
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
        (wmin, wmax)
    }

    /// Le point monde `p` est-il dans l'AABB monde de l'objet `o` ?
    pub fn world_aabb_contains(&self, o: &SceneObject, p: Vec3) -> bool {
        let (wmin, wmax) = self.world_aabb(o);
        p.cmpge(wmin).all() && p.cmple(wmax).all()
    }

    /// Les AABB monde de `a` et `b` se chevauchent-ils ? Contrairement à
    /// `world_aabb_contains` (test d'un *point*), ce test réussit dès le *contact* des
    /// volumes : indispensable quand les deux objets ont un corps physique, car les
    /// colliders empêchent alors le centre de l'un d'entrer dans l'AABB de l'autre.
    pub fn world_aabb_intersects(&self, a: &SceneObject, b: &SceneObject) -> bool {
        let (amin, amax) = self.world_aabb(a);
        let (bmin, bmax) = self.world_aabb(b);
        amin.cmple(bmax).all() && bmin.cmple(amax).all()
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

    /// Ramassage d'arme au contact (cf. `WeaponPickup`) : masque le premier butin touché
    /// et renvoie son profil (`WEAPONS[weapon]`) — un seul par appel, contrairement à
    /// `collect_at` qui peut en ramasser plusieurs d'un coup (équiper 2 armes à la fois
    /// n'aurait pas de sens, contrairement à empocher 2 pièces).
    pub fn weapon_pickup_at(&mut self, p: Vec3, radius: f32) -> Option<Weapon> {
        for o in &mut self.objects {
            if let Some(wp) = o.weapon_pickup
                && o.visible
            {
                let piece_r = o.transform.scale.max_element() * 0.5;
                if (o.transform.position - p).length() <= radius + piece_r {
                    o.visible = false;
                    return Some(WEAPONS[wp.weapon]);
                }
            }
        }
        None
    }

    /// Résout une attaque du joueur en `p` (portée `radius`) : vainc (masque) les ennemis
    /// `attackable` encore visibles à portée. Renvoie les indices vaincus (pour score,
    /// son, et mise en file de réapparition côté `App`, comme les bonus).
    /// Ne vainc que **la cible la plus proche** à portée, pas toutes celles dans le
    /// rayon. Audit gameplay : un swing en zone (toutes les cibles à portée à la fois)
    /// laissait un groupe de monstres convergeant ensemble se faire vaincre d'un seul
    /// coup avant qu'aucun n'ait pu mordre — la taille des monstres (donc leur propre
    /// rayon de mise à mort) compense presque exactement leur vitesse, ce qui les fait
    /// arriver à portée de façon quasi synchronisée plutôt qu'échelonnée. Un coup =
    /// une cible force à revenir au corps-à-corps plusieurs fois pour vider un groupe,
    /// laissant une vraie fenêtre aux autres pendant la recharge.
    pub fn attack_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        match self.nearest_attackable(p, radius) {
            Some(i) => {
                self.objects[i].visible = false;
                vec![i]
            }
            None => Vec::new(),
        }
    }

    /// Frappe de zone (cf. `AttackMode::Zone`) : vainc (masque) TOUTES les cibles
    /// `attackable` encore visibles à portée d'un coup, contrairement à `attack_at` qui
    /// n'en vainc qu'une (cf. sa doc : un swing en zone par défaut trivialise un groupe
    /// convergent). Réservée aux armes qui l'assument explicitement via un coût élevé
    /// (préparation/recharge longues, cf. `Weapon::mode` — le Marteau) : le compromis
    /// change selon l'arme équipée, pas selon un swing par défaut universel.
    /// Ne renvoie que les cibles **vaincues** par ce coup (cf. `damage_attackable`) : une
    /// cible à plusieurs points de vie touchée mais encore vivante n'apparaît pas dans le
    /// résultat (elle reste visible, sans recul — le recul en zone n'a pas de direction
    /// unique à appliquer par cible, contrairement au mode `Single`, cf. `AppState`).
    pub fn attack_zone_at(&mut self, p: Vec3, radius: f32) -> Vec<usize> {
        let targets: Vec<usize> = self
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.combat.as_ref().is_some_and(|c| c.attackable) && o.visible)
            .filter(|(_, o)| {
                let enemy_r = o.transform.scale.max_element() * 0.5;
                (o.transform.position - p).length() - enemy_r <= radius
            })
            .map(|(i, _)| i)
            .collect();
        targets
            .into_iter()
            .filter(|&i| self.damage_attackable(i))
            .collect()
    }

    /// Inflige un coup à la cible `i` (décompte `Combat.hp`) : la masque et renvoie
    /// `true` si ce coup l'achève (hp tombé à 0), renvoie `false` si elle survit (hp
    /// encore > 0, reste visible). Distingue un coup qui achève d'un coup qui blesse
    /// seulement — nécessaire pour un duel à plusieurs points de vie (cf.
    /// `Scene::brawl_demo`), impossible à exprimer avec l'ancien `attack_at`/`attack_zone_at`
    /// (masquage immédiat, sans notion de PV restants).
    pub fn damage_attackable(&mut self, i: usize) -> bool {
        let Some(o) = self.objects.get_mut(i) else {
            return false;
        };
        let Some(c) = &mut o.combat else {
            return false;
        };
        c.hp = c.hp.saturating_sub(1);
        if c.hp == 0 {
            o.visible = false;
            true
        } else {
            false
        }
    }

    /// Cible la plus proche à portée, **sans la vaincre** (contrairement à `attack_at`) :
    /// utilisé pour verrouiller la cible d'un missile au moment du tir (cf.
    /// `AppState::attack_projectile`), l'impact réel étant résolu plus tard, à l'arrivée.
    pub fn nearest_attackable(&self, p: Vec3, radius: f32) -> Option<usize> {
        self.objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.combat.as_ref().is_some_and(|c| c.attackable) && o.visible)
            .map(|(i, o)| {
                let enemy_r = o.transform.scale.max_element() * 0.5;
                (i, (o.transform.position - p).length() - enemy_r)
            })
            .filter(|&(_, dist)| dist <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
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
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
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
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
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
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
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
                    group: String::new(),
                    color: [0.4, 0.5, 0.45],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Joueur".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
                    mesh: MeshKind::Capsule,
                    script: player_script.into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.95, 0.6, 0.25],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
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
                    group: String::new(),
                    color: [0.3, 0.6, 0.9],
                    texture: String::new(),
                    tappable: true,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
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
                group: String::new(),
                color: o.color,
                texture: String::new(),
                tappable: o.tappable,
                metallic: 0.0,
                roughness: 0.6,
                emissive: 0.0,
                trigger: false,
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
        assert!(
            platforms >= 10,
            "une vraie tour à gravir, pas un décor minimal"
        );
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
        assert!(
            ctrl.attack_button.is_empty(),
            "pas de combat dans ce style de niveau"
        );
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
        assert!(
            s.objects[0].controller.is_none(),
            "pas pilotable par défaut"
        );
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
                assert!(
                    o.combat.as_ref().is_some_and(|c| c.attackable),
                    "un ennemi doit être une cible d'attaque valide : {i}"
                );
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
        assert!(
            s.attack_at(pos, 1.5).is_empty(),
            "un ennemi déjà vaincu n'est pas retouché"
        );
    }

    #[test]
    fn attack_zone_at_defeats_every_attackable_target_in_range_at_once() {
        // Contrairement à `attack_at` (une seule cible, cf. sa doc), `attack_zone_at`
        // (mode `AttackMode::Zone`, réservé aux armes qui l'assument via un coût élevé,
        // cf. `Weapon::mode` — le Marteau) doit vaincre TOUT un groupe d'un coup.
        let mk_enemy = |name: &str, pos: Vec3| SceneObject {
            name: name.into(),
            transform: Transform::from_pos(pos),
            combat: Some(Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut s = Scene {
            objects: vec![
                mk_enemy("E1", Vec3::new(0.0, 0.5, 0.0)),
                mk_enemy("E2", Vec3::new(0.5, 0.5, 0.0)),
                mk_enemy("E3", Vec3::new(-0.4, 0.5, 0.3)),
                mk_enemy("Loin", Vec3::new(50.0, 0.5, 0.0)),
            ],
            ..Default::default()
        };

        let hit = s.attack_zone_at(Vec3::ZERO, 2.0);
        assert_eq!(
            hit.len(),
            3,
            "les 3 cibles groupées doivent toutes être vaincues d'un coup"
        );
        for &i in &hit {
            assert!(
                !s.objects[i].visible,
                "chaque cible touchée devient invisible"
            );
        }
        assert!(
            s.objects.iter().find(|o| o.name == "Loin").unwrap().visible,
            "une cible hors de portée ne doit pas être concernée"
        );
        assert!(
            s.attack_zone_at(Vec3::ZERO, 2.0).is_empty(),
            "un groupe déjà vaincu n'est pas retouché"
        );
    }

    #[test]
    fn damage_attackable_survives_until_hp_reaches_zero() {
        // Fondation du duel façon Tekken/Smash (`Scene::brawl_demo`) : une cible à
        // plusieurs PV doit encaisser plusieurs coups, pas tomber au premier — la
        // différence entre `damage_attackable` (décompte `Combat.hp`) et l'ancien
        // masquage immédiat de `attack_at`/`attack_zone_at`.
        let mut s = Scene {
            objects: vec![SceneObject {
                name: "Rival".into(),
                combat: Some(Combat {
                    attackable: true,
                    hp: 3,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(
            !s.damage_attackable(0),
            "1er coup : encaisse, ne meurt pas (hp 3 -> 2)"
        );
        assert!(s.objects[0].visible, "encore visible après le 1er coup");
        assert!(
            !s.damage_attackable(0),
            "2e coup : encaisse encore (hp 2 -> 1)"
        );
        assert!(s.objects[0].visible, "encore visible après le 2e coup");
        assert!(
            s.damage_attackable(0),
            "3e coup : achève la cible (hp 1 -> 0)"
        );
        assert!(!s.objects[0].visible, "invisible une fois achevée");
        // Un index invalide ou sans `Combat` ne doit pas paniquer.
        assert!(!s.damage_attackable(99));
    }

    #[test]
    fn brawl_demo_has_a_multi_hit_rival_a_ring_out_void_and_a_single_wave() {
        let s = Scene::brawl_demo();
        // Un seul adversaire (pas des vagues de monstres comme zombies/donjon).
        let rivals: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
        assert_eq!(rivals.len(), 1, "un seul rival, pas des vagues de monstres");
        let rival = rivals[0];
        let combat = rival
            .combat
            .as_ref()
            .expect("le rival doit être attaquable");
        assert!(combat.attackable);
        assert!(
            combat.hp > 1,
            "le rival doit encaisser plusieurs coups, pas tomber au premier : hp={}",
            combat.hp
        );
        // `wave = 1` : réutilise le système de manches existant pour déclencher la
        // victoire dès que le rival est invisible (achevé ou ring out), sans condition
        // de victoire dédiée à cette démo (cf. doc de `Scene::brawl_demo`).
        assert_eq!(combat.wave, 1);
        assert!(
            rival.trigger,
            "le rival doit pouvoir mordre/frapper au contact"
        );

        // Une zone mortelle (le vide) existe : le ring out doit être possible.
        assert!(
            s.objects.iter().any(|o| o.deadly),
            "l'arène doit avoir une zone mortelle (le vide) pour le ring out"
        );
        // Pas de mur autour de l'arène (contrairement au donjon/zombies) : rien n'empêche
        // physiquement de sortir de l'arène.
        assert!(!s.objects.iter().any(|o| o.name.starts_with("Mur")));

        // Le joueur a une attaque courte et vive (façon jab), pas un tir longue portée.
        let player = s
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable")
            .controller
            .as_ref()
            .unwrap();
        assert!(
            player.attack_range < 2.0,
            "portée courte, façon corps-à-corps"
        );

        // Une caméra de jeu est définie (cadrage de duel), pas la vue par défaut.
        assert!(s.game_camera.is_some());
        assert!(s.camera_follow);
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
    fn controller_and_ai_chaser_rust_default_matches_serde_default() {
        // Piège classique : `#[derive(Default)]` donne 0.0/vide à chaque champ, alors
        // que plusieurs ont un défaut serde non trivial (`default = "fn"`). Un
        // `Controller { ..Default::default() }` en Rust doit produire les MÊMES valeurs
        // qu'un objet JSON sans ces champs (désérialisé avec les défauts serde) — sinon
        // les scènes construites en Rust (toutes les démos) divergent silencieusement
        // des scènes chargées depuis un fichier ancien.
        let rust_default = Controller::default();
        let from_json: Controller = serde_json::from_str("{}").unwrap();
        assert_eq!(rust_default.move_speed, from_json.move_speed);
        assert_eq!(rust_default.jump_height, from_json.jump_height);
        assert_eq!(rust_default.attack_range, from_json.attack_range);
        assert_eq!(rust_default.attack_cooldown, from_json.attack_cooldown);
        assert!(
            rust_default.attack_cooldown > 0.0,
            "sans quoi l'attaque n'a aucune limite"
        );

        let ai_rust_default = AiChaser::default();
        let ai_from_json: AiChaser = serde_json::from_str("{}").unwrap();
        assert_eq!(ai_rust_default.speed, ai_from_json.speed);
        assert!(
            ai_rust_default.speed > 0.0,
            "sans quoi le chasseur reste immobile"
        );
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

    #[test]
    fn audio_source_component_is_optional_and_survives_round_trip() {
        // Un objet sans son garde `audio: None` (pas de bloat JSON pour la majorité des
        // objets). Un objet avec son voit ses 3 champs regroupés survivre à la sérialisation.
        let silent = SceneObject::default();
        assert!(silent.audio.is_none());

        let mut o = SceneObject {
            name: "Ambiance".into(),
            ..Default::default()
        };
        o.audio = Some(AudioSource {
            clip: "assets/wind.wav".into(),
            autoplay: true,
            spatial: true,
        });
        let scene = Scene {
            objects: vec![o],
            ..Default::default()
        };
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        let a = back.objects[0].audio.as_ref().expect("audio round-trip");
        assert_eq!(a.clip, "assets/wind.wav");
        assert!(a.autoplay);
        assert!(a.spatial);
    }

    #[test]
    fn combat_component_is_optional_and_survives_round_trip() {
        // Un objet hors combat garde `combat: None` (décor, collectibles...). Un ennemi
        // voit ses 2 champs regroupés (attackable, is_attack_fx) survivre à la sérialisation.
        let peaceful = SceneObject::default();
        assert!(peaceful.combat.is_none());

        let mut o = SceneObject {
            name: "Ennemi".into(),
            ..Default::default()
        };
        o.combat = Some(Combat {
            attackable: true,
            is_attack_fx: false,
            wave: 2,
            ..Default::default()
        });
        let scene = Scene {
            objects: vec![o],
            ..Default::default()
        };
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        let c = back.objects[0].combat.as_ref().expect("combat round-trip");
        assert!(c.attackable);
        assert!(!c.is_attack_fx);
        assert_eq!(c.wave, 2);
    }

    #[test]
    fn components_demo_exercises_exactly_one_object_per_component() {
        // Scène exemple : chaque composant optionnel (Controller/AudioSource/Combat)
        // n'apparaît que là où il est pertinent, jamais sur les autres objets — c'est
        // tout l'intérêt pédagogique (et la preuve que le bloat plat est bien évité).
        let s = Scene::components_demo();
        assert_eq!(
            s.objects.len(),
            5,
            "5 objets : sol, joueur, boîte, cible, FX"
        );

        let with_controller = s.objects.iter().filter(|o| o.controller.is_some()).count();
        assert_eq!(with_controller, 1, "un seul objet pilotable (le joueur)");

        let with_audio = s.objects.iter().filter(|o| o.audio.is_some()).count();
        assert_eq!(with_audio, 1, "un seul objet sonore (la boîte à musique)");

        let attackable = s
            .objects
            .iter()
            .filter(|o| o.combat.as_ref().is_some_and(|c| c.attackable))
            .count();
        assert_eq!(attackable, 1, "une seule cible d'attaque");

        let fx_anchors = s
            .objects
            .iter()
            .filter(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
            .count();
        assert_eq!(fx_anchors, 1, "une seule ancre d'effet visuel");

        // Le sol n'a aucun des trois : c'est du pur décor.
        let sol = s.objects.iter().find(|o| o.name == "Sol").unwrap();
        assert!(sol.controller.is_none() && sol.audio.is_none() && sol.combat.is_none());
    }

    #[test]
    fn zombies_demo_has_four_waves_of_varied_active_chasers() {
        let s = Scene::zombies_demo();
        let monsters: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
        // 3 archétypes distincts (Rôdeur/Coureur/Brute), pas un seul type répété.
        let distinct_names: std::collections::HashSet<&str> = monsters
            .iter()
            .map(|o| o.name.split(' ').next().unwrap())
            .collect();
        assert!(
            distinct_names.len() >= 3,
            "au moins 3 archétypes de monstres différents : {distinct_names:?}"
        );
        for m in &monsters {
            assert!(
                m.ai_chaser.is_some(),
                "un monstre doit poursuivre activement, pas suivre un script de patrouille"
            );
            assert!(
                m.combat.as_ref().is_some_and(|c| c.attackable),
                "un monstre doit être une cible d'attaque valide (défendable)"
            );
            assert!(
                m.trigger,
                "un monstre doit détecter le contact pour infliger des dégâts"
            );
            assert!(
                m.combat.as_ref().is_some_and(|c| c.wave > 0),
                "un monstre doit appartenir à une manche"
            );
            assert_eq!(
                m.respawn_delay, 0.0,
                "un monstre vaincu reste mort pour la manche"
            );
        }
        // 4 manches, difficulté croissante (de plus en plus de monstres).
        let max_wave = monsters
            .iter()
            .filter_map(|o| o.combat.as_ref())
            .map(|c| c.wave)
            .max()
            .unwrap();
        assert_eq!(max_wave, 4, "4 manches");
        let per_wave = |w: u32| {
            monsters
                .iter()
                .filter(|o| o.combat.as_ref().is_some_and(|c| c.wave == w))
                .count()
        };
        assert!(
            per_wave(1) < per_wave(4),
            "la dernière manche doit être plus dense"
        );

        // Pas d'objectif « collectible » séparé : la victoire vient de vider les manches
        // (cf. `App::update_waves`), pas de ramasser une gemme.
        assert!(s.collectibles().is_none());
        assert!(!s.objects.iter().any(|o| o.name == "Lave"));

        let player = s
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable");
        assert!(!player.controller.as_ref().unwrap().attack_button.is_empty());
    }

    #[test]
    fn mmorpg_demo_is_a_bare_arena_with_no_monsters_and_mobile_controls_on() {
        let s = Scene::mmorpg_demo();
        assert!(
            !s.objects.iter().any(|o| o.ai_chaser.is_some()),
            "la démo MMORPG ne doit avoir aucun monstre (test de connectivité, pas de combat)"
        );
        assert!(
            s.mobile.joystick,
            "le joystick doit être actif par défaut, sans passer par l'éditeur (APK direct)"
        );
        let player = s
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable");
        assert!(!player.controller.as_ref().unwrap().jump_button.is_empty());
    }

    #[test]
    fn exactly_one_weapon_profile_uses_the_zone_attack_mode() {
        // Le mode `Zone` (frappe tout un groupe d'un coup) reste une exception délibérée,
        // pas la norme : un seul profil l'assume (le Marteau, via son coût le plus élevé
        // de la table — préparation et recharge les plus longues), tous les autres
        // restent en mode `Single` (comportement historique de toutes les démos).
        let zone: Vec<_> = WEAPONS
            .iter()
            .filter(|w| w.mode == AttackMode::Zone)
            .collect();
        assert_eq!(zone.len(), 1, "un seul profil en mode Zone : {zone:?}");
        assert_eq!(zone[0].label, "Marteau");
        assert_eq!(
            zone[0].windup,
            WEAPONS.iter().map(|w| w.windup).fold(0.0, f32::max),
            "le mode Zone doit rester la préparation la plus longue de la table"
        );
    }

    #[test]
    fn roguelike_demo_has_three_rooms_one_monster_each_and_a_random_weapon() {
        let s = Scene::roguelike_demo();
        let monsters: Vec<_> = s.objects.iter().filter(|o| o.ai_chaser.is_some()).collect();
        assert_eq!(monsters.len(), 3, "une salle = un monstre, 3 salles");
        // 3 archétypes distincts (Gobelin/Squelette/Ogre), un par salle.
        let distinct_names: std::collections::HashSet<&str> =
            monsters.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(
            distinct_names.len(),
            3,
            "3 monstres distincts, pas 3 copies du même"
        );
        for m in &monsters {
            assert!(
                m.combat
                    .as_ref()
                    .is_some_and(|c| c.attackable && c.wave > 0),
                "chaque monstre doit être une cible d'attaque valide, une manche = une salle"
            );
            assert!(m.trigger, "un monstre doit détecter le contact pour mordre");
        }
        // Une salle à la fois : 3 manches distinctes, une par monstre (pas plusieurs
        // monstres entassés dans la même manche comme dans `zombies_demo`).
        let waves: std::collections::HashSet<u32> = monsters
            .iter()
            .filter_map(|o| o.combat.as_ref())
            .map(|c| c.wave)
            .collect();
        assert_eq!(waves, std::collections::HashSet::from([1, 2, 3]));

        // Arme de départ : un des 5 profils connus (`WEAPONS`), jamais les défauts
        // génériques de `Controller` (qui ne correspondent à aucun des 5).
        let player = s
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("un joueur pilotable")
            .controller
            .as_ref()
            .unwrap();
        let stat = |w: &Weapon| (w.range, w.cooldown, w.windup);
        let starting = stat(
            WEAPONS
                .iter()
                .find(|w| {
                    stat(w)
                        == (
                            player.attack_range,
                            player.attack_cooldown,
                            player.attack_windup,
                        )
                })
                .expect("l'arme de départ doit être l'un des 5 profils connus, pas les défauts génériques"),
        );

        // 2 butins d'arme dans le donjon (cf. `WeaponPickup`), un par salle 1/2 — la
        // salle 3 (l'Ogre) n'en a pas : le joueur doit avoir déjà trouvé sa meilleure
        // arme avant d'y entrer.
        let loot: Vec<_> = s
            .objects
            .iter()
            .filter_map(|o| o.weapon_pickup.map(|wp| WEAPONS[wp.weapon]))
            .collect();
        assert_eq!(
            loot.len(),
            2,
            "2 butins d'arme, un dans chaque première salle"
        );
        // Les 3 armes en jeu (départ + 2 butins) doivent être 3 profils DISTINCTS :
        // sinon trouver un butin n'apporterait rien (même arme que celle déjà en main).
        let mut all_three: std::collections::HashSet<(u32, u32, u32)> = loot
            .iter()
            .map(|w| (w.range.to_bits(), w.cooldown.to_bits(), w.windup.to_bits()))
            .collect();
        all_three.insert((
            starting.0.to_bits(),
            starting.1.to_bits(),
            starting.2.to_bits(),
        ));
        assert_eq!(
            all_three.len(),
            3,
            "l'arme de départ et les 2 butins doivent être 3 profils distincts"
        );

        // Portes fermées (pas de couloir séparé) entre les salles : au moins 4 segments
        // de mur transversal supplémentaires (2 portes à 2 segments chacune), en plus de
        // l'enveloppe extérieure à 4 murs.
        let walls = s
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Mur") || o.name.starts_with("Porte"))
            .count();
        assert!(
            walls >= 8,
            "enveloppe (4 murs) + 2 portes à 2 segments : {walls}"
        );
    }

    /// Sur un grand nombre de tirages, les profils d'arme tirés ne doivent pas toujours
    /// être les mêmes — sinon le tirage serait biaisé (ou codé en dur sur un seul profil).
    #[test]
    fn roguelike_demo_weapon_draw_is_not_always_the_same_profile() {
        let mut seen: std::collections::HashSet<(u32, u32, u32)> = std::collections::HashSet::new();
        for _ in 0..40 {
            let s = Scene::roguelike_demo();
            let c = s
                .objects
                .iter()
                .find_map(|o| o.controller.as_ref().filter(|c| c.input))
                .unwrap();
            // Bits flottants exacts (valeurs codées en dur, pas de calcul) : comparaison
            // par bits sûre pour un ensemble de discrimination.
            seen.insert((
                c.attack_range.to_bits(),
                c.attack_cooldown.to_bits(),
                c.attack_windup.to_bits(),
            ));
            if seen.len() >= 2 {
                break;
            }
        }
        assert!(
            seen.len() >= 2,
            "40 tirages n'ont produit qu'un seul profil d'arme : le tirage semble figé"
        );
    }

    #[test]
    fn weapon_pickup_at_equips_the_right_profile_and_is_one_shot() {
        let mut s = Scene::roguelike_demo();
        let (pos, expected) = s
            .objects
            .iter()
            .find_map(|o| {
                o.weapon_pickup
                    .map(|wp| (o.transform.position, WEAPONS[wp.weapon]))
            })
            .expect("le donjon a au moins un butin d'arme");

        let got = s
            .weapon_pickup_at(pos, 0.9)
            .expect("doit ramasser le butin exactement sur sa position");
        assert_eq!(
            (got.range, got.cooldown, got.windup),
            (expected.range, expected.cooldown, expected.windup),
            "doit renvoyer le profil du butin ramassé, pas un autre"
        );

        // Ramassage à usage unique : retoucher le même endroit ne renvoie plus rien
        // (l'objet a été masqué), contrairement à une pièce qui pourrait réapparaître.
        assert!(
            s.weapon_pickup_at(pos, 0.9).is_none(),
            "un butin déjà ramassé ne doit pas se reramasser"
        );

        // Très loin de tout butin : rien ramassé.
        assert!(
            s.weapon_pickup_at(Vec3::new(500.0, 0.5, 500.0), 0.9)
                .is_none()
        );
    }
}
