//! Serveur de jeu headless (Sprints 51-55, SPRINT_MMORPG.md) : fait tourner une
//! manche en réutilisant `scene`/`runtime`/`app::combat`/`app::multiplayer`
//! **sans fenêtre ni GPU** (aucune dépendance à `gfx`/`egui`/`winit` dans ce
//! binaire), et accepte des connexions WebSocket (`net::server_loop`).
//!
//! Salon unique pour l'instant (pas de multi-salons, cf. `SPRINT_MMORPG.md`) :
//! chaque client qui rejoint obtient son propre objet pilotable
//! (`AppState::spawn_network_player`) dans la même manche « Call of Zombies ».
//!
//! **Limite connue, assumée** : les conditions de victoire/défaite et la vie du
//! HUD (`AppState::has_won`/`is_lost`/`hud_health`) restent celles de l'objet
//! « joueur » gabarit d'origine (cf. `player_index`), pas individualisées par
//! joueur réseau — un vrai combat joueur-contre-joueur demande d'abord de donner
//! à chaque joueur sa propre vie/win condition, hors scope de ce sprint (cf.
//! `AppState::network_snapshot`, qui documente la même limite côté santé).
//!
//! **Progression Firebase (Sprint 57)** : optionnelle, activée par 4 variables
//! d'environnement (`FIREBASE_API_KEY`, `FIREBASE_DATABASE_URL`,
//! `FIREBASE_SERVER_EMAIL`, `FIREBASE_SERVER_PASSWORD` — un compte Firebase
//! dédié au serveur, cf. le commentaire « Qui écrit la progression ? » dans
//! `net::firebase`). Si absentes, le serveur tourne comme avant (pas de
//! régression). En fin de manche, chaque joueur réseau connecté avec un
//! `firebase_uid` (cf. `ClientMsg::Join`) reçoit son score de la manche en XP.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use motor3derust::app::AppState;
use motor3derust::app::multiplayer::NetworkInput;
use motor3derust::net::firebase::{
    self, AuthSession, FirebaseConfig, LeaderboardEntry, PlayerProgress,
};
use motor3derust::net::protocol::{ClientMsg, ServerMsg};
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

/// XP nécessaire pour passer au niveau suivant (formule volontairement simple :
/// un palier fixe, pas de courbe — à raffiner si besoin une fois testé en
/// conditions réelles).
const XP_PER_LEVEL: u32 = 1000;

/// État du salon côté binaire (pas dans `AppState`, qui ne connaît que les
/// indices d'objets, cf. `app::multiplayer`) : nom affiché et `uid` Firebase
/// de chaque joueur réseau connecté.
#[derive(Default)]
struct Lobby {
    names: HashMap<u32, String>,
    firebase_uids: HashMap<u32, String>,
}

/// Traite un message reçu d'un client : fait entrer/sortir le joueur de la
/// partie ou met à jour son `Input` courant. Extrait de `main` pour rester
/// testable (cf. `tests::joining_moving_and_leaving_through_the_real_socket`)
/// sans avoir à lancer le binaire complet.
fn handle_message(app: &mut AppState, net: &NetServer, lobby: &mut Lobby, id: u32, msg: ClientMsg) {
    match msg {
        ClientMsg::Join { name, firebase_uid } => {
            if app.spawn_network_player(id).is_some() {
                log::info!("Joueur {id} ({name}) entre en jeu");
                net.broadcast(&ServerMsg::PlayerJoined {
                    player_id: id,
                    name: name.clone(),
                });
            } else {
                log::warn!("Joueur {id} ({name}) : aucun gabarit pilotable dans la scène");
            }
            lobby.names.insert(id, name);
            if let Some(uid) = firebase_uid {
                lobby.firebase_uids.insert(id, uid);
            }
        }
        ClientMsg::Input {
            move_x,
            move_y,
            attack,
            jump,
        } => {
            app.set_network_input(
                id,
                NetworkInput {
                    move_x,
                    move_y,
                    attack,
                    jump,
                },
            );
        }
        ClientMsg::Leave => {
            app.despawn_network_player(id);
            lobby.names.remove(&id);
            lobby.firebase_uids.remove(&id);
            log::info!("Joueur {id} quitte la partie");
            net.broadcast(&ServerMsg::PlayerLeft { player_id: id });
        }
    }
}

/// Lit la config Firebase serveur depuis l'environnement et se connecte une
/// fois (cf. le commentaire « Qui écrit la progression ? » dans
/// `net::firebase`). `None` si les variables ne sont pas toutes présentes —
/// la progression est alors simplement désactivée, pas une erreur fatale.
fn connect_firebase_server() -> Option<(FirebaseConfig, AuthSession)> {
    let api_key = std::env::var("FIREBASE_API_KEY").ok()?;
    let database_url = std::env::var("FIREBASE_DATABASE_URL").ok()?;
    let email = std::env::var("FIREBASE_SERVER_EMAIL").ok()?;
    let password = std::env::var("FIREBASE_SERVER_PASSWORD").ok()?;
    let config = FirebaseConfig {
        api_key,
        database_url,
    };
    match firebase::sign_in(&config, &email, &password) {
        Ok(session) => {
            log::info!(
                "Firebase : connecté avec le compte serveur ({})",
                session.uid
            );
            Some((config, session))
        }
        Err(e) => {
            log::warn!(
                "Firebase : connexion du compte serveur échouée ({e}) — progression désactivée"
            );
            None
        }
    }
}

