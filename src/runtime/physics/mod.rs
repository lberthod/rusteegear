//! Monde physique rapier3d, construit à l'entrée en mode Play.
//! Mappe les objets de la scène vers des corps rigides et recopie les poses.

use glam::{Quat, Vec3};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

use crate::scene::{MeshKind, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PhysicsKind {
    None,
    Static,
    Dynamic,
    /// Corps `kinematic_position_based` pour un objet déplacé **par script Lua**
    /// (créature qui erre, PNJ en patrouille…) : le script écrit librement
    /// `obj.x/y/z`, et `Physics::resolve_scripted_moves` fait passer ce déplacement
    /// par un `KinematicCharacterController` — l'objet glisse le long des murs, des
    /// objets fixes et du joueur au lieu de les traverser (et le joueur bute sur
    /// son collider en retour). Distinct de `Static` (qui ne suit pas un objet
    /// déplacé par script) et de `Dynamic` (dont le solveur écraserait la position
    /// écrite par le script à chaque pas).
    Kinematic,
}

/// Forme du collider en mode Play. `Auto` = déduite du mesh ; sinon forcée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColliderShape {
    #[default]
    Auto,
    Box,
    Sphere,
    Capsule,
    /// Collider fidèle à la géométrie importée : un triangle par triangle du mesh —
    /// pour un **décor statique** uniquement (`TriMesh` n'a pas de volume défini,
    /// rapier refuse un corps dynamique avec ce collider ; sans garde-fou, un objet
    /// dynamique en TriMesh traverserait tout sans jamais entrer en collision).
    /// Coûteux par rapport aux primitives, mais exact — un décor à la silhouette
    /// complexe (rochers, architecture) n'a plus besoin d'un `Box`/`ConvexHull`
    /// approximatif.
    TriMesh,
    /// Enveloppe convexe des vertices importés : plus fidèle qu'un `Box`, plus léger
    /// qu'un `TriMesh`, et **utilisable en dynamique** (contrairement à `TriMesh`) —
    /// le bon choix par défaut pour un décor importé non convexe qu'on veut quand
    /// même pouvoir faire bouger.
    ConvexHull,
}

/// Multiplicateur d'accélération quand l'entrée **freine** (cible plus lente que la
/// vitesse courante le long du mouvement : relâchement, demi-tour, virage serré).
/// Départ progressif mais arrêt net : un freinage aussi mou que l'accélération donne
/// un personnage « savonnette » qui glisse au-delà de l'intention du joueur — les
/// jeux d'action freinent classiquement 1,5 à 2× plus fort qu'ils n'accélèrent.
const BRAKE_FACTOR: f32 = 2.0;

/// Fraction de l'accélération disponible **en l'air** : à 1.0 (ancien comportement),
/// la trajectoire d'un saut se pilote comme au sol, effet « téléguidé » irréel. Une
/// autorité réduite garde un ajustement possible mais laisse l'arc du saut engager —
/// la direction se choisit surtout à l'impulsion, comme attendu d'un saut crédible.
const AIR_CONTROL: f32 = 0.35;

/// Multiplicateur de gravité pendant la **descente** d'un saut/d'une chute. La
/// parabole symétrique de la gravité seule (montée = descente) donne un saut
/// flottant, « lunaire » ; retomber plus vite qu'on ne monte rend le saut vif et
/// lisible (recette standard des jeux de plateforme). N'affecte que la chute :
/// la hauteur de saut (`jump_height`, atteinte à la montée) reste exacte.
const FALL_GRAVITY_FACTOR: f32 = 1.6;

/// Hauteur maximale (m, absolue) qu'une marche automatique du contrôleur
/// cinématique du joueur (Sprint 103b) franchit sans ralentir — cf.
/// `KinematicCharacterController::autostep`. Absolue plutôt que relative à la
/// capsule : une marche d'escalier standard (~30 cm) ne dépend pas de la
/// taille du personnage. Livrable du sprint : « escalier montable ».
const PLAYER_AUTOSTEP_HEIGHT: f32 = 0.3;

/// Largeur minimale de replat (fraction du rayon de la capsule) exigée après
/// une marche automatique — sans ça, le joueur « grimperait » sur un rebord
/// trop étroit pour s'y tenir debout.
const PLAYER_AUTOSTEP_MIN_WIDTH: f32 = 0.5;

/// Pente maximale (degrés) que le joueur peut gravir sans glisser.
const PLAYER_MAX_SLOPE_CLIMB_DEG: f32 = 50.0;

/// Pente (degrés) au-delà de laquelle le joueur glisse automatiquement, même
/// à l'arrêt (`KinematicCharacterController::min_slope_slide_angle`).
const PLAYER_MIN_SLOPE_SLIDE_DEG: f32 = 45.0;

/// Distance de rattrapage au sol (fraction de la hauteur de la capsule,
/// `snap_to_ground`) : évite un décollement visible en descendant une
/// marche/pente à vitesse normale.
const PLAYER_SNAP_TO_GROUND: f32 = 0.2;

