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

impl Physics {
    /// Construit le monde à partir des objets ayant un type de physique.
    pub fn build(scene: &Scene) -> Physics {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut dynamic = Vec::new();
        let mut controlled = Vec::new();
        let mut kinematic = Vec::new();
        let mut scripted = Vec::new();
        let mut collider_owner = std::collections::HashMap::new();

        for (i, obj) in scene.objects.iter().enumerate() {
            // Le joueur (joystick/gyro) devient un corps **kinématique** (Sprint
            // 103b, `KinematicCharacterController` : pentes/marches/snap au sol
            // natifs) ; une IA poursuivante **visible** reste un corps dynamique
            // ordinaire piloté par vitesse, comme avant — les deux sont « pilotés »
            // par `Physics::control` (le joueur par l'entrée, l'IA par la direction
            // vers le joueur, cf. `App::advance_play`), qui distingue en interne
            // selon la liste (`kinematic` vs `controlled`). Un chasseur masqué
            // (manche pas encore révélée, ou vaincu) n'a pas de corps : sinon son
            // collider bloquerait le joueur alors qu'il est invisible (cf.
            // `App::init_waves`/`update_waves`).
            let is_player = obj.controller.as_ref().is_some_and(|c| c.input || c.gyro);
            let is_ai = obj.ai_chaser.is_some() && obj.visible;
            let controllable = is_player || is_ai;
            if matches!(obj.physics, PhysicsKind::None) && !controllable {
                continue;
            }
            // Objet scripté à collisions (cf. `PhysicsKind::Kinematic`) : corps
            // kinématique piloté par `resolve_scripted_moves`, sauf s'il est déjà
            // joueur (le contrôleur joueur prime) ou IA poursuivante (corps
            // dynamique piloté par vitesse, comme avant).
            let is_scripted = obj.physics == PhysicsKind::Kinematic && !controllable;
            let is_dynamic =
                !is_player && !is_scripted && (obj.physics == PhysicsKind::Dynamic || controllable);

            let t = &obj.transform;
            let (axis, angle) = t.rotation.to_axis_angle();
            let rotvec = axis * angle;

            let mut builder = if is_player || is_scripted {
                RigidBodyBuilder::kinematic_position_based()
            } else if is_dynamic {
                RigidBodyBuilder::dynamic()
            } else {
                RigidBodyBuilder::fixed()
            };
            // Objet pilotable dynamique (IA) : on bloque les rotations pour qu'il
            // reste debout — moot pour un corps kinématique, jamais soumis au
            // solveur de toute façon (sa rotation reste entièrement pilotée par
            // l'appelant, jamais par rapier).
            if controllable && !is_player {
                builder = builder.lock_rotations();
            }
            // CCD : cf. la doc de `SceneObject::ccd` — seulement les objets qui en
            // ont explicitement besoin (missiles/projectiles rapides, toujours
            // dynamiques : un corps kinématique est déplacé par shapecast
            // successif, jamais par intégration rapide sujette au tunneling
            // que la CCD corrige).
            if obj.ccd && !is_player {
                builder = builder.ccd_enabled(true);
            }
            let body = builder
                .translation(Vector::new(t.position.x, t.position.y, t.position.z))
                .rotation(Vector::new(rotvec.x, rotvec.y, rotvec.z))
                .build();
            let handle = bodies.insert(body);

            // demi-dimensions du collider : AABB local mis à l'échelle. `center` :
            // les primitives du moteur sont modélisées centrées sur l'origine
            // (centre ≈ 0, offset sans effet), mais un mesh **importé** ne l'est
            // presque jamais (un personnage a les pieds à l'origine, son AABB
            // s'étend vers le haut) — sans cet offset, un collider déduit de
            // l'AABB (Box/Sphere/Capsule/Auto) serait centré sur les pieds,
            // à moitié enterré et débordant sous le sol.
            let (lmin, lmax) = scene.local_aabb(obj.mesh);
            let he = (lmax - lmin) * 0.5 * t.scale;
            let center = (lmin + lmax) * 0.5 * t.scale;
            let cuboid = || {
                ColliderBuilder::cuboid(
                    he.x.abs().max(0.01),
                    he.y.abs().max(0.01),
                    he.z.abs().max(0.01),
                )
                .translation(center)
            };
            let ball =
                || ColliderBuilder::ball(he.x.abs().max(he.z.abs()).max(0.01)).translation(center);
            let capsule = || {
                let r = he.x.abs().max(he.z.abs()).max(0.01);
                let half = (he.y.abs() - r).max(0.01);
                ColliderBuilder::capsule_y(half, r).translation(center)
            };
            // Vertices bruts du mesh importé, mis à l'échelle de l'objet — même
            // principe que `he` ci-dessus pour les primitives : le collider rapier
            // n'a pas de transform d'échelle séparée, l'échelle doit être bakée dans la
            // géométrie fournie. `None` pour tout ce qui n'est pas `MeshKind::Imported`
            // (primitives) ou dont l'import n'a pas encore chargé de données.
            let imported_points = || -> Option<Vec<Vec3>> {
                let MeshKind::Imported(idx) = obj.mesh else {
                    return None;
                };
                let data = &scene.imported.get(idx as usize)?.data;
                if data.vertices.is_empty() {
                    return None;
                }
                Some(
                    data.vertices
                        .iter()
                        .map(|v| Vec3::from(v.position) * t.scale)
                        .collect(),
                )
            };
            // Silhouette exacte : un triangle rapier par triangle du mesh importé.
            // Réservé au décor **statique** par l'appelant (cf. `ColliderShape::
            // TriMesh` ci-dessous) — `TriMesh` n'a pas de propriétés de masse définies,
            // rapier ne sait pas en faire un corps dynamique cohérent.
            let trimesh = || -> Option<ColliderBuilder> {
                let MeshKind::Imported(idx) = obj.mesh else {
                    return None;
                };
                let data = &scene.imported.get(idx as usize)?.data;
                if data.indices.len() < 3 {
                    return None;
                }
                let points = imported_points()?;
                let tris: Vec<[u32; 3]> = data
                    .indices
                    .chunks_exact(3)
                    .map(|c| [c[0], c[1], c[2]])
                    .collect();
                SharedShape::trimesh(points, tris)
                    .ok()
                    .map(ColliderBuilder::new)
            };
            // Enveloppe convexe : plus fidèle qu'une boîte, et — contrairement à
            // `TriMesh` — utilisable sur un corps dynamique (volume défini, propriétés
            // de masse calculables).
            let convex_hull = || -> Option<ColliderBuilder> {
                Some(ColliderBuilder::new(SharedShape::convex_hull(
                    &imported_points()?,
                )?))
            };
            // Forme explicite si demandée, sinon déduite du mesh.
            let collider = match obj.collider_shape {
                ColliderShape::Box => cuboid(),
                ColliderShape::Sphere => ball(),
                ColliderShape::Capsule => capsule(),
                ColliderShape::TriMesh => {
                    if is_dynamic {
                        log::warn!(
                            "{} : collider TriMesh demandé sur un corps dynamique (sans \
                             propriétés de masse définies) — repli sur ConvexHull.",
                            obj.name
                        );
                        convex_hull().unwrap_or_else(cuboid)
                    } else {
                        trimesh().unwrap_or_else(cuboid)
                    }
                }
                ColliderShape::ConvexHull => convex_hull().unwrap_or_else(cuboid),
                ColliderShape::Auto => match obj.mesh {
                    MeshKind::Sphere => ball(),
                    MeshKind::Capsule => capsule(),
                    MeshKind::Cylinder => {
                        ColliderBuilder::cylinder(he.y.abs().max(0.01), he.x.abs().max(0.01))
                            .translation(center)
                    }
                    _ => cuboid(),
                },
            }
            // Aucun rebond : un personnage n'est pas une balle (cf. docs/audits/
            // physics.md pour le mouvement instable observé avec un rebond non nul).
            // Rien dans le projet ne dépend d'un rebond (aucun mécanisme de type
            // trampoline).
            .restitution(0.0)
            .friction(0.6)
            // Couches de collision : `Group::from_bits_truncate` ignore silencieusement
            // les bits au-delà de 32 plutôt que de paniquer sur une valeur mal formée —
            // un JSON de scène corrompu/ancien ne doit pas faire planter l'entrée en
            // Play. `And` : les deux objets doivent s'accepter mutuellement (cf. la doc
            // de `InteractionGroups`), le mode le plus intuitif pour une paire
            // couche/masque.
            .collision_groups(InteractionGroups::new(
                Group::from_bits_truncate(obj.collision_layer),
                Group::from_bits_truncate(obj.collision_mask),
                InteractionTestMode::And,
            ))
            .build();
            let collider_handle = colliders.insert_with_parent(collider, handle, &mut bodies);
            collider_owner.insert(collider_handle, i);

            if is_dynamic {
                dynamic.push((i, handle));
            }
            if is_player {
                kinematic.push((
                    i,
                    handle,
                    KinematicState {
                        hvel: Vec3::ZERO,
                        vspeed: 0.0,
                        // Vrai par défaut : au repos à l'apparition (vitesse nulle),
                        // même convention que l'ancienne heuristique dynamique
                        // (`cur.y.abs() < 1.0`, vraie tant qu'aucune chute n'a
                        // commencé).
                        grounded: true,
                    },
                ));
            } else if controllable {
                controlled.push((i, handle));
            } else if is_scripted {
                scripted.push((i, handle));
            }
        }

        // Plus d'itérations solveur que la valeur par défaut (4 → 8) : stabilise
        // les contacts (sol, murs, entre joueurs) — avec `restitution(0.0)` seul,
        // il restait un léger tremblement résiduel au repos/contact prolongé,
        // moins perceptible avec un solveur plus précis. Coût négligeable à cette
        // échelle (quelques corps dynamiques, pas des centaines).
        let integration = IntegrationParameters {
            num_solver_iterations: 8,
            ..Default::default()
        };

        Physics {
            bodies,
            colliders,
            gravity: Vector::new(0.0, -9.81, 0.0),
            integration,
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: DefaultBroadPhase::new(),
            narrow: NarrowPhase::new(),
            impulse: ImpulseJointSet::new(),
            multibody: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            dynamic,
            controlled,
            kinematic,
            scripted,
            collider_owner,
        }
    }

