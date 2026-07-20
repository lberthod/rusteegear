//! Client réseau : connecte l'éditeur/le player à un serveur RusteeGear
//! (`src/bin/server.rs`) pour jouer à plusieurs. Desktop et Android pour tout
//! ce qui touche à *rejoindre une partie* (`connect_to_server`,
//! `poll_network`…) — pas encore iOS (cf. `net/mod.rs`). Le compte
//! Firebase/chat/classement reste desktop uniquement — sur Android et iOS,
//! ces méthodes existent mais sont des no-op (même convention que
//! `app::ai`, qui a la même contrainte : `ureq` n'est pas ciblé sur mobile).
//!
//! Le joueur local reste **piloté par prédiction**, exactement comme en solo
//! (`sim_step` ne change pas) : ce module se contente d'envoyer son `Input` au
//! serveur, et d'afficher les *autres* joueurs reçus par `Snapshot` comme des
//! objets « fantômes » — sans contrôleur ni script Lua, leur position suit le
//! dernier `Snapshot` reçu, interpolée (cf. `net::interpolation::RemoteEntity`).
//! Ils ont cependant un corps physique kinématique (comme une créature
//! scriptée, cf. `runtime::physics::resolve_scripted_moves`) pour rester des
//! obstacles réels vis-à-vis du joueur local.

use super::AppState;

/// Serveur RusteeGear par défaut (VPS de l'utilisateur, cf. HANDOFF.md) : APK et
/// build desktop `--player` s'y connectent automatiquement au lancement (voir
/// `make_app` dans `lib.rs`), pour ne pas avoir à ressaisir l'adresse à chaque
/// test — la connexion manuelle (fenêtre/overlay Multijoueur) reste disponible
/// pour pointer ailleurs (ex. un serveur local pendant le développement).
///
/// `wss://` (chiffré) partout : `ws.loicberthod.ch` est un sous-domaine Caddy
/// dédié (HTTPS automatique via Let's Encrypt) qui termine le TLS et relaie en
/// clair vers le serveur de jeu (`localhost:7777` sur le VPS). Obligatoire sur
/// le web (un navigateur qui a chargé la page en HTTPS refuse un WebSocket
/// `ws://` non chiffré — `net::client::web` le détecte en amont et refuse
/// proprement, cf. sa doc) ; côté natif, le client (`net::client::native`,
/// desktop + Android) parle TLS depuis que `tokio-tungstenite` embarque rustls
/// (feature `rustls-tls-webpki-roots`, cf. Cargo.toml — avant ça, seule la
/// route `ws://179.237.71.235:80` en clair marchait en natif ; elle existe
/// toujours côté Caddy pour un client sans TLS). Attention : Caddy répond
/// « 308 Permanent Redirect » si on frappe cette façade en `ws://` non chiffré
/// (redirection HTTP→HTTPS automatique), d'où l'indice ajouté par
/// `net::client::native` dans ce cas.
pub const DEFAULT_SERVER_URL: &str = "wss://ws.loicberthod.ch";

#[path = "network_client_types.rs"]
mod types;
pub use types::*;

#[cfg(not(target_os = "ios"))]
impl AppState {
    /// Se connecte à `url` (ex. `"ws://127.0.0.1:7777"`) sous `name`, en
    /// Assaut (classe par défaut), salon par défaut et mode Vagues — cf.
    /// `connect_to_server_as` pour choisir une autre classe (Sprint 3,
    /// `sprint10audit.md`), un code de partie ou un mode (Sprints 20-21,
    /// `sprintreflecion.md`, fenêtre Multijoueur).
    pub fn connect_to_server(&mut self, url: &str, name: &str) {
        self.connect_to_server_as(
            url,
            name,
            crate::app::multiplayer::PlayerClass::Assault,
            crate::net::protocol::DEFAULT_LOBBY,
            crate::app::multiplayer::RoundObjective::Vagues,
        );
    }

    /// Comme `connect_to_server`, avec une classe choisie (Sprint 3), un code
    /// de partie (`room`, Sprint 20 — **distinct** du salon de chat Firebase,
    /// cf. `editor::windows::multiplayer_window`) et un mode de manche
    /// (`objective`, Sprint 21) choisis dans la fenêtre Multijoueur. `room`
    /// vide retombe sur `protocol::DEFAULT_LOBBY` (comportement inchangé pour
    /// qui laisse le champ vide, même repli que côté protocole,
    /// `net/protocol.rs:52-56`). Remplace une connexion existante s'il y en
    /// avait une. Transmet `self.firebase_uid` au serveur s'il est connu (cf.
    /// `sign_in`/`sign_up` ci-dessous, desktop uniquement — toujours `None`
    /// sur Android) — `None` pour une partie anonyme.
    pub fn connect_to_server_as(
        &mut self,
        url: &str,
        name: &str,
        class: crate::app::multiplayer::PlayerClass,
        room: &str,
        objective: crate::app::multiplayer::RoundObjective,
    ) {
        self.disconnect_from_server();
        let room = room.trim();
        let lobby = if room.is_empty() {
            crate::net::protocol::DEFAULT_LOBBY
        } else {
            room
        };
        match crate::net::client::NetClient::connect_to_lobby(
            url,
            name,
            self.firebase_uid.as_deref(),
            lobby,
            class.to_u8(),
            objective.to_u8(),
        ) {
            Ok(client) => {
                log::info!("Multijoueur : connecté à {url} sous « {name} »");
                self.net_client = Some(client);
                self.net_status = format!("Connexion à {url}…");
                // Arme le watchdog dès maintenant (pas au premier message) : un
                // serveur qui accepte la socket mais ne répond jamais doit finir
                // par déclencher la reconnexion, pas rester « Connexion… » à vie.
                self.net_last_server_msg = Some(crate::time_compat::Instant::now());
                // Mémorisé pour la reconnexion automatique (cf. `poll_network`) —
                // seulement au succès : un échec immédiat reste une erreur
                // affichée au joueur, pas une boucle de reconnexion.
                self.net_last_connect = Some((
                    url.to_string(),
                    name.to_string(),
                    lobby.to_string(),
                    class.to_u8(),
                    objective.to_u8(),
                ));
            }
            Err(e) => {
                log::warn!("Multijoueur : connexion à {url} échouée : {e}");
                self.net_status = format!("Connexion échouée : {e}");
            }
        }
    }

