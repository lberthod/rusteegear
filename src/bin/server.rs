//! Serveur de jeu headless (Sprint 51, SPRINT_MMORPG.md) : fait tourner une manche
//! en réutilisant `scene`/`runtime`/`app::combat` **sans fenêtre ni GPU** (aucune
//! dépendance à `gfx`/`egui`/`winit` dans ce binaire).
//!
//! Pour l'instant : simulation locale d'une manche « Call of Zombies » à tick fixe,
//! loggée en console. Les Sprints 52-53 y brancheront le protocole réseau et le
//! transport WebSocket (un ou plusieurs clients distants au lieu d'une boucle locale).

use std::time::{Duration, Instant};

use motor3derust::app::AppState;

/// Cadence réseau visée pour le serveur (cf. SPRINT_MMORPG.md Sprint 51 : découplée
/// du 60 Hz physique local, qui reste piloté par l'accumulateur à pas fixe existant
/// dans `AppState::advance_play`).
const SERVER_TICK: Duration = Duration::from_millis(50); // 20 Hz

/// Durée maximale d'une manche avant arrêt de sécurité (évite une boucle infinie si
/// la manche ne se termine jamais, ex. bug de configuration de scène).
const MAX_DURATION: Duration = Duration::from_secs(180);

fn main() {
    env_logger::init();
    log::info!("RusteeGear — serveur headless (Sprint 51) : démarrage d'une manche");

    let mut app = AppState::new();
    app.load_zombies_demo();
    app.playing = true;

    let mut last_wave = app.wave;
    let mut last_score = app.score();
    let started = Instant::now();

    loop {
        let tick_start = Instant::now();
        app.advance_play();

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
