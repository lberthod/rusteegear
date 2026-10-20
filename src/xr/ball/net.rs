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
//!
//! **Salons** (audit 2026-10-20, 1.7) : le casque tire un code à 4 chiffres
//! (gardé d'une session à l'autre) et l'affiche ; le joueur PC le tape pour le
//! rejoindre. Avant, un salon unique et public laissait n'importe qui éjecter
//! le joueur VR. Les casques d'avant les salons (`Hello`, version 1) jouent
//! toujours, dans le salon public `""`.

use serde::{Deserialize, Serialize};

/// Adresses du relais essayées tour à tour (`BALL_URL` pour n'en viser
/// qu'une, ex. `ws://127.0.0.1:7790/ball` en local) : chiffrée d'abord, puis
/// en clair sur le port 80 **seulement si l'échec est celui du TLS** (repli
/// ajouté après le 1er essai au casque : « relais injoignable ») — un échec
/// DNS ou réseau ne se règle pas en clair, et une connexion en clair est
/// signalée au joueur (`Link::encrypted`).
pub const DEFAULT_URLS: [&str; 2] = [
    "wss://ws.loicberthod.ch/ball",
    "ws://ws.loicberthod.ch/ball",
];
/// Incrémenté à chaque changement de format des messages.
pub const PROTOCOL_VERSION: u32 = 2;

/// Version des clients d'avant les salons, qui envoient `Hello` : acceptés
/// dans le salon public (`""`), les autres messages n'ont pas changé.
pub const LEGACY_VERSION: u32 = 1;