/// Crédite le score de la manche en XP à chaque joueur réseau connu de
/// Firebase. Les échecs (réseau, règles RTDB non configurées...) sont logués
/// mais ne font pas planter le serveur — la progression est un bonus, pas une
/// condition de fonctionnement du jeu.
fn award_progress(firebase: &Option<(FirebaseConfig, AuthSession)>, lobby: &Lobby, score: u32) {
    let Some((config, session)) = firebase else {
        return;
    };
    for (id, uid) in &lobby.firebase_uids {
        let previous = match firebase::get_progress(config, uid) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Firebase : lecture progression du joueur {id} échouée ({e})");
                PlayerProgress::default()
            }
        };
        let xp = previous.xp + score;
        let level = 1 + xp / XP_PER_LEVEL;
        let updated = PlayerProgress { level, xp };
        match firebase::set_progress(config, uid, updated, &session.id_token) {
            Ok(()) => {
                log::info!("Firebase : joueur {id} ({uid}) → niveau {level}, {xp} XP (+{score})")
            }
            Err(e) => log::warn!("Firebase : écriture progression du joueur {id} échouée ({e})"),
        }
    }
}

/// Poste une entrée de classement pour chaque joueur réseau connu de Firebase
/// (même score que `award_progress`, appelé juste après elle en fin de
/// manche). Mêmes garanties : jamais fatal, juste logué en cas d'échec.
fn post_leaderboard(firebase: &Option<(FirebaseConfig, AuthSession)>, lobby: &Lobby, score: u32) {
    let Some((config, session)) = firebase else {
        return;
    };
    let achieved_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    for id in lobby.firebase_uids.keys() {
        let name = lobby
            .names
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("Joueur {id}"));
        let entry = LeaderboardEntry {
            name,
            score,
            achieved_at_ms,
        };
        match firebase::post_leaderboard_entry(config, &session.id_token, &entry) {
            Ok(()) => log::info!("Firebase : classement mis à jour pour le joueur {id} ({score})"),
            Err(e) => log::warn!("Firebase : écriture classement du joueur {id} échouée ({e})"),
        }
    }
}

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

    let firebase = connect_firebase_server();
    if firebase.is_none() {
        log::info!(
            "Firebase désactivé (FIREBASE_API_KEY/DATABASE_URL/SERVER_EMAIL/SERVER_PASSWORD \
             non renseignées) — pas de progression persistante pour cette manche"
        );
    }

    let mut app = AppState::new();
    app.load_zombies_demo();
    app.playing = true;

    let mut lobby = Lobby::default();

    let mut last_wave = app.wave;
    let mut last_score = app.score();
    let started = Instant::now();
    let mut tick: u32 = 0;

    loop {
        let tick_start = Instant::now();

        if let Some(net) = &net {
            while let Ok((id, msg)) = net.inbox.try_recv() {
                handle_message(&mut app, net, &mut lobby, id, msg);
            }
        }

        app.advance_play();
        tick += 1;

        if let Some(net) = &net {
            net.broadcast(&ServerMsg::Snapshot(app.network_snapshot(tick)));
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
            award_progress(&firebase, &lobby, app.score());
            post_leaderboard(&firebase, &lobby, app.score());
            break;
        }
        if app.is_lost() {
            log::info!(
                "Manche terminée : défaite, score final {} (en {:.1} s)",
                app.score(),
                started.elapsed().as_secs_f32()
            );
            award_progress(&firebase, &lobby, app.score());
            post_leaderboard(&firebase, &lobby, app.score());
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use motor3derust::net::client::NetClient;
    use motor3derust::net::protocol::ServerMsg;

    use super::*;

    /// Bout-en-bout Sprint 55, à travers un vrai socket (pas seulement les
    /// méthodes `AppState` testées isolément dans `app::multiplayer::tests`) :
    /// un `NetClient` rejoint, obtient un objet pilotable, son `Input` déplace
    /// *cet* objet, puis `Leave` le retire. Reproduit exactement la boucle de
    /// `main` (via `handle_message`) sans lancer le binaire dans un sous-processus.
    #[test]
    fn joining_moving_and_leaving_through_the_real_socket() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut app = AppState::new();
        app.load_zombies_demo();
        app.playing = true;
        let mut lobby = Lobby::default();

        let client = NetClient::connect(&url, "Alice", None).expect("connexion du client");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu")
        else {
            panic!("premier message attendu : Welcome");
        };

        // Traite le `Join` relayé par le serveur (comme le ferait `main`).
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        assert_eq!(id, player_id);
        handle_message(&mut app, &net, &mut lobby, id, msg);

        let object_index = app
            .network_player_object(player_id)
            .expect("le Join doit avoir fait apparaître un objet pilotable");
        let start = app.scene.objects[object_index].transform.position;

        client.send(&motor3derust::net::protocol::ClientMsg::Input {
            move_x: 1.0,
            move_y: 0.0,
            attack: false,
            jump: false,
        });
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Input attendu côté serveur");
        handle_message(&mut app, &net, &mut lobby, id, msg);

        // Pas d'accès à `last_frame` (privé) depuis ce binaire externe : on avance
        // en temps réel, comme le fait réellement `main` (contrairement aux tests
        // internes de `app::multiplayer`, qui peuvent retarder `last_frame`
        // directement pour rester déterministes sans dormir).
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(20));
            app.advance_play();
        }
        let end = app.scene.objects[object_index].transform.position;
        assert!(
            (end.x - start.x).abs() > 0.5,
            "l'Input du client doit avoir déplacé son propre objet : {start:?} -> {end:?}"
        );

        client.send(&motor3derust::net::protocol::ClientMsg::Leave);
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Leave attendu côté serveur");
        handle_message(&mut app, &net, &mut lobby, id, msg);
        assert_eq!(app.network_player_object(player_id), None);
        assert!(
            !app.scene.objects[object_index].visible,
            "l'objet du joueur parti doit être masqué"
        );
    }
}
