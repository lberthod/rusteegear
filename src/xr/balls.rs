//! « Balles & cubes » : bac à sable VR **léger**, conçu pour tenir la
//! fréquence du Quest sans le `Renderer` du moteur (trop lourd au casque :
//! image HDR, post-traitements, ombres — Rivière et même RageQuit saccadent).
//!
//! Tout est dans l'espace de la pièce (`STAGE`, sol en y = 0) : pas de rig, pas
//! de déplacement artificiel, donc aucun inconfort. Des tours de cubes sur des
//! socles ; fermer la main (pincement ou poing) fait naître une boule dans la
//! main — ou attrape la boule la plus proche —, l'ouvrir la lance avec la
//! vitesse de la main. Les doigts poussent les cubes. Le gros bouton rouge (ou
//! A / X) remet les tours en place ; elles se reconstruisent aussi seules
//! quelques secondes après la chute du dernier cube.
//!
//! Rendu : deux maillages (cube, sphère basse définition) dessinés par
//! instances, un éclairage de Lambert + ciel, MSAA 4×, une passe par œil —
//! quelques appels de dessin par image. Physique : rapier3d à pas fixe.

use std::collections::VecDeque;

use glam::{Mat4, Quat, Vec3, Vec4};
use rapier3d::prelude::*;

use super::input::{Haptic, LEFT, RIGHT, XrInput};
use super::math::EyeView;
use crate::time_compat::Instant;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
const MAX_INSTANCES: usize = 1024;
/// Pas fixe de la physique (s) ; au plus 4 pas par image.
const PHYSICS_DT: f32 = 1.0 / 90.0;

const BALL_RADIUS: f32 = 0.055;
const MAX_BALLS: usize = 24;
/// Demi-côté des petits cubes des tours (cubes de 12 cm).
const CUBE_HALF: f32 = 0.06;
/// Hauteur du dessus des socles.
const TABLE_TOP: f32 = 0.85;
/// Centre du bouton de remise en place.
const BUTTON: Vec3 = Vec3::new(-0.55, 0.95, -0.55);
/// Gain appliqué à la vitesse de la main au lâcher : un lancer « en VR » sans
/// retour de force paraît mou à vitesse réelle.
const THROW_GAIN: f32 = 1.35;

// Groupes de collision : les mains touchent les cubes et les boules libres,
// jamais la boule tenue (cinématique elle aussi) ni le décor.
const G_WORLD: Group = Group::GROUP_1;
const G_CUBE: Group = Group::GROUP_2;
const G_BALL: Group = Group::GROUP_3;
const G_HAND: Group = Group::GROUP_4;

const SHADER: &str = r#"
struct Eye { view_proj: mat4x4<f32>, pos: vec4<f32> };
@group(0) @binding(0) var<uniform> eye: Eye;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs(
    @location(0) p: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,
) -> Out {
    let model = mat4x4<f32>(m0, m1, m2, m3);
    let world = model * vec4<f32>(p, 1.0);
    var out: Out;
    out.clip = eye.view_proj * world;
    out.world = world.xyz;
    out.normal = normalize((model * vec4<f32>(n, 0.0)).xyz);
    out.color = color;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    var base = in.color.rgb;
    // color.a = 2 : sol en damier de 50 cm.
    if (in.color.a > 1.5) {
        let c = floor(in.world.x * 2.0) + floor(in.world.z * 2.0);
        let odd = abs(c - 2.0 * floor(c * 0.5));
        base = mix(base, base * 0.78, odd);
    }
    let n = normalize(in.normal);
    let sun = normalize(vec3<f32>(0.35, 1.0, 0.45));
    let diffuse = max(dot(n, sun), 0.0);
    // Ciel au-dessus, sol chaud en dessous (hémisphérique).
    let sky = mix(vec3<f32>(0.32, 0.30, 0.28), vec3<f32>(0.55, 0.62, 0.75), n.y * 0.5 + 0.5);
    var rgb = base * (sky * 0.6 + vec3<f32>(1.0, 0.96, 0.88) * diffuse * 0.75);
    // color.a = 0.5 : émissif (bouton, mains qui tiennent).
    if (in.color.a > 0.25 && in.color.a < 0.75) {
        rgb = base;
    }
    // Brume douce au loin, couleur du fond.
    let d = distance(in.world, eye.pos.xyz);
    let fog = clamp((d - 6.0) / 30.0, 0.0, 1.0);
    rgb = mix(rgb, vec3<f32>(0.62, 0.72, 0.85), fog);
    return vec4<f32>(rgb, 1.0);
}
"#;

