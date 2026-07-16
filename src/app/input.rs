//! Événements d'entrée **agnostiques de la plateforme**.
//!
//! winit (desktop), le tactile (iOS/Android) ou la VR (OpenXR) traduisent leurs
//! événements natifs vers cet enum ; la logique applicative ne dépend que de lui.

#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    /// Début d'un appui (clic gauche/milieu / doigt posé). `pan` = pan caméra
    /// forcé (clic milieu ou Maj+glisser), quel que soit l'outil actif.
    PointerDown { pan: bool },
    /// Fin d'un appui (relâchement).
    PointerUp,
    /// Déplacement du pointeur, en pixels physiques.
    PointerMove { x: f64, y: f64 },
    /// Molette / pinch : delta positif = zoom avant.
    Scroll { delta: f32 },
}

/// Boutons manette assignables au remapping (`settings::GamepadBindings`) — sous-
/// ensemble de `gilrs::Button` couvrant les manettes courantes (Xbox/PlayStation/
/// Switch Pro), à l'exclusion des boutons de mode/inconnus (`Mode`, `Unknown`) sans
/// équivalent universel entre fabricants.
pub const GAMEPAD_BUTTON_NAMES: &[&str] = &[
    "South",
    "East",
    "North",
    "West",
    "LeftTrigger",
    "LeftTrigger2",
    "RightTrigger",
    "RightTrigger2",
    "Select",
    "Start",
    "LeftThumb",
    "RightThumb",
    "DPadUp",
    "DPadDown",
    "DPadLeft",
    "DPadRight",
];

/// Résout un nom persisté (`settings::GamepadBindings`) en bouton `gilrs`. `None`
/// pour un nom inconnu (réglage corrompu, ou vidé volontairement pour désactiver
/// l'action à la manette) — l'appelant traite alors l'action comme non tenue,
/// jamais comme une erreur bloquante.
pub fn gamepad_button_from_name(name: &str) -> Option<gilrs::Button> {
    use gilrs::Button::*;
    Some(match name {
        "South" => South,
        "East" => East,
        "North" => North,
        "West" => West,
        "LeftTrigger" => LeftTrigger,
        "LeftTrigger2" => LeftTrigger2,
        "RightTrigger" => RightTrigger,
        "RightTrigger2" => RightTrigger2,
        "Select" => Select,
        "Start" => Start,
        "LeftThumb" => LeftThumb,
        "RightThumb" => RightThumb,
        "DPadUp" => DPadUp,
        "DPadDown" => DPadDown,
        "DPadLeft" => DPadLeft,
        "DPadRight" => DPadRight,
        _ => return None,
    })
}

/// Zone morte du stick analogique : sous le seuil, un stick jamais parfaitement
/// centré au repos (dérive mécanique) produirait sinon un déplacement résiduel
/// permanent. Au-delà, la valeur passe telle quelle (bornée à [-1, 1] par
/// construction côté `gilrs`, mais on reclamp par robustesse).
pub fn apply_deadzone(v: f32, threshold: f32) -> f32 {
    if v.abs() < threshold {
        0.0
    } else {
        v.clamp(-1.0, 1.0)
    }
}

/// État manette résolu pour une frame : axe du stick gauche + actions liées via
/// `settings::GamepadBindings`. Fonction pure (pas de dépendance à `gilrs::Gilrs`
/// ni à winit) — testable sans manette réelle ni boucle d'événements.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GamepadInput {
    /// Stick gauche, zone morte déjà appliquée : x = tourne, y = avance/recul
    /// (mêmes axes que les contrôles clavier « tank », cf. `PlayerInput::turn`/
    /// `thrust`).
    pub turn: f32,
    pub thrust: f32,
    /// Stick droit, zone morte déjà appliquée : x = visée/rotation (cumulée à
    /// `turn` par l'appelant — en contrôles « tank », l'orientation du
    /// personnage EST la visée), y = tangage caméra (cf.
    /// `PlayerInput::gamepad_pitch`).
    pub look_x: f32,
    pub look_y: f32,
    pub jump: bool,
    pub attack: bool,
    pub fire: bool,
    pub heal: bool,
    /// Changement d'arme : état **tenu** ici ; le front montant (un cycle par
    /// appui) est détecté en aval par `fireball::update_fireballs`, comme pour
    /// le bouton tactile « Arme ».
    pub weapon: bool,
    /// Fenêtre Multijoueur : état **tenu** ; la bascule (front montant) est
    /// détectée par `App::recompute_action_buttons`, comme `weapon`.
    pub menu: bool,
    /// Masquer/afficher le HUD : état **tenu**, bascule sur front montant.
    pub hud: bool,
}

/// Zone morte standard (15 %) appliquée aux deux sticks avant résolution.
pub const STICK_DEADZONE: f32 = 0.15;

