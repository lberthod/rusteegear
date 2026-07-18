//! Modèle de scène (sans ECS) : un Vec d'objets, chacun avec un Transform et un type de mesh.

mod demos;
mod hud_widgets;
pub mod import;
mod mobile;
mod persistence;
mod prefab;
mod queries;

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::gfx::mesh::{self, MeshData};
use crate::runtime::physics::PhysicsKind;
pub use hud_widgets::{HudAnchor, HudBinding, HudLayout, HudWidget, HudWidgetKind};
pub use mobile::MobileControls;
pub use prefab::PrefabInstance;

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

/// Angles d'Euler XYZ (radians) d'une rotation, **canonicalisés** pour le
/// round-trip lecture→réécriture : `Quat::to_euler(XYZ)` contraint l'angle du
/// milieu (Y) à [-90°, 90°], donc un yaw pur au-delà s'exprime
/// `(±180°, 180°−yaw, ±180°)`. Piège : un script Lua (ou l'inspecteur) qui
/// n'écrase que `ry` — le cap d'un personnage, le cas le plus courant —
/// recompose alors son yaw avec les flips ±180° restés dans rx/rz, soit un cap
/// effectif de `180°−ry` ; comme la représentation extraite alterne d'un tick à
/// l'autre, la rotation flip-floppe entre `ry` et `180°−ry` à 60 Hz (bug observé
/// sur la créature MMORPG : « les bras et la tête partent en couille dès qu'elle
/// tourne », silhouette dédoublée — l'écart vaut 2×(|yaw|−90°), jusqu'à 180°
/// plein sud). On renvoie le triplet **équivalent** sans flips (identité
/// Tait-Bryan : `(a, b, c) ≡ (a∓180°, 180°−b, c∓180°)`) : même rotation, mais
/// `ry` porte le yaw entier [-180°, 180°] et rx/rz restent proches de 0 pour
/// une rotation quasi-planaire — stable au round-trip. Utilisé par
/// `app::scripting::run_script` (exclu de wasm32 avec Lua, d'où sa place ici)
/// et l'inspecteur de l'éditeur.
pub fn canonical_euler_xyz(q: Quat) -> (f32, f32, f32) {
    use std::f32::consts::PI;
    let (mut rx, mut ry, mut rz) = q.to_euler(glam::EulerRot::XYZ);
    if rx.abs() > PI * 0.5 && rz.abs() > PI * 0.5 {
        rx -= PI * rx.signum();
        rz -= PI * rz.signum();
        ry = PI - ry;
        if ry > PI {
            ry -= 2.0 * PI;
        }
    }
    (rx, ry, rz)
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

/// Géométrie importée d'un fichier glTF. `data`/`aabb`/`skeleton`/`clips` sont
/// reconstruits au chargement (jamais sérialisés — juste dérivés de `path`, cf.
/// `reload_imported`).
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
    /// Squelette du glTF, `None` si le fichier n'a pas de skin — un mesh
    /// statique n'a simplement rien à squeletter.
    #[serde(skip)]
    pub skeleton: Option<import::Skeleton>,
    /// Clips d'animation du glTF, liés aux joints de `skeleton` ci-dessus.
    /// Vide si le fichier n'a ni skin ni animation.
    #[serde(skip)]
    pub clips: Vec<import::Clip>,
    /// Poids de peau par sommet, alignés avec `data.vertices` — nécessaires
    /// pour construire le mesh GPU skinné (`gfx::mesh::SkinnedVertex`).
    #[serde(skip)]
    pub vertex_skins: Vec<import::VertexSkin>,
    /// Tangente par sommet, alignée avec `data.vertices` : xyz = tangente,
    /// w = signe de la bitangente (`cross(normal, tangent) * w`). Calculée pour **tout**
    /// mesh importé (skinné ou non) — contrairement à `skeleton`/`clips`/`vertex_skins`,
    /// rien ici ne dépend d'un skin glTF. Donnée pure, pas encore consommée par le
    /// rendu (aucun normal mapping actuellement).
    #[serde(skip)]
    pub tangents: Vec<[f32; 4]>,
    /// Marqueurs temporels par nom de clip : `(temps en secondes dans le
    /// clip, nom d'événement)`. **Sérialisé** — contrairement à `clips` ci-dessus,
    /// entièrement rederivé du glTF à chaque chargement (le format n'a pas de notion
    /// standard de marqueur) : ce champ-ci est authored à la main (éditeur ou test), il
    /// doit donc survivre à la sauvegarde/au chargement de la scène. Un événement
    /// `anim:<nom>` (cf. `AppState::game_events`) est émis quand la lecture
    /// d'un clip franchit son temps — cf. `notifies_crossed`.
    #[serde(default)]
    pub notifies: std::collections::HashMap<String, Vec<(f32, String)>>,
}

impl ImportedMesh {
    /// Clip joué par défaut quand un objet utilisant ce mesh n'a pas d'`AnimationState` :
    /// « Idle » si présent (convention de tous les packs Blender du projet,
    /// cf. `scripts/blender/`), sinon le premier clip du fichier. `None` si le mesh
    /// n'a aucun clip (statique) — rien à jouer.
    pub fn default_clip(&self) -> Option<&str> {
        self.clips
            .iter()
            .find(|c| c.name == "Idle")
            .or_else(|| self.clips.first())
            .map(|c| c.name.as_str())
    }

    /// Recharge `skeleton`/`clips`/`vertex_skins` depuis `path`, **et**
    /// `tangents` depuis `data` déjà chargée — malgré son nom, cette méthode
    /// recalcule toute donnée dérivée non sérialisée d'un mesh importé, pas seulement le
    /// squelettage ; regroupées ici plutôt qu'en méthodes séparées puisque tous les
    /// appelants (`reload_imported`, tests) les invoquent toujours ensemble, juste après
    /// avoir affecté `self.data`. Reparse le glTF séparément de `data` (`import::
    /// load_gltf`) pour le squelette : un peu redondant (deux passes sur le même
    /// fichier), mais garde le squelettage entièrement optionnel et sans coût pour les
    /// meshes statiques, qui restent sur le seul chemin `load_gltf` existant. Silencieux
    /// en cas d'erreur de lecture du squelette (log seulement) : son absence ne doit pas
    /// empêcher un mesh statique de s'afficher normalement.
    pub fn load_skinning(&mut self) {
        self.skeleton = None;
        self.clips.clear();
        self.vertex_skins.clear();
        match import::load_gltf_skeleton(&self.path) {
            Ok(Some((skeleton, vertex_skins))) => {
                self.skeleton = Some(skeleton);
                self.vertex_skins = vertex_skins;
            }
            Ok(None) => {} // pas de skin : mesh statique, rien à faire
            Err(e) => log::error!("Lecture du squelette de {} échouée : {e}", self.path),
        }
        if self.skeleton.is_some() {
            match import::load_gltf_clips(&self.path) {
                Ok(clips) => self.clips = clips,
                Err(e) => log::error!("Lecture des clips de {} échouée : {e}", self.path),
            }
        }
        self.tangents = import::compute_tangents(&self.data.vertices, &self.data.indices);
    }

    /// Combine `data.vertices` (position/normale/couleur/uv) et `vertex_skins`
    /// (joints/poids) en un `SkinnedMeshData` prêt pour le GPU. `None` si
    /// le mesh n'a pas de squelette, ou si les deux tableaux ont désynchronisé (ne devrait
    /// jamais arriver — les deux sont construits dans le même ordre par
    /// `import::build_from`/`read_vertex_skins` — mais un mesh statique
    /// rendu avec des données incohérentes serait pire qu'un mesh simplement pas skinné).
    pub fn skinned_mesh_data(&self) -> Option<crate::gfx::mesh::SkinnedMeshData> {
        self.skeleton.as_ref()?;
        if self.data.vertices.len() != self.vertex_skins.len() {
            log::error!(
                "{} : {} sommets mais {} poids de peau — squelette ignoré",
                self.path,
                self.data.vertices.len(),
                self.vertex_skins.len()
            );
            return None;
        }
        let vertices = self
            .data
            .vertices
            .iter()
            .zip(&self.vertex_skins)
            .map(|(v, s)| crate::gfx::mesh::SkinnedVertex {
                position: v.position,
                normal: v.normal,
                color: v.color,
                uv: v.uv,
                joints: [
                    s.joints[0] as u32,
                    s.joints[1] as u32,
                    s.joints[2] as u32,
                    s.joints[3] as u32,
                ],
                weights: s.weights,
            })
            .collect();
        Some(crate::gfx::mesh::SkinnedMeshData {
            vertices,
            indices: self.data.indices.clone(),
        })
    }
}

/// Composant optionnel : son associé à un `SceneObject` (clip, autoplay, spatialisation).
/// `None` = aucun son — la grande majorité des objets d'une scène n'en ont pas ; les y
/// laisser à plat (3 champs) aurait alourdi tous les objets pour rien. Même logique de
/// migration que `Controller`.
#[derive(Clone, Serialize, Deserialize)]
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
    /// Multiplicateur de gain calculé à l'import (Sprint 126, normalisation de
    /// loudness) : appliqué en plus de l'atténuation spatiale au moment de la
    /// lecture, pas ré-encodé dans le fichier lui-même — un clip trop fort/faible
    /// enregistré au studio se joue à un niveau cohérent avec les autres sans
    /// avoir à retoucher l'asset. `1.0` = inchangé (valeur par défaut pour les
    /// scènes existantes, qui n'ont jamais eu ce champ).
    #[serde(default = "default_audio_gain")]
    pub gain: f32,
}

fn default_audio_gain() -> f32 {
    1.0
}

impl Default for AudioSource {
    fn default() -> Self {
        Self {
            clip: String::new(),
            autoplay: false,
            spatial: false,
            gain: default_audio_gain(),
        }
    }
}