    /// Pilote un objet (corps `controlled`) : fait tendre la vitesse horizontale vers
    /// `(vx, vz)` (joystick/gyro) et déclenche un saut si demandé **et** que l'objet est
    /// au sol. La vitesse verticale est sinon conservée (gravité), avec une gravité
    /// renforcée en descente (cf. `FALL_GRAVITY_FACTOR` : saut vif plutôt que
    /// « lunaire »). `jump_speed` = vitesse initiale du saut (m/s). `accel` (m/s²)
    /// borne la variation de vitesse horizontale par seconde — `0.0` fixe la vitesse
    /// instantanément (utilisé par l'IA/le recul, qui n'ont pas besoin d'inertie). Une
    /// valeur positive (mouvement du joueur, cf. `Controller::acceleration`) lisse
    /// départs et arrêts au lieu d'un « on/off » robotique, avec un freinage plus fort
    /// que l'accélération (`BRAKE_FACTOR` : arrêts nets) et une autorité réduite en
    /// l'air (`AIR_CONTROL` : arc de saut crédible). Renvoie `true` si un **saut** a
    /// effectivement été déclenché (objet au sol).
    #[allow(clippy::too_many_arguments)] // paramètres physiques distincts d'un même appel
    pub fn control(
        &mut self,
        index: usize,
        vx: f32,
        vz: f32,
        jump: bool,
        jump_speed: f32,
        accel: f32,
        dt: f32,
    ) -> bool {
        if let Some(slot) = self.kinematic.iter().position(|&(i, _, _)| i == index) {
            return self.control_kinematic(slot, vx, vz, jump, jump_speed, accel, dt);
        }
        let mut jumped = false;
        for &(i, handle) in &self.controlled {
            if i != index {
                continue;
            }
            if let Some(body) = self.bodies.get_mut(handle) {
                let cur = body.linvel();
                // Au sol : vitesse verticale quasi nulle (heuristique simple, sans raycast).
                // Effet secondaire bienvenu : le seuil large (< 1 m/s, soit ~0,1 s de chute
                // libre) offre un « coyote time » naturel — sauter juste après avoir quitté
                // un rebord fonctionne encore, comme dans les plateformers soignés.
                let grounded = cur.y.abs() < 1.0;
                let do_jump = jump && grounded;
                let vy = if do_jump {
                    jump_speed
                } else if !grounded && cur.y < 0.0 {
                    // Descente : gravité renforcée (cf. `FALL_GRAVITY_FACTOR`) — la part
                    // de base (×1) est déjà intégrée par `step`, on n'ajoute que l'excès.
                    cur.y - 9.81 * (FALL_GRAVITY_FACTOR - 1.0) * dt
                } else {
                    cur.y
                };
                let (nx, nz) = if accel > 0.0 {
                    // Accélération effective : renforcée au freinage (la cible ne
                    // prolonge pas la vitesse courante — relâchement, demi-tour,
                    // virage : cf. `BRAKE_FACTOR`), réduite en l'air (`AIR_CONTROL`).
                    let cur_sq = cur.x * cur.x + cur.z * cur.z;
                    let braking = vx * cur.x + vz * cur.z < cur_sq - 1e-6;
                    let mut a = accel;
                    if braking {
                        a *= BRAKE_FACTOR;
                    }
                    if !grounded {
                        a *= AIR_CONTROL;
                    }
                    let dx = vx - cur.x;
                    let dz = vz - cur.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    let max_step = a * dt;
                    if dist <= max_step || dist < 1e-6 {
                        (vx, vz)
                    } else {
                        (cur.x + dx / dist * max_step, cur.z + dz / dist * max_step)
                    }
                } else {
                    (vx, vz)
                };
                body.set_linvel(Vector::new(nx, vy, nz), true);
                jumped |= do_jump;
            }
        }
        jumped
    }

    /// Chemin `control` pour un corps **kinématique** (joueur, Sprint 103b) : même
    /// contrat que la boucle `dynamic` ci-dessus (freinage/autorité en l'air/chute
    /// accélérée identiques, cf. `BRAKE_FACTOR`/`AIR_CONTROL`/`FALL_GRAVITY_FACTOR`),
    /// mais la vitesse n'existe plus dans rapier (corps `kinematic_position_based`) :
    /// elle est gardée dans `KinematicState` et le déplacement réel passe par
    /// `KinematicCharacterController::move_shape`, qui gère nativement pentes/
    /// marches/snap au sol (contrairement à l'ancienne heuristique `cur.y.abs() <
    /// 1.0`, remplacée ici par `state.grounded`, le résultat du `move_shape`
    /// précédent).
    #[allow(clippy::too_many_arguments)]
    fn control_kinematic(
        &mut self,
        slot: usize,
        vx: f32,
        vz: f32,
        jump: bool,
        jump_speed: f32,
        accel: f32,
        dt: f32,
    ) -> bool {
        let (_, handle, state) = self.kinematic[slot];

        let grounded = state.grounded;
        let do_jump = jump && grounded;
        let vspeed = if do_jump {
            jump_speed
        } else if grounded {
            // Pas de solveur de contact pour maintenir un corps kinématique au
            // repos sur le sol : on remet explicitement à zéro plutôt que de
            // laisser une vitesse verticale résiduelle s'accumuler.
            0.0
        } else {
            // Gravité manuelle (rapier n'intègre pas de gravité sur un corps
            // kinématique) : base + excès de chute combinés en un seul terme,
            // même physique que l'ancien couple step()+control() sur corps
            // dynamique (cf. `FALL_GRAVITY_FACTOR`).
            let factor = if state.vspeed < 0.0 {
                FALL_GRAVITY_FACTOR
            } else {
                1.0
            };
            state.vspeed - 9.81 * factor * dt
        };

        let (nx, nz) = if accel > 0.0 {
            let cur = state.hvel;
            let cur_sq = cur.x * cur.x + cur.z * cur.z;
            let braking = vx * cur.x + vz * cur.z < cur_sq - 1e-6;
            let mut a = accel;
            if braking {
                a *= BRAKE_FACTOR;
            }
            if !grounded {
                a *= AIR_CONTROL;
            }
            let dx = vx - cur.x;
            let dz = vz - cur.z;
            let dist = (dx * dx + dz * dz).sqrt();
            let max_step = a * dt;
            if dist <= max_step || dist < 1e-6 {
                (vx, vz)
            } else {
                (cur.x + dx / dist * max_step, cur.z + dz / dist * max_step)
            }
        } else {
            (vx, vz)
        };

        let Some(body) = self.bodies.get(handle) else {
            return false;
        };
        let Some(&collider_handle) = body.colliders().first() else {
            return false;
        };
        let Some(collider) = self.colliders.get(collider_handle) else {
            return false;
        };
        let shape = collider.shape();
        let shape_pos = *body.position();
        let translation = body.translation();

        let desired = Vector::new(nx, vspeed, nz) * dt;
        let filter = QueryFilter::new().exclude_rigid_body(handle);
        let queries = self.broad.as_query_pipeline(
            self.narrow.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let controller = KinematicCharacterController {
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(PLAYER_AUTOSTEP_HEIGHT),
                min_width: CharacterLength::Relative(PLAYER_AUTOSTEP_MIN_WIDTH),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: PLAYER_MAX_SLOPE_CLIMB_DEG.to_radians(),
            min_slope_slide_angle: PLAYER_MIN_SLOPE_SLIDE_DEG.to_radians(),
            snap_to_ground: Some(CharacterLength::Relative(PLAYER_SNAP_TO_GROUND)),
            ..Default::default()
        };
        let movement = controller.move_shape(dt, &queries, shape, &shape_pos, desired, |_| {});
        let new_translation = translation + movement.translation;

        // Vitesse horizontale dérivée du mouvement **réel** (post-collision), pas
        // de la cible commandée : un mur doit freiner le joueur visiblement au
        // tick suivant, pas être ignoré par la continuité d'accélération (même
        // sensation que le solveur de contact sur l'ancien corps dynamique). La
        // composante verticale reste analytique (`vspeed` calculé ci-dessus) :
        // un petit ajustement de `snap_to_ground` ne doit pas se lire comme un
        // freinage de chute.
        let new_hvel = if dt > 1e-6 {
            Vec3::new(movement.translation.x, 0.0, movement.translation.z) / dt
        } else {
            Vec3::ZERO
        };
        self.kinematic[slot].2 = KinematicState {
            hvel: new_hvel,
            vspeed,
            grounded: movement.grounded,
        };

        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_next_kinematic_translation(new_translation);
        }

        do_jump
    }

