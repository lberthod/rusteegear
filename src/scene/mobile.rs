//! Configuration des contrôles tactiles (`Scene::mobile`) — extrait de `scene/mod.rs`
//! (Sprint 113a) : pure donnée, lue par `editor::hud::mobile_overlay` et exposée aux
//! scripts Lua via `input`.

use serde::{Deserialize, Serialize};

/// Configuration des contrôles tactiles affichés en mode Play / Player.
/// Le joystick et chaque bouton nommé sont lisibles depuis Lua via `input`.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct MobileControls {
    /// Affiche le stick virtuel flottant (moitié gauche) — même comportement
    /// que `dual_stick`, cf. sa doc.
    pub joystick: bool,
    /// Affiche un pavé « tank » W/A/S/D (coin bas-gauche) à la place du
    /// joystick : mêmes contrôles que le clavier desktop — W/S avance/recule le
    /// long de l'orientation *actuelle* du personnage, A/D le fait pivoter
    /// (cf. `PlayerInput::thrust`/`turn`). Prioritaire sur `joystick` si les deux sont
    /// actifs (cf. `mobile_overlay`), pour ne jamais superposer les deux dans le
    /// même coin de l'écran.
    #[serde(default)]
    pub dpad: bool,
    /// Schéma « stick + caméra au doigt » (roadmap post-audit UX v2
    /// 2026-09-04, 5.7 — cette doc décrivait encore un stick bridé à un axe) :
    ///
    /// - **stick gauche flottant** : il apparaît là où le pouce se pose dans la
    ///   moitié gauche de l'écran (sous la barre de vie), deux axes **relatifs
    ///   à la caméra** écrits dans `PlayerInput::joy` — le même canal que
    ///   WASD/flèches, le personnage tourne tout seul vers la direction
    ///   demandée ; zone morte de 12 %, rayon proportionnel à l'écran (44 à
    ///   90 pt, cf. `app::touch::stick_radius`), le doigt garde le contrôle en
    ///   sortant du cercle et le stick disparaît au relâchement (roadmap v2 5.2) ;
    /// - **orbite au doigt** : glisser dans la moitié droite tourne la caméra
    ///   (`PlayerInput::touch_look`), en même temps que le stick — chaque doigt
    ///   est suivi séparément (`lib.rs`, roadmap v2 5.1) ;
    /// - **grille d'action** bas-droite (`buttons`) : boutons nommés tenables
    ///   pendant que le stick est actif.
    ///
    /// À la place de `joystick` (même schéma aujourd'hui — `joystick` est
    /// l'ancien nom, gardé pour les scènes existantes) ; prioritaire sur
    /// `joystick` mais pas sur `dpad` (cf. `mobile_overlay`), pour ne jamais
    /// superposer plusieurs schémas dans le même coin de l'écran. Il n'y a pas
    /// de second stick dessiné : la moitié droite entière fait office de
    /// « stick caméra », plus large qu'un cercle sous le pouce.
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
    /// Screen Safe Area : rentre les contrôles/HUD dans une marge sûre (encoche,
    /// bords arrondis). Les insets système (iOS `safeAreaInsets`, rectangle de
    /// contenu Android, `env(safe-area-inset-*)` web) s'appliquent toujours à
    /// tout le HUD ; ce drapeau ajoute la marge de repli de 6 % (28 pt max)
    /// sur les côtés sans inset connu — cf. `app::touch::safe_insets`
    /// (roadmap post-audit UX v2 2026-09-04, 5.4).
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
