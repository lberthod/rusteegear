//! Transport WebSocket côté serveur (SPRINT_MMORPG.md, Sprint 53).
//!
//! `NetServer` accepte des connexions dans un thread dédié, et n'expose au
//! reste du programme que des canaux `std::sync::mpsc` **synchrones** — le
//! même schéma que les imports glTF ou les requêtes IA asynchrones déjà
//! présents dans `app/mod.rs` (thread de fond + canal, poll non bloquant côté
//! boucle principale). La boucle de jeu (`AppState`, `src/bin/server.rs`) n'a
//! donc jamais besoin de connaître `tokio`.
//!
//! **Runtime `current_thread` (corrigé à l'audit du 2026-07-07, cf.
//! AUDIT_MMORPG.md §4.3)** : à l'échelle visée (2-16 joueurs/salon), accepter
//! des connexions et faire progresser une poignée de sockets est un travail
//! d'attente réseau, pas de calcul parallèle — un runtime multi-thread
//! (`tokio::runtime::Runtime::new()`, utilisé avant ce correctif) réserve un
//! thread ouvrier par CPU logique pour rien. Le thread dédié ci-dessous
//! `block_on` la boucle d'acceptation *et* toutes les connexions (via
//! `tokio::spawn`, ordonnancées coopérativement sur ce seul thread) pour toute
//! la durée de vie du serveur.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

use super::protocol::{self, ClientMsg, PlayerId, ServerMsg};

/// Message reçu d'un client, avec l'identifiant du joueur qui l'a envoyé.
pub type Inbound = (PlayerId, ClientMsg);

type Outboxes = Arc<Mutex<HashMap<PlayerId, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>;

/// Serveur réseau : accepte des connexions WebSocket, décode les `ClientMsg` reçus
/// et les pousse dans `inbox` ; `send_to`/`broadcast` encodent et poussent des
/// `ServerMsg` vers un ou tous les clients connectés.
pub struct NetServer {
    /// Messages reçus des clients, à consommer par le thread principal (non
    /// bloquant : `try_recv` une fois par tick).
    pub inbox: Receiver<Inbound>,
    outboxes: Outboxes,
    next_id: Arc<AtomicU32>,
    /// Adresse effectivement liée (utile en test : `"127.0.0.1:0"` laisse l'OS
    /// choisir un port libre).
    pub local_addr: SocketAddr,
}