/// Une instance dessinée : matrice modèle (colonnes) et couleur (`a` = mode :
/// 1 normal, 2 damier, 0.5 émissif).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

impl Instance {
    fn new(model: Mat4, color: Vec4) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            color: color.to_array(),
        }
    }
}

/// Ce que fait une main à cette image (mains suivies ou manette).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandFrame {
    /// Point de prise (entre pouce et index, ou paume, ou devant la manette).
    pub grab_point: Vec3,
    /// Main fermée (hystérésis appliquée par `Grabber`) : mesure brute
    /// « fermée » / « ouverte » ; entre les deux, l'état précédent est gardé.
    pub closed: bool,
    pub open: bool,
    /// Points de contact avec les cubes (bout des doigts, paume) et leur rayon.
    pub contacts: [(Vec3, f32); 6],
}

const NO_CONTACT: (Vec3, f32) = (Vec3::new(0.0, -100.0, 0.0), 0.01);

impl HandFrame {
    /// Main suivie (26 articulations OpenXR) ou manette (poignée/gâchette).
    pub fn from_input(input: &XrInput, hand: usize) -> Option<Self> {
        if let Some(j) = &input.hand_joints[hand] {
            use super::hands::{INDEX_TIP, THUMB_TIP, pinch_distance};
            let palm = j[0];
            let tips = [THUMB_TIP, INDEX_TIP, 15, 20, 25];
            let fist = tips[1..].iter().map(|&t| j[t].distance(palm)).sum::<f32>() / 4.0;
            let pinch = pinch_distance(j);
            let pinching = pinch < 0.025;
            let grab_point = if pinching {
                (j[THUMB_TIP] + j[INDEX_TIP]) * 0.5
            } else {
                palm
            };
            let mut contacts = [NO_CONTACT; 6];
            contacts[0] = (palm, 0.035);
            for (c, &t) in contacts[1..].iter_mut().zip(&tips) {
                *c = (j[t], 0.012);
            }
            return Some(Self {
                grab_point,
                closed: pinching || fist < 0.065,
                open: pinch > 0.045 && fist > 0.085,
                contacts,
            });
        }
        let h = &input.hands[hand];
        let (pos, rot) = h.grip?;
        let press = h.squeeze.max(h.trigger);
        let mut contacts = [NO_CONTACT; 6];
        contacts[0] = (pos, 0.05);
        Some(Self {
            grab_point: pos + rot * Vec3::new(0.0, -0.02, -0.06),
            closed: press > 0.6,
            open: press < 0.35,
            contacts,
        })
    }
}

/// État d'une main entre deux images : fermée ou non, boule tenue, et
/// trajectoire récente du point de prise (vitesse du lancer).
#[derive(Debug, Default)]
struct Grabber {
    closed: bool,
    held: Option<RigidBodyHandle>,
    history: VecDeque<(f32, Vec3)>,
}

/// Fenêtre (s) sur laquelle la vitesse de la main est mesurée au lâcher.
const THROW_WINDOW: f32 = 0.08;

impl Grabber {
    fn record(&mut self, t: f32, p: Vec3) {
        self.history.push_back((t, p));
        while self.history.len() > 2 && t - self.history[0].0 > 0.25 {
            self.history.pop_front();
        }
    }

    /// Vitesse moyenne du point de prise sur les `THROW_WINDOW` dernières
    /// secondes (moins bruitée qu'un écart entre deux images).
    fn velocity(&self) -> Vec3 {
        let Some(&(t1, p1)) = self.history.back() else {
            return Vec3::ZERO;
        };
        let (t0, p0) = self
            .history
            .iter()
            .rev()
            .find(|(t, _)| t1 - t >= THROW_WINDOW)
            .or(self.history.front())
            .copied()
            .unwrap_or((t1, p1));
        if t1 - t0 < 1e-3 {
            return Vec3::ZERO;
        }
        (p1 - p0) / (t1 - t0)
    }
}

