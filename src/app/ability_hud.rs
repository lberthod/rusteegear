//! Barre de capacités 1-2-3-4 du HUD (14 septembre 2026 au soir, retour
//! utilisateur « 1-2-3 / J / K ne donnent aucun retour à l'écran ») : instantané
//! par frame de l'état de chaque capacité du kit (`Scene::ability_bar`), lu par
//! `editor::hud::ability_bar` pour **allumer** la case pressée et dessiner sa
//! recharge — un retour visuel immédiat et gros, indépendant du clip
//! d'animation du personnage (petit, vu de dos, souvent invisible).
//!
//! Même principe que `HudWidgetValues` : l'état est pris ici, côté `AppState`,
//! la couche de rendu ne connaît que cette struct.

use super::AppState;

/// État d'une case de la barre.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AbilitySlot {
    /// Capacité en cours (touche tenue **ou** animation/effet encore en cours,
    /// cf. `PlayerAttackState::swing_anim_remaining`) : la case s'allume.
    pub active: bool,
    /// Fraction de recharge restante, 0 = prête, 1 = vient d'être utilisée —
    /// dessinée comme un voile qui se vide. Seules les recharges connues
    /// **localement** sont montrées (ruée, sort en solo) ; en ligne la recharge
    /// du sort/de la mêlée vit côté serveur et n'est pas diffusée (0 ici).
    pub cooldown: f32,
}

/// Les cinq cases, dans l'ordre affiché : 1/J mêlée (G griffe = même action),
/// 2 bouclier, 3/K sort, 4 ruée, H soin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AbilityHud {
    pub melee: AbilitySlot,
    pub shield: AbilitySlot,
    pub spell: AbilitySlot,
    pub dash: AbilitySlot,
    pub heal: AbilitySlot,
}

impl AbilityHud {
    /// Cases dans l'ordre d'affichage (cf. `locale::ability_slot_names`).
    pub fn slots(&self) -> [AbilitySlot; 5] {
        [self.melee, self.shield, self.spell, self.dash, self.heal]
    }
}

impl AppState {
    /// Instantané de la barre de capacités, `None` hors des scènes à kit
    /// (`Scene::ability_bar`) — le HUD affiche alors l'arme équipée comme avant.
    pub fn ability_hud(&self) -> Option<AbilityHud> {
        if !self.scene.ability_bar {
            return None;
        }
        let inp = &self.input_state;
        let a = &self.attack;
        let frac = |remaining: f32, total: f32| {
            if total > 0.0 {
                (remaining / total).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let spell_cooldown = self
            .player_index()
            .and_then(|pi| self.projectiles.fireball_cooldowns.get(&pi))
            .map(|cd| {
                frac(
                    *cd,
                    super::fireball::RANGED_WEAPONS[self.selected_weapon].cooldown,
                )
            })
            .unwrap_or(0.0);
        let attack_total = self
            .player_object()
            .and_then(|o| o.controller.as_ref())
            .map(|c| c.attack_cooldown)
            .unwrap_or(0.0);
        Some(AbilityHud {
            melee: AbilitySlot {
                active: inp.attack || a.swing_anim_remaining > 0.0,
                cooldown: frac(a.attack_cooldown_remaining, attack_total),
            },
            shield: AbilitySlot {
                active: inp.block,
                cooldown: 0.0,
            },
            spell: AbilitySlot {
                active: inp.fire || (a.cast_anim_remaining > 0.0 && !inp.heal),
                cooldown: spell_cooldown,
            },
            dash: AbilitySlot {
                active: inp.dash || a.roll_anim_remaining > 0.0,
                cooldown: frac(
                    a.dash_cooldown_remaining,
                    super::multiplayer::DASH_COOLDOWN,
                ),
            },
            heal: AbilitySlot {
                active: inp.heal,
                cooldown: 0.0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::AppState;
    use crate::scene::Scene;

    #[test]
    fn no_bar_outside_ability_scenes() {
        let app = AppState::new();
        assert!(!app.scene.ability_bar, "la scène par défaut n'a pas de kit");
        assert!(app.ability_hud().is_none());
    }

    #[test]
    fn a_held_key_lights_its_slot_and_the_swing_timer_keeps_it_lit() {
        let mut app = AppState::new();
        app.scene = Scene::default();
        app.scene.ability_bar = true;
        let idle = app.ability_hud().expect("scène à kit");
        assert!(idle.slots().iter().all(|s| !s.active && s.cooldown == 0.0));

        app.input_state.attack = true;
        app.input_state.block = true;
        app.input_state.fire = true;
        app.input_state.dash = true;
        app.input_state.heal = true;
        let held = app.ability_hud().unwrap();
        assert!(held.slots().iter().all(|s| s.active), "{held:?}");

        // Touche relâchée mais clip encore en cours (appui bref) : la case
        // reste allumée le temps du minuteur, comme le personnage.
        app.input_state = Default::default();
        app.attack.swing_anim_remaining = 0.2;
        app.attack.cast_anim_remaining = 0.2;
        app.attack.roll_anim_remaining = 0.2;
        app.attack.dash_cooldown_remaining = super::super::multiplayer::DASH_COOLDOWN * 0.5;
        let after = app.ability_hud().unwrap();
        assert!(after.melee.active && after.spell.active && after.dash.active);
        assert!(!after.shield.active && !after.heal.active);
        assert!((after.dash.cooldown - 0.5).abs() < 1e-3, "{after:?}");
    }
}