impl NetServer {
    /// Démarre le serveur sur `addr` (ex. `"127.0.0.1:7777"`).
    pub fn start(addr: &str) -> std::io::Result<Self> {
        let (tx, rx) = channel::<Inbound>();
        let outboxes: Outboxes = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicU32::new(1));

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::io::Result<SocketAddr>>();
        let addr = addr.to_string();
        let accept_outboxes = outboxes.clone();
        let accept_next_id = next_id.clone();
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match TcpListener::bind(&addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let local_addr = listener.local_addr();
                let bind_ok = local_addr.is_ok();
                let _ = ready_tx.send(local_addr);
                if !bind_ok {
                    return;
                }
                loop {
                    let (stream, peer) = match listener.accept().await {
                        Ok(v) => v,
                        Err(e) => {
                            log::warn!("Connexion entrante refusée : {e}");
                            continue;
                        }
                    };
                    let tx = tx.clone();
                    let outboxes = accept_outboxes.clone();
                    let next_id = accept_next_id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, peer, tx, outboxes, next_id).await
                        {
                            log::info!("Connexion {peer} terminée : {e}");
                        }
                    });
                }
            });
        });

        let local_addr = ready_rx.recv().map_err(|_| {
            std::io::Error::other("le thread réseau du serveur s'est arrêté avant le bind")
        })??;

        Ok(Self {
            inbox: rx,
            outboxes,
            next_id,
            local_addr,
        })
    }

    /// Envoie un message à un joueur précis ; sans effet s'il n'est plus connecté.
    pub fn send_to(&self, id: PlayerId, msg: &ServerMsg) {
        let Ok(bytes) = protocol::encode(msg) else {
            return;
        };
        if let Some(tx) = self.outboxes.lock().unwrap().get(&id) {
            let _ = tx.send(bytes);
        }
    }

    /// Envoie un message à tous les joueurs connectés (ex. un `Snapshot` par tick).
    pub fn broadcast(&self, msg: &ServerMsg) {
        let Ok(bytes) = protocol::encode(msg) else {
            return;
        };
        for tx in self.outboxes.lock().unwrap().values() {
            let _ = tx.send(bytes.clone());
        }
    }

    /// Nombre de clients actuellement connectés.
    pub fn connected_count(&self) -> usize {
        self.outboxes.lock().unwrap().len()
    }

    /// Identifiant qui serait attribué au prochain joueur à rejoindre (utile pour
    /// les tests / logs ; ne réserve rien).
    pub fn next_player_id(&self) -> PlayerId {
        self.next_id.load(Ordering::Relaxed)
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    tx: Sender<Inbound>,
    outboxes: Outboxes,
    next_id: Arc<AtomicU32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut sink, mut stream) = ws.split();

    // Première trame attendue : `ClientMsg::Join`. Toute autre trame, ou une
    // déconnexion avant d'avoir rejoint, met fin à la connexion.
    let first = stream.next().await.ok_or("connexion fermée avant Join")??;
    let join_bytes = match first {
        Message::Binary(b) => b,
        _ => return Err("première trame non binaire".into()),
    };
    let ClientMsg::Join { name, firebase_uid } = protocol::decode::<ClientMsg>(&join_bytes)? else {
        return Err("première trame n'est pas un Join".into());
    };

    let id = next_id.fetch_add(1, Ordering::Relaxed);
    log::info!("Joueur {id} ({name}) connecté depuis {peer}");

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    outboxes.lock().unwrap().insert(id, out_tx);

    let welcome = protocol::encode(&ServerMsg::Welcome { player_id: id })?;
    sink.send(Message::Binary(welcome.into())).await?;

    // Relaie aussi le `Join` lui-même au thread principal (contrairement au
    // `Welcome`, géré ici) : c'est le signal qui doit faire apparaître le joueur
    // dans la partie (cf. `AppState::spawn_network_player`, Sprint 55). Une
    // défaillance d'envoi ici (thread principal arrêté) ne doit pas empêcher la
    // connexion de continuer, donc pas de `?`.
    let _ = tx.send((id, ClientMsg::Join { name, firebase_uid }));

    // Pompe sortante : relaie les messages poussés par `send_to`/`broadcast`
    // (thread principal) vers la socket, jusqu'à fermeture du canal ou erreur
    // d'écriture.
    let outbound = async move {
        while let Some(bytes) = out_rx.recv().await {
            if sink.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    };

    // Pompe entrante : décode chaque trame en `ClientMsg` et la transmet au thread
    // principal via le canal synchrone (jamais bloquant : `std::sync::mpsc` est
    // non borné, même choix que pour les imports glTF/IA dans `app/mod.rs`).
    let inbound_tx = tx.clone();
    let inbound = async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Binary(bytes) = msg
                && let Ok(client_msg) = protocol::decode::<ClientMsg>(&bytes)
            {
                let is_leave = matches!(client_msg, ClientMsg::Leave);
                if inbound_tx.send((id, client_msg)).is_err() || is_leave {
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = outbound => {}
        _ = inbound => {}
    }
    outboxes.lock().unwrap().remove(&id);
    // Signale la déconnexion au thread principal, qu'elle soit volontaire (déjà
    // relayée par la pompe entrante) ou abrupte (perte de connexion) — envoyer un
    // second `Leave` dans le premier cas ne coûte rien : `despawn_network_player`
    // est idempotent (retirer un joueur déjà absent ne fait rien).
    let _ = tx.send((id, ClientMsg::Leave));
    log::info!("Joueur {id} déconnecté");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::client::NetClient;
    use super::*;

    /// Bout-en-bout transport (cf. SPRINT_MMORPG.md Sprint 53) : un `NetClient` se
    /// connecte, envoie un `Join`, reçoit son `Welcome`, puis envoie un `Input` que
    /// le serveur doit recevoir avec le bon `PlayerId`. Vérifie la plomberie
    /// WebSocket + (dé)sérialisation sans dépendre d'une fenêtre graphique (aucun
    /// moyen d'ouvrir deux fenêtres winit dans cet environnement de test).
    #[test]
    fn client_joins_and_server_receives_its_input() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let client = NetClient::connect(&url, "Testeur", None).expect("connexion du client");

        let welcome = client
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le client doit recevoir un Welcome");
        let ServerMsg::Welcome { player_id } = welcome else {
            panic!("premier message attendu : Welcome, reçu {welcome:?}");
        };

        // Le serveur relaie aussi le `Join` initial au thread principal (cf.
        // `AppState::spawn_network_player`, Sprint 55) : c'est le premier message
        // dans `inbox`, avant l'`Input` envoyé ci-dessous.
        let (join_id, join_msg) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le serveur doit recevoir le Join du client");
        assert_eq!(join_id, player_id);
        assert_eq!(
            join_msg,
            ClientMsg::Join {
                name: "Testeur".to_string(),
                firebase_uid: None,
            }
        );

        client.send(&ClientMsg::Input {
            move_x: 0.5,
            move_y: -1.0,
            attack: true,
            jump: false,
        });

        let (id, msg) = server
            .inbox
            .recv_timeout(Duration::from_secs(2))
            .expect("le serveur doit recevoir l'Input du client");
        assert_eq!(id, player_id);
        assert_eq!(
            msg,
            ClientMsg::Input {
                move_x: 0.5,
                move_y: -1.0,
                attack: true,
                jump: false,
            }
        );
    }

    /// Deux clients dans le même salon obtiennent des identifiants distincts, et un
    /// `broadcast` atteint les deux (préfigure la Snapshot diffusée à chaque tick,
    /// Sprint 55).
    #[test]
    fn broadcast_reaches_every_connected_client() {
        let server = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
        let url = format!("ws://{}", server.local_addr);

        let a = NetClient::connect(&url, "A", None).expect("connexion A");
        let b = NetClient::connect(&url, "B", None).expect("connexion B");

        let welcome_a = a.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        let welcome_b = b.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_ne!(
            welcome_a, welcome_b,
            "deux joueurs doivent avoir des id distincts"
        );

        // Laisse le temps aux deux connexions de s'enregistrer dans `outboxes`
        // avant le broadcast (asynchrone, pas garanti terminé au retour de `recv`).
        let mut waited = Duration::ZERO;
        while server.connected_count() < 2 && waited < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        assert_eq!(server.connected_count(), 2);

        server.broadcast(&ServerMsg::Event(protocol::GameEvent::WaveStart {
            wave: 1,
        }));

        for c in [&a, &b] {
            let msg = c.inbox.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(
                msg,
                ServerMsg::Event(protocol::GameEvent::WaveStart { wave: 1 })
            );
        }
    }
}
