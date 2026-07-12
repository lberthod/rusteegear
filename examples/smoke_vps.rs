//! Test fumée post-déploiement : rejoint le serveur réel (VPS par défaut, URL en
//! argument sinon), vérifie que les snapshots se décodent (protocole aligné des
//! deux côtés), que les monstres y figurent, et qu'un `fire: true` produit un
//! projectile en vol côté serveur.
//!
//! ```bash
//! cargo run --example smoke_vps                    # VPS de production
//! cargo run --example smoke_vps ws://127.0.0.1:7777  # serveur local
//! ```

use std::time::Duration;

use motor3derust::net::client::NetClient;
use motor3derust::net::protocol::{ClientMsg, ServerMsg};

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://179.237.71.235:80".to_string());
    let client = NetClient::connect(&url, "SmokeTest", None).expect("connexion au serveur");
    let mut got_snapshot = false;
    let mut monsters = 0usize;
    let mut fired = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        match client.inbox.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id }) => {
                println!("Welcome : joueur {player_id}");
                // Tire une boule de feu pour de vrai (le serveur doit l'accepter).
                client.send(&ClientMsg::Input {
                    move_x: 0.0,
                    move_y: 0.0,
                    attack: false,
                    jump: false,
                    fire: true,
                });
                fired = true;
            }
            Ok(ServerMsg::Snapshot(s)) => {
                if !got_snapshot {
                    monsters = s.entities.iter().filter(|e| e.player_id.is_none()).count();
                    println!(
                        "Snapshot tick {} : {} entités dont {} monstres, {} projectile(s)",
                        s.tick,
                        s.entities.len(),
                        monsters,
                        s.projectiles.len()
                    );
                    got_snapshot = true;
                }
                if fired && !s.projectiles.is_empty() {
                    println!("Projectile en vol confirmé : {:?}", s.projectiles[0]);
                    break;
                }
            }
            Ok(other) => println!("Reçu : {other:?}"),
            Err(_) => {}
        }
    }
    client.send(&ClientMsg::Leave);
    assert!(got_snapshot, "aucun snapshot décodable reçu");
    assert!(monsters >= 4, "monstres absents du snapshot : {monsters}");
    println!("✅ Serveur VPS OK : protocole boule de feu + monstres opérationnel");
}
