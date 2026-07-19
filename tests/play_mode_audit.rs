//! Audit Play/Pause/Stop (Phase C1, sprint.19matin.md) : les règles que le
//! tutoriel FIRST_GAME.md et la doc MENTAL_MODEL.md promettent au testeur
//! externe, prouvées sur la vraie boucle de simulation (`advance_play`,
//! headless) et sur la scène exemple `examples/first_game/scene.json`.
//!
//! Rappel du contrat (src/app/simulation.rs, transitions Edit <-> Play) :
//! - Play  : snapshot de `scene.objects`, construction de la physique ;
//! - Pause : reste en Play (snapshot conservé), simulation gelée ;
//! - Stop  : `scene.objects` restauré depuis le snapshot, états de jeu purgés.
//!
//! La sélection d'objet désactivée pendant Play est une décision produit
//! (mémoire projet « scene-selection-disabled-during-play »), hors de portée
//! de ces tests headless.

use motor3derust::app::AppState;
use motor3derust::scene::Scene;

fn first_game() -> Scene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/first_game");
    Scene::load(path.join("scene.json").to_str().unwrap()).expect("scene.json charge")
}

/// Une frame de simulation avec un vrai dt > 0 (`last_frame` est privé : on
/// laisse s'écouler du temps réel plutôt que d'antidater, cf.
/// tests/first_game_example.rs).
fn frame(app: &mut AppState) {
    std::thread::sleep(std::time::Duration::from_millis(25));
    app.advance_play();
}

fn find<'a>(app: &'a AppState, name: &str) -> &'a motor3derust::scene::SceneObject {
    app.scene
        .objects
        .iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("objet « {name} » absent"))
}

#[test]
fn stop_restores_edits_made_during_play() {
    let mut app = AppState::new();
    app.scene = first_game();
    let original_pos = find(&app, "Caisse 1").transform.position;

    app.playing = true;
    frame(&mut app); // capture le snapshot d'entrée en Play

    // « Modif pendant Play » : déplacer un objet en pleine simulation (ce que
    // ferait un gameplay ou une retouche pendant Play).
    app.scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Caisse 1")
        .unwrap()
        .transform
        .position
        .x += 5.0;
    frame(&mut app);

    app.playing = false;
    frame(&mut app); // la restauration s'applique à cette frame

    assert_eq!(
        find(&app, "Caisse 1").transform.position,
        original_pos,
        "Stop doit ramener l'objet à sa position d'avant Play"
    );
}

#[test]
fn stop_restores_an_object_destroyed_during_play() {
    let mut app = AppState::new();
    app.scene = first_game();

    // « Suppression pendant Play » par la voie officielle : obj:destroy() —
    // suppression douce (visible = false), jamais un retrait du tableau.
    app.scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Caisse 2")
        .unwrap()
        .script = "obj:destroy()".into();

    app.playing = true;
    frame(&mut app);
    frame(&mut app);
    assert!(
        !find(&app, "Caisse 2").visible,
        "obj:destroy() doit masquer l'objet pendant Play"
    );

    app.playing = false;
    frame(&mut app);
    assert!(
        find(&app, "Caisse 2").visible,
        "Stop doit faire réapparaître l'objet détruit pendant Play"
    );
    assert!(
        find(&app, "Caisse 2").script.starts_with("obj:destroy"),
        "le script étant une donnée d'objet, il est restauré avec le snapshot"
    );
}

#[test]
fn a_collected_coin_comes_back_after_stop() {
    // La promesse explicite de FIRST_GAME.md §8 : « les pièces ramassées
    // réapparaissent » au Stop.
    let mut app = AppState::new();
    app.scene = first_game();
    let coin_pos = find(&app, "Pièce 1").transform.position;
    app.scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Joueur")
        .unwrap()
        .transform
        .position = coin_pos;

    app.playing = true;
    for _ in 0..3 {
        frame(&mut app);
    }
    assert_eq!(
        app.scene.collectibles(),
        Some((1, 3)),
        "pré-condition : la pièce sous le joueur est ramassée"
    );

    app.playing = false;
    frame(&mut app);
    assert_eq!(
        app.scene.collectibles(),
        Some((0, 3)),
        "après Stop, l'objectif est réarmé : aucune pièce ramassée"
    );
}