/// Résout l'état manette d'une frame à partir des boutons tenus (`gilrs::Button`),
/// des axes bruts des deux sticks et de la table de remapping — cf. `GamepadInput`.
pub fn resolve_gamepad_input(
    held: &std::collections::HashSet<gilrs::Button>,
    raw_axes: (f32, f32),
    raw_axes_right: (f32, f32),
    bindings: &crate::app::settings::GamepadBindings,
) -> GamepadInput {
    let pressed = |name: &str| gamepad_button_from_name(name).is_some_and(|b| held.contains(&b));
    GamepadInput {
        turn: apply_deadzone(raw_axes.0, STICK_DEADZONE),
        thrust: apply_deadzone(raw_axes.1, STICK_DEADZONE),
        look_x: apply_deadzone(raw_axes_right.0, STICK_DEADZONE),
        look_y: apply_deadzone(raw_axes_right.1, STICK_DEADZONE),
        jump: pressed(&bindings.jump),
        attack: pressed(&bindings.attack),
        fire: pressed(&bindings.fire),
        heal: pressed(&bindings.heal),
        weapon: pressed(&bindings.weapon),
        menu: pressed(&bindings.menu),
        hud: pressed(&bindings.hud),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::settings::GamepadBindings;

    #[test]
    fn apply_deadzone_zeroes_small_drift_but_passes_through_beyond_threshold() {
        assert_eq!(apply_deadzone(0.05, STICK_DEADZONE), 0.0);
        assert_eq!(apply_deadzone(-0.05, STICK_DEADZONE), 0.0);
        assert_eq!(apply_deadzone(0.5, STICK_DEADZONE), 0.5);
        assert_eq!(
            apply_deadzone(1.4, STICK_DEADZONE),
            1.0,
            "reclampé au-delà de 1"
        );
    }

    #[test]
    fn gamepad_button_from_name_round_trips_every_assignable_name() {
        for name in GAMEPAD_BUTTON_NAMES {
            assert!(
                gamepad_button_from_name(name).is_some(),
                "{name} devrait résoudre vers un gilrs::Button assignable"
            );
        }
        assert!(gamepad_button_from_name("PasUnBouton").is_none());
    }

    #[test]
    fn resolve_gamepad_input_reads_default_bindings_from_held_buttons() {
        let mut held = std::collections::HashSet::new();
        held.insert(gilrs::Button::South);
        held.insert(gilrs::Button::East);
        held.insert(gilrs::Button::RightTrigger);
        let bindings = GamepadBindings::default();
        let resolved = resolve_gamepad_input(&held, (0.6, -1.2), (0.0, 0.0), &bindings);
        assert!(resolved.jump, "South est le défaut de Saut");
        assert!(resolved.fire, "East est le défaut de Tir");
        assert!(
            resolved.weapon,
            "RightTrigger (RB) est le défaut de Changer d'arme"
        );
        assert!(!resolved.attack, "West (Attaque) n'est pas tenu");
        assert!(!resolved.heal, "North (Soin) n'est pas tenu");
        assert!(!resolved.menu, "Start (Menu) n'est pas tenu");
        assert!(!resolved.hud, "Select (HUD) n'est pas tenu");
        assert_eq!(resolved.turn, 0.6);
        assert_eq!(resolved.thrust, -1.0, "reclampé à 1 en valeur absolue");
    }

    #[test]
    fn resolve_gamepad_input_respects_a_remapped_binding() {
        let mut held = std::collections::HashSet::new();
        held.insert(gilrs::Button::DPadUp);
        let bindings = GamepadBindings {
            jump: "DPadUp".into(),
            ..GamepadBindings::default()
        };
        let resolved = resolve_gamepad_input(&held, (0.0, 0.0), (0.0, 0.0), &bindings);
        assert!(
            resolved.jump,
            "le remapping doit être respecté, pas le défaut South"
        );
    }

    #[test]
    fn resolve_gamepad_input_reads_right_stick_and_menu_hud_defaults() {
        let mut held = std::collections::HashSet::new();
        held.insert(gilrs::Button::Start);
        held.insert(gilrs::Button::Select);
        let bindings = GamepadBindings::default();
        let resolved = resolve_gamepad_input(&held, (0.0, 0.0), (0.08, -0.7), &bindings);
        assert!(resolved.menu, "Start est le défaut de Menu (Multijoueur)");
        assert!(resolved.hud, "Select est le défaut du masquage HUD");
        assert_eq!(
            resolved.look_x, 0.0,
            "dérive du stick droit sous la zone morte ignorée"
        );
        assert_eq!(
            resolved.look_y, -0.7,
            "axe vertical du stick droit transmis"
        );
    }
}
