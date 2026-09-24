//! Ball à deux : le joueur VR (casque, maître de la partie : physique, score)
//! et un joueur sur ordinateur (`src/bin/ball_pc.rs`) qui se promène dans la
//! scène et pose des murs pour arrêter les tirs. Les deux se retrouvent par un
//! **relais** sur le VPS (`src/bin/ball_relay.rs`, `wss://ws.loicberthod.ch/ball`) :
//! une seule place « VR » et une seule place « PC » ; un nouveau venu prend la
//! place de l'ancien (une connexion morte ne bloque jamais la partie).
//!
//! Le casque joue seul tant que personne ne le rejoint — et sans réseau du
//! tout, rien ne change : la connexion est retentée en arrière-plan.
//!
//! Protocole : messages `bincode` dans des trames WebSocket binaires. Le PC ne
//! connaît rien des règles : il affiche ce que le casque lui décrit (boîtes et
//! sphères) et renvoie sa position et ses murs.

use serde::{Deserialize, Serialize};

/// Adresse du relais par défaut (`BALL_URL` pour en changer, ex.
/// `ws://127.0.0.1:7790/ball` en local).
pub const DEFAULT_URL: &str = "wss://ws.loicberthod.ch/ball";
/// Incrémenté à chaque changement de format des messages.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Vr,
    Pc,
}

/// Boîte à dessiner (centre, rotation, demi-tailles, couleur + mode, cf. `gfx`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NetBox {
    pub p: [f32; 3],
    pub q: [f32; 4],
    pub half: [f32; 3],
    pub color: [f32; 4],
}

/// Sphère à dessiner (boules, mains du joueur VR).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NetSphere {
    pub p: [f32; 3],
    pub r: f32,
    pub color: [f32; 4],
}

/// Ce que voit le joueur PC, décrit par le casque.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Repère du joueur VR (où il se tient) : le PC s'y place autour.
    pub origin: [f32; 3],
    /// Tête du joueur VR (position, orientation).
    pub head: ([f32; 3], [f32; 4]),
    pub boxes: Vec<NetBox>,
    pub spheres: Vec<NetSphere>,
    /// Lignes d'information (parcours, trou, coups, tirs arrêtés).
    pub hud: Vec<String>,
    /// Décor fixe, envoyé à chaque changement de lieu et toutes les secondes.
    pub statics: Option<Vec<NetBox>>,
}

/// Mur posé par le joueur PC : centre au sol et orientation (lacet).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NetWall {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
}

/// Ce que le PC renvoie au casque.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PcState {
    /// Position des pieds et lacet du joueur PC (monde).
    pub pos: [f32; 3],
    pub yaw: f32,
    pub walls: Vec<NetWall>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    /// Premier message d'un client au relais.
    Hello { role: Role, version: u32 },
    /// Relais → client : l'autre joueur est là (ou vient de partir).
    Peer { present: bool },
    /// Relais → client : version incompatible.
    Refused { reason: String },
    Snapshot(Snapshot),
    Pc(PcState),
}

impl Msg {
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

/// État de la liaison, pour l'affichage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    /// Pas (encore) de relais joignable : on joue seul.
    Offline,
    /// Connecté au relais, seul.
    Waiting,
    /// L'autre joueur est là.
    Paired,
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use native::Link;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod native {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    use super::{LinkStatus, Msg, PROTOCOL_VERSION, Role};

    /// Liaison au relais, entretenue par un thread de fond (reconnexion
    /// automatique toutes les 3 s). Jamais bloquante pour la boucle de jeu.
    pub struct Link {
        inbox: Mutex<Receiver<Msg>>,
        outbox: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
        status: Arc<AtomicU8>,
    }

    const OFFLINE: u8 = 0;
    const WAITING: u8 = 1;
    const PAIRED: u8 = 2;

    impl Link {
        pub fn start(url: &str, role: Role) -> Self {
            let (in_tx, in_rx) = channel();
            let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let status = Arc::new(AtomicU8::new(OFFLINE));
            let url = url.to_string();
            let st = status.clone();
            std::thread::Builder::new()
                .name("ball-link".into())
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("runtime tokio");
                    rt.block_on(run(url, role, in_tx, out_rx, st));
                })
                .expect("thread réseau");
            Self {
                inbox: Mutex::new(in_rx),
                outbox: out_tx,
                status,
            }
        }

        pub fn status(&self) -> LinkStatus {
            match self.status.load(Ordering::Relaxed) {
                PAIRED => LinkStatus::Paired,
                WAITING => LinkStatus::Waiting,
                _ => LinkStatus::Offline,
            }
        }

        /// Envoie un message (perdu sans erreur si la liaison est coupée).
        pub fn send(&self, msg: &Msg) {
            if self.status() != LinkStatus::Offline {
                let _ = self.outbox.send(msg.encode());
            }
        }

        /// Messages reçus depuis le dernier appel.
        pub fn poll(&self) -> Vec<Msg> {
            let Ok(rx) = self.inbox.lock() else {
                return Vec::new();
            };
            rx.try_iter().collect()
        }
    }

    async fn run(
        url: String,
        role: Role,
        inbox: Sender<Msg>,
        mut outbox: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
        status: Arc<AtomicU8>,
    ) {
        loop {
            match tokio::time::timeout(
                Duration::from_secs(8),
                tokio_tungstenite::connect_async(&url),
            )
            .await
            {
                Ok(Ok((ws, _))) => {
                    log::info!("Ball : relais {url} joint ({role:?})");
                    status.store(WAITING, Ordering::Relaxed);
                    let (mut tx, mut rx) = ws.split();
                    let hello = Msg::Hello {
                        role,
                        version: PROTOCOL_VERSION,
                    };
                    if tx.send(Message::Binary(hello.encode().into())).await.is_ok() {
                        // Vide ce qui s'est accumulé hors ligne (vieux états).
                        while outbox.try_recv().is_ok() {}
                        loop {
                            tokio::select! {
                                out = outbox.recv() => {
                                    let Some(bytes) = out else { return };
                                    if tx.send(Message::Binary(bytes.into())).await.is_err() {
                                        break;
                                    }
                                }
                                msg = tokio::time::timeout(Duration::from_secs(10), rx.next()) => {
                                    match msg {
                                        Ok(Some(Ok(Message::Binary(b)))) => {
                                            match Msg::decode(&b) {
                                                Some(Msg::Peer { present }) => {
                                                    status.store(if present { PAIRED } else { WAITING }, Ordering::Relaxed);
                                                }
                                                Some(Msg::Refused { reason }) => {
                                                    log::warn!("Ball : relais refusé : {reason}");
                                                    break;
                                                }
                                                Some(m) => {
                                                    if inbox.send(m).is_err() {
                                                        return;
                                                    }
                                                }
                                                None => {}
                                            }
                                        }
                                        Ok(Some(Ok(_))) => {}
                                        // Silence de 10 s, erreur ou fermeture : on recommence.
                                        _ => break,
                                    }
                                }
                            }
                        }
                    }
                    log::info!("Ball : relais perdu, nouvelle tentative");
                }
                Ok(Err(e)) => log::debug!("Ball : relais injoignable ({e})"),
                Err(_) => log::debug!("Ball : relais injoignable (délai dépassé)"),
            }
            status.store(OFFLINE, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_through_bincode() {
        let m = Msg::Pc(PcState {
            pos: [1.0, 0.0, -3.0],
            yaw: 0.5,
            walls: vec![NetWall {
                x: 0.2,
                z: -2.0,
                yaw: 1.0,
            }],
        });
        assert_eq!(Msg::decode(&m.encode()), Some(m));
    }
}
