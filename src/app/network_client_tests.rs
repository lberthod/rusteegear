// Sprint 105a-3 : uniquement utilisés par les tests réseau ci-dessous
// (derrière `net_tests`) — sans ce `cfg`, `cargo test` par défaut (sans
// la feature) les signale comme imports/fonctions mortes.
#[cfg(feature = "net_tests")]
use std::time::{Duration, Instant};

#[cfg(feature = "net_tests")]
use glam::Vec3;

use super::*;
#[cfg(feature = "net_tests")]
use crate::app::multiplayer::NetworkInput;

#[test]
fn max_chat_len_stays_in_sync_with_net_firebase() {
    assert_eq!(
        MAX_CHAT_LEN,
        crate::net::firebase::MAX_CHAT_LEN,
        "copie universelle (cf. sa doc) désynchronisée de la source"
    );
}
#[cfg(feature = "net_tests")]
use crate::net::protocol::EntityDelta;
#[cfg(feature = "net_tests")]
use crate::net::protocol::{ClientMsg, ServerMsg};
#[cfg(feature = "net_tests")]
use crate::net::server_loop::NetServer;

/// Fait progresser le "serveur de test" d'un tick : traite les messages en
/// attente (Join/Input/Leave, cf. `src/bin/server.rs`), simule, diffuse un
/// `Snapshot`. Retourne le numéro de tick utilisé.
#[cfg(feature = "net_tests")]
fn server_tick(server_app: &mut AppState, net: &NetServer, tick: u32) {
    while let Ok((id, msg)) = net.inbox.try_recv() {
        match msg {
            ClientMsg::Join { class, .. } => {
                server_app
                    .spawn_network_player(id, crate::app::multiplayer::PlayerClass::from_u8(class));
            }
            ClientMsg::Input {
                move_x,
                move_y,
                aim_yaw,
                attack,
                jump,
                fire,
                weapon,
                heal,
            } => {
                server_app.set_network_input(
                    id,
                    NetworkInput {
                        move_x,
                        move_y,
                        aim_yaw,
                        attack,
                        jump,
                        fire,
                        weapon,
                        heal,
                    },
                );
            }
            ClientMsg::Leave => {
                server_app.despawn_network_player(id);
            }
        }
    }
    server_app.advance_play();
    net.broadcast_all_rooms(&ServerMsg::Snapshot(server_app.network_snapshot(tick)));
}

/// `poll_firebase` (appelée par `poll_network`) applique le résultat d'une
/// requête Firebase dès qu'il arrive sur le canal — sans dépendre d'un
/// vrai projet Firebase : on pousse directement un résultat simulé sur
/// `firebase_tx`, exactement ce qu'un thread de fond ferait après un
/// `sign_in` réel.
#[test]
fn firebase_uid_is_applied_once_the_background_request_resolves() {
    let mut app = AppState::new();
    assert!(!app.has_firebase_account());

    app.firebase_tx
        .send(Ok(("uid-test-1234".to_string(), "token-test".to_string())))
        .expect("canal ouvert");
    app.poll_network();

    assert!(app.has_firebase_account());
    assert_eq!(app.firebase_uid.as_deref(), Some("uid-test-1234"));
}

/// GDD §5.3 : « la mort d'un allié est un événement de groupe ». Un
/// `PlayerDown` pour un **autre** joueur doit déclencher la bannière
/// partagée (`ally_down_flash`) et ne jamais toucher `damage_flash` (déjà
/// réservé à notre propre mort) — pas besoin d'un vrai socket, la
/// distribution des `GameEvent` est un aiguillage pur dans
/// `handle_server_msg`.
#[test]
fn player_down_for_an_ally_raises_the_shared_banner_not_the_self_flash() {
    let mut app = AppState::new();
    app.net_player_id = Some(1);

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::PlayerDown {
            player_id: 2,
            cause: None,
        },
    ));

    assert_eq!(
        app.ally_down_flash, 1.0,
        "la mort d'un allié doit déclencher la bannière partagée"
    );
    assert_eq!(
        app.damage_flash, 0.0,
        "la mort d'un allié ne doit jamais déclencher notre propre flash de dégâts"
    );
}

/// Symétrique du test ci-dessus : notre propre `PlayerDown` continue de
/// déclencher `damage_flash` (comportement préexistant), jamais la
/// bannière « allié à terre » (qui n'a de sens que pour les autres).
#[test]
fn player_down_for_ourselves_raises_the_self_flash_not_the_ally_banner() {
    let mut app = AppState::new();
    app.net_player_id = Some(1);

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::PlayerDown {
            player_id: 1,
            cause: None,
        },
    ));

    assert_eq!(
        app.damage_flash, 1.0,
        "notre propre mort doit déclencher le flash de dégâts"
    );
    assert_eq!(
        app.ally_down_flash, 0.0,
        "notre propre mort ne doit pas déclencher la bannière « allié à terre »"
    );
}

/// Phase C (Sprint 5, `sprint10audit.md`, auto-relecture) : sans ce
/// message, un client resterait sur son défaut local `Vagues` même dans
/// un salon `Survie`/etc. arbitré côté serveur (`Lobby::objective`,
/// `bin/server.rs`) — `update_round` (`app::combat`) déclencherait alors
/// une victoire locale prématurée à la dernière manche vidée, que le
/// serveur, lui, reboucle.
#[test]
fn round_objective_event_aligns_our_local_objective_with_the_room() {
    let mut app = AppState::new();
    assert_eq!(
        app.objective,
        crate::app::multiplayer::RoundObjective::Vagues,
        "défaut avant tout message du serveur"
    );

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::RoundObjective {
            objective: crate::app::multiplayer::RoundObjective::Survie.to_u8(),
        },
    ));

    assert_eq!(
        app.objective,
        crate::app::multiplayer::RoundObjective::Survie
    );
}

/// Phase H, Sprint 1 (écran de fin de manche détaillé, GDD §9.2/§17.4) :
/// `GameEvent::Win` doit remplir `round_summary`/`round_summary_won`/
/// `round_contract_label` — avant ce fix, ces événements tombaient dans
/// le catch-all `ServerMsg::Event(_) => {}`, aucune donnée n'était jamais
/// mémorisée côté client.
#[test]
fn win_event_stores_the_round_summary_and_contract() {
    use crate::net::protocol::RoundPlayerSummary;

    let mut app = AppState::new();
    assert!(app.round_summary.is_none());

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::Win {
            summary: vec![RoundPlayerSummary {
                player_id: 1,
                name: "Loïc".to_string(),
                frags: 3,
                assists: 1,
                xp: 245,
            }],
            contract: Some(crate::app::multiplayer::Contract::AubeJuste.to_u8()),
        },
    ));

    assert!(app.round_summary_won);
    assert_eq!(
        app.round_summary.as_deref(),
        Some(
            [RoundPlayerSummary {
                player_id: 1,
                name: "Loïc".to_string(),
                frags: 3,
                assists: 1,
                xp: 245,
            }]
            .as_slice()
        )
    );
    assert_eq!(
        app.round_contract_label,
        Some(crate::app::multiplayer::Contract::AubeJuste.label())
    );
}

/// Symétrique : `GameEvent::Lose` mémorise le résumé mais jamais de
/// contrat (une défaite ne remplit jamais de Contrat du jour, GDD §3.4).
#[test]
fn lose_event_stores_the_round_summary_without_a_contract() {
    use crate::net::protocol::RoundPlayerSummary;

    let mut app = AppState::new();

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::Lose {
            summary: vec![RoundPlayerSummary {
                player_id: 1,
                name: "Loïc".to_string(),
                frags: 0,
                assists: 0,
                xp: 150,
            }],
        },
    ));

    assert!(!app.round_summary_won);
    assert!(app.round_summary.is_some());
    assert!(app.round_contract_label.is_none());
}

