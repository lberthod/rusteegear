//! Relais de Ball (VPS) : dans chaque **salon** (code affiché par le casque),
//! apparie un casque et un ordinateur et leur fait suivre les messages de
//! l'un à l'autre, sans les lire. Un nouveau venu prend la place de l'ancien
//! du même rôle **dans son salon**. Toutes les 2 s, chacun reçoit `Peer`
//! (l'autre est-il là ?) : ça tient la connexion en vie même quand personne
//! d'autre ne parle. Lancé par `src/bin/ball_relay.rs`.
//!
//! Limites (audit 2026-10-20, 1.7) : message de 256 Kio au plus, 120
//! messages/s par connexion, file d'envoi bornée (les messages en trop sont
//! jetés : un état plus récent suit), 64 connexions et 32 salons au plus.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Sender, channel};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::net::{LEGACY_VERSION, Msg, PROTOCOL_VERSION, Role, valid_room};

/// Taille maximale d'un message : le décor complet d'un parcours (quelques
/// centaines de boîtes de 56 octets) tient très largement dedans.
const MAX_MESSAGE_BYTES: usize = 256 * 1024;
/// Messages par seconde et par connexion : le casque en envoie 20, le PC 30.
const MAX_MESSAGES_PER_SEC: u32 = 120;
/// Messages en attente d'envoi vers un client.
const OUTBOX_CAPACITY: usize = 64;
/// Connexions simultanées, tous salons confondus.
const MAX_CONNECTIONS: usize = 64;
/// Salons ouverts simultanément.
const MAX_ROOMS: usize = 32;

struct Slot {
    id: u64,
    tx: Sender<Vec<u8>>,
}

#[derive(Default)]
struct Room {
    slots: [Option<Slot>; 2],
}

#[derive(Default)]
struct Rooms {
    rooms: HashMap<String, Room>,
    next_id: u64,
    connections: usize,
}

fn index(role: Role) -> usize {
    match role {
        Role::Vr => 0,
        Role::Pc => 1,
    }
}

fn ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
}

/// Libère le compteur de connexions à la fin d'une connexion, quelle qu'en
/// soit l'issue.
struct ConnectionGuard(Arc<Mutex<Rooms>>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let rooms = self.0.clone();
        tokio::spawn(async move {
            let mut r = rooms.lock().await;
            r.connections = r.connections.saturating_sub(1);
        });
    }
}

/// Accepte les connexions sur `listener` jusqu'à la fin des temps.
pub async fn serve(listener: TcpListener) {
    let rooms = Arc::new(Mutex::new(Rooms::default()));
    loop {
        let Ok((stream, addr)) = listener.accept().await else {
            continue;
        };
        {
            let mut r = rooms.lock().await;
            if r.connections >= MAX_CONNECTIONS {
                log::warn!("Ball relais : {addr} refusé, {MAX_CONNECTIONS} connexions ouvertes");
                continue;
            }
            r.connections += 1;
        }
        let guard = ConnectionGuard(rooms.clone());
        let rooms = rooms.clone();
        tokio::spawn(async move {
            let _guard = guard;
            if let Err(e) = handle(stream, rooms).await {
                log::debug!("Ball relais : {addr} : {e}");
            }
        });
    }
}

/// Rôle et salon annoncés par le premier message, ou la raison du refus.
fn admit(msg: Option<Msg>) -> Result<(Role, String), String> {
    match msg {
        Some(Msg::Join {
            role,
            version,
            room,
        }) if version == PROTOCOL_VERSION => {
            if valid_room(&room) {
                Ok((role, room))
            } else {
                Err("code de salon invalide (1 à 12 chiffres ou lettres)".into())
            }
        }
        // Casque d'avant les salons : salon public.
        Some(Msg::Hello { role, version }) if version == LEGACY_VERSION => {
            Ok((role, String::new()))
        }
        Some(Msg::Join { version, .. } | Msg::Hello { version, .. }) => Err(format!(
            "version {version}, le relais attend {PROTOCOL_VERSION} : mets le jeu à jour"
        )),
        _ => Err("premier message invalide".into()),
    }
}

