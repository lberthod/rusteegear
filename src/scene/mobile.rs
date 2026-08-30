//! Configuration des contrôles tactiles (`Scene::mobile`) — extrait de `scene/mod.rs`
//! (Sprint 113a) : pure donnée, lue par `editor::hud::mobile_overlay` et exposée aux
//! scripts Lua via `input`.

use serde::{Deserialize, Serialize};

/// Configuration des contrôles tactiles affichés en mode Play / Player.
/// Le joystick et chaque bouton nommé sont lisibles depuis Lua via `input`.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct MobileControls {
    /// Affiche un joystick virtuel (coin bas-gauche).
    pub joystick: bool,
    /// Affiche un pavé « tank » W/A/S/D (coin bas-gauche) à la place du
    /// joystick : mêmes contrôles que le clavier desktop — W/S avance/recule le
    /// long de l'orientation *actuelle* du personnage, A/D le fait pivoter
    /// (cf. `PlayerInput::thrust`/`turn`). Prioritaire sur `joystick` si les deux sont
    /// actifs (cf. `mobile_overlay`), pour ne jamais superposer les deux dans le
    /// même coin de l'écran.
    #[serde(default)]
    pub dpad: bool,
    /// Joystick virtuel bridé à l'axe avance/recul (coin bas-gauche) : contrairement
    /// à `joystick` (axe libre X/Y), dévier le pouce latéralement n'a aucun effet —
    /// seul l'axe vertical compte, écrit dans `PlayerInput::touch_thrust`. À la
    /// place de `joystick`. Prioritaire sur `joystick` mais pas sur `dpad` (cf.
    /// `mobile_overlay`), pour ne jamais superposer plusieurs schémas de contrôle
    /// dans le même coin de l'écran. **Pas de second stick pour tourner** : une
    /// première version ajoutait un stick droit (axe horizontal → rotation
    /// caméra/personnage) mais il a été retiré sur retour explicite — tourner
    /// reste au clavier (flèches) tant qu'aucun remplacement tactile n'est défini.
    #[serde(default)]
    pub dual_stick: bool,
    /// Boutons tactiles nommés (coin bas-droite).
    pub buttons: Vec<String>,
    /// Zone tactile plein écran : un tap n'importe où expose `input.btn.touch` au script.
    #[serde(default)]
    pub touch_zone: bool,
    /// Affiche la barre de vie du HUD (pilotée par `set_health` côté script).
    #[serde(default)]
    pub health_bar: bool,
    /// Screen Safe Area : rentre les contrôles/HUD dans une marge sûre (encoche, bords arrondis).
    #[serde(default)]
    pub safe_area: bool,
}

impl MobileControls {
    /// Au moins un contrôle est-il actif ?
    pub fn any(&self) -> bool {
        self.joystick
            || self.dpad
            || self.dual_stick
            || !self.buttons.is_empty()
            || self.touch_zone
            || self.health_bar
    }

    /// Bascule le joystick virtuel ; l'activer désactive le pavé
    /// directionnel — les deux se dessinent dans le même coin de l'écran
    /// (bas-gauche), jamais les deux à la fois (cf. `editor::menus::menu_ajouter`,
    /// menu « UI mobile »). Extrait de la logique de menu (roadmap post-audit
    /// 2026-08-29, item 2.4) pour pouvoir vérifier l'invariant par un test,
    /// indépendamment d'`egui`.
    pub fn toggle_joystick(&mut self) {
        self.joystick = !self.joystick;
        if self.joystick {
            self.dpad = false;
        }
    }

    /// Bascule le pavé directionnel ; l'activer désactive le joystick — même
    /// raison que `toggle_joystick`.
    pub fn toggle_dpad(&mut self) {
        self.dpad = !self.dpad;
        if self.dpad {
            self.joystick = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_joystick_turns_it_on_from_the_default_off_state() {
        let mut m = MobileControls::default();
        m.toggle_joystick();
        assert!(m.joystick);
    }

    #[test]
    fn toggle_joystick_turns_it_back_off_on_a_second_call() {
        let mut m = MobileControls::default();
        m.toggle_joystick();
        m.toggle_joystick();
        assert!(!m.joystick);
    }

    #[test]
    fn toggle_joystick_on_clears_an_active_dpad() {
        let mut m = MobileControls {
            dpad: true,
            ..Default::default()
        };
        m.toggle_joystick();
        assert!(m.joystick);
        assert!(
            !m.dpad,
            "joystick et pavé ne doivent jamais être actifs ensemble"
        );
    }

    #[test]
    fn toggle_dpad_on_clears_an_active_joystick() {
        let mut m = MobileControls {
            joystick: true,
            ..Default::default()
        };
        m.toggle_dpad();
        assert!(m.dpad);
        assert!(
            !m.joystick,
            "joystick et pavé ne doivent jamais être actifs ensemble"
        );
    }

    #[test]
    fn toggling_dpad_off_again_does_not_reactivate_the_joystick() {
        // Ce n'est pas une bascule vers l'état précédent : désactiver le pavé
        // laisse simplement les deux contrôles inactifs.
        let mut m = MobileControls::default();
        m.toggle_dpad();
        m.toggle_dpad();
        assert!(!m.dpad);
        assert!(!m.joystick);
    }

    #[test]
    fn toggle_joystick_never_touches_unrelated_controls() {
        let mut m = MobileControls {
            dual_stick: true,
            touch_zone: true,
            health_bar: true,
            safe_area: true,
            buttons: vec!["B1".into()],
            ..Default::default()
        };
        m.toggle_joystick();
        assert!(m.dual_stick);
        assert!(m.touch_zone);
        assert!(m.health_bar);
        assert!(m.safe_area);
        assert_eq!(m.buttons, vec!["B1".to_string()]);
    }
}
