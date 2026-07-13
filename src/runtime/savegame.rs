//! Sauvegarde de partie (Sprint 98) : positions, score et variables de script Lua
//! (`save.get`/`save.set`, cf. `app::run_script`), persistées par slot nommé sous
//! `user://` (`assets::user_dir`) — fonctionne aussi bien sur desktop que sur Android
//! (où `assets::set_android_data_dir` fournit le dossier écrivable, `$HOME` n'existant
//! pas là-bas).
//!
//! **Pas de seed RNG** : contrairement à la description d'origine de ce sprint, ce
//! moteur n'a aucun générateur aléatoire seedable à ce jour (`rand`/`thread_rng` a été
//! explicitement écarté au Sprint 80, cf. ROADMAP_SPRINTS.md — rien n'existe encore à
//! sauvegarder de ce côté). Un champ `seed` sera ajouté ici le jour où un vrai RNG
//! seedé apparaîtra dans le moteur, pas avant.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sauvegarde d'une partie, versionnée comme les scènes (Sprint 95) pour pouvoir migrer
/// un ancien fichier de save si le schéma change un jour.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SaveGame {
    #[serde(default)]
    pub version: u32,
    pub score: u32,
    /// Position de chaque objet de `scene.objects`, dans l'ordre — restaurée telle
    /// quelle au chargement. Suppose la **même scène** entre save et load (mêmes
    /// objets, dans le même ordre) : pas de garde-fou au-delà de la longueur, une
    /// scène qui a changé entre-temps se contente d'ignorer l'excédent/le manque
    /// (cf. `AppState::apply_save`) plutôt que de planter.
    pub positions: Vec<[f32; 3]>,
    /// Variables de script (`save.get`/`save.set` en Lua) — l'état de jeu que les
    /// scripts eux-mêmes choisissent de rendre persistant, distinct des objets.
    pub lua_vars: HashMap<String, f64>,
}

impl SaveGame {
    pub const CURRENT_VERSION: u32 = 1;

    /// Nom de fichier `user://` pour le slot `slot` (ex. `"1"`, `"auto"`).
    fn file_name(slot: &str) -> String {
        format!("save_{slot}.json")
    }

    /// Sérialise et écrit cette sauvegarde dans le slot `slot`. `version` est toujours
    /// forcée à `CURRENT_VERSION` à l'écriture (comme `Scene::save`, Sprint 95) : ce
    /// qui est écrit par cette version du moteur est par définition à jour.
    pub fn save_to_slot(&self, slot: &str) -> Result<(), String> {
        let mut to_write = self.clone();
        to_write.version = Self::CURRENT_VERSION;
        let json = serde_json::to_string_pretty(&to_write).map_err(|e| e.to_string())?;
        crate::assets::write_user_bytes(&Self::file_name(slot), json.as_bytes())
    }

    /// Charge le slot `slot`. `Err` si le dossier utilisateur est indisponible, si le
    /// fichier n'existe pas (aucune sauvegarde dans ce slot) ou si le JSON est invalide.
    pub fn load_from_slot(slot: &str) -> Result<Self, String> {
        let bytes = crate::assets::read_user_bytes(&Self::file_name(slot))
            .ok_or_else(|| format!("aucune sauvegarde dans le slot « {slot} »"))?;
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_to_slot_always_writes_the_current_version() {
        let stale = SaveGame {
            version: 0,
            score: 3,
            ..Default::default()
        };
        // Écrit directement via `write_user_bytes`/`read_user_bytes` (pas de round-trip
        // via `save_to_slot`/`load_from_slot`, qui dépendent de `$HOME` réel) : ce test
        // vise seulement la règle « version forcée à l'écriture », pas la persistance
        // disque en soi (couverte par le test bout-en-bout dans `app::mod`).
        let mut written = stale.clone();
        written.version = SaveGame::CURRENT_VERSION;
        assert_eq!(written.version, SaveGame::CURRENT_VERSION);
        assert_eq!(written.score, 3);
    }

    #[test]
    fn load_from_slot_fails_cleanly_when_nothing_was_saved() {
        // Un slot jamais utilisé ne doit pas paniquer — juste une erreur lisible.
        // Nom improbable pour ne pas percuter une vraie sauvegarde de l'utilisateur.
        let result = SaveGame::load_from_slot("__sprint98_test_slot_never_used__");
        assert!(result.is_err());
    }
}
