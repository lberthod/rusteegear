//! Test d'intégration du pont de pilotage (`motor3derust::pilot`) : démarre le
//! serveur TCP sur un port éphémère avec un `AppState` headless (sans fenêtre ni
//! GPU — exécutable en CI), puis pilote l'application via de vraies requêtes TCP
//! JSON-lines, comme le ferait le client `pilot` ou un agent : éval Lua, Play,
//! inspection d'état/scène, injection d'entrées, gestion d'erreur.
//!
//! Les tests qui ouvrent un socket sont derrière la feature `net_tests`, comme
//! les tests réseau de `src/bin/server.rs` : certains environnements (runners
//! CI restreints, sandboxes) interdisent même le bind loopback, et
//! `cargo test --all-targets` doit passer partout. Couverture complète :
//! `cargo test --features net_tests --test pilot_bridge` (job CI `net-tests`).

#[cfg(feature = "net_tests")]
use std::io::{BufRead, BufReader, Write};
#[cfg(feature = "net_tests")]
use std::net::TcpStream;
#[cfg(feature = "net_tests")]
use std::time::{Duration, Instant};

use motor3derust::app::AppState;
#[cfg(feature = "net_tests")]
use motor3derust::pilot::PilotServer;

/// Fait tourner la boucle applicative (drainage pilot + pas de simulation) sur ce
/// thread pendant que `client` s'exécute sur un autre, puis renvoie son résultat.
/// Borne dure de 30 s : un pont cassé doit faire échouer le test, pas le geler.
#[cfg(feature = "net_tests")]
fn drive<T: Send + 'static>(
    app: &mut AppState,
    server: &PilotServer,
    client: impl FnOnce(std::net::SocketAddr) -> T + Send + 'static,
) -> T {
    let addr = server.local_addr;
    let handle = std::thread::spawn(move || client(addr));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !handle.is_finished() {
        assert!(
            Instant::now() < deadline,
            "le client n'a pas terminé en 30 s — pont bloqué ?"
        );
        server.poll(app, None);
        // Fait avancer le gameplay comme la boucle réelle (`advance_play` détecte
        // les fronts Play/Stop) — cf. le pattern de `tests/play_mode_audit.rs`.
        app.advance_play();
        std::thread::sleep(Duration::from_millis(1));
    }
    handle.join().expect("thread client")
}

/// Envoie une ligne JSON, lit la ligne de réponse, la parse — le protocole exact
/// du pont, sans passer par le binaire client.
#[cfg(feature = "net_tests")]
fn ask(
    reader: &mut BufReader<TcpStream>,
    writer: &mut TcpStream,
    request: serde_json::Value,
) -> serde_json::Value {
    writeln!(writer, "{request}").expect("envoi de la requête");
    let mut line = String::new();
    reader.read_line(&mut line).expect("lecture de la réponse");
    serde_json::from_str(&line).expect("réponse JSON valide")
}

#[cfg(feature = "net_tests")]
fn connect(addr: std::net::SocketAddr) -> (BufReader<TcpStream>, TcpStream) {
    let stream = TcpStream::connect(addr).expect("connexion au pont");
    let reader = BufReader::new(stream.try_clone().expect("clone du flux"));
    (reader, stream)
}

#[cfg(feature = "net_tests")]
#[test]
fn pilot_bridge_drives_lua_play_scene_and_inputs_over_tcp() {
    let mut app = AppState::new();
    // Port 0 : l'OS choisit un port libre — pas de collision entre tests ni avec
    // une vraie instance `--pilot` qui tournerait sur la machine.
    let server = PilotServer::start(0, None).expect("démarrage du pont");

    let responses = drive(&mut app, &server, |addr| {
        let (mut reader, mut writer) = connect(addr);
        let lua = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "lua", "src": "return 1 + 1"}),
        );
        let play = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "console", "arg": "play"}),
        );
        let state = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "state"}),
        );
        let scene = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "scene"}),
        );
        let input = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "input", "thrust": 1.0, "jump": true}),
        );
        let stop = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "console", "arg": "stop"}),
        );
        (lua, play, state, scene, input, stop)
    });

    let (lua, play, state, scene, input, stop) = responses;
    assert_eq!(lua["ok"], true);
    assert_eq!(lua["result"], "2");
    assert_eq!(play["result"], "Play démarré");
    assert_eq!(state["ok"], true);
    assert_eq!(state["result"]["playing"], true, "state : {state}");
    assert!(
        state["result"]["objects"].as_u64().unwrap() > 0,
        "la scène de démo n'est pas vide"
    );
    // Le dump de scène expose nom/position/visibilité de chaque objet.
    let objects = scene["result"].as_array().expect("liste d'objets");
    assert!(!objects.is_empty());
    assert!(objects[0]["name"].is_string() && objects[0]["pos"].is_array());
    assert_eq!(input["ok"], true);
    assert_eq!(stop["result"], "arrêté");

    // Les entrées injectées ont bien atterri dans l'état joueur réel.
    assert_eq!(app.input_state.key_thrust, 1.0);
    assert!(app.input_state.jump);
    // Et le Play a été arrêté (restauration comprise, gérée par `advance_play`).
    assert!(!app.playing);
}

