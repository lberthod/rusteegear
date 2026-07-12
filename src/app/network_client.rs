//! Client réseau (SPRINT_MMORPG.md) : connecte l'éditeur/le player à un
//! serveur RusteeGear (`src/bin/server.rs`) pour jouer à plusieurs. Desktop et
//! Android depuis le Sprint 65 (pas encore iOS, cf. `net/mod.rs`) pour tout ce
//! qui touche à *rejoindre une partie* (`connect_to_server`, `poll_network`…).
//! Le compte Firebase/chat/classement reste desktop uniquement — sur Android
//! et iOS, ces méthodes existent mais sont des no-op (même convention que
//! `app::ai`, qui a la même contrainte : `ureq` n'est pas ciblé sur mobile).
//!
//! Le joueur local reste **piloté par prédiction**, exactement comme en solo
//! (`sim_step` ne change pas) : ce module se contente d'envoyer son `Input` au
//! serveur, et d'afficher les *autres* joueurs reçus par `Snapshot` comme des
//! objets « fantômes » — sans physique ni script, leur position suit le
//! dernier `Snapshot` reçu, interpolée (cf. `net::interpolation::RemoteEntity`,
//! Sprint 54).

use super::AppState;

/// Serveur RusteeGear par défaut (VPS de l'utilisateur, cf. HANDOFF.md) : APK et
/// build desktop `--player` s'y connectent automatiquement au lancement (voir
/// `make_app` dans `lib.rs`), pour ne pas avoir à ressaisir l'adresse à chaque
/// test — la connexion manuelle (fenêtre/overlay Multijoueur) reste disponible
/// pour pointer ailleurs (ex. un serveur local pendant le développement).
pub const DEFAULT_SERVER_URL: &str = "ws://179.237.71.235:80";

/// Un autre joueur réseau, affiché comme un objet fantôme dans la scène locale.
pub struct RemotePlayer {
    pub name: String,
    // `pub(super)` : lu par `AppState::remote_player_scene_indices` (app/mod.rs) pour
    // exclure les fantômes de l'interpolation de rendu locale.
    pub(super) scene_index: usize,
    interp: crate::net::interpolation::RemoteEntity,
}

/// Fraction de l'écart comblée à chaque appel de `apply_local_network_position`
/// quand `reconcile` dépasse `SNAP_THRESHOLD` (Sprint 66, corrigé au
/// « Sprint 66bis » — cf. `SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md`).
///
/// **Bug de la première version (2026-07-12), trouvé en testant réellement
/// l'app** : elle figeait la position sur une interpolation entre deux points
/// **captés une fois** (`from`/`to`) et maintenue pendant `120 ms`, écrasant
/// à chaque frame ce que `sim_step`/`Physics::step` venaient de calculer à
/// partir de l'input réel. Or `Physics::step` (`runtime/physics.rs`) recopie
/// la pose du corps rigide dans `transform.position` à **chaque** tick, avant
/// que cette fonction ne s'exécute (sync à sens unique physique → transform,
/// jamais l'inverse) — donc toute correction qu'on y écrit est de toute façon
/// remplacée par la vraie position physique dès le tick suivant. Figer
/// l'écran sur une ligne entre deux points immobiles pendant que la vraie
/// position continuait d'avancer produisait exactement le symptôme
/// rapporté : personnage qui semble bloqué/trembler entre deux points,
/// ignorant l'input pendant toute la fenêtre de correction.
///
/// Le correctif : ne jamais figer plusieurs frames sur une cible ancienne.
/// Chaque frame où l'écart dépasse le seuil, on ne fait qu'un petit pas
/// (`CORRECTION_PULL`) depuis la position **fraîche** de ce tick vers la
/// position autoritative — jamais une valeur mémorisée. Le mouvement piloté
/// par l'input n'est donc jamais interrompu ; seule une légère dérive vers la
/// position serveur s'ajoute par-dessus, tick après tick, tant que l'écart
/// reste significatif.
const CORRECTION_PULL: f32 = 0.15;

/// Fenêtre (s) de l'historique des positions prédites du joueur local
/// (`net_local_history`) : doit couvrir la latence aller-retour vers le serveur,
/// son tick et le retard d'interpolation — 1 s laisse une marge confortable
/// au-dessus des ~150-250 ms mesurés vers le VPS réel, sans retenir assez de
/// points pour coûter quoi que ce soit (une entrée par frame, ~60-120).
const HISTORY_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

