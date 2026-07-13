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
///
/// **Relevée de 20 Hz à 50 Hz (2026-07-12)** : à 20 Hz, chaque fantôme distant
/// n'a une position fraîche que toutes les 50 ms, et `RemoteEntity::sample`
/// interpole *entre* les deux derniers snapshots reçus — donc affiche toujours
/// un état vieux d'au moins un tick, en plus du round-trip réseau réel. Constaté
/// en test réel : latence perçue trop grande sur le mouvement des autres
/// joueurs. Le Sprint 61 a mesuré une large marge à 16 joueurs même à 20 Hz
/// (30 threads OS, aucune limite CPU/réseau atteinte) — 60 Hz reste trivial à
/// cette échelle (2 joueurs de test).
///
/// **Alignée sur 60 Hz (2026-07-12)**, la cadence de `advance_play`/la physique
/// elle-même (`FIXED_DT` dans `AppState::advance_play`) : un tick réseau par
/// pas physique, au lieu d'un rythme intermédiaire arbitraire — chaque
/// `Snapshot` reflète alors un état fraîchement simulé, jamais un état déjà
/// périmé de plusieurs pas physiques en attendant le prochain tick réseau.
const SERVER_TICK: Duration = Duration::from_millis(16); // ~60 Hz

/// Durée maximale d'une manche avant arrêt de sécurité (évite une boucle infinie si
/// la manche ne se termine jamais, ex. bug de configuration de scène).
const MAX_DURATION: Duration = Duration::from_secs(1200);

/// Adresse d'écoute par défaut ; `RUSTEEGEAR_SERVER_ADDR` pour surcharger (ex. tests
/// manuels avec plusieurs instances sur la même machine).
const DEFAULT_ADDR: &str = "127.0.0.1:7777";

/// XP nécessaire pour passer au niveau suivant (formule volontairement simple :
/// un palier fixe, pas de courbe — à raffiner si besoin une fois testé en
/// conditions réelles).
const XP_PER_LEVEL: u32 = 1000;

/// Durée sans le moindre message d'un joueur réseau (même un `Input` inchangé —
/// cf. le protocole, un client légitime en envoie un par tick) au-delà de
/// laquelle il est considéré perdu et retiré de la partie (Sprint 60). Un
/// client frappé de silence radio (freeze, crash sans fermeture propre de la
/// socket) ne doit pas laisser un objet fantôme immobile indéfiniment dans la
/// manche des autres joueurs.
///
/// **Relevé de 10 s à 60 s (constaté en test réel du 2026-07-12)** : le rendu
/// desktop (`winit`/macOS) ralentit ou suspend `advance_play` — donc l'envoi
/// d'`Input` — quand la fenêtre n'est plus au premier plan/est occultée (App
/// Nap), et Android fait de même en arrière-plan. Un client légitime qui perd
/// juste le focus quelques secondes se faisait éjecter par cette limite,
/// silencieusement (aucune des deux apps ne détecte sa propre éviction),
/// rendant le multijoueur quasi inutilisable dès qu'on changeait de fenêtre
/// pour regarder autre chose.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

/// État du salon côté binaire (pas dans `AppState`, qui ne connaît que les
/// indices d'objets, cf. `app::multiplayer`) : nom affiché, `uid` Firebase et
/// dernière activité de chaque joueur réseau connecté.
#[derive(Default)]
struct Lobby {
    names: HashMap<u32, String>,
    firebase_uids: HashMap<u32, String>,
    /// Horodatage du dernier message reçu de chaque joueur (cf. `CLIENT_TIMEOUT`).
    last_seen: HashMap<u32, Instant>,
}

impl Lobby {
    fn forget(&mut self, id: u32) {
        self.names.remove(&id);
        self.firebase_uids.remove(&id);
        self.last_seen.remove(&id);
    }
}

/// Traite un message reçu d'un client : fait entrer/sortir le joueur de la
/// partie ou met à jour son `Input` courant. Extrait de `main` pour rester
/// testable (cf. `tests::joining_moving_and_leaving_through_the_real_socket`)
/// sans avoir à lancer le binaire complet.
fn handle_message(app: &mut AppState, net: &NetServer, lobby: &mut Lobby, id: u32, msg: ClientMsg) {
    if !matches!(msg, ClientMsg::Leave) {
        lobby.last_seen.insert(id, Instant::now());
    }
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
        ClientMsg::Leave => {
            app.despawn_network_player(id);
            lobby.forget(id);
            log::info!("Joueur {id} quitte la partie");
            net.broadcast(&ServerMsg::PlayerLeft { player_id: id });
        }
    }
}

