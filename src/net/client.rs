//! Transport WebSocket côté client (SPRINT_MMORPG.md, Sprint 53).
//!
//! Même schéma que `server_loop` : un thread de fond avec son propre runtime
//! tokio pousse les `ServerMsg` reçus dans `inbox` (canal `std::sync::mpsc`), et
//! `send` encode un `ClientMsg` vers le serveur. La boucle `winit` (Sprint 54+)
//! n'a qu'à `try_recv()` sur `inbox` une fois par frame, exactement comme elle le
//! fait déjà pour les imports glTF ou les réponses IA asynchrones.

use std::sync::mpsc::{Receiver, channel};

use futures_util::{SinkExt, StreamExt};
use tokio::runtime::Runtime;
use tokio_tungstenite::tungstenite::Message;

use super::protocol::{self, ClientMsg, ServerMsg};

/// Connexion réseau côté client à un salon RusteeGear.
pub struct NetClient {
    /// Messages reçus du serveur, à consommer par la boucle de jeu (non bloquant :
    /// `try_recv` une fois par frame).
    pub inbox: Receiver<ServerMsg>,
    outbox: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    _runtime: Runtime,
}

impl NetClient {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) et envoie immédiatement un
    /// `ClientMsg::Join`. Bloquant le temps de la connexion TCP/WebSocket initiale
    /// (raisonnable au lancement/join d'un salon, pas dans la boucle de rendu).
    pub fn connect(url: &str, name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = Runtime::new()?;
        let (ws, _response) = runtime.block_on(tokio_tungstenite::connect_async(url))?;
        let (mut sink, mut stream) = ws.split();

        let (in_tx, in_rx) = channel::<ServerMsg>();
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let join = protocol::encode(&ClientMsg::Join {
            name: name.to_string(),
        })?;
        out_tx.send(join)?;

        runtime.spawn(async move {
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

        Ok(Self {
            inbox: in_rx,
            outbox: out_tx,
            _runtime: runtime,
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