/// Intervalle minimal entre deux envois de `ClientMsg::Input` (Sprint 68,
/// `SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.2) — aligné sur
/// `SERVER_TICK` (`src/bin/server.rs`, ~60 Hz) : le serveur ne consomme
/// l'input qu'une fois par tick, envoyer plus souvent (jusqu'à la fréquence
/// d'affichage du client, potentiellement 144 Hz+) ne change rien au
/// gameplay et gaspille de la bande passante des deux côtés. Constante
/// dupliquée plutôt qu'importée de `src/bin/server.rs` : ce dernier est un
/// binaire séparé (pas une dépendance de la lib), cf. `net/mod.rs`.
const INPUT_SEND_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// Message de chat affichable — représentation universelle (contrairement à
/// `net::firebase::ChatMessage`, absent des cibles mobiles) : permet à
/// `AppState::chat_messages` de rester un champ normal, sans avoir besoin
/// d'être gaté par plateforme.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatLine {
    pub sender: String,
    pub text: String,
}

/// Entrée de classement affichable — même raison d'être universelle que
/// `ChatLine` (pas `net::firebase::LeaderboardEntry`, absent des cibles mobiles).
#[derive(Clone, Debug, PartialEq)]
pub struct LeaderboardLine {
    pub name: String,
    pub score: u32,
}