/// Le monde physique et ce qu'on y a posé.
pub struct Sandbox {
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
    /// Décor fixe : (centre, demi-tailles, couleur).
    statics: Vec<(Vec3, Vec3, Vec4)>,
    /// Cubes des tours : corps, demi-taille, couleur, hauteur de départ.
    cubes: Vec<(RigidBodyHandle, f32, Vec4, f32)>,
    /// Boules, de la plus ancienne à la plus récente.
    pub balls: VecDeque<RigidBodyHandle>,
    /// Sphères cinématiques des mains (6 par main).
    hand_bodies: [[RigidBodyHandle; 6]; 2],
    grabbers: [Grabber; 2],
    time: f32,
    accumulator: f32,
    /// Instant où tous les cubes sont tombés (reconstruction automatique).
    all_down_since: Option<f32>,
    button_cooldown: f32,
    /// Remises en place depuis le lancement (tests, simulateur).
    pub resets: u32,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Self {
        let mut s = Self {
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
            cubes: Vec::new(),
            balls: VecDeque::new(),
            hand_bodies: [[RigidBodyHandle::invalid(); 6]; 2],
            grabbers: Default::default(),
            time: 0.0,
            accumulator: 0.0,
            all_down_since: None,
            button_cooldown: 0.0,
            resets: 0,
        };
        s.build_static();
        s.build_hands();
        s.build_towers();
        s
    }

    fn add_static(&mut self, center: Vec3, half: Vec3, color: Vec4) {
        let body = self
            .bodies
            .insert(RigidBodyBuilder::fixed().translation(center).build());
        let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
            .friction(0.8)
            .collision_groups(InteractionGroups::new(G_WORLD, Group::ALL, InteractionTestMode::And))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        self.statics.push((center, half, color));
    }

    fn build_static(&mut self) {
        // Sol en damier (y = 0 : le sol de la pièce).
        self.add_static(
            Vec3::new(0.0, -0.05, 0.0),
            Vec3::new(30.0, 0.05, 30.0),
            Vec4::new(0.55, 0.6, 0.52, 2.0),
        );
        let wood = Vec4::new(0.55, 0.42, 0.3, 1.0);
        let top = TABLE_TOP * 0.5;
        // Trois socles devant le joueur, à portée de lancer (1,3 à 3 m).
        for (x, z, w) in [(0.0, -1.3, 0.35), (-1.3, -2.4, 0.35), (1.4, -3.0, 0.45)] {
            self.add_static(Vec3::new(x, top, z), Vec3::new(w, top, w), wood);
        }
        // Colonne du bouton de remise en place, à gauche, à portée de main.
        self.add_static(
            Vec3::new(BUTTON.x, 0.43, BUTTON.z),
            Vec3::new(0.09, 0.43, 0.09),
            Vec4::new(0.3, 0.32, 0.36, 1.0),
        );
    }

    fn build_hands(&mut self) {
        for hand in 0..2 {
            for k in 0..6 {
                let body = self.bodies.insert(
                    RigidBodyBuilder::kinematic_position_based()
                        .translation(NO_CONTACT.0)
                        .build(),
                );
                let r = if k == 0 { 0.035 } else { 0.012 };
                let collider = ColliderBuilder::ball(r)
                    .collision_groups(InteractionGroups::new(
                        G_HAND,
                        G_CUBE | G_BALL,
                        InteractionTestMode::And,
                    ))
                    .build();
                self.colliders
                    .insert_with_parent(collider, body, &mut self.bodies);
                self.hand_bodies[hand][k] = body;
            }
        }
    }

