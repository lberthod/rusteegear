//! Déplacement et confort en VR (phase 4 de la roadmap). Logique pure, testée.
//!
//! Règles de confort retenues (standards des jeux Quest) :
//! - **la tête n'est jamais bougée par le jeu sans action du joueur** : en vue
//!   première personne, la pièce (le rig) suit le personnage que le joueur
//!   dirige au stick ; en vue spectateur, elle reste fixe ;
//! - rotation **par crans** (30° par défaut) plutôt que continue ;
//! - **vignette** (bords assombris) pendant le déplacement au stick ;
//! - pas de secousse de caméra (déjà coupée dans `Renderer::render_views`).

use glam::{Quat, Vec3};

use super::rig::Rig;

/// Vue du joueur en VR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Les yeux dans le personnage : la pièce suit le personnage.
    #[default]
    FirstPerson,
    /// Le personnage vu de derrière, pièce fixe (recentrée à la demande).
    Spectator,
}

impl ViewMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::FirstPerson => Self::Spectator,
            Self::Spectator => Self::FirstPerson,
        }
    }
}

/// Réglages de confort (menu VR, phase 5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comfort {
    pub view: ViewMode,
    /// Angle d'un cran de rotation (degrés).
    pub snap_degrees: f32,
    pub vignette: bool,
}

impl Default for Comfort {
    fn default() -> Self {
        Self {
            view: ViewMode::FirstPerson,
            snap_degrees: 30.0,
            vignette: true,
        }
    }
}

/// Rotation par crans au stick droit : un cran par poussée franche
/// (> 0,7), réarmé quand le stick revient près du centre (< 0,3) — tenir le
/// stick poussé ne fait pas tourner en continu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapTurn {
    armed: bool,
}

impl Default for SnapTurn {
    fn default() -> Self {
        Self { armed: true }
    }
}

impl SnapTurn {
    /// Variation de lacet (rad) de la pièce pour cette image : stick à droite
    /// = tourner à droite (lacet négatif, cf. `Rig::yaw`).
    pub fn update(&mut self, stick_x: f32, degrees: f32) -> f32 {
        if self.armed && stick_x.abs() > 0.7 {
            self.armed = false;
            -stick_x.signum() * degrees.to_radians()
        } else {
            if stick_x.abs() < 0.3 {
                self.armed = true;
            }
            0.0
        }
    }
}

impl Rig {
    /// Première personne : place la pièce pour que la tête (position dans la
    /// pièce `head_stage`) soit à l'aplomb du personnage (`player_feet`, ses
    /// pieds dans le monde) — le joueur qui fait un pas dans sa vraie pièce
    /// se décale donc par rapport au personnage, comme dans tout jeu VR.
    pub fn follow_first_person(&mut self, player_feet: Vec3, head_stage: Vec3) {
        let r = Quat::from_rotation_y(self.yaw);
        let head_flat = r * Vec3::new(head_stage.x, 0.0, head_stage.z);
        self.origin = Vec3::new(
            player_feet.x - head_flat.x,
            player_feet.y,
            player_feet.z - head_flat.z,
        );
    }

    /// Tourne la pièce de `delta_yaw` **autour de la tête** : la tête reste au
    /// même point du monde, seul le monde tourne autour d'elle.
    pub fn rotate_about_head(&mut self, delta_yaw: f32, head_stage: Vec3) {
        let head_world = self.origin + Quat::from_rotation_y(self.yaw) * head_stage;
        self.yaw += delta_yaw;
        self.origin = head_world - Quat::from_rotation_y(self.yaw) * head_stage;
    }

    /// Direction du regard dans le monde (`head_orientation` dans la pièce).
    pub fn look_world(&self, head_orientation: Quat) -> Vec3 {
        Quat::from_rotation_y(self.yaw) * head_orientation * Vec3::NEG_Z
    }
}

/// Entrée de déplacement (`PlayerInput::gamepad_move`) et lacet de caméra
/// (`AppState::vr_camera_yaw`) à donner au moteur pour que le stick (x = droite,
/// y = avant) déplace le personnage **relativement au regard** `look` (monde).
///
/// Le moteur transforme l'entrée par `vx = x·cos θ − y·sin θ`,
/// `vz = −x·sin θ − y·cos θ` (`simulation::camera_relative_move`, rendue en
/// convention joystick par `camera_relative_axes` puis re-niée en Z dans
/// `drive_local_and_networked_players`) : une **rotation** vers le regard de la
/// caméra, qui regarde le long de `(−sin θ, −cos θ)` (cf. `OrbitCamera::eye`).
/// Il suffit donc de lui donner θ = atan2(−Lx, −Lz) et le stick tel quel.
/// (Jusqu'au 24 septembre 2026, le moteur niait Z deux fois — une réflexion —
/// et cette fonction compensait en inversant x ; corrigé dans le moteur.)
/// Le test d'intégration `vr_stick_moves_the_player_where_the_head_looks` fait
/// tourner le vrai moteur : si sa convention change, il casse ici plutôt
/// qu'en jeu.
pub fn engine_move(stick: (f32, f32), look: Vec3) -> ((f32, f32), f32) {
    let flat = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
    let yaw = (-flat.x).atan2(-flat.z);
    (stick, yaw)
}

