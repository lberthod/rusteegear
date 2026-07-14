//! Réglages utilisateur persistés : clé API DeepSeek pour la
//! génération de scripts Lua par IA, et config Firebase pour les comptes
//! multijoueur. Stockés dans `~/.motor3derust/settings.json`.

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
    /// Clé API Web Firebase (Project Settings → Web API Key). Laisser vide pour
    /// désactiver les comptes/backend annexe multijoueur (cf. `net::firebase`).
    /// Publique par conception côté Firebase — la sécurité vient des **règles**
    /// RTDB, pas du secret de cette clé (cf. commentaire dans `net::firebase`).
    #[serde(default)]
    pub firebase_api_key: String,
    /// URL de la Realtime Database (ex. `https://xxx-default-rtdb.firebaseio.com`).
    #[serde(default)]
    pub firebase_database_url: String,
    /// Volume (0..1) de la piste musique/ambiance (Sprint 104, cf.
    /// `runtime::audio::Audio::set_music_volume`).
    #[serde(default = "default_volume")]
    pub music_volume: f32,
    /// Volume (0..1) de la piste effets sonores (Sprint 104, cf.
    /// `runtime::audio::Audio::set_sfx_volume`).
    #[serde(default = "default_volume")]
    pub sfx_volume: f32,
}

fn default_model() -> String {
    "deepseek-chat".to_string()
}

fn default_temperature() -> f32 {
    0.2
}

fn default_volume() -> f32 {
    1.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            deepseek_api_key: String::new(),
            deepseek_model: default_model(),
            deepseek_temperature: default_temperature(),
            firebase_api_key: String::new(),
            firebase_database_url: String::new(),
            music_volume: default_volume(),
            sfx_volume: default_volume(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip JSON pur (`serde_json`, pas `Settings::save`/`load` qui
    /// touchent le vrai `$HOME` de la machine — à éviter dans un test).
    #[test]
    fn music_and_sfx_volume_round_trip_at_full_volume_by_default() {
        let settings = Settings::default();
        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.sfx_volume, 1.0);
        let json = serde_json::to_string(&settings).expect("sérialisable");
        let back: Settings = serde_json::from_str(&json).expect("désérialisable");
        assert_eq!(back.music_volume, 1.0);
        assert_eq!(back.sfx_volume, 1.0);
    }

    /// Sprint 104 : un `settings.json` déjà sur le disque d'un utilisateur
    /// (écrit par une version antérieure, sans `music_volume`/`sfx_volume`)
    /// doit continuer à charger, avec les valeurs par défaut pour les
    /// nouveaux champs — même garde-fou que `scene::tests::old_scene_
    /// without_new_fields_loads_with_defaults`.
    #[test]
    fn an_old_settings_file_without_volume_fields_loads_with_defaults() {
        let old_json = r#"{
            "deepseek_api_key": "sk-test",
            "deepseek_model": "deepseek-chat",
            "deepseek_temperature": 0.2,
            "firebase_api_key": "",
            "firebase_database_url": ""
        }"#;
        let settings: Settings = serde_json::from_str(old_json)
            .expect("un ancien settings.json sans les champs volume doit rester lisible");
        assert_eq!(settings.deepseek_api_key, "sk-test");
        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.sfx_volume, 1.0);
    }
}