    /// Quitte la partie en ligne (sans effet si non connecté) : prévient le
    /// serveur, masque les fantômes des autres joueurs. Déconnexion
    /// **volontaire** : annule aussi toute reconnexion automatique (en cours ou
    /// à venir) — quitter la partie ne doit jamais voir le client se
    /// reconnecter tout seul dans le dos du joueur.
    pub fn disconnect_from_server(&mut self) {
        if let Some(client) = &self.net_client {
            client.send(&crate::net::protocol::ClientMsg::Leave);
        }
        self.reset_network_session();
        self.net_last_connect = None;
        self.net_reconnect = None;
        self.net_last_server_msg = None;
        self.net_status = "Déconnecté".to_string();
    }

    /// Nettoyage commun à toute fin de session réseau : déconnexion volontaire
    /// (`disconnect_from_server`) comme perte de connexion détectée
    /// (`poll_network`) — sans toucher à la politique de reconnexion ni au
    /// `net_status`, qui diffèrent entre les deux.
    fn reset_network_session(&mut self) {
        self.net_client = None;
        self.net_player_id = None;
        for rp in self.remote_players.values() {
            if let Some(o) = self.scene.objects.get_mut(rp.scene_index) {
                if o.visible {
                    self.net_visibility_dirty = true;
                }
                o.visible = false;
            }
        }
        self.remote_players.clear();
        self.net_local_interp = crate::net::interpolation::RemoteEntity::default();
        self.net_local_health = None;
        self.net_local_kills = None;
        self.net_local_assists = None;
        self.net_local_history.clear();
        self.net_last_input_sent = None;
        // Plus de snapshots à venir : oublie les projectiles serveur et masque le
        // pool, sinon les dernières boules reçues resteraient figées à l'écran.
        self.net_projectiles.clear();
        self.sync_fireball_pool(&[]);
    }

    /// `true` si une connexion au serveur est active **et vivante** : transport
    /// sain (`NetClient::is_alive`) et serveur entendu récemment
    /// (`NET_SILENCE_TIMEOUT`). L'ancien test (`net_client.is_some()`) déclarait
    /// « connecté » un client dont la connexion était morte depuis des minutes —
    /// roster figé, envois dans un canal fermé, aucun message d'erreur.
    pub fn is_connected(&self) -> bool {
        self.net_client.as_ref().is_some_and(|c| c.is_alive())
            && self
                .net_last_server_msg
                .is_none_or(|t| t.elapsed() < NET_SILENCE_TIMEOUT)
    }

    /// État de la connexion multijoueur (cf. `NetConnState`) — la forme
    /// exploitable par du code, là où `net_status` reste le texte du HUD.
    pub fn net_connection_state(&self) -> NetConnState {
        if let Some(r) = &self.net_reconnect {
            return NetConnState::Reconnecting { attempt: r.attempt };
        }
        if !self.is_connected() {
            return NetConnState::Offline;
        }
        if self.net_player_id.is_some() {
            NetConnState::Connected
        } else {
            NetConnState::Connecting
        }
    }

    /// `true` si ce joueur réseau est vaincu (0 PV, GAMEDESIGN_EN_LIGNE.md §3.1)
    /// — spectateur pour le reste de la manche. Dérivé de `net_local_health`
    /// (déjà tenu à jour depuis le dernier `Snapshot`, cf. `handle_server_msg`)
    /// plutôt qu'un champ dédié à maintenir en synchronisation. Sert au HUD
    /// (`defeated_banner`, `editor/mod.rs`) : sans retour explicite ici, un
    /// joueur qui meurt voyait son personnage disparaître en silence — un
    /// flash rouge d'un tiers de seconde puis un écran figé, indiscernable
    /// d'un vrai bug.
    pub fn is_locally_defeated(&self) -> bool {
        self.is_connected() && self.net_local_health.is_some_and(|h| h <= 0.0)
    }

    /// Frags à afficher au HUD (brique de progression pour un futur MMORPG) :
    /// le compteur individualisé du serveur si connecté (`net_local_kills`,
    /// `None` avant le premier snapshot ⇒ 0 affiché), sinon le score solo
    /// (`self.score`, qui compte déjà chaque monstre vaincu localement — un
    /// seul joueur, pas besoin d'individualiser).
    pub fn displayed_kill_count(&self) -> u32 {
        if self.is_connected() {
            self.net_local_kills.unwrap_or(0)
        } else {
            self.score()
        }
    }

    /// Assists à afficher au HUD (Phase L Sprint 3, `sprint2audijeu0718.md`,
    /// GDD §8.3) — même principe que `displayed_kill_count` : 0 en solo (pas
    /// d'assist possible sans coéquipier).
    pub fn displayed_assist_count(&self) -> u32 {
        if self.is_connected() {
            self.net_local_assists.unwrap_or(0)
        } else {
            0
        }
    }

    /// Liste des joueurs de la partie en ligne pour le HUD (GAMEDESIGN_EN_LIGNE.md
    /// §3.4 — identité, vie et frags des autres joueurs affichées) : `(nom, vie
    /// 0..1 ou `None` avant le premier snapshot, frags, soi-même ?)`. Vide si
    /// non connecté.
    pub fn multiplayer_roster(&self) -> Vec<(String, Option<f32>, Option<u32>, bool)> {
        if !self.is_connected() {
            return Vec::new();
        }
        let mut roster = vec![(
            "Vous".to_string(),
            self.net_local_health,
            self.net_local_kills,
            true,
        )];
        roster.extend(
            self.remote_players
                .values()
                .map(|rp| (rp.name.clone(), rp.health, rp.kills, false)),
        );
        roster
    }

