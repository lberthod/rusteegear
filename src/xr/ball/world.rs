//! Monde physique de Ball : décor fixe, blocs (cibles, bois, pierre),
//! éléments mobiles cinématiques, boules et mains. Coordonnées **monde** ;
//! un parcours pose ses trous les uns derrière les autres (`course`).

use std::collections::VecDeque;

use glam::{Quat, Vec3, Vec4};
use rapier3d::prelude::*;

use super::hands::{Grab, Grabber, HandFrame};
use crate::xr::input::Haptic;

/// Pas fixe de la physique (s) ; au plus 4 pas par image.
pub const PHYSICS_DT: f32 = 1.0 / 90.0;
pub const BALL_RADIUS: f32 = 0.055;
const MAX_BALLS: usize = 16;
/// Gain appliqué à la vitesse de la main au lâcher : sans retour de force, un
/// lancer à vitesse réelle paraît mou.
pub const THROW_GAIN: f32 = 1.4;
/// Vitesse minimale (m/s) pour qu'un lâcher compte comme un coup.
const STROKE_SPEED: f32 = 1.2;
/// Une cible est « tombée » quand elle est descendue de ce qu'elle était.
pub const TARGET_DROP: f32 = 0.3;

// Groupes de collision. Les mains ne touchent QUE les blocs : jamais les
// boules — sinon, à l'ouverture de la main, la boule lâchée (redevenue
// dynamique) heurte les doigts encore autour d'elle et tombe au lieu de
// partir (retour du 1er test au casque).
const G_WORLD: Group = Group::GROUP_1;
const G_BLOCK: Group = Group::GROUP_2;
const G_BALL: Group = Group::GROUP_3;
const G_HAND: Group = Group::GROUP_4;
/// Joueur PC (avatar et murs) : n'arrête que les boules, ne touche jamais
/// les blocs — il défend, il ne peut pas marquer à la place du joueur VR.
const G_REMOTE: Group = Group::GROUP_5;

/// Mur du joueur PC : 60 cm de large, 1,6 m de haut, posé au sol.
pub const WALL_HALF: Vec3 = Vec3::new(0.3, 0.8, 0.03);
/// Corps du joueur PC (boîte grossière, pieds au sol).
pub const AVATAR_HALF: Vec3 = Vec3::new(0.22, 0.85, 0.15);
pub const MAX_WALLS: usize = 2;

fn groups(member: Group, filter: Group) -> InteractionGroups {
    InteractionGroups::new(member, filter, InteractionTestMode::And)
}

/// Matière d'un bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    /// À faire tomber : compte pour finir le trou.
    Target,
    /// Structure légère (socles de pyramide, murets qui s'écroulent).
    Wood,
    /// Lourde, presque inébranlable : protège les cibles.
    Stone,
}

impl Material {
    pub fn color(self, index: usize) -> Vec4 {
        match self {
            Self::Target => [
                Vec4::new(0.95, 0.28, 0.22, 1.0),
                Vec4::new(1.0, 0.55, 0.12, 1.0),
            ][index % 2],
            Self::Wood => [
                Vec4::new(0.72, 0.52, 0.32, 1.0),
                Vec4::new(0.64, 0.45, 0.27, 1.0),
            ][index % 2],
            Self::Stone => Vec4::new(0.5, 0.52, 0.56, 1.0),
        }
    }

