//! Manifeste de projet (Sprint 3, audit du 19 juillet 2026) : jusqu'ici RusteeGear
//! n'ouvrait que des fichiers de scène isolés (`examples/first_game/scene.json`),
//! sans savoir qu'un dossier constitue un « jeu ». `ProjectManifest` donne un point
//! d'entrée explicite (`project.rusteegear.json`) qui déclare la scène de démarrage
//! d'un projet.
//!
//! Périmètre de ce sprint : le manifeste et son ouverture uniquement. Les assets
//! restent résolus comme avant (dossier global `assets::assets_dir()`, schémas
//! `asset://`/`asset-id://`/`bundle://`) — un projet ne redéfinit pas encore où
//! vivent ses propres assets. La cible long terme (assets **par** projet, index
//! d'assets, commande « Convertir en projet ») est documentée dans
//! `docs/SprintAudit12h24.md` (Sprint 3, « cible long terme ») et
//! `docs/KNOWN_LIMITATIONS.md` — pas dans ce module.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nom du fichier de manifeste attendu à la racine d'un projet.
pub const MANIFEST_FILE: &str = "project.rusteegear.json";

/// Version de format la plus récente comprise par cette version du moteur.
/// `ProjectManifest::load` refuse tout manifeste dont `format` est supérieur.
const CURRENT_FORMAT: u32 = 1;

/// Contenu de `project.rusteegear.json`. `build` est optionnel : un projet minimal
/// (comme `examples/first_game` avant le Sprint 5) n'a pas encore de config
/// d'export dédiée et retombe sur le panneau Build & Export existant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectManifest {
    /// Version du format de ce fichier. Un moteur plus ancien que le manifeste
    /// (`format` supérieur à [`CURRENT_FORMAT`]) doit refuser de l'ouvrir plutôt
    /// que de mal l'interpréter en silence.
    pub format: u32,
    /// Nom affiché du projet (fenêtre, projets récents — Sprint 4).
    pub name: String,
    /// Chemin de la scène de démarrage, relatif à la racine du projet (jamais un
    /// chemin absolu ni `..` — validé par [`ProjectManifest::resolve_main_scene`]
    /// via `assets::safe_join`).
    pub main_scene: String,
    /// Config de build (`build.json`), pour l'instant non exploitée par le moteur
    /// (le panneau Export garde sa configuration séparée, cf. `app::build_config`).
    /// Réservé pour un futur sprint d'intégration.
    #[serde(default)]
    pub build: Option<String>,
}

/// Projet actuellement ouvert dans l'éditeur (posé sur `AppState::current_project`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRoot {
    /// Nom déclaré par le manifeste (`ProjectManifest::name`), pas le nom du dossier.
    pub name: String,
    /// Dossier racine du projet (celui qui contient `project.rusteegear.json`).
    pub root: PathBuf,
}