#[test]
fn pause_freezes_the_simulation_and_resume_continues_from_the_same_state() {
    let mut app = AppState::new();
    app.scene = first_game();

    app.playing = true;
    for _ in 0..3 {
        frame(&mut app);
    }
    let rotating = find(&app, "Cube tournant").transform.rotation;
    assert!(rotating.w < 0.999_999, "pré-condition : le cube a tourné");

    // Pause : plus rien ne bouge, mais on reste en Play (snapshot conservé).
    app.paused = true;
    let frozen = find(&app, "Cube tournant").transform.rotation;
    for _ in 0..3 {
        frame(&mut app);
    }
    assert_eq!(
        find(&app, "Cube tournant").transform.rotation,
        frozen,
        "en pause, les scripts ne tournent plus"
    );

    // Reprise : la rotation repart d'où elle était (pas d'un état réinitialisé).
    app.paused = false;
    frame(&mut app);
    let resumed = find(&app, "Cube tournant").transform.rotation;
    assert_ne!(
        resumed, frozen,
        "après la reprise, le script tourne à nouveau"
    );

    // Et Stop depuis « après pause » restaure bien l'état d'édition.
    app.playing = false;
    frame(&mut app);
    assert_eq!(
        find(&app, "Cube tournant").transform.rotation.w,
        1.0,
        "Stop restaure la rotation d'édition (identité)"
    );
}

#[test]
fn a_lua_error_in_one_object_neither_panics_nor_stops_the_others() {
    let mut app = AppState::new();
    app.scene = first_game();

    // Erreur d'EXÉCUTION (le chunk compile, puis échoue à chaque frame).
    app.scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Caisse 3")
        .unwrap()
        .script = "error('boom pédagogique')".into();
    // Erreur de COMPILATION (syntaxe invalide).
    app.scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Caisse 1")
        .unwrap()
        .script = "ceci n'est pas du lua ((".into();

    app.playing = true;
    for _ in 0..3 {
        frame(&mut app); // ne doit pas paniquer
    }

    assert!(
        find(&app, "Cube tournant").transform.rotation.w < 0.999_999,
        "les scripts sains continuent de tourner malgré les objets en erreur"
    );

    app.playing = false;
    frame(&mut app);
}

#[test]
fn play_stop_play_starts_from_a_fresh_snapshot_each_time() {
    let mut app = AppState::new();
    app.scene = first_game();

    // 1er Play : on déplace la caisse pendant Play, Stop la restaure.
    app.playing = true;
    frame(&mut app);
    app.scene
        .objects
        .iter_mut()
        .find(|o| o.name == "Caisse 1")
        .unwrap()
        .transform
        .position
        .x += 3.0;
    app.playing = false;
    frame(&mut app);

    // Édition entre deux Play : ce changement-là doit persister au 2e cycle.
    let edited_x = {
        let o = app
            .scene
            .objects
            .iter_mut()
            .find(|o| o.name == "Caisse 1")
            .unwrap();
        o.transform.position.x = -7.5;
        o.transform.position.x
    };

    app.playing = true;
    frame(&mut app);
    app.playing = false;
    frame(&mut app);
    assert_eq!(
        find(&app, "Caisse 1").transform.position.x,
        edited_x,
        "le snapshot du 2e Play doit venir de la scène ÉDITÉE, pas de l'ancien"
    );
}

#[test]
fn loading_an_invalid_scene_json_fails_cleanly_without_panicking() {
    // C2 (part) : un JSON corrompu → Err propre, jamais un crash.
    let tmp = std::env::temp_dir().join(format!("invalid_scene_{}.json", std::process::id()));
    std::fs::write(&tmp, b"{ pas du json valide ][").unwrap();
    let result = Scene::load(tmp.to_str().unwrap());
    let _ = std::fs::remove_file(&tmp);
    assert!(result.is_err(), "un JSON invalide doit renvoyer une erreur");
}
