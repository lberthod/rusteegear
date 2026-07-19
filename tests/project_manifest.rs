//! Sprint 3 (audit du 19 juillet 2026) : ouverture d'un projet de bout en bout
//! via `AppState::open_project`, par-dessus les tests unitaires de
//! `crate::project` qui couvrent la validation du manifeste seule. Prouve que
//! le chemin complet — manifeste → résolution de la scène de démarrage →
//! `Scene::load` réel → `current_project` posé — fonctionne, pas seulement
//! chacune de ses briques isolément.

use motor3derust::app::AppState;
use motor3derust::scene::Scene;

/// Un dossier de projet minimal mais réel sous `target/`, jamais `$HOME` (même
/// isolation que les autres tests d'assets/sauvegarde du dépôt).
fn fixture_dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-projects-integration")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).expect("création de l'arborescence de test");
    dir
}

fn write_minimal_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("project.rusteegear.json"),
        r#"{"format": 1, "name": "Projet de test", "main_scene": "scenes/main.scene.json"}"#,
    )
    .expect("écriture du manifeste");
    Scene::default()
        .save(
            dir.join("scenes/main.scene.json")
                .to_str()
                .expect("chemin UTF-8"),
        )
        .expect("écriture d'une scène minimale valide");
}

#[test]
fn opening_a_minimal_project_loads_its_starting_scene_and_sets_current_project() {
    let dir = fixture_dir("minimal");
    write_minimal_project(&dir);

    let mut app = AppState::new();
    assert!(app.current_project.is_none(), "aucun projet au départ");

    app.open_project(&dir)
        .expect("le projet minimal doit s'ouvrir");

    let project = app
        .current_project
        .as_ref()
        .expect("current_project doit être posé après ouverture");
    assert_eq!(project.name, "Projet de test");
    assert_eq!(project.root, dir);
    // La scène par défaut n'a pas d'objets ; ce qui compte est qu'elle a bien
    // été chargée (pas d'erreur) plutôt que de rester la scène précédente.
    assert!(app.scene.objects.is_empty());
    assert!(
        !app.scene_dirty,
        "une scène tout juste chargée n'est pas modifiée"
    );
}

#[test]
fn opening_a_project_without_a_manifest_fails_with_a_clear_message() {
    let dir = fixture_dir("sans-manifeste");
    let mut app = AppState::new();

    let err = app
        .open_project(&dir)
        .expect_err("un dossier sans project.rusteegear.json doit être refusé");
    assert!(
        err.contains("project.rusteegear.json"),
        "message obtenu : {err}"
    );
    assert!(
        app.current_project.is_none(),
        "un échec d'ouverture ne doit pas poser current_project"
    );
}

#[test]
fn opening_a_project_with_an_escaping_main_scene_fails_and_leaves_the_app_untouched() {
    let dir = fixture_dir("evasion-integration");
    std::fs::write(
        dir.join("project.rusteegear.json"),
        r#"{"format": 1, "name": "Évasion", "main_scene": "../ailleurs.json"}"#,
    )
    .expect("écriture du manifeste");

    let mut app = AppState::new();
    let baseline_objects = app.scene.objects.len();

    let err = app
        .open_project(&dir)
        .expect_err("main_scene hors du projet doit être refusé");
    assert!(err.contains("sort du projet"), "message obtenu : {err}");
    assert!(app.current_project.is_none());
    assert_eq!(
        app.scene.objects.len(),
        baseline_objects,
        "la scène courante ne doit pas changer sur un échec d'ouverture"
    );
}
