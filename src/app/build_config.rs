//! Configuration de build/export persistée (Sprint 20) : nom de l'app, identifiant
//! de bundle, version, numéro de build. Éditée dans le panneau Export, sauvegardée
//! dans `~/.motor3derust/build_config.json` et transmise aux scripts de packaging.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Nom affiché / nom du fichier de sortie.
    pub app_name: String,
    /// Identifiant inversé (ex. `com.exemple.monjeu`).
    pub bundle_id: String,
    /// Version « marketing » (CFBundleShortVersionString / versionName).
    pub version: String,
    /// Numéro de build interne, incrémenté à chaque export.
    pub build_number: u32,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            app_name: "MonJeu".into(),
            bundle_id: "com.exemple.monjeu".into(),
            version: "1.0.0".into(),
            build_number: 1,
        }
    }
}

impl BuildConfig {
    fn path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join(".motor3derust")
                .join("build_config.json"),
        )
    }

    /// Charge la config depuis le disque, ou les valeurs par défaut.
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persiste la config (crée `~/.motor3derust/` au besoin).
    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, json);
        }
    }

    /// Nom de fichier nettoyé (alphanumérique, `-`, `_`) ; défaut « MonJeu ».
    pub fn safe_name(&self) -> String {
        let n: String = self
            .app_name
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if n.is_empty() { "MonJeu".into() } else { n }
    }

    /// Vérifie que la config est exploitable ; renvoie le premier problème détecté.
    pub fn validate(&self) -> Result<(), String> {
        if self.app_name.trim().is_empty() {
            return Err("nom d'app requis".into());
        }
        if self.version.trim().is_empty() {
            return Err("version requise".into());
        }
        if !valid_bundle_id(&self.bundle_id) {
            return Err("bundle id : segments alphanumériques séparés par des points".into());
        }
        Ok(())
    }
}

/// Identifiant valide : au moins deux segments, chacun commençant par une lettre,
/// composé de lettres/chiffres/`-`, séparés par des points.
fn valid_bundle_id(id: &str) -> bool {
    let segs: Vec<&str> = id.split('.').collect();
    segs.len() >= 2
        && segs.iter().all(|s| {
            !s.is_empty()
                && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}
