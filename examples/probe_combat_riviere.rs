//! Sonde headless de combat pour le salon « riviere » (technique documentée
//! dans README-water.md, §"sonde headless à deux clients") : rejoint le vrai
//! serveur de production via `NetClient::connect_to_lobby`, sans winit ni
//! rendu, et observe mécaniquement — directement dans les `Snapshot` reçus —
//! ce qu'un joueur PvE réel vivrait : mise à mort d'un monstre du bestiaire,
//! présence/PV des deux boss, prise en compte des capacités réseau
//! (bouclier/ruée) et latence réelle mesurée (Ping/Pong transport +
//! réflexion d'un déplacement dans le prochain `Snapshot`).
//!
//! N'écrit rien de définitif : un client jetable de plus dans le salon
//! `riviere` en production, se déconnecte proprement (`ClientMsg::Leave`) à
//! la fin.
//!
//! Usage : `cargo run --release --example probe_combat_riviere -- wss://ws.loicberthod.ch`
//! (défaut : serveur de production, salon `riviere`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use motor3derust::net::client::NetClient;
use motor3derust::net::protocol::{ClientMsg, RIVIERE_LOBBY, ServerMsg};

const ATTACK_RANGE: f32 = 1.2; // NETWORK_ATTACK_RANGE, src/app/multiplayer.rs
const APPROACH_MARGIN: f32 = 0.9; // s'arrête un peu en-deçà de la portée d'attaque
const RESPAWN_DELAY_S: f32 = 20.0; // bestiaire normal, riviere.rs:1132

// --- Reconstitution du centre de rivière (src/scene/demos/riviere.rs) -----
// `river_center`/les constantes Z_LIP/POOL_Z sont `pub(crate)`/privées à ce
// module — inaccessibles depuis un exemple (crate séparée). On recopie ici
// uniquement la formule, pour situer approximativement le Renard enragé 1
// (poste documenté : x = river_center(-20.0) + 7.0, z = -20.0) et guider le
// choix de cible si plusieurs monstres sont à portée égale du spawn.
const Z_LIP: f32 = -30.0;
const POOL_Z: f32 = -24.5;
fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn river_center(z: f32) -> f32 {
    let lower = 3.0 * ((z - POOL_Z) * 0.075).sin() + 1.2 * ((z - POOL_Z) * 0.21 + 1.0).sin();
    let upper = 1.5 * ((z - Z_LIP) * 0.12).sin();
    let t = smoothstep(Z_LIP - 1.5, Z_LIP + 1.5, z);
    upper * (1.0 - t) + lower * t
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dz = a[2] - b[2];
    dx * dx + dz * dz
}

/// Distance 3D pleine (X/Y/Z) — celle que le serveur utilise réellement pour
/// résoudre un coup de mêlée réseau (`Scene::attack_at`/`nearest_attackable`,
/// `(o.transform.position - p).length() - enemy_r <= NETWORK_ATTACK_RANGE`,
/// cf. `src/scene/queries.rs`). Diagnostic du 15 septembre 2026 (soir) : la
/// sonde ne comparait jusqu'ici que X/Z (`dist2`/calculs ad hoc ci-dessous)
/// pour décider « je suis à portée de mêlée » — sur terrain plat (Renard
/// enragé 1) l'écart d'altitude est négligeable et ça ne se voyait pas, mais
/// contre un monstre posté en sous-bois accidenté (Champignon mordeur,
/// entité 137, testé en production : dy mesuré ≈ 1,06 m entre joueur et
/// cible une fois « à portée » selon le seul X/Z) l'écart vertical à lui
/// seul dépasse `NETWORK_ATTACK_RANGE` (1,2 m) une fois combiné à la
/// composante horizontale — la sonde martelait alors `attack: true` à une
/// distance 3D réelle bien supérieure à la portée serveur, d'où 0 coup
/// compté malgré 15 s d'acharnement. Reproduit et confirmé localement
/// (`examples/probe_local_repro.rs`, `examples/probe_direct_hit_test.rs`) :
/// téléporté à 0.5 m 3D réels de la même cible, le premier coup porte
/// instantanément — `attack_at` fonctionne, seule la mesure de portée de la
/// sonde était fautive.
fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn elapsed(start: Instant) -> f32 {
    start.elapsed().as_secs_f32()
}

