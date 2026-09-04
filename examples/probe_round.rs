//! Sonde de manche (roadmap post-audit UX v2 2026-09-04, 0.1 bis) : un client
//! headless — sans winit, donc sans dépendance à la boucle de rendu ni à
//! l'occultation de fenêtre — reste connecté `SECONDES` à un serveur, envoie
//! une entrée idle à cadence fixe, et rend compte de ce qu'un arrivant qui ne
//! touche à rien vit réellement : vie diffusée par le serveur, âge du dernier
//! snapshot, événements de manche (`RoundStart`, `WaveStart`, `Lose`, `Win`,
//! `PlayerDown`).
//!
//! Verdict (code de sortie 1 si l'un des deux est faux) :
//! - aucun trou de diffusion supérieur à `NET_SILENCE_TIMEOUT` (8 s) — sinon le
//!   client réel se croirait déconnecté ;
//! - aucune défaite (`Lose`) décidée alors que ce joueur était encore vivant.
//!
//! Usage : `cargo run --release --example probe_round -- ws://127.0.0.1:7777 Sonde 60`
//! (défauts : serveur public, « Sonde », 60 s).

use std::time::{Duration, Instant};

use motor3derust::net::client::NetClient;
use motor3derust::net::protocol::{ClientMsg, GameEvent, ServerMsg};

const SILENCE: Duration = Duration::from_secs(8);

fn main() {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .unwrap_or_else(|| "wss://ws.loicberthod.ch".to_string());
    let name = args.next().unwrap_or_else(|| "Sonde".to_string());
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);

    let client = NetClient::connect(&url, &name, None).expect("connexion au serveur");
    client
        .wait_ready(Duration::from_secs(8))
        .expect("poignée de main avec le serveur");
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut me = None;
    let mut health = None;
    let mut last_snapshot: Option<Instant> = None;
    let mut worst_gap = Duration::ZERO;
    let mut lose_while_alive = false;
    let mut next_report = start + Duration::from_secs(5);
    let mut next_input = start;

    while Instant::now() < deadline {
        let now = Instant::now();
        if now >= next_input {
            client.send(&ClientMsg::Input {
                move_x: 0.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            });
            next_input = now + Duration::from_millis(100);
        }
        match client.inbox.recv_timeout(Duration::from_millis(50)) {
            Ok(ServerMsg::Welcome { player_id }) => {
                me = Some(player_id);
                println!("[{:>5.1}s] Welcome : joueur {player_id}", elapsed(start));
            }
            Ok(ServerMsg::Snapshot(s)) => {
                let now = Instant::now();
                if let Some(prev) = last_snapshot {
                    worst_gap = worst_gap.max(now - prev);
                }
                last_snapshot = Some(now);
                if let Some(id) = me
                    && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(id))
                {
                    health = e.health;
                }
            }
            Ok(ServerMsg::Event(ev)) => {
                let alive = health.is_none_or(|h| h > 0.0);
                match &ev {
                    GameEvent::Lose { .. } => {
                        println!(
                            "[{:>5.1}s] Lose reçu (vie {health:?}, vivant : {alive})",
                            elapsed(start)
                        );
                        if alive {
                            lose_while_alive = true;
                        }
                    }
                    GameEvent::Win { .. }
                    | GameEvent::RoundStart
                    | GameEvent::WaveStart { .. }
                    | GameEvent::PlayerDown { .. } => {
                        println!("[{:>5.1}s] {ev:?}", elapsed(start));
                    }
                    _ => {}
                }
            }
            Ok(ServerMsg::JoinRejected { reason }) => {
                println!("[{:>5.1}s] Refusé : {reason}", elapsed(start));
                std::process::exit(1);
            }
            Ok(_) => {}
            Err(_) => {}
        }
        if Instant::now() >= next_report {
            let age = last_snapshot.map(|t| t.elapsed().as_millis());
            println!(
                "[{:>5.1}s] vie {health:?} · dernier snapshot il y a {age:?} ms · pire trou {} ms",
                elapsed(start),
                worst_gap.as_millis()
            );
            next_report += Duration::from_secs(5);
        }
    }
    client.send(&ClientMsg::Leave);
    let silent = worst_gap > SILENCE;
    println!(
        "Bilan : pire trou de diffusion {} ms ({}), défaite reçue vivant : {}",
        worst_gap.as_millis(),
        if silent {
            "au-delà du silence toléré"
        } else {
            "ok"
        },
        lose_while_alive
    );
    if silent || lose_while_alive {
        std::process::exit(1);
    }
    println!("✅ Manche jouable : diffusion continue, aucune défaite injustifiée");
}

fn elapsed(start: Instant) -> f32 {
    start.elapsed().as_secs_f32()
}