/// Phase H, Sprint 2 (bannière de vague, GDD §17.2) : `GameEvent::WaveStart`
/// doit armer `wave_banner_flash` (décroissance par frame côté
/// `advance_play`) avec le numéro de vague annoncé — jusqu'ici jamais
/// émis par la boucle serveur, ce message n'avait donc aucun effet observable.
#[test]
fn wave_start_event_arms_the_wave_banner() {
    let mut app = AppState::new();
    assert_eq!(app.wave_banner_flash, 0.0);

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::WaveStart { wave: 3 },
    ));

    assert_eq!(app.wave_banner_flash, 1.0);
    assert_eq!(app.wave_banner_wave, 3);
}

/// Sprint 2 (`sprint10audit.md`) : la cause de mort reçue du serveur doit
/// être mémorisée pour notre propre mort (affichage `defeated_banner`),
/// mais jamais pour la mort d'un allié — elle n'a de sens que pour la
/// victime qui la lit sur son propre écran.
#[test]
fn death_cause_is_stored_for_our_own_death_only() {
    let mut app = AppState::new();
    app.net_player_id = Some(1);
    let cause = crate::net::protocol::DeathCause {
        kind: crate::net::protocol::DeathCauseKind::Monster,
        distinct_attackers: 2,
    };

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::PlayerDown {
            player_id: 2,
            cause: Some(cause),
        },
    ));
    assert_eq!(
        app.death_cause, None,
        "la cause de mort d'un allié ne doit pas être mémorisée comme la nôtre"
    );

    app.handle_server_msg(crate::net::protocol::ServerMsg::Event(
        crate::net::protocol::GameEvent::PlayerDown {
            player_id: 1,
            cause: Some(cause),
        },
    ));
    assert_eq!(
        app.death_cause,
        Some(cause),
        "notre propre cause de mort doit être mémorisée pour l'affichage"
    );
}

/// Une fois un `uid` Firebase connu, `connect_to_server` doit le
/// transmettre au `Join` — c'est ce qui permet au serveur de créditer la
/// bonne progression. Vérifié à travers un vrai socket : le
/// `Join` reçu côté serveur doit porter le même `uid`.
#[cfg(feature = "net_tests")]
#[test]
fn connect_to_server_forwards_the_known_firebase_uid() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let url = format!("ws://{}", net.local_addr);

    let mut app = AppState::new();
    app.firebase_tx
        .send(Ok(("uid-alice".to_string(), "token-alice".to_string())))
        .expect("canal ouvert");
    app.poll_network(); // applique le uid simulé avant la connexion

    app.connect_to_server(&url, "Alice");

    let (_, msg) = net
        .inbox
        .recv_timeout(Duration::from_secs(2))
        .expect("Join attendu côté serveur");
    assert_eq!(
        msg,
        ClientMsg::Join {
            protocol: crate::net::protocol::PROTOCOL_VERSION,
            name: "Alice".to_string(),
            firebase_uid: Some("uid-alice".to_string()),
            lobby: crate::net::protocol::DEFAULT_LOBBY.to_string(),
            class: 0,
            objective: 0,
        }
    );
}

/// Sans compte connecté (`firebase_id_token` absent), l'envoi d'un message
/// de chat ne doit ni planter ni démarrer de requête réseau — les règles
/// RTDB refuseraient de toute façon l'écriture à un client anonyme
/// (cf. `net::firebase`).
#[test]
fn sending_chat_without_an_account_is_a_no_op() {
    let mut app = AppState::new();
    assert!(!app.chat_busy);

    app.request_send_chat_message(
        "api-key".to_string(),
        "https://example.firebaseio.com".to_string(),
        "salon-1".to_string(),
        "Alice".to_string(),
        "coucou".to_string(),
    );

    assert!(
        !app.chat_busy,
        "aucune requête ne doit démarrer sans compte connecté"
    );
    assert!(app.chat_messages.is_empty());
}

/// `poll_leaderboard` (appelée par `poll_network`) applique le résultat
/// d'une requête de classement dès qu'il arrive sur le canal — même schéma
/// que le test équivalent pour Firebase Auth, sans dépendre d'un vrai
/// projet Firebase.
#[test]
fn leaderboard_is_applied_once_the_background_request_resolves() {
    let mut app = AppState::new();
    assert!(app.leaderboard.is_empty());

    app.leaderboard_tx
        .send(Ok(vec![
            LeaderboardLine {
                name: "Bob".to_string(),
                score: 42,
            },
            LeaderboardLine {
                name: "Alice".to_string(),
                score: 12,
            },
        ]))
        .expect("canal ouvert");
    app.poll_network();

    assert_eq!(app.leaderboard.len(), 2);
    assert_eq!(app.leaderboard[0].name, "Bob");
}

/// Bout-en-bout (vrai socket) : deux clients rejoignent le même serveur de
/// test. Chacun doit voir un fantôme pour **l'autre**, mais jamais pour
/// lui-même — c'est exactement le bug qu'aurait causé l'absence de
/// `EntityDelta::player_id` (sans lui, impossible de distinguer « moi » de
/// « l'autre joueur » dans un `Snapshot`).
#[cfg(feature = "net_tests")]
#[test]
fn client_sees_a_ghost_for_the_other_player_but_never_for_itself() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let url = format!("ws://{}", net.local_addr);

    let mut server_app = AppState::new();
    server_app.load_zombies_demo();
    server_app.playing = true;

    let mut alice = AppState::new();
    alice.load_zombies_demo();
    alice.connect_to_server(&url, "Alice");
    assert!(alice.is_connected());

    let mut bob = AppState::new();
    bob.load_zombies_demo();
    bob.connect_to_server(&url, "Bob");
    assert!(bob.is_connected());

    // Quelques itérations : traite les Join, envoie des Input, diffuse des
    // Snapshot — le temps que tout le monde reçoive son Welcome et le
    // Snapshot de l'autre joueur.
    for tick in 0..20 {
        std::thread::sleep(Duration::from_millis(20));
        server_tick(&mut server_app, &net, tick);
        alice.poll_network();
        bob.poll_network();
    }

    assert_eq!(
        alice.remote_players.len(),
        1,
        "Alice doit voir exactement un fantôme (Bob), pas elle-même"
    );
    assert_eq!(
        bob.remote_players.len(),
        1,
        "Bob doit voir exactement un fantôme (Alice), pas lui-même"
    );
    assert!(alice.net_player_id.is_some());
    assert!(bob.net_player_id.is_some());
    assert_ne!(alice.net_player_id, bob.net_player_id);
    assert!(
        !alice
            .remote_players
            .contains_key(&alice.net_player_id.expect("vérifié ci-dessus")),
        "Alice ne doit jamais avoir un fantôme d'elle-même"
    );
}

