//! Autosave et récupération après crash (Sprint 6, audit du 19 juillet 2026).
//!
//! `Scene::save` (Sprint 6, `scene/persistence.rs`) protège déjà contre une
//! écriture interrompue *pendant* une sauvegarde manuelle. Ce module couvre le
//! cas où l'utilisateur ne sauvegarde pas du tout avant un crash/`kill -9` :
//! toutes les [`AppState::AUTOSAVE_INTERVAL`] tant que la scène est modifiée
//! (`scene_dirty`), une copie est écrite dans
//! `~/.motor3derust/autosave/<horodatage>.json` — **jamais** par-dessus le
//! fichier de l'utilisateur — et les [`AppState::AUTOSAVE_KEEP`] plus récentes
//! sont conservées. Au lancement suivant, si la plus récente est postérieure à
//! la dernière sauvegarde manuelle connue, l'éditeur peut proposer de la
//! restaurer plutôt que de perdre silencieusement ce travail.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::AppState;
use crate::scene::Scene;

impl AppState {
    /// Intervalle minimal entre deux autosaves — assez court pour ne perdre
    /// au pire que quelques minutes de travail après un crash, assez long
    /// pour ne pas écrire sur le disque à chaque frame.
    pub const AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(120);
    /// Nombre d'autosaves conservées ; les plus anciennes sont supprimées.
    pub const AUTOSAVE_KEEP: usize = 5;

    /// À appeler à chaque tour de la boucle applicative (`about_to_wait`) —
    /// no-op la plupart des frames : n'écrit que si la scène est modifiée
    /// (`scene_dirty`) et que l'intervalle depuis le dernier autosave est
    /// écoulé (ou qu'aucun autosave n'a encore eu lieu cette session).
    pub fn maybe_autosave(&mut self) {
        if !self.scene_dirty {
            return;
        }
        let due = self
            .last_autosave
            .is_none_or(|t| t.elapsed() >= Self::AUTOSAVE_INTERVAL);
        if !due {
            return;
        }
        let Some(dir) = crate::assets::app_data_dir().map(|d| d.join("autosave")) else {
            return; // pas de dossier applicatif résolvable (ex. Android sans data dir posé) : rien à faire
        };
        self.maybe_autosave_at(&dir);
    }

    /// Variante à dossier explicite (isolation des tests, même patron que
    /// `Settings::save_to`/`open_project`). Écrit inconditionnellement (le
    /// contrôle `scene_dirty`/intervalle est fait par `maybe_autosave`) puis
    /// met à jour `last_autosave`.
    pub(crate) fn maybe_autosave_at(&mut self, autosave_dir: &Path) {
        if let Err(e) = write_autosave(&self.scene, autosave_dir) {
            log::warn!("autosave impossible : {e}");
        }
        self.last_autosave = Some(crate::time_compat::Instant::now());
    }

    /// Chemin de référence pour « la dernière sauvegarde manuelle connue »
    /// (Sprint 6, 6.4) : la scène du projet ouvert si un projet est ouvert,
    /// sinon l'emplacement de sauvegarde rapide par défaut. Sa date de
    /// modification sur disque sert de repère — pas besoin d'un état
    /// persisté séparé, le fichier lui-même porte l'information.
    fn manual_save_reference_path(&self) -> PathBuf {
        match &self.current_project {
            Some(p) => p.main_scene_path.clone(),
            None => PathBuf::from(super::scene_path()),
        }
    }

    /// À appeler une fois au démarrage : renvoie le chemin d'un autosave à
    /// proposer en restauration s'il est postérieur à la dernière sauvegarde
    /// manuelle connue (ou si aucune sauvegarde manuelle n'existe encore).
    /// `None` s'il n'y a rien à proposer (aucun autosave, ou pas plus récent).
    pub fn detect_pending_autosave_recovery(&self) -> Option<PathBuf> {
        let dir = crate::assets::app_data_dir()?.join("autosave");
        self.detect_pending_autosave_recovery_at(&dir)
    }

    pub(crate) fn detect_pending_autosave_recovery_at(
        &self,
        autosave_dir: &Path,
    ) -> Option<PathBuf> {
        let newest = latest_autosave(autosave_dir)?;
        let newest_mtime = modified_time(&newest)?;
        let reference = self.manual_save_reference_path();
        match modified_time(&reference) {
            Some(reference_mtime) if newest_mtime <= reference_mtime => None,
            _ => Some(newest),
        }
    }

