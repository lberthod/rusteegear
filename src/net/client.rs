//! Transport WebSocket côté client (SPRINT_MMORPG.md, Sprint 53).
//!
//! Même schéma que `server_loop` : un thread de fond dédié pousse les
//! `ServerMsg` reçus dans `inbox` (canal `std::sync::mpsc`), et `send` encode
//! un `ClientMsg` vers le serveur. La boucle `winit` (Sprint 54+) n'a qu'à
//! `try_recv()` sur `inbox` une fois par frame, exactement comme elle le fait
//! déjà pour les imports glTF ou les réponses IA asynchrones.
//!
//! **Runtime `current_thread` (corrigé à l'audit du 2026-07-07, cf.
//! AUDIT_MMORPG.md §4.3)** : une connexion réseau n'a besoin que d'un thread
//! pour attendre les octets qui arrivent, pas d'un pool de threads ouvriers —
//! `tokio::runtime::Runtime::new()` (utilisé avant ce correctif) construit par
//! défaut un runtime **multi-thread** (un ouvrier par CPU logique). Un
//! `current_thread` n'a pas de thread ouvrier propre : il ne progresse que
//! pendant qu'un thread appelle `block_on` dessus — d'où le thread dédié
//! ci-dessous, qui `block_on` la boucle de vie entière de la connexion (pas
//! seulement la connexion initiale).

use std::sync::mpsc::{Receiver, channel};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::protocol::{self, ClientMsg, ServerMsg};

/// Connexion réseau côté client à un salon RusteeGear.
pub struct NetClient {
    /// Messages reçus du serveur, à consommer par la boucle de jeu (non bloquant :
    /// `try_recv` une fois par frame).
    pub inbox: Receiver<ServerMsg>,
    outbox: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

impl NetClient {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) et envoie immédiatement un
    /// `ClientMsg::Join`. Bloquant le temps de la connexion TCP/WebSocket initiale
    /// (raisonnable au lancement/join d'un salon, pas dans la boucle de rendu).
    /// `firebase_uid` : `uid` obtenu par `net::firebase::sign_in`/`sign_up`, si le
    /// joueur s'est connecté avant de rejoindre (cf. Sprint 57) ; `None` pour une
    /// partie locale/anonyme.
    pub fn connect(
        url: &str,
        name: &str,
        firebase_uid: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (in_tx, in_rx) = channel::<ServerMsg>();
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let join = protocol::encode(&ClientMsg::Join {
            name: name.to_string(),
            firebase_uid: firebase_uid.map(str::to_string),
        })?;
        // Mis en file avant même que le thread de fond n'existe : la pompe
        // sortante le trouvera prêt dès sa première itération.
        out_tx.send(join)?;

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let url = url.to_string();
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            runtime.block_on(async move {
                let (ws, _response) = match tokio_tungstenite::connect_async(&url).await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let (mut sink, mut stream) = ws.split();

                let outbound = async {
                    while let Some(bytes) = out_rx.recv().await {
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                };
                let inbound = async {
                    while let Some(Ok(msg)) = stream.next().await {
                        if let Message::Binary(bytes) = msg
                            && let Ok(server_msg) = protocol::decode::<ServerMsg>(&bytes)
                            && in_tx.send(server_msg).is_err()
                        {
                            break;
                        }
                    }
                };
                tokio::select! {
                    _ = outbound => {}
                    _ = inbound => {}
                }
            });
            // Le thread se termine naturellement ici une fois la connexion
            // close : pas de nettoyage explicite à faire, `out_tx` (côté
            // `NetClient`) est la seule source de `out_rx`, qui se ferme
            // d'elle-même quand `NetClient` est droppé.
        });

        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => return Err("le thread réseau s'est arrêté avant la connexion".into()),
        }

        Ok(Self {
            inbox: in_rx,
            outbox: out_tx,
        })
    }

    /// Envoie un message au serveur (non bloquant : mis en file, transmis par le
    /// thread réseau de fond).
    pub fn send(&self, msg: &ClientMsg) {
        if let Ok(bytes) = protocol::encode(msg) {
            let _ = self.outbox.send(bytes);
        }
    }
}
