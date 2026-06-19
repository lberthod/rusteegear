//! Réglages utilisateur persistés (Sprint 33) : clé API DeepSeek pour la
//! génération de scripts Lua par IA. Stockés dans `~/.motor3derust/settings.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Clé API DeepSeek (laisser vide pour désactiver la génération IA).
    #[serde(default)]
    pub deepseek_api_key: String,
    /// Modèle DeepSeek à utiliser (`deepseek-chat`, `deepseek-reasoner`, ou un id précis).
    #[serde(default = "default_model")]
    pub deepseek_model: String,
    /// Température de génération (0 = déterministe, 1 = créatif).
    #[serde(default = "default_temperature")]
    pub deepseek_temperature: f32,
}

fn default_model() -> String {
    "deepseek-chat".to_string()
}

fn default_temperature() -> f32 {
    0.2
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            deepseek_api_key: String::new(),
            deepseek_model: default_model(),
            deepseek_temperature: default_temperature(),
        }
    }
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