/// Pas besoin d'un vrai socket : construit une scène minimale (sol +
/// joueur pilotable), crée le fantôme d'un joueur réseau via
/// `ensure_remote_player` puis simule l'arrivée d'un premier `Snapshot`
/// (le fantôme devient visible juste devant le joueur, chevauchement
/// voulu) — preuve de la demande gameplay « les entités mobiles ne
/// doivent pas se retrouver superposées » : avant ce correctif, le
/// fantôme réseau n'avait aucun corps physique (`PhysicsKind::None`,
/// « affichage seul ») et le joueur local pouvait le traverser librement.
#[test]
fn a_remote_player_ghost_blocks_the_local_player_instead_of_overlapping() {
    let mut app = AppState::new();
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(glam::Vec3::new(0.0, -1.0, 0.0))
            .with_scale(glam::Vec3::new(20.0, 1.0, 20.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    });
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(glam::Vec3::new(-1.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    });
    let player = 1;
    app.playing = true;
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

    // Crée le fantôme (encore masqué, `visible: false` — pas de corps tant
    // qu'aucun `Snapshot` n'est arrivé, cf. `Physics::build`).
    let rp = app.ensure_remote_player(1, "Bob");
    let ghost = rp.scene_index;
    assert_eq!(
        app.scene.objects[ghost].physics,
        crate::runtime::physics::PhysicsKind::Kinematic,
        "un fantôme réseau doit être un corps kinématique scripté, comme une créature"
    );

    // Simule le premier `Snapshot` : Bob apparaît juste devant Alice,
    // presque confondu avec elle (chevauchement délibéré).
    app.scene.objects[ghost].transform.position = glam::Vec3::new(-0.9, 1.0, 0.0);
    app.scene.objects[ghost].visible = true;
    // `poll_network` déclenche normalement ce rebuild via `net_visibility_dirty`
    // (bascule détectée en comparant l'ancienne et la nouvelle valeur de
    // `visible`) — reproduit ici à la main, ce test écrivant directement
    // `scene.objects[ghost].visible` sans passer par un vrai `Snapshot` réseau.
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

    // Alice fonce plein est droit dans le fantôme.
    let dt = 1.0 / 60.0;
    for _ in 0..120 {
        let phys = app.physics.as_mut().expect("construit ci-dessus");
        phys.control(player, 2.0, 0.0, false, 0.0, 0.0, dt);
        phys.resolve_scripted_moves(dt, &mut app.scene);
        phys.step(dt, &mut app.scene);
    }
    let player_pos = app.scene.objects[player].transform.position;
    let ghost_pos = app.scene.objects[ghost].transform.position;
    assert!(
        player_pos.distance(ghost_pos) > 0.5,
        "le joueur local doit être bloqué par le fantôme réseau, pas se superposer avec \
             lui — joueur={player_pos:?}, fantôme={ghost_pos:?}"
    );
}

/// Marqueur allié hors-écran (Phase L Sprint 2, `sprint2audijeu0718.md`) :
/// `nearest_downed_ally_position` doit ignorer un fantôme encore en vie
/// et ne renvoyer que le plus proche parmi ceux à 0 PV — même scène
/// minimale (sol + joueur pilotable) que le test de blocage physique
/// ci-dessus, deux fantômes réseau plutôt qu'un.
#[test]
fn nearest_downed_ally_position_ignores_ghosts_still_alive() {
    let mut app = AppState::new();
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(glam::Vec3::ZERO),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        visible: true,
        ..Default::default()
    });

    let near = app.ensure_remote_player(1, "Bob");
    let near_ghost = near.scene_index;
    near.health = Some(1.0);
    app.scene.objects[near_ghost].transform.position = glam::Vec3::new(2.0, 0.0, 0.0);
    app.scene.objects[near_ghost].visible = true;

    let far = app.ensure_remote_player(2, "Carla");
    let far_ghost = far.scene_index;
    far.health = Some(0.0);
    app.scene.objects[far_ghost].transform.position = glam::Vec3::new(5.0, 0.0, 0.0);
    app.scene.objects[far_ghost].visible = true;

    assert_eq!(
        app.nearest_downed_ally_position(),
        Some(glam::Vec3::new(5.0, 0.0, 0.0)),
        "Bob est plus proche mais debout (1.0 PV) — seule Carla (0 PV) doit ressortir"
    );
}

