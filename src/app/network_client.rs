//! Client réseau desktop (SPRINT_MMORPG.md) : connecte l'éditeur/le player à un
//! serveur RusteeGear (`src/bin/server.rs`) pour jouer à plusieurs. Desktop
//! uniquement — sur mobile, les mêmes méthodes existent mais renvoient une
//! erreur (même convention que `app::ai`, qui a la même contrainte : `net::client`
//! dépend de `tokio`, absent des cibles mobiles, cf. `net/mod.rs`).
//!
//! Le joueur local reste **piloté par prédiction**, exactement comme en solo
//! (`sim_step` ne change pas) : ce module se contente d'envoyer son `Input` au
//! serveur, et d'afficher les *autres* joueurs reçus par `Snapshot` comme des
//! objets « fantômes » — sans physique ni script, leur position suit le
//! dernier `Snapshot` reçu, interpolée (cf. `net::interpolation::RemoteEntity`,
//! Sprint 54).

use super::AppState;

/// Un autre joueur réseau, affiché comme un objet fantôme dans la scène locale.
pub struct RemotePlayer {
    pub name: String,
    scene_index: usize,
    interp: crate::net::interpolation::RemoteEntity,
}

/// Message de chat affichable — représentation universelle (contrairement à
/// `net::firebase::ChatMessage`, absent des cibles mobiles) : permet à
/// `AppState::chat_messages` de rester un champ normal, sans avoir besoin
/// d'être gaté par plateforme.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatLine {
    pub sender: String,
    pub text: String,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl AppState {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) sous `name`.
    /// Remplace une connexion existante s'il y en avait une. Transmet
    /// `self.firebase_uid` au serveur s'il est connu (cf. `sign_in`/`sign_up`
    /// ci-dessous) — `None` pour une partie anonyme.
    pub fn connect_to_server(&mut self, url: &str, name: &str) {
        self.disconnect_from_server();
        match crate::net::client::NetClient::connect(url, name, self.firebase_uid.as_deref()) {
            Ok(client) => {
                log::info!("Multijoueur : connecté à {url} sous « {name} »");
                self.net_client = Some(client);
                self.net_status = format!("Connexion à {url}…");
            }
            Err(e) => {
                log::warn!("Multijoueur : connexion à {url} échouée : {e}");
                self.net_status = format!("Connexion échouée : {e}");
            }
        }
    }

    /// `true` si un compte Firebase est associé à cette session (cf.
    /// `sign_in`/`sign_up`).
    pub fn has_firebase_account(&self) -> bool {
        self.firebase_uid.is_some()
    }

    /// Se connecte à un compte Firebase existant (thread de fond, comme les
    /// requêtes IA déjà présentes) : au succès, `self.firebase_uid` est
    /// renseigné et transmis au prochain `connect_to_server`. Sans effet si
    /// une requête est déjà en cours.
    pub fn request_firebase_sign_in(
        &mut self,
        api_key: String,
        database_url: String,
        email: String,
        password: String,
    ) {
        self.request_firebase_auth(api_key, database_url, email, password, false);
    }

    /// Crée un compte Firebase puis s'y connecte. Mêmes garanties que
    /// `request_firebase_sign_in`.
    pub fn request_firebase_sign_up(
        &mut self,
        api_key: String,
        database_url: String,
        email: String,
        password: String,
    ) {
        self.request_firebase_auth(api_key, database_url, email, password, true);
    }

    fn request_firebase_auth(
        &mut self,
        api_key: String,
        database_url: String,
        email: String,
        password: String,
        sign_up: bool,
    ) {
        if self.firebase_busy {
            return;
        }
        self.firebase_busy = true;
        self.net_status = "Connexion à Firebase…".to_string();
        let tx = self.firebase_tx.clone();
        std::thread::spawn(move || {
            let config = crate::net::firebase::FirebaseConfig {
                api_key,
                database_url,
            };
            let result = if sign_up {
                crate::net::firebase::sign_up(&config, &email, &password)
            } else {
                crate::net::firebase::sign_in(&config, &email, &password)
            };
            let _ = tx.send(result.map(|session| (session.uid, session.id_token)));
        });
    }

    /// Applique le résultat d'une requête Firebase en attente, s'il y en a un
    /// (appelé depuis `poll_network`, non bloquant).
    fn poll_firebase(&mut self) {
        while let Ok(result) = self.firebase_rx.try_recv() {
            self.firebase_busy = false;
            match result {
                Ok((uid, id_token)) => {
                    log::info!("Firebase : connecté (uid {uid})");
                    self.net_status = format!("Connecté à Firebase (uid {uid})");
                    self.firebase_uid = Some(uid);
                    self.firebase_id_token = Some(id_token);
                }
                Err(e) => {
                    log::warn!("Firebase : connexion échouée : {e}");
                    self.net_status = format!("Connexion Firebase échouée : {e}");
                }
            }
        }
    }

    /// Poste un message dans le chat du salon `lobby_code` (thread de fond),
    /// puis rafraîchit la liste. Nécessite un compte connecté (`sign_in`/
    /// `sign_up`) : les règles RTDB réservent l'écriture aux comptes
    /// authentifiés (cf. `net::firebase`, Sprint 58). Sans effet si non
    /// connecté à un compte, ou si une requête de chat est déjà en cours.
    pub fn request_send_chat_message(
        &mut self,
        api_key: String,
        database_url: String,
        lobby_code: String,
        sender_name: String,
        text: String,
    ) {
        let Some(id_token) = self.firebase_id_token.clone() else {
            self.net_status = "Connecte-toi d'abord à un compte pour discuter".to_string();
            return;
        };
        if self.chat_busy || text.trim().is_empty() {
            return;
        }
        self.chat_busy = true;
        let tx = self.chat_tx.clone();
        std::thread::spawn(move || {
            let config = crate::net::firebase::FirebaseConfig {
                api_key,
                database_url,
            };
            let sent_at_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let message = crate::net::firebase::ChatMessage {
                sender: sender_name,
                text,
                sent_at_ms,
            };
            if let Err(e) =
                crate::net::firebase::post_chat_message(&config, &lobby_code, &id_token, &message)
            {
                log::warn!("Chat : envoi échoué : {e}");
            }
            let result = fetch_chat_lines(&config, &lobby_code);
            let _ = tx.send(result);
        });
    }

    /// Rafraîchit la liste des messages du salon `lobby_code` (lecture
    /// publique, ne nécessite pas de compte connecté). Sans effet si une
    /// requête de chat est déjà en cours.
    pub fn request_refresh_chat(
        &mut self,
        api_key: String,
        database_url: String,
        lobby_code: String,
    ) {
        if self.chat_busy {
            return;
        }
        self.chat_busy = true;
        let tx = self.chat_tx.clone();
        std::thread::spawn(move || {
            let config = crate::net::firebase::FirebaseConfig {
                api_key,
                database_url,
            };
            let _ = tx.send(fetch_chat_lines(&config, &lobby_code));
        });
    }

    /// Applique le résultat d'une requête de chat en attente, s'il y en a un.
    fn poll_chat(&mut self) {
        while let Ok(result) = self.chat_rx.try_recv() {
            self.chat_busy = false;
            match result {
                Ok(lines) => self.chat_messages = lines,
                Err(e) => log::warn!("Chat : requête échouée : {e}"),
            }
        }
    }

    /// Quitte la partie en ligne (sans effet si non connecté) : prévient le
    /// serveur, masque les fantômes des autres joueurs.
    pub fn disconnect_from_server(&mut self) {
        if let Some(client) = &self.net_client {
            client.send(&crate::net::protocol::ClientMsg::Leave);
        }
        self.net_client = None;
        self.net_player_id = None;
        for rp in self.remote_players.values() {
            if let Some(o) = self.scene.objects.get_mut(rp.scene_index) {
                o.visible = false;
            }
        }
        self.remote_players.clear();
        self.net_status = "Déconnecté".to_string();
    }

    /// `true` si une connexion au serveur est active.
    pub fn is_connected(&self) -> bool {
        self.net_client.is_some()
    }

    /// Appelé une fois par frame depuis `advance_play` : envoie l'input local,
    /// draine les messages serveur, met à jour les fantômes des autres joueurs.
    pub(super) fn poll_network(&mut self) {
        self.poll_firebase();
        self.poll_chat();
        if self.net_client.is_none() {
            return;
        }

        // Envoie l'input courant du joueur local (même calcul que dans
        // `sim_step` pour son propre pilotage) — le serveur en a besoin pour
        // faire bouger *notre* objet côté autres clients.
        let inp = &self.input_state;
        let input = crate::net::protocol::ClientMsg::Input {
            move_x: (inp.joy.0 + inp.key_move.0).clamp(-1.0, 1.0),
            move_y: (inp.joy.1 + inp.key_move.1).clamp(-1.0, 1.0),
            attack: inp.attack,
            jump: inp.jump,
        };
        if let Some(client) = &self.net_client {
            client.send(&input);
        }

        let messages: Vec<crate::net::protocol::ServerMsg> = match &self.net_client {
            Some(client) => client.inbox.try_iter().collect(),
            None => Vec::new(),
        };
        for msg in messages {
            self.handle_server_msg(msg);
        }

        let now = std::time::Instant::now();
        for rp in self.remote_players.values_mut() {
            if let Some((pos, yaw, visible)) = rp.interp.sample(now)
                && let Some(o) = self.scene.objects.get_mut(rp.scene_index)
            {
                o.transform.position = pos;
                o.transform.rotation = glam::Quat::from_rotation_y(yaw);
                o.visible = visible;
            }
        }
    }

    fn handle_server_msg(&mut self, msg: crate::net::protocol::ServerMsg) {
        use crate::net::protocol::ServerMsg;
        match msg {
            ServerMsg::Welcome { player_id } => {
                log::info!("Multijoueur : bienvenue, joueur {player_id}");
                self.net_player_id = Some(player_id);
                self.net_status = format!("Connecté (joueur {player_id})");
            }
            ServerMsg::PlayerJoined { player_id, name } => {
                if Some(player_id) != self.net_player_id {
                    log::info!("Multijoueur : « {name} » (joueur {player_id}) a rejoint");
                    self.ensure_remote_player(player_id, &name);
                }
            }
            ServerMsg::PlayerLeft { player_id } => {
                log::info!("Multijoueur : joueur {player_id} est parti");
                if let Some(rp) = self.remote_players.remove(&player_id)
                    && let Some(o) = self.scene.objects.get_mut(rp.scene_index)
                {
                    o.visible = false;
                }
            }
            ServerMsg::Snapshot(snap) => {
                let now = std::time::Instant::now();
                for e in snap.entities {
                    let Some(pid) = e.player_id else { continue };
                    // Notre propre joueur : piloté en local par prédiction,
                    // jamais écrasé par le snapshot serveur (cf. la doc du
                    // module — pas de réconciliation implémentée pour
                    // l'instant, cf. SPRINT_MMORPG.md Sprint 54).
                    if Some(pid) == self.net_player_id {
                        continue;
                    }
                    let default_name = format!("Joueur {pid}");
                    let rp = self.ensure_remote_player(pid, &default_name);
                    rp.interp.push(e, now);
                }
            }
            ServerMsg::Event(_) => {}
        }
    }

    /// Renvoie le fantôme du joueur `id`, en le créant s'il n'existe pas
    /// encore : clone du gabarit pilotable local, mais sans contrôleur ni
    /// physique (affichage seul — le serveur est autoritaire sur sa position,
    /// appliquée directement au `transform`, pas simulée localement).
    fn ensure_remote_player(
        &mut self,
        id: crate::net::protocol::PlayerId,
        name: &str,
    ) -> &mut RemotePlayer {
        if !self.remote_players.contains_key(&id) {
            let template = self
                .scene
                .objects
                .iter()
                .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
                .cloned()
                .unwrap_or_default();
            let ghost = crate::scene::SceneObject {
                name: format!("Joueur réseau {name}"),
                controller: None,
                physics: crate::runtime::physics::PhysicsKind::None,
                // Masqué tant qu'aucun `Snapshot` n'est arrivé pour ce joueur.
                visible: false,
                ..template
            };
            let scene_index = self.scene.objects.len();
            self.scene.objects.push(ghost);
            self.remote_players.insert(
                id,
                RemotePlayer {
                    name: name.to_string(),
                    scene_index,
                    interp: crate::net::interpolation::RemoteEntity::default(),
                },
            );
        }
        self.remote_players
            .get_mut(&id)
            .expect("vient d'être inséré juste au-dessus")
    }
}

