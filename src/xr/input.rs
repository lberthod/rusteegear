//! Entrées VR (phase 3 de la roadmap) : état des deux manettes Meta Quest
//! Touch — lu par OpenXR sur le casque (`xr::hello`), simulé au clavier/souris
//! par `quest_sim` — et sa traduction vers les commandes du jeu
//! (`PlayerInput`), plus le retour haptique déclenché par les événements du
//! jeu. Logique pure, testée sur le poste de dev : le même code sert aux
//! vraies manettes et au simulateur.
//!
//! Correspondance (manettes Touch, droitier) :
//!
//! | Manette | Jeu |
//! |---|---|
//! | stick gauche | déplacement (relatif au regard, cf. `xr::locomotion`) |
//! | stick droit ← → | rotation par crans (`xr::locomotion::SnapTurn`) |
//! | clic stick droit | recentrer / changer de vue |
//! | A | saut |
//! | B | ruée |
//! | gâchette droite | attaque |
//! | gâchette gauche | boule de feu |
//! | grip gauche | bouclier |
//! | X | soin |
//! | Y | changer d'arme |
//! | menu (gauche) | pause |

use glam::{Quat, Vec3};

use crate::app::PlayerInput;

/// Seuil d'appui d'une gâchette ou d'un grip analogique.
pub const PRESS_THRESHOLD: f32 = 0.5;
/// Zone morte des sticks (les Touch dérivent légèrement au repos).
pub const STICK_DEADZONE: f32 = 0.15;

/// Une manette, dans l'espace de la **pièce** (`STAGE`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HandInput {
    /// Pose de la poignée (où dessiner la manette) — `None` si non suivie.
    pub grip: Option<(Vec3, Quat)>,
    /// Pose de visée (rayon de pointage de l'interface, phase 5).
    pub aim: Option<(Vec3, Quat)>,
    pub trigger: f32,
    pub squeeze: f32,
    /// Stick, x à droite, y vers l'avant, dans \[−1, 1\].
    pub stick: (f32, f32),
    pub stick_click: bool,
    /// A (droite) / X (gauche).
    pub primary: bool,
    /// B (droite) / Y (gauche).
    pub secondary: bool,
    /// Bouton menu (gauche seulement sur les Touch).
    pub menu: bool,
}

pub const LEFT: usize = 0;
pub const RIGHT: usize = 1;

/// Les deux manettes (`hands[LEFT]`, `hands[RIGHT]`) et, en suivi des mains
/// (phase 8), les 26 articulations de chaque main dans la pièce.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct XrInput {
    pub hands: [HandInput; 2],
    pub hand_joints: [Option<super::hands::Joints>; 2],
}

/// Zone morte radiale, rééchelonnée pour repartir de 0 au bord de la zone.
pub fn deadzone(stick: (f32, f32)) -> (f32, f32) {
    let len = stick.0.hypot(stick.1);
    if len <= STICK_DEADZONE {
        return (0.0, 0.0);
    }
    let scaled = ((len - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0);
    (stick.0 / len * scaled, stick.1 / len * scaled)
}

impl XrInput {
    /// Écrit les commandes de jeu dans `player` (canaux « manette » : les
    /// autres sources — clavier, tactile — ne sont pas touchées). Le
    /// déplacement est relatif à la caméra de jeu, que la session VR oriente
    /// selon le regard (`xr::locomotion`).
    pub fn apply_to(&self, player: &mut PlayerInput) {
        let (l, r) = (&self.hands[LEFT], &self.hands[RIGHT]);
        player.gamepad_move = deadzone(l.stick);
        // La caméra de jeu ne doit pas orbiter au stick droit en VR : la tête
        // la dirige, le stick droit sert à la rotation par crans.
        player.gamepad_yaw = 0.0;
        player.gamepad_pitch = 0.0;
        player.jump = r.primary;
        player.dash = r.secondary;
        player.attack = r.trigger >= PRESS_THRESHOLD;
        player.fire = l.trigger >= PRESS_THRESHOLD;
        player.block = l.squeeze >= PRESS_THRESHOLD;
        player.heal = l.primary;
        player.weapon_cycle = l.secondary;
    }
}

/// Une vibration à jouer sur une manette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Haptic {
    /// Intensité dans \[0, 1\].
    pub amplitude: f32,
    pub seconds: f32,
}

/// Retours haptiques d'une image, déduits des effets du jeu (flash de dégâts,
/// coup porté) : on vibre sur un **front montant** de l'effet, pas tant qu'il
/// décroît. `prev`/`now` = (`fx.damage_flash`, `fx.attack_flash`).
pub fn haptics_from_fx(prev: (f32, f32), now: (f32, f32)) -> [Option<Haptic>; 2] {
    let mut out = [None, None];
    if now.0 > prev.0 + 0.2 {
        // Coup encaissé : les deux mains, franchement.
        let hit = Haptic {
            amplitude: 0.8,
            seconds: 0.15,
        };
        out = [Some(hit), Some(hit)];
    }
    if now.1 > prev.1 + 0.2 && out[RIGHT].is_none() {
        // Coup porté : la main qui frappe, brièvement.
        out[RIGHT] = Some(Haptic {
            amplitude: 0.5,
            seconds: 0.06,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_kills_drift_and_keeps_full_range() {
        assert_eq!(deadzone((0.1, -0.05)), (0.0, 0.0));
        let (x, y) = deadzone((0.0, 1.0));
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        let (x, _) = deadzone((0.2, 0.0));
        assert!(
            x > 0.0 && x < 0.1,
            "repart doucement au bord de la zone : {x}"
        );
    }

    #[test]
    fn touch_controllers_map_to_the_game_actions() {
        let mut input = XrInput::default();
        input.hands[LEFT].stick = (0.0, 1.0);
        input.hands[LEFT].trigger = 0.9;
        input.hands[LEFT].squeeze = 0.7;
        input.hands[RIGHT].primary = true;
        input.hands[RIGHT].trigger = 0.6;
        input.hands[RIGHT].stick = (1.0, 0.0);
        let mut player = PlayerInput {
            gamepad_yaw: 0.8,
            ..Default::default()
        };
        input.apply_to(&mut player);
        assert!((player.gamepad_move.1 - 1.0).abs() < 1e-6, "avance");
        assert!(player.jump && player.attack && player.fire && player.block);
        assert!(!player.dash && !player.heal && !player.weapon_cycle);
        assert_eq!(
            player.gamepad_yaw, 0.0,
            "le stick droit n'orbite pas la caméra"
        );
    }

    #[test]
    fn releasing_everything_releases_every_action() {
        let mut player = PlayerInput {
            jump: true,
            attack: true,
            block: true,
            gamepad_move: (0.5, 0.5),
            ..Default::default()
        };
        XrInput::default().apply_to(&mut player);
        assert!(!player.jump && !player.attack && !player.block);
        assert_eq!(player.gamepad_move, (0.0, 0.0));
    }

    #[test]
    fn haptics_fire_on_rising_edges_only() {
        let hurt = haptics_from_fx((0.0, 0.0), (1.0, 0.0));
        assert!(hurt[LEFT].is_some() && hurt[RIGHT].is_some());
        let decaying = haptics_from_fx((1.0, 0.0), (0.9, 0.0));
        assert_eq!(decaying, [None, None]);
        let hit = haptics_from_fx((0.0, 0.1), (0.0, 1.0));
        assert!(hit[LEFT].is_none() && hit[RIGHT].is_some());
    }
}
