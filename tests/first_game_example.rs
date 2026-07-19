//! Preuves du projet exemple `examples/first_game/` (Phase B, sprint.19matin.md) :
//! la scène passe par le vrai chargeur (`Scene::load`, migration comprise) et
//! contient exactement ce que le README promet — joueur pilotable, objet animé
//! par script, zone déclencheuse, objectif à 3 pièces, aucun asset externe.
//! Garde-fou supplémentaire : les copies lisibles `scripts/*.lua` restent
//! synchronisées avec les scripts inline de `scene.json` (c'est la promesse
//! faite au lecteur du dossier).

use motor3derust::scene::{Scene, TapAction};

fn example_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/first_game")
}

fn load_scene() -> Scene {
    let path = example_dir().join("scenes/main.scene.json");
    Scene::load(path.to_str().unwrap())
        .expect("examples/first_game/scenes/main.scene.json doit charger via le vrai chargeur")
}

#[test]
fn first_game_scene_loads_and_matches_the_readme_promise() {
    let scene = load_scene();

    // Migration : la scène est au format courant (pas de legacy silencieux).
    assert_eq!(scene.version, Scene::CURRENT_VERSION);

    // Joueur pilotable au format ACTUEL (`controller.input`) — pas l'ancien
    // champ plat `input_receiver`, disparu du moteur (les scènes
    // `assets/examples/demo_*.json` datent de ce format et ne donnent plus un
    // joueur pilotable ; ce test empêche first_game de vieillir pareil).
    let joueur = scene
        .objects
        .iter()
        .find(|o| o.name == "Joueur")
        .expect("un objet « Joueur »");
    assert!(
        joueur.controller.as_ref().is_some_and(|c| c.input),
        "le Joueur doit être pilotable (controller.input)"
    );

    // Objet animé : un script non vide qui touche obj.ry.
    let tournant = scene
        .objects
        .iter()
        .find(|o| o.name == "Cube tournant")
        .expect("un objet « Cube tournant »");
    assert!(tournant.script.contains("obj.ry"));

    // Zone déclencheuse.
    let zone = scene
        .objects
        .iter()
        .find(|o| o.name == "Zone d'éveil")
        .expect("une « Zone d'éveil »");
    assert!(zone.trigger, "la zone doit être un trigger");
    assert!(zone.script.contains("obj.triggered"));

    // Objectif : exactement 3 pièces comptées par la vraie logique du moteur
    // (`collectibles()` : tap_action == Hide && respawn_delay == 0).
    assert_eq!(
        scene.collectibles(),
        Some((0, 3)),
        "3 pièces à ramasser, aucune ramassée au chargement"
    );
    // Et ce sont bien les « Pièce N » du README.
    let pieces = scene
        .objects
        .iter()
        .filter(|o| o.tap_action == TapAction::Hide && o.respawn_delay == 0.0)
        .count();
    assert_eq!(pieces, 3);

    // Aucun asset externe : la scène doit s'ouvrir depuis n'importe quel clone,
    // sans import préalable ni dépendance à ~/.motor3derust/.
    assert!(
        scene.imported.is_empty(),
        "first_game ne doit référencer aucun mesh importé"
    );
    for o in &scene.objects {
        assert!(
            o.texture.is_empty(),
            "{} : pas de texture externe dans l'exemple",
            o.name
        );
    }
}