/// Intensité de la vignette (0 = aucune, ~0,75 = bords très sombres) selon la
/// vitesse horizontale du personnage déplacé au stick (m/s).
pub fn vignette_strength(speed: f32, enabled: bool) -> f32 {
    if !enabled {
        return 0.0;
    }
    (speed / 3.0).clamp(0.0, 1.0) * 0.75
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_turn_turns_once_per_push() {
        let mut s = SnapTurn::default();
        let step = s.update(1.0, 30.0);
        assert!(
            (step + 30f32.to_radians()).abs() < 1e-6,
            "droite = lacet négatif"
        );
        assert_eq!(s.update(1.0, 30.0), 0.0, "stick tenu : pas de second cran");
        assert_eq!(s.update(0.5, 30.0), 0.0, "pas encore réarmé");
        assert_eq!(s.update(0.1, 30.0), 0.0);
        assert!(s.update(-0.9, 30.0) > 0.0, "réarmé : cran à gauche");
    }

    #[test]
    fn rotating_about_the_head_keeps_the_head_in_place() {
        let mut rig = Rig {
            origin: Vec3::new(4.0, 1.0, -2.0),
            yaw: 0.3,
        };
        let head = Vec3::new(0.4, 1.65, -0.2);
        let before = rig.origin + Quat::from_rotation_y(rig.yaw) * head;
        rig.rotate_about_head(30f32.to_radians(), head);
        let after = rig.origin + Quat::from_rotation_y(rig.yaw) * head;
        assert!(before.distance(after) < 1e-5);
    }

    #[test]
    fn first_person_puts_the_head_over_the_character() {
        let mut rig = Rig {
            origin: Vec3::ZERO,
            yaw: 1.1,
        };
        let head = Vec3::new(0.3, 1.7, 0.5);
        let feet = Vec3::new(10.0, 2.0, -3.0);
        rig.follow_first_person(feet, head);
        let head_world = rig.origin + Quat::from_rotation_y(rig.yaw) * head;
        assert!((head_world.x - feet.x).abs() < 1e-5 && (head_world.z - feet.z).abs() < 1e-5);
        assert!((head_world.y - (feet.y + 1.7)).abs() < 1e-5);
    }

    /// Transformation du moteur (cf. `engine_move`), recopiée pour le test pur.
    fn engine_dir(input: (f32, f32), yaw: f32) -> Vec3 {
        let (s, c) = yaw.sin_cos();
        Vec3::new(input.0 * c - input.1 * s, 0.0, -input.0 * s - input.1 * c)
    }

    #[test]
    fn engine_move_sends_forward_along_the_look_and_right_to_the_right() {
        for look_yaw in [0.0f32, 0.9, 2.0, -2.6] {
            let look = Quat::from_rotation_y(look_yaw) * Vec3::NEG_Z;
            let right = look.cross(Vec3::Y);
            let (fwd_in, yaw) = engine_move((0.0, 1.0), look);
            assert!(
                engine_dir(fwd_in, yaw).dot(look) > 0.9999,
                "avant, regard {look_yaw}"
            );
            let (right_in, yaw) = engine_move((1.0, 0.0), look);
            assert!(
                engine_dir(right_in, yaw).dot(right) > 0.9999,
                "droite, regard {look_yaw}"
            );
        }
    }

    /// Le vrai moteur (démo contrôleur, simulation complète) : le stick avant
    /// fait avancer le personnage vers le regard, le stick à droite vers la
    /// droite du regard — quel que soit l'angle.
    #[test]
    fn vr_stick_moves_the_player_where_the_head_looks() {
        for look_yaw in [0.0f32, std::f32::consts::FRAC_PI_2, 2.4, -2.0] {
            for (stick, label) in [((0.0, 1.0), "avant"), ((1.0, 0.0), "droite")] {
                let mut app = crate::app::AppState::new();
                app.load_controller_demo();
                app.playing = true;
                let i = app.player_index().expect("joueur");
                let look = Quat::from_rotation_y(look_yaw) * Vec3::NEG_Z;
                let wanted = if label == "avant" {
                    look
                } else {
                    look.cross(Vec3::Y)
                };
                let (input, yaw) = engine_move(stick, look);
                let p0 = app.scene.objects[i].transform.position;
                app.input_state.gamepad_move = input;
                app.vr_camera_yaw = Some(yaw);
                app.advance_steps(30);
                let moved = app.scene.objects[i].transform.position - p0;
                let flat = Vec3::new(moved.x, 0.0, moved.z);
                assert!(
                    flat.length() > 0.3,
                    "{label} (regard {look_yaw}) : {moved:?}"
                );
                assert!(
                    flat.normalize().dot(wanted) > 0.9,
                    "{label} (regard {look_yaw}) : déplacement {moved:?}, voulu {wanted:?}"
                );
            }
        }
    }

    #[test]
    fn vignette_grows_with_speed_and_can_be_disabled() {
        assert_eq!(vignette_strength(0.0, true), 0.0);
        assert!(vignette_strength(1.5, true) > 0.3);
        assert!(vignette_strength(10.0, true) <= 0.75);
        assert_eq!(vignette_strength(3.0, false), 0.0);
    }
}