/// Code de salon valide : 1 à 12 chiffres ou lettres ASCII.
pub fn valid_room(room: &str) -> bool {
    (1..=12).contains(&room.len()) && room.chars().all(|c| c.is_ascii_alphanumeric())
}

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
    Hello {
        role: Role,
        version: u32,
    },
    /// Relais → client : l'autre joueur est là (ou vient de partir).
    Peer {
        present: bool,
    },
    /// Relais → client : version incompatible.
    Refused {
        reason: String,
    },
    Snapshot(Snapshot),
    Pc(PcState),
    /// Premier message d'un client de version ≥ 2 : rôle et salon. Ajouté en
    /// **dernier** variant pour que l'encodage des précédents ne change pas
    /// (les casques en version 1 restent compris).
    Join {
        role: Role,
        version: u32,
        room: String,
    },
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
        /// Raison du dernier échec de connexion (affichée au joueur : c'est
        /// le seul diagnostic possible sur un casque sans câble).
        last_error: Arc<Mutex<String>>,
        /// Connexion courante chiffrée (`wss://`) ; `false` après un repli `ws://`.
        encrypted: Arc<std::sync::atomic::AtomicBool>,
    }

    const OFFLINE: u8 = 0;
    const WAITING: u8 = 1;
    const PAIRED: u8 = 2;

    impl Link {
        pub fn start(urls: &[String], role: Role, room: &str) -> Self {
            let (in_tx, in_rx) = channel();
            let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let status = Arc::new(AtomicU8::new(OFFLINE));
            let last_error = Arc::new(Mutex::new(String::from("connexion en cours…")));
            let encrypted = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let urls = urls.to_vec();
            let room = room.to_string();
            let st = status.clone();
            let err = last_error.clone();
            let enc = encrypted.clone();
            // Sans thread ni runtime, la liaison reste « hors ligne » et la
            // cause s'affiche dans le casque (`last_error`), comme un échec réseau.
            let spawned = std::thread::Builder::new()
                .name("ball-link".into())
                .spawn(move || {
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => rt.block_on(run(Session {
                            urls,
                            role,
                            room,
                            inbox: in_tx,
                            outbox: out_rx,
                            status: st,
                            last_error: err,
                            encrypted: enc,
                        })),
                        Err(e) => set_error(&err, format!("runtime réseau : {e}")),
                    }
                });
            if let Err(e) = spawned {
                set_error(&last_error, format!("thread réseau : {e}"));
            }
            Self {
                inbox: Mutex::new(in_rx),
                outbox: out_tx,
                status,
                last_error,
                encrypted,
            }
        }

        /// `false` si la liaison passe par le repli en clair (`ws://`).
        pub fn encrypted(&self) -> bool {
            self.encrypted.load(Ordering::Relaxed)
        }

        /// Pourquoi la dernière tentative a échoué (vide une fois connecté).
        pub fn last_error(&self) -> String {
            self.last_error
                .lock()
                .map(|e| e.clone())
                .unwrap_or_default()
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

    /// Erreur de connexion en clair, avec sa cause profonde (DNS, TLS…).
    /// Échec avant toute tentative de connexion (thread ou runtime).
    fn set_error(last_error: &Mutex<String>, e: String) {
        log::error!("Ball : {e}");
        if let Ok(mut le) = last_error.lock() {
            *le = e;
        }
    }

    fn describe(e: &tokio_tungstenite::tungstenite::Error) -> String {
        use std::error::Error as _;
        let mut text = e.to_string();
        let mut source = e.source();
        while let Some(s) = source {
            text.push_str(" ← ");
            text.push_str(&s.to_string());
            source = s.source();
        }
        text
    }

    /// Tout ce dont la boucle de connexion a besoin.
    struct Session {
        urls: Vec<String>,
        role: Role,
        room: String,
        inbox: Sender<Msg>,
        outbox: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
        status: Arc<AtomicU8>,
        last_error: Arc<Mutex<String>>,
        encrypted: Arc<std::sync::atomic::AtomicBool>,
    }

    /// Échec qui tient au TLS lui-même (certificat, poignée de main) : le seul
    /// cas où retenter en clair a un sens.
    pub(super) fn looks_like_tls_failure(error: &str) -> bool {
        let e = error.to_ascii_lowercase();
        [
            "tls",
            "certificate",
            "handshake",
            "alert",
            "unknownissuer",
            "badsignature",
        ]
        .iter()
        .any(|k| e.contains(k))
    }

    async fn run(session: Session) {
        let Session {
            urls,
            role,
            room,
            inbox,
            mut outbox,
            status,
            last_error,
            encrypted,
        } = session;
        // Dernier échec d'une adresse chiffrée dû au TLS : autorise le repli `ws://`.
        let mut tls_failed = false;
        let set_error = |url: &str, e: String| {
            if !e.is_empty() {
                log::warn!("Ball : relais {url} : {e}");
            }
            if let Ok(mut le) = last_error.lock() {
                *le = if e.is_empty() {
                    e
                } else {
                    format!("{url} : {e}")
                };
            }
        };
        let mut attempt = 0usize;
        loop {
            let url = urls[attempt % urls.len().max(1)].clone();
            attempt += 1;
            let plain = url.starts_with("ws://");
            // Repli en clair seulement après un échec TLS d'une adresse chiffrée
            // (et toujours si `BALL_URL` ne donne qu'une adresse en clair, en local).
            if plain && urls.len() > 1 && !tls_failed {
                if attempt.is_multiple_of(urls.len()) {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                continue;
            }
            match tokio::time::timeout(
                Duration::from_secs(8),
                tokio_tungstenite::connect_async(&url),
            )
            .await
            {
                Ok(Ok((ws, _))) => {
                    log::info!("Ball : relais {url} joint ({role:?}, salon {room})");
                    status.store(WAITING, Ordering::Relaxed);
                    encrypted.store(!plain, Ordering::Relaxed);
                    set_error(&url, String::new());
                    let (mut tx, mut rx) = ws.split();
                    let hello = Msg::Join {
                        role,
                        version: PROTOCOL_VERSION,
                        room: room.clone(),
                    };
                    if tx
                        .send(Message::Binary(hello.encode().into()))
                        .await
                        .is_ok()
                    {
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
                                                    set_error(&url, format!("refusé : {reason}"));
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
                    set_error(&url, "connexion perdue".into());
                }
                Ok(Err(e)) => {
                    let text = describe(&e);
                    if !plain {
                        tls_failed = looks_like_tls_failure(&text);
                    }
                    set_error(&url, text);
                }
                Err(_) => set_error(&url, "délai dépassé (8 s) : réseau lent ou bloqué".into()),
            }
            status.store(OFFLINE, Ordering::Relaxed);
            // Adresse suivante tout de suite ; pause une fois la liste parcourue.
            if attempt.is_multiple_of(urls.len().max(1)) {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
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

    /// Les casques d'avant les salons (version 1, `Hello`) restent compris :
    /// les variants existants gardent leur encodage, `Join` vient en dernier.
    #[test]
    fn legacy_hello_still_decodes_and_join_is_appended() {
        let hello = Msg::Hello {
            role: Role::Vr,
            version: LEGACY_VERSION,
        };
        // Octets d'un casque v1 : tag 0 (`Hello`), tag 0 (`Vr`), version 1.
        assert_eq!(hello.encode(), [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        let join = Msg::Join {
            role: Role::Pc,
            version: PROTOCOL_VERSION,
            room: "4821".into(),
        };
        assert_eq!(join.encode()[..4], [5, 0, 0, 0]);
        assert_eq!(Msg::decode(&join.encode()), Some(join));
    }

    #[test]
    fn room_codes_are_short_and_alphanumeric() {
        assert!(valid_room("4821"));
        assert!(valid_room("abc123"));
        assert!(!valid_room(""));
        assert!(!valid_room("13 chiffres!!"));
        assert!(!valid_room("0123456789012"));
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    #[test]
    fn only_tls_failures_allow_the_plain_fallback() {
        use super::native::looks_like_tls_failure;
        assert!(looks_like_tls_failure(
            "TLS error: invalid peer certificate: UnknownIssuer"
        ));
        assert!(looks_like_tls_failure(
            "io error: received fatal alert: HandshakeFailure"
        ));
        assert!(!looks_like_tls_failure(
            "IO error: failed to lookup address information"
        ));
        assert!(!looks_like_tls_failure("délai dépassé (8 s)"));
    }
}