impl ProjectManifest {
    /// Cherche et charge `project.rusteegear.json` dans `dir`. Erreurs en
    /// français, actionnables : le message dit quel fichier, quel problème, et
    /// souvent quoi corriger — même esprit que les erreurs de chargement de
    /// scène (`AppState::load_from`).
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "{} introuvable dans {} ({e}) — ce dossier est-il bien un projet RusteeGear ?",
                MANIFEST_FILE,
                dir.display()
            )
        })?;
        let manifest: Self = serde_json::from_str(&text)
            .map_err(|e| format!("{} invalide : {e}", path.display()))?;
        if manifest.format > CURRENT_FORMAT {
            return Err(format!(
                "{} déclare le format {}, mais cette version de RusteeGear ne comprend \
                 que jusqu'au format {CURRENT_FORMAT} — mets à jour l'éditeur.",
                path.display(),
                manifest.format
            ));
        }
        if manifest.name.trim().is_empty() {
            return Err(format!("{} : le champ « name » est vide", path.display()));
        }
        Ok(manifest)
    }

    /// Résout `main_scene` en chemin absolu, à l'intérieur de `dir` uniquement —
    /// un `main_scene` du type `"../ailleurs.json"` ou absolu est refusé (même
    /// garde que les assets de projet, cf. `assets::safe_join`).
    pub fn resolve_main_scene(&self, dir: &Path) -> Result<PathBuf, String> {
        let resolved = crate::assets::safe_join(dir, &self.main_scene).ok_or_else(|| {
            format!(
                "« {} » sort du projet (chemin absolu ou `..`) — refusé",
                self.main_scene
            )
        })?;
        if !resolved.exists() {
            return Err(format!(
                "scène de démarrage introuvable : {} (déclarée par {})",
                resolved.display(),
                MANIFEST_FILE
            ));
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &Path, json: &str) {
        std::fs::write(dir.join(MANIFEST_FILE), json).expect("écriture du manifeste de test");
    }

    fn temp_dir(name: &str) -> PathBuf {
        // Un sous-dossier par test sous `target/`, comme les autres tests
        // d'isolation du dépôt (cf. `assets::temp_assets_dir`) — pas `$HOME`.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-projects")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("création du dossier de test");
        dir
    }

    #[test]
    fn loads_a_valid_manifest() {
        let dir = temp_dir("valid");
        write_manifest(
            &dir,
            r#"{"format": 1, "name": "Mon jeu", "main_scene": "scenes/main.scene.json"}"#,
        );
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::write(dir.join("scenes/main.scene.json"), "{}").unwrap();

        let manifest = ProjectManifest::load(&dir).expect("manifeste valide");
        assert_eq!(manifest.name, "Mon jeu");
        assert_eq!(manifest.build, None);
        let scene = manifest
            .resolve_main_scene(&dir)
            .expect("scène de démarrage résolue");
        assert_eq!(scene, dir.join("scenes/main.scene.json"));
    }

    #[test]
    fn rejects_a_format_newer_than_understood() {
        let dir = temp_dir("format-trop-recent");
        write_manifest(
            &dir,
            r#"{"format": 99, "name": "Futur", "main_scene": "main.scene.json"}"#,
        );
        let err = ProjectManifest::load(&dir).expect_err("format 99 doit être refusé");
        assert!(
            err.contains("format 99") && err.contains("99"),
            "le message doit citer le format en cause : {err}"
        );
    }

    #[test]
    fn rejects_a_main_scene_escaping_the_project_root() {
        let dir = temp_dir("evasion");
        write_manifest(
            &dir,
            r#"{"format": 1, "name": "Évasion", "main_scene": "../ailleurs.json"}"#,
        );
        let manifest = ProjectManifest::load(&dir).expect("le manifeste lui-même est valide");
        let err = manifest
            .resolve_main_scene(&dir)
            .expect_err("../ailleurs.json doit être refusé");
        assert!(err.contains("sort du projet"), "message obtenu : {err}");
    }

    #[test]
    fn rejects_a_missing_manifest_with_a_clear_message() {
        let dir = temp_dir("absent");
        let err = ProjectManifest::load(&dir).expect_err("dossier sans manifeste");
        assert!(
            err.contains(MANIFEST_FILE),
            "le message doit nommer le fichier attendu : {err}"
        );
    }

    #[test]
    fn rejects_a_missing_main_scene_file() {
        let dir = temp_dir("scene-absente");
        write_manifest(
            &dir,
            r#"{"format": 1, "name": "Sans scène", "main_scene": "scenes/main.scene.json"}"#,
        );
        let manifest = ProjectManifest::load(&dir).expect("manifeste valide");
        let err = manifest
            .resolve_main_scene(&dir)
            .expect_err("le fichier de scène n'existe pas sur disque");
        assert!(err.contains("introuvable"), "message obtenu : {err}");
    }

    #[test]
    fn rejects_an_empty_name() {
        let dir = temp_dir("nom-vide");
        write_manifest(
            &dir,
            r#"{"format": 1, "name": "", "main_scene": "main.scene.json"}"#,
        );
        let err = ProjectManifest::load(&dir).expect_err("un nom vide doit être refusé");
        assert!(err.contains("name"), "message obtenu : {err}");
    }

    #[test]
    fn rejects_invalid_json() {
        let dir = temp_dir("json-invalide");
        // Pas d'accolade isolée dans cette chaîne : casserait le comptage
        // d'accolades de `scripts/check_unwrap_budget.py`, qui détecte les
        // modules de test par correspondance de `{`/`}` plutôt que par une
        // vraie analyse syntaxique (limitation documentée dans son docstring).
        write_manifest(&dir, "ceci n'est pas du JSON");
        let err = ProjectManifest::load(&dir).expect_err("JSON invalide doit être refusé");
        assert!(err.contains(MANIFEST_FILE), "message obtenu : {err}");
    }
}