/// Bout-en-bout (vrai socket, scène MMORPG) : les créatures scriptées
/// (`Combat::attackable` posé par `scene::demos::MMORPG_CREATURES`, cf.
/// Sprint « créatures réseau ») doivent être **identiques** — pas
/// seulement proches — sur le serveur et sur deux clients distincts,
/// preuve qu'aucun des deux clients ne fait tourner sa propre copie de la
/// patrouille en plus de celle du serveur (`is_online_client()` dans la
/// boucle de scripts, `simulation.rs`). Couvre aussi la mise à mort
/// (`visible:false` diffusé identiquement) et les dégâts de morsure
/// (`network_health`, `health::update_creature_bite`, générique à
/// **toute** créature qui pose `SceneObject::bite` — testé ici sur la
/// Créature 1, mais rien dans le code de la boucle ne la nomme).
#[cfg(feature = "net_tests")]
#[test]
fn two_connected_clients_see_the_same_creature_position_kill_and_bite_damage() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let url = format!("ws://{}", net.local_addr);

    let mut server_app = AppState::new();
    server_app.scene = crate::scene::Scene::mmorpg_demo();
    server_app.hide_local_player_template();
    server_app.playing = true;

    let mut alice = AppState::new();
    alice.scene = crate::scene::Scene::mmorpg_demo();
    alice.connect_to_server(&url, "Alice");
    assert!(alice.is_connected());

    let mut bob = AppState::new();
    bob.scene = crate::scene::Scene::mmorpg_demo();
    bob.connect_to_server(&url, "Bob");
    assert!(bob.is_connected());

    for tick in 0..20 {
        std::thread::sleep(Duration::from_millis(20));
        server_tick(&mut server_app, &net, tick);
        alice.poll_network();
        bob.poll_network();
    }
    // Laisse le **dernier** `Snapshot` déjà envoyé finir d'arriver (socket
    // asynchrone : `poll_network()` juste après `server_tick` peut courir
    // plus vite que la trame réseau) — sans figer la position serveur
    // elle-même (aucun `server_tick` ici), quelques passages suffisent à
    // vider la file sans introduire de nouvel état à rattraper.
    let settle = |alice: &mut AppState, bob: &mut AppState| {
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(15));
            alice.poll_network();
            bob.poll_network();
        }
    };
    settle(&mut alice, &mut bob);

    let creature_idx = server_app
        .scene
        .objects
        .iter()
        .position(|o| o.name == "Créature")
        .expect("la démo MMORPG doit contenir « Créature »");

    // --- Position : identique des deux côtés, pas juste proche (sinon
    // c'est encore une double simulation résiduelle).
    let server_pos = server_app.scene.objects[creature_idx].transform.position;
    assert_eq!(
        alice.scene.objects[creature_idx].transform.position, server_pos,
        "Alice doit voir exactement la position serveur de la Créature"
    );
    assert_eq!(
        bob.scene.objects[creature_idx].transform.position, server_pos,
        "Bob doit voir exactement la position serveur de la Créature"
    );

    // --- Morsure : place un joueur réseau au contact de la Créature 1
    // (mordeuse, cf. `MMORPG_CREATURES`) côté serveur, avance, vérifie que
    // `network_health` baisse et se propage identiquement aux deux clients.
    let alice_id = alice.net_player_id.expect("Alice doit être connectée");
    let alice_idx = *server_app
        .network_players
        .get(&alice_id)
        .expect("Alice doit avoir un objet serveur");
    // Cooldown 2,2 s / chance 0,4 (cf. `MMORPG_CREATURES`, Créature 1) : une
    // fenêtre courte laisserait une réelle chance qu'aucun tirage ne
    // réussisse — même raison que le test solo équivalent
    // (`creature_1_bites_the_player_sometimes_not_on_every_contact_tick`,
    // fenêtre de 20 s) d'utiliser une fenêtre large plutôt qu'une poignée
    // de tentatives.
    const BITE_CONTACT_TICKS: u32 = 12 * 60;
    // Vie **minimale** observée pendant la fenêtre, pas la vie finale : la
    // régénération passive (`REGEN_PER_S` = 0,05/s) dépasse en moyenne les
    // dégâts de morsure (0,12 × 40 % / 2,2 s ≈ 0,022/s) — la vie remonte à
    // 100 % en ~2,4 s après chaque morsure réussie. Une assertion sur la
    // vie *finale* ne tenait donc que si un tirage (déterministe,
    // cf. `health::deterministic_roll`) réussissait dans les toutes
    // dernières secondes : n'importe quel décalage d'horodatage des
    // contacts (ex. l'audit des déplacements de créatures) suffisait à la
    // casser sans qu'aucune mécanique ne soit fausse.
    let mut min_server_health = f32::MAX;
    let mut min_alice_seen = f32::MAX;
    let mut min_bob_seen = f32::MAX;
    for tick in 20..(20 + BITE_CONTACT_TICKS) {
        let pos = server_app.scene.objects[creature_idx].transform.position;
        // Recale aussi le corps physique réel, pas seulement `transform`
        // (même piège documenté par
        // `creature_1_bites_the_player_sometimes_not_on_every_contact_tick`,
        // `app::simulation::tests`) : sans ça, le pas de physique du
        // prochain `advance_play` réécrit `transform.position` depuis le
        // corps rigide resté à son ancienne position, avant même que
        // `update_creature_bite` ne teste le contact.
        if let Some(phys) = server_app.physics.as_mut() {
            phys.set_position(alice_idx, pos);
        }
        server_app.scene.objects[alice_idx].transform.position = pos;
        std::thread::sleep(Duration::from_millis(16));
        server_tick(&mut server_app, &net, tick);
        alice.poll_network();
        bob.poll_network();
        if let Some(h) = server_app.network_player_health(alice_id) {
            min_server_health = min_server_health.min(h);
        }
        if let Some(h) = alice.net_local_health {
            min_alice_seen = min_alice_seen.min(h);
        }
        if let Some(h) = bob.remote_players.get(&alice_id).and_then(|rp| rp.health) {
            min_bob_seen = min_bob_seen.min(h);
        }
    }
    settle(&mut alice, &mut bob);
    assert!(
        min_server_health < 1.0,
        "{BITE_CONTACT_TICKS} ticks de contact avec la Créature 1 (mordeuse) \
             auraient dû infliger des dégâts à un moment de la fenêtre : {min_server_health}"
    );
    // Le creux de vie doit avoir été **vu** par les deux clients (une
    // morsure dure ~2,4 s avant régénération complète, soit ~144 snapshots
    // à 60 Hz — largement le temps d'arriver aux deux).
    assert!(
        min_alice_seen < 1.0,
        "Alice doit avoir vu sa vie baisser après une morsure : {min_alice_seen}"
    );
    assert!(
        min_bob_seen < 1.0,
        "Bob doit avoir vu la vie d'Alice baisser après une morsure : {min_bob_seen}"
    );
    let server_health = server_app
        .network_player_health(alice_id)
        .expect("Alice doit avoir une vie réseau");
    assert_eq!(
        alice.net_local_health,
        Some(server_health),
        "Alice doit voir sa propre vie exactement comme le serveur la connaît"
    );
    assert_eq!(
        bob.remote_players.get(&alice_id).and_then(|rp| rp.health),
        Some(server_health),
        "Bob doit voir la vie d'Alice exactement comme le serveur la connaît"
    );

    // --- Mise à mort : `Combat::attackable` (posé par `MMORPG_CREATURES`)
    // suffit à rendre la créature tuable via le pipeline générique
    // (`Scene::damage_attackable_by`, le même que `fireball::resolve_
    // fireball_hit`) — `visible:false` doit se propager identiquement.
    assert!(
        server_app.scene.damage_attackable_by(creature_idx, 999),
        "avec Combat::attackable + hp par défaut (1), un coup doit suffire à vaincre la créature"
    );
    for tick in (20 + BITE_CONTACT_TICKS)..(20 + BITE_CONTACT_TICKS + 20) {
        std::thread::sleep(Duration::from_millis(20));
        server_tick(&mut server_app, &net, tick);
        alice.poll_network();
        bob.poll_network();
    }
    settle(&mut alice, &mut bob);
    assert!(
        !alice.scene.objects[creature_idx].visible,
        "Alice doit voir la Créature vaincue disparaître"
    );
    assert!(
        !bob.scene.objects[creature_idx].visible,
        "Bob doit voir la Créature vaincue disparaître"
    );
}

/// Prépare un `AppState` connecté (vrai socket) avec un gabarit pilotable
/// chargé, prêt pour les tests de `apply_local_network_position` — la
/// même mise en place que `client_sees_a_ghost_for_the_other_player_but_
/// never_for_itself`, réduite à un seul client (ces tests ne portent que
/// sur la réconciliation du joueur local, pas sur les fantômes distants).
#[cfg(feature = "net_tests")]
fn connected_app_with_a_player(net: &NetServer) -> AppState {
    let url = format!("ws://{}", net.local_addr);
    let mut app = AppState::new();
    app.load_zombies_demo();
    app.connect_to_server(&url, "Testeur");
    assert!(app.is_connected());
    app
}

/// Bout-en-bout (vrai socket) : `is_locally_defeated()` doit refléter la
/// vie **réelle** connue du serveur (`net_local_health`, mise à jour à
/// chaque `Snapshot`), pas un état local présumé — sans ce garde-fou HUD
/// (`defeated_banner`, `editor/mod.rs`), un joueur à 0 PV disparaissait de
/// l'écran sans le moindre message (juste le flash rouge d'un tiers de
/// seconde), indiscernable d'un bug.
#[cfg(feature = "net_tests")]
#[test]
fn is_locally_defeated_reflects_the_servers_health_once_it_reaches_zero() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut server_app = AppState::new();
    server_app.load_zombies_demo();
    server_app.playing = true;

    let mut app = connected_app_with_a_player(&net);
    for tick in 0..15 {
        std::thread::sleep(Duration::from_millis(20));
        server_tick(&mut server_app, &net, tick);
        app.poll_network();
    }
    assert!(
        !app.is_locally_defeated(),
        "un joueur fraîchement connecté, en pleine santé, n'est pas vaincu"
    );

    // Vainc le joueur directement côté serveur (0 PV), sans dépendre d'un
    // vrai contact monstre — ce test isole la propagation Snapshot → HUD,
    // pas le combat lui-même (déjà couvert par `src/app/health.rs`).
    let id = app.net_player_id.expect("connecté, donc un id attribué");
    server_app.network_health.insert(id, 0.0);
    if let Some(&index) = server_app.network_players.get(&id) {
        server_app.scene.objects[index].visible = false;
    }
    for tick in 15..30 {
        std::thread::sleep(Duration::from_millis(20));
        server_tick(&mut server_app, &net, tick);
        app.poll_network();
    }

    assert!(
        app.is_locally_defeated(),
        "une fois la vie serveur tombée à 0, le client doit le savoir (net_local_health={:?})",
        app.net_local_health
    );
}