    fn add_cube(&mut self, center: Vec3, half: f32, color: Vec4) {
        let body = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(center)
                .linear_damping(0.05)
                .angular_damping(0.2)
                .build(),
        );
        let collider = ColliderBuilder::cuboid(half, half, half)
            .density(400.0)
            .friction(0.7)
            .collision_groups(InteractionGroups::new(G_CUBE, Group::ALL, InteractionTestMode::And))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        self.cubes.push((body, half, color, center.y));
    }

    fn build_towers(&mut self) {
        let palette = [
            Vec4::new(0.95, 0.5, 0.15, 1.0),
            Vec4::new(0.2, 0.55, 0.95, 1.0),
            Vec4::new(0.3, 0.8, 0.4, 1.0),
            Vec4::new(0.9, 0.3, 0.4, 1.0),
            Vec4::new(0.85, 0.75, 0.25, 1.0),
            Vec4::new(0.6, 0.4, 0.85, 1.0),
        ];
        let gap = 0.004;
        let size = CUBE_HALF * 2.0 + gap;
        let base = TABLE_TOP + CUBE_HALF + 0.002;
        // Pyramide de 4 étages sur le socle du milieu.
        let (cx, cz) = (0.0, -1.3);
        for row in 0..4 {
            let n = 4 - row;
            for i in 0..n {
                let x = cx + (i as f32 - (n - 1) as f32 * 0.5) * size;
                let y = base + row as f32 * (CUBE_HALF * 2.0 + 0.001);
                self.add_cube(Vec3::new(x, y, cz), CUBE_HALF, palette[(row + i) % 6]);
            }
        }
        // Tour de 7 cubes sur le socle de gauche.
        for k in 0..7 {
            let y = base + k as f32 * (CUBE_HALF * 2.0 + 0.001);
            self.add_cube(Vec3::new(-1.3, y, -2.4), CUBE_HALF, palette[k % 6]);
        }
        // Mur de 3 × 3 gros cubes (18 cm) sur le socle du fond.
        let big = 0.09;
        for row in 0..3 {
            for i in 0..3 {
                let x = 1.4 + (i as f32 - 1.0) * (big * 2.0 + gap);
                let y = TABLE_TOP + big + 0.002 + row as f32 * (big * 2.0 + 0.001);
                self.add_cube(Vec3::new(x, y, -3.0), big, palette[(row * 3 + i + 2) % 6]);
            }
        }
    }

    /// Remet les tours en place (bouton, A/X, ou tout est tombé) ; les boules
    /// libres disparaissent, celles tenues restent en main.
    pub fn reset(&mut self) {
        let held: Vec<RigidBodyHandle> = self.grabbers.iter().filter_map(|g| g.held).collect();
        let doomed: Vec<RigidBodyHandle> = self
            .cubes
            .drain(..)
            .map(|c| c.0)
            .chain(self.balls.iter().copied().filter(|b| !held.contains(b)))
            .collect();
        for h in doomed {
            self.bodies.remove(
                h,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse,
                &mut self.multibody,
                true,
            );
        }
        self.balls.retain(|b| held.contains(b));
        self.build_towers();
        self.all_down_since = None;
        self.resets += 1;
    }

    fn spawn_ball(&mut self, at: Vec3) -> RigidBodyHandle {
        if self.balls.len() >= MAX_BALLS {
            let held: Vec<RigidBodyHandle> =
                self.grabbers.iter().filter_map(|g| g.held).collect();
            if let Some(pos) = self.balls.iter().position(|b| !held.contains(b)) {
                let old = self.balls.remove(pos).expect("indice valide");
                self.bodies.remove(
                    old,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulse,
                    &mut self.multibody,
                    true,
                );
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
            .restitution(0.35)
            .friction(0.6)
            .collision_groups(InteractionGroups::new(G_BALL, Group::ALL, InteractionTestMode::And))
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        self.balls.push_back(body);
        body
    }

    fn position(&self, h: RigidBodyHandle) -> Vec3 {
        self.bodies.get(h).map_or(Vec3::ZERO, |b| b.translation())
    }

    /// Une image : mains → prises / lancers / contacts, puis physique à pas
    /// fixe. Renvoie les vibrations (prise, lancer).
    pub fn update(&mut self, dt: f32, hands: [Option<HandFrame>; 2], reset_button: bool) -> [Option<Haptic>; 2] {
        self.time += dt;
        let mut haptics = [None, None];
        for hand in [LEFT, RIGHT] {
            haptics[hand] = self.update_hand(hand, hands[hand]);
        }

        // Bouton rouge touché (ou A / X) : remise en place.
        self.button_cooldown = (self.button_cooldown - dt).max(0.0);
        let touched = hands.iter().flatten().any(|h| {
            h.contacts
                .iter()
                .any(|(p, r)| p.distance(BUTTON) < 0.07 + r)
        });
        if (touched || reset_button) && self.button_cooldown == 0.0 {
            self.reset();
            self.button_cooldown = 1.0;
            for (h, f) in haptics.iter_mut().zip(&hands) {
                if f.is_some() {
                    *h = Some(Haptic {
                        amplitude: 0.6,
                        seconds: 0.08,
                    });
                }
            }
        }

        // Physique à pas fixe.
        self.accumulator = (self.accumulator + dt).min(PHYSICS_DT * 4.0);
        while self.accumulator >= PHYSICS_DT {
            self.accumulator -= PHYSICS_DT;
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
        }

        // Boules tombées hors du monde : retirées.
        let lost: Vec<RigidBodyHandle> = self
            .balls
            .iter()
            .copied()
            .filter(|&b| self.position(b).y < -3.0)
            .collect();
        for b in lost {
            self.balls.retain(|&x| x != b);
            self.bodies.remove(
                b,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse,
                &mut self.multibody,
                true,
            );
        }

        // Tout est tombé : reconstruction 4 s plus tard.
        if self.fallen() == self.cubes.len() {
            let since = *self.all_down_since.get_or_insert(self.time);
            if self.time - since > 4.0 {
                self.reset();
            }
        } else {
            self.all_down_since = None;
        }
        haptics
    }

    fn update_hand(&mut self, hand: usize, frame: Option<HandFrame>) -> Option<Haptic> {
        // Sphères des doigts : suivent la main (ou rangées sous le sol).
        let contacts = frame.map_or([NO_CONTACT; 6], |f| f.contacts);
        for (k, (p, _)) in contacts.iter().enumerate() {
            if let Some(b) = self.bodies.get_mut(self.hand_bodies[hand][k]) {
                b.set_next_kinematic_translation(*p);
            }
        }
        let Some(frame) = frame else {
            // Main perdue : la boule tenue tombe.
            if let Some(ball) = self.grabbers[hand].held.take() {
                self.release(ball, Vec3::ZERO);
            }
            self.grabbers[hand].closed = false;
            self.grabbers[hand].history.clear();
            return None;
        };
        let time = self.time;
        let g = &mut self.grabbers[hand];
        g.record(time, frame.grab_point);
        let was = g.closed;
        if frame.closed {
            g.closed = true;
        } else if frame.open {
            g.closed = false;
        }
        let now = g.closed;
        match (was, now) {
            (false, true) => {
                let ball = self.nearest_free_ball(frame.grab_point, 0.15);
                let ball = match ball {
                    Some(b) => {
                        if let Some(body) = self.bodies.get_mut(b) {
                            body.set_body_type(RigidBodyType::KinematicPositionBased, true);
                        }
                        b
                    }
                    None => self.spawn_ball(frame.grab_point),
                };
                self.grabbers[hand].held = Some(ball);
                Some(Haptic {
                    amplitude: 0.35,
                    seconds: 0.04,
                })
            }
            (true, false) => {
                let v = self.grabbers[hand].velocity() * THROW_GAIN;
                if let Some(ball) = self.grabbers[hand].held.take() {
                    self.release(ball, v.clamp_length_max(25.0));
                }
                Some(Haptic {
                    amplitude: 0.2,
                    seconds: 0.03,
                })
            }
            _ => {
                if let Some(ball) = self.grabbers[hand].held
                    && let Some(body) = self.bodies.get_mut(ball)
                {
                    body.set_next_kinematic_translation(frame.grab_point);
                }
                None
            }
        }
    }

    fn release(&mut self, ball: RigidBodyHandle, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(ball) {
            body.set_body_type(RigidBodyType::Dynamic, true);
            body.set_linvel(velocity, true);
        }
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

    /// Cubes tombés de leur socle (plus de 25 cm sous leur hauteur de départ).
    pub fn fallen(&self) -> usize {
        self.cubes
            .iter()
            .filter(|(h, _, _, y0)| self.position(*h).y < y0 - 0.25)
            .count()
    }

    pub fn cube_count(&self) -> usize {
        self.cubes.len()
    }

    /// Boule tenue par cette main.
    pub fn held(&self, hand: usize) -> Option<RigidBodyHandle> {
        self.grabbers[hand].held
    }

    pub fn velocity_of(&self, h: RigidBodyHandle) -> Vec3 {
        self.bodies.get(h).map_or(Vec3::ZERO, |b| b.linvel())
    }

    /// Instances à dessiner : (cubes, sphères).
    fn instances(&self, hands: &XrInput) -> (Vec<Instance>, Vec<Instance>) {
        let mut cubes = Vec::with_capacity(96);
        let mut spheres = Vec::with_capacity(96);
        for &(c, half, color) in &self.statics {
            cubes.push(Instance::new(
                Mat4::from_scale_rotation_translation(half, Quat::IDENTITY, c),
                color,
            ));
        }
        // Bouton rouge (émissif).
        cubes.push(Instance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.07, 0.025, 0.07),
                Quat::IDENTITY,
                BUTTON - Vec3::Y * 0.06,
            ),
            Vec4::new(0.95, 0.12, 0.1, 0.5),
        ));
        for &(h, half, color, y0) in &self.cubes {
            if let Some(b) = self.bodies.get(h) {
                let p = b.translation();
                // Tombé : doré, pour voir où on en est d'un coup d'œil.
                let color = if p.y < y0 - 0.25 {
                    Vec4::new(1.0, 0.82, 0.3, 1.0)
                } else {
                    color
                };
                cubes.push(Instance::new(
                    Mat4::from_scale_rotation_translation(Vec3::splat(half), *b.rotation(), p),
                    color,
                ));
            }
        }
        let held: Vec<RigidBodyHandle> = self.grabbers.iter().filter_map(|g| g.held).collect();
        for &h in &self.balls {
            if let Some(b) = self.bodies.get(h) {
                let color = if held.contains(&h) {
                    Vec4::new(1.0, 0.95, 0.9, 1.0)
                } else {
                    Vec4::new(0.92, 0.92, 0.95, 1.0)
                };
                spheres.push(Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(BALL_RADIUS),
                        *b.rotation(),
                        b.translation(),
                    ),
                    color,
                ));
            }
        }
        // Mains : une petite sphère par articulation, orange quand la main tient.
        for hand in [LEFT, RIGHT] {
            let color = if self.grabbers[hand].closed {
                Vec4::new(1.0, 0.6, 0.2, 0.5)
            } else {
                Vec4::new(0.85, 0.88, 0.95, 1.0)
            };
            if let Some(j) = &hands.hand_joints[hand] {
                for (k, p) in j.iter().enumerate() {
                    let r = if k == 0 { 0.014 } else { 0.0085 };
                    spheres.push(Instance::new(
                        Mat4::from_scale_rotation_translation(Vec3::splat(r), Quat::IDENTITY, *p),
                        color,
                    ));
                }
            } else if let Some((pos, rot)) = hands.hands[hand].grip {
                cubes.push(Instance::new(
                    Mat4::from_scale_rotation_translation(Vec3::new(0.025, 0.02, 0.05), rot, pos),
                    color,
                ));
            }
        }
        cubes.truncate(MAX_INSTANCES / 2);
        spheres.truncate(MAX_INSTANCES / 2);
        (cubes, spheres)
    }
}

