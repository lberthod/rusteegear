//! Vérification mécanique de la Phase L (playtest réel) : rejoint le vrai
//! serveur `bin/server.rs` sur chacun des modes réseau (Vagues/Survie/Escorte/
//! Boss) via `connect_to_lobby` (Phase I, `sprintreflecion.md`) et observe les
//! événements réellement diffusés, pour confirmer/infirmer mécaniquement que
//! chaque mode se termine correctement — pas un jugement de ressenti (ça reste
//! à un vrai playtest humain), juste la mécanique de bout en bout.
//!
//! Usage : `RUSTEEGEAR_SERVER_ADDR=127.0.0.1:17790 cargo run --bin server &`
//! puis `cargo run --example phase_l_mode_check -- ws://127.0.0.1:17790`

use std::time::{Duration, Instant};

use motor3derust::app::multiplayer::RoundObjective;
use motor3derust::net::client::NetClient;
use motor3derust::net::protocol::{GameEvent, ServerMsg};

fn check_mode(url: &str, objective: RoundObjective, lobby: &str, watch_secs: u64) {
    println!("\n=== Mode {objective:?} (salon « {lobby} ») ===");
    let client = NetClient::connect_to_lobby(url, "PhaseLBot", None, lobby, 0, objective.to_u8())
        .unwrap_or_else(|e| panic!("connexion échouée pour {objective:?} : {e}"));

    let deadline = Instant::now() + Duration::from_secs(watch_secs);
    let mut got_objective_echo = None;
    let mut got_win = false;
    let mut got_lose = false;
    let mut snapshots = 0u32;

    while Instant::now() < deadline {
        match client.inbox.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Event(GameEvent::RoundObjective { objective: o })) => {
                got_objective_echo = Some(RoundObjective::from_u8(o));
            }
            Ok(ServerMsg::Event(GameEvent::Win { .. })) => got_win = true,
            Ok(ServerMsg::Event(GameEvent::Lose { .. })) => got_lose = true,
            Ok(ServerMsg::Snapshot(_)) => snapshots += 1,
            _ => {}
        }
    }

    println!("  objectif échoué par le serveur : {got_objective_echo:?} (attendu {objective:?})");
    println!("  snapshots reçus : {snapshots}");
    println!("  GameEvent::Win reçu : {got_win}");
    println!("  GameEvent::Lose reçu : {got_lose}");
    if !got_win && !got_lose {
        println!(
            "  ⚠️  Aucune fin de manche observée en {watch_secs}s — mode probablement bloqué sur cette scène."
        );
    }
}

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:17790".to_string());

    // Escorte : la scène réseau réelle (mmorpg_demo, embarquée) n'a AUCUN
    // objet `convoy` (contrairement à `Scene::escorte_demo()`, réservée au
    // solo) — `update_escorte`/`is_convoy_destroyed` retournent tôt sans rien
    // faire quand `scene.objects.iter().find(|o| o.convoy.is_some())` est
    // `None` (src/app/combat.rs:182, src/app/health.rs:472-478). Fenêtre
    // courte : la question n'est pas "est-ce lent", c'est "est-ce que ça
    // bouge du tout".
    check_mode(&url, RoundObjective::Escorte, "phasel-escorte", 20);

    // Boss : retombe sur `update_waves` (documenté dans sprint10audit.md,
    // Sprint 8) — sur la scène réseau réelle (26 créatures, plusieurs
    // vagues), pas le layout 1-vague de `Scene::boss_demo()`. Fenêtre courte :
    // on vérifie juste que le mode est accepté et que la partie tourne
    // normalement (pas de fin attendue ici, un vrai clear prendrait bien plus
    // que quelques secondes avec un bot simple).
    check_mode(&url, RoundObjective::Boss, "phasel-boss", 15);

    // Vagues : témoin, doit juste tourner (pas de fin attendue en 10s).
    check_mode(&url, RoundObjective::Vagues, "phasel-vagues", 10);

    println!(
        "\nSurvie (chrono 180s) non testée ici en réseau réel — déjà couverte par un test \
         unitaire dédié en temps simulé (survie_mode_loops_the_wave_then_wins_once_the_timer_elapses)."
    );
}
