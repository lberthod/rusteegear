//! Modèle de scène (sans ECS) : un Vec d'objets, chacun avec un Transform et un type de mesh.

pub(crate) mod demos;
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
    /// Impostor « croix » (deux plans verticaux perpendiculaires) — LOD à distance pour le
    /// feuillage dense (Phase D, `sprintoptimation3daudit10h.md`). Contrairement à `Plane`
    /// (horizontal), reste visible sous un angle de vue à hauteur d'œil.
    Billboard,
    /// Modèle glTF importé, index dans `Scene::imported`.
    Imported(u32),
}

impl MeshKind {
    /// Primitives générées par code (clés du cache de meshes GPU).
    pub const ALL: [MeshKind; 7] = [
        MeshKind::Cube,
        MeshKind::Sphere,
        MeshKind::Plane,
        MeshKind::Cylinder,
        MeshKind::Capsule,
        MeshKind::Terrain,
        MeshKind::Billboard,
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
            MeshKind::Billboard => mesh::billboard_cross([0.3, 0.45, 0.2]),
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
            MeshKind::Billboard => "Impostor",
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

/// Grammaire d'archétypes de créatures (GDD_MMORPG.md §5.4) : des paramètres sur
/// `AiChaser`, pas de nouveaux systèmes — chaque archétype doit rester identifiable à sa
/// silhouette/gabarit (choix du prefab), le champ ci-dessous ne module que la poursuite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Archetype {
    /// Valeurs standard — silhouette moyenne, tout l'arsenal en contre-jeu.
    #[default]
    Traqueuse,
    /// Vitesse accrue, PV réduits (côté `Combat`, hors `AiChaser`) — coordination à
    /// plusieurs sur une cible, dans la limite du plafond `MAX_ACTIVE_CHASERS_PER_TARGET`.
    Meute,
    /// Silhouette massive, poursuite ralentie — PV élevés et contact fort (côté `Combat`).
    Colosse,
    /// Éveil tardif : n'engage la poursuite qu'à courte portée (`FURTIVE_DETECT_RANGE`,
    /// < `CHASER_DETECT_RANGE`), mais fonce une fois éveillée.
    Furtive,
}

impl Archetype {
    /// Multiplicateur appliqué à `AiChaser::speed` une fois la poursuite engagée.
    pub fn speed_multiplier(self) -> f32 {
        match self {
            Archetype::Traqueuse => 1.0,
            Archetype::Meute => 1.25,
            Archetype::Colosse => 0.65,
            Archetype::Furtive => 1.5,
        }
    }

    /// Multiplicateur appliqué au `hp` de base d'un prefab (GDD_MMORPG.md §5.4) :
    /// Meute encaisse moins (compensé par le nombre et la vitesse), Colosse
    /// encaisse beaucoup plus (silhouette massive, contact fort) — Furtive n'est
    /// pas décrite avec des PV particuliers dans le GDD, donc standard comme
    /// Traqueuse.
    pub fn hp_multiplier(self) -> f32 {
        match self {
            Archetype::Traqueuse => 1.0,
            Archetype::Meute => 0.6,
            Archetype::Colosse => 1.8,
            Archetype::Furtive => 1.0,
        }
    }
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
    /// Famille de chasse (GDD §5.4) — `Traqueuse` par défaut (comportement historique).
    #[serde(default)]
    pub archetype: Archetype,
}

// Manuel comme `Controller` : `derive(Default)` donnerait speed=0.0 (immobile), pas la
// vitesse par défaut serde — mêmes raisons, cf. le commentaire sur `impl Default for Controller`.
impl Default for AiChaser {
    fn default() -> Self {
        Self {
            speed: default_move_speed(),
            archetype: Archetype::default(),
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
    /// Convoi à escorter (`RoundObjective::Escorte`, Sprint 7 de `sprint10audit.md`) :
    /// `None` pour la grande majorité des objets — un seul objet par scène en a un.
    /// Déplacement piloté par `AppState::update_escorte` (pas de script Lua ni
    /// d'`AiChaser`, dont la poursuite viserait le joueur au lieu d'une destination
    /// fixe). Combiné à `combat` (PV, cible d'attaque) pour être vaincu comme tout
    /// autre `Combat::attackable`.
    #[serde(default)]
    pub convoy: Option<Convoy>,
}

/// Composant optionnel : trajectoire d'un convoi à escorter (GDD_MMORPG.md §4, mode
/// Escorte — « Amener un chariot lent d'une porte du hameau à l'autre »). Ligne droite
/// vers `destination` plutôt qu'une liste de points : aucune scène existante n'a de
/// notion de chemin/patrouille structuré (les créatures errent via script Lua, cf.
/// `AiChaser`), une trajectoire simple suffit au premier mode Escorte livré — à
/// enrichir en `Vec<Vec3>` si un futur niveau a besoin de détours.
#[derive(Clone, Serialize, Deserialize)]
pub struct Convoy {
    /// Point d'arrivée (coordonnées monde) : la manche est gagnée quand le convoi
    /// s'en approche suffisamment (cf. `AppState::update_escorte`).
    pub destination: Vec3,
    /// Vitesse d'avance (unités/seconde) — « chariot lent » (GDD §4) : volontairement
    /// plus bas qu'un `AiChaser` de créature (cf. `default_move_speed`).
    #[serde(default = "default_convoy_speed")]
    pub speed: f32,
}

fn default_convoy_speed() -> f32 {
    1.2
}

impl Default for Convoy {
    fn default() -> Self {
        Self {
            destination: Vec3::ZERO,
            speed: default_convoy_speed(),
        }
    }
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
            convoy: None,
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
#[path = "mod_tests.rs"]
mod tests;
