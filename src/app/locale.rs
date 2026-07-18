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

/// Frags + assists détaillés (Phase L Sprint 3, `sprint2audijeu0718.md`,
/// GDD §8.3) : `kills_hud` (`editor/hud.rs`) n'affichait jusqu'ici que le
/// compteur de frags de `kills` ci-dessus — un assist (dégât porté sur un
/// monstre achevé par un autre joueur) n'est pas visible séparément alors
/// qu'il est déjà calculé côté serveur (`app::multiplayer::network_assists`).
pub fn kills_and_assists(_locale: Locale, kills: u32, assists: u32) -> String {
    // Icônes seules (💀/🤝) plutôt qu'un libellé traduit : contrairement aux
    // autres textes du HUD, aucun mot ne se traduit ici, donc pas de branche
    // par langue à maintenir.
    format!("💀 {kills} · 🤝 {assists}")
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

/// Bannière brève (GDD §5.3, §16.3 : « < 2 s ») quand un allié — pas nous —
/// tombe à 0 PV : `ally_down_banner`.
pub fn ally_down(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "🕯 Un allié est tombé",
        Locale::En => "🕯 An ally is down",
    }
}

/// Titre de l'écran de fin de manche détaillé (Phase H, Sprint 1, GDD §9.2/
/// §17.4) — `round_summary_banner`, distinct du `won`/`lost` minimal
/// existant (celui-ci reste utilisé par les démos solo sans salon réseau).
pub fn round_outcome_title(locale: Locale, won: bool) -> &'static str {
    match (locale, won) {
        (Locale::Fr, true) => "🎉 Manche gagnée !",
        (Locale::Fr, false) => "💀 Manche perdue",
        (Locale::En, true) => "🎉 Round won!",
        (Locale::En, false) => "💀 Round lost",
    }
}

/// Ligne de résumé d'un joueur (Phase H, Sprint 1) : « Loïc — 3 frags, 1
/// assist, +245 XP ».
pub fn round_summary_row(locale: Locale, name: &str, frags: u32, assists: u32, xp: u32) -> String {
    match locale {
        Locale::Fr => format!("{name} — {frags} frag(s), {assists} assist(s), +{xp} XP"),
        Locale::En => format!("{name} — {frags} frag(s), {assists} assist(s), +{xp} XP"),
    }
}

/// Bannière « Contrat du jour rempli » (Phase H, Sprint 2, GDD §3.4/§3.5) —
/// le montant (`XP_CONTRACT`, `bin/server.rs`) est dupliqué ici pour
/// l'affichage : le serveur ne renvoie que l'identité du contrat rempli
/// (`GameEvent::Win::contract`), pas son XP (crédité côté Firebase, pas
/// forcément pour ce compte s'il l'a déjà réclamé aujourd'hui).
pub fn contract_completed(locale: Locale, label: &str) -> String {
    match locale {
        Locale::Fr => format!("📜 Contrat du jour rempli : {label} (+250 XP)"),
        Locale::En => format!("📜 Daily contract completed: {label} (+250 XP)"),
    }
}

/// Bannière brève de nouvelle vague (Phase H, Sprint 2, GDD §17.2), distincte
/// de `wave` (HUD permanent haut d'écran) — celle-ci s'affiche au centre et
/// s'efface après quelques secondes (`AppState::wave_banner_flash`).
pub fn wave_start_banner(locale: Locale, wave: u32) -> String {
    match locale {
        Locale::Fr => format!("🧟 Vague {wave} !"),
        Locale::En => format!("🧟 Wave {wave}!"),
    }
}

/// Résumé lisible d'une `net::protocol::DeathCause` (Sprint 2,
/// `sprint10audit.md`, GDD §16.5 : « Encerclé — 2 Traqueuses ») — affiché sous
/// `defeated_spectator` tant qu'on reste spectateur. Un seul agresseur du même
/// type reste au singulier (« Rattrapé par… »), plusieurs deviennent
/// « Encerclé — N… » comme l'exemple du GDD.
pub fn death_cause(
    locale: Locale,
    kind: crate::net::protocol::DeathCauseKind,
    distinct_attackers: u8,
) -> String {
    use crate::net::protocol::DeathCauseKind;
    match (locale, kind, distinct_attackers.max(1)) {
        (Locale::Fr, DeathCauseKind::Monster, 1) => "Rattrapé par un monstre".to_string(),
        (Locale::Fr, DeathCauseKind::Monster, n) => format!("Encerclé — {n} monstres"),
        (Locale::Fr, DeathCauseKind::Creature, 1) => "Mordu par une créature".to_string(),
        (Locale::Fr, DeathCauseKind::Creature, n) => format!("Encerclé — {n} créatures"),
        (Locale::En, DeathCauseKind::Monster, 1) => "Caught by a monster".to_string(),
        (Locale::En, DeathCauseKind::Monster, n) => format!("Surrounded — {n} monsters"),
        (Locale::En, DeathCauseKind::Creature, 1) => "Bitten by a creature".to_string(),
        (Locale::En, DeathCauseKind::Creature, n) => format!("Surrounded — {n} creatures"),
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

/// Titre du menu pause (Phase J, `sprintreflecion.md`), ouvert à la demande
/// (Échap) pendant une partie — distinct des bannières de fin de manche.
pub fn pause_title(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "⏸ Pause",
        Locale::En => "⏸ Paused",
    }
}

pub fn resume_button_label(locale: Locale) -> &'static str {
    match locale {
        Locale::Fr => "▶ Reprendre",
        Locale::En => "▶ Resume",
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
        // `kills_and_assists` exclu volontairement : icônes seules (💀/🤝),
        // aucun mot traduit — cf. sa doc.
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
        use crate::net::protocol::DeathCauseKind;
        for kind in [DeathCauseKind::Monster, DeathCauseKind::Creature] {
            for n in [1, 2] {
                assert_ne!(
                    death_cause(Locale::Fr, kind, n),
                    death_cause(Locale::En, kind, n)
                );
            }
        }
    }

    #[test]
    fn interpolated_values_are_preserved_regardless_of_locale() {
        assert!(weapon_label(Locale::Fr, "Marteau").contains("Marteau"));
        assert!(weapon_label(Locale::En, "Marteau").contains("Marteau"));
        assert!(kills_and_assists(Locale::Fr, 42, 7).contains("42"));
        assert!(kills_and_assists(Locale::Fr, 42, 7).contains('7'));
        assert!(kills_and_assists(Locale::En, 42, 7).contains("42"));
        assert!(wave(Locale::En, 3, 7).contains('3') && wave(Locale::En, 3, 7).contains('7'));
        assert!(remaining(Locale::En, 5).contains('5'));
    }

    /// Sprint 2 (`sprint10audit.md`) : « Encerclé/Surrounded » (GDD §16.5) ne
    /// doit apparaître qu'à partir de deux agresseurs distincts — un seul
    /// agresseur reste au singulier (« Rattrapé par… »/« Caught by… »).
    #[test]
    fn death_cause_only_says_surrounded_for_two_or_more_attackers() {
        use crate::net::protocol::DeathCauseKind;
        assert!(!death_cause(Locale::Fr, DeathCauseKind::Monster, 1).contains("Encerclé"));
        assert!(death_cause(Locale::Fr, DeathCauseKind::Monster, 2).contains("Encerclé"));
        assert!(!death_cause(Locale::En, DeathCauseKind::Monster, 1).contains("Surrounded"));
        assert!(death_cause(Locale::En, DeathCauseKind::Monster, 2).contains("Surrounded"));
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