    fn density(self) -> f32 {
        match self {
            Self::Target | Self::Wood => 350.0,
            Self::Stone => 4000.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub body: RigidBodyHandle,
    pub half: Vec3,
    pub material: Material,
    pub spawn: Vec3,
    pub color: Vec4,
}

/// Mouvement d'un élément cinématique (plateau, mur, bras).
#[derive(Debug, Clone, Copy)]
pub enum Motion {
    /// Aller-retour sinusoïdal entre `a` et `b` en `period` secondes.
    Slide { a: Vec3, b: Vec3, period: f32 },
    /// Rotation autour de l'axe vertical passant par son centre (rad/s).
    Spin { center: Vec3, speed: f32 },
}

#[derive(Debug, Clone)]
pub struct Mover {
    pub body: RigidBodyHandle,
    pub half: Vec3,
    pub motion: Motion,
    pub color: Vec4,
    /// Instant de pose : le mouvement part de là, à l'arrêt (sinon des blocs
    /// posés immobiles sur un plateau déjà lancé glissent ou basculent).
    t0: f32,
}

impl Motion {
    fn pose(&self, t: f32) -> (Vec3, Quat) {
        match *self {
            Self::Slide { a, b, period } => {
                let k = 0.5 - 0.5 * (t * std::f32::consts::TAU / period).cos();
                (a.lerp(b, k), Quat::IDENTITY)
            }
            Self::Spin { center, speed } => (center, Quat::from_rotation_y(speed * t)),
        }
    }
}

/// Décor fixe dessiné : (centre, demi-tailles, couleur).
pub type Static = (Vec3, Vec3, Vec4);

pub struct World {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: DefaultBroadPhase,
    narrow: NarrowPhase,
    impulse: ImpulseJointSet,
    multibody: MultibodyJointSet,
    ccd: CCDSolver,
    integration: IntegrationParameters,
    pub statics: Vec<Static>,
    pub blocks: Vec<Block>,
    pub movers: Vec<Mover>,
    pub balls: VecDeque<RigidBodyHandle>,
    hand_bodies: [[RigidBodyHandle; 6]; 2],
    /// Avatar puis murs du joueur PC (rangés sous le sol quand absents).
    remote_bodies: [RigidBodyHandle; 1 + MAX_WALLS],
    /// Pose voulue de chacun (`None` = absent).
    pub remote_poses: [Option<(Vec3, f32)>; 1 + MAX_WALLS],
    /// Boules déjà arrêtées par le joueur PC (comptées une fois).
    stopped: std::collections::HashSet<RigidBodyHandle>,
    /// Tirs arrêtés par le joueur PC depuis le début du parcours.
    pub blocked: u32,
    pub grabbers: [Grabber; 2],
    /// Temps de simulation (s) : anime les éléments mobiles.
    pub time: f32,
    accumulator: f32,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        let mut w = Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: DefaultBroadPhase::new(),
            narrow: NarrowPhase::new(),
            impulse: ImpulseJointSet::new(),
            multibody: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            integration: IntegrationParameters {
                dt: PHYSICS_DT,
                ..Default::default()
            },
            statics: Vec::new(),
            blocks: Vec::new(),
            movers: Vec::new(),
            balls: VecDeque::new(),
            hand_bodies: [[RigidBodyHandle::invalid(); 6]; 2],
            remote_bodies: [RigidBodyHandle::invalid(); 1 + MAX_WALLS],
            remote_poses: [None; 1 + MAX_WALLS],
            stopped: Default::default(),
            blocked: 0,
            grabbers: Default::default(),
            time: 0.0,
            accumulator: 0.0,
        };
        for hand in 0..2 {
            for k in 0..6 {
                let body = w.bodies.insert(
                    RigidBodyBuilder::kinematic_position_based()
                        .translation(Vec3::new(0.0, -100.0 - k as f32, 0.0))
                        .build(),
                );
                let r = if k == 0 { 0.035 } else { 0.012 };
                let collider = ColliderBuilder::ball(r)
                    .collision_groups(groups(G_HAND, G_BLOCK))
                    .build();
                w.colliders
                    .insert_with_parent(collider, body, &mut w.bodies);
                w.hand_bodies[hand][k] = body;
            }
        }
        for k in 0..=MAX_WALLS {
            let half = if k == 0 { AVATAR_HALF } else { WALL_HALF };
            let body = w.bodies.insert(
                RigidBodyBuilder::kinematic_position_based()
                    .translation(Vec3::new(0.0, -200.0 - k as f32 * 3.0, 0.0))
                    .build(),
            );
            let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
                .collision_groups(groups(G_REMOTE, G_BALL))
                .build();
            w.colliders
                .insert_with_parent(collider, body, &mut w.bodies);
            w.remote_bodies[k] = body;
        }
        w
    }