#[test]
fn first_game_actually_plays_script_runs_and_a_coin_can_be_collected() {
    // La scène ne doit pas seulement charger : elle doit JOUER. On la fait
    // tourner dans la vraie boucle de Play (`advance_play`, scripts Lua +
    // simulation) — même patron que les tests de démos internes, mais depuis
    // l'extérieur du crate : `last_frame` étant privé, on laisse du vrai temps
    // s'écouler entre les frames (dt réel > 0) au lieu de l'antidater.
    let mut app = motor3derust::app::AppState::new();
    app.scene = load_scene();

    // Le joueur démarre sur une pièce : le ramassage par contact doit compter.
    let coin_pos = app
        .scene
        .objects
        .iter()
        .find(|o| o.name == "Pièce 1")
        .unwrap()
        .transform
        .position;
    let joueur = app
        .scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Joueur")
        .unwrap();
    joueur.transform.position = coin_pos;

    app.playing = true;
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        app.advance_play();
    }

    // Le script inline a tourné : la rotation du cube n'est plus l'identité
    // (lecture par les champs publics du quaternion, sans dépendre de glam).
    let q = app
        .scene
        .objects
        .iter()
        .find(|o| o.name == "Cube tournant")
        .unwrap()
        .transform
        .rotation;
    assert!(
        q.w < 0.999_999,
        "après ~150 ms de Play à 45°/s, le cube doit avoir tourné (w = {})",
        q.w
    );

    // Et la pièce sous le joueur a été ramassée par la vraie logique de jeu.
    assert_eq!(
        app.scene.collectibles(),
        Some((1, 3)),
        "le joueur posé sur la Pièce 1 doit l'avoir ramassée"
    );
}

#[test]
fn first_game_survives_a_save_and_reload_round_trip() {
    // L'étape « Enregistrer sous… puis Ouvrir » du tutoriel (FIRST_GAME.md §9)
    // par les mêmes fonctions que l'éditeur : Scene::save → Scene::load.
    let scene = load_scene();
    let tmp =
        std::env::temp_dir().join(format!("first_game_roundtrip_{}.json", std::process::id()));
    scene.save(tmp.to_str().unwrap()).expect("sauvegarde");
    let reloaded = Scene::load(tmp.to_str().unwrap()).expect("relecture");
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(reloaded.objects.len(), scene.objects.len());
    for (a, b) in scene.objects.iter().zip(&reloaded.objects) {
        assert_eq!(a.name, b.name);
        assert_eq!(
            a.script, b.script,
            "{} : script perdu au round-trip",
            a.name
        );
    }
    assert_eq!(reloaded.collectibles(), scene.collectibles());
    assert_eq!(reloaded.point_lights.len(), 1);
}

/// Extrait le code effectif d'un script (lignes non vides hors commentaires
/// `--`) pour comparer un script inline de `scene.json` à sa copie lisible
/// `scripts/*.lua` sans être sensible aux commentaires pédagogiques.
fn effective_lines(script: &str) -> Vec<String> {
    script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("--"))
        .map(str::to_string)
        .collect()
}

/// Sprint 5 (audit du 19 juillet 2026) : First Game est maintenant un vrai
/// projet RusteeGear (`project.rusteegear.json`), plus seulement un fichier de
/// scène isolé — il doit s'ouvrir par `AppState::open_project`, exactement
/// comme un projet créé par l'assistant « Nouveau projet » (Sprint 4).
#[test]
fn first_game_opens_as_a_project_via_its_manifest() {
    let mut app = motor3derust::app::AppState::new();
    let count = app
        .open_project(&example_dir())
        .expect("examples/first_game doit s'ouvrir comme un projet");

    let project = app
        .current_project
        .as_ref()
        .expect("open_project doit poser current_project");
    assert_eq!(project.name, "First Game");
    assert_eq!(project.root, example_dir());
    assert_eq!(
        project.main_scene_path,
        example_dir().join("scenes/main.scene.json")
    );
    assert_eq!(count, app.scene.objects.len());
    assert!(!app.scene.objects.is_empty());
}

#[test]
fn the_readable_lua_copies_stay_in_sync_with_the_inline_scripts() {
    let scene = load_scene();
    for (object, file) in [
        ("Cube tournant", "scripts/rotating_object.lua"),
        ("Zone d'éveil", "scripts/zone_signal.lua"),
    ] {
        let inline = &scene
            .objects
            .iter()
            .find(|o| o.name == object)
            .unwrap_or_else(|| panic!("objet « {object} » absent"))
            .script;
        let copy = std::fs::read_to_string(example_dir().join(file))
            .unwrap_or_else(|e| panic!("{file} illisible : {e}"));
        assert_eq!(
            effective_lines(inline),
            effective_lines(&copy),
            "le code de {file} doit rester identique au script inline de « {object} » \
             (mettre à jour les deux ensemble)"
        );
    }
}