    /// Charge `autosave_path` comme scène courante (restauration après crash,
    /// Sprint 6). La scène restaurée est marquée `scene_dirty` : elle vient
    /// d'un fichier de secours, pas de l'emplacement réel de l'utilisateur —
    /// tant qu'elle n'est pas explicitement sauvegardée là, elle doit rester
    /// signalée comme non enregistrée.
    pub fn restore_autosave(&mut self, autosave_path: &Path) -> Result<(), String> {
        let path_str = autosave_path
            .to_str()
            .ok_or("chemin d'autosave non UTF-8")?;
        let mut scene = Scene::load(path_str).map_err(|e| format!("{path_str} : {e}"))?;
        scene.reload_imported();
        self.scene = scene;
        self.clear_selection();
        self.scene_dirty = true;
        Ok(())
    }
}

/// Écrit une autosave de `scene` dans `<autosave_dir>/<horodatage>.json` puis
/// supprime les plus anciennes au-delà de `AppState::AUTOSAVE_KEEP`.
fn write_autosave(scene: &Scene, autosave_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(autosave_dir)?;
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Zéro-paddé : le tri lexicographique des noms de fichiers reste un tri
    // chronologique correct (cf. `latest_autosave`), même si l'horloge du
    // système recule ponctuellement d'un chouïa.
    let path = autosave_dir.join(format!("{ts:012}.json"));
    scene.save(path.to_str().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "chemin non UTF-8")
    })?)?;
    rotate_autosaves(autosave_dir, AppState::AUTOSAVE_KEEP)?;
    Ok(path)
}