/// Vitesse de descente (m/s) appliquée aux corps kinématiques **scriptés**
/// (`PhysicsKind::Kinematic`, cf. `Physics::resolve_scripted_moves`) : les
/// scripts de patrouille ne pilotent que x/z, cette descente constante plaque
/// l'objet au sol (et le fait retomber d'un rebord) sans intégrer une vraie
/// chute libre — inutilement complexe pour un PNJ qui marche.
const SCRIPTED_FALL_SPEED: f32 = 3.0;

/// Distance (m) au-delà de laquelle `Physics::set_position` (Sprint 103c)
/// considère qu'un déplacement kinématique imposé hors de `move_shape` a pu
/// invalider l'état « au sol » mis en cache — largement au-dessus de ce
/// qu'une correction de réconciliation réseau normale déplace en un appel
/// (`CORRECTION_PULL`/`IDLE_SETTLE_PULL` dans `app::network_client`, bornées
/// par des fractions de `interpolation::SNAP_THRESHOLD` ≈ 0,5 m), pour ne
/// viser que les vraies téléportations (respawn, gros désync).
const TELEPORT_INVALIDATES_GROUND: f32 = 1.0;

/// État propre au contrôleur cinématique du joueur (Sprint 103b) : un corps
/// `kinematic_position_based` n'a pas de `linvel` géré par rapier (il est
/// déplacé par consigne, pas par force/vitesse) — on garde donc nous-mêmes la
/// vitesse horizontale visée, la vitesse verticale, et le dernier statut « au
/// sol » renvoyé par `move_shape` (utilisé au tick suivant, pas de requête de
/// sol à chaque appel).
#[derive(Clone, Copy)]
struct KinematicState {
    hvel: Vec3,
    vspeed: f32,
    grounded: bool,
}

pub struct Physics {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    gravity: Vector,
    integration: IntegrationParameters,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: DefaultBroadPhase,
    narrow: NarrowPhase,
    impulse: ImpulseJointSet,
    multibody: MultibodyJointSet,
    ccd: CCDSolver,
    /// (index d'objet, handle) pour les corps dynamiques à recopier.
    dynamic: Vec<(usize, RigidBodyHandle)>,
    /// (index d'objet, handle) pour les objets **pilotables** dynamiques (IA
    /// poursuivante, recul/knockback) — le joueur n'y est plus depuis le
    /// Sprint 103b, cf. `kinematic` ci-dessous.
    controlled: Vec<(usize, RigidBodyHandle)>,
    /// (index d'objet, handle, état) pour le(s) joueur(s) (Sprint 103b) :
    /// corps `kinematic_position_based`, piloté par `KinematicCharacterController`
    /// plutôt que par vitesse/force — gère nativement pentes, marches et snap
    /// au sol, contrairement à l'ancienne heuristique `cur.y.abs() < 1.0`.
    kinematic: Vec<(usize, RigidBodyHandle, KinematicState)>,
    /// (index d'objet, handle) pour les objets **scriptés** à collisions
    /// (`PhysicsKind::Kinematic`) : corps `kinematic_position_based` dont le
    /// déplacement écrit par le script Lua est résolu chaque pas par
    /// `resolve_scripted_moves` (glisse contre murs/objets fixes/joueur).
    scripted: Vec<(usize, RigidBodyHandle)>,
    /// Collider → index d'objet, pour **tous** les colliders construits (statiques
    /// inclus, contrairement à `dynamic`/`controlled`/`kinematic` qui ne suivent que
    /// ce qui doit être recopié/piloté chaque frame) — nécessaire pour retrouver
    /// quel objet une requête spatiale (`raycast`/`overlap_sphere`) a touché.
    collider_owner: std::collections::HashMap<ColliderHandle, usize>,
    /// Broad-phase de requête mémoïsée entre deux mutations du monde (cf.
    /// `with_query_broad_phase`) : les sondes des créatures lancent jusqu'à
    /// 60 rayons par tick sur une scène dense — reconstruire la BVH jetable à
    /// chaque appel redevenait O(rayons × colliders). `RefCell` car les requêtes
    /// prennent `&self` ; `Physics` vit dans `AppState`, mono-thread.
    query_cache: std::cell::RefCell<Option<DefaultBroadPhase>>,
}

/// Résultat d'un `Physics::raycast` : point d'impact (monde), distance parcourue
/// depuis l'origine, et index de l'objet touché (`None` si le collider touché n'a
/// pas été retrouvé dans `collider_owner` — ne doit pas arriver en pratique, tous
/// les colliders construits par `build` y sont enregistrés).
pub struct RaycastHit {
    pub point: Vec3,
    pub distance: f32,
    pub index: Option<usize>,
}

mod build;
mod control;
mod query;
mod step;

#[cfg(test)]
mod tests;
