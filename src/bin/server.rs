//! Serveur de jeu headless (Sprints 51-53, SPRINT_MMORPG.md) : fait tourner une
//! manche en réutilisant `scene`/`runtime`/`app::combat` **sans fenêtre ni GPU**
//! (aucune dépendance à `gfx`/`egui`/`winit` dans ce binaire), et accepte des
//! connexions WebSocket (`net::server_loop`).
//!
//! Intégration minimale pour l'instant : les messages des clients connectés sont
//! juste logués, et un `Snapshot` de la position du « joueur » local est diffusé
//! à chaque tick — de quoi valider `NetServer` dans la vraie boucle de jeu, pas
//! encore de quoi laisser un client distant *piloter* ce joueur (branchement
//! réel de l'input réseau dans `AppState` : Sprint 55, avec les salons).

use std::time::{Duration, Instant};

use motor3derust::app::AppState;
use motor3derust::net::protocol::{EntityDelta, ServerMsg, Snapshot};
use motor3derust::net::server_loop::NetServer;

/// Cadence réseau visée pour le serveur (cf. SPRINT_MMORPG.md Sprint 51 : découplée
/// du 60 Hz physique local, qui reste piloté par l'accumulateur à pas fixe existant
/// dans `AppState::advance_play`).
const SERVER_TICK: Duration = Duration::from_millis(50); // 20 Hz

/// Durée maximale d'une manche avant arrêt de sécurité (évite une boucle infinie si
/// la manche ne se termine jamais, ex. bug de configuration de scène).
const MAX_DURATION: Duration = Duration::from_secs(180);

/// Adresse d'écoute par défaut ; `RUSTEEGEAR_SERVER_ADDR` pour surcharger (ex. tests
/// manuels avec plusieurs instances sur la même machine).
const DEFAULT_ADDR: &str = "127.0.0.1:7777";

fn main() {
    env_logger::init();
    log::info!("RusteeGear — serveur headless (Sprint 51) : démarrage d'une manche");

    let addr = std::env::var("RUSTEEGEAR_SERVER_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let net = match NetServer::start(&addr) {
        Ok(n) => {
            log::info!("Serveur réseau à l'écoute sur {}", n.local_addr);
            Some(n)
        }
        Err(e) => {
            log::warn!(
                "Réseau désactivé (échec du bind sur {addr} : {e}) — manche locale uniquement"
            );
            None
        }
    };

    let mut app = AppState::new();
    app.load_zombies_demo();
    app.playing = true;

    let mut last_wave = app.wave;
    let mut last_score = app.score();
    let started = Instant::now();
    let mut tick: u32 = 0;

    loop {
        let tick_start = Instant::now();
        app.advance_play();
        tick += 1;

        if let Some(net) = &net {
            while let Ok((id, msg)) = net.inbox.try_recv() {
                log::info!("Message du joueur {id} : {msg:?}");
            }
            if let Some(i) = app
                .scene
                .objects
                .iter()
                .position(|o| o.controller.is_some())
            {
                let o = &app.scene.objects[i];
                let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
                net.broadcast(&ServerMsg::Snapshot(Snapshot {
                    tick,
                    entities: vec![EntityDelta {
                        index: i as u32,
                        position: o.transform.position.to_array(),
                        yaw,
                        visible: o.visible,
                        health: app.hud_health,
                    }],
                }));
            }
        }

        if app.wave != last_wave {
            log::info!("Manche {} révélée", app.wave);
            last_wave = app.wave;
        }
        if app.score() != last_score {
            log::info!("Score : {}", app.score());
            last_score = app.score();
        }

        if app.has_won() {
            log::info!(
                "Manche terminée : victoire, score final {} (en {:.1} s)",
                app.score(),
                started.elapsed().as_secs_f32()
            );
            break;
        }
        if app.is_lost() {
            log::info!(
                "Manche terminée : défaite, score final {} (en {:.1} s)",
                app.score(),
                started.elapsed().as_secs_f32()
            );
            break;
        }
        if started.elapsed() > MAX_DURATION {
            log::warn!("Arrêt de sécurité : durée maximale de manche atteinte sans issue");
            break;
        }

        let elapsed = tick_start.elapsed();
        if elapsed < SERVER_TICK {
            std::thread::sleep(SERVER_TICK - elapsed);
        }
    }
}