/// La scène complète : monde + rendu.
pub struct BallScene {
    pub sandbox: Sandbox,
    gfx: Gfx,
    last: Option<Instant>,
    paused: bool,
    reset_was: bool,
}

impl BallScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        Self {
            sandbox: Sandbox::new(),
            gfx: Gfx::new(device, format, width, height),
            last: None,
            paused: false,
            reset_was: false,
        }
    }

    /// Sans focus (menu système, casque retiré) : physique figée.
    pub fn set_focused(&mut self, focused: bool) {
        self.paused = !focused;
        self.last = None;
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        input: &XrInput,
        targets: [&wgpu::TextureView; 2],
    ) -> [Option<Haptic>; 2] {
        let now = Instant::now();
        let dt = self
            .last
            .map_or(1.0 / 72.0, |t| now.duration_since(t).as_secs_f32())
            .min(0.1);
        self.last = Some(now);
        let reset = input.hands[LEFT].primary || input.hands[RIGHT].primary;
        let reset_edge = reset && !self.reset_was;
        self.reset_was = reset;
        let haptics = if self.paused {
            [None, None]
        } else {
            let hands = [LEFT, RIGHT].map(|h| HandFrame::from_input(input, h));
            self.sandbox.update(dt, hands, reset_edge)
        };
        let (cubes, spheres) = self.sandbox.instances(input);
        self.gfx.draw(device, queue, eyes, targets, &cubes, &spheres);
        haptics
    }
}