    /// Appelé une fois par frame depuis `advance_play` : envoie l'input local,
    /// draine les messages serveur, met à jour les fantômes des autres joueurs.
    pub(super) fn poll_network(&mut self) {
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        {
            self.poll_firebase();
            self.poll_chat();
            self.poll_leaderboard();
            self.poll_online_players();
        }
        // Reconnexion automatique en cours : fait avancer la machine à états
        // (résultat d'une tentative en vol, lancement de la suivante au terme
        // du backoff) — **avant** le early-return ci-dessous, qui masquait
        // jusqu'ici toute vie réseau dès que `net_client` était `None`.
        self.advance_reconnection();
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
        // **Plafonné à `INPUT_SEND_INTERVAL`** : `poll_network` est appelée
        // une fois par frame de rendu, potentiellement bien au-dessus
        // du tick serveur (ex. 144 Hz) — sans ce plafond, la plupart des
        // messages envoyés seraient jetés sans effet côté serveur (`set_
        // network_input` remplace l'entrée précédente, il ne les cumule pas),
        // pour un coût réseau/CPU inutile des deux côtés.
        let now = crate::time_compat::Instant::now();
        let should_send_input = self
            .net_last_input_sent
            .is_none_or(|last| now.duration_since(last) >= INPUT_SEND_INTERVAL);
        if should_send_input {
            let input = network_input_msg(
                &self.input_state,
                self.camera.yaw,
                self.player_object(),
                self.selected_weapon() as u8,
            );
            if let Some(client) = &self.net_client {
                client.send(&input);
            }
            self.net_last_input_sent = Some(now);
        }

        let messages: Vec<crate::net::protocol::ServerMsg> = match &self.net_client {
            Some(client) => client.inbox.try_iter().collect(),
            None => Vec::new(),
        };
        if !messages.is_empty() {
            // Preuve de vie du serveur, quel que soit le message — mise à jour
            // **après** le drain (pas message par message) et avant le
            // diagnostic ci-dessous : si la boucle de jeu a été suspendue
            // (pause, App Nap), les messages accumulés pendant la suspension
            // comptent comme vie récente, sinon la reprise déclencherait une
            // fausse reconnexion.
            self.net_last_server_msg = Some(crate::time_compat::Instant::now());
        }
        for msg in messages {
            self.handle_server_msg(msg);
        }

        // Diagnostic de coupure, après le drain : transport mort
        // (`is_alive()`) ou silence serveur prolongé (`NET_SILENCE_TIMEOUT`,
        // cf. sa doc pour la hiérarchie des timeouts). Détectée ⇒ session
        // nettoyée et reconnexion automatique armée — le joueur voit
        // « Connexion perdue… » au lieu d'un monde figé sans explication.
        let transport_dead = self.net_client.as_ref().is_some_and(|c| !c.is_alive());
        let server_silent = self
            .net_last_server_msg
            .is_some_and(|t| t.elapsed() >= NET_SILENCE_TIMEOUT);
        if transport_dead || server_silent {
            let cause = if transport_dead {
                "connexion fermée"
            } else {
                "serveur silencieux"
            };
            log::warn!("Multijoueur : connexion perdue ({cause})");
            self.reset_network_session();
            self.schedule_reconnect_attempt();
            return;
        }

        // `sample_delayed` (cf. `SPRINTNETWORK.md`) plutôt que `sample`
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
                if o.visible != visible {
                    self.net_visibility_dirty = true;
                }
                o.visible = visible;
            }
            // Animation répliquée : le clip n'est **pas** interpolé
            // comme la position (cf. `RemoteEntity::latest_anim_clip`) — juste
            // poussé dans `AnimationState::set_clip()` du fantôme dès qu'il
            // change, pour bénéficier du même fondu enchaîné qu'en solo.
            if let Some(clip) = rp.interp.latest_anim_clip()
                && !clip.is_empty()
                && let Some(o) = self.scene.objects.get_mut(rp.scene_index)
                && let Some(state) = o.animation.as_mut()
            {
                state.set_clip(clip);
            }
        }
        // Le joueur local : cf. `apply_local_network_position`, appelée séparément
        // par `advance_play` *après* la physique — appliquer la position réseau
        // ici (avant `sim_step`) serait aussitôt écrasé par la simulation locale
        // du même objet, produisant un aller-retour visible entre les deux
        // positions à chaque frame.

        // Un fantôme (joueur distant ou créature diffusée) vient de changer de
        // visibilité : reconstruit la physique pour que son corps kinématique
        // apparaisse/disparaisse en même temps que lui à l'écran (cf. la doc
        // de `net_visibility_dirty` et `ensure_remote_player` — même principe
        // que `App::update_waves` pour une manche révélée). Rare (connexion/
        // déconnexion, mort) : pas de rebuild à chaque frame en régime normal.
        if self.net_visibility_dirty {
            self.net_visibility_dirty = false;
            if self.physics.is_some() {
                self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
            }
        }
    }

    /// Arme (ou fait progresser d'une tentative) la reconnexion automatique
    /// après une coupure détectée : tentative suivante planifiée avec backoff
    /// (cf. `reconnect_delay`), abandon définitif après
    /// `MAX_RECONNECT_ATTEMPTS` — dans tous les cas, `net_status` dit au
    /// joueur ce qui se passe.
    fn schedule_reconnect_attempt(&mut self) {
        if self.net_last_connect.is_none() {
            // Rien à rejouer : jamais connecté avec succès, ou déconnexion
            // volontaire entre-temps.
            self.net_reconnect = None;
            self.net_status = "Déconnecté".to_string();
            return;
        }
        let attempt = self.net_reconnect.as_ref().map_or(1, |r| r.attempt + 1);
        if attempt > MAX_RECONNECT_ATTEMPTS {
            log::warn!(
                "Multijoueur : reconnexion abandonnée après {MAX_RECONNECT_ATTEMPTS} tentatives"
            );
            self.net_reconnect = None;
            self.net_last_connect = None;
            self.net_status = format!(
                "Déconnecté (reconnexion échouée après {MAX_RECONNECT_ATTEMPTS} tentatives)"
            );
            return;
        }
        self.net_status = format!(
            "Connexion perdue — reconnexion (tentative {attempt}/{MAX_RECONNECT_ATTEMPTS})…"
        );
        self.net_reconnect = Some(ReconnectState {
            attempt,
            next_try: crate::time_compat::Instant::now() + reconnect_delay(attempt),
            #[cfg(not(target_arch = "wasm32"))]
            pending: None,
        });
        // Watchdog désarmé le temps de l'attente : il ne surveille qu'une
        // connexion active, pas un backoff.
        self.net_last_server_msg = None;
    }

    /// Fait avancer la reconnexion automatique (appelée à chaque frame par
    /// `poll_network`, avant son early-return « pas de client ») : récolte le
    /// résultat d'une tentative en vol, ou en lance une nouvelle au terme du
    /// backoff. Sans effet si connecté ou si aucune reconnexion n'est armée.
    fn advance_reconnection(&mut self) {
        if self.net_client.is_some() || self.net_reconnect.is_none() {
            return;
        }
        // 1. Résultat d'une tentative en vol (natif uniquement : la connexion
        //    bloquante vit dans un thread éphémère, cf. `ReconnectState::pending`).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let outcome = self.net_reconnect.as_ref().and_then(|s| {
                let rx = s.pending.as_ref()?;
                match rx.try_recv() {
                    Ok(res) => Some(res),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                        "le thread de reconnexion s'est arrêté sans répondre".to_string(),
                    )),
                }
            });
            match outcome {
                Some(Ok(client)) => {
                    self.install_reconnected_client(client);
                    return;
                }
                Some(Err(e)) => {
                    log::warn!("Multijoueur : tentative de reconnexion échouée : {e}");
                    self.schedule_reconnect_attempt();
                    return;
                }
                None => {
                    // Tentative toujours en vol : attendre son verdict avant
                    // d'en lancer une autre.
                    if self
                        .net_reconnect
                        .as_ref()
                        .is_some_and(|s| s.pending.is_some())
                    {
                        return;
                    }
                }
            }
        }
        // 2. Backoff écoulé ? → lancer la tentative suivante.
        let now = crate::time_compat::Instant::now();
        if self
            .net_reconnect
            .as_ref()
            .is_some_and(|s| now < s.next_try)
        {
            return;
        }
        let Some((url, name, lobby, class, objective)) = self.net_last_connect.clone() else {
            self.net_reconnect = None;
            return;
        };
        let uid = self.firebase_uid.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Connexion bloquante (poignée de main TCP/WebSocket complète) :
            // jamais sur le thread de rendu — même patron thread éphémère +
            // canal que les imports glTF ou les requêtes IA.
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let res = crate::net::client::NetClient::connect_to_lobby(
                    &url,
                    &name,
                    uid.as_deref(),
                    &lobby,
                    class,
                    objective,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(res);
            });
            if let Some(s) = self.net_reconnect.as_mut() {
                s.pending = Some(rx);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // `connect` ne bloque jamais sur cette cible (cf. `net::client::web`) :
            // appel direct, l'échec réel arrivera par `is_alive()` — la boucle de
            // détection de `poll_network` enchaînera alors sur la tentative suivante.
            match crate::net::client::NetClient::connect_to_lobby(
                &url,
                &name,
                uid.as_deref(),
                &lobby,
                class,
                objective,
            ) {
                Ok(client) => self.install_reconnected_client(client),
                Err(e) => {
                    log::warn!("Multijoueur : tentative de reconnexion échouée : {e}");
                    self.schedule_reconnect_attempt();
                }
            }
        }
    }

    /// Installe le transport d'une reconnexion réussie. `net_reconnect` reste
    /// armé jusqu'au `Welcome` (cf. `handle_server_msg`) : si cette connexion
    /// meurt avant, la tentative suivante reprend le compte là où il en était
    /// au lieu de repartir de 1 en boucle contre un serveur mort. Le `Join` est
    /// déjà parti (encodé par `connect_to_lobby`) ; le `Welcome` attribuera un
    /// **nouveau** `player_id` — côté serveur, c'est un nouveau joueur, l'ancien
    /// avatar a été retiré à la fermeture de l'ancienne socket (`Leave`
    /// synthétique de `server_loop::handle_connection`).
    fn install_reconnected_client(&mut self, client: crate::net::client::NetClient) {
        self.net_client = Some(client);
        // Ré-arme le watchdog : cette connexion neuve a droit à sa pleine
        // fenêtre de silence avant d'être déclarée morte à son tour.
        self.net_last_server_msg = Some(crate::time_compat::Instant::now());
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = self.net_reconnect.as_mut() {
            s.pending = None;
        }
        self.net_status = "Reconnexion : transport rétabli, en attente du serveur…".to_string();
    }

    /// Réconcilie le joueur local avec la position renvoyée par le serveur : à
    /// appeler **après** la physique locale (`sim_step`).
    ///
    /// **Prédiction + réconciliation**, pas un simple écrasement : `sim_step`
    /// continue de piloter le joueur local en prédiction immédiate (comme en
    /// solo) ; le serveur reste autoritaire mais ne **corrige** que si l'écart
    /// dépasse `interpolation::SNAP_THRESHOLD` (triche, désync, perte de
    /// paquets) — cf. `net::interpolation::reconcile`. Afficher telle quelle
    /// la position serveur à chaque snapshot ferait attendre au joueur local
    /// un aller-retour réseau complet avant de voir le moindre mouvement.
    ///
    /// **Correction par petits pas, jamais une cible figée** (cf.
    /// `SPRINTNETWORK.md`) : chaque frame où l'écart dépasse
    /// `SNAP_THRESHOLD`, on ne fait qu'un petit pas (`CORRECTION_PULL`)
    /// depuis la position **fraîche** de ce tick (`o.transform.position`,
    /// déjà mise à jour par `sim_step`/`Physics::step` avant cet appel) vers
    /// `server_pos` — jamais une valeur mémorisée d'un tick précédent.
    /// `Physics::step` (`runtime/physics.rs`) recopie la pose du corps rigide
    /// dans `transform.position` à **chaque** tick, avant que cette fonction
    /// ne s'exécute (sync à sens unique physique → transform, jamais
    /// l'inverse) : une correction basée sur une cible mémorisée serait donc
    /// écrasée avant de pouvoir converger (cf. docs/audits/app-network.md
    /// pour le bug réel que ça a causé). Le mouvement piloté par l'input
    /// n'est donc jamais interrompu ; seule une légère dérive vers la
    /// position serveur s'ajoute par-dessus, tant que l'écart reste
    /// significatif. Rien à faire si non connecté ou si aucun snapshot n'est
    /// encore arrivé.
    pub(super) fn apply_local_network_position(&mut self) {
        if self.net_client.is_none() {
            return;
        }
        let now = crate::time_compat::Instant::now();
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
            // latence : corriger là-dessus (comparaison à la seule position
            // instantanée) déclencherait une traction continue dès qu'on bouge —
            // le personnage freinerait par à-coups et tremblerait à l'arrêt (cf.
            // docs/audits/app-network.md).
            let on_recent_path = self
                .net_local_history
                .iter()
                .any(|&(_, p)| p.distance(server_pos) <= crate::net::interpolation::SNAP_THRESHOLD);
            let mut correction =
                crate::net::interpolation::reconcile(o.transform.position, server_pos)
                    .filter(|_| !on_recent_path)
                    .map(|_| o.transform.position.lerp(server_pos, CORRECTION_PULL));
            // Rattrapage doux à l'arrêt (cf. `IDLE_SETTLE_PULL`) : sous
            // `SNAP_THRESHOLD`, `reconcile` ne corrige volontairement rien — mais un
            // joueur immobile peut alors rester décalé en permanence de la position
            // que les *autres* clients voient de lui. Immobile + écart notable ⇒ on
            // converge lentement vers la vérité serveur, tous écrans alignés.
            if correction.is_none() {
                // `is_some_and` (pas `is_none_or`) : sans corps physique (mode
                // édition, pause), on ne peut pas savoir si le joueur bouge — dans
                // le doute, ne rien rattraper plutôt que de tirer un joueur en
                // mouvement.
                let at_rest = self
                    .physics
                    .as_ref()
                    .and_then(|p| p.velocity(pi))
                    .is_some_and(|v| v.length() < IDLE_SPEED_EPSILON);
                if at_rest && o.transform.position.distance(server_pos) > IDLE_SETTLE_MIN {
                    correction = Some(o.transform.position.lerp(server_pos, IDLE_SETTLE_PULL));
                }
            }
            if let Some(new_pos) = correction {
                o.transform.position = new_pos;
            }
            // L'orientation reste pilotée localement (input) : la corriger comme
            // la position ferait tourner brutalement le personnage à chaque
            // snapshot, pour un gain quasi nul (l'orientation ne sert pas à
            // l'anti-triche ici).
            o.visible = visible;

            // **Indispensable, pas cosmétique** : écrire uniquement dans
            // `transform.position` ne survit qu'à la frame courante —
            // `Physics::step` la recopie depuis le corps rigide à *chaque*
            // tick (sync à sens unique physique → transform, jamais
            // l'inverse), donc sans cet appel, la correction est effacée dès
            // le tick suivant et ne progresse jamais : elle oscille
            // indéfiniment entre la position physique (inchangée) et
            // `server_pos`, un aller-retour visible à chaque frame (cf.
            // docs/audits/app-network.md pour le bug réel que ça a causé).
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
                // Solde une éventuelle reconnexion automatique : le serveur nous
                // a réadmis (sous un **nouveau** `player_id` — l'ancien avatar a
                // été retiré à la fermeture de l'ancienne socket), le compteur
                // de tentatives repart de zéro pour la prochaine coupure.
                self.net_reconnect = None;
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
                    if o.visible {
                        self.net_visibility_dirty = true;
                    }
                    o.visible = false;
                }
            }
            ServerMsg::Snapshot(snap) => {
                let now = crate::time_compat::Instant::now();
                // Boules de feu en vol côté serveur : mémorisées telles quelles,
                // affichées par le pool local (cf. `sync_fireball_pool`, appelé
                // par `update_fireballs` à chaque frame). Pas d'interpolation :
                // à 60 Hz de snapshots pour un projectile qui vit ~1,5 s, le
                // pas entre deux positions reçues est déjà sous le pixel utile.
                self.net_projectiles = snap
                    .projectiles
                    .iter()
                    .map(|p| (glam::Vec3::from_array(p.position), p.weapon as usize))
                    .collect();
                // Projectiles de créature (jet d'eau, crachat de feu...) — même
                // principe, affichés par `creature_attack::sync_creature_shot_pool`.
                self.net_creature_shots = snap
                    .creature_shots
                    .iter()
                    .map(|s| {
                        (
                            glam::Vec3::from_array(s.position),
                            glam::Vec3::from_array(s.dir),
                            s.cfg as usize,
                        )
                    })
                    .collect();
                for e in snap.entities {
                    let Some(pid) = e.player_id else {
                        // Entité sans propriétaire = monstre diffusé par le
                        // serveur autoritaire (cf. `network_snapshot`) :
                        // appliquée directement à l'objet de même indice — la
                        // scène est la même des deux côtés (embarquée). Le
                        // garde-fou `attackable` évite d'écraser un autre objet
                        // si les scènes divergeaient (version différente).
                        let i = e.index as usize;
                        if let Some(o) = self.scene.objects.get_mut(i)
                            && o.controller.is_none()
                            && o.combat.as_ref().is_some_and(|c| c.attackable)
                        {
                            self.net_creature_last_snapshot.insert(i, now);
                            o.transform.position = glam::Vec3::from_array(e.position);
                            o.transform.rotation = glam::Quat::from_rotation_y(e.yaw);
                            if o.visible != e.visible {
                                self.net_visibility_dirty = true;
                            }
                            o.visible = e.visible;
                            // Animation répliquée : même mécanisme que
                            // pour les fantômes de joueurs, cf. `poll_network`.
                            if !e.anim_clip.is_empty()
                                && let Some(state) = o.animation.as_mut()
                            {
                                state.set_clip(e.anim_clip);
                            }
                        }
                        continue;
                    };
                    // Notre propre joueur : le serveur reste maître de sa
                    // position lui aussi (pas de prédiction locale, cf. la doc
                    // de `net_local_interp`) — même traitement que les autres
                    // joueurs, appliqué à `player_index` plutôt qu'à un
                    // fantôme dans `poll_network`.
                    if Some(pid) == self.net_player_id {
                        self.net_local_health = e.health;
                        self.net_local_kills = e.kills;
                        self.net_local_assists = e.assists;
                        self.net_local_interp.push(e, now);
                        continue;
                    }
                    let default_name = format!("Joueur {pid}");
                    let rp = self.ensure_remote_player(pid, &default_name);
                    rp.health = e.health;
                    rp.assists = e.assists;
                    rp.kills = e.kills;
                    rp.interp.push(e, now);
                }
            }
            // Évènements ponctuels : le seul exploité côté client est `Defeated`
            // (monstre vaincu, ex. par la boule de feu d'un joueur) — son + flash
            // immédiats, sans attendre le `visible: false` du prochain snapshot.
            ServerMsg::Event(crate::net::protocol::GameEvent::Defeated { index }) => {
                if let Some(o) = self.scene.objects.get_mut(index as usize)
                    && o.combat.as_ref().is_some_and(|c| c.attackable)
                {
                    o.visible = false;
                }
                self.attack_flash = 1.0;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Defeat);
            }
            // Un joueur réseau vient de tomber à 0 PV (GDD §5.3 : « la mort
            // d'un allié est un événement de groupe », diffusé à tous, pas
            // seulement à la victime). Nous : flash + son (comme avant). Un
            // allié : bannière dédiée (`ally_down_flash`, éditeur/HUD) — son
            // fantôme se masquer au prochain `Snapshot` ne suffisait pas, un
            // groupe doit *sentir* qu'il perd quelqu'un, pas le déduire.
            ServerMsg::Event(crate::net::protocol::GameEvent::PlayerDown { player_id, cause }) => {
                if Some(player_id) == self.net_player_id {
                    self.damage_flash = 1.0;
                    self.camera_shake = 1.0;
                    // Sprint 2 (`sprint10audit.md`) : mémorisé pour la bannière
                    // « Vaincu » enrichie (cf. `editor::hud::defeated_banner`) —
                    // seulement pour nous, une cause de mort n'a de sens que pour
                    // la victime qui la lit sur son propre écran.
                    self.death_cause = cause;
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
                } else {
                    self.ally_down_flash = 1.0;
                    // Phase O Sprint 1 (`sprint2audijeu0718.md`, GDD §10.4 rang 2) : un
                    // allié à terre doit sonner différemment de notre propre défaite
                    // (`Sfx::Lose` ci-dessus) — jusqu'ici les deux branches jouaient le
                    // même son, aucun moyen de distinguer les deux événements à l'oreille.
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::AllyDown);
                }
            }
            // Mode de manche arbitré par le salon (Phase C, `sprint10audit.md`) :
            // aligne notre `AppState::objective` locale sur celle, autoritaire,
            // du serveur — sans ça, `update_round` (`app::combat`) resterait
            // sur le défaut `Vagues` local même dans un salon `Survie`/etc.,
            // et déclencherait une victoire locale prématurée à la dernière
            // manche vidée (le serveur, lui, la reboucle). La visibilité des
            // monstres reste dictée par les `Snapshot` (`EntityDelta::visible`),
            // ce message ne change que la *condition de victoire* évaluée
            // localement.
            ServerMsg::Event(crate::net::protocol::GameEvent::RoundObjective { objective }) => {
                self.objective = crate::app::multiplayer::RoundObjective::from_u8(objective);
            }
            // Fin de manche détaillée (Phase H, Sprint 1, GDD §9.2/§17.4) :
            // jusqu'ici `Win`/`Lose` tombaient dans le catch-all ci-dessous,
            // la bannière restant pilotée uniquement par notre simulation
            // locale (`has_won()`/`is_room_lost()`). Mémorisé pour
            // `editor::hud::round_summary_banner`, tant que la manche
            // suivante ne l'a pas remplacé (`restart_game`).
            ServerMsg::Event(crate::net::protocol::GameEvent::Win { summary, contract }) => {
                self.round_summary = Some(summary);
                self.round_summary_won = true;
                self.round_contract_label =
                    contract.map(|c| crate::app::multiplayer::Contract::from_u8(c).label());
            }
            ServerMsg::Event(crate::net::protocol::GameEvent::Lose { summary }) => {
                self.round_summary = Some(summary);
                self.round_summary_won = false;
                self.round_contract_label = None;
            }
            // Bannière de vague (Phase H, Sprint 2, GDD §17.2) : jusqu'ici
            // jamais émis par la boucle de jeu (`bin/server.rs`), donc jamais
            // reçu ici — tombait aussi dans le catch-all.
            ServerMsg::Event(crate::net::protocol::GameEvent::WaveStart { wave }) => {
                self.wave_banner_flash = 1.0;
                self.wave_banner_wave = wave;
            }
            // Rejet **fatal** (version de protocole incompatible…) : on affiche
            // la raison et on n'insiste JAMAIS — désarmer la reconnexion
            // automatique est essentiel, sinon le client re-tenterait en boucle
            // contre un serveur qui le refusera à chaque fois.
            ServerMsg::JoinRejected { reason } => {
                log::warn!("Multijoueur : connexion refusée par le serveur : {reason}");
                self.reset_network_session();
                self.net_reconnect = None;
                self.net_last_connect = None;
                self.net_last_server_msg = None;
                self.net_status = reason;
            }
        }
    }

    /// Renvoie le fantôme du joueur `id`, en le créant s'il n'existe pas
    /// encore : clone du gabarit pilotable local, mais sans contrôleur ni
    /// prédiction propre — le serveur reste autoritaire sur sa position,
    /// appliquée directement au `transform` (`poll_network`). `PhysicsKind::
    /// Kinematic` (et non `None`) : un corps kinématique scripté (comme les
    /// créatures, cf. `runtime::physics::resolve_scripted_moves`) fait de ce
    /// fantôme un obstacle réel pour le joueur local — sans quoi il n'a aucun
    /// collider et le joueur local peut le traverser/se superposer avec lui
    /// (demande gameplay « les entités mobiles ne doivent pas se retrouver
    /// superposées »). Masqué tant qu'invisible, `Physics::build` lui refuse
    /// alors tout corps (cf. sa doc) — un rebuild est déclenché à chaque
    /// bascule de visibilité (`poll_network`/`handle_server_msg`) pour que ce
    /// corps apparaisse/disparaisse en même temps que le fantôme à l'écran.
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
                physics: crate::runtime::physics::PhysicsKind::Kinematic,
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
                    health: None,
                    kills: None,
                    assists: None,
                },
            );
        }
        self.remote_players
            .get_mut(&id)
            .expect("vient d'être inséré juste au-dessus")
    }
}

