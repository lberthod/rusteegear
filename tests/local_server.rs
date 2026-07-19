//! Sprint 7 (audit du 19 juillet 2026) : preuve du cycle complet « démarrer
//! le vrai binaire serveur → un vrai client s'y connecte → l'arrêter, sans
//! processus orphelin » — ce qu'`editor::Editor::start_local_server`/
//! `stop_local_server` font depuis l'éditeur (`std::process::Command` sur
//! `src/bin/server.rs`). `Editor` a besoin d'un contexte egui/wgpu réel et ne
//! se construit pas en headless ; ce test descend donc d'un cran et exerce le
//! même mécanisme (binaire réel en processus enfant, vrai client réseau)
//! directement, sans passer par l'UI.
//!
//! Gaté `net_tests` : ouvre de vrais sockets TCP, comme les autres tests
//! réseau du dépôt (`src/bin/server.rs`, `tests/pilot_bridge.rs`).

#![cfg(feature = "net_tests")]

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use motor3derust::net::client::NetClient;

/// Localise le binaire `server` compilé à côté de ce test — pas
/// `current_exe()` directement (qui pointe vers `target/<profil>/deps/`, où
/// les binaires portent un suffixe de hash), mais son dossier parent
/// `target/<profil>/`, où `cargo build`/`cargo test` posent aussi les
/// binaires sous leur nom exact (même mécanisme que
/// `editor::sibling_binary_path`, utilisé lui depuis l'éditeur déjà compilé).
fn server_binary_path() -> PathBuf {
    let test_exe = std::env::current_exe().expect("chemin de ce test");
    let deps_dir = test_exe.parent().expect("dossier deps/");
    let profile_dir = deps_dir
        .parent()
        .expect("dossier de profil (debug/release)");
    let name = if cfg!(windows) {
        "server.exe"
    } else {
        "server"
    };
    let path = profile_dir.join(name);
    assert!(
        path.exists(),
        "{} introuvable — `cargo test` doit aussi compiler le binaire `server` \
         (aucune section [[bin]] dans Cargo.toml : découverte automatique depuis src/bin/)",
        path.display()
    );
    path
}

fn wait_for_port(addr: &str, timeout: Duration) -> bool {
    let socket_addr = addr.parse().expect("adresse valide");
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect_timeout(&socket_addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn spawn_server(addr: &str) -> Child {
    Command::new(server_binary_path())
        .env("RUSTEEGEAR_SERVER_ADDR", addr)
        .spawn()
        .expect("lancement du serveur local")
}

#[test]
fn a_real_client_can_join_a_freshly_spawned_local_server() {
    // Port 0 impossible ici : `RUSTEEGEAR_SERVER_ADDR` doit être un port fixe
    // connu à l'avance pour s'y connecter depuis ce process — port dédié à ce
    // test pour ne pas entrer en collision avec un autre test réseau parallèle.
    let addr = "127.0.0.1:17801";
    let mut child = spawn_server(addr);

    assert!(
        wait_for_port(addr, Duration::from_secs(5)),
        "le serveur local doit accepter des connexions TCP sous 5 s"
    );

    let url = format!("ws://{addr}");
    let client = NetClient::connect_to_lobby(&url, "Testeur", None, "salon-sprint7", 0, 0)
        .expect("un vrai client doit pouvoir rejoindre le serveur qui vient de démarrer");
    drop(client);

    child.kill().expect("arrêt du serveur");
    child.wait().expect("réclamation du process serveur");
}

#[test]
fn stopping_the_server_leaves_no_process_listening_on_its_port() {
    let addr = "127.0.0.1:17802";
    let mut child = spawn_server(addr);
    assert!(wait_for_port(addr, Duration::from_secs(5)));

    child.kill().expect("arrêt du serveur");
    child
        .wait()
        .expect("réclamation du process serveur — pas de zombie");

    // Après arrêt, un nouveau serveur doit pouvoir reprendre exactement la
    // même adresse : si l'ancien process avait survécu (orphelin), le bind
    // échouerait avec « address already in use ».
    let mut second = spawn_server(addr);
    assert!(
        wait_for_port(addr, Duration::from_secs(5)),
        "un second serveur doit pouvoir reprendre le même port — \
         le premier ne doit laisser aucun processus orphelin dessus"
    );
    second.kill().expect("arrêt du second serveur");
    second
        .wait()
        .expect("réclamation du second process serveur");
}