/// Envoie un `Input` neutre (ou orienté) et avance `next_input` de 50 ms —
/// cadence documentée dans README-water.md pour la sonde à deux clients.
#[allow(clippy::too_many_arguments)]
fn send_input(
    client: &NetClient,
    move_x: f32,
    move_y: f32,
    aim_yaw: f32,
    attack: bool,
    block: bool,
    dash: bool,
) {
    client.send(&ClientMsg::Input {
        move_x,
        move_y,
        aim_yaw,
        attack,
        jump: false,
        fire: false,
        weapon: 0,
        heal: false,
        block,
        dash,
    });
}

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "wss://ws.loicberthod.ch".to_string());
    println!("== Sonde combat Rivière — {url} (salon « {RIVIERE_LOBBY} ») ==");

    let client = NetClient::connect_to_lobby(&url, "SondeCombat", None, RIVIERE_LOBBY, 0, 0)
        .expect("connexion au serveur");
    client
        .wait_ready(Duration::from_secs(8))
        .expect("poignée de main avec le serveur");

    let start = Instant::now();
    let mut my_id = None;
    let mut my_pos = None;
    let mut kills_before: Option<u32> = None;

    // Boucle de réception initiale : Welcome + premier Snapshot complet.
    let deadline = Instant::now() + Duration::from_secs(8);
    while (my_id.is_none() || my_pos.is_none()) && Instant::now() < deadline {
        match client.inbox.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id }) => {
                my_id = Some(player_id);
                println!("[{:>6.2}s] Welcome : joueur {player_id}", elapsed(start));
            }
            Ok(ServerMsg::Snapshot(s)) => {
                if let Some(id) = my_id
                    && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(id))
                {
                    my_pos = Some(e.position);
                    kills_before = e.kills;
                }
            }
            _ => {}
        }
    }
    let my_id = my_id.expect("pas de Welcome reçu");
    let my_pos = my_pos.expect("pas de position initiale reçue pour notre joueur");
    println!(
        "[{:>6.2}s] Position de départ : {my_pos:?} (repère : Renard enragé 1 attendu vers x≈{:.1}, z=-20.0)",
        elapsed(start),
        river_center(-20.0) + 7.0
    );

    // -----------------------------------------------------------------
    // 0. Latence réelle : RTT transport (Ping/Pong) sur plusieurs échantillons,
    //    indépendant de tout rendu — répond à la question "378 ms vu dans
    //    l'UI, est-ce réseau normal ou souci serveur ?"
    // -----------------------------------------------------------------
    println!("\n-- 0. Latence transport (Ping/Pong) --");
    let mut ping_sent: HashMap<u64, Instant> = HashMap::new();
    let mut rtts: Vec<Duration> = Vec::new();
    for i in 0..8u64 {
        let t = start.elapsed().as_millis() as u64 + i; // clé quasi-unique
        ping_sent.insert(t, Instant::now());
        client.send(&ClientMsg::Ping { t });
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            match client.inbox.recv_timeout(Duration::from_millis(50)) {
                Ok(ServerMsg::Pong { t: pt }) => {
                    if let Some(sent) = ping_sent.remove(&pt) {
                        rtts.push(sent.elapsed());
                    }
                }
                Ok(ServerMsg::Snapshot(s)) => {
                    if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                        // garde la position à jour pendant qu'on mesure
                        let _ = e.position;
                    }
                }
                _ => {}
            }
        }
    }
    if rtts.is_empty() {
        println!("  ⚠️  Aucun Pong reçu — impossible de mesurer le RTT transport.");
    } else {
        let n = rtts.len();
        let sum: Duration = rtts.iter().sum();
        let avg = sum / n as u32;
        let min = *rtts.iter().min().unwrap();
        let max = *rtts.iter().max().unwrap();
        println!(
            "  {n} échantillons — RTT min {:.1} ms / moyen {:.1} ms / max {:.1} ms",
            min.as_secs_f64() * 1000.0,
            avg.as_secs_f64() * 1000.0,
            max.as_secs_f64() * 1000.0
        );
    }

    // -----------------------------------------------------------------
    // 0bis. Latence de réflexion input → Snapshot : on bouge, on mesure le
    //    temps réel avant que le Snapshot reflète un déplacement mesurable.
    // -----------------------------------------------------------------
    println!("\n-- 0bis. Latence de réflexion déplacement → Snapshot --");
    let mut current_pos = my_pos;
    let mut reflect_samples: Vec<Duration> = Vec::new();
    for trial in 0..5 {
        let baseline = current_pos;
        let t0 = Instant::now();
        let mut got = false;
        let phase_deadline = t0 + Duration::from_millis(600);
        while Instant::now() < phase_deadline {
            send_input(&client, 1.0, 0.0, 0.0, false, false, false);
            match client.inbox.recv_timeout(Duration::from_millis(40)) {
                Ok(ServerMsg::Snapshot(s)) => {
                    if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                        current_pos = e.position;
                        if !got && dist2(current_pos, baseline).sqrt() > 0.03 {
                            let lat = t0.elapsed();
                            reflect_samples.push(lat);
                            println!(
                                "  essai {trial} : mouvement reflété après {:.1} ms",
                                lat.as_secs_f64() * 1000.0
                            );
                            got = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if !got {
            println!("  essai {trial} : aucun déplacement mesurable observé en 600 ms");
        }
        // petite pause idle entre essais
        let idle_deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < idle_deadline {
            send_input(&client, 0.0, 0.0, 0.0, false, false, false);
            if let Ok(ServerMsg::Snapshot(s)) =
                client.inbox.recv_timeout(Duration::from_millis(40))
                && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id))
            {
                current_pos = e.position;
            }
        }
    }
    if !reflect_samples.is_empty() {
        let n = reflect_samples.len();
        let sum: Duration = reflect_samples.iter().sum();
        println!(
            "  {n} échantillons — réflexion moyenne {:.1} ms",
            (sum / n as u32).as_secs_f64() * 1000.0
        );
    }

    // -----------------------------------------------------------------
    // 1. Combat PvE de base : trouve le monstre le plus proche du spawn,
    //    marche jusqu'à portée de mêlée, attaque jusqu'à mise à mort, mesure
    //    le temps réel, puis attend la réapparition (20 s attendus).
    // -----------------------------------------------------------------
    println!("\n-- 1. Combat PvE (recherche de cible) --");
    let mut target_index: Option<u32> = None;
    let mut target_pos = [0.0f32; 3];
    let mut target_health = None;
    let find_deadline = Instant::now() + Duration::from_secs(3);
    while target_index.is_none() && Instant::now() < find_deadline {
        send_input(&client, 0.0, 0.0, 0.0, false, false, false);
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(60)) {
            if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                current_pos = e.position;
            }
            let mut best: Option<(f32, &motor3derust::net::protocol::EntityDelta)> = None;
            for e in s
                .entities
                .iter()
                .filter(|e| e.player_id.is_none() && e.health.is_some() && e.visible)
            {
                let d = dist2(current_pos, e.position);
                if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                    best = Some((d, e));
                }
            }
            if let Some((d, e)) = best {
                target_index = Some(e.index);
                target_pos = e.position;
                target_health = e.health;
                println!(
                    "  cible verrouillée : entité {} à {:.1} m de nous, position {:?}, vie normalisée {:?}",
                    e.index,
                    d.sqrt(),
                    e.position,
                    e.health
                );
            }
        }
    }
    let target_index = target_index.expect("aucun monstre trouvé près du spawn");
    println!(
        "  PV initiaux (normalisés 0..1) : {target_health:?} — rappel : normalisé depuis le \
         14 sept. 2026, donc 1.0 = plein PV quel que soit le hp brut (Renard=3, Golem=6, etc.)"
    );

    // Marche vers la cible jusqu'à portée d'attaque (avec limite de temps).
    let walk_deadline = Instant::now() + Duration::from_secs(25);
    let mut in_range = false;
    while Instant::now() < walk_deadline {
        let dx = target_pos[0] - current_pos[0];
        let dz = target_pos[2] - current_pos[2];
        // Direction de marche : horizontale uniquement (move_x/move_y ne
        // pilotent que le plan XZ, la hauteur suit le terrain côté serveur).
        let d_horiz = (dx * dx + dz * dz).sqrt();
        // Décision « à portée » : distance 3D pleine, comme le serveur (cf.
        // `dist3`) — un simple d_horiz sous-estime la distance réelle sur
        // terrain accidenté et fait croire à une portée jamais atteinte
        // côté serveur.
        let d3 = dist3(target_pos, current_pos);
        if d3 <= ATTACK_RANGE * APPROACH_MARGIN {
            in_range = true;
            break;
        }
        let (mx, my) = if d_horiz > 1e-4 {
            (dx / d_horiz, -dz / d_horiz)
        } else {
            (0.0, 0.0)
        };
        let aim_yaw = (-dx).atan2(-dz); // orientation indicative (pas d'enjeu anti-triche, cf. protocol.rs)
        send_input(&client, mx, my, aim_yaw, false, false, false);
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(50)) {
            if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                current_pos = e.position;
            }
            if let Some(e) = s.entities.iter().find(|e| e.index == target_index) {
                target_pos = e.position;
                if !e.visible {
                    // Le monstre a été vaincu par quelqu'un d'autre (salon
                    // partagé en production) avant qu'on l'atteigne.
                    println!(
                        "  ⚠️  la cible {target_index} a disparu (tuée par un autre joueur du \
                         salon partagé) avant qu'on l'atteigne — on continue quand même vers sa \
                         dernière position connue."
                    );
                }
            }
        }
    }
    if !in_range {
        println!(
            "  ⚠️  jamais entré en portée de mêlée en 25 s (obstacle/poursuite ? terrain ?) — \
             on tente quand même quelques attaques depuis la position actuelle."
        );
    } else {
        println!("[{:>6.2}s] à portée de mêlée de l'entité {target_index}", elapsed(start));
    }

    // Boucle d'attaque : martèle `attack: true` à 50 ms, suit la vie
    // normalisée de la cible coup par coup, détecte la mise à mort
    // (`visible == false`).
    let attack_start = Instant::now();
    let mut last_health = target_health;
    let mut hits_observed = 0u32;
    let mut kill_time: Option<Duration> = None;
    let attack_deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < attack_deadline && kill_time.is_none() {
        // continue à recentrer légèrement dessus pour rester à portée si le
        // monstre recule/charge (AiChaser peut le déplacer).
        let dx = target_pos[0] - current_pos[0];
        let dz = target_pos[2] - current_pos[2];
        let d_horiz = (dx * dx + dz * dz).sqrt();
        // Recentrage : se rapproche tant que la distance 3D réelle (celle du
        // serveur) dépasse la portée — pas seulement l'horizontale, cf. la
        // doc de `dist3` plus haut sur le bug qu'un simple X/Z masquait.
        let d3 = dist3(target_pos, current_pos);
        let (mx, my) = if d3 > ATTACK_RANGE {
            (dx / d_horiz.max(1e-4), -dz / d_horiz.max(1e-4))
        } else {
            (0.0, 0.0)
        };
        send_input(&client, mx, my, 0.0, true, false, false);
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(50)) {
            if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                current_pos = e.position;
            }
            if let Some(e) = s.entities.iter().find(|e| e.index == target_index) {
                target_pos = e.position;
                if e.health != last_health {
                    hits_observed += 1;
                    println!(
                        "[{:>6.2}s] vie cible : {:?} -> {:?} (coup #{hits_observed})",
                        elapsed(start),
                        last_health,
                        e.health
                    );
                    last_health = e.health;
                }
                if !e.visible && kill_time.is_none() {
                    kill_time = Some(attack_start.elapsed());
                    println!(
                        "[{:>6.2}s] cible {target_index} masquée (vaincue). Temps réel de mise \
                         à mort depuis le premier coup : {:.2} s ({hits_observed} coup(s) \
                         observé(s) de vie décroissante)",
                        elapsed(start),
                        kill_time.unwrap().as_secs_f32()
                    );
                }
            }
        }
    }
    if kill_time.is_none() {
        println!(
            "  ⚠️  BUG POTENTIEL : la cible {target_index} n'a jamais été masquée après 15 s \
             d'attaques continues à portée (dernière vie observée : {last_health:?}, {hits_observed} \
             coup(s) comptés). Soit le monstre est increvable dans ce salon, soit `attack_at`/`damage_attackable` \
             ne compte pas nos coups malgré la portée."
        );
    }

    // Vérifie via nos propres `kills` diffusés que le serveur nous crédite
    // bien exactement UNE fois pour cette mise à mort (pas de double-comptage
    // si deux coups arrivent le même tick, pas d'absence de crédit non plus).
    let confirm_deadline = Instant::now() + Duration::from_secs(2);
    let mut kills_after = None;
    while Instant::now() < confirm_deadline {
        send_input(&client, 0.0, 0.0, 0.0, false, false, false);
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(60))
            && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id))
        {
            kills_after = e.kills;
        }
    }
    println!(
        "  Frags diffusés pour ce joueur : avant {kills_before:?} -> après {kills_after:?} \
         (salon partagé : peut inclure des frags sur d'autres monstres faits par nous entre-temps, \
         donc pas forcément +1 exact, mais doit avoir strictement augmenté si le kill ci-dessus est réel)"
    );

    // Réapparition (20 s attendues, riviere.rs:1132) — seulement si on a bien
    // observé une mise à mort.
    if kill_time.is_some() {
        println!("\n-- Attente de réapparition (≈{RESPAWN_DELAY_S:.0} s attendues) --");
        let respawn_wait_start = Instant::now();
        let respawn_deadline = respawn_wait_start + Duration::from_secs(35);
        let mut respawned = false;
        while Instant::now() < respawn_deadline {
            send_input(&client, 0.0, 0.0, 0.0, false, false, false);
            if let Ok(ServerMsg::Snapshot(s)) =
                client.inbox.recv_timeout(Duration::from_millis(100))
                && let Some(e) = s.entities.iter().find(|e| e.index == target_index)
                && e.visible
            {
                let dt = respawn_wait_start.elapsed();
                println!(
                    "[{:>6.2}s] cible {target_index} réapparue après {:.1} s (vie {:?}, \
                     position {:?})",
                    elapsed(start),
                    dt.as_secs_f32(),
                    e.health,
                    e.position
                );
                if (dt.as_secs_f32() - RESPAWN_DELAY_S).abs() > 3.0 {
                    println!(
                        "  ⚠️  écart notable avec le délai documenté ({RESPAWN_DELAY_S:.0} s) : {:.1} s",
                        dt.as_secs_f32()
                    );
                } else {
                    println!("  Conforme au délai documenté ({RESPAWN_DELAY_S:.0} s).");
                }
                respawned = true;
                break;
            }
        }
        if !respawned {
            println!(
                "  ⚠️  BUG POTENTIEL : la cible {target_index} n'est pas réapparue en 35 s alors \
                 que {RESPAWN_DELAY_S:.0} s sont attendues."
            );
        }
    }

    // -----------------------------------------------------------------
    // 2. Les deux boss : présence, visibilité, PV normalisés, position
    //    cohérente avec riviere.rs (boss_x=20, boss_z=-70 ; boss2_x=
    //    river_center(70)-18, boss2_z=70).
    // -----------------------------------------------------------------
    println!("\n-- 2. Vérification des deux boss --");
    let boss_expect = [20.0f32, -70.0f32];
    let boss2_expect_x = river_center(70.0) - 18.0;
    let boss2_expect = [boss2_expect_x, 70.0f32];
    let mut latest_entities: Vec<motor3derust::net::protocol::EntityDelta> = Vec::new();
    let scan_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < scan_deadline {
        send_input(&client, 0.0, 0.0, 0.0, false, false, false);
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(80)) {
            latest_entities = s.entities;
        }
    }
    for (label, expect, min_hp_hint) in [
        ("Boss 1 « L'Aîné de la Cascade »", boss_expect, "hp brut attendu 60"),
        (
            "Boss 2 « Le Roi-Champignon du Sous-bois »",
            boss2_expect,
            "hp brut attendu ~70",
        ),
    ] {
        let closest = latest_entities
            .iter()
            .filter(|e| e.player_id.is_none() && e.health.is_some())
            .min_by(|a, b| {
                let da = (a.position[0] - expect[0]).powi(2) + (a.position[2] - expect[1]).powi(2);
                let db = (b.position[0] - expect[0]).powi(2) + (b.position[2] - expect[1]).powi(2);
                da.partial_cmp(&db).unwrap()
            });
        match closest {
            Some(e) => {
                let d = ((e.position[0] - expect[0]).powi(2) + (e.position[2] - expect[1]).powi(2))
                    .sqrt();
                println!(
                    "  {label} : entité {} à {:.1} m de la position documentée {expect:?}, \
                     position réelle {:?}, visible={}, vie normalisée={:?} ({min_hp_hint})",
                    e.index, d, e.position, e.visible, e.health
                );
                if d > 8.0 {
                    println!(
                        "  ⚠️  écart de position notable (> 8 m) avec ce que documente riviere.rs."
                    );
                }
            }
            None => println!("  ⚠️  BUG POTENTIEL : aucune entité avec vie trouvée près de {label}."),
        }
    }

    // -----------------------------------------------------------------
    // 3. Kit de capacités réseau : dash (déplacement réel mesuré) et block
    //    (réduction de dégâts pendant une morsure, si on peut l'orchestrer).
    // -----------------------------------------------------------------
    println!("\n-- 3. Kit de capacités réseau --");
    // Dash : une impulsion vers +X, mesure le déplacement réel dans le
    // prochain Snapshot vs DASH_DISTANCE documenté (5.0 m, borné par obstacle
    // éventuel).
    let before_dash = current_pos;
    send_input(&client, 1.0, 0.0, 0.0, false, false, true);
    let dash_deadline = Instant::now() + Duration::from_millis(400);
    let mut after_dash = before_dash;
    while Instant::now() < dash_deadline {
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(40))
            && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id))
        {
            after_dash = e.position;
        }
    }
    current_pos = after_dash;
    let dash_moved = dist2(before_dash, after_dash).sqrt();
    println!(
        "  Dash : déplacement mesuré {:.2} m (documenté : jusqu'à 5.0 m, borné par obstacle \
         proche) — {}",
        dash_moved,
        if dash_moved > 0.3 {
            "pris en compte côté serveur"
        } else {
            "⚠️ déplacement quasi nul, capacité peut-être ignorée ou obstacle immédiat"
        }
    );

    // Block : reste au contact d'un monstre vivant (si on en a un à
    // proximité après la réapparition/les déplacements ci-dessus) et compare
    // la perte de vie sur 2 s sans bouclier puis 2 s avec.
    let mut nearby_monster: Option<[f32; 3]> = None;
    let scan2_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < scan2_deadline {
        send_input(&client, 0.0, 0.0, 0.0, false, false, false);
        if let Ok(ServerMsg::Snapshot(s)) = client.inbox.recv_timeout(Duration::from_millis(60)) {
            if let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id)) {
                current_pos = e.position;
            }
            if let Some(e) = s
                .entities
                .iter()
                .filter(|e| e.player_id.is_none() && e.visible && e.health.is_some())
                .min_by(|a, b| {
                    dist2(current_pos, a.position)
                        .partial_cmp(&dist2(current_pos, b.position))
                        .unwrap()
                })
            {
                nearby_monster = Some(e.position);
            }
        }
    }
    if let Some(mpos) = nearby_monster {
        let d = dist2(current_pos, mpos).sqrt();
        println!("  Monstre le plus proche pour le test bouclier : à {d:.1} m");
        if d < 6.0 {
            let mut my_health_before = None;
            let phase_a = Instant::now() + Duration::from_secs(3);
            while Instant::now() < phase_a {
                // sans bouclier, on reste au contact si possible
                let dx = mpos[0] - current_pos[0];
                let dz = mpos[2] - current_pos[2];
                let dd = (dx * dx + dz * dz).sqrt().max(1e-4);
                send_input(&client, dx / dd, -dz / dd, 0.0, false, false, false);
                if let Ok(ServerMsg::Snapshot(s)) =
                    client.inbox.recv_timeout(Duration::from_millis(50))
                    && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id))
                {
                    current_pos = e.position;
                    my_health_before = e.health;
                }
            }
            let health_start_a = my_health_before;
            let phase_a2 = Instant::now() + Duration::from_secs(2);
            let mut health_end_a = my_health_before;
            while Instant::now() < phase_a2 {
                send_input(&client, 0.0, 0.0, 0.0, false, false, false);
                if let Ok(ServerMsg::Snapshot(s)) =
                    client.inbox.recv_timeout(Duration::from_millis(50))
                    && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id))
                {
                    health_end_a = e.health;
                    current_pos = e.position;
                }
            }
            let phase_b_start = Instant::now();
            let mut health_end_b = health_end_a;
            while phase_b_start.elapsed() < Duration::from_secs(2) {
                send_input(&client, 0.0, 0.0, 0.0, false, true, false);
                if let Ok(ServerMsg::Snapshot(s)) =
                    client.inbox.recv_timeout(Duration::from_millis(50))
                    && let Some(e) = s.entities.iter().find(|e| e.player_id == Some(my_id))
                {
                    health_end_b = e.health;
                }
            }
            println!(
                "  Vie propre : {health_start_a:?} -> (2s sans bouclier) {health_end_a:?} -> \
                 (2s avec block:true) {health_end_b:?}"
            );
            match (health_end_a, health_end_b, health_start_a) {
                (Some(a), Some(b), Some(s0)) => {
                    let loss_no_block = s0 - a;
                    let loss_block = a - b;
                    println!(
                        "  Perte sans bouclier ≈ {:.4} PV, perte avec bouclier ≈ {:.4} PV \
                         (attendu : avec bouclier notablement plus faible, facteur ≈0.25)",
                        loss_no_block, loss_block
                    );
                    if loss_no_block > 0.001 && loss_block >= loss_no_block {
                        println!(
                            "  ⚠️  BUG POTENTIEL : le bouclier ne réduit pas les dégâts mesurés."
                        );
                    } else if loss_no_block <= 0.001 {
                        println!(
                            "  (aucun dégât mesuré sans bouclier non plus — le monstre n'a \
                             peut-être pas mordu pendant la fenêtre, test non concluant)"
                        );
                    } else {
                        println!("  Réduction observée, cohérente avec BLOCK_DAMAGE_MULT.");
                    }
                }
                _ => println!("  Vie non diffusée pendant ce test — non concluant."),
            }
        } else {
            println!("  Pas de monstre assez proche pour orchestrer un test de morsure fiable.");
        }
    } else {
        println!("  Aucun monstre visible à proximité pour le test bouclier.");
    }

    client.send(&ClientMsg::Leave);
    println!(
        "\n== Fin de sonde à {:.1}s — déconnexion propre envoyée ==",
        elapsed(start)
    );
}
