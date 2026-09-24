//! Sonde minimale (diagnostic) : rejoint « riviere » en prod et envoie un
//! Input move_x=1,move_y=0 constant pendant 12s, loggant position/tick.
use std::time::{Duration, Instant};
use motor3derust::net::client::NetClient;
use motor3derust::net::protocol::{ClientMsg, RIVIERE_LOBBY, ServerMsg};

fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| "wss://ws.loicberthod.ch".to_string());
    let client = NetClient::connect_to_lobby(&url, "SondeWalk", None, RIVIERE_LOBBY, 0, 0)
        .expect("connexion");
    client.wait_ready(Duration::from_secs(8)).expect("handshake");
    let mut my_id = None;
    let deadline = Instant::now() + Duration::from_secs(8);
    while my_id.is_none() && Instant::now() < deadline {
        if let Ok(ServerMsg::Welcome { player_id }) = client.inbox.recv_timeout(Duration::from_millis(500)) {
            my_id = Some(player_id);
        }
    }
    let my_id = my_id.expect("pas de welcome");
    println!("id={my_id}");
    let start = Instant::now();
    let mut last_print = Instant::now();
    let run_deadline = start + Duration::from_secs(12);
    let mut n = 0u32;
    while Instant::now() < run_deadline {
        client.send(&ClientMsg::Input {
            move_x: 1.0, move_y: 0.0, aim_yaw: 0.0, attack: false, jump: false,
            fire: false, weapon: 0, heal: false, block: false, dash: false,
        });
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(50)) {
            n += 1;
            if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                if last_print.elapsed() > Duration::from_millis(400) {
                    println!("t={:.2}s tick#{} pos={:?}", start.elapsed().as_secs_f32(), n, e.position);
                    last_print = Instant::now();
                }
            }
        }
    }
    client.send(&ClientMsg::Leave);
    println!("done");
}
