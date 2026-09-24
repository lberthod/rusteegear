//! Relais de Ball (VPS) : apparie **un** casque et **un** ordinateur et leur
//! fait suivre les messages de l'un à l'autre, sans les lire. Un nouveau venu
//! prend la place de l'ancien du même rôle. Toutes les 2 s, chacun reçoit
//! `Peer` (l'autre est-il là ?) : ça tient la connexion en vie même quand
//! personne d'autre ne parle. Lancé par `src/bin/ball_relay.rs`.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio_tungstenite::tungstenite::Message;

use super::net::{Msg, PROTOCOL_VERSION, Role};

struct Slot {
    id: u64,
    tx: UnboundedSender<Vec<u8>>,
}

#[derive(Default)]
struct Rooms {
    slots: [Option<Slot>; 2],
    next_id: u64,
}

fn index(role: Role) -> usize {
    match role {
        Role::Vr => 0,
        Role::Pc => 1,
    }
}

/// Accepte les connexions sur `listener` jusqu'à la fin des temps.
pub async fn serve(listener: TcpListener) {
    let rooms = Arc::new(Mutex::new(Rooms::default()));
    loop {
        let Ok((stream, addr)) = listener.accept().await else {
            continue;
        };
        let rooms = rooms.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, rooms).await {
                log::debug!("Ball relais : {addr} : {e}");
            }
        });
    }
}

async fn handle(stream: tokio::net::TcpStream, rooms: Arc<Mutex<Rooms>>) -> Result<(), String> {
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|e| e.to_string())?;
    let (mut sink, mut source) = ws.split();
    let first = tokio::time::timeout(Duration::from_secs(5), source.next())
        .await
        .map_err(|_| "pas de Hello".to_string())?;
    let role = match first {
        Some(Ok(Message::Binary(b))) => match Msg::decode(&b) {
            Some(Msg::Hello { role, version }) if version == PROTOCOL_VERSION => role,
            Some(Msg::Hello { version, .. }) => {
                let reason = format!(
                    "version {version}, le relais attend {PROTOCOL_VERSION} : mets le jeu à jour"
                );
                let _ = sink
                    .send(Message::Binary(Msg::Refused { reason }.encode().into()))
                    .await;
                return Err("version".into());
            }
            _ => return Err("premier message invalide".into()),
        },
        _ => return Err("premier message absent".into()),
    };
    let me = index(role);
    let (tx, mut rx) = unbounded_channel::<Vec<u8>>();
    let id = {
        let mut r = rooms.lock().await;
        r.next_id += 1;
        let id = r.next_id;
        // Le nouveau venu remplace l'ancien : sa file se ferme, sa connexion aussi.
        r.slots[me] = Some(Slot { id, tx: tx.clone() });
        notify(&r);
        id
    };
    log::info!("Ball relais : {role:?} connecté (#{id})");

    // Écriture : messages de l'autre + état du partenaire toutes les 2 s.
    let rooms_w = rooms.clone();
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
                    let present = rooms_w.lock().await.slots[1 - me].is_some();
                    let still_me = rooms_w.lock().await.slots[me].as_ref().is_some_and(|s| s.id == id);
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
    loop {
        let next = tokio::time::timeout(Duration::from_secs(20), source.next()).await;
        let bytes = match next {
            Ok(Some(Ok(Message::Binary(b)))) => b,
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => break,
            Ok(Some(Ok(_))) => continue,
        };
        let r = rooms.lock().await;
        if !r.slots[me].as_ref().is_some_and(|s| s.id == id) {
            break; // remplacé
        }
        if let Some(other) = &r.slots[1 - me] {
            let _ = other.tx.send(bytes.to_vec());
        }
    }
    {
        let mut r = rooms.lock().await;
        if r.slots[me].as_ref().is_some_and(|s| s.id == id) {
            r.slots[me] = None;
            notify(&r);
        }
    }
    writer.abort();
    log::info!("Ball relais : {role:?} parti (#{id})");
    Ok(())
}

/// Dit à chacun si l'autre est là.
fn notify(r: &Rooms) {
    for (i, slot) in r.slots.iter().enumerate() {
        if let Some(s) = slot {
            let present = r.slots[1 - i].is_some();
            let _ = s.tx.send(Msg::Peer { present }.encode());
        }
    }
}
