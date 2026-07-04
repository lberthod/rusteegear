//! Configuration de build/export persistée (Sprint 20) : nom de l'app, identifiant
//! de bundle, version, numéro de build. Éditée dans le panneau Export, sauvegardée
//! dans `~/.motor3derust/build_config.json` et transmise aux scripts de packaging.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Orientation d'écran imposée à l'app Android.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Orientation {
    #[default]
    Sensor,
    Portrait,
    Landscape,
}

impl Orientation {
    /// Valeur `android:screenOrientation` du manifeste.
    pub fn manifest_value(self) -> &'static str {
        match self {
            Orientation::Sensor => "sensor",
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Orientation::Sensor => "Auto (capteur)",
            Orientation::Portrait => "Portrait",
            Orientation::Landscape => "Paysage",
        }
    }
}

/// Niveau de qualité de rendu visé par le player mobile.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl RenderQuality {
    pub fn label(self) -> &'static str {
        match self {
            RenderQuality::Low => "Basse (perf)",
            RenderQuality::Medium => "Moyenne",
            RenderQuality::High => "Haute (qualité)",
        }
    }

    /// Nombre maximal de lumières ponctuelles envoyées au shader pour ce niveau de
    /// qualité (culling/LOD, cf. `Scene::nearest_point_lights`). Utilisé en mode Play
    /// (éditeur et player exporté) pour que le réglage ait un effet réel sur la perf,
    /// pas seulement sur les métadonnées d'export.
    pub fn light_budget(self) -> usize {
        match self {
            RenderQuality::Low => 2,
            RenderQuality::Medium => 4,
            RenderQuality::High => crate::scene::MAX_POINT_LIGHTS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_budget_scales_with_quality_and_stays_in_bounds() {
        assert_eq!(RenderQuality::Low.light_budget(), 2);
        assert_eq!(RenderQuality::Medium.light_budget(), 4);
        assert_eq!(
            RenderQuality::High.light_budget(),
            crate::scene::MAX_POINT_LIGHTS
        );
        // Chaque niveau doit rester borné par la capacité shader (jamais dépassée).
        for q in [
            RenderQuality::Low,
            RenderQuality::Medium,
            RenderQuality::High,
        ] {
            assert!(q.light_budget() <= crate::scene::MAX_POINT_LIGHTS);
        }
        // Ordre croissant strict : plus de qualité = plus de lumières.
        assert!(RenderQuality::Low.light_budget() < RenderQuality::Medium.light_budget());
        assert!(RenderQuality::Medium.light_budget() < RenderQuality::High.light_budget());
    }
}

fn default_min_sdk() -> u32 {
    26
}
fn default_target_sdk() -> u32 {
    33
}
fn default_fps() -> u32 {
    60
}
fn default_true() -> bool {
    true
}
fn default_msaa() -> u32 {
    1
}

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

    // --- Signature iOS (vide = défauts du script `build_ios.sh`) ---
    /// Team identifier Apple (ex. `N668CK695Q`).
    #[serde(default)]
    pub ios_team_id: String,
    /// Nom de l'identité de signature (« Apple Development: … »).
    #[serde(default)]
    pub ios_identity: String,
    /// Chemin d'un profil de provisioning `.mobileprovision` (pour installer sur device).
    #[serde(default)]
    pub ios_profile: String,

    // --- Application Android (Sprint 39) ---
    /// Orientation d'écran imposée.
    #[serde(default)]
    pub orientation: Orientation,
    /// `minSdkVersion` (API niveau minimal supporté).
    #[serde(default = "default_min_sdk")]
    pub min_sdk: u32,
    /// `targetSdkVersion` (API niveau ciblé).
    #[serde(default = "default_target_sdk")]
    pub target_sdk: u32,
    /// Icône PNG de l'app (vide = icône par défaut du projet).
    #[serde(default)]
    pub icon_path: String,
    /// Image de splash/écran de démarrage (vide = aucun).
    #[serde(default)]
    pub splash_path: String,

    // --- Rendu mobile (Sprint 39) : visés par le player, persistés et transmis au build ---
    /// Qualité de rendu visée.
    #[serde(default)]
    pub render_quality: RenderQuality,
    /// FPS cible (cadence visée par le player).
    #[serde(default = "default_fps")]
    pub target_fps: u32,
    /// Ombres activées dans le player.
    #[serde(default = "default_true")]
    pub shadows: bool,
    /// Anti-aliasing MSAA (1 = désactivé, 2 ou 4 = échantillons).
    #[serde(default = "default_msaa")]
    pub msaa: u32,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            app_name: "MonJeu".into(),
            bundle_id: "com.exemple.monjeu".into(),
            version: "1.0.0".into(),
            build_number: 1,
            ios_team_id: String::new(),
            ios_identity: String::new(),
            ios_profile: String::new(),
            orientation: Orientation::default(),
            min_sdk: default_min_sdk(),
            target_sdk: default_target_sdk(),
            icon_path: String::new(),
            splash_path: String::new(),
            render_quality: RenderQuality::default(),
            target_fps: default_fps(),
            shadows: default_true(),
            msaa: default_msaa(),
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

    /// Dossier des préréglages (`~/.motor3derust/presets/`).
    fn presets_dir() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(".motor3derust").join("presets"))
    }

    /// Liste les préréglages disponibles (par nom de fichier, triés).
    pub fn list_presets() -> Vec<String> {
        let Some(dir) = Self::presets_dir() else {
            return Vec::new();
        };
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension()?.to_str()? == "json")
                    .then(|| p.file_stem()?.to_str().map(str::to_string))?
            })
            .collect();
        names.sort();
        names
    }

    /// Enregistre la config courante comme préréglage nommé.
    pub fn save_preset(&self, name: &str) {
        let Some(dir) = Self::presets_dir() else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join(format!("{name}.json")), json);
        }
    }

    /// Charge un préréglage par nom.
    pub fn load_preset(name: &str) -> Option<Self> {
        let dir = Self::presets_dir()?;
        let s = std::fs::read_to_string(dir.join(format!("{name}.json"))).ok()?;
        serde_json::from_str(&s).ok()
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
pub fn valid_bundle_id(id: &str) -> bool {
    let segs: Vec<&str> = id.split('.').collect();
    segs.len() >= 2
        && segs.iter().all(|s| {
            !s.is_empty()
                && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}
