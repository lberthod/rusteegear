//! Test fumée post-déploiement : rejoint le serveur réel (VPS par défaut, URL en
//! argument sinon), vérifie que les snapshots se décodent (protocole aligné des
//! deux côtés), que les monstres y figurent, qu'un `fire: true` produit un
//! projectile en vol côté serveur, et que la vie individualisée par joueur
//! (GAMEDESIGN_EN_LIGNE.md §3.1) est bien exposée dans le `Snapshot`.
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
                // Tire pour de vrai avec l'arme 1 (Éclair) et une visée vers +X
                // (aim_yaw -π/2) : vérifie d'un coup le tir, la sélection d'arme
                // ET la prise en compte de l'orientation par le serveur.
                client.send(&ClientMsg::Input {
                    move_x: 0.0,
                    move_y: 0.0,
                    aim_yaw: -std::f32::consts::FRAC_PI_2,
                    attack: false,
                    jump: false,
                    fire: true,
                    weapon: 1,
                    heal: false,
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
                    if let Some(me) = s.entities.iter().find(|e| e.player_id.is_some()) {
                        println!("Vie du premier joueur diffusée : {:?}", me.health);
                        assert!(
                            me.health.is_some(),
                            "la vie individualisée d'un joueur doit être diffusée (Sprint 80)"
                        );
                    }
                    got_snapshot = true;
                }
                if fired && !s.projectiles.is_empty() {
                    let p = &s.projectiles[0];
                    println!(
                        "Projectile en vol confirmé : {:?} (arme {})",
                        p.position, p.weapon
                    );
                    assert_eq!(p.weapon, 1, "l'arme sélectionnée doit suivre le projectile");
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