/// cf. `SPRINTNETWORK.md`, §2.3 de `AUDIT_LATENCE_MULTIJOUEUR.md` :
/// un écart au-dessus de `SNAP_THRESHOLD` ne doit **jamais** faire sauter
/// le joueur local directement à la position autoritative en un seul
/// appel — seulement un petit pas (`CORRECTION_PULL`) vers elle.
#[cfg(feature = "net_tests")]
#[test]
fn a_single_call_only_takes_a_small_step_toward_the_authoritative_position() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);

    let pi = app.player_index().expect("gabarit pilotable");
    let predicted = app.scene.objects[pi].transform.position;
    // Écart largement au-dessus de SNAP_THRESHOLD (0,5 m).
    let authoritative = predicted + Vec3::new(2.0, 0.0, 0.0);

    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: authoritative.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );

    app.apply_local_network_position();

    let after_one_call = app.scene.objects[pi].transform.position;
    assert_ne!(
        after_one_call, authoritative,
        "un seul appel ne doit jamais sauter directement à la valeur autoritative"
    );
    assert!(
        after_one_call.distance(authoritative) > 1.0,
        "un seul appel doit rester proche de la position prédite, pas de la cible : \
             {after_one_call:?}"
    );
}

/// Des appels répétés (sans qu'aucun mouvement local n'intervienne entre
/// eux) doivent faire converger progressivement la position vers la
/// valeur autoritative — le petit pas par appel n'est pas qu'un lissage
/// ponctuel, il finit par combler l'écart.
#[cfg(feature = "net_tests")]
#[test]
fn repeated_calls_gradually_converge_toward_the_authoritative_position() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);

    let pi = app.player_index().expect("gabarit pilotable");
    let predicted = app.scene.objects[pi].transform.position;
    let authoritative = predicted + Vec3::new(2.0, 0.0, 0.0);

    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: authoritative.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );

    let mut previous_distance = predicted.distance(authoritative);
    for _ in 0..30 {
        app.apply_local_network_position();
        let current_distance = app.scene.objects[pi]
            .transform
            .position
            .distance(authoritative);
        assert!(
            current_distance <= previous_distance,
            "chaque appel doit rapprocher (ou laisser inchangé) l'écart, jamais l'agrandir"
        );
        previous_distance = current_distance;
    }
    // La correction s'arrête volontairement dès que l'écart repasse sous
    // `SNAP_THRESHOLD` (cf. `interpolation::reconcile`) : un aller-retour
    // réseau normal ne doit pas produire de micro-saccade perpétuelle.
    // Converge donc jusqu'au seuil, pas jusqu'à zéro.
    assert!(
        previous_distance <= crate::net::interpolation::SNAP_THRESHOLD,
        "après 30 appels, l'écart doit être retombé à SNAP_THRESHOLD ou moins : \
             {previous_distance}"
    );
}

/// La position renvoyée par le serveur date d'une latence + un tick — en
/// pleine course elle est *toujours* en retard par rapport à la
/// prédiction, au-delà de `SNAP_THRESHOLD`. Comparer à la seule position
/// *instantanée* déclencherait donc une traction arrière continue pendant
/// tout déplacement (cf. docs/audits/app-network.md pour le bug réel que
/// ça a causé). Une position serveur **sur notre trajectoire récente**
/// signifie « en phase, juste en retard » : aucune correction ne doit
/// s'appliquer.
#[cfg(feature = "net_tests")]
#[test]
fn a_lagging_server_position_on_our_recent_path_triggers_no_correction() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);

    let pi = app.player_index().expect("gabarit pilotable");
    let start = app.scene.objects[pi].transform.position;
    // 1. Serveur et client d'accord au départ : peuple l'historique avec `start`.
    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: start.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );
    app.apply_local_network_position();

    // 2. Le joueur court 2 m devant (prédiction locale) ; le serveur, en
    // retard d'une latence, renvoie encore la position de départ.
    let ran_to = start + Vec3::new(2.0, 0.0, 0.0);
    app.scene.objects[pi].transform.position = ran_to;
    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: start.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );
    app.apply_local_network_position();

    assert_eq!(
        app.scene.objects[pi].transform.position, ran_to,
        "une position serveur en retard mais sur notre trajectoire ne doit \
             déclencher aucune traction arrière"
    );
}

/// Sous `SNAP_THRESHOLD`, `reconcile` ne corrige volontairement rien —
/// mais le serveur (physique plus ancienne) s'arrête quelques dizaines de
/// cm plus loin que la prédiction locale : sans rattrapage, chaque client
/// garderait un décalage permanent avec la position que les autres voient
/// de lui (cf. docs/audits/app-network.md). Un joueur **immobile** doit
/// converger doucement vers la vérité serveur.
#[cfg(feature = "net_tests")]
#[test]
fn an_idle_player_softly_settles_onto_the_server_position() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);
    // Corps physique présent et au repos (aucun pas simulé) : condition
    // « immobile » du rattrapage.
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

    let pi = app.player_index().expect("gabarit pilotable");
    let start = app.scene.objects[pi].transform.position;
    // Écart *sous* SNAP_THRESHOLD : ignoré par `reconcile`, mais visible à
    // l'écran (~0,3 m) — c'est le cas que le rattrapage doit combler.
    let authoritative = start + Vec3::new(0.3, 0.0, 0.0);
    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: authoritative.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );

    for _ in 0..120 {
        app.apply_local_network_position();
    }
    let dist = app.scene.objects[pi]
        .transform
        .position
        .distance(authoritative);
    assert!(
        dist <= IDLE_SETTLE_MIN + 1e-3,
        "immobile, le joueur doit finir aligné sur la position serveur (écart={dist})"
    );
}

/// Une correction basée sur une cible mémorisée (`from`/`to` figés)
/// écraserait la vraie position, fraîchement avancée par l'input réel, à
/// chaque tick où `Physics::step` (`runtime/physics.rs`) la recopie
/// depuis le corps rigide (cf. docs/audits/app-network.md pour le bug
/// réel que ça a causé). Ce test simule un mouvement local (`sim_step`)
/// entre deux appels de `apply_local_network_position` et vérifie que ce
/// mouvement **n'est jamais écrasé** : la position finale doit refléter à
/// la fois le mouvement local et un petit pas de correction, jamais
/// uniquement l'un ou l'autre.
#[cfg(feature = "net_tests")]
#[test]
fn a_correction_never_discards_local_movement_that_happened_between_calls() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);

    let pi = app.player_index().expect("gabarit pilotable");
    let start = app.scene.objects[pi].transform.position;
    let authoritative = start + Vec3::new(2.0, 0.0, 0.0);

    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: authoritative.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );

    app.apply_local_network_position();

    // Simule ce que ferait `sim_step`/`Physics::step` au tick suivant :
    // avance la position d'un mouvement local, sans rapport avec la
    // correction réseau (ex. le joueur continue d'appuyer sur une touche).
    let local_move = Vec3::new(0.0, 0.0, 5.0);
    app.scene.objects[pi].transform.position += local_move;
    let position_before_second_call = app.scene.objects[pi].transform.position;

    app.apply_local_network_position();

    let position_after_second_call = app.scene.objects[pi].transform.position;
    // Avec l'ancien bug, `position_after_second_call` aurait été calculée
    // à partir de `from`/`to` figés (sans rapport avec `local_move`) —
    // ici, elle doit rester proche de `position_before_second_call` (un
    // simple petit pas de correction par-dessus), pas revenir à une
    // valeur qui ignore le mouvement local qui vient d'avoir lieu.
    assert!(
        position_after_second_call.distance(position_before_second_call) < 1.0,
        "le mouvement local entre les deux appels ne doit pas être écrasé par la \
             correction réseau : avant={position_before_second_call:?}, \
             après={position_after_second_call:?}"
    );
}

