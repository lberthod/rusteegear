//! Preuves de `examples/broken_scene/` (Phase C4, sprint.19matin.md) : une
//! scène pleine de pannes délibérées (asset manquant, Lua en erreur, référence
//! de mesh invalide) doit s'ouvrir, se jouer et échouer **proprement** — les
//! erreurs nomment l'objet/le chemin fautif, les objets sains continuent.

use motor3derust::app::AppState;
use motor3derust::scene::Scene;

fn load_broken() -> Scene {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/broken_scene/scene.json");
    let mut scene = Scene::load(path.to_str().unwrap())
        .expect("la scène cassée doit CHARGER (les pannes sont dans son contenu, pas son format)");
    // Comme l'éditeur après un Ouvrir : recharge les imports (c'est ici que
    // l'asset manquant échoue — en loguant, pas en cassant le chargement).
    scene.reload_imported();
    scene
}

#[test]
fn the_broken_scene_loads_plays_and_fails_cleanly_with_named_errors() {
    // Capture des logs (même tampon que la Console de l'éditeur).
    motor3derust::log_buffer::install();
    motor3derust::log_buffer::clear();

    let mut app = AppState::new();
    app.scene = load_broken();

    // L'asset manquant n'a laissé aucune géométrie, mais l'objet existe.
    let statue_idx = app
        .scene
        .objects
        .iter()
        .position(|o| o.name.starts_with("Statue absente"))
        .expect("l'objet à l'asset manquant existe");
    assert!(
        app.scene.imported[0].data.vertices.is_empty(),
        "pas de géométrie pour un asset introuvable"
    );
    let _ = statue_idx;

    // Play : personne ne panique, le témoin sain tourne malgré les 3 pannes.
    app.playing = true;
    for _ in 0..4 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        app.advance_play();
    }
    let temoin = app
        .scene
        .objects
        .iter()
        .find(|o| o.name.starts_with("Témoin sain"))
        .unwrap();
    assert!(
        temoin.transform.rotation.w < 0.999_999,
        "le témoin sain doit tourner malgré les objets en panne"
    );
    app.playing = false;
    app.advance_play();

    // Les erreurs sont capturées ET nomment le fautif (objet ou chemin) —
    // c'est ce qui rend la panne diagnosticable par un utilisateur.
    let logs = motor3derust::log_buffer::snapshot().join("\n");
    assert!(
        logs.contains("asset://modele_disparu.glb"),
        "l'erreur d'asset manquant doit citer le chemin ; logs :\n{logs}"
    );
    assert!(
        logs.contains("Cube en panne (erreur d'exécution)"),
        "l'erreur Lua d'exécution doit citer le NOM de l'objet ; logs :\n{logs}"
    );
    assert!(
        logs.contains("Cube en panne (erreur de syntaxe)"),
        "l'erreur de compilation doit citer le NOM de l'objet ; logs :\n{logs}"
    );
    // mlua inclut la position dans le source ("[string ...]:1:") : l'erreur
    // d'exécution est localisée à la ligne, pas juste nommée.
    assert!(
        logs.contains(":1:"),
        "l'erreur Lua doit inclure la ligne fautive ; logs :\n{logs}"
    );
    // Et le chunk est nommé d'après l'objet (Phase C5) — pas d'après le
    // call-site Rust (`src/app/simulation.rs:NNN`), illisible pour l'utilisateur.
    assert!(
        logs.contains("script de « Cube en panne"),
        "le chunk Lua doit porter le nom de l'objet ; logs :\n{logs}"
    );
    // Le message d'asset manquant propose une réparation (quoi/pourquoi/réparer).
    assert!(
        logs.contains("Réimportez"),
        "l'erreur d'asset manquant doit proposer une réparation ; logs :\n{logs}"
    );

    check_invalid_glb_import_reports_the_file_and_a_fix();
}

/// Suite du test principal (pas un `#[test]` séparé : le tampon de logs est
/// GLOBAL au process, deux tests parallèles s'écraseraient mutuellement les
/// messages — c'est arrivé, `clear()` de l'un effaçant les preuves de l'autre).
fn check_invalid_glb_import_reports_the_file_and_a_fix() {
    let tmp = std::env::temp_dir().join(format!("pas_un_glb_{}.glb", std::process::id()));
    std::fs::write(&tmp, b"ceci n'est pas un fichier glTF").unwrap();

    let mut app = AppState::new();
    app.scene = load_broken();
    let objects_before = app.scene.objects.len();

    app.import_gltf(tmp.to_str().unwrap());
    // L'import tourne en thread de fond ; on laisse quelques frames à poll_imports.
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        app.advance_play();
        let logs = motor3derust::log_buffer::snapshot().join("\n");
        if logs.contains("Import glTF échoué") {
            break;
        }
    }
    let _ = std::fs::remove_file(&tmp);

    let logs = motor3derust::log_buffer::snapshot().join("\n");
    assert!(
        logs.contains("Import glTF échoué") && logs.contains("pas_un_glb"),
        "l'erreur d'import doit citer le fichier fautif ; logs :\n{logs}"
    );
    assert!(
        logs.contains(".glb/.gltf valide"),
        "l'erreur d'import doit proposer une piste de réparation ; logs :\n{logs}"
    );
    assert_eq!(
        app.scene.objects.len(),
        objects_before,
        "un import raté ne doit pas modifier la scène"
    );
}