/// Audit C1 (gestion d'erreur Lua) : une erreur de script doit revenir au client
/// en clair (`ok: false` + message), la connexion doit survivre, et une requête
/// malformée ne doit ni tuer le pont ni l'application.
#[cfg(feature = "net_tests")]
#[test]
fn pilot_bridge_reports_lua_errors_and_survives_malformed_requests() {
    let mut app = AppState::new();
    let server = PilotServer::start(0, None).expect("démarrage du pont");

    let (bad_lua, malformed, still_alive) = drive(&mut app, &server, |addr| {
        let (mut reader, mut writer) = connect(addr);
        let bad_lua = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "lua", "src": "error('boom C1')"}),
        );
        // Ligne non-JSON : le serveur doit répondre une erreur, pas fermer.
        writeln!(writer, "n'importe quoi").expect("envoi");
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("réponse à la ligne malformée");
        let malformed: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        let still_alive = ask(
            &mut reader,
            &mut writer,
            serde_json::json!({"cmd": "lua", "src": "return 'ok'"}),
        );
        (bad_lua, malformed, still_alive)
    });

    assert_eq!(bad_lua["ok"], false);
    assert!(
        bad_lua["error"].as_str().unwrap().contains("boom C1"),
        "l'erreur Lua doit remonter en clair : {bad_lua}"
    );
    assert_eq!(malformed["ok"], false);
    assert_eq!(still_alive["result"], "ok");
}

/// Pilot v2 — session complète « tout faire » : créer un cube, l'éditer, le
/// relire, cadrer la caméra, lancer le Play, avancer 30 pas déterministes,
/// interroger le joueur, blesser un monstre, supprimer, annuler — le tout par
/// de vraies requêtes TCP, sans GPU.
#[cfg(feature = "net_tests")]
#[test]
fn pilot_bridge_full_editing_and_gameplay_session() {
    let mut app = AppState::new();
    let server = PilotServer::start(0, None).expect("démarrage du pont");
    let baseline = app.scene.objects.len();

    let r = drive(&mut app, &server, move |addr| {
        let (mut reader, mut writer) = connect(addr);
        let mut ask = |req: serde_json::Value| ask(&mut reader, &mut writer, req);

        let added = ask(serde_json::json!({"cmd": "object", "op": "add", "kind": "cube"}));
        let index = added["result"]["index"].as_u64().expect("index du cube");
        let set = ask(serde_json::json!({
            "cmd": "object", "op": "set", "index": index,
            "patch": {"name": "Cube pilote", "pos": [1.0, 2.0, 3.0], "color": [1.0, 0.0, 0.0]}
        }));
        let get = ask(serde_json::json!({"cmd": "object", "op": "get", "index": index}));
        let camera = ask(serde_json::json!({
            "cmd": "camera", "target": [1.0, 2.0, 3.0], "yaw": 45.0, "distance": 8.0,
            "follow": false
        }));
        let play = ask(serde_json::json!({"cmd": "console", "arg": "play"}));
        let pause = ask(serde_json::json!({"cmd": "console", "arg": "pause"}));
        let steps = ask(serde_json::json!({"cmd": "console", "arg": "step 30"}));
        let player = ask(serde_json::json!({"cmd": "player"}));
        let weapon = ask(serde_json::json!({"cmd": "console", "arg": "weapon 2"}));
        let stop = ask(serde_json::json!({"cmd": "console", "arg": "stop"}));
        let deleted = ask(serde_json::json!({"cmd": "object", "op": "delete", "index": index}));
        let undone = ask(serde_json::json!({"cmd": "console", "arg": "undo"}));
        let bad_field = ask(serde_json::json!({
            "cmd": "object", "op": "set", "index": 0, "patch": {"licorne": 1}
        }));
        (
            added, set, get, camera, play, pause, steps, player, weapon, stop, deleted, undone,
            bad_field,
        )
    });
    let (
        added,
        set,
        get,
        camera,
        play,
        pause,
        steps,
        player,
        weapon,
        stop,
        deleted,
        undone,
        bad_field,
    ) = r;

    assert_eq!(added["ok"], true, "add : {added}");
    assert_eq!(set["ok"], true, "set : {set}");
    assert_eq!(get["result"]["name"], "Cube pilote", "get : {get}");
    assert_eq!(
        get["result"]["transform"]["position"],
        serde_json::json!([1.0, 2.0, 3.0]),
        "get position : {get}"
    );
    assert_eq!(camera["result"]["yaw_deg"].as_f64().unwrap().round(), 45.0);
    assert_eq!(camera["result"]["follow"], false);
    assert_eq!(play["result"], "Play démarré");
    assert_eq!(pause["result"], "en pause");
    assert_eq!(steps["result"], "30 pas de 1/60 s exécutés");
    assert_eq!(player["ok"], true, "player : {player}");
    assert_eq!(weapon["ok"], true, "weapon : {weapon}");
    assert_eq!(stop["result"], "arrêté");
    assert_eq!(deleted["ok"], true, "delete : {deleted}");
    assert_eq!(undone["result"], "annulé");
    assert_eq!(bad_field["ok"], false, "champ inconnu doit être refusé");

    // Après stop (restauration) : le cube créé AVANT Play doit exister dans le
    // snapshot restauré, puis delete + undo le ramènent — bilan : baseline + 1.
    assert_eq!(app.scene.objects.len(), baseline + 1);
    assert!(!app.scene.camera_follow, "follow off doit persister");
}

