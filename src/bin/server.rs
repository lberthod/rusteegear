//! Serveur de jeu headless : fait tourner des manches en réutilisant
//! `scene`/`runtime`/`app::combat`/`app::multiplayer` **sans fenêtre ni GPU**
//! (aucune dépendance à `gfx`/`egui`/`winit` dans ce binaire), et accepte des
//! connexions WebSocket (`net::server_loop`).
//!
//! **Multi-salons** (cf. GAMEDESIGN_EN_LIGNE.md §3.3) : un process sert
//! plusieurs salons simultanément, chacun sa propre `AppState` (donc sa propre
//! scène, ses propres joueurs, sa propre victoire/défaite) —
//! `ClientMsg::Join::lobby` choisit le salon (créé à la demande au premier
//! join, fermé quand son dernier joueur part). Portée volontairement mesurée,
//! pas un vrai matchmaking MMO : pas de découverte de salons, juste un code à
//! saisir (cf. `net::protocol::DEFAULT_LOBBY`, utilisé par tous les clients
//! actuels — ils continuent donc à se retrouver dans le même salon partagé
//! tant qu'aucune UI ne propose de choisir un autre code). Une manche décidée
//! (victoire/défaite) ne termine pas le *process* : seul ce salon est
//! réinitialisé en place (les joueurs encore connectés y sont re-spawnés),
//! les autres salons continuent sans interruption.
//!
//! **Progression Firebase** : optionnelle, activée par 4 variables
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
use motor3derust::net::protocol::{
    ClientMsg, DEFAULT_LOBBY, PlayerId, ServerMsg, valid_join_fields,
};
use motor3derust::net::server_loop::NetServer;

/// Cadence réseau du serveur : alignée sur la cadence de la physique elle-même
/// (`FIXED_DT` dans `AppState::advance_play`) — un tick réseau par pas
/// physique, au lieu d'un rythme intermédiaire arbitraire, pour que chaque
/// `Snapshot` reflète un état fraîchement simulé plutôt qu'un état déjà périmé
/// de plusieurs pas physiques en attendant le prochain tick réseau (cf.
/// docs/audits/misc.md pour la latence perçue que mesurait une cadence plus
/// basse, et la marge CPU/réseau disponible à cette fréquence).
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
/// laquelle il est considéré perdu et retiré de la partie. Un client frappé de
/// silence radio (freeze, crash sans fermeture propre de la socket) ne doit
/// pas laisser un objet fantôme immobile indéfiniment dans la manche des
/// autres joueurs.
///
/// Volontairement généreux (pas quelques secondes) : le rendu desktop
/// (`winit`/macOS) ralentit ou suspend `advance_play` — donc l'envoi
/// d'`Input` — quand la fenêtre n'est plus au premier plan/est occultée (App
/// Nap), et Android fait de même en arrière-plan ; aucune des deux apps ne
/// détecte sa propre éviction, donc un client légitime qui perd juste le
/// focus quelques secondes ne doit pas se faire éjecter silencieusement (cf.
/// docs/audits/misc.md).
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

/// État d'un salon côté binaire (pas dans `AppState`, qui ne connaît que les
/// indices d'objets, cf. `app::multiplayer`) : nom affiché, `uid` Firebase et
/// dernière activité de chaque joueur réseau connecté à **ce** salon.
#[derive(Default)]
struct Lobby {
    names: HashMap<PlayerId, String>,
    firebase_uids: HashMap<PlayerId, String>,
    /// Horodatage du dernier message reçu de chaque joueur (cf. `CLIENT_TIMEOUT`).
    last_seen: HashMap<PlayerId, Instant>,
}

impl Lobby {
    fn forget(&mut self, id: PlayerId) {
        self.names.remove(&id);
        self.firebase_uids.remove(&id);
        self.last_seen.remove(&id);
    }
}

/// Un salon : sa propre manche (`AppState`, donc sa propre scène/physique/
/// combat), ses propres joueurs connectés, et le suivi nécessaire pour logger
/// les changements de manche/score sans les répéter à chaque tick.
struct Room {
    app: AppState,
    lobby: Lobby,
    last_wave: u32,
    last_score: u32,
    started: Instant,
}

