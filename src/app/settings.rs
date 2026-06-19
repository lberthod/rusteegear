//! Réglages utilisateur persistés (Sprint 33) : clé API DeepSeek pour la
//! génération de scripts Lua par IA. Stockés dans `~/.motor3derust/settings.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Clé API DeepSeek (laisser vide pour désactiver la génération IA).
    #[serde(default)]
    pub deepseek_api_key: String,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join(".motor3derust")
                .join("settings.json"),
        )
    }

    /// Charge les réglages depuis le disque, ou les valeurs par défaut.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persiste les réglages (crée `~/.motor3derust/` au besoin).
    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, json);
        }
    }
}
