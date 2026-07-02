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
            // Un objet pilotable (joystick/gyro/saut) OU une IA poursuivante devient un
            // corps dynamique, même sans physique explicite, pour entrer en collision
            // avec le décor — les deux sont « pilotés » par `Physics::control` (le joueur
            // par l'entrée, l'IA par la direction vers le joueur, cf. `App::advance_play`).
            let controllable = obj.controller.as_ref().is_some_and(|c| c.input || c.gyro)
                || obj.ai_chaser.is_some();
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
            .restitution(0.5)
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

        Physics {
            bodies,
            colliders,
            gravity: Vector::new(0.0, -9.81, 0.0),
            integration: IntegrationParameters::default(),
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

    /// Pilote un objet (corps `controlled`) : fixe la vitesse horizontale (joystick/gyro)
    /// et déclenche un saut si demandé **et** que l'objet est au sol. La vitesse verticale
    /// est sinon conservée (gravité). `jump_speed` = vitesse initiale du saut (m/s).
    /// Renvoie `true` si un **saut** a effectivement été déclenché (objet au sol).
    pub fn control(&mut self, index: usize, vx: f32, vz: f32, jump: bool, jump_speed: f32) -> bool {
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
                body.set_linvel(Vector::new(vx, vy, vz), true);
                jumped |= do_jump;
            }
        }
        jumped
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
            phys.control(p, 4.0, 0.0, false, 0.0);
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
            phys.control(p, 0.0, 0.0, false, 0.0);
            phys.step(1.0 / 60.0, &mut scene);
        }
        let grounded_y = scene.objects[p].transform.position.y;
        // Impulsion de saut (vitesse pour ~1,6 m), puis on relâche.
        let jump_speed = (2.0 * 9.81 * 1.6_f32).sqrt();
        phys.control(p, 0.0, 0.0, true, jump_speed);
        let mut max_y = grounded_y;
        for _ in 0..25 {
            phys.control(p, 0.0, 0.0, false, 0.0);
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
            phys.control(p, 8.0, 0.0, false, 0.0);
            phys.step(1.0 / 60.0, &mut scene);
        }
        let x = scene.objects[p].transform.position.x;
        assert!(
            x < 7.2,
            "le joueur doit être bloqué par le mur de pourtour (x≈7), mais x={x}"
        );
    }
}