impl Room {
    /// Charge une manche fraîche : la même scène que les clients (cf.
    /// `AppState::use_embedded_scene`), gabarit local masqué avant le premier
    /// join (`AUDIT_MMORPG.md` : sans ça, l'IA poursuit un mannequin inerte et
    /// sa santé s'épuise pendant l'attente du premier joueur).
    fn new() -> Self {
        let mut app = AppState::new();
        app.use_embedded_scene();
        app.hide_local_player_template();
        app.playing = true;
        let last_wave = app.wave;
        let last_score = app.score();
        Room {
            app,
            lobby: Lobby::default(),
            last_wave,
            last_score,
            started: Instant::now(),
        }
    }

    /// Recharge une manche fraîche **sans déconnecter** les joueurs déjà
    /// présents : ils sont re-spawnés dans la scène recomposée. Appelé quand
    /// la manche de ce salon se termine (victoire/défaite) ou dépasse
    /// `MAX_DURATION` — seul ce salon repart, les autres salons ne sont pas
    /// affectés.
    fn restart(&mut self) {
        let ids: Vec<PlayerId> = self.lobby.names.keys().copied().collect();
        self.app = AppState::new();
        self.app.use_embedded_scene();
        self.app.hide_local_player_template();
        self.app.playing = true;
        for id in ids {
            self.app.spawn_network_player(id);
        }
        self.last_wave = self.app.wave;
        self.last_score = self.app.score();
        self.started = Instant::now();
    }

    /// Joueurs actuellement connectés à ce salon (pour cibler les envois —
    /// `NetServer` ne connaît pas la notion de salon, cf. sa doc : un
    /// `broadcast()` atteint TOUS les clients du serveur, pas seulement ceux
    /// d'un salon donné, donc jamais utilisé ici, uniquement `send_to` en boucle).
    fn connected_ids(&self) -> Vec<PlayerId> {
        self.lobby.names.keys().copied().collect()
    }
}

