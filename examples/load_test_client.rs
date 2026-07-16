//! Test de charge multijoueur (SPRINT_MMORPG.md, Sprint 61) : connecte `N_CLIENTS`
//! bots à un serveur de jeu démarré dans ce même processus, les fait bouger et
//! attaquer pendant `TEST_DURATION`, et publie des chiffres mesurés (pas
//! estimés) : taille moyenne d'un `Snapshot`, débit réseau par joueur, temps de
//! traitement serveur par tick.
//!
//! Usage : `cargo run --release --example load_test_client`
//! (`--release` : le `dev` non optimisé fausserait la mesure du temps de
//! traitement serveur, cf. la même remarque déjà faite ailleurs dans le projet
//! pour la physique/le rendu).

use std::time::{Duration, Instant};

use motor3derust::app::AppState;
use motor3derust::app::multiplayer::NetworkInput;
use motor3derust::net::client::NetClient;
use motor3derust::net::protocol::{self, ClientMsg, ServerMsg};
use motor3derust::net::server_loop::NetServer;

/// Haut de la fourchette visée par le scope du projet (SPRINT_MMORPG.md §0 :
/// salons de 2 à 16 joueurs) — le cas le plus exigeant à mesurer.
const N_CLIENTS: usize = 16;

/// Durée de la fenêtre de mesure, après une courte période de chauffe (les
/// premiers ticks incluent la connexion de tous les bots, pas représentatifs
/// d'un régime stable).
const TEST_DURATION: Duration = Duration::from_secs(10);
const WARMUP: Duration = Duration::from_secs(1);

/// Même cadence que `src/bin/server.rs` (20 Hz) : la mesure doit refléter la
/// configuration réellement déployée, pas une cadence différente choisie pour
/// arranger les chiffres.
const SERVER_TICK: Duration = Duration::from_millis(50);
const CLIENT_INPUT_TICK: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Counters {
    server_ticks: u64,
    server_tick_time_total: Duration,
    server_tick_time_max: Duration,
    snapshot_bytes_total: u64,
    snapshot_count: u64,
    input_bytes_total: u64,
    input_count: u64,
}

