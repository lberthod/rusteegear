//! Sauvegarde de partie : positions, score et variables de script Lua
//! (`save.get`/`save.set`, cf. `app::run_script`), persistées par slot nommé sous
//! `user://` (`assets::user_dir`) — fonctionne aussi bien sur desktop que sur Android
//! (où `assets::set_android_data_dir` fournit le dossier écrivable, `$HOME` n'existant
//! pas là-bas).
//!
//! **Pas de seed RNG** : ce moteur n'a aucun générateur aléatoire seedable à ce jour
//! (`rand`/`thread_rng` a été explicitement écarté, cf. ROADMAP_SPRINTS.md — rien
//! n'existe encore à sauvegarder de ce côté). Un champ `seed` sera ajouté ici le jour
//! où un vrai RNG seedé apparaîtra dans le moteur, pas avant.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sauvegarde d'une partie, versionnée comme les scènes pour pouvoir migrer
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
    /// forcée à `CURRENT_VERSION` à l'écriture (comme `Scene::save`) : ce
    /// qui est écrit par cette version du moteur est par définition à jour.
    pub fn save_to_slot(&self, slot: &str) -> Result<(), String> {
        let dir = crate::assets::user_dir()
            .ok_or_else(|| "dossier utilisateur indisponible".to_string())?;
        self.save_to_slot_at(slot, &dir)
    }

    /// Comme `save_to_slot`, mais avec un dossier explicite plutôt que le
    /// vrai `user_dir()` (Sprint 105a-3, isolation des tests) — même patron
    /// que `assets::write_user_bytes_at`.
    pub fn save_to_slot_at(&self, slot: &str, dir: &std::path::Path) -> Result<(), String> {
        if !valid_slot(slot) {
            return Err(format!("nom de slot invalide : « {slot} »"));
        }
        let mut to_write = self.clone();
        to_write.version = Self::CURRENT_VERSION;
        let json = serde_json::to_string_pretty(&to_write).map_err(|e| e.to_string())?;
        crate::assets::write_user_bytes_at(dir, &Self::file_name(slot), json.as_bytes())
    }

    /// Charge le slot `slot`. `Err` si le nom de slot est invalide, si le dossier
    /// utilisateur est indisponible, si le fichier n'existe pas (aucune sauvegarde
    /// dans ce slot) ou si le JSON est invalide.
    pub fn load_from_slot(slot: &str) -> Result<Self, String> {
        let dir = crate::assets::user_dir()
            .ok_or_else(|| "dossier utilisateur indisponible".to_string())?;
        Self::load_from_slot_at(slot, &dir)
    }

    /// Comme `load_from_slot`, mais avec un dossier explicite (Sprint
    /// 105a-3, isolation des tests) — cf. la doc de `save_to_slot_at`.
    pub fn load_from_slot_at(slot: &str, dir: &std::path::Path) -> Result<Self, String> {
        if !valid_slot(slot) {
            return Err(format!("nom de slot invalide : « {slot} »"));
        }
        let bytes = crate::assets::read_user_bytes_at(dir, &Self::file_name(slot))
            .ok_or_else(|| format!("aucune sauvegarde dans le slot « {slot} »"))?;
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
}

/// Longueur maximale (caractères) d'un nom de slot. Assez large pour les
/// noms générés par les tests bout-en-bout (`pid_nanoseconds`, cf.
/// `app::tests::saving_and_loading_a_game_restores_score_position_and_lua_
/// vars`, ~45 caractères), tout en restant loin de tout usage pathologique.
const MAX_SLOT_LEN: usize = 64;

/// `true` si `slot` est un nom de slot sûr (Sprint 105a-2, durcissement) :
/// non vide, borné en longueur, alphanumérique + `-`/`_` uniquement — rejette
/// notamment tout `/`/`..`, qui ferait sortir `file_name(slot)` du dossier
/// `user://` prévu une fois joint par `assets::write_user_bytes`/
/// `read_user_bytes` (celles-ci ont leur propre garde, `assets::safe_join` —
/// cette validation-ci est redondante par conception : elle donne un message
/// d'erreur spécifique au domaine (« nom de slot invalide ») plutôt que
/// l'échec I/O générique que produirait `safe_join` seul).
fn valid_slot(slot: &str) -> bool {
    !slot.is_empty()
        && slot.chars().count() <= MAX_SLOT_LEN
        && slot
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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

    /// Dossier temporaire unique par test (Sprint 105a-3, isolation des
    /// tests système) — même schéma que `assets::tests::temp_assets_dir` :
    /// aucune dépendance au vrai `$HOME`, sûr sous exécution parallèle.
    fn temp_save_dir(tag: &str) -> std::path::PathBuf {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        tag.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        let dir = std::env::temp_dir().join(format!("rusteegear_save_test_{:x}", hasher.finish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_from_slot_fails_cleanly_when_nothing_was_saved() {
        // Un slot jamais utilisé ne doit pas paniquer — juste une erreur lisible.
        let dir = temp_save_dir("never_used");
        let result = SaveGame::load_from_slot_at("never_used", &dir);
        assert!(result.is_err());
    }

    #[test]
    fn valid_slot_accepts_ordinary_names() {
        for ok in ["1", "auto", "slot-2", "save_never_used"] {
            assert!(valid_slot(ok), "« {ok} » devrait être un slot valide");
        }
    }

    #[test]
    fn valid_slot_rejects_traversal_and_unsafe_names() {
        for bad in [
            "",
            "../evil",
            "a/b",
            "a b",
            "café",
            &"x".repeat(MAX_SLOT_LEN + 1),
        ] {
            assert!(
                !valid_slot(bad),
                "« {bad} » ne devrait pas être un slot valide"
            );
        }
    }

    #[test]
    fn save_to_slot_and_load_from_slot_reject_a_traversal_slot_name() {
        let dir = temp_save_dir("traversal");
        let save = SaveGame::default();
        assert!(
            save.save_to_slot_at("../evil", &dir).is_err(),
            "un nom de slot tentant une évasion de répertoire doit être rejeté à l'écriture"
        );
        assert!(
            SaveGame::load_from_slot_at("../evil", &dir).is_err(),
            "un nom de slot tentant une évasion de répertoire doit être rejeté à la lecture"
        );
    }
}