/// Pilot v2 — options et chargement de démos par le pont : les volumes ne
/// paniquent pas sans périphérique audio, les toggles UI headless répondent une
/// erreur explicite, et `demo zombies` remplace bien la scène.
#[cfg(feature = "net_tests")]
#[test]
fn pilot_bridge_options_and_demo_loading() {
    let mut app = AppState::new();
    let server = PilotServer::start(0, None).expect("démarrage du pont");

    let (options, toggle_headless, demo, state) = drive(&mut app, &server, move |addr| {
        let (mut reader, mut writer) = connect(addr);
        let mut ask = |req: serde_json::Value| ask(&mut reader, &mut writer, req);
        let options = ask(serde_json::json!({
            "cmd": "options", "music": 0.3, "sfx": 0.7, "timescale": 2.0
        }));
        let toggle_headless = ask(serde_json::json!({"cmd": "options", "hud": true}));
        let demo = ask(serde_json::json!({"cmd": "console", "arg": "demo zombies"}));
        let state = ask(serde_json::json!({"cmd": "state"}));
        (options, toggle_headless, demo, state)
    });

    assert_eq!(options["ok"], true, "options : {options}");
    assert!((app.time_scale - 2.0).abs() < 1e-6);
    assert_eq!(
        toggle_headless["ok"], false,
        "toggle UI headless : erreur attendue"
    );
    assert!(
        toggle_headless["error"]
            .as_str()
            .unwrap()
            .contains("headless")
    );
    assert_eq!(demo["result"], "démo zombies chargée");
    assert!(
        state["result"]["objects"].as_u64().unwrap() > 0,
        "la démo zombies doit peupler la scène : {state}"
    );
}

/// Sécurité (Sprint 1, audit du 19 juillet 2026) : le pont ne doit écouter que
/// sur la boucle locale — jamais exposé au réseau, quel que soit le port
/// demandé. On lit l'adresse réellement liée, pas celle de la doc.
#[cfg(feature = "net_tests")]
#[test]
fn pilot_bridge_listens_only_on_localhost() {
    let server = PilotServer::start(0, None).expect("démarrage du pont");
    assert_eq!(
        server.local_addr.ip(),
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        "le pont doit être lié à 127.0.0.1 exclusivement"
    );
}

/// `advance_steps` (pas-à-pas simulé du pont) doit produire le même gameplay
/// que la boucle temps réel : entrée tenue → le joueur avance réellement.
#[test]
fn advance_steps_moves_the_player_like_realtime_frames() {
    let mut app = AppState::new();
    app.run_console_command("demo controleur");
    app.playing = true;
    app.advance_play(); // front d'entrée en Play : snapshot + physique construite
    let before = app.player_position().expect("joueur présent");
    app.input_state.key_thrust = 1.0;
    assert!(app.advance_steps(60));
    let after = app.player_position().expect("joueur présent");
    assert!(
        (after - before).length() > 0.3,
        "60 pas de poussée doivent déplacer le joueur : avant={before:?} après={after:?}"
    );
}

#[test]
fn advance_steps_moves_the_hamlet_player_even_without_a_prior_render_frame() {
    // Régression de l'audit du 19 juillet 2026 : fenêtre masquée (App Nap), le
    // front d'entrée en Play — porté par la boucle de rendu — peut ne pas avoir
    // tourné quand le pont demande des pas ; `advance_steps` doit le déclencher
    // lui-même, sinon la physique n'existe pas et le joueur reste figé.
    let mut app = AppState::new();
    app.run_console_command("demo hameau");
    app.playing = true;
    // PAS d'`advance_play()` ici : c'est précisément le cas audité.
    let before = app.player_position().expect("joueur présent");
    app.input_state.key_thrust = 1.0;
    assert!(app.advance_steps(60));
    let after = app.player_position().expect("joueur présent");
    assert!(
        (after - before).length() > 0.3,
        "le joueur du hameau doit bouger"
    );
}