fn main() {
    env_logger::init();

    // Plafond par IP relevé : les 16 bots arrivent tous de 127.0.0.1, le
    // plafond de production (4, anti-DoS) refuserait tout le monde dès le 5ᵉ —
    // cf. `NetServer::start_with_ip_cap`. +1 de marge pour un client de
    // diagnostic ouvert à côté pendant la mesure.
    let net = NetServer::start_with_ip_cap("127.0.0.1:0", N_CLIENTS + 1)
        .expect("démarrage du serveur de test");
    let addr = net.local_addr;
    println!(
        "[load_test] serveur démarré sur {addr}, {N_CLIENTS} bots, {TEST_DURATION:?} de mesure (+{WARMUP:?} de chauffe)"
    );

    let mut app = AppState::new();
    app.load_zombies_demo();
    app.playing = true;

    let mut clients = Vec::with_capacity(N_CLIENTS);
    for i in 0..N_CLIENTS {
        let url = format!("ws://{addr}");
        let client = NetClient::connect(&url, &format!("Bot{i}"), None)
            .unwrap_or_else(|e| panic!("connexion du bot {i} échouée : {e}"));
        clients.push(client);
    }

    let mut counters = Counters::default();
    let started = Instant::now();
    let measure_from = started + WARMUP;
    let stop_at = measure_from + TEST_DURATION;
    let mut tick: u32 = 0;
    let mut next_input_at = Instant::now();

    loop {
        let now = Instant::now();
        if now >= stop_at {
            break;
        }
        let tick_start = Instant::now();

        // Messages des bots vers l'"AppState" du serveur de test — même logique
        // que la boucle de `src/bin/server.rs` (Join → spawn, Input → set).
        while let Ok((id, msg)) = net.inbox.try_recv() {
            match msg {
                ClientMsg::Join { .. } => {
                    app.spawn_network_player(id);
                }
                ClientMsg::Input {
                    move_x,
                    move_y,
                    aim_yaw,
                    attack,
                    jump,
                    fire,
                    weapon,
                    heal,
                } => {
                    app.set_network_input(
                        id,
                        NetworkInput {
                            move_x,
                            move_y,
                            aim_yaw,
                            attack,
                            jump,
                            fire,
                            weapon,
                            heal,
                        },
                    );
                }
                ClientMsg::Leave => {}
            }
        }

        app.advance_play();
        let tick_time = tick_start.elapsed();
        tick += 1;

        let snapshot = ServerMsg::Snapshot(app.network_snapshot(tick));
        if let Ok(bytes) = protocol::encode(&snapshot)
            && now >= measure_from
        {
            counters.snapshot_bytes_total += bytes.len() as u64;
            counters.snapshot_count += 1;
        }
        net.broadcast_all_rooms(&snapshot);

        // Chaque bot envoie son `Input` à sa propre cadence (indépendante du
        // tick serveur, comme un vrai client) : mouvement pseudo-aléatoire
        // simple (sinusoïde déphasée par bot), pas de dépendance à une crate
        // `rand` pour un test qui n'a pas besoin d'aléatoire cryptographique.
        if now >= next_input_at {
            next_input_at = now + CLIENT_INPUT_TICK;
            let t = now.duration_since(started).as_secs_f32();
            for (i, client) in clients.iter().enumerate() {
                let phase = i as f32 * 0.7;
                let input = ClientMsg::Input {
                    move_x: (t * 1.3 + phase).sin(),
                    move_y: (t * 0.9 + phase).cos(),
                    aim_yaw: 0.0,
                    attack: (t + phase) % 1.0 < 0.1,
                    jump: false,
                    fire: false,
                    weapon: 0,
                    heal: false,
                };
                if let Ok(bytes) = protocol::encode(&input)
                    && now >= measure_from
                {
                    counters.input_bytes_total += bytes.len() as u64;
                    counters.input_count += 1;
                }
                client.send(&input);
            }
        }

        // Draine les messages reçus par chaque bot (Snapshot/Event) : purge les
        // canaux (sans ça, `Receiver` non bornés grossiraient pendant tout le
        // test) — la taille est déjà comptée côté serveur au moment du
        // broadcast, pas la peine de la recompter ici.
        for client in &clients {
            while client.inbox.try_recv().is_ok() {}
        }

        if now >= measure_from {
            counters.server_ticks += 1;
            counters.server_tick_time_total += tick_time;
            counters.server_tick_time_max = counters.server_tick_time_max.max(tick_time);
        }

        let elapsed = tick_start.elapsed();
        if elapsed < SERVER_TICK {
            std::thread::sleep(SERVER_TICK - elapsed);
        }
    }

    report(&counters);
}

fn report(c: &Counters) {
    let measured_secs = TEST_DURATION.as_secs_f64();
    let avg_snapshot_bytes = c.snapshot_bytes_total as f64 / c.snapshot_count.max(1) as f64;
    let avg_input_bytes = c.input_bytes_total as f64 / c.input_count.max(1) as f64;
    let downstream_kb_per_s_per_player = c.snapshot_bytes_total as f64 / 1024.0 / measured_secs;
    let upstream_kb_per_s_total = c.input_bytes_total as f64 / 1024.0 / measured_secs;
    let upstream_kb_per_s_per_player = upstream_kb_per_s_total / N_CLIENTS as f64;
    let avg_tick_ms =
        c.server_tick_time_total.as_secs_f64() * 1000.0 / c.server_ticks.max(1) as f64;

    println!("\n--- Résultats (SPRINT_MMORPG.md, Sprint 61) ---");
    println!("Joueurs simultanés     : {N_CLIENTS}");
    println!("Ticks serveur mesurés  : {}", c.server_ticks);
    println!(
        "Temps de traitement/tick : moyenne {avg_tick_ms:.3} ms, max {:.3} ms (budget {:.1} ms à 20 Hz)",
        c.server_tick_time_max.as_secs_f64() * 1000.0,
        SERVER_TICK.as_secs_f64() * 1000.0
    );
    println!("Taille moyenne d'un Snapshot ({N_CLIENTS} joueurs) : {avg_snapshot_bytes:.0} octets");
    println!(
        "Débit descendant (serveur -> 1 client) : {downstream_kb_per_s_per_player:.2} Ko/s/joueur"
    );
    println!(
        "Débit montant total (tous les Input, {N_CLIENTS} joueurs) : {upstream_kb_per_s_total:.2} Ko/s \
         ({upstream_kb_per_s_per_player:.2} Ko/s/joueur)"
    );
    println!("Taille moyenne d'un Input : {avg_input_bytes:.0} octets");
    println!("-----------------------------------------------\n");
}