/// Avec la physique réellement construite (`Physics::build`), une
/// correction qui n'écrit que dans `transform.position` sans passer par
/// `Physics::set_position` est effacée dès le tick physique suivant —
/// `Physics::step` recopie la pose du corps rigide (resté inchangé)
/// par-dessus (cf. docs/audits/app-network.md, `SPRINTNETWORK.md`). Ce
/// test simule exactement cette séquence (correction, puis un tick
/// physique, comme le ferait `advance_play` à la frame suivante) et
/// vérifie que la correction **survit**.
#[cfg(feature = "net_tests")]
#[test]
fn a_local_position_correction_survives_the_next_physics_step() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

    let pi = app.player_index().expect("gabarit pilotable");
    let predicted = app.scene.objects[pi].transform.position;
    let authoritative = predicted + Vec3::new(2.0, 0.0, 0.0);

    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: authoritative.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );

    app.apply_local_network_position();
    assert_ne!(
        app.scene.objects[pi].transform.position, predicted,
        "la correction doit avoir déplacé la position affichée"
    );

    // Simule ce que ferait le tick physique suivant (`sim_step`/`Physics::
    // step`, appelé avant `apply_local_network_position` en temps normal,
    // cf. `advance_play`) : sans `Physics::set_position`, cette étape
    // aurait ramené la position exactement à `predicted` (le corps
    // rigide, jamais informé de la correction) — cf. docs/audits/app-network.md.
    app.physics
        .as_mut()
        .expect("construite ci-dessus")
        .step(1.0 / 60.0, &mut app.scene);

    let after_physics_step = app.scene.objects[pi].transform.position;
    assert!(
        after_physics_step.distance(predicted) > 0.1,
        "la correction ne doit pas être effacée par le tick physique suivant : \
             position retombée à {after_physics_step:?} (départ {predicted:?})"
    );
}

/// Sprint 103c (audit réseau après la migration du joueur vers
/// `KinematicCharacterController`, Sprint 103b) : un escalier fait
/// bouger le joueur verticalement de façon quasi instantanée
/// (`autostep`, jusqu'à `PLAYER_AUTOSTEP_HEIGHT` = 0,3 m par marche) —
/// un axe de mouvement que l'ancien corps dynamique n'avait jamais
/// (sol toujours plat). `on_recent_path`/`reconcile`
/// (`interpolation::SNAP_THRESHOLD` = 0,5 m) comparent des positions
/// **3D** : une bosse verticale d'autostep pourrait, combinée à un
/// léger décalage latéral, franchir le seuil et déclencher une
/// correction parasite pile en montant. Ce test fait grimper un
/// escalier de 4 marches (mêmes constantes que
/// `physics::tests::kinematic_player_climbs_a_low_staircase`, dupliquées
/// ici car ce test-là est privé à `physics.rs`) tout en simulant un
/// serveur légèrement en retard (position d'il y a quelques ticks,
/// « en phase, juste en retard », comme
/// `a_lagging_server_position_on_our_recent_path_triggers_no_correction`
/// ci-dessus) — aucun saut de correction ne doit se produire du seul
/// fait de grimper les marches.
#[cfg(feature = "net_tests")]
#[test]
fn climbing_stairs_does_not_trigger_a_spurious_correction() {
    const STEP_RISE: f32 = 0.2;
    const STEP_DEPTH: f32 = 0.6;
    const STEPS: i32 = 4;
    const START: Vec3 = Vec3::new(100.0, 1.0, -1.5);

    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);
    let pi = app.player_index().expect("gabarit pilotable");

    // Loin du reste de la scène (x=100) pour ne heurter aucun décor de la
    // démo zombies — seul l'escalier ajouté ici doit influencer le test.
    app.scene.objects[pi].transform.position = START;
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Sol (test)".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(100.0, -0.1, -1.5))
            .with_scale(Vec3::new(4.0, 0.2, 3.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    });
    for k in 0..STEPS {
        let top = STEP_RISE * (k + 1) as f32;
        app.scene.objects.push(crate::scene::SceneObject {
            name: format!("Marche (test) {k}"),
            mesh: crate::scene::MeshKind::Cube,
            transform: crate::scene::Transform::from_pos(Vec3::new(
                100.0,
                top * 0.5,
                (k as f32 + 0.5) * STEP_DEPTH,
            ))
            .with_scale(Vec3::new(4.0, top, STEP_DEPTH)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
    }
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

    // 130 pas (~2,2 s) : le joueur atteint le sommet de l'escalier sans le
    // dépasser — au-delà, il marcherait dans le vide derrière la dernière
    // marche (aucun palier dans cette scène de test), ce qui n'est pas ce
    // que ce test vérifie (cf. le palier ajouté pour la même raison dans
    // `physics::tests::kinematic_player_climbs_a_low_staircase`).
    let dt = 1.0 / 60.0;
    let mut recent: std::collections::VecDeque<Vec3> = std::collections::VecDeque::new();
    let mut max_jump = 0.0_f32;
    for _ in 0..130 {
        app.physics
            .as_mut()
            .expect("construite ci-dessus")
            .control(pi, 0.0, 2.0, false, 0.0, 0.0, dt);
        app.physics
            .as_mut()
            .expect("construite ci-dessus")
            .step(dt, &mut app.scene);

        // « Serveur » simulé en retard de quelques ticks (~100 ms) : la
        // position injectée est celle d'un passé récent, jamais
        // l'instantanée — comme un aller-retour réseau normal.
        recent.push_back(app.scene.objects[pi].transform.position);
        if recent.len() > 6 {
            recent.pop_front();
        }
        let lagging = *recent.front().unwrap();
        app.net_local_interp.push(
            EntityDelta {
                index: pi as u32,
                player_id: None,
                position: lagging.to_array(),
                yaw: 0.0,
                visible: true,
                health: None,
                anim_clip: String::new(),
                kills: None,
                assists: None,
            },
            Instant::now(),
        );

        let before = app.scene.objects[pi].transform.position;
        app.apply_local_network_position();
        let after = app.scene.objects[pi].transform.position;
        max_jump = max_jump.max(before.distance(after));
    }

    assert!(
        max_jump < crate::net::interpolation::SNAP_THRESHOLD,
        "grimper un escalier ne doit déclencher aucune correction \
             parasite (saut maximal observé : {max_jump} m)"
    );
    assert!(
        app.scene.objects[pi].transform.position.y > STEP_RISE * (STEPS as f32 - 1.5),
        "le joueur doit avoir réellement grimpé l'escalier pendant le test \
             (y={})",
        app.scene.objects[pi].transform.position.y
    );
}

/// Sprint 103c : `Physics::velocity` (Sprint 103b) dérive désormais la
/// vitesse horizontale du mouvement **réel** post-collision de
/// `move_shape`, pas d'un `linvel` rapier — un joueur qui pousse contre
/// un mur en tenant l'entrée a donc une vitesse quasi nulle, comme un
/// joueur réellement immobile. Ce test vérifie que le rattrapage doux à
/// l'arrêt (`IDLE_SETTLE_PULL`) continue de fonctionner dans ce cas :
/// un joueur bloqué contre un mur est, en pratique, immobile dans le
/// monde — le rattrapage doit s'appliquer comme pour tout autre arrêt.
#[cfg(feature = "net_tests")]
#[test]
fn a_wall_blocked_player_settles_without_fighting_the_correction() {
    const START: Vec3 = Vec3::new(200.0, 1.0, 0.0);

    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);
    let pi = app.player_index().expect("gabarit pilotable");

    app.scene.objects[pi].transform.position = START;
    // Sol sous le joueur — sans lui, il tombe en chute libre et sa
    // vitesse mesurée reflète la chute, pas le blocage contre le mur.
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Sol (test)".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(200.0, -0.1, 0.0))
            .with_scale(Vec3::new(4.0, 0.2, 4.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    });
    // Mur juste devant (au sens +Z, la direction poussée ci-dessous).
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Mur (test)".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(200.0, 1.0, 1.0))
            .with_scale(Vec3::new(4.0, 3.0, 0.2)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    });
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

    let dt = 1.0 / 60.0;
    // Pousse contre le mur pendant assez de temps pour s'y presser
    // (vitesse résultante quasi nulle après `move_shape`).
    for _ in 0..60 {
        app.physics
            .as_mut()
            .expect("construite ci-dessus")
            .control(pi, 0.0, 4.0, false, 0.0, 0.0, dt);
        app.physics
            .as_mut()
            .expect("construite ci-dessus")
            .step(dt, &mut app.scene);
    }
    let blocked_speed = app
        .physics
        .as_ref()
        .expect("construite ci-dessus")
        .velocity(pi)
        .expect("le joueur est un corps piloté")
        .length();
    assert!(
        blocked_speed < IDLE_SPEED_EPSILON,
        "un joueur pressé contre un mur doit être mesuré comme quasi \
             immobile (vitesse={blocked_speed})"
    );

    let stuck_at = app.scene.objects[pi].transform.position;
    // Écart **latéral** (X), pas vers l'arrière (-Z) : l'entrée tenue
    // pousse continuellement vers +Z contre le mur — un écart cible
    // situé plus loin dans -Z serait immédiatement défait par cette
    // même entrée à chaque tick (le joueur ne peut physiquement pas
    // reculer tout en poussant en avant), ce qui n'a rien d'un bug de
    // réconciliation. Sous `SNAP_THRESHOLD` (ignoré par `reconcile`),
    // comme dans `an_idle_player_softly_settles_onto_the_server_position`.
    let authoritative = stuck_at + Vec3::new(0.3, 0.0, 0.0);
    app.net_local_interp.push(
        EntityDelta {
            index: pi as u32,
            player_id: None,
            position: authoritative.to_array(),
            yaw: 0.0,
            visible: true,
            health: None,
            anim_clip: String::new(),
            kills: None,
            assists: None,
        },
        Instant::now(),
    );
    for _ in 0..120 {
        // Le joueur continue de tenir l'entrée contre le mur pendant le
        // rattrapage — comme un joueur réel qui n'a pas relâché le stick.
        app.physics
            .as_mut()
            .expect("construite ci-dessus")
            .control(pi, 0.0, 4.0, false, 0.0, 0.0, dt);
        app.physics
            .as_mut()
            .expect("construite ci-dessus")
            .step(dt, &mut app.scene);
        app.apply_local_network_position();
    }
    let dist = app.scene.objects[pi]
        .transform
        .position
        .distance(authoritative);
    assert!(
        dist <= IDLE_SETTLE_MIN + 1e-2,
        "un joueur bloqué contre un mur doit quand même converger vers \
             la position serveur (écart={dist})"
    );
}

