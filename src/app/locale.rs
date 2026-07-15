//! Localisation du texte **runtime** (HUD affiché en Play, cf. `editor::hud`) —
//! pas l'éditeur, dont l'UI reste en français (outil de développement, jamais exporté
//! dans le player). Fonctions pures, testables sans egui : chaque chaîne qui existait
//! en dur dans `editor/hud.rs` devient un appel ici, `locale` transmis en paramètre
//! (pas de RTL, hors scope — cf. roadmap Sprint 130).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Locale {
    #[default]
    Fr,
    En,
}

pub fn weapon_label(locale: Locale, label: &str) -> String {
    match locale {
        Locale::Fr => format!("Arme : {label}"),
        Locale::En => format!("Weapon: {label}"),
    }
}

pub fn fire_hint(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "K ou « Feu » : tirer — 1/2/3 ou « Arme » : changer",
        Locale::En => "K or \u{201c}Fire\u{201d}: shoot — 1/2/3 or \u{201c}Weapon\u{201d}: switch",
    }
}

pub fn kills(locale: Locale, kills: u32) -> String {
    match locale {
        Locale::Fr => format!("💀 Frags : {kills}"),
        Locale::En => format!("💀 Kills: {kills}"),
    }
}

/// Suffixe « (équipée) » dans l'inventaire d'armes.
pub fn equipped_suffix(locale: Locale, label: &str) -> String {
    match locale {
        Locale::Fr => format!("{label} (équipée)"),
        Locale::En => format!("{label} (equipped)"),
    }
}

/// Suffixe « (toi) » sur ta propre ligne dans le classement multijoueur.
pub fn you_suffix(locale: Locale, name: &str) -> String {
    match locale {
        Locale::Fr => format!("{name} (toi)"),
        Locale::En => format!("{name} (you)"),
    }
}

pub fn wave(locale: Locale, wave: u32, max_wave: u32) -> String {
    match locale {
        Locale::Fr => format!("🧟 Vague {wave} / {max_wave}"),
        Locale::En => format!("🧟 Wave {wave} / {max_wave}"),
    }
}

pub fn remaining(locale: Locale, remaining: u32) -> String {
    match locale {
        Locale::Fr => format!("{remaining} restant(s)"),
        Locale::En => format!("{remaining} remaining"),
    }
}

/// Bannière de victoire, avec ou sans temps chronométré.
pub fn won(locale: Locale, time: Option<f32>) -> String {
    match (locale, time) {
        (Locale::Fr, Some(t)) => format!("🎉 Gagné en {t:.1}s !"),
        (Locale::Fr, None) => "🎉 Gagné !".to_string(),
        (Locale::En, Some(t)) => format!("🎉 Won in {t:.1}s!"),
        (Locale::En, None) => "🎉 Won!".to_string(),
    }
}

pub fn lost(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "💀 Perdu !",
        Locale::En => "💀 Lost!",
    }
}

pub fn defeated_spectator(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "Vaincu — spectateur",
        Locale::En => "Defeated — spectating",
    }
}

pub fn waiting_next_round(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "En attente de la prochaine manche…",
        Locale::En => "Waiting for the next round…",
    }
}

pub fn restart_button_label(locale: Locale, won: bool) -> &'static str {
    match (locale, won) {
        (Locale::Fr, true) => "➡ Niveau suivant",
        (Locale::Fr, false) => "🔄 Rejouer",
        (Locale::En, true) => "➡ Next level",
        (Locale::En, false) => "🔄 Replay",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_differs_between_locales() {
        // Filet de sécurité grossier mais utile : si une traduction a été oubliée
        // (copier-coller du français), Fr et En produiraient la même chaîne.
        assert_ne!(
            weapon_label(Locale::Fr, "Dague"),
            weapon_label(Locale::En, "Dague")
        );
        assert_ne!(fire_hint(Locale::Fr), fire_hint(Locale::En));
        assert_ne!(kills(Locale::Fr, 3), kills(Locale::En, 3));
        assert_ne!(
            equipped_suffix(Locale::Fr, "Arc"),
            equipped_suffix(Locale::En, "Arc")
        );
        assert_ne!(
            you_suffix(Locale::Fr, "Alice"),
            you_suffix(Locale::En, "Alice")
        );
        assert_ne!(wave(Locale::Fr, 1, 3), wave(Locale::En, 1, 3));
        assert_ne!(remaining(Locale::Fr, 2), remaining(Locale::En, 2));
        assert_ne!(won(Locale::Fr, Some(12.3)), won(Locale::En, Some(12.3)));
        assert_ne!(won(Locale::Fr, None), won(Locale::En, None));
        assert_ne!(lost(Locale::Fr), lost(Locale::En));
        assert_ne!(
            defeated_spectator(Locale::Fr),
            defeated_spectator(Locale::En)
        );
        assert_ne!(
            waiting_next_round(Locale::Fr),
            waiting_next_round(Locale::En)
        );
        assert_ne!(
            restart_button_label(Locale::Fr, true),
            restart_button_label(Locale::En, true)
        );
        assert_ne!(
            restart_button_label(Locale::Fr, false),
            restart_button_label(Locale::En, false)
        );
    }

    #[test]
    fn interpolated_values_are_preserved_regardless_of_locale() {
        assert!(weapon_label(Locale::Fr, "Marteau").contains("Marteau"));
        assert!(weapon_label(Locale::En, "Marteau").contains("Marteau"));
        assert!(kills(Locale::Fr, 42).contains("42"));
        assert!(kills(Locale::En, 42).contains("42"));
        assert!(wave(Locale::En, 3, 7).contains('3') && wave(Locale::En, 3, 7).contains('7'));
        assert!(remaining(Locale::En, 5).contains('5'));
    }

    #[test]
    fn won_without_a_time_omits_any_number() {
        assert!(!won(Locale::Fr, None).contains(char::is_numeric));
        assert!(!won(Locale::En, None).contains(char::is_numeric));
    }

    #[test]
    fn restart_label_depends_on_whether_the_run_was_won() {
        assert_ne!(
            restart_button_label(Locale::Fr, true),
            restart_button_label(Locale::Fr, false)
        );
        assert_ne!(
            restart_button_label(Locale::En, true),
            restart_button_label(Locale::En, false)
        );
    }

    #[test]
    fn default_locale_is_french() {
        assert_eq!(Locale::default(), Locale::Fr);
    }
}