struct Gfx {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    cube_range: std::ops::Range<u32>,
    sphere_range: std::ops::Range<u32>,
    instances: wgpu::Buffer,
    eye_buffers: [wgpu::Buffer; 2],
    eye_groups: [wgpu::BindGroup; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
}

impl Gfx {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        use wgpu::util::DeviceExt as _;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xr-balls"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xr-balls-eye"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let eye_buffers = [0, 1].map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("xr-balls-eye"),
                size: 80,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let eye_groups = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("xr-balls-eye"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: eye_buffers[i].as_entire_binding(),
                }],
            })
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xr-balls"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("xr-balls"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4
                        ],
                    },
                ],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut verts = super::test_scene::cube_vertices();
        let cube_range = 0..verts.len() as u32;
        verts.extend(sphere_vertices(12, 16));
        let sphere_range = cube_range.end..verts.len() as u32;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("xr-balls-mesh"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xr-balls-instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let target = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        Self {
            pipeline,
            vertices,
            cube_range,
            sphere_range,
            instances,
            eye_buffers,
            eye_groups,
            msaa: target("xr-balls-msaa", format),
            depth: target("xr-balls-depth", DEPTH_FORMAT),
        }
    }

    fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: [EyeView; 2],
        targets: [&wgpu::TextureView; 2],
        cubes: &[Instance],
        spheres: &[Instance],
    ) {
        for (buffer, e) in self.eye_buffers.iter().zip(eyes) {
            let mut data = [0.0f32; 20];
            data[..16].copy_from_slice(&e.view_proj().to_cols_array());
            data[16..19].copy_from_slice(&e.position.to_array());
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&data));
        }
        let all: Vec<Instance> = cubes.iter().chain(spheres).copied().collect();
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&all));
        let n_cubes = cubes.len() as u32;
        let n_all = all.len() as u32;
        let mut encoder = device.create_command_encoder(&Default::default());
        for (target, group) in targets.into_iter().zip(&self.eye_groups) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("xr-balls-eye"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.62,
                            g: 0.72,
                            b: 0.85,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.draw(self.cube_range.clone(), 0..n_cubes);
            pass.draw(self.sphere_range.clone(), n_cubes..n_all);
        }
        queue.submit([encoder.finish()]);
    }
}

