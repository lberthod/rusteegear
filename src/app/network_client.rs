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
//! objets « fantômes » — sans physique ni script, leur position suit le
//! dernier `Snapshot` reçu, interpolée (cf. `net::interpolation::RemoteEntity`).

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

/// Un autre joueur réseau, affiché comme un objet fantôme dans la scène locale.
pub struct RemotePlayer {
    pub name: String,
    // `pub(super)` : lu par `AppState::remote_player_scene_indices` (app/mod.rs) pour
    // exclure les fantômes de l'interpolation de rendu locale.
    pub(super) scene_index: usize,
    interp: crate::net::interpolation::RemoteEntity,
    /// Dernière vie connue (0..1, cf. `app::health`, GAMEDESIGN_EN_LIGNE.md
    /// §3.1/§3.4) : lue telle quelle du dernier `Snapshot` reçu, **pas**
    /// interpolée comme la position (une vie n'a pas besoin d'être lissée
    /// visuellement, contrairement à un mouvement) — `None` tant qu'aucun
    /// snapshot ne l'a renseignée.
    pub health: Option<f32>,
    /// Frags individualisés (brique de progression pour un futur MMORPG,
    /// GAMEDESIGN_EN_LIGNE.md) — même provenance que `health` : lus tels
    /// quels du dernier `Snapshot`, `None` tant qu'aucun n'est arrivé.
    pub kills: Option<u32>,
}

/// Fraction de l'écart comblée à chaque appel de `apply_local_network_position`
/// quand `reconcile` dépasse `SNAP_THRESHOLD` (cf. `SPRINTNETWORK.md`,
/// `AUDIT_LATENCE_MULTIJOUEUR.md`).
///
/// **Ne jamais figer une cible sur plusieurs frames** : `Physics::step`
/// (`runtime/physics.rs`) recopie la pose du corps rigide dans
/// `transform.position` à **chaque** tick, avant que cette fonction ne
/// s'exécute (sync à sens unique physique → transform, jamais l'inverse) —
/// une correction qui interpolerait vers une cible mémorisée d'un appel
/// précédent serait donc remplacée par la vraie position physique dès le
/// tick suivant, sans jamais converger (cf. docs/audits/app-network.md pour
/// le bug réel que ça a causé). Chaque frame où l'écart dépasse le seuil, on
/// ne fait donc qu'un petit pas (`CORRECTION_PULL`) depuis la position
/// **fraîche** de ce tick vers la position autoritative — jamais une valeur
/// mémorisée. Le mouvement piloté par l'input n'est donc jamais interrompu ;
/// seule une légère dérive vers la position serveur s'ajoute par-dessus,
/// tick après tick, tant que l'écart reste significatif.
const CORRECTION_PULL: f32 = 0.15;

/// Fenêtre (s) de l'historique des positions prédites du joueur local
/// (`net_local_history`) : doit couvrir la latence aller-retour vers le serveur,
/// son tick et le retard d'interpolation — 1 s laisse une marge confortable
/// au-dessus des ~150-250 ms mesurés vers le VPS réel, sans retenir assez de
/// points pour coûter quoi que ce soit (une entrée par frame, ~60-120).
const HISTORY_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

/// Vitesse (m/s) sous laquelle le joueur local est considéré **immobile** pour le
/// rattrapage doux à l'arrêt (cf. `IDLE_SETTLE_PULL`) — assez basse pour ne jamais
/// se déclencher en cours de déplacement réel (vitesse de marche ≥ 3 m/s).
const IDLE_SPEED_EPSILON: f32 = 0.15;

/// Écart minimal (m) déclenchant le rattrapage à l'arrêt : en-deçà, les deux
/// positions sont visuellement confondues — inutile d'écrire des micro-corrections
/// sans fin (et de marquer le transform « modifié » pour l'interpolation de rendu).
const IDLE_SETTLE_MIN: f32 = 0.03;

