
/// Un autre joueur réseau, affiché comme un objet fantôme dans la scène locale.
pub struct RemotePlayer {
    pub name: String,
    // `pub(crate)` : lu par `AppState::remote_player_scene_indices` (app/mod.rs) pour
    // exclure les fantômes de l'interpolation de rendu locale.
    pub(crate) scene_index: usize,
    pub(super) interp: crate::net::interpolation::RemoteEntity,
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
    /// Assists individualisés (Phase L Sprint 3, `sprint2audijeu0718.md`, GDD
    /// §8.3) — même provenance que `kills` ci-dessus.
    pub assists: Option<u32>,
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
pub(super) const CORRECTION_PULL: f32 = 0.15;

/// Fenêtre (s) de l'historique des positions prédites du joueur local
/// (`net_local_history`) : doit couvrir la latence aller-retour vers le serveur,
/// son tick et le retard d'interpolation — 1 s laisse une marge confortable
/// au-dessus des ~150-250 ms mesurés vers le VPS réel, sans retenir assez de
/// points pour coûter quoi que ce soit (une entrée par frame, ~60-120).
pub(super) const HISTORY_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

/// Vitesse (m/s) sous laquelle le joueur local est considéré **immobile** pour le
/// rattrapage doux à l'arrêt (cf. `IDLE_SETTLE_PULL`) — assez basse pour ne jamais
/// se déclencher en cours de déplacement réel (vitesse de marche ≥ 3 m/s).
pub(super) const IDLE_SPEED_EPSILON: f32 = 0.15;

/// Écart minimal (m) déclenchant le rattrapage à l'arrêt : en-deçà, les deux
/// positions sont visuellement confondues — inutile d'écrire des micro-corrections
/// sans fin (et de marquer le transform « modifié » pour l'interpolation de rendu).
pub(super) const IDLE_SETTLE_MIN: f32 = 0.03;

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
pub(super) const IDLE_SETTLE_PULL: f32 = 0.05;

/// Intervalle minimal entre deux envois de `ClientMsg::Input` (cf.
/// `SPRINTNETWORK.md`, `AUDIT_LATENCE_MULTIJOUEUR.md` §2.2) — aligné sur
/// `SERVER_TICK` (`src/bin/server.rs`, ~60 Hz) : le serveur ne consomme
/// l'input qu'une fois par tick, envoyer plus souvent (jusqu'à la fréquence
/// d'affichage du client, potentiellement 144 Hz+) ne change rien au
/// gameplay et gaspille de la bande passante des deux côtés. Constante
/// dupliquée plutôt qu'importée de `src/bin/server.rs` : ce dernier est un
/// binaire séparé (pas une dépendance de la lib), cf. `net/mod.rs`.
pub(super) const INPUT_SEND_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

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
pub(super) const NET_SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// Tentatives de reconnexion automatique avant d'abandonner (cf.
/// `reconnect_delay` pour la cadence) : au-delà, le serveur est
/// vraisemblablement hors service — on rend la main au joueur (`net_status`
/// explicite) plutôt que de marteler un serveur mort indéfiniment.
pub(super) const MAX_RECONNECT_ATTEMPTS: u32 = 5;

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
pub(crate) struct ReconnectState {
    /// Tentative courante (1-indexée, plafonnée à `MAX_RECONNECT_ATTEMPTS`).
    pub(super) attempt: u32,
    /// Prochain essai au plus tôt (backoff, cf. `reconnect_delay`).
    pub(super) next_try: crate::time_compat::Instant,
    /// Tentative de fond en vol, s'il y en a une : la connexion native est
    /// **bloquante** (poignée de main TCP/WebSocket complète), elle vit donc
    /// dans un thread éphémère qui pousse son résultat ici — même patron
    /// canal + `try_recv` que les imports glTF ou les requêtes IA (cf.
    /// `net::client::native`). Inutile sur wasm : `connect` n'y bloque
    /// jamais, l'échec différé arrive par `is_alive()`.
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    pub(super) pending: Option<std::sync::mpsc::Receiver<Result<crate::net::client::NetClient, String>>>,
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

/// Copie universelle de `net::firebase::MAX_CHAT_LEN` (mêmes raisons que
/// `ChatLine`/`LeaderboardLine`) : l'UI de chat (`editor::windows`) affiche
/// cette limite même sur les cibles où `net::firebase` n'existe pas.
pub(crate) const MAX_CHAT_LEN: usize = 240;