/// Sphère unité basse définition (`rings` × `segments`), faces sortantes CCW :
/// (position, normale) — la normale d'une sphère unité est sa position.
fn sphere_vertices(rings: u32, segments: u32) -> Vec<[f32; 6]> {
    let point = |r: u32, s: u32| {
        let theta = std::f32::consts::PI * r as f32 / rings as f32;
        let phi = std::f32::consts::TAU * s as f32 / segments as f32;
        let p = Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin());
        [p.x, p.y, p.z, p.x, p.y, p.z]
    };
    let mut out = Vec::with_capacity((rings * segments * 6) as usize);
    for r in 0..rings {
        for s in 0..segments {
            let (a, b, c, d) = (point(r, s), point(r + 1, s), point(r + 1, s + 1), point(r, s + 1));
            out.extend([a, b, c, a, c, d]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Main (manette) qui ferme la poigne en `p`, puis s'ouvre en `q` après
    /// un mouvement rapide : la boule naît dans la main et part vers l'avant.
    fn frame(p: Vec3, closed: bool) -> HandFrame {
        let mut contacts = [NO_CONTACT; 6];
        contacts[0] = (p, 0.05);
        HandFrame {
            grab_point: p,
            closed,
            open: !closed,
            contacts,
        }
    }

    #[test]
    fn closing_a_hand_spawns_a_ball_and_opening_it_throws_forward() {
        let mut s = Sandbox::new();
        let dt = 1.0 / 72.0;
        let mut p = Vec3::new(0.3, 1.2, -0.2);
        s.update(dt, [None, Some(frame(p, true))], false);
        let ball = s.held(RIGHT).expect("boule née dans la main");
        // Bras qui avance à 4 m/s vers −Z pendant 0,15 s, main fermée.
        for _ in 0..11 {
            p += Vec3::new(0.0, 0.0, -4.0 * dt);
            s.update(dt, [None, Some(frame(p, true))], false);
        }
        assert!(s.position(ball).distance(p) < 0.02, "la boule suit la main");
        s.update(dt, [None, Some(frame(p, false))], false);
        assert!(s.held(RIGHT).is_none());
        let v = s.velocity_of(ball);
        assert!(v.z < -4.0, "lancée vers l'avant : {v:?}");
    }

    #[test]
    fn towers_stand_still_until_hit() {
        let mut s = Sandbox::new();
        for _ in 0..180 {
            s.update(1.0 / 72.0, [None, None], false);
        }
        assert_eq!(s.fallen(), 0, "les tours tiennent seules");
        assert!(s.cube_count() >= 20);
    }

    #[test]
    fn a_thrown_ball_knocks_cubes_off_the_middle_table() {
        let mut s = Sandbox::new();
        let dt = 1.0 / 72.0;
        // Boule lâchée à 1 m de la pyramide, lancée droit dessus à 8 m/s.
        let mut p = Vec3::new(0.0, TABLE_TOP + 0.12, -0.2);
        s.update(dt, [None, Some(frame(p, true))], false);
        for _ in 0..8 {
            p += Vec3::new(0.0, 0.0, -8.0 / THROW_GAIN * dt);
            s.update(dt, [None, Some(frame(p, true))], false);
        }
        s.update(dt, [None, Some(frame(p, false))], false);
        for _ in 0..180 {
            s.update(dt, [None, None], false);
        }
        assert!(s.fallen() >= 3, "{} cubes tombés", s.fallen());
    }

    #[test]
    fn reset_rebuilds_the_towers_and_clears_free_balls() {
        let mut s = Sandbox::new();
        let n = s.cube_count();
        let dt = 1.0 / 72.0;
        let p = Vec3::new(0.3, 1.2, -0.2);
        s.update(dt, [None, Some(frame(p, true))], false);
        s.update(dt, [None, Some(frame(p, false))], false);
        assert_eq!(s.balls.len(), 1);
        s.update(dt, [None, None], true);
        assert_eq!((s.cube_count(), s.balls.len(), s.resets), (n, 0, 1));
    }

    #[test]
    fn a_tracked_hand_grabs_with_a_pinch() {
        let open = super::super::hands::synthetic_hand(
            Vec3::new(0.2, 1.2, -0.3),
            Quat::IDENTITY,
            true,
            0.0,
            0.0,
        );
        let pinched = super::super::hands::synthetic_hand(
            Vec3::new(0.2, 1.2, -0.3),
            Quat::IDENTITY,
            true,
            1.0,
            0.0,
        );
        let mut input = XrInput::default();
        input.hand_joints[RIGHT] = Some(open);
        let f = HandFrame::from_input(&input, RIGHT).expect("main suivie");
        assert!(f.open && !f.closed, "main ouverte");
        input.hand_joints[RIGHT] = Some(pinched);
        let f = HandFrame::from_input(&input, RIGHT).expect("main suivie");
        assert!(f.closed, "pincement = prise");
    }
}