/// Construit le `ClientMsg::Input` envoyé au serveur à partir de l'état local
/// **complet** — exactement les mêmes sources que la prédiction locale de
/// `sim_step` : joystick/croix tactile + flèches + WASD + stick gauche manette,
/// tous relatifs à la caméra (`network_move_axes`), poussée tactile W/S
/// (`touch_thrust`, même formule que `AppState::advance_play` :
/// `-sin(yaw)`/`-cos(yaw)`, pour rester cohérent avec le mouvement prédit
/// localement), gyroscope si l'objet joueur l'active, et saut/attaque venant
/// aussi bien du clavier que des **boutons tactiles nommés**
/// (`Controller::jump_button`/`attack_button`) — omettre l'une de ces sources
/// laisserait le serveur ignorer un mouvement que la prédiction locale affiche
/// pourtant, et la réconciliation finirait par tirer le joueur en arrière pour
/// un mouvement qu'il a réellement fait (cf. docs/audits/app-network.md).
fn network_input_msg(
    inp: &super::PlayerInput,
    camera_yaw: f32,
    player: Option<&crate::scene::SceneObject>,
    weapon: u8,
) -> crate::net::protocol::ClientMsg {
    let player_yaw = player.map(|o| o.transform.rotation.to_euler(glam::EulerRot::YXZ).0);
    let (mut mx, mut my) = network_move_axes(inp, camera_yaw, player_yaw);
    let ctrl = player.and_then(|o| o.controller.as_ref());
    if let Some(c) = ctrl {
        // Gyroscope : la prédiction locale l'applique brut (pas caméra-relative,
        // cf. `sim_step`) quand `Controller::gyro` est actif — même convention
        // joystick que le reste (`move_y` positif = avant, le serveur nie en Z).
        if c.gyro {
            mx += inp.tilt.0;
            my += inp.tilt.1;
        }
    }
    let touch_jump =
        ctrl.is_some_and(|c| !c.jump_button.is_empty() && inp.buttons.contains(&c.jump_button));
    let touch_attack =
        ctrl.is_some_and(|c| !c.attack_button.is_empty() && inp.buttons.contains(&c.attack_button));
    let touch_fire =
        ctrl.is_some_and(|c| !c.fire_button.is_empty() && inp.buttons.contains(&c.fire_button));
    let touch_heal =
        ctrl.is_some_and(|c| !c.heal_button.is_empty() && inp.buttons.contains(&c.heal_button));
    crate::net::protocol::ClientMsg::Input {
        move_x: mx,
        move_y: my,
        // Orientation prédite localement (celle que CE joueur voit à son écran) :
        // le serveur l'applique à son objet et en fait la direction de ses tirs
        // (cf. `ClientMsg::Input::aim_yaw`). 0.0 si l'objet joueur n'existe pas
        // (encore) — le neutre, même valeur qu'à l'apparition.
        aim_yaw: player_yaw.unwrap_or(0.0),
        attack: inp.attack || touch_attack,
        jump: inp.jump || touch_jump,
        // Tir à distance : touche clavier (K) ou bouton tactile nommé
        // (`Controller::fire_button`) — le serveur simule le tir et sa recharge
        // (cf. `app::fireball`), ce client ne fait qu'exprimer l'intention.
        fire: inp.fire || touch_fire,
        weapon,
        // Soin coopératif (cf. `app::health`) : touche clavier (H) ou bouton
        // tactile nommé (`Controller::heal_button`) — résolu côté serveur.
        heal: inp.heal || touch_heal,
    }
}