/// Mécanique de résolution de l'attaque du joueur (cf. `Controller::attack_mode`).
/// `Single` reste le comportement par défaut de toutes les démos existantes : une
/// résolution de zone par défaut vainc trivialement tout un groupe convergent avant
/// qu'aucun monstre n'ait pu mordre (cf. docs/audits/scene-mod.md).
/// `Zone` redevient disponible ici en **opt-in par arme** (cf. `Weapon::mode`, le
/// Marteau) : le coût (préparation/recharge longues) compense le fait de vaincre tout
/// un groupe d'un coup, ce qu'un défaut non compensé ne ferait pas.
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
    /// `Physics::control`).
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
    /// Nom du bouton tactile qui tire une boule de feu devant soi (vide = pas de tir
    /// à distance). Combiné à la touche clavier Feu (K, desktop) — cf.
    /// `PlayerInput::fire` et `app::fireball` : contrairement à `attack_button`
    /// (coup au contact/missile verrouillé sur une cible), la boule de feu part en
    /// ligne droite dans l'orientation du personnage et frappe le premier obstacle
    /// physique ou monstre sur son chemin.
    #[serde(default)]
    pub fire_button: String,
    /// Nom du bouton tactile qui fait défiler l'arme à distance équipée (vide =
    /// pas de changement d'arme au tactile). Pendant du clavier 1/2/3 (sélection
    /// directe) — le bouton, lui, **cycle** (front montant uniquement, cf.
    /// `AppState::update_fireballs`) : un seul bouton suffit à l'écran tactile.
    #[serde(default)]
    pub weapon_button: String,
    /// Nom du bouton tactile qui soigne l'allié blessé le plus proche à portée
    /// (vide = pas de soin au tactile). Pendant tactile de la touche clavier
    /// Soin (H) — cf. `app::health`, GAMEDESIGN_EN_LIGNE.md §3.6 : action
    /// continue (pas d'appui unique), résolue et validée côté serveur.
    #[serde(default)]
    pub heal_button: String,
    /// Portée (mètres) de l'attaque, centrée sur la position de l'objet.
    #[serde(default = "default_attack_range")]
    pub attack_range: f32,
    /// Temps de recharge (s) entre deux attaques (0 = pas de limite — à éviter : sans
    /// recharge, maintenir le bouton défait instantanément tout ce qui entre en portée,
    /// sans risque). Cf. `AppState::attack_cooldown_remaining`.
    #[serde(default = "default_attack_cooldown")]
    pub attack_cooldown: f32,
    /// Temps de préparation (s) entre l'appui et le départ du missile (0 = tir immédiat).
    /// Le temps de vol du missile seul ne suffit pas à garantir un risque en 1 contre 1
    /// (un missile homing tiré dès l'entrée en portée arrive presque toujours avant
    /// qu'un monstre en approche directe n'atteigne sa propre portée de morsure) — un
    /// temps de préparation, lui, laisse la cible continuer d'approcher *avant même que
    /// le missile ne parte*, créant une vraie fenêtre de vulnérabilité.
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
            fire_button: String::new(),
            weapon_button: String::new(),
            heal_button: String::new(),
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
    /// Un ennemi qui réapparaît (`respawn_delay` positif) est remis à ses PV
    /// d'origine (cf. `max_hp`), pas laissé à 0 — sinon il reviendrait « déjà
    /// vaincu », re-masqué au premier coup sans jamais encaisser ses PV.
    #[serde(default = "default_combat_hp")]
    pub hp: u32,
    /// PV d'origine, capturés au **premier coup reçu** (cf.
    /// `Scene::damage_attackable_by`) : 0 = pas encore capturés (jamais touché).
    /// Sert à restaurer `hp` au respawn (cf. `AppState::process_respawns`) — un
    /// champ d'exécution, pas d'authoring, donc jamais sérialisé (`skip`) : les
    /// scènes JSON existantes n'ont pas à le connaître, `hp` reste la seule
    /// source d'authoring des PV.
    #[serde(skip)]
    pub max_hp: u32,
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
            max_hp: 0,
        }
    }
}

fn default_combat_hp() -> u32 {
    1
}

/// Attaque au contact d'une créature scriptée — cf. `SceneObject::bite`. Mêmes
/// paramètres que ceux passés à `scene::demos::creature_bite_script` (moins `salt`,
/// un détail du tirage Lua sans équivalent nécessaire côté natif).
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct BiteAttack {
    /// Temps minimal (s) entre deux tentatives, réussies ou non.
    pub cooldown: f32,
    /// Probabilité qu'une tentative au contact se concrétise réellement.
    pub chance: f32,
    /// Vie retirée par morsure réussie.
    pub damage: f32,
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

/// Nature d'un objet d'inventaire (cf. `ItemPickup`). Enum fermé plutôt qu'un
/// nom libre : le gameplay natif (soin à l'utilisation, couleur HUD) a besoin
/// de connaître chaque sorte — même choix que `WEAPONS` (table fixe) contre
/// une donnée ouverte que rien ne saurait interpréter.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub enum ItemKind {
    /// Potion de soin : consommable, rend une bonne part de la vie.
    Potion,
    /// Baie : petit consommable de soin, plus courant que la potion.
    Baie,
    /// Clé : non consommable — sert d'objectif/collection (portes futures).
    Cle,
    /// Gemme : trésor à collectionner, non consommable.
    Gemme,
}

impl ItemKind {
    /// Toutes les sortes, pour les listes déroulantes de l'éditeur.
    pub const ALL: [ItemKind; 4] = [
        ItemKind::Potion,
        ItemKind::Baie,
        ItemKind::Cle,
        ItemKind::Gemme,
    ];

    /// Nom affiché dans le HUD et les journaux (projet en français).
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Potion => "Potion de soin",
            ItemKind::Baie => "Baie",
            ItemKind::Cle => "Clé",
            ItemKind::Gemme => "Gemme",
        }
    }

    /// Couleur de la pastille HUD — et teinte conseillée pour l'objet posé en
    /// scène (cf. `demos`), pour que le sac reflète ce qu'on a vu au sol.
    pub fn color(self) -> [f32; 3] {
        match self {
            ItemKind::Potion => [0.9, 0.2, 0.35],
            ItemKind::Baie => [0.95, 0.55, 0.2],
            ItemKind::Cle => [0.95, 0.85, 0.25],
            ItemKind::Gemme => [0.35, 0.55, 0.95],
        }
    }

    /// Vie rendue quand l'objet est **utilisé** depuis le sac (0 = pas
    /// consommable : l'objet reste dans l'inventaire, aucun bouton Utiliser).
    pub fn heal(self) -> f32 {
        match self {
            ItemKind::Potion => 0.35,
            ItemKind::Baie => 0.15,
            ItemKind::Cle | ItemKind::Gemme => 0.0,
        }
    }
}

/// Composant optionnel : objet d'**inventaire** à ramasser au contact (cf.
/// `Scene::item_pickups_at`) — il rejoint le sac du joueur (`AppState::inventory`)
/// au lieu d'équiper une arme (`WeaponPickup`) ou de compter comme pièce-objectif
/// (`collect_at`). Comme le butin d'arme, il ne participe **pas** à la condition
/// de victoire des collectibles.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ItemPickup {
    /// Sorte d'objet ramassé.
    pub kind: ItemKind,
    /// Quantité ajoutée au sac (ex. un buisson qui donne 3 baies d'un coup).
    #[serde(default = "default_item_count")]
    pub count: u32,
}

fn default_item_count() -> u32 {
    1
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
    /// Détection de collision continue : évite qu'un corps rapide et fin
    /// (missile, projectile) traverse un décor mince sans jamais entrer en collision
    /// en un seul pas de simulation (« tunneling » — cf. docs/audits/physics.md). Coûteux
    /// (calcul supplémentaire par pas) : `false` par défaut, réservé aux objets qui en
    /// ont réellement besoin plutôt qu'activé partout par précaution.
    #[serde(default)]
    pub ccd: bool,
    /// Couche(s) de collision de cet objet (bits, cf. `rapier3d::geometry::Group`) —
    /// avec `collision_mask`, permet par exemple qu'un projectile ami traverse les
    /// joueurs de sa propre équipe. Toutes les couches par défaut (`u32::MAX`) :
    /// aucune scène existante ne change de comportement tant que ce champ n'est pas
    /// explicitement réglé.
    #[serde(default = "default_collision_mask")]
    pub collision_layer: u32,
    /// Couches avec lesquelles cet objet entre en collision (bits). Toutes par défaut.
    #[serde(default = "default_collision_mask")]
    pub collision_mask: u32,
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
    /// Force de zone (Sprint 125, vent/buoyancy) : vecteur monde (m/s², type
    /// accélération — appliqué à la vitesse, pas une impulsion ponctuelle) ajouté
    /// chaque pas de simulation à tout corps **dynamique** dont l'AABB touche celui
    /// de cet objet, tant que `trigger` est vrai (sans `trigger`, une zone de vent
    /// n'a pas de volume de détection — cohérent avec les autres zones de cette
    /// scène). `None` pour la grande majorité des objets, qui n'ont pas de vent.
    #[serde(default)]
    pub wind: Option<Vec3>,
    // --- Composants mobiles Android ---
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
    /// Attaque au contact d'une créature scriptée (morsure, pincement...) — cf.
    /// `BiteAttack`. `None` pour la grande majorité des objets. Redondant avec le
    /// script Lua de contact (`scene::demos::creature_bite_script`) qui pilote déjà
    /// cette attaque en solo : ce champ **persiste la même intention** dans la scène
    /// sérialisée (le script Lua, lui, ne l'est qu'en texte opaque), pour qu'un code
    /// natif — la résolution de dégâts réseau (`app::health`) — puisse retrouver
    /// « quelles créatures mordent » sans redécouvrir chaque nom en dur : générique à
    /// toute créature future qui poserait ce champ, pas câblé sur une créature précise.
    #[serde(default)]
    pub bite: Option<BiteAttack>,
    /// Butin d'arme à ramasser au contact (cf. `WeaponPickup`) : `None` pour la grande
    /// majorité des objets — seuls les butins du donjon roguelike en ont.
    #[serde(default)]
    pub weapon_pickup: Option<WeaponPickup>,
    /// Objet d'inventaire à ramasser au contact (cf. `ItemPickup`) : `None` pour la
    /// grande majorité des objets — seuls les objets « trouvables » (potions, clés…)
    /// en ont.
    #[serde(default)]
    pub item_pickup: Option<ItemPickup>,
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
    /// Animation squelettale : `None` pour la grande majorité des objets —
    /// seuls les meshes importés skinnés (`ImportedMesh::skeleton` renseigné) en ont un
    /// usage réel ; sans effet sur un mesh statique (aucun joint à animer).
    #[serde(default)]
    pub animation: Option<AnimationState>,
    /// Lien vers un prefab : `None` pour un objet indépendant (la grande
    /// majorité). `Some` fait de cet objet une **instance** — resynchronisée depuis le
    /// template par `Scene::sync_prefab_instances`, champ par champ, sauf ceux listés
    /// dans `PrefabInstance::overrides`.
    #[serde(default)]
    pub prefab: Option<PrefabInstance>,
    /// Étiquette libre interrogeable en Lua via `find_tag("nom")` — vide =
    /// n'apparaît dans aucune recherche. Distinct de `group` (dossier de la hiérarchie,
    /// usage éditeur) et de `name` (affichage, pas forcément stable/unique) : un tag
    /// sert spécifiquement au script à retrouver des objets par rôle de gameplay
    /// (ex. plusieurs ennemis tagués `"ennemi"`) sans connaître leurs indices.
    #[serde(default)]
    pub tag: String,
}

