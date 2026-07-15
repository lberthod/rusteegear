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
        self.joystick || self.dpad || !self.buttons.is_empty() || self.touch_zone || self.health_bar
    }
}