#[cfg(not(target_os = "ios"))]
impl AppState {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) sous `name`.
    /// Remplace une connexion existante s'il y en avait une. Transmet
    /// `self.firebase_uid` au serveur s'il est connu (cf. `sign_in`/`sign_up`
    /// ci-dessous, desktop uniquement — toujours `None` sur Android) — `None`
    /// pour une partie anonyme.
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
        self.net_local_interp = crate::net::interpolation::RemoteEntity::default();
        self.net_local_history.clear();
        self.net_last_input_sent = None;
        self.net_status = "Déconnecté".to_string();
    }

    /// `true` si une connexion au serveur est active.
    pub fn is_connected(&self) -> bool {
        self.net_client.is_some()
    }

    /// Appelé une fois par frame depuis `advance_play` : envoie l'input local,
    /// draine les messages serveur, met à jour les fantômes des autres joueurs.
    pub(super) fn poll_network(&mut self) {
        #[cfg(not(target_os = "android"))]
        {
            self.poll_firebase();
            self.poll_chat();
            self.poll_leaderboard();
        }
        if self.net_client.is_none() {
            return;
        }

        // Envoie l'input courant du joueur local, déjà tourné selon **notre**
        // caméra (`camera_relative_move`, même calcul que `sim_step`) : le
        // serveur (headless, sans caméra) reçoit ainsi directement une direction
        // monde correcte pour ce joueur — il n'a pas à connaître l'orientation
        // de qui que ce soit, chaque client fait sa propre conversion avant
        // d'envoyer. Sans ça, la caméra de chaque joueur pourrait pointer dans
        // une direction différente sans que son mouvement n'en tienne compte
        // une fois reçu côté serveur.
        //
        // **Plafonné à `INPUT_SEND_INTERVAL` (Sprint 68)** : `poll_network` est
        // appelée une fois par frame de rendu, potentiellement bien au-dessus
        // du tick serveur (ex. 144 Hz) — sans ce plafond, la plupart des
        // messages envoyés seraient jetés sans effet côté serveur (`set_
        // network_input` remplace l'entrée précédente, il ne les cumule pas),
        // pour un coût réseau/CPU inutile des deux côtés.
        let now = std::time::Instant::now();
        let should_send_input = self
            .net_last_input_sent
            .is_none_or(|last| now.duration_since(last) >= INPUT_SEND_INTERVAL);
        if should_send_input {
            let inp = &self.input_state;
            let player_yaw = self
                .player_object()
                .map(|o| o.transform.rotation.to_euler(glam::EulerRot::YXZ).0);
            let (mx, my) = network_move_axes(inp, self.camera.yaw, player_yaw);
            let input = crate::net::protocol::ClientMsg::Input {
                move_x: mx,
                move_y: my,
                attack: inp.attack,
                jump: inp.jump,
            };
            if let Some(client) = &self.net_client {
                client.send(&input);
            }
            self.net_last_input_sent = Some(now);
        }

        let messages: Vec<crate::net::protocol::ServerMsg> = match &self.net_client {
            Some(client) => client.inbox.try_iter().collect(),
            None => Vec::new(),
        };
        for msg in messages {
            self.handle_server_msg(msg);
        }

        // `sample_delayed` (Sprint 67, `SPRINTNETWORK.md`) plutôt que `sample`
        // à `now` directement : affiche les fantômes légèrement dans le passé
        // (`interpolation::RENDER_DELAY`) pour rester fluide sous gigue
        // réseau, cf. `AUDIT_LATENCE_MULTIJOUEUR.md` §2.4. Réutilise le `now`
        // capturé plus haut (juste avant l'envoi éventuel de l'`Input`) plutôt
        // que d'en reprendre un nouveau : les deux usages sont à quelques
        // microsecondes d'écart, pas la peine d'un second appel système.
        for rp in self.remote_players.values_mut() {
            if let Some((pos, yaw, visible)) = rp.interp.sample_delayed(now)
                && let Some(o) = self.scene.objects.get_mut(rp.scene_index)
            {
                o.transform.position = pos;
                o.transform.rotation = glam::Quat::from_rotation_y(yaw);
                o.visible = visible;
            }
        }
        // Le joueur local : cf. `apply_local_network_position`, appelée séparément
        // par `advance_play` *après* la physique — appliquer la position réseau
        // ici (avant `sim_step`) serait aussitôt écrasé par la simulation locale
        // du même objet, produisant un aller-retour visible entre les deux
        // positions à chaque frame (constaté en test réel : effet de dédoublement
        // du personnage en mouvement, 2026-07-12).
    }

    /// Réconcilie le joueur local avec la position renvoyée par le serveur : à
    /// appeler **après** la physique locale (`sim_step`).
    ///
    /// **Prédiction + réconciliation (2026-07-12)**, pas un simple écrasement :
    /// une première version affichait telle quelle la position du serveur,
    /// systématiquement — le serveur restait bien la seule source de vérité,
    /// mais le joueur local attendait alors un aller-retour réseau complet
    /// avant de voir le moindre mouvement, ce qui, aux ~150-250 ms de latence
    /// réelle vers le VPS, rendait le jeu poisseux (constaté en test réel :
    /// « pas fluide, pas temps réel »). `sim_step` continue donc de piloter le
    /// joueur local en prédiction immédiate (comme en solo) ; le serveur reste
    /// autoritaire mais ne **corrige** que si l'écart dépasse
    /// `interpolation::SNAP_THRESHOLD` (triche, désync, perte de paquets) —
    /// cf. `net::interpolation::reconcile`, écrit dès le Sprint 54 mais jamais
    /// câblé jusqu'ici.
    ///
    /// **Correction par petits pas (Sprint 66, révisé — bug trouvé en testant
    /// réellement l'app le 2026-07-12, cf. `SPRINTNETWORK.md`)** : une
    /// première version mémorisait `from`/`to` une fois puis figeait la
    /// position affichée sur une interpolation entre ces deux points figés
    /// pendant `120 ms` — mais `Physics::step` (`runtime/physics.rs`) recopie
    /// la pose du corps rigide dans `transform.position` à **chaque** tick,
    /// avant que cette fonction ne s'exécute (sync à sens unique physique →
    /// transform, jamais l'inverse). Cette correction figée écrasait donc,
    /// frame après frame, la vraie position fraîchement calculée à partir de
    /// l'input réel — le personnage semblait bloqué/trembler entre deux
    /// points pendant toute la fenêtre de correction, ignorant l'input.
    ///
    /// Le correctif : ne jamais figer une cible. Chaque frame où l'écart
    /// dépasse `SNAP_THRESHOLD`, on ne fait qu'un petit pas
    /// (`CORRECTION_PULL`) depuis la position **fraîche** de ce tick
    /// (`o.transform.position`, déjà mise à jour par `sim_step`/`Physics::
    /// step` avant cet appel) vers `server_pos` — jamais une valeur
    /// mémorisée d'un tick précédent. Le mouvement piloté par l'input n'est
    /// donc jamais interrompu ; seule une légère dérive vers la position
    /// serveur s'ajoute par-dessus, tant que l'écart reste significatif.
    /// Rien à faire si non connecté ou si aucun snapshot n'est encore arrivé.
    pub(super) fn apply_local_network_position(&mut self) {
        if self.net_client.is_none() {
            return;
        }
        let now = std::time::Instant::now();
        if let Some((server_pos, _yaw, visible)) = self.net_local_interp.sample(now)
            && let Some(pi) = self.player_index()
            && let Some(o) = self.scene.objects.get_mut(pi)
        {
            // Historique de la trajectoire prédite (une entrée par frame, fenêtre
            // `HISTORY_WINDOW`) : la position que le serveur renvoie date d'une
            // latence + un tick — en pleine course, elle est *toujours* à
            // vitesse × latence (≈ 1 m sur le VPS réel) derrière la prédiction.
            self.net_local_history
                .push_back((now, o.transform.position));
            while let Some(&(t, _)) = self.net_local_history.front()
                && now.duration_since(t) > HISTORY_WINDOW
            {
                self.net_local_history.pop_front();
            }
            // Vraie désynchronisation = la position serveur n'est **nulle part**
            // sur notre trajectoire récente. Si elle est proche d'un point où l'on
            // est réellement passé, le serveur est simplement en retard de la
            // latence : corriger là-dessus (ancien comportement, comparaison à la
            // seule position instantanée) déclenchait une traction continue dès
            // qu'on bougeait — le personnage freinait par à-coups et tremblait à
            // l'arrêt (constaté en vidéo, 2026-07-12, serveur VPS à ~200 ms).
            let on_recent_path = self
                .net_local_history
                .iter()
                .any(|&(_, p)| p.distance(server_pos) <= crate::net::interpolation::SNAP_THRESHOLD);
            let correction = crate::net::interpolation::reconcile(o.transform.position, server_pos)
                .filter(|_| !on_recent_path)
                .map(|_| o.transform.position.lerp(server_pos, CORRECTION_PULL));
            if let Some(new_pos) = correction {
                o.transform.position = new_pos;
            }
            // L'orientation reste pilotée localement (input) : la corriger comme
            // la position ferait tourner brutalement le personnage à chaque
            // snapshot, pour un gain quasi nul (l'orientation ne sert pas à
            // l'anti-triche ici).
            o.visible = visible;

            // **Indispensable, pas cosmétique** (bug trouvé en testant l'app
            // réellement) : écrire uniquement dans `transform.position` ne
            // survit qu'à la frame courante — `Physics::step` la recopie
            // depuis le corps rigide à *chaque* tick (sync à sens unique
            // physique → transform, jamais l'inverse), donc sans cet appel,
            // la correction est effacée dès le tick suivant et ne progresse
            // jamais : elle oscille indéfiniment entre la position physique
            // (inchangée) et `server_pos`, un aller-retour visible à chaque
            // frame (rapporté par l'utilisateur comme un personnage
            // « dupliqué »/tremblant entre deux points).
            if let Some(new_pos) = correction
                && let Some(physics) = &mut self.physics
            {
                physics.set_position(pi, new_pos);
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
                    // Notre propre joueur : le serveur reste maître de sa
                    // position lui aussi (pas de prédiction locale, cf. la doc
                    // de `net_local_interp`) — même traitement que les autres
                    // joueurs, appliqué à `player_index` plutôt qu'à un
                    // fantôme dans `poll_network`.
                    if Some(pid) == self.net_player_id {
                        self.net_local_interp.push(e, now);
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

/// Calcule la direction **monde** (`move_x`/`move_y`) à envoyer au serveur pour le
/// joueur local, à partir de son `PlayerInput`, du yaw de la caméra (joystick/
/// flèches, cf. `camera_relative_move`) et de son yaw propre s'il est connu (avance/
/// recul clavier « tank », `key_thrust`).
///
/// **Bug corrigé (2026-07-12, constaté en test réel)** : `key_thrust` (W/S) n'était
/// pas inclus ici — le serveur ne voyait donc jamais ce mouvement, et la
/// réconciliation (`apply_local_network_position`) finissait par annuler la
/// prédiction locale au bout de quelques secondes, donnant l'impression que le
/// déplacement au clavier « buguait ». Même formule que `AppState::advance_play`
/// (`-sin(yaw)`/`-cos(yaw)`) pour rester cohérent avec le mouvement prédit localement.
fn network_move_axes(
    inp: &super::PlayerInput,
    camera_yaw: f32,
    player_yaw: Option<f32>,
) -> (f32, f32) {
    let raw_mx = (inp.joy.0 + inp.key_move.0).clamp(-1.0, 1.0);
    let raw_my = (inp.joy.1 + inp.key_move.1).clamp(-1.0, 1.0);
    let (mut mx, mut my) = super::camera_relative_move(raw_mx, raw_my, camera_yaw);
    if inp.key_thrust != 0.0
        && let Some(yaw) = player_yaw
    {
        mx += inp.key_thrust * -yaw.sin();
        my += inp.key_thrust * -yaw.cos();
    }
    (mx, my)
}

/// Compte Firebase, chat, classement : desktop uniquement (`ureq`/`net::firebase`
/// ne ciblent pas mobile, cf. `Cargo.toml`/`net/mod.rs`).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl AppState {
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

    /// Rafraîchit le classement global (les `limit` meilleurs scores, lecture
    /// publique — ne nécessite pas de compte connecté ; l'écriture reste
    /// réservée au serveur de jeu, cf. `net::firebase`, Sprint 59). Sans effet
    /// si une requête est déjà en cours.
    pub fn request_refresh_leaderboard(
        &mut self,
        api_key: String,
        database_url: String,
        limit: usize,
    ) {
        if self.leaderboard_busy {
            return;
        }
        self.leaderboard_busy = true;
        let tx = self.leaderboard_tx.clone();
        std::thread::spawn(move || {
            let config = crate::net::firebase::FirebaseConfig {
                api_key,
                database_url,
            };
            let result = crate::net::firebase::get_top_leaderboard(&config, limit).map(|entries| {
                entries
                    .into_iter()
                    .map(|e| LeaderboardLine {
                        name: e.name,
                        score: e.score,
                    })
                    .collect()
            });
            let _ = tx.send(result);
        });
    }

    /// Applique le résultat d'une requête de classement en attente, s'il y en a un.
    fn poll_leaderboard(&mut self) {
        while let Ok(result) = self.leaderboard_rx.try_recv() {
            self.leaderboard_busy = false;
            match result {
                Ok(entries) => self.leaderboard = entries,
                Err(e) => log::warn!("Classement : requête échouée : {e}"),
            }
        }
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

/// iOS uniquement : `net::client` n'y est pas encore compilé (cf. `net/mod.rs`),
/// contrairement à Android depuis le Sprint 65.
#[cfg(target_os = "ios")]
impl AppState {
    pub fn connect_to_server(&mut self, _url: &str, _name: &str) {
        self.net_status = "Multijoueur indisponible sur iOS".to_string();
    }

    pub fn disconnect_from_server(&mut self) {}

    pub fn is_connected(&self) -> bool {
        false
    }

    pub(super) fn poll_network(&mut self) {}

    pub(super) fn apply_local_network_position(&mut self) {}
}

/// Compte Firebase, chat, classement : hors mobile (iOS et Android), cf. le
/// bloc desktop équivalent plus haut.
#[cfg(any(target_os = "ios", target_os = "android"))]
impl AppState {
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

    pub fn request_refresh_leaderboard(
        &mut self,
        _api_key: String,
        _database_url: String,
        _limit: usize,
    ) {
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use std::time::{Duration, Instant};

    use glam::Vec3;

    use super::*;
    use crate::app::multiplayer::NetworkInput;
    use crate::net::protocol::EntityDelta;
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

    /// Prépare un `AppState` connecté (vrai socket) avec un gabarit pilotable
    /// chargé, prêt pour les tests de `apply_local_network_position` — la
    /// même mise en place que `client_sees_a_ghost_for_the_other_player_but_
    /// never_for_itself`, réduite à un seul client (ces tests ne portent que
    /// sur la réconciliation du joueur local, pas sur les fantômes distants).
    fn connected_app_with_a_player(net: &NetServer) -> AppState {
        let url = format!("ws://{}", net.local_addr);
        let mut app = AppState::new();
        app.load_zombies_demo();
        app.connect_to_server(&url, "Testeur");
        assert!(app.is_connected());
        app
    }

    /// Sprint 66 (`SPRINTNETWORK.md`, §2.3 de `AUDIT_LATENCE_MULTIJOUEUR.md`) :
    /// un écart au-dessus de `SNAP_THRESHOLD` ne doit **jamais** faire sauter
    /// le joueur local directement à la position autoritative en un seul
    /// appel — seulement un petit pas (`CORRECTION_PULL`) vers elle.
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

    /// **Bug réel constaté en vidéo (2026-07-12, serveur VPS à ~200 ms)** : la
    /// position renvoyée par le serveur date d'une latence + un tick — en pleine
    /// course à 4,5 m/s elle est *toujours* ~1 m derrière la prédiction, au-delà
    /// de `SNAP_THRESHOLD`. L'ancienne comparaison à la seule position
    /// *instantanée* déclenchait donc une traction arrière continue pendant tout
    /// déplacement : vitesse en dents de scie et tremblement à l'arrêt, visibles
    /// image par image dans l'enregistrement. Une position serveur **sur notre
    /// trajectoire récente** signifie « en phase, juste en retard » : aucune
    /// correction ne doit s'appliquer.
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

    /// **Bug réel trouvé en testant l'app réelle (2026-07-12)** : la première
    /// version de ce sprint mémorisait une cible figée (`from`/`to`) et
    /// écrasait `transform.position` avec une interpolation entre ces deux
    /// points fixes pendant 120 ms — mais `Physics::step`
    /// (`runtime/physics.rs`) recopie la pose du corps rigide dans
    /// `transform.position` à *chaque* tick, avant cette fonction. Toute
    /// correction figée sur une ancienne cible écrasait donc la vraie
    /// position, fraîchement avancée par l'input réel, pendant toute la
    /// fenêtre de correction — le joueur semblait bloqué/trembler entre deux
    /// points, l'input étant purement ignoré. Ce test simule un mouvement
    /// local (`sim_step`) entre deux appels de `apply_local_network_position`
    /// et vérifie que ce mouvement **n'est jamais écrasé** : la position
    /// finale doit refléter à la fois le mouvement local et un petit pas de
    /// correction, jamais uniquement l'un ou l'autre.
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

    /// **Bug réel trouvé en testant l'app réellement (capture d'écran
    /// utilisateur : personnage dupliqué/tremblant entre deux points),
    /// cf. `SPRINTNETWORK.md` Sprint 66bis.** Avec la physique réellement
    /// construite (`Physics::build`), une correction qui n'écrit que dans
    /// `transform.position` sans passer par `Physics::set_position` est
    /// effacée dès le tick physique suivant — `Physics::step` recopie la pose
    /// du corps rigide (resté inchangé) par-dessus. Ce test simule
    /// exactement cette séquence (correction, puis un tick physique, comme
    /// le ferait `advance_play` à la frame suivante) et vérifie que la
    /// correction **survit**.
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
        // rigide, jamais informé de la correction) — le bug exact rapporté.
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

    /// Sprint 68 (`SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.2) :
    /// `poll_network` est appelée une fois par frame de rendu, potentiellement
    /// bien plus souvent que le tick serveur — sans plafond, un client sur un
    /// écran rapide enverrait un `Input` par frame affichée. Ce test appelle
    /// `poll_network` en boucle serrée (sans dormir entre les appels, donc
    /// bien plus vite que `INPUT_SEND_INTERVAL`) et vérifie que le serveur ne
    /// reçoit qu'une poignée d'`Input`, pas un par appel.
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
        // Bug corrigé : sans le yaw du joueur, W/S (contrôles tank) ne produisaient
        // aucune direction monde à envoyer au serveur — le mouvement clavier restait
        // invisible pour lui malgré la prédiction locale.
        let inp = super::super::PlayerInput {
            key_thrust: 1.0,
            ..Default::default()
        };
        let yaw = 0.0_f32; // face à -Z (cf. `Physics::face_direction`)
        let (mx, my) = network_move_axes(&inp, 0.0, Some(yaw));
        assert!(
            (mx - 0.0).abs() < 1e-5 && (my - -1.0).abs() < 1e-5,
            "avancer (W) à yaw=0 doit donner une direction monde vers -Z : ({mx}, {my})"
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
}