/// Composant optionnel : lecture d'un clip d'animation squelettale. `None` =
/// pose de liaison figée (mesh skinné mais immobile) — même logique que `Controller`/
/// `Combat` : la plupart des objets, même skinnés, n'ont pas forcément besoin d'un clip
/// qui tourne (ex. un décor posé en pose figée).
#[derive(Clone, Serialize, Deserialize)]
pub struct AnimationState {
    /// Nom du clip à jouer (doit correspondre à un `Clip::name` de
    /// `ImportedMesh::clips`) ; vide ou introuvable ⇒ pose de liaison.
    #[serde(default)]
    pub clip: String,
    /// Position de lecture (secondes), rebouclée automatiquement par `Clip::sample_joint`.
    /// Avance de `dt` à chaque pas de simulation fixe en Play (cf. `AppState::sim_step`).
    #[serde(default)]
    pub time: f32,
    /// Multiplicateur de vitesse de lecture (1.0 = normal, 0 = figé sur `time` courant).
    #[serde(default = "default_anim_speed")]
    pub speed: f32,
    /// Clip quitté pendant une transition en fondu ; vide = pas de transition
    /// en cours, `clip` se lit pur. Renseigné par `set_clip` quand `clip` change.
    #[serde(default)]
    pub prev_clip: String,
    /// Position de lecture de `prev_clip` au moment du changement, avancée comme `time`
    /// tant que la transition dure (le clip quitté continue de jouer pendant le fondu,
    /// il ne se fige pas).
    #[serde(default)]
    pub prev_time: f32,
    /// Progression du fondu 0..1 (0 = `prev_clip` pur, 1 = `clip` pur — transition
    /// terminée). Avance de `dt / crossfade` à chaque pas fixe (cf. `AppState::sim_step`).
    /// `1.0` par défaut : un `AnimationState` fraîchement créé joue `clip` pur, pas mélangé
    /// à un `prev_clip` vide.
    #[serde(default = "default_anim_blend")]
    pub blend: f32,
}

impl AnimationState {
    /// Durée du fondu enchaîné entre deux clips (secondes).
    pub const CROSSFADE_SECONDS: f32 = 0.2;

    /// Change le clip joué, en démarrant un fondu enchaîné depuis le clip actuel si
    /// `clip` diffère réellement (sans effet si on redemande le clip déjà en cours — pas
    /// de fondu ni de redémarrage à chaque frame si un script réaffecte `obj.anim` en
    /// boucle avec la même valeur). Le nouveau clip repart de `time = 0.0` — convention
    /// simple et prévisible plutôt que de préserver la phase.
    pub fn set_clip(&mut self, clip: impl Into<String>) {
        let clip = clip.into();
        if clip == self.clip {
            return;
        }
        self.prev_clip = std::mem::replace(&mut self.clip, clip);
        self.prev_time = self.time;
        self.time = 0.0;
        self.blend = 0.0;
    }
}

fn default_anim_speed() -> f32 {
    1.0
}

fn default_anim_blend() -> f32 {
    1.0
}

impl Default for AnimationState {
    // Manuel plutôt que `#[derive(Default)]` : `blend` doit valoir 1.0 (« pas de
    // transition en cours ») par défaut, comme pour la désérialisation JSON
    // (`#[serde(default = "default_anim_blend")]`) — `derive(Default)` donnerait 0.0
    // (`f32::default()`), ce qui lirait un `AnimationState` fraîchement créé comme
    // mélangé à un `prev_clip` vide plutôt que jouant `clip` pur.
    fn default() -> Self {
        Self {
            clip: String::new(),
            time: 0.0,
            speed: default_anim_speed(),
            prev_clip: String::new(),
            prev_time: 0.0,
            blend: default_anim_blend(),
        }
    }
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
            ccd: false,
            collision_layer: default_collision_mask(),
            collision_mask: default_collision_mask(),
            audio: None,
            group: String::new(),
            color: white(),
            texture: String::new(),
            tappable: false,
            metallic: 0.0,
            roughness: default_roughness(),
            emissive: 0.0,
            trigger: false,
            wind: None,
            controller: None,
            vibrate_on_tap: 0,
            combat: None,
            ai_chaser: None,
            bite: None,
            weapon_pickup: None,
            item_pickup: None,
            tap_action: TapAction::None,
            visible: true,
            deadly: false,
            respawn_delay: 0.0,
            animation: None,
            prefab: None,
            tag: String::new(),
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

/// Toutes les couches : défaut pour `collision_layer`/`collision_mask`,
/// équivalent à l'absence de filtrage.
fn default_collision_mask() -> u32 {
    u32::MAX
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
    /// Ciel (dégradé horizon/zénith) + brouillard exponentiel.
    #[serde(default)]
    pub sky: Sky,
    /// Version du schéma JSON : `0` = scène antérieure à ce champ
    /// (« legacy »), migrée automatiquement par `Scene::load` (cf. `migrate`). Ne pas
    /// relire à la main ; sert seulement à savoir quelles migrations restent à
    /// appliquer, jamais à décider d'un comportement de gameplay.
    #[serde(default)]
    pub version: u32,
    /// Décalages des overlays HUD (réticule, arme, frags, inventaire, joueurs) par
    /// rapport à leur position par défaut — réglables en les glissant dans l'éditeur
    /// (panneau 👁 Aperçu HUD › 🖐 Repositionner). Persistés dans la scène :
    /// s'appliquent donc aussi bien en Play qu'en jeu exporté (APK/player), pas
    /// seulement à l'aperçu éditeur.
    #[serde(default)]
    pub hud_layout: HudLayout,
    /// Widgets de HUD déclaratifs (texte, image, jauge, bouton) : contenu et
    /// liaison aux valeurs de jeu décrits en donnée de scène plutôt qu'en code Rust
    /// dédié — un niveau exporté peut définir son propre HUD sans toucher au moteur.
    /// S'affichent au-dessus des overlays historiques (`hud_layout`), pas à leur
    /// place : ceux-ci restent le chemin garanti (vie, viseur…) tant que tous les
    /// niveaux existants n'ont pas migré.
    #[serde(default)]
    pub hud_widgets: Vec<HudWidget>,
}

/// Fond de scène : dégradé de ciel dessiné derrière toute la géométrie,
/// et brouillard exponentiel mélangé dans le shader PBR selon la distance à la caméra.
///
/// **Par défaut identique à l'ancien fond fixe** (`0.07, 0.08, 0.1`, la même couleur que
/// l'ancien `wgpu::Color` codé en dur du clear) : `horizon_color == zenith_color` produit
/// un dégradé plat visuellement indiscernable de l'ancien rendu, et `fog_density = 0.0`
/// désactive le brouillard — aucune scène existante (ni ses goldens) ne change d'aspect
/// tant que ces champs ne sont pas explicitement réglés dans l'inspecteur.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Sky {
    /// Couleur du ciel près de l'horizon (direction de vue quasi horizontale).
    pub horizon_color: [f32; 3],
    /// Couleur du ciel au zénith (direction de vue vers le haut).
    pub zenith_color: [f32; 3],
    /// Couleur vers laquelle le brouillard mélange les objets lointains.
    pub fog_color: [f32; 3],
    /// Densité du brouillard exponentiel (0 = désactivé). Le facteur de mélange est
    /// `1 - exp(-distance * fog_density)` — cf. `main.wgsl`.
    pub fog_density: f32,
    /// Intensité du bloom (0 = désactivé) : halo ajouté aux zones dont la
    /// radiance HDR dépasse 1.0 (émissifs, spéculaire fort). L'opt-out mobile ne
    /// touche pas ce champ — cf. `RenderQuality::bloom_enabled`, qui coupe
    /// l'application du réglage (et les passes GPU correspondantes) sans changer la
    /// scène elle-même.
    #[serde(default = "default_bloom_intensity")]
    pub bloom_intensity: f32,
}

fn default_bloom_intensity() -> f32 {
    0.6
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            horizon_color: [0.07, 0.08, 0.1],
            zenith_color: [0.07, 0.08, 0.1],
            fog_color: [0.07, 0.08, 0.1],
            fog_density: 0.0,
            bloom_intensity: default_bloom_intensity(),
        }
    }
}