    /// Place l'avatar et les murs du joueur PC (`None` / liste vide : absent).
    /// `avatar` : pieds et lacet ; `walls` : centre au sol et lacet.
    pub fn set_remote(&mut self, avatar: Option<(Vec3, f32)>, walls: &[(Vec3, f32)]) {
        self.remote_poses[0] = avatar;
        for k in 0..MAX_WALLS {
            self.remote_poses[1 + k] = walls.get(k).copied();
        }
    }

    fn apply_remote(&mut self) {
        for (k, pose) in self.remote_poses.iter().enumerate() {
            let half = if k == 0 { AVATAR_HALF } else { WALL_HALF };
            let (p, r) = match pose {
                Some((feet, yaw)) => (*feet + Vec3::Y * half.y, Quat::from_rotation_y(*yaw)),
                None => (Vec3::new(0.0, -200.0 - k as f32 * 3.0, 0.0), Quat::IDENTITY),
            };
            if let Some(b) = self.bodies.get_mut(self.remote_bodies[k]) {
                // Absent → présent : téléporté, pas glissé depuis sous le sol.
                if b.translation().y < -50.0 || p.y < -50.0 {
                    b.set_translation(p, true);
                }
                b.set_next_kinematic_translation(p);
                b.set_next_kinematic_rotation(r);
            }
        }
    }

    /// Compte les boules qui touchent l'avatar ou un mur du joueur PC.
    fn count_stopped(&mut self) {
        let remote: Vec<ColliderHandle> = self
            .remote_bodies
            .iter()
            .filter_map(|h| self.bodies.get(*h))
            .filter_map(|b| b.colliders().first().copied())
            .collect();
        for &ball in &self.balls {
            if self.stopped.contains(&ball) {
                continue;
            }
            let Some(c) = self.bodies.get(ball).and_then(|b| b.colliders().first().copied()) else {
                continue;
            };
            let hit = remote.iter().any(|&r| {
                self.narrow
                    .contact_pair(c, r)
                    .is_some_and(|p| p.has_any_active_contact())
            });
            if hit {
                self.stopped.insert(ball);
                self.blocked += 1;
            }
        }
    }

    /// Vide tout sauf les mains (changement de parcours).
    pub fn clear(&mut self) {
        let doomed: Vec<RigidBodyHandle> = self
            .bodies
            .iter()
            .map(|(h, _)| h)
            .filter(|h| {
                !self.hand_bodies.iter().flatten().any(|x| x == h)
                    && !self.remote_bodies.contains(h)
            })
            .collect();
        for h in doomed {
            self.remove(h);
        }
        self.statics.clear();
        self.blocks.clear();
        self.movers.clear();
        self.balls.clear();
        self.stopped.clear();
        self.blocked = 0;
        for g in &mut self.grabbers {
            g.held = None;
        }
    }

    fn remove(&mut self, h: RigidBodyHandle) {
        self.bodies.remove(
            h,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse,
            &mut self.multibody,
            true,
        );
    }

    pub fn add_static(&mut self, center: Vec3, half: Vec3, color: Vec4) {
        let body = self
            .bodies
            .insert(RigidBodyBuilder::fixed().translation(center).build());
        let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
            .friction(0.8)
            .collision_groups(groups(G_WORLD, Group::ALL))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        self.statics.push((center, half, color));
    }

    /// Décor visuel sans collision (repères, anneaux) : dessiné seulement.
    pub fn add_decor(&mut self, center: Vec3, half: Vec3, color: Vec4) {
        self.statics.push((center, half, color));
    }