/// cf. `SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.2 :
/// `poll_network` est appelée une fois par frame de rendu, potentiellement
/// bien plus souvent que le tick serveur — sans plafond, un client sur un
/// écran rapide enverrait un `Input` par frame affichée. Ce test appelle
/// `poll_network` en boucle serrée (sans dormir entre les appels, donc
/// bien plus vite que `INPUT_SEND_INTERVAL`) et vérifie que le serveur ne
/// reçoit qu'une poignée d'`Input`, pas un par appel.
#[cfg(feature = "net_tests")]
#[test]
fn input_send_rate_is_capped_regardless_of_poll_network_call_rate() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let mut app = connected_app_with_a_player(&net);

    // Vide le `Join` initial, seul le décompte des `Input` nous intéresse.
    let _ = net.inbox.recv_timeout(Duration::from_secs(2));

    const CALLS: usize = 500;
    for _ in 0..CALLS {
        app.poll_network();
    }

    let mut input_count = 0;
    while net.inbox.try_recv().is_ok() {
        input_count += 1;
    }

    assert!(
        input_count < CALLS / 10,
        "500 appels serrés à poll_network ne doivent envoyer qu'une poignée d'Input, \
             pas un par appel (reçus : {input_count})"
    );
}

#[test]
fn network_move_axes_includes_keyboard_thrust_along_the_players_own_yaw() {
    // Sans le yaw du joueur, W/S ne produit aucune direction à envoyer au
    // serveur. La composante `move_y` doit suivre la convention *joystick*
    // (`move_y` positif = avant, le serveur applique `vz = -move_y ×
    // vitesse`, cf. `sim_step`), pas le Z monde — les confondre ferait
    // prédire W vers l'avant en local et simuler vers l'arrière côté
    // serveur, la réconciliation tirant alors le joueur à contresens en
    // pleine course (cf. docs/audits/app-network.md).
    let inp = super::super::PlayerInput {
        key_thrust: 1.0,
        ..Default::default()
    };
    let yaw = 0.0_f32; // face à -Z (cf. `Physics::face_direction`)
    let (mx, my) = network_move_axes(&inp, 0.0, Some(yaw));
    assert!(
        (mx - 0.0).abs() < 1e-5 && (my - 1.0).abs() < 1e-5,
        "avancer (W) à yaw=0 doit s'envoyer comme move_y=+1 (avant, convention \
             joystick) : ({mx}, {my})"
    );
    // Vérité de bout en bout : la convention serveur (`vz = -move_y`) doit
    // redonner exactement la vitesse que la prédiction locale applique
    // (`vz = thrust × -cos(yaw)`), pour tout yaw — sinon les deux simulations
    // divergent et la réconciliation se met à « corriger » un joueur sain.
    for yaw in [0.0_f32, 0.9, -2.3, std::f32::consts::PI] {
        let (mx, my) = network_move_axes(&inp, 0.0, Some(yaw));
        let (server_vx, server_vz) = (mx, -my);
        let (local_vx, local_vz) = (-yaw.sin(), -yaw.cos());
        assert!(
            (server_vx - local_vx).abs() < 1e-5 && (server_vz - local_vz).abs() < 1e-5,
            "yaw={yaw} : serveur ({server_vx}, {server_vz}) ≠ local ({local_vx}, {local_vz})"
        );
    }
}