async fn handle(stream: tokio::net::TcpStream, rooms: Arc<Mutex<Rooms>>) -> Result<(), String> {
    let ws = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::accept_async_with_config(stream, Some(ws_config())),
    )
    .await
    .map_err(|_| "poignée de main trop lente".to_string())?
    .map_err(|e| e.to_string())?;
    let (mut sink, mut source) = ws.split();
    let first = tokio::time::timeout(Duration::from_secs(5), source.next())
        .await
        .map_err(|_| "pas de Hello".to_string())?;
    let first = match first {
        Some(Ok(Message::Binary(b))) => Msg::decode(&b),
        _ => return Err("premier message absent".into()),
    };
    let (role, code) = match admit(first) {
        Ok(admitted) => admitted,
        Err(reason) => {
            let _ = sink
                .send(Message::Binary(
                    Msg::Refused {
                        reason: reason.clone(),
                    }
                    .encode()
                    .into(),
                ))
                .await;
            return Err(reason);
        }
    };
    let me = index(role);
    let (tx, mut rx) = channel::<Vec<u8>>(OUTBOX_CAPACITY);
    let id = {
        let mut r = rooms.lock().await;
        if !r.rooms.contains_key(&code) && r.rooms.len() >= MAX_ROOMS {
            drop(r);
            let reason = "relais plein, réessaie dans un moment".to_string();
            let _ = sink
                .send(Message::Binary(
                    Msg::Refused {
                        reason: reason.clone(),
                    }
                    .encode()
                    .into(),
                ))
                .await;
            return Err(reason);
        }
        r.next_id += 1;
        let id = r.next_id;
        let room = r.rooms.entry(code.clone()).or_default();
        // Le nouveau venu remplace l'ancien : sa file se ferme, sa connexion aussi.
        room.slots[me] = Some(Slot { id, tx: tx.clone() });
        notify(room);
        id
    };
    log::info!("Ball relais : {role:?} connecté (#{id}, salon « {code} »)");

    // Écriture : messages de l'autre + état du partenaire toutes les 2 s.
    let rooms_w = rooms.clone();
    let code_w = code.clone();
    let writer = tokio::spawn(async move {
        let mut beat = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                out = rx.recv() => {
                    let Some(bytes) = out else { break };
                    if sink.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                _ = beat.tick() => {
                    let (still_me, present) = {
                        let r = rooms_w.lock().await;
                        match r.rooms.get(&code_w) {
                            Some(room) => (
                                room.slots[me].as_ref().is_some_and(|s| s.id == id),
                                room.slots[1 - me].is_some(),
                            ),
                            None => (false, false),
                        }
                    };
                    if !still_me {
                        break;
                    }
                    let msg = Msg::Peer { present }.encode();
                    if sink.send(Message::Binary(msg.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = sink.close().await;
    });
    drop(tx);

    // Lecture : tout ce qui arrive part chez l'autre, tel quel.
    let mut window = Instant::now();
    let mut count = 0u32;
    loop {
        let next = tokio::time::timeout(Duration::from_secs(20), source.next()).await;
        let bytes = match next {
            Ok(Some(Ok(Message::Binary(b)))) => b,
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => break,
            Ok(Some(Ok(_))) => continue,
        };
        if window.elapsed() >= Duration::from_secs(1) {
            window = Instant::now();
            count = 0;
        }
        count += 1;
        if count > MAX_MESSAGES_PER_SEC {
            log::warn!("Ball relais : #{id} coupé, plus de {MAX_MESSAGES_PER_SEC} messages/s");
            break;
        }
        let r = rooms.lock().await;
        let Some(room) = r.rooms.get(&code) else {
            break;
        };
        if !room.slots[me].as_ref().is_some_and(|s| s.id == id) {
            break; // remplacé
        }
        if let Some(other) = &room.slots[1 - me] {
            // File pleine : l'autre lit mal, ce message est perdu (un plus
            // récent suit à 20-30 par seconde).
            let _ = other.tx.try_send(bytes.to_vec());
        }
    }
    {
        let mut r = rooms.lock().await;
        if let Some(room) = r.rooms.get_mut(&code)
            && room.slots[me].as_ref().is_some_and(|s| s.id == id)
        {
            room.slots[me] = None;
            notify(room);
            if room.slots.iter().all(Option::is_none) {
                r.rooms.remove(&code);
            }
        }
    }
    writer.abort();
    log::info!("Ball relais : {role:?} parti (#{id}, salon « {code} »)");
    Ok(())
}

/// Dit à chacun du salon si l'autre est là.
fn notify(room: &Room) {
    for (i, slot) in room.slots.iter().enumerate() {
        if let Some(s) = slot {
            let present = room.slots[1 - i].is_some();
            let _ = s.tx.try_send(Msg::Peer { present }.encode());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_and_legacy_headsets_are_admitted() {
        let join = |room: &str, version| {
            admit(Some(Msg::Join {
                role: Role::Pc,
                version,
                room: room.into(),
            }))
        };
        assert_eq!(
            join("4821", PROTOCOL_VERSION),
            Ok((Role::Pc, "4821".into()))
        );
        assert!(join("", PROTOCOL_VERSION).is_err());
        assert!(join("4821", 99).is_err());
        assert_eq!(
            admit(Some(Msg::Hello {
                role: Role::Vr,
                version: LEGACY_VERSION
            })),
            Ok((Role::Vr, String::new()))
        );
        assert!(admit(Some(Msg::Peer { present: true })).is_err());
        assert!(admit(None).is_err());
    }

    /// Bout en bout sur une vraie socket : deux salons ne se voient pas, et
    /// un casque et un PC du même salon se trouvent. Sockets réelles : derrière
    /// `net_tests`, comme les autres tests réseau.
    #[cfg(feature = "net_tests")]
    #[test]
    fn players_only_meet_inside_their_room() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime de test");
        rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let url = format!("ws://{}/ball", listener.local_addr().expect("adresse"));
            tokio::spawn(serve(listener));
            let join = |role, room: &'static str| {
                let url = url.clone();
                async move {
                    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
                        .await
                        .expect("connexion");
                    let hello = Msg::Join {
                        role,
                        version: PROTOCOL_VERSION,
                        room: room.into(),
                    };
                    ws.send(Message::Binary(hello.encode().into()))
                        .await
                        .expect("Join");
                    ws
                }
            };
            // Attend un `Peer` qui vaut `want` (3 s au plus) : le relais en
            // envoie un à l'arrivée, puis toutes les 2 s.
            async fn peer_becomes(
                ws: &mut tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                want: bool,
            ) -> bool {
                let wait = async {
                    while let Some(Ok(msg)) = ws.next().await {
                        if let Message::Binary(b) = msg
                            && Msg::decode(&b) == Some(Msg::Peer { present: want })
                        {
                            return true;
                        }
                    }
                    false
                };
                tokio::time::timeout(Duration::from_secs(3), wait)
                    .await
                    .unwrap_or(false)
            }
            let mut vr = join(Role::Vr, "1111").await;
            assert!(peer_becomes(&mut vr, false).await, "seul dans son salon");
            let mut intruder = join(Role::Pc, "2222").await;
            assert!(
                !peer_becomes(&mut intruder, true).await,
                "un autre salon ne voit pas le casque"
            );
            let mut pc = join(Role::Pc, "1111").await;
            assert!(
                peer_becomes(&mut pc, true).await,
                "le PC du même salon trouve le casque"
            );
            assert!(peer_becomes(&mut vr, true).await, "le casque est prévenu");
        });
    }
}