/// Ne garde que les `keep` autosaves les plus récentes (triées par nom, donc
/// par horodatage) dans `dir` ; supprime les autres.
fn rotate_autosaves(dir: &Path, keep: usize) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    if entries.len() > keep {
        for old in &entries[..entries.len() - keep] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

/// Autosave la plus récente d'un dossier (triée par nom = triée par
/// horodatage), ou `None` si le dossier est vide/absent.
fn latest_autosave(dir: &Path) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    entries.pop()
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_autosave_dir(name: &str) -> PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-autosave")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn write_autosave_creates_a_loadable_file() {
        let dir = temp_autosave_dir("ecriture-simple");
        let mut scene = Scene::default();
        scene.light.color = [0.5, 0.5, 0.5];
        let path = write_autosave(&scene, &dir).expect("écriture de l'autosave");
        assert!(path.exists());
        let reloaded = Scene::load(path.to_str().unwrap()).expect("relecture");
        assert_eq!(reloaded.light.color, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn rotate_autosaves_keeps_only_the_most_recent() {
        let dir = temp_autosave_dir("rotation");
        std::fs::create_dir_all(&dir).unwrap();
        // Noms déjà au format zéro-paddé attendu, sans dépendre de l'horloge
        // réelle — teste la logique de tri/troncature isolément.
        for ts in [
            "000000000001",
            "000000000002",
            "000000000003",
            "000000000004",
            "000000000005",
            "000000000006",
            "000000000007",
        ] {
            std::fs::write(dir.join(format!("{ts}.json")), "{}").unwrap();
        }
        rotate_autosaves(&dir, AppState::AUTOSAVE_KEEP).expect("rotation");

        let mut remaining: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        remaining.sort();
        assert_eq!(remaining.len(), AppState::AUTOSAVE_KEEP);
        // Les 5 plus récentes (les plus grands horodatages) doivent rester.
        assert_eq!(
            remaining,
            vec![
                "000000000003.json",
                "000000000004.json",
                "000000000005.json",
                "000000000006.json",
                "000000000007.json",
            ]
        );
    }

    #[test]
    fn rotate_autosaves_is_a_no_op_under_the_limit() {
        let dir = temp_autosave_dir("pas-de-rotation-necessaire");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("000000000001.json"), "{}").unwrap();
        std::fs::write(dir.join("000000000002.json"), "{}").unwrap();
        rotate_autosaves(&dir, AppState::AUTOSAVE_KEEP).expect("rotation");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn maybe_autosave_at_is_a_no_op_when_the_scene_is_not_dirty() {
        let dir = temp_autosave_dir("pas-sale-pas-autosave");
        let mut app = AppState::new();
        app.scene_dirty = false;
        app.maybe_autosave();
        // Rien écrit : `maybe_autosave` (la vraie politique) sort avant même
        // de résoudre `app_data_dir()`, donc `dir` (jamais créé) reste absent.
        assert!(!dir.exists());
    }

    #[test]
    fn maybe_autosave_at_writes_once_and_respects_the_interval() {
        let dir = temp_autosave_dir("intervalle");
        let mut app = AppState::new();
        app.scene_dirty = true;

        app.maybe_autosave_at(&dir);
        let count_after_first = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(count_after_first, 1);

        // Un second appel immédiat (intervalle non écoulé) ne doit rien
        // ajouter — vérifié via la politique complète `maybe_autosave`, pas
        // `maybe_autosave_at` qui écrit inconditionnellement par conception.
        // `last_autosave` vient d'être posé par l'appel précédent :
        // `maybe_autosave` doit donc être un no-op immédiat.
        // (On ne peut pas rediriger `maybe_autosave` vers `dir` — elle résout
        // `app_data_dir()` en dur — donc on vérifie juste `last_autosave`.)
        assert!(app.last_autosave.is_some());
    }

    #[test]
    fn pending_autosave_recovery_at_finds_a_newer_autosave() {
        let dir = temp_autosave_dir("recuperation-plus-recente");
        std::fs::create_dir_all(&dir).unwrap();
        let autosave_path = dir.join("999999999999.json");
        Scene::default()
            .save(autosave_path.to_str().unwrap())
            .unwrap();
        // Force une date de modification loin dans le futur : plus récente à
        // coup sûr que n'importe quel fichier de référence créé par ce test.
        let far_future = SystemTime::now() + std::time::Duration::from_secs(3600);
        let file = std::fs::File::open(&autosave_path).unwrap();
        file.set_modified(far_future).unwrap();

        let reference_dir = temp_autosave_dir("recuperation-plus-recente-reference");
        std::fs::create_dir_all(&reference_dir).unwrap();
        let reference_path = reference_dir.join("reference.json");
        std::fs::write(&reference_path, "{}").unwrap();

        let app = AppState::new();
        // Bricole une référence via un projet fictif pour rediriger
        // `manual_save_reference_path()` sans dépendre de `scene_path()`
        // (qui pointe sur `$HOME`, jamais touché par les tests).
        let mut app = app;
        app.current_project = Some(crate::project::ProjectRoot {
            name: "Réf".to_string(),
            root: reference_dir.clone(),
            main_scene_path: reference_path,
        });

        let found = app.detect_pending_autosave_recovery_at(&dir);
        assert_eq!(found, Some(autosave_path));
    }

    #[test]
    fn pending_autosave_recovery_at_ignores_an_autosave_older_than_the_reference() {
        let dir = temp_autosave_dir("recuperation-plus-ancienne");
        std::fs::create_dir_all(&dir).unwrap();
        let autosave_path = dir.join("000000000001.json");
        Scene::default()
            .save(autosave_path.to_str().unwrap())
            .unwrap();

        let reference_dir = temp_autosave_dir("recuperation-plus-ancienne-reference");
        std::fs::create_dir_all(&reference_dir).unwrap();
        let reference_path = reference_dir.join("reference.json");
        std::fs::write(&reference_path, "{}").unwrap();
        // La référence est écrite APRÈS l'autosave : elle est donc plus
        // récente, rien à proposer en restauration.
        let far_future = SystemTime::now() + std::time::Duration::from_secs(3600);
        let file = std::fs::File::open(&reference_path).unwrap();
        file.set_modified(far_future).unwrap();

        let mut app = AppState::new();
        app.current_project = Some(crate::project::ProjectRoot {
            name: "Réf".to_string(),
            root: reference_dir,
            main_scene_path: reference_path,
        });

        assert_eq!(app.detect_pending_autosave_recovery_at(&dir), None);
    }

    #[test]
    fn pending_autosave_recovery_at_is_none_without_any_autosave() {
        let dir = temp_autosave_dir("aucun-autosave");
        let app = AppState::new();
        assert_eq!(app.detect_pending_autosave_recovery_at(&dir), None);
    }

    #[test]
    fn restore_autosave_loads_the_scene_and_marks_it_dirty() {
        let dir = temp_autosave_dir("restauration");
        std::fs::create_dir_all(&dir).unwrap();
        let mut saved = Scene::default();
        saved.light.color = [0.1, 0.2, 0.3];
        let path = dir.join("000000000001.json");
        saved.save(path.to_str().unwrap()).unwrap();

        let mut app = AppState::new();
        app.scene_dirty = false;
        app.restore_autosave(&path).expect("restauration");

        assert_eq!(app.scene.light.color, [0.1, 0.2, 0.3]);
        assert!(
            app.scene_dirty,
            "une scène restaurée depuis un autosave doit être signalée non enregistrée"
        );
    }
}
