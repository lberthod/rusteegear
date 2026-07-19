//! Garde-fou des scènes d'exemple `assets/examples/` : chacune doit passer par
//! le vrai chargeur (`Scene::load`, migration comprise) et contenir un objet
//! pilotable au format ACTUEL (`controller.input`). Les deux scènes dataient de
//! l'ancien champ plat `input_receiver`, disparu de `SceneObject` — serde
//! l'ignorait silencieusement et le « Joueur » n'était plus pilotable au
//! chargement. Ce test empêche la régression de revenir (même modèle que
//! `tests/first_game_example.rs`).

use motor3derust::scene::Scene;

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/examples")
}

#[test]
fn every_example_scene_loads_and_has_a_controllable_player() {
    let mut checked = 0;
    for entry in std::fs::read_dir(examples_dir()).expect("assets/examples/ doit exister") {
        let path = entry.expect("entrée lisible").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let scene = Scene::load(path.to_str().unwrap())
            .unwrap_or_else(|e| panic!("{name} doit charger via le vrai chargeur : {e}"));

        // La scène est au format courant (pas de legacy silencieux).
        assert_eq!(scene.version, Scene::CURRENT_VERSION, "{name} : version");

        // Un objet pilotable existe au format actuel — pas l'ancien champ plat
        // `input_receiver`, que serde ignorerait sans erreur.
        assert!(
            scene
                .objects
                .iter()
                .any(|o| o.controller.as_ref().is_some_and(|c| c.input)),
            "{name} : aucun objet pilotable (controller.input) — scène restée \
             à un format legacy ?"
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "au moins demo_controleur.json et demo_composants.json attendus \
         ({checked} scène(s) vérifiée(s))"
    );
}