/// Récupère les messages d'un salon et les convertit en `ChatLine`
/// (représentation universelle, cf. sa doc).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn fetch_chat_lines(
    config: &crate::net::firebase::FirebaseConfig,
    lobby_code: &str,
) -> Result<Vec<ChatLine>, String> {
    crate::net::firebase::list_chat_messages(config, lobby_code).map(|messages| {
        messages
            .into_iter()
            .map(|m| ChatLine {
                sender: m.sender,
                text: m.text,
            })
            .collect()
    })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl AppState {
    pub fn connect_to_server(&mut self, _url: &str, _name: &str) {
        self.net_status = "Multijoueur indisponible sur mobile".to_string();
    }

    pub fn disconnect_from_server(&mut self) {}

    pub fn is_connected(&self) -> bool {
        false
    }

    pub fn has_firebase_account(&self) -> bool {
        false
    }

    pub fn request_firebase_sign_in(
        &mut self,
        _api_key: String,
        _database_url: String,
        _email: String,
        _password: String,
    ) {
        self.net_status = "Firebase indisponible sur mobile".to_string();
    }

    pub fn request_firebase_sign_up(
        &mut self,
        _api_key: String,
        _database_url: String,
        _email: String,
        _password: String,
    ) {
        self.net_status = "Firebase indisponible sur mobile".to_string();
    }

    pub fn request_send_chat_message(
        &mut self,
        _api_key: String,
        _database_url: String,
        _lobby_code: String,
        _sender_name: String,
        _text: String,
    ) {
    }

    pub fn request_refresh_chat(
        &mut self,
        _api_key: String,
        _database_url: String,
        _lobby_code: String,
    ) {
    }

    pub(super) fn poll_network(&mut self) {}
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::app::multiplayer::NetworkInput;
    use crate::net::protocol::{ClientMsg, ServerMsg};
    use crate::net::server_loop::NetServer;

    /// Fait progresser le "serveur de test" d'un tick : traite les messages en
    /// attente (Join/Input/Leave, cf. `src/bin/server.rs`), simule, diffuse un
    /// `Snapshot`. Retourne le numéro de tick utilisé.
    fn server_tick(server_app: &mut AppState, net: &NetServer, tick: u32) {
        while let Ok((id, msg)) = net.inbox.try_recv() {
            match msg {
                ClientMsg::Join { .. } => {
                    server_app.spawn_network_player(id);
                }
                ClientMsg::Input {
                    move_x,
                    move_y,
                    attack,
                    jump,
                } => {
                    server_app.set_network_input(
                        id,
                        NetworkInput {
                            move_x,
                            move_y,
                            attack,
                            jump,
                        },
                    );
                }
                ClientMsg::Leave => {
                    server_app.despawn_network_player(id);
                }
            }
        }
        server_app.advance_play();
        net.broadcast(&ServerMsg::Snapshot(server_app.network_snapshot(tick)));
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

    /// Une fois un `uid` Firebase connu, `connect_to_server` doit le
    /// transmettre au `Join` — c'est ce qui permet au serveur de créditer la
    /// bonne progression (Sprint 57). Vérifié à travers un vrai socket : le
    /// `Join` reçu côté serveur doit porter le même `uid`.
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
                name: "Alice".to_string(),
                firebase_uid: Some("uid-alice".to_string()),
            }
        );
    }

    /// Sans compte connecté (`firebase_id_token` absent), l'envoi d'un message
    /// de chat ne doit ni planter ni démarrer de requête réseau — les règles
    /// RTDB refuseraient de toute façon l'écriture à un client anonyme
    /// (cf. `net::firebase`, Sprint 58).
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

    /// Bout-en-bout (vrai socket) : deux clients rejoignent le même serveur de
    /// test. Chacun doit voir un fantôme pour **l'autre**, mais jamais pour
    /// lui-même — c'est exactement le bug qu'aurait causé l'absence de
    /// `EntityDelta::player_id` (sans lui, impossible de distinguer « moi » de
    /// « l'autre joueur » dans un `Snapshot`).
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
}