    /// Résout les déplacements écrits par les scripts Lua pour les objets
    /// `PhysicsKind::Kinematic` (cf. `scripted`) — à appeler chaque pas fixe
    /// **après** la boucle des scripts et **avant** `step`. Le script écrit
    /// librement `obj.x/y/z` ; ici, le déplacement demandé (position écrite −
    /// position réelle du corps) passe par un `KinematicCharacterController` :
    /// l'objet glisse le long des murs, objets fixes et autres corps (joueur
    /// compris) au lieu de les traverser, et la position **réellement atteinte**
    /// est réécrite dans la scène (le script du tick suivant repart de là — même
    /// principe que la vitesse post-collision de `control_kinematic`).
    ///
    /// Pas d'`autostep` (contrairement au joueur) : une créature ne doit pas
    /// « escalader » automatiquement le joueur ou un petit obstacle — elle bute
    /// et glisse, c'est tout. Une descente constante (`SCRIPTED_FALL_SPEED`)
    /// plaque l'objet au sol : les scripts de patrouille ne pilotent que x/z, et
    /// sans elle un objet apparu légèrement au-dessus du sol flotterait pour
    /// toujours (un corps kinématique ne subit pas la gravité de rapier).
    ///
    /// **Dépénétration bornée** (audit gameplay « gros sauts ») : quand deux
    /// corps scriptés finissent superposés — chacun résolu contre la position
    /// *d'avant-pas* de l'autre (`set_next_kinematic_translation` ne s'applique
    /// qu'au `step`), deux créatures qui se croisent peuvent avancer l'une dans
    /// l'autre au même tick — le contrôleur les expulsait d'un seul coup au tick
    /// suivant : un **pop latéral** de plusieurs fois la vitesse de marche,
    /// visible en jeu comme une téléportation. Le déplacement horizontal résolu
    /// est donc plafonné au déplacement demandé, plus un petit budget de
    /// séparation (`DEPEN_SPEED`) : la même expulsion s'étale sur quelques
    /// ticks — une poussée, pas un bond. Preuve :
    /// `app::simulation::tests::mmorpg_creatures_never_teleport_nor_snap_turn`.
    pub fn resolve_scripted_moves(&mut self, dt: f32, scene: &mut Scene) {
        /// Vitesse maximale (m/s) que la dépénétration peut ajouter au
        /// déplacement demandé par le script.
        const DEPEN_SPEED: f32 = 0.8;
        for slot in 0..self.scripted.len() {
            let (index, handle) = self.scripted[slot];
            let Some(obj) = scene.objects.get_mut(index) else {
                continue;
            };
            let Some(body) = self.bodies.get(handle) else {
                continue;
            };
            let Some(&collider_handle) = body.colliders().first() else {
                continue;
            };
            let Some(collider) = self.colliders.get(collider_handle) else {
                continue;
            };
            let shape = collider.shape();
            let cur = body.translation();
            // Pose du shape : translation réelle du corps, mais rotation écrite
            // par le script ce tick (`obj.ry`, pas encore commise sur le corps),
            // composée avec l'offset local du collider (mesh importé non centré).
            let local = collider.position_wrt_parent().copied().unwrap_or_default();
            let body_pose = Pose::from_parts(cur, obj.transform.rotation);
            let shape_pos = body_pose * local;

            let target = obj.transform.position;
            let mut desired = target - cur;
            desired.y -= SCRIPTED_FALL_SPEED * dt;

            let filter = QueryFilter::new().exclude_rigid_body(handle);
            let queries = self.broad.as_query_pipeline(
                self.narrow.query_dispatcher(),
                &self.bodies,
                &self.colliders,
                filter,
            );
            let controller = KinematicCharacterController {
                slide: true,
                snap_to_ground: Some(CharacterLength::Relative(PLAYER_SNAP_TO_GROUND)),
                ..Default::default()
            };
            let movement = controller.move_shape(dt, &queries, shape, &shape_pos, desired, |_| {});
            let mut translation = movement.translation;
            // Plafond horizontal : jamais plus loin que demandé + le budget de
            // dépénétration (cf. la doc de cette fonction).
            let wanted_xz = Vec3::new(desired.x, 0.0, desired.z).length();
            let got_xz = Vec3::new(translation.x, 0.0, translation.z).length();
            let cap = wanted_xz + DEPEN_SPEED * dt;
            if got_xz > cap && got_xz > 1e-9 {
                let k = cap / got_xz;
                translation.x *= k;
                translation.z *= k;
            }
            let resolved = cur + translation;

            obj.transform.position = resolved;
            let next_rotation = obj.transform.rotation;
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_next_kinematic_translation(resolved);
                body.set_next_kinematic_rotation(next_rotation);
            }
        }
    }

    /// Vitesse linéaire (m/s) de l'objet `index` (corps dynamique **ou**
    /// kinématique, Sprint 103b), `None` s'il n'en a pas. Sert au rattrapage
    /// doux à l'arrêt de la réconciliation réseau (cf. `app::network_client`) :
    /// distinguer « joueur immobile » (on peut aligner sans gêner) de « en
    /// plein déplacement ». Un corps kinématique n'a pas de `linvel` géré par
    /// rapier — on renvoie la vitesse suivie nous-mêmes dans `KinematicState`
    /// (cf. `control_kinematic`), mise à jour à chaque appel de `control`.
    pub fn velocity(&self, index: usize) -> Option<Vec3> {
        if let Some(&(_, _, state)) = self.kinematic.iter().find(|&&(i, _, _)| i == index) {
            return Some(Vec3::new(state.hvel.x, state.vspeed, state.hvel.z));
        }
        let &(_, handle) = self.dynamic.iter().find(|&&(i, _)| i == index)?;
        let v = self.bodies.get(handle)?.linvel();
        Some(Vec3::new(v.x, v.y, v.z))
    }

    /// Force la position du corps rigide (dynamique **ou** kinématique,
    /// Sprint 103b) de l'objet `index`, sans effet s'il n'en a pas (objet
    /// statique/sans physique) — utilisé par la réconciliation réseau du
    /// joueur local (`app::network_client::apply_local_network_position`,
    /// `SPRINTNETWORK.md`).
    ///
    /// **Nécessaire, pas cosmétique** : `step` recopie la pose du corps
    /// rigide dans `scene.objects[index].transform` à *chaque* appel (sync à
    /// sens unique physique → transform, jamais l'inverse) — écrire
    /// directement dans `transform.position` sans passer par cette méthode
    /// n'a donc d'effet que pour la frame courante ; `step` l'écrase dès le
    /// tick suivant avec la position du corps rigide, resté inchangé (cf.
    /// docs/audits/physics.md pour le bug réel que ça a causé).
    /// `set_translation` fonctionne aussi bien sur un corps kinématique
    /// (téléportation directe, hors de `move_shape`) que dynamique.
    ///
    /// Sprint 103c (audit réseau après la migration 103b) : pour un corps
    /// kinématique, remet aussi `KinematicState.grounded` à `false` si le
    /// déplacement dépasse `TELEPORT_INVALIDATES_GROUND` — une vraie
    /// téléportation (respawn, gros désync) place le corps *hors* de
    /// `move_shape`, où l'état « au sol » mis en cache par le dernier
    /// `control_kinematic` n'a plus aucune raison d'être encore valable
    /// (ex. la correction retire le joueur d'une plateforme). **Pas** pour
    /// les petites corrections de réconciliation habituelles (`CORRECTION_
    /// PULL`/`IDLE_SETTLE_PULL` dans `app::network_client`, de l'ordre du
    /// centimètre à quelques dizaines de cm par appel) : un premier essai
    /// remettait `grounded` à `false` sur *toute* correction, quelle que
    /// soit son amplitude — en écrivant le test de montée d'escalier avec
    /// réconciliation simulée (`network_client::tests::climbing_stairs_
    /// does_not_trigger_a_spurious_correction`), ça cassait la montée
    /// normale : la réconciliation corrige quasiment à chaque tick pendant
    /// un déplacement réel, donc `grounded` ne restait jamais vrai assez
    /// longtemps pour que `control_kinematic` cesse d'appliquer un tick de
    /// gravité parasite à chaque correction, cumulant une chute jamais
    /// voulue. Le seuil distingue les deux cas : sous lui, on fait confiance
    /// à l'état mis en cache (la correction est trop petite pour avoir pu
    /// faire décoller le joueur) ; au-dessus, on force une vraie détection.
    pub fn set_position(&mut self, index: usize, pos: Vec3) {
        if let Some(slot) = self.kinematic.iter().position(|&(i, _, _)| i == index) {
            let handle = self.kinematic[slot].1;
            if let Some(body) = self.bodies.get_mut(handle) {
                let prev = body.translation();
                let moved = (Vector::new(pos.x, pos.y, pos.z) - prev).length();
                body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
                if moved > TELEPORT_INVALIDATES_GROUND {
                    self.kinematic[slot].2.grounded = false;
                }
            }
            return;
        }
        if let Some(&(_, handle)) = self.dynamic.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
            return;
        }
        // Corps scripté (`PhysicsKind::Kinematic`, cf. `scripted` et
        // `resolve_scripted_moves`) : sans ce cas, un appelant qui téléporte un
        // objet scripté (tests, réconciliation future) ne ferait bouger que
        // `scene.objects[index].transform` — `resolve_scripted_moves` lirait
        // ensuite une position physique périmée au tick suivant (`cur =
        // body.translation()`), et calculerait un déplacement `desired` aberrant
        // à partir de l'ancien emplacement jamais mis à jour ici.
        if let Some(&(_, handle)) = self.scripted.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_next_kinematic_translation(Vector::new(pos.x, pos.y, pos.z));
            body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
        }
    }

    /// Impose la vitesse linéaire d'un corps dynamique : utile pour un projectile qui
    /// doit partir à une vitesse connue dès sa création, plutôt que de l'accélérer
    /// progressivement comme le ferait `control` pour un joueur piloté.
    pub fn set_velocity(&mut self, index: usize, v: Vec3) {
        if let Some(&(_, handle)) = self.dynamic.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_linvel(Vector::new(v.x, v.y, v.z), true);
        }
    }

    /// Broad-phase **jetable**, reconstruite à la volée pour une requête spatiale
    /// ponctuelle (`raycast`/`overlap_sphere`) — délibérément distincte de
    /// `self.broad` (la BVH incrémentale que `step` fait vivre d'un pas à l'autre) :
    /// la peupler nous-mêmes ici évite de perturber son état interne (compteurs de
    /// changement, détection de première passe) entre deux pas de simulation (cf.
    /// docs/audits/physics.md — la réutiliser a fait dérailler la physique réelle en
    /// test). Reconstruire à chaque appel coûte O(nombre de colliders) — acceptable à
    /// l'échelle d'un script par tick, pas d'un appel par frame et par pixel.
    fn query_broad_phase(&self) -> DefaultBroadPhase {
        let mut broad = DefaultBroadPhase::new();
        let handles: Vec<ColliderHandle> = self.collider_owner.keys().copied().collect();
        broad.update(
            &self.integration,
            &self.colliders,
            &self.bodies,
            &handles,
            &[],
            &mut Vec::new(),
        );
        broad
    }

    /// Lance un rayon dans le monde physique, via le `QueryPipeline` de rapier —
    /// brique de `raycast()` côté Lua (`src/app/mod.rs`) : capteur de sol (rayon vers
    /// le bas), ligne de vue d'un cône de vision, etc. `mask` filtre les colliders par
    /// couche (mêmes bits que `collision_layer`/`collision_mask`) : seuls les colliders
    /// dont la couche recoupe `mask` sont touchés. `dir` n'a pas besoin d'être
    /// normalisé ; direction nulle → `None` sans planter plutôt que de diviser par
    /// zéro (`Vec3::try_normalize`).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_toi: f32, mask: u32) -> Option<RaycastHit> {
        let dir = dir.try_normalize()?;
        let broad = self.query_broad_phase();
        let query = broad.as_query_pipeline(
            self.narrow.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::new().groups(InteractionGroups::new(
                Group::ALL,
                Group::from_bits_truncate(mask),
                InteractionTestMode::And,
            )),
        );
        let ray = Ray::new(origin, dir);
        let (handle, toi) = query.cast_ray(&ray, max_toi.max(0.0), true)?;
        Some(RaycastHit {
            point: origin + dir * toi,
            distance: toi,
            index: self.collider_owner.get(&handle).copied(),
        })
    }

    /// Renvoie les index d'objets dont le collider recoupe une sphère de `radius`
    /// centrée en `center` (`QueryPipeline::intersect_shape`) — brique
    /// d'`overlap_sphere()` côté Lua : détection de proximité (ennemis dans un rayon,
    /// zone d'effet), sans avoir à lancer un rayon par direction possible. Même
    /// filtrage par couche que `raycast`.
    pub fn overlap_sphere(&self, center: Vec3, radius: f32, mask: u32) -> Vec<usize> {
        let broad = self.query_broad_phase();
        let query = broad.as_query_pipeline(
            self.narrow.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::new().groups(InteractionGroups::new(
                Group::ALL,
                Group::from_bits_truncate(mask),
                InteractionTestMode::And,
            )),
        );
        let ball = Ball::new(radius.max(0.0));
        query
            .intersect_shape(Pose::from_translation(center), &ball)
            .filter_map(|(handle, _)| self.collider_owner.get(&handle).copied())
            .collect()
    }

    /// Avance la simulation de `dt` et recopie les poses des corps dynamiques
    /// **et** kinématiques (Sprint 103b). `pipeline.step` déplace un corps
    /// kinématique vers la translation programmée par `control_kinematic` via
    /// `set_next_kinematic_translation` — la recopie ci-dessous ne fait que
    /// refléter ce résultat dans `transform`, comme pour un corps dynamique.
    /// Sprint 125 : ajoute la vitesse de chaque zone de vent (`SceneObject::wind`,
    /// `trigger: true`) aux corps dynamiques dont l'AABB la touche, avant l'intégration
    /// de ce pas — un corps qui quitte la zone n'est plus poussé dès le pas suivant
    /// (pas de vitesse résiduelle stockée), et un corps traversé par deux zones cumule
    /// les deux forces.
    fn apply_wind_zones(&mut self, dt: f32, scene: &Scene) {
        let zones: Vec<(Vec3, usize)> = scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.trigger && o.visible)
            .filter_map(|(i, o)| o.wind.map(|w| (w, i)))
            .collect();
        if zones.is_empty() {
            return;
        }
        for &(i, handle) in &self.dynamic {
            let Some(body_obj) = scene.objects.get(i) else {
                continue;
            };
            let mut push = Vec3::ZERO;
            for &(wind, zi) in &zones {
                if zi == i {
                    continue;
                }
                if let Some(zone_obj) = scene.objects.get(zi)
                    && scene.world_aabb_intersects(body_obj, zone_obj)
                {
                    push += wind;
                }
            }
            if push == Vec3::ZERO {
                continue;
            }
            if let Some(body) = self.bodies.get_mut(handle) {
                let v = body.linvel();
                body.set_linvel(v + push * dt, true);
            }
        }
    }

    pub fn step(&mut self, dt: f32, scene: &mut Scene) {
        self.integration.dt = dt.clamp(1.0 / 240.0, 1.0 / 20.0);
        self.apply_wind_zones(dt, scene);
        self.pipeline.step(
            self.gravity,
            &self.integration,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse,
            &mut self.multibody,
            &mut self.ccd,
            &(),
            &(),
        );

        for &(i, handle) in &self.dynamic {
            if let (Some(body), Some(obj)) = (self.bodies.get(handle), scene.objects.get_mut(i)) {
                let t = body.translation();
                obj.transform.position = Vec3::new(t.x, t.y, t.z);
                let r = body.rotation();
                obj.transform.rotation = Quat::from_xyzw(r.x, r.y, r.z, r.w);
            }
        }
        for &(i, handle, _) in &self.kinematic {
            if let (Some(body), Some(obj)) = (self.bodies.get(handle), scene.objects.get_mut(i)) {
                let t = body.translation();
                obj.transform.position = Vec3::new(t.x, t.y, t.z);
                let r = body.rotation();
                obj.transform.rotation = Quat::from_xyzw(r.x, r.y, r.z, r.w);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImportedMesh, Scene, SceneObject};

    /// Décor triangulaire : un seul triangle plat couvrant la moitié « arrière-gauche »
    /// du carré `[-1, 1] × [-1, 1]` (z=0 fixe) — sa boîte englobante est le carré
    /// entier, mais sa silhouette réelle laisse le coin « avant-droit » (x>0, z>0
    /// environ) complètement vide. Un collider `Box`/`Auto` (bounding box) bloquerait
    /// donc n'importe où sur tout le carré ; un `TriMesh`/`ConvexHull` fidèle ne
    /// bloque que sur la moitié réellement couverte.
    fn wedge_scene(shape: ColliderShape) -> Scene {
        use crate::gfx::mesh::{MeshData, Vertex};
        let v = |x: f32, z: f32| Vertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            color: [1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        };
        let data = MeshData {
            vertices: vec![v(-1.0, -1.0), v(1.0, -1.0), v(-1.0, 1.0)],
            // Ordre choisi pour une normale +Y (règle de la main droite) : une boule
            // qui tombe dessus doit heurter la face « du dessus », pas le dos du
            // triangle — l'ordre [0,1,2] donnerait une normale vers -Y, et la boule
            // tomberait au travers malgré un TriMesh construit avec succès.
            indices: vec![0, 2, 1],
        };
        let mut imported = ImportedMesh {
            name: "Coin".into(),
            ..Default::default()
        };
        imported.data = data;
        // `local_aabb` (utilisé par le repli `Auto`/`Box`) lit ces champs directement,
        // pas les vertices de `data` — sans eux, la boîte englobante serait nulle et
        // les deux tests seraient des faux positifs (tout tomberait à travers, y
        // compris le cas `Auto` censé bloquer).
        imported.aabb_min = Vec3::new(-1.0, -0.05, -1.0);
        imported.aabb_max = Vec3::new(1.0, 0.05, 1.0);
        let mut scene = Scene::default();
        scene.imported.push(imported);
        scene.objects.push(SceneObject {
            name: "Décor".into(),
            mesh: crate::scene::MeshKind::Imported(0),
            physics: PhysicsKind::Static,
            collider_shape: shape,
            ..Default::default()
        });
        scene
    }

    /// Départ bas (0.5 m, pas 3 m) : un `TriMesh` n'a pas d'épaisseur, et une boule
    /// qui tombe assez vite peut le traverser en un seul pas de simulation sans jamais
    /// être détectée en collision (tunneling) — la CCD qui corrigerait ça sur un corps
    /// dynamique rapide (`ccd` par objet) est hors sujet ici. Une chute courte reste
    /// assez lente pour ne pas tunneliser, sans avoir besoin d'anticiper ce mécanisme.
    fn drop_ball(scene: &mut Scene, name: &str, x: f32, z: f32) -> usize {
        scene.objects.push(SceneObject {
            name: name.into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(x, 0.5, z))
                .with_scale(Vec3::splat(0.2)),
            physics: PhysicsKind::Dynamic,
            ..Default::default()
        });
        scene.objects.len() - 1
    }

    /// Un décor importé (`TriMesh`) doit bloquer une boule qui tombe sur sa silhouette
    /// réelle, et **laisser tomber** une boule au-dessus d'un coin vide de sa boîte
    /// englobante — la preuve que le collider suit la géométrie, pas juste l'AABB
    /// (`Auto`/`Box` ne suivent que l'AABB).
    #[test]
    fn a_trimesh_collider_follows_the_actual_silhouette_not_the_bounding_box() {
        let mut scene = wedge_scene(ColliderShape::TriMesh);
        let covered = drop_ball(&mut scene, "Boule couverte", -0.5, -0.5);
        let empty_corner = drop_ball(&mut scene, "Boule coin vide", 0.6, 0.6);
        let mut phys = Physics::build(&scene);
        for _ in 0..120 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let y_covered = scene.objects[covered].transform.position.y;
        let y_empty = scene.objects[empty_corner].transform.position.y;
        assert!(
            y_covered > -0.5,
            "au-dessus du triangle, la boule doit être arrêtée près du sol (y={y_covered})"
        );
        assert!(
            y_empty < -1.0,
            "au-dessus du coin vide, la boule doit être passée à travers (y={y_empty})"
        );
    }

    /// Contre-épreuve : **sans** le repli `TriMesh` (`Auto`, la boîte englobante du
    /// triangle), la même boule « coin vide » resterait bloquée — la preuve que le
    /// test précédent mesure bien la fidélité du collider, pas autre chose (ex. une
    /// gravité qui ne s'applique jamais).
    #[test]
    fn without_trimesh_the_bounding_box_wrongly_blocks_the_empty_corner() {
        let mut scene = wedge_scene(ColliderShape::Auto);
        let empty_corner = drop_ball(&mut scene, "Boule coin vide", 0.6, 0.6);
        let mut phys = Physics::build(&scene);
        for _ in 0..120 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let y_empty = scene.objects[empty_corner].transform.position.y;
        assert!(
            y_empty > -1.0,
            "avec un collider en boîte englobante, la boule doit être (à tort) \
             bloquée au-dessus du coin vide (y={y_empty})"
        );
    }

    /// Petit tétraèdre (4 points non coplanaires) : un `ConvexHull` en a besoin pour
    /// un volume 3D bien défini — contrairement au triangle plat de `wedge_scene`,
    /// suffisant pour `TriMesh` (une surface) mais dégénéré comme volume.
    fn tetrahedron_mesh() -> ImportedMesh {
        use crate::gfx::mesh::{MeshData, Vertex};
        let v = |x: f32, y: f32, z: f32| Vertex {
            position: [x, y, z],
            normal: [0.0, 1.0, 0.0],
            color: [1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        };
        let data = MeshData {
            vertices: vec![
                v(-0.2, -0.2, -0.2),
                v(0.2, -0.2, -0.2),
                v(0.0, -0.2, 0.2),
                v(0.0, 0.2, 0.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3, 0, 3, 1, 1, 3, 2],
        };
        let mut imported = ImportedMesh {
            name: "Rocher".into(),
            ..Default::default()
        };
        imported.data = data;
        imported.aabb_min = Vec3::splat(-0.2);
        imported.aabb_max = Vec3::splat(0.2);
        imported
    }

    fn floor_and_falling_rock(shape: ColliderShape) -> Scene {
        let mut scene = Scene::default();
        scene.imported.push(tetrahedron_mesh());
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                .with_scale(Vec3::new(10.0, 1.0, 10.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Rocher".into(),
            mesh: crate::scene::MeshKind::Imported(0),
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.3, 0.0)),
            physics: PhysicsKind::Dynamic,
            collider_shape: shape,
            ..Default::default()
        });
        scene
    }

    /// Second cas : contrairement à `TriMesh` (pas de propriétés de masse), un
    /// `ConvexHull` doit fonctionner sur un corps **dynamique** — c'est tout l'intérêt
    /// de proposer les deux formes plutôt qu'une seule. Un rocher importé tombe sur un
    /// sol et doit s'y arrêter, pas le traverser (ce qui arriverait si
    /// `SharedShape::convex_hull` échouait silencieusement et que le repli `cuboid()`
    /// était lui-même mal dimensionné).
    #[test]
    fn a_convex_hull_collider_works_on_a_dynamic_body() {
        let mut scene = floor_and_falling_rock(ColliderShape::ConvexHull);
        let mut phys = Physics::build(&scene);
        for _ in 0..120 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let y = scene.objects[1].transform.position.y;
        assert!(
            y > -1.5,
            "le rocher (ConvexHull, dynamique) doit se poser sur le sol, pas le \
             traverser (y={y})"
        );
    }

    /// Garde-fou : demander `TriMesh` sur un corps dynamique ne doit ni planter ni
    /// laisser l'objet traverser indéfiniment le décor — `Physics::build` doit se
    /// replier sur `ConvexHull` (cf. le `log::warn!` correspondant), avec le même
    /// comportement observable que le test précédent.
    #[test]
    fn requesting_trimesh_on_a_dynamic_body_falls_back_to_convex_hull() {
        let mut scene = floor_and_falling_rock(ColliderShape::TriMesh);
        let mut phys = Physics::build(&scene);
        for _ in 0..120 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let y = scene.objects[1].transform.position.y;
        assert!(
            y > -1.5,
            "TriMesh sur un corps dynamique doit se replier sur ConvexHull, pas \
             laisser tomber l'objet indéfiniment (y={y})"
        );
    }

    /// Mur fin (5 cm d'épaisseur) + missile positionné juste devant, à `x=5` — cf.
    /// `ccd`. Index 0 = mur, 1 = missile.
    fn missile_and_thin_wall(ccd: bool) -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Mur".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
                .with_scale(Vec3::new(0.05, 2.0, 2.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Missile".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(0.1)),
            physics: PhysicsKind::Dynamic,
            ccd,
            ..Default::default()
        });
        scene
    }

    /// Un missile assez rapide pour traverser un mur fin en un seul pas de simulation
    /// (le même « tunneling » que `drop_ball` évite en partant bas) ne doit plus le
    /// faire une fois `ccd` activé.
    #[test]
    fn ccd_prevents_a_fast_missile_from_tunneling_through_a_thin_wall() {
        let mut scene = missile_and_thin_wall(true);
        let mut phys = Physics::build(&scene);
        phys.set_velocity(1, Vec3::new(200.0, 0.0, 0.0));
        for _ in 0..30 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[1].transform.position.x;
        assert!(
            x < 5.0,
            "avec ccd, le missile doit être arrêté par le mur fin (x={x})"
        );
    }

    /// Contre-épreuve : sans `ccd`, le même missile à la même vitesse traverse le mur
    /// — la preuve que le test précédent mesure bien l'effet de `ccd`, pas autre
    /// chose (ex. un mur mal placé).
    #[test]
    fn without_ccd_the_same_fast_missile_tunnels_through_the_wall() {
        let mut scene = missile_and_thin_wall(false);
        let mut phys = Physics::build(&scene);
        phys.set_velocity(1, Vec3::new(200.0, 0.0, 0.0));
        for _ in 0..30 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[1].transform.position.x;
        assert!(
            x > 5.0,
            "sans ccd, le missile doit traverser le mur fin par tunneling (x={x})"
        );
    }

    /// `collision_mask` doit pouvoir faire ignorer une couche précise — un missile
    /// qui ne collisionne pas la couche du mur (`collision_mask` sans le bit du mur)
    /// doit le traverser à vitesse normale (pas besoin de `ccd` ici, la vitesse reste
    /// modeste).
    #[test]
    fn a_collision_mask_lets_a_projectile_ignore_a_specific_layer() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Mur".into(),
            mesh: crate::scene::MeshKind::Cube,
            // Très haut (pas juste 2 m) : à 3 m/s le missile met ~1,7 s à atteindre le
            // mur, largement assez pour que la gravité le fasse tomber sous un mur de
            // hauteur normale avant d'y arriver — un mur haut isole le test de cet
            // effet, pour ne mesurer que le filtrage par couche.
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
                .with_scale(Vec3::new(0.5, 100.0, 2.0)),
            physics: PhysicsKind::Static,
            collision_layer: 0b010, // couche 2
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Missile".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(0.1)),
            physics: PhysicsKind::Dynamic,
            collision_mask: 0b101, // couches 1 et 3 — pas la couche 2 du mur
            ..Default::default()
        });
        let mut phys = Physics::build(&scene);
        phys.set_velocity(1, Vec3::new(3.0, 0.0, 0.0));
        for _ in 0..120 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[1].transform.position.x;
        assert!(
            x > 5.0,
            "un missile dont le masque exclut la couche du mur doit le traverser (x={x})"
        );
    }

    /// Contre-épreuve : sans réglage de masque (défaut = toutes les couches), le même
    /// missile à la même vitesse est bloqué normalement par le mur.
    #[test]
    fn without_a_mask_the_same_projectile_collides_normally() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Mur".into(),
            mesh: crate::scene::MeshKind::Cube,
            // Très haut : cf. le commentaire équivalent dans le test précédent — sans
            // ça, la gravité ferait passer le missile sous un mur de hauteur normale
            // avant qu'il n'ait le temps de parcourir les 5 m à cette vitesse modeste.
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
                .with_scale(Vec3::new(0.5, 100.0, 2.0)),
            physics: PhysicsKind::Static,
            collision_layer: 0b010,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Missile".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(0.1)),
            physics: PhysicsKind::Dynamic,
            ..Default::default()
        });
        let mut phys = Physics::build(&scene);
        phys.set_velocity(1, Vec3::new(3.0, 0.0, 0.0));
        for _ in 0..120 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[1].transform.position.x;
        assert!(
            x < 5.0,
            "sans masque, le missile doit être bloqué normalement par le mur (x={x})"
        );
    }

    /// Index de l'objet pilotable (`controller.input`) dans la scène.
    fn player_index(scene: &Scene) -> usize {
        scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la démo contrôleur a un joueur pilotable")
    }

    #[test]
    fn controller_demo_player_moves_with_joystick() {
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let x0 = scene.objects[p].transform.position.x;

        let mut phys = Physics::build(&scene);
        // Joystick poussé vers +X (vx = move_speed) pendant ~0,5 s.
        for _ in 0..30 {
            phys.control(p, 4.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x1 = scene.objects[p].transform.position.x;
        assert!(
            x1 > x0 + 0.3,
            "le joueur doit avancer en +X (x0={x0}, x1={x1})"
        );
    }

    #[test]
    fn controller_demo_player_can_jump() {
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        // Laisse le joueur se poser au sol (gravité) avant de sauter.
        for _ in 0..40 {
            phys.control(p, 0.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
            phys.step(1.0 / 60.0, &mut scene);
        }
        let grounded_y = scene.objects[p].transform.position.y;
        // Impulsion de saut (vitesse pour ~1,6 m), puis on relâche.
        let jump_speed = (2.0 * 9.81 * 1.6_f32).sqrt();
        phys.control(p, 0.0, 0.0, true, jump_speed, 0.0, 1.0 / 60.0);
        let mut max_y = grounded_y;
        for _ in 0..25 {
            phys.control(p, 0.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
            phys.step(1.0 / 60.0, &mut scene);
            max_y = max_y.max(scene.objects[p].transform.position.y);
        }
        assert!(
            max_y > grounded_y + 0.3,
            "le joueur doit s'élever en sautant (sol={grounded_y}, max={max_y})"
        );
    }

    #[test]
    fn controller_demo_player_collides_with_wall() {
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        // Le mur de pourtour Est est à x = 7.5 (demi-épaisseur 0.25 → face interne ~7.25).
        let mut phys = Physics::build(&scene);
        // Pousse fort vers +X pendant 3 s : sans mur il sortirait largement de l'aire.
        for _ in 0..180 {
            phys.control(p, 8.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[p].transform.position.x;
        assert!(
            x < 7.2,
            "le joueur doit être bloqué par le mur de pourtour (x≈7), mais x={x}"
        );
    }

    /// Scène « couloir » pour les corps scriptés (`PhysicsKind::Kinematic`) :
    /// sol, mur fixe à x=+4 (face intérieure à 3.75), joueur pilotable (capsule,
    /// donc vrai corps kinématique joueur) à x=−4, et un « marcheur » cubique au
    /// centre dont les tests jouent le rôle du script Lua (écriture directe de
    /// `transform.position`, exactement ce que fait `obj.x = …` côté Lua).
    fn scripted_walker_scene(kind: PhysicsKind) -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                .with_scale(Vec3::new(20.0, 1.0, 20.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Mur".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(4.0, 1.0, 0.0))
                .with_scale(Vec3::new(0.5, 2.0, 4.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(-4.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Marcheur".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
            physics: kind,
            ..Default::default()
        });
        scene
    }

    /// Preuve de la demande gameplay « les créatures ne doivent pas marcher sur
    /// le joueur ni traverser murs et objets fixes » : un corps scripté
    /// (`PhysicsKind::Kinematic`) dont le script force tout droit est bloqué par
    /// le mur, puis par le joueur — sans grimper ni pousser personne.
    #[test]
    fn a_scripted_kinematic_body_cannot_walk_through_walls_or_the_player() {
        let mut scene = scripted_walker_scene(PhysicsKind::Kinematic);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        let walker = 3;
        // 10 s plein est, vers le mur (le script demande 2 m/s quoi qu'il arrive).
        for _ in 0..600 {
            scene.objects[walker].transform.position.x += 2.0 * dt;
            phys.resolve_scripted_moves(dt, &mut scene);
            phys.step(dt, &mut scene);
        }
        let p = scene.objects[walker].transform.position;
        assert!(
            p.x > 2.0,
            "le marcheur doit avoir avancé librement (x={})",
            p.x
        );
        assert!(
            p.x < 3.3,
            "…mais être bloqué par le mur (face intérieure à 3.75, demi-cube 0.5, \
             arrêt attendu ≈ 3.25) : x={}",
            p.x
        );
        assert!(p.y < 1.0, "…sans grimper sur le mur (y={})", p.y);

        // 15 s plein ouest, vers le joueur (capsule en x=−4, rayon 0.5).
        for _ in 0..900 {
            scene.objects[walker].transform.position.x -= 2.0 * dt;
            phys.resolve_scripted_moves(dt, &mut scene);
            phys.step(dt, &mut scene);
        }
        let p = scene.objects[walker].transform.position;
        let joueur = scene.objects[2].transform.position;
        assert!(
            p.x - joueur.x > 0.6,
            "le marcheur doit buter sur le joueur, pas le pénétrer (demi-cube 0.5 \
             + rayon de la capsule : l'écart doit dépasser largement le demi-cube \
             seul) — marcheur x={}, joueur x={}",
            p.x,
            joueur.x
        );
        assert!(p.y < 1.0, "…sans marcher sur le joueur (y={})", p.y);
        assert!(
            (joueur.x + 4.0).abs() < 0.3,
            "un corps kinématique ne doit pas pousser le joueur (x={})",
            joueur.x
        );
    }

    /// Contre-épreuve : le même marcheur **sans** corps physique (`None`, l'état
    /// des créatures avant `PhysicsKind::Kinematic`) traverse le mur comme si de
    /// rien n'était — c'est bien le nouveau variant qui apporte le blocage.
    #[test]
    fn without_kinematic_physics_the_same_scripted_walker_passes_through_the_wall() {
        let mut scene = scripted_walker_scene(PhysicsKind::None);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        let walker = 3;
        for _ in 0..600 {
            scene.objects[walker].transform.position.x += 2.0 * dt;
            phys.resolve_scripted_moves(dt, &mut scene);
            phys.step(dt, &mut scene);
        }
        let x = scene.objects[walker].transform.position.x;
        assert!(
            x > 5.0,
            "sans corps physique, rien ne devrait bloquer le marcheur (x={x})"
        );
    }

    /// Laisse le joueur (corps kinématique, Sprint 103b) se poser réellement au
    /// sol (gravité + `move_shape`/snap au sol) avant de mesurer la maths de
    /// `control` — à l'apparition, la capsule n'est pas encore en contact avec
    /// le sol (`Scene::controller_demo` la fait tomber depuis y=1.0), et un
    /// corps kinématique détecte l'air *réellement* (shapecast) là où l'ancien
    /// corps dynamique se croyait « au sol » dès la première frame (heuristique
    /// de vitesse, toujours vraie à vitesse nulle) — sans se poser d'abord, les
    /// tests ci-dessous mesureraient l'autorité réduite de l'air (`AIR_CONTROL`)
    /// au lieu de celle du sol.
    fn settle_on_ground(phys: &mut Physics, scene: &mut Scene, p: usize) {
        let dt = 1.0 / 60.0;
        for _ in 0..40 {
            phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
            phys.step(dt, scene);
        }
    }

    #[test]
    fn control_with_acceleration_ramps_up_instead_of_snapping_to_target() {
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        settle_on_ground(&mut phys, &mut scene, p);
        // Accélération de 4 m/s² : après un seul pas de 1/60 s, la vitesse ne doit
        // pas déjà valoir la cible (8 m/s) — contrairement à `accel = 0.0` (instantané).
        phys.control(p, 8.0, 0.0, false, 0.0, 4.0, 1.0 / 60.0);
        let vx = phys.velocity(p).unwrap().x;
        assert!(
            vx > 0.0 && vx < 8.0,
            "la vitesse doit monter progressivement, pas instantanément (vx={vx})"
        );
    }

    #[test]
    fn control_brakes_harder_than_it_accelerates() {
        // Le freinage doit décélérer nettement plus vite (`BRAKE_FACTOR`) qu'une
        // accélération de même magnitude ne fait progresser la vitesse depuis
        // l'arrêt — arrêt net quand le joueur relâche, pas une glissade symétrique
        // du départ. Comparaison entre deux scénarios (plutôt qu'une formule
        // figée sur la vitesse absolue) : un corps kinématique subit un léger
        // frottement de contact avec le sol (`KinematicCharacterController`, sans
        // équivalent sur l'ancien corps dynamique) qui décale une vitesse absolue
        // exacte, mais affecte les deux scénarios de façon comparable — le
        // *ratio* freinage/accélération reste la grandeur fiable à tester.
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let dt = 1.0 / 60.0;

        let mut phys_brake = Physics::build(&scene);
        settle_on_ground(&mut phys_brake, &mut scene, p);
        phys_brake.control(p, 8.0, 0.0, false, 0.0, 0.0, dt);
        let v1 = phys_brake.velocity(p).unwrap().x;
        phys_brake.control(p, 0.0, 0.0, false, 0.0, 20.0, dt);
        let brake_delta = v1 - phys_brake.velocity(p).unwrap().x;

        let mut scene2 = Scene::controller_demo();
        let mut phys_accel = Physics::build(&scene2);
        settle_on_ground(&mut phys_accel, &mut scene2, p);
        phys_accel.control(p, 8.0, 0.0, false, 0.0, 20.0, dt);
        let accel_delta = phys_accel.velocity(p).unwrap().x;

        assert!(
            brake_delta > accel_delta * (BRAKE_FACTOR * 0.75),
            "le freinage (Δ={brake_delta}) doit décélérer nettement plus vite \
             que l'accélération (Δ={accel_delta}) ne progresse (facteur \
             attendu ≈ {BRAKE_FACTOR})"
        );
    }

    #[test]
    fn control_has_reduced_authority_in_the_air() {
        // En l'air (saut en cours), l'accélération horizontale doit être réduite à
        // `AIR_CONTROL` : la trajectoire d'un saut s'engage à l'impulsion, elle ne se
        // repilote pas librement comme au sol (effet « téléguidé » sinon).
        let scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        // Saut : vitesse verticale nette (5 m/s) → plus « au sol » pour l'appel suivant.
        phys.control(p, 0.0, 0.0, true, 5.0, 0.0, dt);
        phys.control(p, 8.0, 0.0, false, 0.0, 20.0, dt);
        let vx = phys.velocity(p).unwrap().x;
        let expected = 20.0 * AIR_CONTROL * dt;
        assert!(
            (vx - expected).abs() < 1e-4,
            "en l'air, l'accélération doit être ×{AIR_CONTROL} (vx={vx}, attendu={expected})"
        );
    }

    #[test]
    fn control_makes_falling_faster_than_rising() {
        // Gravité renforcée en descente (`FALL_GRAVITY_FACTOR`) : un saut retombe
        // plus vite qu'il ne monte — saut vif et lisible, pas une parabole flottante.
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        // Se pose au sol, saute, puis laisse la simulation courir jusqu'à la chute.
        for _ in 0..40 {
            phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        phys.control(p, 0.0, 0.0, true, 6.0, 0.0, dt);
        for _ in 0..200 {
            if phys.velocity(p).unwrap().y < -1.5 {
                break;
            }
            phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        let vy_before = phys.velocity(p).unwrap().y;
        assert!(
            vy_before < -1.5,
            "le joueur doit être en chute (vy={vy_before})"
        );
        // Un appel `control` seul (sans pas de simulation) doit appliquer la
        // gravité de chute renforcée en un seul coup (pas de solveur `step`
        // séparé pour un corps kinématique, cf. `control_kinematic`).
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
        let vy_after = phys.velocity(p).unwrap().y;
        let boost = 9.81 * FALL_GRAVITY_FACTOR * dt;
        assert!(
            (vy_before - vy_after - boost).abs() < 1e-3,
            "la chute doit être accélérée de {boost} m/s par pas (avant={vy_before}, après={vy_after})"
        );
    }

    #[test]
    fn control_with_zero_acceleration_snaps_instantly_as_before() {
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        settle_on_ground(&mut phys, &mut scene, p);
        phys.control(p, 8.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
        let vx = phys.velocity(p).unwrap().x;
        assert!(
            (vx - 8.0).abs() < 0.05,
            "vx doit valoir la cible à peu près instantanément, vx={vx}"
        );
    }

    fn kinematic_player(pos: Vec3) -> SceneObject {
        SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(pos),
            controller: Some(crate::scene::Controller {
                input: true,
                move_speed: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Livrable du Sprint 103b (« escalier montable ») : un escalier de 4 marches
    /// de 20 cm de haut (sous `PLAYER_AUTOSTEP_HEIGHT` = 30 cm) doit être franchi
    /// sans ralentir en butant contre chaque contremarche — la preuve que
    /// `KinematicCharacterController::autostep` fait le travail que l'ancienne
    /// heuristique de vitesse (`cur.y.abs() < 1.0`, sans aucune notion de forme du
    /// sol) ne pouvait pas faire.
    #[test]
    fn kinematic_player_climbs_a_low_staircase() {
        const STEP_RISE: f32 = 0.2;
        const STEP_DEPTH: f32 = 0.6;
        const STEPS: i32 = 4;

        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -0.1, -1.5))
                .with_scale(Vec3::new(4.0, 0.2, 3.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        for k in 0..STEPS {
            let top = STEP_RISE * (k + 1) as f32;
            scene.objects.push(SceneObject {
                name: format!("Marche {k}"),
                mesh: crate::scene::MeshKind::Cube,
                transform: crate::scene::Transform::from_pos(Vec3::new(
                    0.0,
                    top * 0.5,
                    (k as f32 + 0.5) * STEP_DEPTH,
                ))
                .with_scale(Vec3::new(4.0, top, STEP_DEPTH)),
                physics: PhysicsKind::Static,
                ..Default::default()
            });
        }
        // Palier au sommet : sans lui, le joueur (avancée constante en +Z) finit
        // par dépasser le bord de la dernière marche et tombe dans le vide — ce
        // test vérifie l'ascension, pas la chute derrière l'escalier.
        let top_step = STEP_RISE * STEPS as f32;
        scene.objects.push(SceneObject {
            name: "Palier".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(
                0.0,
                top_step * 0.5,
                STEPS as f32 * STEP_DEPTH + 1.0,
            ))
            .with_scale(Vec3::new(4.0, top_step, 2.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene
            .objects
            .push(kinematic_player(Vec3::new(0.0, 1.0, -1.5)));
        let p = scene.objects.len() - 1;

        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        for _ in 0..200 {
            phys.control(p, 0.0, 2.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        let pos = scene.objects[p].transform.position;
        assert!(
            pos.z > STEP_DEPTH * STEPS as f32 - 1.0,
            "le joueur doit avoir avancé jusqu'au sommet de l'escalier (z={})",
            pos.z
        );
        assert!(
            pos.y > top_step - STEP_RISE * 1.5,
            "le joueur doit être monté sur les marches (y={}, sommet={})",
            pos.y,
            top_step
        );
    }

    /// Décor en pente : un plan incliné statique (rotation autour de X), du bas
    /// (z négatif, y≈0) vers le haut (z positif). `angle_deg` positif fait monter
    /// la pente en +Z, direction dans laquelle le joueur avance dans les tests.
    fn ramp_scene(angle_deg: f32) -> (Scene, usize) {
        let theta = -angle_deg.to_radians();
        let mut scene = Scene::default();
        // Sol plat avant le bas de la rampe (bord bas ≈ z=-2.7, cf. commentaire
        // ci-dessous) — sans lui, le joueur tombe dans le vide avant même
        // d'atteindre la rampe.
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -0.1, -4.5))
                .with_scale(Vec3::new(4.0, 0.2, 3.5)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Rampe".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform {
                position: Vec3::new(0.0, angle_deg.to_radians().sin() * 3.0, 0.0),
                rotation: Quat::from_rotation_x(theta),
                scale: Vec3::new(4.0, 0.2, 6.0),
            },
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        // Départ à plat, juste avant le bas de la rampe (bord bas ≈ z=-2.7 pour
        // une demi-longueur de 3 m, cf. calcul géométrique en commentaire du test).
        scene
            .objects
            .push(kinematic_player(Vec3::new(0.0, 1.0, -4.0)));
        let p = scene.objects.len() - 1;
        (scene, p)
    }

    /// Pente franchissable (25°, sous `PLAYER_MAX_SLOPE_CLIMB_DEG` = 50°) : le
    /// joueur doit rester au contact et monter avec elle, pas rebondir/tunneler.
    #[test]
    fn kinematic_player_climbs_a_gentle_slope() {
        // 220 pas (~3,7 s) : le joueur atteint le haut de la rampe (~z=2,7) sans
        // la dépasser — au-delà, il marcherait dans le vide derrière la rampe,
        // ce que ce test ne vérifie pas.
        let (mut scene, p) = ramp_scene(25.0);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        for _ in 0..220 {
            phys.control(p, 0.0, 2.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        let pos = scene.objects[p].transform.position;
        assert!(
            pos.y > 2.0,
            "le joueur doit avoir grimpé une pente franchissable (y={})",
            pos.y
        );
        assert!(
            pos.z > -1.0,
            "le joueur doit avoir avancé sur la pente (z={})",
            pos.z
        );
    }

    /// Contre-épreuve : une pente trop raide (65°, au-delà de
    /// `PLAYER_MAX_SLOPE_CLIMB_DEG`/`PLAYER_MIN_SLOPE_SLIDE_DEG`) ne doit pas se
    /// gravir comme la précédente — le joueur reste bloqué en bas / glisse,
    /// loin de la hauteur atteinte sur la pente franchissable.
    #[test]
    fn kinematic_player_cannot_climb_a_steep_slope() {
        let (mut scene, p) = ramp_scene(65.0);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        for _ in 0..360 {
            phys.control(p, 0.0, 2.0, false, 0.0, 0.0, dt);
            phys.step(dt, &mut scene);
        }
        let pos = scene.objects[p].transform.position;
        assert!(
            pos.y < 0.8,
            "une pente trop raide ne doit pas être gravie comme une pente \
             franchissable (y={})",
            pos.y
        );
    }

    /// Sol plat (index 0) + mur vertical (index 1), tous deux statiques — sert aux
    /// tests de `raycast`/`overlap_sphere` (`QueryPipeline`).
    fn ground_and_wall_scene() -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                .with_scale(Vec3::new(10.0, 1.0, 10.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Mur".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
                .with_scale(Vec3::new(0.5, 2.0, 2.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene
    }

    /// `raycast` doit trouver le collider le plus proche sur la trajectoire et
    /// identifier l'objet touché — brique du « capteur de sol » (rayon vers le bas)
    /// et du « cône de vision » (ligne de vue vers une cible).
    #[test]
    fn raycast_hits_the_nearest_collider_and_reports_its_object_index() {
        let scene = ground_and_wall_scene();
        let phys = Physics::build(&scene);
        // Vers le bas depuis 5 m au-dessus du sol (demi-épaisseur 0.5, face haute à
        // y=-0.5) : capteur de sol typique.
        let hit = phys
            .raycast(
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                100.0,
                u32::MAX,
            )
            .expect("le rayon vers le bas doit toucher le sol");
        assert_eq!(hit.index, Some(0), "doit identifier l'objet « Sol »");
        assert!(
            (hit.distance - 5.5).abs() < 0.05,
            "distance attendue ~5.5 m (dist={})",
            hit.distance
        );
        assert!(
            (hit.point.y - -0.5).abs() < 0.05,
            "le point d'impact doit être sur la face haute du sol (y={})",
            hit.point.y
        );

        // Vers +X depuis l'origine : ligne de vue vers le mur.
        let hit_wall = phys
            .raycast(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 100.0, u32::MAX)
            .expect("le rayon vers le mur doit toucher quelque chose");
        assert_eq!(hit_wall.index, Some(1), "doit identifier l'objet « Mur »");

        // Vers le haut : rien au-dessus des deux objets, aucun impact.
        assert!(
            phys.raycast(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 100.0, u32::MAX)
                .is_none(),
            "un rayon vers le ciel ne doit rien toucher"
        );
    }

    /// Contre-épreuve : direction nulle → `None` sans diviser par zéro (`try_normalize`).
    #[test]
    fn raycast_with_a_zero_direction_returns_none_instead_of_panicking() {
        let scene = ground_and_wall_scene();
        let phys = Physics::build(&scene);
        assert!(
            phys.raycast(Vec3::ZERO, Vec3::ZERO, 100.0, u32::MAX)
                .is_none()
        );
    }

    /// `mask` doit filtrer les colliders par couche, mêmes bits que
    /// `collision_layer`/`collision_mask` — un rayon ne doit toucher que les colliders
    /// dont la couche recoupe le masque demandé.
    #[test]
    fn raycast_mask_filters_by_collision_layer() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Mur".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.0, 0.0))
                .with_scale(Vec3::new(0.5, 2.0, 2.0)),
            physics: PhysicsKind::Static,
            collision_layer: 0b010, // couche 2
            ..Default::default()
        });
        let phys = Physics::build(&scene);
        let origin = Vec3::ZERO;
        let dir = Vec3::new(1.0, 0.0, 0.0);
        assert!(
            phys.raycast(origin, dir, 100.0, 0b010).is_some(),
            "un masque incluant la couche du mur doit le toucher"
        );
        assert!(
            phys.raycast(origin, dir, 100.0, 0b101).is_none(),
            "un masque excluant la couche du mur ne doit rien toucher"
        );
    }

    /// `overlap_sphere` doit détecter les colliders à portée et ignorer ceux hors de
    /// la sphère — brique du « cône de vision » (détection de proximité avant même de
    /// tester l'angle/la ligne de vue).
    #[test]
    fn overlap_sphere_finds_colliders_within_radius_and_ignores_far_ones() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Proche".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.0, 0.0))
                .with_scale(Vec3::splat(0.2)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Loin".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(20.0, 0.0, 0.0))
                .with_scale(Vec3::splat(0.2)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        let phys = Physics::build(&scene);

        let near_only = phys.overlap_sphere(Vec3::ZERO, 2.0, u32::MAX);
        assert_eq!(
            near_only,
            vec![0],
            "seul l'objet proche doit être détecté (trouvé={near_only:?})"
        );

        let mut both = phys.overlap_sphere(Vec3::ZERO, 25.0, u32::MAX);
        both.sort_unstable();
        assert_eq!(
            both,
            vec![0, 1],
            "un rayon suffisant doit détecter les deux objets (trouvé={both:?})"
        );
    }

    /// Même filtrage par couche que `raycast` (mêmes bits `collision_layer`/`mask`).
    #[test]
    fn overlap_sphere_mask_filters_by_collision_layer() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Capteur".into(),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.0, 0.0))
                .with_scale(Vec3::splat(0.2)),
            physics: PhysicsKind::Static,
            collision_layer: 0b010, // couche 2
            ..Default::default()
        });
        let phys = Physics::build(&scene);
        assert_eq!(
            phys.overlap_sphere(Vec3::ZERO, 2.0, 0b010),
            vec![0],
            "un masque incluant la couche du capteur doit le détecter"
        );
        assert!(
            phys.overlap_sphere(Vec3::ZERO, 2.0, 0b101).is_empty(),
            "un masque excluant la couche du capteur ne doit rien détecter"
        );
    }

    /// Sprint 125 : zone de vent — un corps dynamique dont l'AABB touche celle d'une
    /// zone `trigger` + `wind` doit dériver dans la direction du vent ; un corps hors
    /// de la zone garde son comportement normal (chute verticale, pas de dérive
    /// horizontale) — la preuve que la force est bien **locale** à la zone, pas globale.
    #[test]
    fn a_wind_zone_pushes_a_dynamic_body_only_while_inside_its_aabb() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Vent".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(10.0)),
            physics: PhysicsKind::None,
            trigger: true,
            wind: Some(Vec3::new(4.0, 0.0, 0.0)),
            ..Default::default()
        });
        let inside = drop_ball(&mut scene, "Dedans", 0.0, 0.0);
        let outside = drop_ball(&mut scene, "Dehors", 20.0, 0.0);
        let mut phys = Physics::build(&scene);
        for _ in 0..30 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x_inside = scene.objects[inside].transform.position.x;
        let x_outside = scene.objects[outside].transform.position.x;
        assert!(
            x_inside > 0.5,
            "poussée par le vent attendue en x, x={x_inside}"
        );
        assert!(
            (x_outside - 20.0).abs() < 0.05,
            "hors de la zone, aucune dérive horizontale attendue, x={x_outside}"
        );
    }

    /// Contre-épreuve : sans `trigger`, un `wind` renseigné ne pousse personne — la
    /// zone n'a alors aucun volume de détection (cohérent avec les autres zones,
    /// `obj.triggered`).
    #[test]
    fn a_wind_zone_without_trigger_pushes_nobody() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Vent sans trigger".into(),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO).with_scale(Vec3::splat(10.0)),
            physics: PhysicsKind::None,
            trigger: false,
            wind: Some(Vec3::new(4.0, 0.0, 0.0)),
            ..Default::default()
        });
        let ball = drop_ball(&mut scene, "Dedans", 0.0, 0.0);
        let mut phys = Physics::build(&scene);
        for _ in 0..30 {
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[ball].transform.position.x;
        assert!(x.abs() < 0.05, "aucune dérive horizontale attendue, x={x}");
    }

    /// Sprint 103c : une correction de position réseau (`set_position`) ne
    /// doit pas laisser le joueur figé « au sol » un tick de plus après avoir
    /// été téléporté en l'air — la gravité doit reprendre dès le prochain
    /// `control`, pas seulement après un tick de retard (cf. le correctif de
    /// `set_position`, qui remet `grounded` à `false`).
    #[test]
    fn set_position_does_not_trust_a_stale_grounded_state() {
        let mut scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        let dt = 1.0 / 60.0;
        settle_on_ground(&mut phys, &mut scene, p);
        assert!(
            phys.velocity(p).unwrap().y.abs() < 1e-6,
            "posé au sol, la vitesse verticale doit être nulle avant le test"
        );

        // Téléporte loin en l'air, bien au-dessus de tout support — simule une
        // correction réseau qui déplace le joueur hors de portée du sol connu.
        phys.set_position(p, Vec3::new(0.0, 20.0, -6.0));
        phys.control(p, 0.0, 0.0, false, 0.0, 0.0, dt);
        let vy = phys.velocity(p).unwrap().y;
        assert!(
            vy < 0.0,
            "la gravité doit s'appliquer dès le premier `control` après une \
             téléportation, pas seulement au tick suivant (vy={vy})"
        );
    }
}