/// Retire les joueurs réseau sans le moindre message depuis `timeout` (cf. la
/// doc de `CLIENT_TIMEOUT`) — appelé une fois par tick avec `CLIENT_TIMEOUT`,
/// après avoir traité les messages reçus. Symétrique à un `ClientMsg::Leave`
/// explicite (même nettoyage), sauf que c'est le serveur qui l'initie faute de
/// nouvelles du client. `timeout` en paramètre (pas seulement la constante) :
/// permet aux tests d'utiliser un délai court plutôt que d'attendre 10 s réelles.
fn evict_timed_out_players(
    app: &mut AppState,
    net: &NetServer,
    lobby: &mut Lobby,
    timeout: Duration,
) {
    let now = Instant::now();
    let timed_out: Vec<u32> = lobby
        .last_seen
        .iter()
        .filter(|&(_, &at)| now.duration_since(at) > timeout)
        .map(|(&id, _)| id)
        .collect();
    for id in timed_out {
        log::warn!("Joueur {id} : timeout ({timeout:?} sans message), retiré de la partie");
        app.despawn_network_player(id);
        lobby.forget(id);
        net.broadcast(&ServerMsg::PlayerLeft { player_id: id });
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
    // Charge la même scène que les clients (le jeu réellement exporté, cf.
    // `assets/player_scene.json`/`Scene::embedded_player`) plutôt que l'arène
    // de test générique `Scene::mmorpg_demo` : le serveur est autoritaire sur
    // les positions, transmises telles quelles aux clients (`network_snapshot`) —
    // si sa scène diffère de la leur (géométrie/repère différents), un fantôme
    // reçoit des coordonnées qui ne correspondent à rien de visible dans LEUR
    // scène (constaté en conditions réelles : deux clients connectés plusieurs
    // minutes sans jamais se voir, cf. le test manuel du 2026-07-12).
    app.use_embedded_scene();
    // Masque le gabarit joueur local *avant* le premier join : sans ça, l'IA
    // le poursuit et sa santé s'épuise pendant l'attente du premier joueur,
    // terminant la manche en défaite avant même qu'un joueur ait pu se
    // connecter (cf. AUDIT_MMORPG.md, bug trouvé en conditions réelles).
    app.hide_local_player_template();
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
            evict_timed_out_players(&mut app, net, &mut lobby, CLIENT_TIMEOUT);
        }

        app.advance_play();
        tick += 1;

        if let Some(net) = &net {
            net.broadcast(&ServerMsg::Snapshot(app.network_snapshot(tick)));
            // Évènements ponctuels produits par la simulation de ce tick (monstre
            // vaincu par une boule de feu...) : diffusés une fois, pour que les
            // clients jouent son/flash sans attendre de comparer deux snapshots.
            for event in app.take_net_events() {
                net.broadcast(&ServerMsg::Event(event));
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
            award_progress(&firebase, &lobby, app.score());
            post_leaderboard(&firebase, &lobby, app.score());
            break;
        }
        // `is_room_lost()` (pas `is_lost()`, pensé pour un joueur local unique) :
        // en multijoueur, la défaite de salon n'arrive que si TOUS les joueurs
        // réseau connus sont vaincus (GAMEDESIGN_EN_LIGNE.md §3.1) — un seul
        // joueur qui meurt devient spectateur, la manche continue pour les autres.
        if app.is_room_lost() {
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
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
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

    /// Sprint 60 : un joueur qui ne donne plus signe de vie (freeze, crash sans
    /// `Leave` propre) doit être retiré après le délai de timeout, sans bloquer
    /// la partie des autres. Utilise un `timeout` court (paramètre de
    /// `evict_timed_out_players`) plutôt que `CLIENT_TIMEOUT` (10 s réelles).
    #[test]
    fn a_silent_client_is_evicted_after_the_timeout() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut app = AppState::new();
        app.load_zombies_demo();
        app.playing = true;
        let mut lobby = Lobby::default();

        let client = NetClient::connect(&url, "Silencieux", None).expect("connexion");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu")
        else {
            panic!("premier message attendu : Welcome");
        };
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut app, &net, &mut lobby, id, msg);
        assert!(app.network_player_object(player_id).is_some());

        // Aucun message pendant plus que le timeout court : le joueur doit être
        // évincé au prochain passage de `evict_timed_out_players`.
        let short_timeout = Duration::from_millis(50);
        std::thread::sleep(Duration::from_millis(120));
        evict_timed_out_players(&mut app, &net, &mut lobby, short_timeout);

        assert_eq!(
            app.network_player_object(player_id),
            None,
            "un joueur silencieux depuis plus que le timeout doit être retiré"
        );
        assert!(!lobby.last_seen.contains_key(&player_id));
    }
}