fn network_move_axes(
    inp: &super::PlayerInput,
    camera_yaw: f32,
    player_yaw: Option<f32>,
) -> (f32, f32) {
    let raw_mx = (inp.joy.0 + inp.key_move.0 + inp.gamepad_move.0).clamp(-1.0, 1.0);
    let raw_my = (inp.joy.1 + inp.key_move.1 + inp.gamepad_move.1).clamp(-1.0, 1.0);
    let (mut mx, mut my) = super::simulation::camera_relative_move(raw_mx, raw_my, camera_yaw);
    // `thrust()` (clavier + pavé tactile W/A/S/D) et non `key_thrust` seul : le
    // pavé de l'APK doit être vu par le serveur exactement comme le clavier.
    let thrust = inp.thrust();
    if thrust != 0.0
        && let Some(yaw) = player_yaw
    {
        // Convention **joystick** attendue par le serveur (cf. `sim_step` :
        // `vz += -move_y × vitesse`), pas du Z monde : `move_y` positif = avant
        // (-Z à yaw 0). La composante monde de la poussée est `vz = -cos(yaw)`,
        // donc `move_y = +cos(yaw)` une fois la négation du serveur appliquée —
        // envoyer `-yaw.cos()` ici inverserait l'avance W en Z côté serveur, et
        // la réconciliation tirerait le joueur à contresens de sa prédiction
        // locale dès que l'écart dépasse `SNAP_THRESHOLD` (cf.
        // docs/audits/app-network.md).
        mx += thrust * -yaw.sin();
        my += thrust * yaw.cos();
    }
    (mx, my)
}

