//! Monde physique rapier3d, construit à l'entrée en mode Play.
//! Mappe les objets de la scène vers des corps rigides et recopie les poses.

use glam::{Quat, Vec3};
use rapier3d::prelude::*;

use crate::scene::{MeshKind, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PhysicsKind {
    None,
    Static,
    Dynamic,
}

/// Forme du collider en mode Play. `Auto` = déduite du mesh ; sinon forcée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColliderShape {
    #[default]
    Auto,
    Box,
    Sphere,
    Capsule,
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
    /// (index d'objet, handle) pour les objets **pilotables** (joystick/gyro/saut).
    controlled: Vec<(usize, RigidBodyHandle)>,
}

impl Physics {
    /// Construit le monde à partir des objets ayant un type de physique.
    pub fn build(scene: &Scene) -> Physics {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut dynamic = Vec::new();
        let mut controlled = Vec::new();

        for (i, obj) in scene.objects.iter().enumerate() {
            // Un objet pilotable (joystick/gyro/saut) OU une IA poursuivante **visible**
            // devient un corps dynamique, même sans physique explicite, pour entrer en
            // collision avec le décor — les deux sont « pilotés » par `Physics::control`
            // (le joueur par l'entrée, l'IA par la direction vers le joueur, cf.
            // `App::advance_play`). Un chasseur masqué (manche pas encore révélée, ou
            // vaincu) n'a pas de corps : sinon son collider bloquerait le joueur alors
            // qu'il est invisible (cf. `App::init_waves`/`update_waves`).
            let controllable = obj.controller.as_ref().is_some_and(|c| c.input || c.gyro)
                || (obj.ai_chaser.is_some() && obj.visible);
            let is_dynamic = match obj.physics {
                PhysicsKind::None if !controllable => continue,
                PhysicsKind::Dynamic => true,
                _ => controllable,
            };

            let t = &obj.transform;
            let (axis, angle) = t.rotation.to_axis_angle();
            let rotvec = axis * angle;

            let mut builder = if is_dynamic {
                RigidBodyBuilder::dynamic()
            } else {
                RigidBodyBuilder::fixed()
            };
            // Objet pilotable : on bloque les rotations pour qu'il reste debout.
            if controllable {
                builder = builder.lock_rotations();
            }
            let body = builder
                .translation(Vector::new(t.position.x, t.position.y, t.position.z))
                .rotation(Vector::new(rotvec.x, rotvec.y, rotvec.z))
                .build();
            let handle = bodies.insert(body);

            // demi-dimensions du collider : AABB local mis à l'échelle
            let (lmin, lmax) = scene.local_aabb(obj.mesh);
            let he = (lmax - lmin) * 0.5 * t.scale;
            let cuboid = || {
                ColliderBuilder::cuboid(
                    he.x.abs().max(0.01),
                    he.y.abs().max(0.01),
                    he.z.abs().max(0.01),
                )
            };
            let ball = || ColliderBuilder::ball(he.x.abs().max(he.z.abs()).max(0.01));
            let capsule = || {
                let r = he.x.abs().max(he.z.abs()).max(0.01);
                let half = (he.y.abs() - r).max(0.01);
                ColliderBuilder::capsule_y(half, r)
            };
            // Forme explicite si demandée, sinon déduite du mesh.
            let collider = match obj.collider_shape {
                ColliderShape::Box => cuboid(),
                ColliderShape::Sphere => ball(),
                ColliderShape::Capsule => capsule(),
                ColliderShape::Auto => match obj.mesh {
                    MeshKind::Sphere => ball(),
                    MeshKind::Capsule => capsule(),
                    MeshKind::Cylinder => {
                        ColliderBuilder::cylinder(he.y.abs().max(0.01), he.x.abs().max(0.01))
                    }
                    _ => cuboid(),
                },
            }
            // Aucun rebond (0.0) : un personnage n'est pas une balle — à 0.5
            // (valeur précédente), chaque atterrissage/contact avec un mur ou un
            // autre joueur renvoyait la moitié de la vitesse d'impact, donnant un
            // mouvement instable qui « bug » visuellement (constaté en test réel,
            // 2026-07-12 : « comme une boule qui bug, pas fluide »). Rien dans le
            // projet ne dépend d'un rebond (aucun mécanisme de type trampoline).
            .restitution(0.0)
            .friction(0.6)
            .build();
            colliders.insert_with_parent(collider, handle, &mut bodies);

            if is_dynamic {
                dynamic.push((i, handle));
            }
            if controllable {
                controlled.push((i, handle));
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
        }
    }

    /// Pilote un objet (corps `controlled`) : fait tendre la vitesse horizontale vers
    /// `(vx, vz)` (joystick/gyro) et déclenche un saut si demandé **et** que l'objet est
    /// au sol. La vitesse verticale est sinon conservée (gravité). `jump_speed` = vitesse
    /// initiale du saut (m/s). `accel` (m/s²) borne la variation de vitesse horizontale
    /// par seconde — `0.0` fixe la vitesse instantanément (comportement historique,
    /// utilisé par l'IA/le recul qui n'ont pas besoin d'inertie). Une valeur positive
    /// (mouvement du joueur, cf. `Controller::acceleration`) lisse départs et arrêts au
    /// lieu d'un « on/off » robotique (demandé le 2026-07-12). Renvoie `true` si un
    /// **saut** a effectivement été déclenché (objet au sol).
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
        let mut jumped = false;
        for &(i, handle) in &self.controlled {
            if i != index {
                continue;
            }
            if let Some(body) = self.bodies.get_mut(handle) {
                let cur = body.linvel();
                // Au sol : vitesse verticale quasi nulle (heuristique simple, sans raycast).
                let grounded = cur.y.abs() < 1.0;
                let do_jump = jump && grounded;
                let vy = if do_jump { jump_speed } else { cur.y };
                let (nx, nz) = if accel > 0.0 {
                    let dx = vx - cur.x;
                    let dz = vz - cur.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    let max_step = accel * dt;
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

    /// Force la position du corps rigide (dynamique) de l'objet `index`, sans
    /// effet s'il n'en a pas (objet statique/sans physique) — utilisé par la
    /// réconciliation réseau du joueur local (`app::network_client::apply_
    /// local_network_position`, Sprint 66bis, `SPRINTNETWORK.md`).
    ///
    /// **Nécessaire, pas cosmétique** : `step` recopie la pose du corps
    /// rigide dans `scene.objects[index].transform` à *chaque* appel (sync à
    /// sens unique physique → transform, jamais l'inverse) — écrire
    /// directement dans `transform.position` sans passer par cette méthode
    /// n'a donc d'effet que pour la frame courante ; `step` l'écrase dès le
    /// tick suivant avec la position du corps rigide, resté inchangé. Bug
    /// réel trouvé en testant l'app réellement (capture d'écran utilisateur :
    /// personnage qui semble dupliqué/trembler entre deux points, la
    /// correction n'ayant jamais persisté au-delà d'une frame).
    pub fn set_position(&mut self, index: usize, pos: Vec3) {
        if let Some(&(_, handle)) = self.dynamic.iter().find(|&&(i, _)| i == index)
            && let Some(body) = self.bodies.get_mut(handle)
        {
            body.set_translation(Vector::new(pos.x, pos.y, pos.z), true);
        }
    }

    /// Avance la simulation de `dt` et recopie les poses des corps dynamiques.
    pub fn step(&mut self, dt: f32, scene: &mut Scene) {
        self.integration.dt = dt.clamp(1.0 / 240.0, 1.0 / 20.0);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

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

    #[test]
    fn control_with_acceleration_ramps_up_instead_of_snapping_to_target() {
        let scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        // Accélération de 4 m/s² : après un seul pas de 1/60 s, la vitesse ne doit
        // pas déjà valoir la cible (8 m/s) — contrairement à `accel = 0.0` (instantané).
        phys.control(p, 8.0, 0.0, false, 0.0, 4.0, 1.0 / 60.0);
        let handle = phys.controlled.iter().find(|&&(i, _)| i == p).unwrap().1;
        let vx = phys.bodies.get(handle).unwrap().linvel().x;
        assert!(
            vx > 0.0 && vx < 8.0,
            "la vitesse doit monter progressivement, pas instantanément (vx={vx})"
        );
    }

    #[test]
    fn control_with_zero_acceleration_snaps_instantly_as_before() {
        let scene = Scene::controller_demo();
        let p = player_index(&scene);
        let mut phys = Physics::build(&scene);
        phys.control(p, 8.0, 0.0, false, 0.0, 0.0, 1.0 / 60.0);
        let handle = phys.controlled.iter().find(|&&(i, _)| i == p).unwrap().1;
        let vx = phys.bodies.get(handle).unwrap().linvel().x;
        assert!((vx - 8.0).abs() < 1e-5, "vx doit valoir la cible, vx={vx}");
    }
}