/// Fraction de l'écart comblée par frame quand le joueur est **immobile** et que
/// l'écart avec la position serveur est sous `SNAP_THRESHOLD` (donc ignoré par
/// `reconcile`). Sans ce rattrapage, chaque client garde à l'arrêt un décalage
/// permanent (jusqu'à ~0,5 m) avec la vérité serveur — le serveur (physique
/// plus ancienne, freinage plus mou) s'arrête systématiquement quelques
/// dizaines de cm plus loin que la prédiction locale, et les autres clients ne
/// voient pas la même position relative que le joueur lui-même (cf.
/// docs/audits/app-network.md). 5 %/frame ≈ imperceptible (~0,25 s pour
/// combler la moitié de l'écart), et uniquement à l'arrêt — le ressenti en
/// mouvement ne change pas.
const IDLE_SETTLE_PULL: f32 = 0.05;

/// Intervalle minimal entre deux envois de `ClientMsg::Input` (cf.
/// `SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.2) — aligné sur
/// `SERVER_TICK` (`src/bin/server.rs`, ~60 Hz) : le serveur ne consomme
/// l'input qu'une fois par tick, envoyer plus souvent (jusqu'à la fréquence
/// d'affichage du client, potentiellement 144 Hz+) ne change rien au
/// gameplay et gaspille de la bande passante des deux côtés. Constante
/// dupliquée plutôt qu'importée de `src/bin/server.rs` : ce dernier est un
/// binaire séparé (pas une dépendance de la lib), cf. `net/mod.rs`.
const INPUT_SEND_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// Silence serveur maximal avant de déclarer la connexion morte (watchdog
/// applicatif, cf. `AppState::net_last_server_msg`) : `NetClient::is_alive()`
/// ne voit pas une connexion TCP à moitié morte (half-open, façade Caddy qui
/// gèle) — un serveur sain diffuse un `Snapshot` par tick (~60 Hz), 8 s sans
/// **aucun** message est donc pathologique sans ambiguïté. Hiérarchie des
/// timeouts, volontaire : `CREATURE_SNAPSHOT_TIMEOUT` (2,5 s,
/// `app::simulation`) < **8 s** < `CLIENT_TIMEOUT` serveur (60 s,
/// `src/bin/server.rs`) — le filet créatures reprend la simulation locale
/// *avant* que la coupure ne soit déclarée ici (rien ne se fige à l'écran
/// pendant le diagnostic), et le serveur garde le joueur assez longtemps
/// pour couvrir toute la fenêtre de reconnexion.
const NET_SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// Tentatives de reconnexion automatique avant d'abandonner (cf.
/// `reconnect_delay` pour la cadence) : au-delà, le serveur est
/// vraisemblablement hors service — on rend la main au joueur (`net_status`
/// explicite) plutôt que de marteler un serveur mort indéfiniment.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// Délai avant la tentative de reconnexion `attempt` (1-indexée) : backoff
/// exponentiel 1 s, 2 s, 4 s, 8 s, plafonné à 15 s — assez réactif pour
/// qu'une micro-coupure (Wi-Fi qui bascule, serveur redémarré) se répare en
/// quelques secondes, assez espacé pour ne pas inonder un serveur en
/// difficulté. Fonction pure, séparée de l'état pour être testable sans
/// socket ni horloge.
pub(crate) fn reconnect_delay(attempt: u32) -> std::time::Duration {
    let exp = attempt.saturating_sub(1).min(4);
    std::time::Duration::from_secs((1u64 << exp).min(15))
}

/// État d'une reconnexion automatique en cours (cf. `AppState::net_reconnect`
/// et la logique dans `poll_network`).
pub(super) struct ReconnectState {
    /// Tentative courante (1-indexée, plafonnée à `MAX_RECONNECT_ATTEMPTS`).
    attempt: u32,
    /// Prochain essai au plus tôt (backoff, cf. `reconnect_delay`).
    next_try: crate::time_compat::Instant,
    /// Tentative de fond en vol, s'il y en a une : la connexion native est
    /// **bloquante** (poignée de main TCP/WebSocket complète), elle vit donc
    /// dans un thread éphémère qui pousse son résultat ici — même patron
    /// canal + `try_recv` que les imports glTF ou les requêtes IA (cf.
    /// `net::client::native`). Inutile sur wasm : `connect` n'y bloque
    /// jamais, l'échec différé arrive par `is_alive()`.
    #[cfg(not(target_arch = "wasm32"))]
    pending: Option<std::sync::mpsc::Receiver<Result<crate::net::client::NetClient, String>>>,
}