#[test]
fn network_input_msg_sends_touch_jump_attack_and_gyro_like_local_prediction() {
    // Le message réseau doit inclure les boutons tactiles nommés et le
    // gyroscope, pas seulement le clavier (`inp.jump`/`inp.attack`) : sur
    // APK, le bouton tactile « Saut »/« Attaque » et le gyroscope pilotent
    // la prédiction locale mais resteraient invisibles pour le serveur
    // s'ils n'étaient pas transmis ici aussi (cf. docs/audits/app-network.md).
    let obj = crate::scene::SceneObject {
        controller: Some(crate::scene::Controller {
            input: true,
            gyro: true,
            jump_button: "Saut".into(),
            attack_button: "Attaque".into(),
            fire_button: "Feu".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut inp = super::super::PlayerInput {
        tilt: (0.3, 0.6),
        ..Default::default()
    };
    inp.buttons.insert("Saut".into());
    inp.buttons.insert("Attaque".into());
    inp.buttons.insert("Feu".into());
    let msg = network_input_msg(&inp, 0.0, Some(&obj), 1);
    let crate::net::protocol::ClientMsg::Input {
        move_x,
        move_y,
        attack,
        jump,
        fire,
        weapon,
        ..
    } = msg
    else {
        panic!("network_input_msg doit produire un ClientMsg::Input");
    };
    assert!(jump, "le bouton tactile Saut doit être transmis au serveur");
    assert!(
        attack,
        "le bouton tactile Attaque doit être transmis au serveur"
    );
    assert!(
        fire,
        "le bouton tactile Feu (boule de feu) doit être transmis au serveur"
    );
    assert_eq!(weapon, 1, "l'arme sélectionnée doit partir avec l'Input");
    // Gyroscope en convention joystick : le serveur applique `vz = -move_y`,
    // la prédiction locale `vz = -tilt.1` — donc move_y = tilt.1 tel quel.
    assert!(
        (move_x - 0.3).abs() < 1e-5 && (move_y - 0.6).abs() < 1e-5,
        "l'inclinaison gyro doit partir dans les axes envoyés : ({move_x}, {move_y})"
    );
}

#[test]
fn network_input_msg_ignores_gyro_and_buttons_the_controller_does_not_use() {
    // Un objet joueur sans `gyro` et sans boutons nommés : l'inclinaison
    // résiduelle du capteur et des boutons pressés par hasard ne doivent pas
    // partir au serveur (le local les ignore aussi — mêmes sources des deux côtés).
    let obj = crate::scene::SceneObject {
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut inp = super::super::PlayerInput {
        tilt: (0.9, -0.4),
        ..Default::default()
    };
    inp.buttons.insert("Saut".into());
    inp.buttons.insert("Feu".into());
    let msg = network_input_msg(&inp, 0.0, Some(&obj), 0);
    let crate::net::protocol::ClientMsg::Input {
        move_x,
        move_y,
        attack,
        jump,
        fire,
        ..
    } = msg
    else {
        panic!("network_input_msg doit produire un ClientMsg::Input");
    };
    assert!(!jump && !attack && !fire);
    assert_eq!((move_x, move_y), (0.0, 0.0));
}

#[test]
fn network_move_axes_includes_touch_pad_thrust_like_the_keyboard() {
    // Le pavé tactile W/A/S/D (APK) doit être vu par le serveur exactement
    // comme le W/S clavier : même conversion via l'orientation du joueur.
    let inp = super::super::PlayerInput {
        touch_thrust: 1.0,
        ..Default::default()
    };
    let (mx, my) = network_move_axes(&inp, 0.0, Some(0.0));
    assert!(
        mx.abs() < 1e-5 && (my - 1.0).abs() < 1e-5,
        "W tactile à yaw=0 doit s'envoyer comme move_y=+1 : ({mx}, {my})"
    );
}

#[test]
fn network_move_axes_is_neutral_without_thrust_or_camera_input() {
    let inp = super::super::PlayerInput::default();
    let (mx, my) = network_move_axes(&inp, 0.7, Some(1.2));
    assert_eq!((mx, my), (0.0, 0.0));
}

#[test]
fn network_move_axes_ignores_thrust_when_player_yaw_is_unknown() {
    // Objet sans corps physique/pas encore construit (`player_object` renvoie
    // `None`) : ne doit pas paniquer, simplement ignorer la composante clavier.
    let inp = super::super::PlayerInput {
        key_thrust: 1.0,
        ..Default::default()
    };
    let (mx, my) = network_move_axes(&inp, 0.0, None);
    assert_eq!((mx, my), (0.0, 0.0));
}

/// Backoff de reconnexion : croît en 1 s → 2 s → 4 s → 8 s puis plafonne à
/// 15 s, sans jamais paniquer sur une entrée dégénérée (tentative 0).
#[test]
fn reconnect_delay_grows_and_caps() {
    use std::time::Duration as D;
    assert_eq!(reconnect_delay(1), D::from_secs(1));
    assert_eq!(reconnect_delay(2), D::from_secs(2));
    assert_eq!(reconnect_delay(3), D::from_secs(4));
    assert_eq!(reconnect_delay(4), D::from_secs(8));
    assert_eq!(reconnect_delay(5), D::from_secs(15));
    assert_eq!(reconnect_delay(50), D::from_secs(15));
    // Défensif : `attempt` est 1-indexée, mais un 0 accidentel ne doit ni
    // paniquer ni produire un délai nul.
    assert_eq!(reconnect_delay(0), D::from_secs(1));
}

/// La machine à états de reconnexion compte ses tentatives puis abandonne
/// après `MAX_RECONNECT_ATTEMPTS`, avec un `net_status` qui l'explique —
/// jamais de martèlement infini contre un serveur mort.
#[test]
fn reconnection_gives_up_after_max_attempts_and_says_so() {
    let mut app = AppState::new();
    app.net_last_connect = Some((
        "ws://127.0.0.1:9".to_string(),
        "Testeur".to_string(),
        crate::net::protocol::DEFAULT_LOBBY.to_string(),
        0,
        0,
    ));
    for expected in 1..=MAX_RECONNECT_ATTEMPTS {
        app.schedule_reconnect_attempt();
        assert_eq!(
            app.net_reconnect.as_ref().map(|r| r.attempt),
            Some(expected),
            "tentative {expected} attendue"
        );
        assert_eq!(
            app.net_connection_state(),
            NetConnState::Reconnecting { attempt: expected }
        );
    }
    app.schedule_reconnect_attempt();
    assert!(app.net_reconnect.is_none(), "abandon attendu après le max");
    assert!(
        app.net_status.contains("échouée"),
        "le statut doit expliquer l'abandon : {}",
        app.net_status
    );
    assert!(
        app.net_last_connect.is_none(),
        "plus rien à rejouer après l'abandon"
    );
}

/// Une déconnexion **volontaire** annule toute reconnexion automatique :
/// quitter la partie ne doit jamais voir le client se reconnecter tout seul.
#[test]
fn voluntary_disconnect_cancels_any_pending_reconnection() {
    let mut app = AppState::new();
    app.net_last_connect = Some((
        "ws://127.0.0.1:9".to_string(),
        "Testeur".to_string(),
        crate::net::protocol::DEFAULT_LOBBY.to_string(),
        0,
        0,
    ));
    app.schedule_reconnect_attempt();
    assert!(app.net_reconnect.is_some());

    app.disconnect_from_server();
    assert!(app.net_reconnect.is_none());
    assert!(app.net_last_connect.is_none());
    assert_eq!(app.net_connection_state(), NetConnState::Offline);

    // Et `poll_network` ne relance rien dans le dos du joueur.
    app.poll_network();
    assert!(app.net_client.is_none() && app.net_reconnect.is_none());
}

/// Bout-en-bout de la reconnexion automatique : le serveur coupe la
/// connexion (`NetServer::disconnect`, même chemin qu'une perte réseau
/// réelle), le client doit le détecter, se reconnecter au terme du backoff
/// (1 s pour la première tentative) et recevoir un **nouveau** `Welcome`
/// avec un nouvel identifiant — preuve que `is_connected()` redevient vrai
/// sans aucune action du joueur.
#[cfg(feature = "net_tests")]
#[test]
fn the_client_reconnects_and_rejoins_after_a_lost_connection() {
    let net = NetServer::start("127.0.0.1:0").expect("démarrage du serveur");
    let url = format!("ws://{}", net.local_addr);

    let mut app = AppState::new();
    app.connect_to_server(&url, "Testeur");

    // Welcome initial (le `NetServer` l'envoie lui-même au Join).
    let deadline = Instant::now() + Duration::from_secs(2);
    let first_id = loop {
        app.poll_network();
        if let Some(id) = app.net_player_id {
            break id;
        }
        assert!(
            Instant::now() < deadline,
            "Welcome initial attendu sous 2 s"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(app.is_connected());

    net.disconnect(first_id);

    // Détection + backoff (1 s) + nouvelle poignée de main + nouveau Welcome.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_network();
        if let Some(id) = app.net_player_id
            && id != first_id
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "reconnexion attendue sous 5 s (statut : {})",
            app.net_status
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(app.is_connected(), "la connexion doit être redevenue saine");
    assert!(
        app.net_reconnect.is_none(),
        "le Welcome doit solder la reconnexion"
    );
}