/// Compte Firebase, chat, classement : desktop uniquement (`ureq`/`net::firebase`
/// ne ciblent pas mobile, cf. `Cargo.toml`/`net/mod.rs`).
#[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
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
    /// authentifiés (cf. `net::firebase`). Sans effet si non
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
        if self.chat_busy {
            return;
        }
        if let Err(reason) = crate::net::firebase::valid_chat_text(&text) {
            self.net_status = format!("Message non envoyé : {reason}");
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
    /// réservée au serveur de jeu, cf. `net::firebase`). Sans effet
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

    /// Rafraîchit la liste des `uid` en ligne (Phase L Sprint 1,
    /// `sprint2audijeu0718.md` — lecture publique de `/presence`, ne nécessite
    /// pas de compte connecté). Sans effet si une requête est déjà en cours.
    pub fn request_refresh_online_players(&mut self, api_key: String, database_url: String) {
        if self.online_players_busy {
            return;
        }
        self.online_players_busy = true;
        let tx = self.online_players_tx.clone();
        std::thread::spawn(move || {
            let config = crate::net::firebase::FirebaseConfig {
                api_key,
                database_url,
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let _ = tx.send(crate::net::firebase::list_online_players(&config, now_ms));
        });
    }

    /// Applique le résultat d'une requête de présence en attente, s'il y en a un.
    fn poll_online_players(&mut self) {
        while let Ok(result) = self.online_players_rx.try_recv() {
            self.online_players_busy = false;
            match result {
                Ok(uids) => self.online_players = uids,
                Err(e) => log::warn!("Présence : requête échouée : {e}"),
            }
        }
    }

    /// Envoie le heartbeat de présence (`net::firebase::set_presence`) pour le
    /// compte Firebase connecté (Phase L Sprint 1) : sans effet si aucun
    /// compte n'est connecté (`firebase_id_token` absent) — la partie peut
    /// rester anonyme, cf. `firebase_uid`. Fire-and-forget, comme
    /// `request_send_chat_message` : pas de suivi de résultat, un heartbeat
    /// manqué se rattrape au suivant.
    pub fn request_presence_heartbeat(&mut self, api_key: String, database_url: String) {
        let (Some(uid), Some(id_token)) =
            (self.firebase_uid.clone(), self.firebase_id_token.clone())
        else {
            return;
        };
        std::thread::spawn(move || {
            let config = crate::net::firebase::FirebaseConfig {
                api_key,
                database_url,
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if let Err(e) = crate::net::firebase::set_presence(&config, &uid, &id_token, now_ms) {
                log::warn!("Présence : heartbeat échoué : {e}");
            }
        });
    }
}

/// Récupère les messages d'un salon et les convertit en `ChatLine`
/// (représentation universelle, cf. sa doc).
#[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
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

/// iOS uniquement : `net::client` n'y est pas encore compilé (cf. `net/mod.rs`).
/// wasm32 a désormais sa propre implémentation de `net::client::NetClient`
/// (Sprint 116, `web_sys::WebSocket`) — le bloc `not(target_os = "ios")`
/// ci-dessus s'applique donc aussi au web.
#[cfg(target_os = "ios")]
impl AppState {
    pub fn connect_to_server(&mut self, _url: &str, _name: &str) {
        self.net_status = "Multijoueur indisponible sur iOS".to_string();
    }

    /// Stub de parité avec le bloc `not(target_os = "ios")` ci-dessus (Sprint 3,
    /// `sprint10audit.md` ; Sprints 20-21, `sprintreflecion.md`) : classe,
    /// code de partie et mode sont ignorés, comme le reste de la connexion
    /// sur cette cible.
    pub fn connect_to_server_as(
        &mut self,
        _url: &str,
        _name: &str,
        _class: crate::app::multiplayer::PlayerClass,
        _room: &str,
        _objective: crate::app::multiplayer::RoundObjective,
    ) {
        self.net_status = "Multijoueur indisponible sur iOS".to_string();
    }

    pub fn disconnect_from_server(&mut self) {}

    pub fn is_connected(&self) -> bool {
        false
    }

    pub fn is_locally_defeated(&self) -> bool {
        false
    }

    pub fn multiplayer_roster(&self) -> Vec<(String, Option<f32>, Option<u32>, bool)> {
        Vec::new()
    }

    pub fn displayed_kill_count(&self) -> u32 {
        self.score()
    }

    pub fn displayed_assist_count(&self) -> u32 {
        0
    }

    pub(super) fn poll_network(&mut self) {}

    pub(super) fn apply_local_network_position(&mut self) {}
}

/// Compte Firebase, chat, classement : hors mobile (iOS et Android), cf. le
/// bloc desktop équivalent plus haut.
#[cfg(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))]
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

    pub fn request_refresh_online_players(&mut self, _api_key: String, _database_url: String) {}

    pub fn request_presence_heartbeat(&mut self, _api_key: String, _database_url: String) {}
}

#[cfg(all(
    test,
    not(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))
))]
#[path = "network_client_tests.rs"]
mod tests;