/// Traite un message reçu d'un client : fait entrer/sortir le joueur d'un
/// salon ou met à jour son `Input` courant. Extrait de `main` pour rester
/// testable (cf. `tests::joining_moving_and_leaving_through_the_real_socket`)
/// sans avoir à lancer le binaire complet.
///
/// `player_room` associe chaque joueur connecté au code du salon qu'il a
/// rejoint (renseigné au `Join`, consulté pour router `Input`/`Leave` sans
/// que ces messages n'aient besoin de reporter le code à chaque fois).
fn handle_message(
    rooms: &mut HashMap<String, Room>,
    player_room: &mut HashMap<PlayerId, String>,
    net: &NetServer,
    id: PlayerId,
    msg: ClientMsg,
) {
    match msg {
        ClientMsg::Join {
            name,
            firebase_uid,
            lobby,
        } => {
            // Durcissement (Sprint 105a-2) : `lobby` devient une clé de `rooms`
            // et `firebase_uid` finit non échappé dans une URL Firebase RTDB
            // (`net::firebase::rtdb_url`) — un champ hors bornes/charset
            // rejeté ici, avant toute inscription, plutôt qu'un comportement
            // indéfini plus loin dans la chaîne.
            if let Err(e) = valid_join_fields(&name, &lobby, firebase_uid.as_deref()) {
                log::warn!("Join rejeté ({id}) : {e}");
                return;
            }
            let code = if lobby.trim().is_empty() {
                DEFAULT_LOBBY.to_string()
            } else {
                lobby
            };
            let room = rooms.entry(code.clone()).or_insert_with(Room::new);
            room.lobby.last_seen.insert(id, Instant::now());
            if room.app.spawn_network_player(id).is_some() {
                log::info!("Joueur {id} ({name}) entre en jeu (salon « {code} »)");
                room.lobby.names.insert(id, name.clone());
                if let Some(uid) = firebase_uid {
                    room.lobby.firebase_uids.insert(id, uid);
                }
                player_room.insert(id, code);
                for pid in room.connected_ids() {
                    net.send_to(
                        pid,
                        &ServerMsg::PlayerJoined {
                            player_id: id,
                            name: name.clone(),
                        },
                    );
                }
            } else {
                log::warn!(
                    "Joueur {id} ({name}) : aucun gabarit pilotable dans la scène (salon « {code} »)"
                );
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
            let Some(room) = player_room.get(&id).and_then(|code| rooms.get_mut(code)) else {
                return;
            };
            room.lobby.last_seen.insert(id, Instant::now());
            room.app.set_network_input(
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
            let Some(code) = player_room.remove(&id) else {
                return;
            };
            let Some(room) = rooms.get_mut(&code) else {
                return;
            };
            room.app.despawn_network_player(id);
            room.lobby.forget(id);
            log::info!("Joueur {id} quitte le salon « {code} »");
            for pid in room.connected_ids() {
                net.send_to(pid, &ServerMsg::PlayerLeft { player_id: id });
            }
        }
    }
}

/// Retire, dans chaque salon, les joueurs réseau sans le moindre message
/// depuis `timeout` (cf. la doc de `CLIENT_TIMEOUT`) — appelé une fois par
/// tick avec `CLIENT_TIMEOUT`, après avoir traité les messages reçus.
/// Symétrique à un `ClientMsg::Leave` explicite (même nettoyage), sauf que
/// c'est le serveur qui l'initie faute de nouvelles du client. `timeout` en
/// paramètre (pas seulement la constante) : permet aux tests d'utiliser un
/// délai court plutôt que d'attendre 60 s réelles.
fn evict_timed_out_players(
    rooms: &mut HashMap<String, Room>,
    player_room: &mut HashMap<PlayerId, String>,
    net: &NetServer,
    timeout: Duration,
) {
    let now = Instant::now();
    for room in rooms.values_mut() {
        let timed_out: Vec<PlayerId> = room
            .lobby
            .last_seen
            .iter()
            .filter(|&(_, &at)| now.duration_since(at) > timeout)
            .map(|(&id, _)| id)
            .collect();
        for id in timed_out {
            log::warn!("Joueur {id} : timeout ({timeout:?} sans message), retiré de la partie");
            room.app.despawn_network_player(id);
            room.lobby.forget(id);
            player_room.remove(&id);
            for pid in room.connected_ids() {
                net.send_to(pid, &ServerMsg::PlayerLeft { player_id: id });
            }
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
    log::info!("RusteeGear — serveur headless : salons multiples (Sprint 82)");

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

    let mut rooms: HashMap<String, Room> = HashMap::new();
    let mut player_room: HashMap<PlayerId, String> = HashMap::new();
    let mut tick: u32 = 0;

    // Sans réseau (bind échoué) : un unique salon local, pour ne pas régresser
    // le comportement historique (aucun moyen de le rejoindre de toute façon,
    // mais la manche tourne quand même — utile en test manuel sans port libre).
    if net.is_none() {
        rooms.insert(DEFAULT_LOBBY.to_string(), Room::new());
    }

    loop {
        let tick_start = Instant::now();

        if let Some(net) = &net {
            while let Ok((id, msg)) = net.inbox.try_recv() {
                handle_message(&mut rooms, &mut player_room, net, id, msg);
            }
            evict_timed_out_players(&mut rooms, &mut player_room, net, CLIENT_TIMEOUT);
        }

        let mut to_close: Vec<String> = Vec::new();
        for (code, room) in rooms.iter_mut() {
            room.app.advance_play();

            if let Some(net) = &net {
                let ids = room.connected_ids();
                let snapshot = ServerMsg::Snapshot(room.app.network_snapshot(tick));
                for &pid in &ids {
                    net.send_to(pid, &snapshot);
                }
                // Évènements ponctuels produits par la simulation de ce tick
                // (monstre vaincu, joueur vaincu...) : diffusés une fois, pour
                // que les clients réagissent (son/flash) sans comparer deux
                // snapshots — uniquement aux joueurs *de ce salon*.
                for event in room.app.take_net_events() {
                    let msg = ServerMsg::Event(event);
                    for &pid in &ids {
                        net.send_to(pid, &msg);
                    }
                }
            }

            if room.app.wave != room.last_wave {
                log::info!("[{code}] Manche {} révélée", room.app.wave);
                room.last_wave = room.app.wave;
            }
            if room.app.score() != room.last_score {
                log::info!("[{code}] Score : {}", room.app.score());
                room.last_score = room.app.score();
            }

            // `is_room_lost()` (pas `is_lost()`, pensé pour un joueur local
            // unique) : la défaite n'arrive que si TOUS les joueurs réseau de
            // CE salon sont vaincus (GAMEDESIGN_EN_LIGNE.md §3.1) — un seul
            // joueur qui meurt devient spectateur, la manche continue pour
            // les autres, dans ce salon comme dans les autres.
            let decided = room.app.has_won() || room.app.is_room_lost();
            let timed_out = room.started.elapsed() > MAX_DURATION;
            if decided || timed_out {
                if decided {
                    log::info!(
                        "[{code}] Manche terminée : {}, score final {} (en {:.1} s)",
                        if room.app.has_won() {
                            "victoire"
                        } else {
                            "défaite"
                        },
                        room.app.score(),
                        room.started.elapsed().as_secs_f32()
                    );
                } else {
                    log::warn!(
                        "[{code}] Arrêt de sécurité : durée maximale de manche atteinte sans issue"
                    );
                }
                award_progress(&firebase, &room.lobby, room.app.score());
                post_leaderboard(&firebase, &room.lobby, room.app.score());
                // Une manche décidée ne ferme pas tout le serveur : seul CE
                // salon repart, les autres continuent — sauf s'il est déjà
                // vide (dernier joueur parti entre-temps), auquel cas autant
                // le fermer plutôt que de le faire tourner pour personne.
                if room.connected_ids().is_empty() {
                    to_close.push(code.clone());
                } else {
                    room.restart();
                }
            }
        }
        for code in to_close {
            rooms.remove(&code);
            log::info!("Salon « {code} » fermé (vide)");
        }

        tick += 1;

        let elapsed = tick_start.elapsed();
        if elapsed < SERVER_TICK {
            std::thread::sleep(SERVER_TICK - elapsed);
        }
    }
}

// Sprint 105a-3 : tous les tests de ce module ouvrent un vrai socket
// (NetServer/NetClient) — regroupés derrière `net_tests` plutôt qu'annotés
// un par un, `cargo test` par défaut reste rapide et indépendant d'un
// environnement CI qui restreint parfois le bind loopback (cf.
// docs/architecture.md, section réseau, pour lancer la couverture complète).
#[cfg(all(test, feature = "net_tests"))]
mod tests {
    use std::time::Duration;

    use motor3derust::net::client::NetClient;
    use motor3derust::net::protocol::ServerMsg;

    use super::*;

    /// Bout-en-bout à travers un vrai socket (pas seulement les
    /// méthodes `AppState` testées isolément dans `app::multiplayer::tests`) :
    /// un `NetClient` rejoint, obtient un objet pilotable, son `Input` déplace
    /// *cet* objet, puis `Leave` le retire. Reproduit exactement la boucle de
    /// `main` (via `handle_message`) sans lancer le binaire dans un sous-processus.
    /// Construit une manche de test (démo zombies, pilotable + monstres) plutôt
    /// que la scène embarquée (`Room::new()`) : ces tests visent la plomberie
    /// réseau/salons, pas le contenu de `assets/player_scene.json`.
    fn zombies_room() -> Room {
        let mut app = AppState::new();
        app.load_zombies_demo();
        app.playing = true;
        Room {
            app,
            lobby: Lobby::default(),
            last_wave: 0,
            last_score: 0,
            started: Instant::now(),
        }
    }

    #[test]
    fn joining_moving_and_leaving_through_the_real_socket() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

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
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        let object_index = room
            .app
            .network_player_object(player_id)
            .expect("le Join doit avoir fait apparaître un objet pilotable");
        let start = room.app.scene.objects[object_index].transform.position;

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
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // Pas d'accès à `last_frame` (privé) depuis ce binaire externe : on avance
        // en temps réel, comme le fait réellement `main` (contrairement aux tests
        // internes de `app::multiplayer`, qui peuvent retarder `last_frame`
        // directement pour rester déterministes sans dormir).
        let room = rooms.get_mut(DEFAULT_LOBBY).unwrap();
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(20));
            room.app.advance_play();
        }
        let end = room.app.scene.objects[object_index].transform.position;
        assert!(
            (end.x - start.x).abs() > 0.5,
            "l'Input du client doit avoir déplacé son propre objet : {start:?} -> {end:?}"
        );

        client.send(&motor3derust::net::protocol::ClientMsg::Leave);
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Leave attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);
        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        assert_eq!(room.app.network_player_object(player_id), None);
        assert!(
            !room.app.scene.objects[object_index].visible,
            "l'objet du joueur parti doit être masqué"
        );
    }

    /// Sprint 105a-2 (durcissement des entrées réseau) : un `Join` dont le
    /// code de salon contient des caractères interdits (`valid_join_fields`)
    /// est rejeté — le joueur ne doit apparaître dans aucun salon, à la
    /// différence d'un `Join` valide (cf. `joining_moving_and_leaving_
    /// through_the_real_socket` ci-dessus). Le transport (`Welcome`) reste
    /// inconditionnel (envoyé avant que `handle_message` ne voie le `Join`),
    /// seule l'inscription applicative est bloquée.
    #[test]
    fn a_join_with_an_unsafe_lobby_code_is_rejected() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect_to_lobby(&url, "Alice", None, "salon/traversal")
            .expect("connexion du client");
        let ServerMsg::Welcome { player_id } = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu (transport inconditionnel)")
        else {
            panic!("premier message attendu : Welcome");
        };

        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        assert_eq!(
            room.app.network_player_object(player_id),
            None,
            "un Join avec un code de salon invalide ne doit inscrire le joueur \
             dans aucun salon"
        );
        assert!(
            !player_room.contains_key(&player_id),
            "un Join rejeté ne doit pas router les messages suivants de ce joueur"
        );
    }

    /// Sprint 103c (audit réseau après la migration du joueur vers
    /// `KinematicCharacterController`, Sprint 103b) : livrable explicite du
    /// roadmap — « aucun rubber-banding à 100 ms simulées ». Mêmes
    /// `NetServer`/`NetClient` réels que `joining_moving_and_leaving_
    /// through_the_real_socket` ci-dessus, mais le serveur ne traite son
    /// inbox/n'avance sa simulation qu'une fois toutes les 100 ms (au lieu
    /// des ~20 ms habituels) — une pacing bien plus lente que le tick
    /// serveur réel simule un aller-retour réseau dégradé sans horloge
    /// simulée (ce dépôt n'utilise que des `sleep`/`Instant` réels, cf.
    /// `SPRINTNETWORK.md`). « Rubber-banding » = la position oscille ou
    /// recule brièvement avant de repartir en avant ; ce test suit la
    /// position à chaque tick traité et vérifie qu'elle progresse
    /// globalement dans le sens du mouvement, jamais un aller-retour marqué
    /// entre deux ticks consécutifs.
    #[test]
    fn sustained_movement_does_not_rubber_band_at_100ms_simulated_latency() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client = NetClient::connect(&url, "Bob", None).expect("connexion du client");
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
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        let object_index = room
            .app
            .network_player_object(player_id)
            .expect("le Join doit avoir fait apparaître un objet pilotable");
        let start = room.app.scene.objects[object_index].transform.position;

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
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // Comme `joining_moving_and_leaving_through_the_real_socket` : pas
        // d'autre `Input` envoyé après celui-ci, `advance_play` continue de
        // piloter l'objet à partir de la dernière entrée connue
        // (`network_inputs`, persistante jusqu'au prochain message) — inutile
        // de redrainer l'inbox à chaque tick de la boucle.
        let room = rooms.get_mut(DEFAULT_LOBBY).unwrap();
        let mut previous = start;
        let mut max_backward_step = 0.0_f32;
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(100));
            room.app.advance_play();
            let current = room.app.scene.objects[object_index].transform.position;
            // Recul entre deux ticks consécutifs le long de l'axe de
            // déplacement (X, `move_x = 1.0` ci-dessus) : au-delà d'un bruit
            // négligeable, ce serait le symptôme même du rubber-banding.
            let backward = (previous.x - current.x).max(0.0);
            max_backward_step = max_backward_step.max(backward);
            previous = current;
        }

        let end = room.app.scene.objects[object_index].transform.position;
        assert!(
            (end.x - start.x).abs() > 0.5,
            "le mouvement doit progresser malgré la latence simulée : {start:?} -> {end:?}"
        );
        assert!(
            max_backward_step < 0.05,
            "aucun tick ne doit reculer sensiblement (rubber-banding) : recul \
             maximal observé {max_backward_step} m"
        );
    }

    /// Un joueur qui ne donne plus signe de vie (freeze, crash sans
    /// `Leave` propre) doit être retiré après le délai de timeout, sans bloquer
    /// la partie des autres. Utilise un `timeout` court (paramètre de
    /// `evict_timed_out_players`) plutôt que `CLIENT_TIMEOUT` (60 s réelles).
    #[test]
    fn a_silent_client_is_evicted_after_the_timeout() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        rooms.insert(DEFAULT_LOBBY.to_string(), zombies_room());
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

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
        handle_message(&mut rooms, &mut player_room, &net, id, msg);
        assert!(
            rooms
                .get(DEFAULT_LOBBY)
                .unwrap()
                .app
                .network_player_object(player_id)
                .is_some()
        );

        // Aucun message pendant plus que le timeout court : le joueur doit être
        // évincé au prochain passage de `evict_timed_out_players`.
        let short_timeout = Duration::from_millis(50);
        std::thread::sleep(Duration::from_millis(120));
        evict_timed_out_players(&mut rooms, &mut player_room, &net, short_timeout);

        let room = rooms.get(DEFAULT_LOBBY).unwrap();
        assert_eq!(
            room.app.network_player_object(player_id),
            None,
            "un joueur silencieux depuis plus que le timeout doit être retiré"
        );
        assert!(!room.lobby.last_seen.contains_key(&player_id));
        assert!(!player_room.contains_key(&player_id));
    }

    /// Deux clients qui rejoignent des salons différents (cf.
    /// GAMEDESIGN_EN_LIGNE.md §3.3) ne doivent jamais se voir l'un l'autre —
    /// chacun reste dans sa propre `AppState`, avec ses propres indices d'objets.
    #[test]
    fn two_clients_in_different_lobbies_land_in_separate_rooms() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let a = NetClient::connect_to_lobby(&url, "A", None, "salon-a").expect("connexion A");
        let b = NetClient::connect_to_lobby(&url, "B", None, "salon-b").expect("connexion B");
        let welcome_a = a.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        let welcome_b = b.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        let (ServerMsg::Welcome { player_id: id_a }, ServerMsg::Welcome { player_id: id_b }) =
            (welcome_a, welcome_b)
        else {
            panic!("Welcome attendu pour les deux clients");
        };

        for _ in 0..2 {
            let (id, msg) = net
                .inbox
                .recv_timeout(Duration::from_secs(2))
                .expect("Join attendu côté serveur");
            handle_message(&mut rooms, &mut player_room, &net, id, msg);
        }

        assert_eq!(
            rooms.len(),
            2,
            "deux salons distincts doivent avoir été créés"
        );
        assert!(rooms.contains_key("salon-a"));
        assert!(rooms.contains_key("salon-b"));
        assert_eq!(player_room.get(&id_a), Some(&"salon-a".to_string()));
        assert_eq!(player_room.get(&id_b), Some(&"salon-b".to_string()));

        // Le salon de B n'a aucune trace de A, et réciproquement.
        assert!(rooms["salon-a"].app.network_player_object(id_b).is_none());
        assert!(rooms["salon-b"].app.network_player_object(id_a).is_none());
        assert_eq!(rooms["salon-a"].lobby.names.len(), 1);
        assert_eq!(rooms["salon-b"].lobby.names.len(), 1);
    }

    /// Quand le dernier joueur d'un salon part, le salon disparaît
    /// (pas de manche qui tourne indéfiniment pour personne).
    #[test]
    fn a_room_closes_once_its_last_player_leaves() {
        let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", net.local_addr);
        let mut rooms: HashMap<String, Room> = HashMap::new();
        let mut player_room: HashMap<PlayerId, String> = HashMap::new();

        let client =
            NetClient::connect_to_lobby(&url, "Solo", None, "ephemere").expect("connexion");
        client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Welcome attendu");
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Join attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);
        assert!(rooms.contains_key("ephemere"));

        client.send(&motor3derust::net::protocol::ClientMsg::Leave);
        let (id, msg) = net
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("Leave attendu côté serveur");
        handle_message(&mut rooms, &mut player_room, &net, id, msg);

        // `handle_message` masque le joueur mais ne ferme le salon vide que la
        // boucle `main` (le nettoyage `to_close` vit dans `main`, pas dans
        // `handle_message`, pour rester testable sans lancer tout le binaire) —
        // ici on vérifie juste la partie qu'expose `handle_message` :
        // plus aucun joueur connecté, prêt à être fermé au prochain tour de boucle.
        assert!(rooms["ephemere"].connected_ids().is_empty());
    }
}