/// État de la connexion multijoueur, pour le HUD et les décisions internes
/// (cf. `AppState::net_connection_state`) — `net_status` reste le texte
/// affiché, cet enum donne la forme exploitable par du code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetConnState {
    /// Aucune connexion (jamais connecté, ou déconnexion volontaire/définitive).
    Offline,
    /// Connexion établie côté transport, `Welcome` pas encore reçu.
    Connecting,
    /// Connecté et vivant (transport sain + serveur entendu récemment).
    Connected,
    /// Connexion perdue, reconnexion automatique en cours.
    Reconnecting { attempt: u32 },
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
                    crate::net::protocol::DEFAULT_LOBBY.to_string(),
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
                o.visible = false;
            }
        }
        self.remote_players.clear();
        self.net_local_interp = crate::net::interpolation::RemoteEntity::default();
        self.net_local_health = None;
        self.net_local_kills = None;
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
        let Some((url, name, lobby)) = self.net_last_connect.clone() else {
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
                        self.net_local_interp.push(e, now);
                        continue;
                    }
                    let default_name = format!("Joueur {pid}");
                    let rp = self.ensure_remote_player(pid, &default_name);
                    rp.health = e.health;
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
            // Un joueur réseau vient de tomber à 0 PV (GAMEDESIGN_EN_LIGNE.md
            // §3.1) : réagit seulement si c'est **nous** (son) — la disparition
            // d'un allié est déjà visible (son fantôme se masque, cf. le
            // `Snapshot` suivant), pas la peine de sonoriser la mort de chacun.
            ServerMsg::Event(crate::net::protocol::GameEvent::PlayerDown { player_id }) => {
                if Some(player_id) == self.net_player_id {
                    self.damage_flash = 1.0;
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
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
                    health: None,
                    kills: None,
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
/// `sim_step` : joystick/croix tactile + flèches (`network_move_axes`),
/// poussée clavier/tactile W/S (`key_thrust`/`touch_thrust`, même formule que
/// `AppState::advance_play` : `-sin(yaw)`/`-cos(yaw)`, pour rester cohérent
/// avec le mouvement prédit localement), gyroscope si l'objet joueur l'active,
/// et saut/attaque venant aussi bien du clavier que des **boutons tactiles
/// nommés** (`Controller::jump_button`/`attack_button`) — omettre l'une de ces
/// sources laisserait le serveur ignorer un mouvement que la prédiction
/// locale affiche pourtant, et la réconciliation finirait par tirer le joueur
/// en arrière pour un mouvement qu'il a réellement fait (cf.
/// docs/audits/app-network.md).
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
    let raw_mx = (inp.joy.0 + inp.key_move.0).clamp(-1.0, 1.0);
    let raw_my = (inp.joy.1 + inp.key_move.1).clamp(-1.0, 1.0);
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
}

#[cfg(all(
    test,
    not(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))
))]
mod tests {
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
                ClientMsg::Join { .. } => {
                    server_app.spawn_network_player(id);
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
                name: "Alice".to_string(),
                firebase_uid: Some("uid-alice".to_string()),
                lobby: crate::net::protocol::DEFAULT_LOBBY.to_string(),
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
        }
        settle(&mut alice, &mut bob);
        let server_health = server_app
            .network_player_health(alice_id)
            .expect("Alice doit avoir une vie réseau");
        assert!(
            server_health < 1.0,
            "{BITE_CONTACT_TICKS} ticks de contact avec la Créature 1 (mordeuse) \
             auraient dû infliger des dégâts : {server_health}"
        );
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
}
