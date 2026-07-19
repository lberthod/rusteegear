//! Sprint 4 (audit du 19 juillet 2026) : cycle de vie complet d'un projet
//! depuis `AppState` — création depuis un template, ouverture qui en résulte,
//! fermeture (avec/sans confirmation), et duplication. La liste des projets
//! récents (`Settings::record_recent_project`) est déjà couverte par les
//! tests unitaires de `src/app/settings.rs` (elle vit sur `Settings`, pas sur
//! `AppState` — ce fichier ne la retexte pas).

use motor3derust::app::AppState;
use motor3derust::project::ProjectTemplate;

/// Un dossier parent temporaire sous `target/`, jamais `$HOME` — chaque test a
/// le sien pour ne jamais entrer en collision (`create_project` refuse
/// d'écraser un dossier existant).
fn fixture_parent(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-project-manager")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("création du dossier parent de test");
    dir
}

#[test]
fn creating_an_empty_project_writes_a_valid_manifest_and_opens_it() {
    let parent = fixture_parent("creer-vide");
    let mut app = AppState::new();

    let root = app
        .create_project(&parent, "Mon Premier Jeu", ProjectTemplate::Empty)
        .expect("la création doit réussir");

    assert_eq!(root, parent.join("Mon Premier Jeu"));
    assert!(root.join("project.rusteegear.json").exists());
    assert!(root.join("scenes/main.scene.json").exists());
    assert!(root.join("scripts").is_dir());

    let project = app
        .current_project
        .as_ref()
        .expect("create_project doit ouvrir le projet créé");
    assert_eq!(project.name, "Mon Premier Jeu");
    assert_eq!(project.root, root);
    // Template vide : aucun objet.
    assert!(app.scene.objects.is_empty());
}

#[test]
fn creating_from_the_controller_template_populates_the_scene() {
    let parent = fixture_parent("creer-controleur");
    let mut app = AppState::new();

    app.create_project(&parent, "Démo contrôleur", ProjectTemplate::Controller)
        .expect("la création doit réussir");

    // Le template « démo contrôleur » peuple la scène (joueur pilotable,
    // décor) — contrairement au template vide, elle n'est pas vide.
    assert!(
        !app.scene.objects.is_empty(),
        "le template contrôleur doit peupler la scène"
    );
}

#[test]
fn creating_a_project_twice_at_the_same_location_fails_without_clobbering() {
    let parent = fixture_parent("creer-collision");
    let mut app = AppState::new();
    app.create_project(&parent, "Collision", ProjectTemplate::Empty)
        .expect("première création");

    let err = app
        .create_project(&parent, "Collision", ProjectTemplate::CombatDemo)
        .expect_err("une seconde création au même endroit doit être refusée");
    assert!(err.contains("existe déjà"), "message obtenu : {err}");

    // Le projet ouvert reste celui de la première création, pas écrasé.
    assert_eq!(app.current_project.as_ref().unwrap().name, "Collision");
    assert!(
        app.scene.objects.is_empty(),
        "la tentative refusée ne doit pas avoir chargé le template combat"
    );
}

#[test]
fn creating_a_project_with_an_empty_name_fails() {
    let parent = fixture_parent("creer-nom-vide");
    let mut app = AppState::new();
    let err = app
        .create_project(&parent, "   ", ProjectTemplate::Empty)
        .expect_err("un nom vide (ou blanc) doit être refusé");
    assert!(err.contains("vide"), "message obtenu : {err}");
    assert!(app.current_project.is_none());
}

#[test]
fn request_close_project_closes_immediately_when_the_scene_is_not_dirty() {
    let parent = fixture_parent("fermer-propre");
    let mut app = AppState::new();
    app.create_project(&parent, "À fermer", ProjectTemplate::Empty)
        .expect("création");
    assert!(!app.scene_dirty, "juste après création, rien à sauvegarder");

    app.request_close_project();

    assert!(app.current_project.is_none());
    assert!(!app.confirm_close_project);
}

#[test]
fn request_close_project_asks_for_confirmation_when_the_scene_is_dirty() {
    let parent = fixture_parent("fermer-sale");
    let mut app = AppState::new();
    app.create_project(&parent, "Modifié", ProjectTemplate::Empty)
        .expect("création");
    app.scene_dirty = true;

    app.request_close_project();

    // Ni fermé, ni silencieusement ignoré : la modale doit s'ouvrir.
    assert!(app.current_project.is_some(), "pas fermé sans confirmation");
    assert!(app.confirm_close_project);

    // Un « Fermer sans enregistrer » explicite (simulé ici par close_project
    // directement, comme le fait le consommateur d'actions) ferme bien.
    app.close_project();
    assert!(app.current_project.is_none());
    assert!(!app.confirm_close_project);
}

#[test]
fn request_close_project_without_an_open_project_is_a_no_op() {
    let mut app = AppState::new();
    app.request_close_project();
    assert!(app.current_project.is_none());
    assert!(!app.confirm_close_project);
}

#[test]
fn duplicating_a_project_creates_a_sibling_folder_with_a_renamed_manifest() {
    let parent = fixture_parent("dupliquer");
    let mut app = AppState::new();
    app.create_project(&parent, "Original", ProjectTemplate::Empty)
        .expect("création");

    let dup_root = app
        .duplicate_project()
        .expect("la duplication doit réussir");

    assert_eq!(dup_root, parent.join("Original copie"));
    assert!(dup_root.join("project.rusteegear.json").exists());
    assert!(dup_root.join("scenes/main.scene.json").exists());

    let dup_manifest = motor3derust::project::ProjectManifest::load(&dup_root)
        .expect("le manifeste de la copie doit être valide");
    assert_eq!(dup_manifest.name, "Original copie");

    // Le projet ouvert dans l'éditeur reste l'original, pas la copie.
    assert_eq!(app.current_project.as_ref().unwrap().name, "Original");
}

#[test]
fn duplicating_without_an_open_project_fails() {
    let mut app = AppState::new();
    let err = app
        .duplicate_project()
        .expect_err("aucun projet ouvert : la duplication doit être refusée");
    assert!(err.contains("aucun projet"), "message obtenu : {err}");
}

#[test]
fn duplicating_twice_fails_on_the_second_attempt() {
    let parent = fixture_parent("dupliquer-deux-fois");
    let mut app = AppState::new();
    app.create_project(&parent, "Deux Fois", ProjectTemplate::Empty)
        .expect("création");
    app.duplicate_project().expect("première duplication");

    let err = app
        .duplicate_project()
        .expect_err("la destination existe déjà après la première duplication");
    assert!(err.contains("existe déjà"), "message obtenu : {err}");
}