    pub fn add_block(&mut self, center: Vec3, half: Vec3, material: Material) -> usize {
        let body = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(center)
                .linear_damping(0.05)
                .angular_damping(0.2)
                .build(),
        );
        let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
            .density(material.density())
            .friction(0.7)
            .collision_groups(groups(G_BLOCK, Group::ALL))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        let color = material.color(self.blocks.len());
        self.blocks.push(Block {
            body,
            half,
            material,
            spawn: center,
            color,
        });
        self.blocks.len() - 1
    }

    pub fn add_mover(&mut self, half: Vec3, motion: Motion, color: Vec4) {
        let (p, r) = motion.pose(0.0);
        let body = self.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(p)
                .rotation(r.to_scaled_axis())
                .build(),
        );
        let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
            .friction(1.0)
            .collision_groups(groups(G_WORLD, Group::ALL))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        self.movers.push(Mover {
            body,
            half,
            motion,
            color,
            t0: self.time,
        });
    }

    /// Retire blocs, éléments mobiles et boules libres (trou suivant ou
    /// recommencé : `course::populate` repose ensuite ceux du trou).
    pub fn clear_dynamic(&mut self) {
        let doomed: Vec<RigidBodyHandle> = self
            .blocks
            .drain(..)
            .map(|b| b.body)
            .chain(self.movers.drain(..).map(|m| m.body))
            .collect();
        for h in doomed {
            self.remove(h);
        }
        self.clear_free_balls();
    }

    /// Retire les boules libres (changement de trou).
    pub fn clear_free_balls(&mut self) {
        let held: Vec<RigidBodyHandle> = self.grabbers.iter().filter_map(|g| g.held).collect();
        let free: Vec<RigidBodyHandle> = self
            .balls
            .iter()
            .copied()
            .filter(|b| !held.contains(b))
            .collect();
        for b in free {
            self.remove(b);
        }
        self.balls.retain(|b| held.contains(b));
    }

    /// Lance une boule libre (tests).
    #[cfg(test)]
    pub fn throw_ball(&mut self, from: Vec3, velocity: Vec3) -> RigidBodyHandle {
        let b = self.spawn_ball(from);
        self.release(b, velocity);
        b
    }

    pub fn position(&self, h: RigidBodyHandle) -> Vec3 {
        self.bodies.get(h).map_or(Vec3::ZERO, |b| b.translation())
    }

    pub fn rotation(&self, h: RigidBodyHandle) -> Quat {
        self.bodies.get(h).map_or(Quat::IDENTITY, |b| *b.rotation())
    }

    /// La cible est tombée. Seule la hauteur compte : une cible sur un
    /// chariot s'éloigne de sa place sans être tombée.
    pub fn is_down(&self, block: &Block) -> bool {
        self.position(block.body).y < block.spawn.y - TARGET_DROP
    }

    fn spawn_ball(&mut self, at: Vec3) -> RigidBodyHandle {
        if self.balls.len() >= MAX_BALLS {
            let held: Vec<RigidBodyHandle> =
                self.grabbers.iter().filter_map(|g| g.held).collect();
            if let Some(i) = self.balls.iter().position(|b| !held.contains(b)) {
                let old = self.balls.remove(i).expect("indice valide");
                self.remove(old);
            }
        }
        let body = self.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(at)
                .ccd_enabled(true)
                .build(),
        );
        let collider = ColliderBuilder::ball(BALL_RADIUS)
            .density(2500.0)
            .restitution(0.3)
            .friction(0.6)
            .collision_groups(groups(G_BALL, G_WORLD | G_BLOCK | G_BALL | G_REMOTE))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        self.balls.push_back(body);
        body
    }

    fn nearest_free_ball(&self, p: Vec3, max: f32) -> Option<RigidBodyHandle> {
        let held: Vec<RigidBodyHandle> = self.grabbers.iter().filter_map(|g| g.held).collect();
        self.balls
            .iter()
            .copied()
            .filter(|b| !held.contains(b))
            .map(|b| (b, self.position(b).distance(p)))
            .filter(|(_, d)| *d < max)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(b, _)| b)
    }

    /// Une main à cette image (`None` : main perdue). `can_grab` : faux quand
    /// un menu est ouvert (le pincement y clique). Renvoie une vibration et
    /// vrai si un coup vient d'être lancé.
    pub fn update_hand(
        &mut self,
        hand: usize,
        frame: Option<HandFrame>,
        can_grab: bool,
    ) -> (Option<Haptic>, bool) {
        let contacts = frame.map_or([(Vec3::new(0.0, -100.0, 0.0), 0.01); 6], |f| f.contacts);
        for (k, (p, _)) in contacts.iter().enumerate() {
            if let Some(b) = self.bodies.get_mut(self.hand_bodies[hand][k]) {
                b.set_next_kinematic_translation(*p);
            }
        }
        let time = self.time;
        let Some(frame) = frame else {
            if let Some(ball) = self.grabbers[hand].held.take() {
                self.release(ball, Vec3::ZERO);
            }
            self.grabbers[hand].reset();
            return (None, false);
        };
        match self.grabbers[hand].step(time, &frame, can_grab) {
            Grab::Start(point) => {
                let ball = match self.nearest_free_ball(point, 0.15) {
                    Some(b) => {
                        if let Some(body) = self.bodies.get_mut(b) {
                            body.set_body_type(RigidBodyType::KinematicPositionBased, true);
                        }
                        b
                    }
                    None => self.spawn_ball(point),
                };
                self.grabbers[hand].held = Some(ball);
                (
                    Some(Haptic {
                        amplitude: 0.3,
                        seconds: 0.03,
                    }),
                    false,
                )
            }
            Grab::Hold(point) => {
                if let Some(ball) = self.grabbers[hand].held
                    && let Some(body) = self.bodies.get_mut(ball)
                {
                    body.set_next_kinematic_translation(point);
                }
                (None, false)
            }
            Grab::Release(velocity) => {
                let v = (velocity * THROW_GAIN).clamp_length_max(25.0);
                let stroke = self.grabbers[hand].held.is_some() && v.length() > STROKE_SPEED;
                if let Some(ball) = self.grabbers[hand].held.take() {
                    self.release(ball, v);
                }
                (
                    stroke.then_some(Haptic {
                        amplitude: 0.2,
                        seconds: 0.03,
                    }),
                    stroke,
                )
            }
            Grab::Idle => (None, false),
        }
    }

    fn release(&mut self, ball: RigidBodyHandle, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(ball) {
            body.set_body_type(RigidBodyType::Dynamic, true);
            body.set_linvel(velocity, true);
        }
    }

    /// Avance la physique de `dt` (pas fixes), éléments mobiles compris.
    pub fn step(&mut self, dt: f32) {
        self.accumulator = (self.accumulator + dt).min(PHYSICS_DT * 4.0);
        while self.accumulator >= PHYSICS_DT {
            self.accumulator -= PHYSICS_DT;
            self.time += PHYSICS_DT;
            self.apply_remote();
            for m in &self.movers {
                let (p, r) = m.motion.pose(self.time - m.t0);
                if let Some(b) = self.bodies.get_mut(m.body) {
                    b.set_next_kinematic_translation(p);
                    b.set_next_kinematic_rotation(r);
                }
            }
            self.pipeline.step(
                Vector::new(0.0, -9.81, 0.0),
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
            self.count_stopped();
        }
        let lost: Vec<RigidBodyHandle> = self
            .balls
            .iter()
            .copied()
            .filter(|&b| self.position(b).y < -3.0)
            .collect();
        for b in lost {
            self.balls.retain(|&x| x != b);
            self.remove(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un mur du joueur PC arrête une boule (et compte un tir arrêté) ; sans
    /// mur, la même boule passe.
    #[test]
    fn a_pc_wall_stops_a_throw() {
        for with_wall in [false, true] {
            let mut w = World::new();
            w.add_static(Vec3::new(0.0, -0.05, 0.0), Vec3::new(10.0, 0.05, 10.0), Vec4::ONE);
            let walls = if with_wall {
                vec![(Vec3::new(0.0, 0.0, -1.5), 0.0)]
            } else {
                vec![]
            };
            w.set_remote(Some((Vec3::new(2.0, 0.0, -1.0), 0.0)), &walls);
            let ball = w.throw_ball(Vec3::new(0.0, 1.2, -0.3), Vec3::new(0.0, 0.5, -8.0));
            for _ in 0..60 {
                w.step(PHYSICS_DT);
            }
            let z = w.position(ball).z;
            if with_wall {
                assert!(z > -1.6, "arrêtée devant le mur : z = {z}");
                assert_eq!(w.blocked, 1);
            } else {
                assert!(z < -3.0, "passe sans mur : z = {z}");
                assert_eq!(w.blocked, 0);
            }
        }
    }
}