/// Point de vue de jeu (mêmes paramètres que la caméra orbitale), appliqué en Play.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct GameCamera {
    pub target: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
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
pub(super) fn demo_obj(name: &str, mesh: MeshKind, pos: Vec3) -> SceneObject {
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

/// Marqueurs franchis entre deux temps de lecture d'un clip de durée `duration`,
/// rebouclé exactement comme `Clip::sample_joint` (`rem_euclid`). Gère le
/// passage du bouclage : un marqueur proche de la fin (ex. 0.95 s d'un clip de 1 s)
/// n'est pas manqué même si la lecture vient de reboucler à 0 sur ce même pas — sans
/// ce cas, un pas qui traverse la fin du clip (`prev_time` proche de `duration`,
/// `cur_time` proche de 0 après `rem_euclid`) donnerait `cur < prev` et l'intervalle
/// naïf `[prev, cur)` ne contiendrait jamais rien.
pub fn notifies_crossed(
    markers: &[(f32, String)],
    prev_time: f32,
    cur_time: f32,
    duration: f32,
) -> Vec<String> {
    if duration <= 0.0 || markers.is_empty() {
        return Vec::new();
    }
    let prev = prev_time.rem_euclid(duration);
    let cur = cur_time.rem_euclid(duration);
    if prev_time == cur_time {
        return Vec::new(); // temps figé (vitesse nulle, pause) : rien à franchir.
    }
    let mut hit = Vec::new();
    if cur >= prev {
        for (t, name) in markers {
            if *t >= prev && *t < cur {
                hit.push(name.clone());
            }
        }
    } else {
        // Le pas a traversé la fin du clip (bouclage) : deux tronçons, [prev,
        // duration) puis [0, cur).
        for (t, name) in markers {
            if *t >= prev || *t < cur {
                hit.push(name.clone());
            }
        }
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Intégration bout en bout : un `SceneObject.animation` fait bouger un
    /// mesh skinné à travers `Renderer::render_scene_headless`, pas seulement les briques
    /// isolées (déjà testées ailleurs : `Clip::sample_joint`, `ImportedMesh::load_skinning`,
    /// `skinned_mesh_data`, le pipeline GPU via `tests/golden_skinning.rs`). Sauté (pas en
    /// échec) sans GPU headless — même raison que `tests/golden_render.rs` (CI Linux sans
    /// GPU).
    #[test]
    fn scene_object_animation_moves_a_skinned_mesh_through_the_full_render_path() {
        let bytes = import::tests::animated_skinned_glb();
        let path = import::tests::write_temp_glb(&bytes, "scene_object_animation_integration");

        let render_at = |time: f32| -> Option<Vec<u8>> {
            let mut renderer =
                match pollster::block_on(crate::gfx::renderer::Renderer::new_headless(64, 64)) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "scene_object_animation_integration : pas de GPU headless ({e}) \
                             — test sauté."
                        );
                        return None;
                    }
                };
            let mut app = crate::app::AppState::default();
            app.scene.light.ambient = 0.4;
            let (data, aabb_min, aabb_max) =
                import::load_gltf(path.to_str().unwrap()).expect("glTF de test valide");
            let mut imported = ImportedMesh {
                path: path.to_str().unwrap().to_string(),
                data,
                aabb_min,
                aabb_max,
                ..Default::default()
            };
            imported.load_skinning();
            let clip_name = imported.clips[0].name.clone();
            app.scene.imported.push(imported);
            app.scene.objects.push(SceneObject {
                mesh: MeshKind::Imported(0),
                transform: Transform::default(),
                color: [0.9, 0.5, 0.2],
                animation: Some(AnimationState {
                    clip: clip_name,
                    time,
                    speed: 1.0,
                    ..Default::default()
                }),
                ..Default::default()
            });
            Some(renderer.render_scene_headless(&mut app, 64, 64))
        };

        let (Some(at_0), Some(at_1)) = (render_at(0.0), render_at(1.0)) else {
            return; // pas de GPU : rien à comparer (message déjà expliqué ci-dessus)
        };
        let _ = std::fs::remove_file(&path);

        assert_eq!(at_0.len(), at_1.len());
        let differing = at_0
            .iter()
            .zip(&at_1)
            .filter(|(a, b)| a.abs_diff(**b) > 8)
            .count();
        assert!(
            differing > 0,
            "l'image à t=0 et t=1 est identique : l'animation du joint (translation \
             linéaire testée séparément dans import::tests) ne semble pas atteindre le \
             rendu — la chaîne SceneObject → prepare_skinned_draws → shader est cassée \
             quelque part"
        );
    }

    /// Intégration bout en bout du fondu enchaîné : un `SceneObject` en
    /// pleine transition (`blend` intermédiaire, `prev_clip` renseigné) doit produire un
    /// rendu **différent** de la pose de liaison pure et du clip cible pur — preuve que
    /// `prepare_skinned_draws` prend bien la branche mélangée (`compute_joint_matrices_blended`)
    /// à travers le rendu réel, pas seulement testée isolément côté CPU
    /// (`blended_joint_matrices_*` dans `import::tests`). `prev_clip` pointe vers un nom
    /// de clip inexistant : `find_clip` retombe sur la pose de liaison pour ce côté du
    /// mélange, un cas valide (transition depuis un état non animé) et pratique à
    /// construire sans fixture à deux clips.
    #[test]
    fn scene_object_crossfade_renders_differently_from_either_pure_endpoint() {
        let bytes = import::tests::animated_skinned_glb();
        let path = import::tests::write_temp_glb(&bytes, "scene_object_crossfade_integration");

        let render_with = |anim: AnimationState| -> Option<Vec<u8>> {
            let mut renderer =
                match pollster::block_on(crate::gfx::renderer::Renderer::new_headless(64, 64)) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "scene_object_crossfade_integration : pas de GPU headless ({e}) \
                             — test sauté."
                        );
                        return None;
                    }
                };
            let mut app = crate::app::AppState::default();
            app.scene.light.ambient = 0.4;
            let (data, aabb_min, aabb_max) =
                import::load_gltf(path.to_str().unwrap()).expect("glTF de test valide");
            let mut imported = ImportedMesh {
                path: path.to_str().unwrap().to_string(),
                data,
                aabb_min,
                aabb_max,
                ..Default::default()
            };
            imported.load_skinning();
            app.scene.imported.push(imported);
            app.scene.objects.push(SceneObject {
                mesh: MeshKind::Imported(0),
                transform: Transform::default(),
                color: [0.9, 0.5, 0.2],
                animation: Some(anim),
                ..Default::default()
            });
            Some(renderer.render_scene_headless(&mut app, 64, 64))
        };

        let clip_name = {
            let mut m = ImportedMesh {
                path: path.to_str().unwrap().to_string(),
                ..Default::default()
            };
            m.load_skinning();
            m.clips[0].name.clone()
        };

        let base = AnimationState {
            clip: clip_name,
            time: 1.0, // clip cible pur : à t=1.0, translation (10,0,0) de la fixture
            speed: 1.0,
            prev_clip: "PoseDeLiaisonInexistante".into(), // cf. doc du test
            prev_time: 0.0,
            blend: 1.0,
        };
        let Some(pure_target) = render_with(base.clone()) else {
            return; // pas de GPU
        };
        let mut mid = base.clone();
        mid.blend = 0.5;
        let Some(mid_transition) = render_with(mid) else {
            return;
        };
        let mut pure_bind = base;
        pure_bind.blend = 0.0;
        let Some(pure_bind_pose) = render_with(pure_bind) else {
            return;
        };
        let _ = std::fs::remove_file(&path);

        // Comparaison volontairement limitée à « mi-transition vs pose de liaison pure » :
        // à blend=1.0 la translation du clip (10 unités sur X de la fixture) pousse
        // l'objet hors du petit cadre 64×64 de ce test, rendant blend=0.5 et blend=1.0
        // visuellement indiscernables (les deux hors champ) bien que les matrices de
        // joints diffèrent réellement entre les deux (déjà prouvé au niveau CPU par
        // `blended_joint_matrices_at_midpoint_interpolate_translation`). La pose de
        // liaison, elle, reste à l'origine — toujours dans le cadre, comparaison fiable.
        let differs = |a: &[u8], b: &[u8]| a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 8);
        assert!(
            differs(&mid_transition, &pure_bind_pose),
            "à mi-transition, le rendu ne doit pas être identique à la pose de liaison pure \
             — le fondu ne semble pas atteindre le rendu réel"
        );
        assert!(
            differs(&pure_target, &pure_bind_pose),
            "précondition : le clip cible pur doit lui-même différer de la pose de liaison \
             (sinon toute cette comparaison serait vide de sens)"
        );
    }

    #[test]
    fn imported_mesh_load_skinning_populates_skeleton_clips_and_vertex_skins() {
        // Réutilise la fixture .glb existante (`import::tests`) plutôt que d'en
        // reconstruire une : elle est déjà vérifiée correcte, seule la *plomberie*
        // `ImportedMesh::load_skinning` est testée ici.
        let path = import::tests::write_temp_glb(
            &import::tests::skinned_triangle_glb(),
            "scene_load_skinning",
        );
        let mut m = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            ..Default::default()
        };
        m.load_skinning();
        let _ = std::fs::remove_file(&path);

        let skeleton = m.skeleton.expect("la fixture a un skin");
        assert_eq!(skeleton.joints.len(), 2);
        assert_eq!(
            m.vertex_skins.len(),
            3,
            "un VertexSkin par sommet du triangle"
        );
        // Cette fixture n'a pas de bloc "animations" : pas de clip, mais pas d'erreur
        // non plus (skin sans animation = squelette utilisable en pose de liaison seule).
        assert!(m.clips.is_empty());
    }

    #[test]
    fn imported_mesh_load_skinning_leaves_a_static_mesh_untouched() {
        let path = import::tests::write_temp_glb(
            &import::tests::unskinned_triangle_glb(),
            "scene_load_skinning_static",
        );
        let mut m = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            ..Default::default()
        };
        m.load_skinning();
        let _ = std::fs::remove_file(&path);

        assert!(m.skeleton.is_none());
        assert!(m.clips.is_empty());
        assert!(m.vertex_skins.is_empty());
    }

    #[test]
    fn skinned_mesh_data_combines_geometry_and_skin_weights() {
        let bytes = import::tests::skinned_triangle_glb();
        let path = import::tests::write_temp_glb(&bytes, "scene_skinned_mesh_data");
        let (data, aabb_min, aabb_max) = import::load_gltf(path.to_str().unwrap()).unwrap();
        let mut m = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        m.load_skinning();
        let _ = std::fs::remove_file(&path);

        let skinned = m.skinned_mesh_data().expect("mesh skinné : Some attendu");
        assert_eq!(skinned.vertices.len(), 3);
        assert_eq!(skinned.indices, m.data.indices);
        // Sommet 2 de la fixture : joints [0,1,0,0], poids [0.5,0.5,0,0].
        assert_eq!(skinned.vertices[2].joints, [0, 1, 0, 0]);
        assert_eq!(skinned.vertices[2].weights, [0.5, 0.5, 0.0, 0.0]);
        // Géométrie transportée telle quelle depuis `data.vertices`.
        assert_eq!(skinned.vertices[0].position, m.data.vertices[0].position);
    }

    #[test]
    fn load_skinning_also_populates_tangents_for_any_imported_mesh() {
        // Contrairement au squelette (skin glTF requis), les tangentes
        // sont calculées pour n'importe quel mesh importé — vérifié ici sur la même
        // fixture skinnée que `skinned_mesh_data_combines_geometry_and_skin_weights`,
        // mais rien dans `compute_tangents` ne dépend du skin.
        let bytes = import::tests::skinned_triangle_glb();
        let path = import::tests::write_temp_glb(&bytes, "scene_load_skinning_tangents");
        let (data, aabb_min, aabb_max) = import::load_gltf(path.to_str().unwrap()).unwrap();
        let mut m = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        m.load_skinning();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            m.tangents.len(),
            m.data.vertices.len(),
            "une tangente par sommet"
        );
        for t in &m.tangents {
            assert!(t[0].is_finite() && t[1].is_finite() && t[2].is_finite());
            assert!(
                t[3] == 1.0 || t[3] == -1.0,
                "signe de bitangente : {}",
                t[3]
            );
        }
    }

    #[test]
    fn skinned_mesh_data_is_none_for_a_static_mesh() {
        let bytes = import::tests::unskinned_triangle_glb();
        let path = import::tests::write_temp_glb(&bytes, "scene_skinned_mesh_data_static");
        let (data, aabb_min, aabb_max) = import::load_gltf(path.to_str().unwrap()).unwrap();
        let mut m = ImportedMesh {
            path: path.to_str().unwrap().to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        m.load_skinning();
        let _ = std::fs::remove_file(&path);

        assert!(m.skinned_mesh_data().is_none());
    }

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
    fn default_clip_prefers_idle_then_first() {
        let mut m = ImportedMesh {
            clips: vec![
                import::Clip::without_tracks("Walk", 1.0),
                import::Clip::without_tracks("Idle", 2.0),
            ],
            ..Default::default()
        };
        assert_eq!(m.default_clip(), Some("Idle"));
        m.clips.remove(1);
        assert_eq!(m.default_clip(), Some("Walk"));
        m.clips.clear();
        assert_eq!(m.default_clip(), None, "mesh statique : rien à jouer");
    }

    #[test]
    fn ensure_default_animations_fills_only_missing_states() {
        let mut scene = Scene::demo(); // Sol/Cube/Sphère : meshes builtin, jamais touchés
        scene.imported.push(ImportedMesh {
            clips: vec![import::Clip::without_tracks("Idle", 2.0)],
            ..Default::default()
        });
        // Un GLB riggé sans état (le bug : T-pose pour toujours) et un autre dont le
        // clip a déjà été choisi (par une démo ou un script) qui doit rester intact.
        scene.objects.push(SceneObject {
            mesh: MeshKind::Imported(0),
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            mesh: MeshKind::Imported(0),
            animation: Some(AnimationState {
                clip: "Walk".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        scene.ensure_default_animations();
        assert!(
            scene.objects[..3].iter().all(|o| o.animation.is_none()),
            "les meshes builtin ne reçoivent pas d'état d'animation"
        );
        assert_eq!(scene.objects[3].animation.as_ref().unwrap().clip, "Idle");
        assert_eq!(
            scene.objects[4].animation.as_ref().unwrap().clip,
            "Walk",
            "un état existant n'est jamais écrasé"
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
    fn a_legacy_json_file_loads_at_the_current_version() {
        // Une scène sans champ `version` du tout (fichier antérieur à l'introduction de
        // ce champ) doit ressortir de `Scene::load` au numéro courant, migrations
        // appliquées.
        let json = r#"{"objects":[],"groups":["A","A","B"]}"#;
        let path = std::env::temp_dir().join(format!(
            "rusteegear_legacy_scene_test_{}.json",
            std::process::id()
        ));
        std::fs::write(&path, json).unwrap();
        let scene = Scene::load(path.to_str().unwrap()).unwrap();
        assert_eq!(scene.version, Scene::CURRENT_VERSION);
        assert_eq!(
            scene.groups,
            vec!["A".to_string(), "B".to_string()],
            "la migration doit dédoublonner les groupes d'une scène legacy (version 0)"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_scene_already_at_the_current_version_is_left_untouched_by_migrate() {
        // `migrate` ne doit rien changer à une scène déjà à jour, même avec des
        // doublons de groupe (probablement volontaires, recréés à la main par
        // l'utilisateur) : le nettoyage n'est appliqué qu'à `version == 0`.
        let mut scene = Scene {
            groups: vec!["A".into(), "A".into()],
            version: Scene::CURRENT_VERSION,
            ..Default::default()
        };
        scene.migrate();
        assert_eq!(scene.groups, vec!["A".to_string(), "A".to_string()]);
        assert_eq!(scene.version, Scene::CURRENT_VERSION);
    }

    /// Sprint 131 : migration v1 → v2, la première migration réelle de ce projet (pas
    /// juste un champ absent comblé par `#[serde(default)]`) — une scène `version < 2`
    /// avec `roughness: 0.0` (valeur explicitement présente dans le JSON, possible
    /// avant que l'inspecteur n'impose un plancher de 0,04) doit être relevée au
    /// plancher par `migrate()`.
    #[test]
    fn migrate_v1_to_v2_raises_zero_roughness_to_the_inspector_floor() {
        let mut scene = Scene {
            objects: vec![SceneObject {
                roughness: 0.0,
                ..Default::default()
            }],
            version: 1,
            ..Default::default()
        };
        scene.migrate();
        assert_eq!(scene.objects[0].roughness, 0.04);
        assert_eq!(scene.version, Scene::CURRENT_VERSION);
    }

    /// La migration ne doit pas toucher une valeur de roughness déjà au-dessus du
    /// plancher (pas une correction générale, juste un relevage du plancher minimal),
    /// ni une scène déjà à `CURRENT_VERSION` (même logique que le test de dédoublonnage
    /// des groupes ci-dessus — les migrations sont gardées par version, pas rejouées).
    #[test]
    fn migrate_v1_to_v2_leaves_valid_roughness_and_up_to_date_scenes_untouched() {
        let mut scene = Scene {
            objects: vec![SceneObject {
                roughness: 0.6,
                ..Default::default()
            }],
            version: 1,
            ..Default::default()
        };
        scene.migrate();
        assert_eq!(scene.objects[0].roughness, 0.6);

        let mut already_current = Scene {
            objects: vec![SceneObject {
                roughness: 0.0,
                ..Default::default()
            }],
            version: Scene::CURRENT_VERSION,
            ..Default::default()
        };
        already_current.migrate();
        assert_eq!(
            already_current.objects[0].roughness, 0.0,
            "une scène déjà à jour n'est pas re-corrigée, même avec une valeur \
             qu'une scène plus ancienne aurait fait migrer"
        );
    }

    /// Dossier temporaire unique par test (même schéma que `assets::tests::
    /// temp_assets_dir`) : `Scene::save_prefab_at`/`instantiate_prefab_at`/
    /// `sync_prefab_instances_at` ne touchent plus `~/.motor3derust/assets/` réel
    /// depuis ce complément — auparavant ces tests y écrivaient réellement (comme le
    /// ferait l'éditeur), faute d'une variante testable par répertoire séparé.
    fn temp_prefabs_dir(tag: &str) -> std::path::PathBuf {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        tag.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        let dir =
            std::env::temp_dir().join(format!("rusteegear_prefab_test_{:x}", hasher.finish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn modifying_a_prefab_updates_its_instances_except_overrides() {
        // Un prefab « gemme » modifié met à jour ses N instances, sauf les propriétés
        // surchargées.
        let dir = temp_prefabs_dir("gemme");
        let gemme_v1 = SceneObject {
            name: "Gemme".into(),
            mesh: MeshKind::Sphere,
            color: [1.0, 1.0, 0.0], // jaune
            tap_action: TapAction::Hide,
            ..Default::default()
        };
        let asset_id = Scene::save_prefab_at(
            &dir,
            &gemme_v1,
            "Gemme",
            &crate::assets::PrefabScope::General,
        )
        .expect("sauvegarde du prefab impossible");

        // 20 instances, chacune à sa propre position (transform/name surchargés par
        // défaut par `instantiate_prefab_at`).
        let mut scene = Scene::default();
        for i in 0..20 {
            let obj = Scene::instantiate_prefab_at(
                &dir,
                &asset_id,
                format!("Gemme {i}"),
                Vec3::new(i as f32, 0.0, 0.0),
            )
            .expect("instanciation impossible");
            scene.objects.push(obj);
        }

        // L'utilisateur retouche la couleur d'une seule instance (#5) à la main : ce
        // champ devient une surcharge, protégée des futures resynchronisations.
        scene.objects[5].color = [1.0, 0.0, 0.0]; // rouge
        scene.objects[5]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .push("color".to_string());

        // Le prefab change de couleur (verte) — sauvegardé sous le même nom/asset_id.
        let gemme_v2 = SceneObject {
            color: [0.0, 1.0, 0.0],
            ..gemme_v1
        };
        Scene::save_prefab_at(
            &dir,
            &gemme_v2,
            "Gemme",
            &crate::assets::PrefabScope::General,
        )
        .unwrap();
        scene.sync_prefab_instances_at(&dir);

        for (i, obj) in scene.objects.iter().enumerate() {
            if i == 5 {
                assert_eq!(
                    obj.color,
                    [1.0, 0.0, 0.0],
                    "l'instance surchargée garde sa couleur"
                );
            } else {
                assert_eq!(
                    obj.color,
                    [0.0, 1.0, 0.0],
                    "l'instance {i} doit suivre la nouvelle couleur du prefab"
                );
            }
            // `transform`/`name` restent propres à chaque instance (surchargés par
            // défaut), jamais écrasés par la resynchronisation.
            assert_eq!(obj.transform.position, Vec3::new(i as f32, 0.0, 0.0));
            assert_eq!(obj.name, format!("Gemme {i}"));
            assert!(
                obj.mesh == MeshKind::Sphere,
                "le mesh doit suivre le template"
            );
            assert_eq!(obj.tap_action, TapAction::Hide);
        }
    }

    #[test]
    fn sync_prefab_instances_leaves_non_prefab_objects_untouched() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Solo".into(),
            color: [0.3, 0.3, 0.3],
            ..Default::default()
        });
        scene.sync_prefab_instances();
        assert_eq!(scene.objects[0].name, "Solo");
        assert_eq!(scene.objects[0].color, [0.3, 0.3, 0.3]);
    }

    #[test]
    fn sync_prefab_instances_is_a_no_op_when_the_prefab_file_is_missing() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Orpheline".into(),
            prefab: Some(PrefabInstance {
                asset_id: "asset-id://inconnu".into(),
                overrides: vec![],
            }),
            ..Default::default()
        });
        // Ne doit pas paniquer, et laisser l'objet inchangé (prefab introuvable).
        scene.sync_prefab_instances();
        assert_eq!(scene.objects[0].name, "Orpheline");
    }

    #[test]
    fn notifies_crossed_detects_a_marker_within_a_simple_forward_step() {
        let markers = vec![(0.5, "hit".to_string())];
        let hit = notifies_crossed(&markers, 0.4, 0.6, 1.0);
        assert_eq!(hit, vec!["hit".to_string()]);
    }

    #[test]
    fn notifies_crossed_ignores_a_marker_outside_the_step() {
        let markers = vec![(0.5, "hit".to_string())];
        assert!(notifies_crossed(&markers, 0.6, 0.8, 1.0).is_empty());
    }

    #[test]
    fn notifies_crossed_handles_the_wraparound_at_the_end_of_the_clip() {
        // Le pas traverse la fin du clip (0.95 -> 0.05 après rebouclage) : un marqueur
        // proche de la fin (0.97) doit être détecté malgré `cur < prev`.
        let markers = vec![(0.97, "fin".to_string())];
        let hit = notifies_crossed(&markers, 0.95, 1.05, 1.0);
        assert_eq!(hit, vec!["fin".to_string()]);
    }

    #[test]
    fn notifies_crossed_is_empty_when_time_is_frozen() {
        // Vitesse nulle (pause, `AnimationState::speed == 0`) : rien ne doit se
        // déclencher en boucle à chaque tick sous prétexte que `prev == cur`.
        let markers = vec![(0.5, "hit".to_string())];
        assert!(notifies_crossed(&markers, 0.5, 0.5, 1.0).is_empty());
    }

    #[test]
    fn notifies_crossed_is_empty_for_a_zero_duration_clip() {
        let markers = vec![(0.0, "hit".to_string())];
        assert!(notifies_crossed(&markers, 0.0, 0.1, 0.0).is_empty());
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

    /// Sprint 126 : `asset_references` indexe les 4 champs qui peuvent porter une
    /// référence `asset-id://` stable (texture, audio, mesh importé, image HUD) et
    /// ignore les chemins qui n'ont pas ce schéma (`asset://`/`bundle://` bruts,
    /// aucune identité stable à indexer) — cf. sa doc.
    #[test]
    fn asset_references_indexes_all_four_reference_fields_by_uuid() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Caisse".into(),
            texture: "asset-id://tex-uuid".into(),
            audio: Some(AudioSource {
                clip: "asset-id://audio-uuid".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        // Chemin `asset://` brut : pas de schéma `asset-id://`, ne doit apparaître
        // dans aucune entrée (aucune identité stable à indexer).
        scene.objects.push(SceneObject {
            name: "Sans référence stable".into(),
            texture: "asset://old_style.png".into(),
            ..Default::default()
        });
        scene.imported.push(ImportedMesh {
            name: "Robot".into(),
            path: "asset-id://mesh-uuid".into(),
            ..Default::default()
        });
        scene.hud_widgets.push(HudWidget {
            id: "icone_vie".into(),
            anchor: HudAnchor::TopLeft,
            offset: [0.0, 0.0],
            size: [32.0, 32.0],
            kind: HudWidgetKind::Image {
                path: "asset-id://hud-uuid".into(),
            },
        });

        let refs = scene.asset_references();
        assert_eq!(refs.len(), 4, "un uuid par référence stable, pas plus");
        assert!(refs["tex-uuid"][0].contains("Caisse"));
        assert!(refs["audio-uuid"][0].contains("Caisse"));
        assert!(refs["mesh-uuid"][0].contains("Robot"));
        assert!(refs["hud-uuid"][0].contains("icone_vie"));
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
            ..Default::default()
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
    fn mmorpg_demo_has_a_visible_wind_zone_with_no_collider() {
        // Sprint 125, preuve d'implémentation : une zone de vent jouable, pas
        // seulement testée en isolation dans `runtime::physics`.
        let s = Scene::mmorpg_demo();
        let vent = s
            .objects
            .iter()
            .find(|o| o.name == "Zone de vent")
            .expect("une zone de vent dans la démo MMORPG");
        assert!(
            vent.trigger,
            "sans trigger, la zone de vent n'a aucun volume de détection"
        );
        assert!(vent.wind.is_some(), "wind doit être renseigné");
        assert_eq!(
            vent.physics,
            crate::runtime::physics::PhysicsKind::None,
            "une zone de vent ne doit rien bloquer physiquement"
        );
        assert!(
            vent.visible,
            "doit être visible à l'écran, pas un objet caché"
        );
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

    #[test]
    fn hud_anchor_fraction_matches_the_named_corner() {
        assert_eq!(HudAnchor::TopLeft.fraction(), (0.0, 0.0));
        assert_eq!(HudAnchor::TopRight.fraction(), (1.0, 0.0));
        assert_eq!(HudAnchor::BottomLeft.fraction(), (0.0, 1.0));
        assert_eq!(HudAnchor::BottomRight.fraction(), (1.0, 1.0));
        assert_eq!(HudAnchor::Center.fraction(), (0.5, 0.5));
    }

    #[test]
    fn hud_widget_round_trips_through_json_with_its_kind_and_binding_intact() {
        let widgets = vec![
            HudWidget {
                id: "score_label".into(),
                anchor: HudAnchor::TopRight,
                offset: [-10.0, 10.0],
                size: [0.0, 0.0],
                kind: HudWidgetKind::Text {
                    content: "Score".into(),
                    binding: HudBinding::Score,
                },
            },
            HudWidget {
                id: "health_gauge".into(),
                anchor: HudAnchor::BottomLeft,
                offset: [10.0, -10.0],
                size: [160.0, 18.0],
                kind: HudWidgetKind::Gauge {
                    binding: HudBinding::Health,
                    max: 1.0,
                    color: [0.8, 0.15, 0.15],
                },
            },
            HudWidget {
                id: "restart_btn".into(),
                anchor: HudAnchor::Center,
                offset: [0.0, 0.0],
                size: [140.0, 36.0],
                kind: HudWidgetKind::Button {
                    label: "Recommencer".into(),
                    action: "restart".into(),
                },
            },
        ];
        let scene = Scene {
            hud_widgets: widgets.clone(),
            ..Default::default()
        };

        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();

        assert_eq!(back.hud_widgets, widgets);
    }

    #[test]
    fn scene_without_hud_widgets_field_deserializes_to_an_empty_vec() {
        // Scène pré-Sprint 109 (JSON antérieur, sans le champ) : ne doit pas échouer
        // à charger, cf. `#[serde(default)]` sur `Scene::hud_widgets`.
        let legacy = r#"{"objects": []}"#;
        let scene: Scene = serde_json::from_str(legacy).unwrap();
        assert!(scene.hud_widgets.is_empty());
    }

    /// Garde-fou compagnon de `the_embedded_scene_ships_monsters_and_the_fire_button`
    /// (`app::fireball`) : chaque mesh `bundle://` référencé par la scène embarquée
    /// doit se résoudre **réellement** (clé présente dans `assets/bundle/`, inclus à
    /// la compilation) — les deux créatures Blender comprises, squelette inclus. Une
    /// clé manquante ne serait sinon qu'un `log::error!` silencieux au chargement :
    /// la créature apparaîtrait comme un mesh vide invisible dans le jeu exporté.
    /// OUTIL, pas une preuve (lancé explicitement :
    /// `cargo test sync_embedded_scene_creatures_from_the_demo -- --ignored --nocapture`) :
    /// resynchronise les créatures de `assets/player_scene.json` (la scène
    /// embarquée) depuis `Scene::mmorpg_demo()`, la source de vérité — objets
    /// « Créature* » remplacés, imports réécrits en `bundle://m{i}_<fichier>`
    /// (même ordre d'indices), tag « joueur » posé. Remplace les fusions JSON
    /// à la main qui ont déjà perdu 3 fois le contenu multijoueur de cette
    /// scène (cf. le garde-fou `the_embedded_scene_creatures_match_the_demo`).
    /// Tout le reste du fichier (monstres, tour, boutons) est préservé tel quel.
    #[test]
    #[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
    fn sync_embedded_scene_creatures_from_the_demo() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
        let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
        let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
        let demo = Scene::mmorpg_demo();

        embedded.objects.retain(|o| !o.name.starts_with("Créature"));
        for obj in demo
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
        {
            embedded.objects.push(obj.clone());
        }
        // Seuls les imports des **créatures** relèvent de cet outil : elles sont
        // ajoutées en premier dans `Scene::mmorpg_demo` (avant le décor nature,
        // Sprint parallèle), donc contiguës en tête de `demo.imported` — ne
        // reconstruit que ces N premières entrées (bundle://m{i}_<fichier>,
        // même convention que `editor::export::bundle_scene_json`), et
        // préserve tel quel le reste d'`embedded.imported` (décor déjà
        // embarqué séparément, hors du périmètre de cet outil).
        let n_creatures = demo
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
            .count();
        let mut imported: Vec<ImportedMesh> = demo
            .imported
            .iter()
            .take(n_creatures)
            .enumerate()
            .map(|(i, m)| {
                let file = std::path::Path::new(&m.path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .expect("nom de fichier d'import");
                ImportedMesh {
                    path: format!("{}m{i}_{file}", crate::assets::SCHEME),
                    ..Default::default()
                }
            })
            .collect();
        imported.extend(embedded.imported.into_iter().skip(n_creatures));
        embedded.imported = imported;
        if let Some(joueur) = embedded.objects.iter_mut().find(|o| o.name == "Joueur") {
            joueur.tag = "joueur".into();
        }

        std::fs::write(
            path,
            serde_json::to_string_pretty(&embedded).expect("sérialisation"),
        )
        .expect("écriture de player_scene.json");
        println!(
            "player_scene.json resynchronisé : {} créatures, {} imports",
            embedded
                .objects
                .iter()
                .filter(|o| o.name.starts_with("Créature"))
                .count(),
            embedded.imported.len()
        );
    }

    /// Outil (portée plus large que `sync_embedded_scene_creatures_from_the_demo`
    /// ci-dessus) : remplace tout l'environnement de la scène embarquée
    /// (`assets/player_scene.json`) par `Scene::hameau_gdd_demo()` (le nouveau
    /// hameau fortifié, cf. la doc de cette fonction) sans jamais toucher les
    /// champs listés dans la consigne d'intégration : `mobile`, `hud_layout`,
    /// `hud_widgets`, `point_lights`, `camera_follow`, `game_camera`, `sky`,
    /// `version`, et l'objet « Joueur » (mesh riggé `fairy_hero` +
    /// `fire_button`/`weapon_button`/`heal_button`). Les imports sont réécrits
    /// en `bundle://m{i}_<fichier>` (même convention que
    /// `editor::export::bundle_scene_json`) — ne compresse rien lui-même, ne
    /// fait que réécrire des chemins ; chaque fichier référencé doit déjà
    /// exister dans `assets/bundle/` (vrai pour tous les modèles cités par la
    /// spec du hameau au moment de l'intégration).
    #[test]
    #[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
    fn sync_embedded_scene_hameau_from_the_demo() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
        let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
        let embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
        let demo = Scene::hameau_gdd_demo();

        let mut joueur = embedded
            .objects
            .iter()
            .find(|o| o.name == "Joueur")
            .cloned()
            .expect("« Joueur » doit exister dans la scène embarquée actuelle");
        let joueur_mesh_key = match joueur.mesh {
            MeshKind::Imported(i) => embedded.imported.get(i as usize).map(|m| m.path.clone()),
            _ => None,
        };

        let mut objects: Vec<SceneObject> = demo
            .objects
            .into_iter()
            .filter(|o| o.name != "Joueur")
            .collect();
        let mut imported = demo.imported;
        if let Some(key) = joueur_mesh_key {
            let idx = match imported.iter().position(|m: &ImportedMesh| m.path == key) {
                Some(i) => i,
                None => {
                    imported.push(ImportedMesh {
                        path: key,
                        ..Default::default()
                    });
                    imported.len() - 1
                }
            };
            joueur.mesh = MeshKind::Imported(idx as u32);
        }
        objects.push(joueur);

        // Un chemin déjà `bundle://mNN_<fichier>` (cas du mesh du joueur,
        // repris tel quel de la scène embarquée actuelle) doit perdre son
        // ancien préfixe numérique avant d'en recevoir un nouveau — sinon la
        // clé réécrite (`mNN_m126_fairy_hero.glb`) ne correspondrait plus au
        // fichier réellement présent dans `assets/bundle/`.
        fn clean_file_name(path: &str) -> String {
            let file = std::path::Path::new(path)
                .file_name()
                .and_then(|f| f.to_str())
                .expect("nom de fichier d'import")
                .to_string();
            if let Some(rest) = file.strip_prefix('m')
                && let Some(us) = rest.find('_')
                && rest[..us].chars().all(|c| c.is_ascii_digit())
            {
                return rest[us + 1..].to_string();
            }
            file
        }
        let imported: Vec<ImportedMesh> = imported
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                let file = clean_file_name(&m.path);
                ImportedMesh {
                    path: format!("{}m{i}_{file}", crate::assets::SCHEME),
                    ..Default::default()
                }
            })
            .collect();

        let merged = Scene {
            objects,
            imported,
            groups: embedded.groups,
            light: demo.light,
            point_lights: embedded.point_lights,
            mobile: embedded.mobile,
            camera_follow: embedded.camera_follow,
            game_camera: embedded.game_camera,
            // Ciel du hameau fortifié : nuit bleutée avec brouillard léger,
            // conforme à GDD_MMORPG.md §2.3/§10 ("féerique crépusculaire").
            // L'ancien ciel embarqué (hérité de `mmorpg_demo`) était une
            // palette de plein jour — incohérente avec la fiction et avec
            // `Sky::default()` qui, lui, est déjà nocturne.
            sky: Sky {
                horizon_color: [0.10, 0.11, 0.20],
                zenith_color: [0.04, 0.05, 0.12],
                fog_color: [0.09, 0.10, 0.16],
                fog_density: 0.02,
                bloom_intensity: 0.9,
            },
            version: embedded.version,
            hud_layout: embedded.hud_layout,
            hud_widgets: embedded.hud_widgets,
        };

        std::fs::write(
            path,
            serde_json::to_string_pretty(&merged).expect("sérialisation"),
        )
        .expect("écriture de player_scene.json");
        println!(
            "player_scene.json remplacé par le hameau fortifié : {} objets, {} imports",
            merged.objects.len(),
            merged.imported.len()
        );
    }

    /// Garde-fou compagnon de `sync_embedded_scene_creatures_from_the_demo` :
    /// les créatures de la scène embarquée doivent rester **identiques** à
    /// celles de `Scene::mmorpg_demo`
    /// (script, collisions, trigger, mesh, physique) — c'est la démo qui est la
    /// source de vérité, la scène embarquée n'en est qu'une copie avec des
    /// chemins `bundle://`. Une divergence = quelqu'un a modifié une créature
    /// d'un seul côté : relancer l'outil de synchronisation.
    #[test]
    fn the_embedded_scene_creatures_match_the_demo() {
        let embedded = Scene::embedded_player();
        let demo = Scene::mmorpg_demo();
        let demo_creatures: Vec<&SceneObject> = demo
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
            .collect();
        assert!(!demo_creatures.is_empty());
        for d in demo_creatures {
            let e = embedded
                .objects
                .iter()
                .find(|o| o.name == d.name)
                .unwrap_or_else(|| {
                    panic!(
                        "« {} » absente de la scène embarquée — lancer `cargo test \
                         sync_embedded_scene_creatures_from_the_demo -- --ignored`",
                        d.name
                    )
                });
            let sync_hint = "désynchronisé de la démo — lancer `cargo test \
                 sync_embedded_scene_creatures_from_the_demo -- --ignored`";
            assert_eq!(e.script, d.script, "script de « {} » {sync_hint}", d.name);
            assert_eq!(
                e.trigger, d.trigger,
                "trigger de « {} » {sync_hint}",
                d.name
            );
            assert_eq!(
                e.collision_layer, d.collision_layer,
                "couche de « {} » {sync_hint}",
                d.name
            );
            assert!(e.mesh == d.mesh, "mesh de « {} » {sync_hint}", d.name);
            assert!(
                e.physics == d.physics,
                "physique de « {} » {sync_hint}",
                d.name
            );
        }
        // Imports : seuls ceux référencés par les créatures doivent correspondre
        // (même indice, même fichier — juste `bundle://` au lieu du chemin
        // disque). La démo peut porter d'autres imports (décor nature ajouté
        // par le chantier MMORPG) sans que la scène embarquée n'y soit tenue.
        for d in demo
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
        {
            let MeshKind::Imported(i) = d.mesh else {
                panic!("« {} » devrait être un mesh importé", d.name);
            };
            let demo_file = std::path::Path::new(&demo.imported[i as usize].path)
                .file_name()
                .and_then(|f| f.to_str())
                .expect("nom de fichier d'import démo");
            let embedded_path = &embedded
                .imported
                .get(i as usize)
                .unwrap_or_else(|| {
                    panic!("import {i} absent de la scène embarquée — lancer l'outil de sync")
                })
                .path;
            assert!(
                embedded_path.ends_with(demo_file),
                "import {i} : « {embedded_path} » devrait pointer le même fichier que la \
                 démo (« {demo_file} ») — lancer l'outil de synchronisation"
            );
        }
        assert_eq!(
            embedded
                .objects
                .iter()
                .find(|o| o.name == "Joueur")
                .map(|o| o.tag.as_str()),
            Some("joueur"),
            "le joueur embarqué doit porter le tag « joueur » (scripts des créatures 12/13)"
        );
    }

    /// Garde-fou du trou de synchro démo ↔ scène servie : l'authoring des vagues
    /// (`mmorpg_demo_waves_follow_the_gdd_authoring_rules`, cf. `demos.rs`) ne
    /// valide que `Scene::mmorpg_demo()`, alors que la scène réellement servie en
    /// ligne est `assets/player_scene.json` (embarquée via `embedded_player`,
    /// réécrite par `editor::export::bundle_scene_json` et les outils
    /// `sync_embedded_scene_*`). Sans ce test, on peut retoucher les vagues de la
    /// démo, garder tous les tests verts, et laisser la scène en ligne diverger.
    /// Comparaison **structurelle** (aucune constante en dur) : chaque créature
    /// attaquable doit porter la même manche (`Combat::wave`) et les mêmes PV
    /// (`Combat::hp`) des deux côtés, appariée par nom — plus précis qu'un simple
    /// multiset (wave, hp), qui laisserait passer un échange de stats entre deux
    /// créatures. Lit le JSON du disque (pas `embedded_player()`, dont le repli
    /// silencieux vers `Scene::demo()` masquerait un fichier corrompu).
    #[test]
    fn the_embedded_scene_waves_and_hp_match_the_demo() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
        let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
        let embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
        let demo = Scene::mmorpg_demo();

        // name → (wave, hp) des cibles attaquables d'une scène.
        fn combat_stats(scene: &Scene) -> std::collections::BTreeMap<&str, (u32, u32)> {
            scene
                .objects
                .iter()
                .filter_map(|o| {
                    let c = o.combat.as_ref()?;
                    c.attackable.then_some((o.name.as_str(), (c.wave, c.hp)))
                })
                .collect()
        }
        let demo_stats = combat_stats(&demo);
        let embedded_stats = combat_stats(&embedded);
        assert!(
            !demo_stats.is_empty(),
            "la démo MMORPG doit avoir des cibles attaquables"
        );
        let sync_hint = "démo et scène servie divergent — relancer `cargo test \
             sync_embedded_scene_creatures_from_the_demo -- --ignored` puis \
             recompiler (cf. embedded-scene-export-overwrite-trap)";
        assert_eq!(
            demo_stats.len(),
            embedded_stats.len(),
            "nombre de cibles attaquables : {sync_hint}"
        );
        for (name, &(wave, hp)) in &demo_stats {
            let &(e_wave, e_hp) = embedded_stats.get(name).unwrap_or_else(|| {
                panic!("« {name} » absente de la scène embarquée : {sync_hint}")
            });
            assert_eq!(
                (e_wave, e_hp),
                (wave, hp),
                "(wave, hp) de « {name} » : {sync_hint}"
            );
        }
    }

    #[test]
    fn the_embedded_scene_resolves_its_bundle_creatures() {
        let scene = Scene::embedded_player();
        for name in ["Créature", "Créature 2"] {
            assert!(
                scene.objects.iter().any(|o| o.name == name),
                "la scène embarquée doit contenir « {name} » (cf. assets/player_scene.json)"
            );
        }
        assert!(
            scene.imported.len() >= 2,
            "la scène embarquée doit référencer les deux glb de créatures \
             (imports trouvés : {})",
            scene.imported.len()
        );
        for m in &scene.imported {
            assert!(
                !m.data.vertices.is_empty(),
                "mesh embarqué « {} » non résolu (clé absente du bundle ?)",
                m.path
            );
        }
        // Seuls les imports référencés par les **créatures** (et le joueur riggé)
        // doivent être skinnés : depuis le décor village/nature embarqué
        // (cf. commits « Décor MMORPG »), la scène référence aussi des meshes
        // statiques (pont, cabane…) légitimement sans squelette.
        for o in scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature") || o.tag == "joueur")
        {
            let MeshKind::Imported(i) = o.mesh else {
                continue;
            };
            let m = &scene.imported[i as usize];
            assert!(
                m.skeleton.is_some(),
                "mesh embarqué « {} » (objet « {} ») doit être skinné (rig requis)",
                m.path,
                o.name
            );
        }
    }

    /// Préfixes de noms du décor ambiant intégré (faune 27-61 + flore/objets des
    /// packs Blender headless générés en 2026-07) : partagés entre l'outil de
    /// synchro et son garde-fou compagnon ci-dessous. Aucun ne recoupe
    /// « Créature » (combat, `MMORPG_CREATURES`) ni les préfixes déjà utilisés
    /// par le décor historique (`NATURE_DECOR`/`VILLAGE_PROPS`/`MONSTER_DECOR`) —
    /// ni, surtout, « Faune » : déjà utilisé par `hameau_gdd_demo()`
    /// (`Faune {n} {cluster}-{poses}`, cf. `faune_scatter`), présent dans la
    /// scène déjà embarquée. Un run avec « Faune » comme préfixe ici a
    /// effectivement retiré 119 de ces objets sans les réinjecter (la source de
    /// vérité de cet outil est `Scene::mmorpg_demo`, qui ne les contient pas) —
    /// détecté par `git diff` avant tout commit, corrigé en renommant en
    /// « Errant » ci-dessous.
    const AMBIENT_DECOR_PREFIXES: &[&str] = &[
        "Errant ",
        "Arbre exotique",
        "Mobilier du hameau",
        "Rocher moussu",
        "Sous-bois exotique",
        "Fleur des prés",
        "Culture ",
        "Rive du lac",
        "Décor du hameau",
        "Établi d'armes",
        "Étal des vivres",
        "Coin trésor",
        "Table d'apothicaire",
        // Prairie centrale élargie + haltes à mi-distance (audit de composition
        // du paysage, capture en jeu : grand aplat vert vide entre le spawn et
        // les biomes). Vérifié : ni "Prairie", ni "Halte" ne préfixe aucun nom
        // déjà embarqué (contrairement à l'incident "Faune"/`hameau_gdd_demo`).
        "Prairie centrale",
        "Halte ",
    ];

    /// OUTIL (portée : décor ambiant ajouté à `Scene::mmorpg_demo` — faune 27-61
    /// non combattante + flore/objets complémentaires), pas une preuve (lancé
    /// explicitement : `cargo test sync_embedded_scene_ambient_decor_from_the_demo
    /// -- --ignored --nocapture`). Même patron que
    /// `sync_embedded_scene_creatures_from_the_demo` (retire puis réinjecte les
    /// objets du préfixe visé, réécrit leurs imports en `bundle://m{i}_<fichier>`,
    /// préserve tout le reste), en PUREMENT ADDITIF sur `assets/bundle/` :
    /// contrairement aux deux outils existants (qui supposent leurs fichiers déjà
    /// bundlés), celui-ci copie et compresse zstd chaque fichier réellement
    /// nouveau — jamais de suppression, jamais de renumérotation des entrées déjà
    /// présentes dans `assets/player_scene.json` (la numérotation continue après
    /// la dernière entrée `imported` existante).
    #[test]
    #[ignore = "outil : réécrit assets/player_scene.json et copie/compresse dans assets/bundle/, à lancer explicitement"]
    #[cfg(not(target_arch = "wasm32"))]
    fn sync_embedded_scene_ambient_decor_from_the_demo() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
        let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
        let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
        let demo = Scene::mmorpg_demo();

        // Idempotent : un second run retire d'abord toute instance issue d'un
        // run précédent avant de réinjecter celles de la démo (pas de doublons).
        embedded
            .objects
            .retain(|o| !AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)));

        let to_add: Vec<&SceneObject> = demo
            .objects
            .iter()
            .filter(|o| AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)))
            .collect();
        assert!(
            !to_add.is_empty(),
            "la démo doit contenir le nouveau décor ambiant (faune 27-61, flore, objets)"
        );

        let bundle_dir =
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bundle"));
        std::fs::create_dir_all(bundle_dir).expect("assets/bundle/ doit exister");

        // Indice d'import démo → indice d'import embarqué : réutilise une entrée
        // déjà présente si le même fichier y est déjà référencé (dédoublonnage,
        // p. ex. `nature_tree.glb` déjà embarqué par un autre outil), sinon crée
        // une entrée neuve en continuant la numérotation.
        let mut index_map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut next_index = embedded.imported.len() as u32;
        for obj in &to_add {
            let MeshKind::Imported(demo_idx) = obj.mesh else {
                continue;
            };
            if index_map.contains_key(&demo_idx) {
                continue;
            }
            let demo_path = demo.imported[demo_idx as usize].path.clone();
            let file = std::path::Path::new(&demo_path)
                .file_name()
                .and_then(|f| f.to_str())
                .expect("nom de fichier d'import")
                .to_string();
            let embedded_idx = match embedded
                .imported
                .iter()
                .position(|m| m.path.ends_with(&file))
            {
                Some(i) => i as u32,
                None => {
                    let key = format!("m{next_index}_{file}");
                    let bundle_path = bundle_dir.join(&key);
                    if !bundle_path.exists() {
                        let data = std::fs::read(&demo_path)
                            .unwrap_or_else(|e| panic!("lecture de {demo_path} : {e}"));
                        // Même appel que `editor::export::copy_to_bundle` — un seul
                        // frame zstd niveau par défaut, décompressé par
                        // `assets::bundle_bytes` à la lecture.
                        let compressed =
                            zstd::stream::encode_all(data.as_slice(), 0).expect("compression zstd");
                        std::fs::write(&bundle_path, compressed)
                            .unwrap_or_else(|e| panic!("écriture de {key} : {e}"));
                    }
                    embedded.imported.push(ImportedMesh {
                        path: format!("{}{key}", crate::assets::SCHEME),
                        ..Default::default()
                    });
                    let idx = next_index;
                    next_index += 1;
                    idx
                }
            };
            index_map.insert(demo_idx, embedded_idx);
        }

        let n_added = to_add.len();
        for obj in to_add {
            let mut clone = obj.clone();
            if let MeshKind::Imported(demo_idx) = clone.mesh {
                clone.mesh = MeshKind::Imported(index_map[&demo_idx]);
            }
            embedded.objects.push(clone);
        }

        std::fs::write(
            path,
            serde_json::to_string_pretty(&embedded).expect("sérialisation"),
        )
        .expect("écriture de player_scene.json");
        println!(
            "player_scene.json : décor ambiant synchronisé ({n_added} objets ajoutés, {} imports au total)",
            embedded.imported.len()
        );
    }

    /// Garde-fou compagnon de `sync_embedded_scene_ambient_decor_from_the_demo` :
    /// le décor ambiant (faune 27-61 + flore/objets) doit être présent et
    /// résolu dans la scène embarquée après synchronisation — même logique que
    /// `the_embedded_scene_creatures_match_the_demo`/
    /// `the_embedded_scene_resolves_its_bundle_creatures`, étendue aux nouveaux
    /// préfixes.
    #[test]
    fn the_embedded_scene_ambient_decor_matches_the_demo() {
        let embedded = Scene::embedded_player();
        let demo = Scene::mmorpg_demo();

        let demo_names: std::collections::BTreeSet<&str> = demo
            .objects
            .iter()
            .filter(|o| AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)))
            .map(|o| o.name.as_str())
            .collect();
        assert!(
            !demo_names.is_empty(),
            "la démo doit contenir le nouveau décor ambiant"
        );
        let sync_hint = "lancer `cargo test sync_embedded_scene_ambient_decor_from_the_demo \
             -- --ignored --nocapture`";
        for name in &demo_names {
            assert!(
                embedded.objects.iter().any(|o| o.name == *name),
                "« {name} » absent de la scène embarquée — {sync_hint}"
            );
        }
        for o in embedded
            .objects
            .iter()
            .filter(|o| AMBIENT_DECOR_PREFIXES.iter().any(|p| o.name.starts_with(p)))
        {
            let MeshKind::Imported(i) = o.mesh else {
                continue;
            };
            let m = embedded.imported.get(i as usize).unwrap_or_else(|| {
                panic!(
                    "« {} » référence un import {i} absent — {sync_hint}",
                    o.name
                )
            });
            assert!(
                !m.data.vertices.is_empty(),
                "mesh embarqué « {} » (objet « {} ») non résolu (clé absente d'assets/bundle/ ?) \
                 — {sync_hint}",
                m.path,
                o.name
            );
        }
    }

    /// OUTIL, pas une preuve (lancé explicitement : `cargo test
    /// sync_embedded_scene_pickups_from_the_demo -- --ignored --nocapture`) :
    /// `Scene::mmorpg_demo` définit des `ItemPickup` (potions, baies, clé,
    /// gemme — `MMORPG_ITEMS`) qu'aucun des trois outils `sync_embedded_scene_*`
    /// existants ne reporte sur la scène servie (le remplacement d'environnement
    /// vient de `hameau_gdd_demo`, qui n'en définit aucun ; le décor ambiant ne
    /// filtre que les préfixes de `AMBIENT_DECOR_PREFIXES`) — audité dans
    /// SPRINT3D_AUDIT_GAMEDESIGN.md §4 : la carte servie n'avait donc **aucun**
    /// objet à ramasser, contredisant GDD_MMORPG.md §5.1/§15.4/§17.1. Ces objets
    /// utilisent des meshes primitifs (`Sphere`/`Capsule`, cf. `DemoItem` dans
    /// `demos.rs`) : aucun import/bundle à gérer, contrairement aux deux autres
    /// outils. Idempotent : retire d'abord toute instance déjà synchronisée
    /// (marquée par `item_pickup.is_some()`) avant de réinjecter celles de la
    /// démo.
    #[test]
    #[ignore = "outil : réécrit assets/player_scene.json, à lancer explicitement"]
    fn sync_embedded_scene_pickups_from_the_demo() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
        let json = std::fs::read_to_string(path).expect("player_scene.json lisible");
        let mut embedded: Scene = serde_json::from_str(&json).expect("player_scene.json valide");
        let demo = Scene::mmorpg_demo();

        embedded.objects.retain(|o| o.item_pickup.is_none());
        let to_add: Vec<&SceneObject> = demo
            .objects
            .iter()
            .filter(|o| o.item_pickup.is_some())
            .collect();
        assert!(
            !to_add.is_empty(),
            "la démo doit contenir des `ItemPickup` (GDD §5.1/§15.4)"
        );
        for obj in &to_add {
            assert!(
                matches!(obj.mesh, MeshKind::Sphere | MeshKind::Capsule),
                "« {} » : cet outil ne gère que des meshes primitifs, pas d'import à \
                 bundler (ajouter la gestion d'import si un futur pickup en a besoin)",
                obj.name
            );
        }
        let n_added = to_add.len();
        for obj in to_add {
            embedded.objects.push(obj.clone());
        }

        std::fs::write(
            path,
            serde_json::to_string_pretty(&embedded).expect("sérialisation"),
        )
        .expect("écriture de player_scene.json");
        println!("player_scene.json : {n_added} objets ramassables synchronisés");
    }

    /// Garde-fou compagnon de `sync_embedded_scene_pickups_from_the_demo`.
    #[test]
    fn the_embedded_scene_has_item_pickups_from_the_demo() {
        let embedded = Scene::embedded_player();
        let demo = Scene::mmorpg_demo();
        let demo_names: std::collections::BTreeSet<&str> = demo
            .objects
            .iter()
            .filter(|o| o.item_pickup.is_some())
            .map(|o| o.name.as_str())
            .collect();
        assert!(
            !demo_names.is_empty(),
            "la démo doit contenir des `ItemPickup`"
        );
        let sync_hint = "lancer `cargo test sync_embedded_scene_pickups_from_the_demo \
             -- --ignored --nocapture`";
        for name in &demo_names {
            assert!(
                embedded
                    .objects
                    .iter()
                    .any(|o| o.name == *name && o.item_pickup.is_some()),
                "« {name} » absent (ou sans `item_pickup`) de la scène embarquée — {sync_hint}"
            );
        }
    }
}
