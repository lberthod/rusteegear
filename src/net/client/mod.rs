//! Transport WebSocket côté client : deux implémentations derrière la même API
//! publique (`NetClient::connect`/`connect_to_lobby`/`send`/`is_alive`, champ
//! `inbox`), pas deux modules à connaître séparément pour les appelants
//! (`app::network_client`).
//!
//! **Contrat `is_alive()`** (commun aux deux implémentations) : `true` tant que
//! le transport peut encore livrer ou transmettre des messages, `false` de façon
//! définitive dès que la connexion est morte (fermée par le serveur, perte
//! réseau, échec de connexion différé côté web). Un `NetClient` mort ne revit
//! jamais — la reconnexion passe par une **nouvelle** instance (cf.
//! `AppState::poll_network`, qui s'en sert pour détecter la coupure et relancer
//! une connexion avec backoff). Attention, ce n'est qu'une détection de
//! transport : une connexion TCP à moitié morte (half-open, façade qui gèle)
//! peut rester `is_alive()` — le watchdog applicatif de `AppState`
//! (`NET_SILENCE_TIMEOUT`) couvre ce cas-là.
//!
//! **Contrat `handshake()`** (roadmap post-audit UX v2 2026-09-04, 2.1) :
//! `connect`/`connect_to_lobby` **ne bloquent jamais**, sur aucune cible — ils
//! rendent la main dès que la connexion est *lancée*, et l'appelant interroge
//! `handshake()` à chaque frame : `Pending` tant que la poignée de main
//! TCP/TLS/WebSocket est en cours, `Open` une fois la socket ouverte (le
//! `Join` est parti, le `Welcome` arrivera par `inbox`), `Failed(raison)` si
//! elle a échoué (serveur injoignable, délai `CONNECT_TIMEOUT` dépassé, TLS
//! refusé…). Avant : `native::connect` bloquait le **thread de rendu**
//! jusqu'au bout de la poignée de main — jusqu'à ~75 s de gel sur une IP
//! filtrée (timeout TCP du système), sans aucun retour à l'écran.
//!
//! - **`native`** (desktop/Android) : `tokio` + `tokio_tungstenite`, thread de
//!   fond dédié qui `block_on` la connexion entière ; l'issue de la poignée de
//!   main remonte par un canal que `handshake()` sonde sans bloquer.
//! - **`web`** (Sprint 116, wasm32) : `web_sys::WebSocket`, l'API native du
//!   navigateur — ni `tokio` ni threads OS n'existent sur cette cible.
//!   `web_sys::WebSocket::new` ne bloque jamais (la connexion TCP/TLS est
//!   gérée par le navigateur, invisible depuis Rust) : `Ok` garantit
//!   seulement que l'URL est syntaxiquement valide, `handshake()` reste
//!   `Pending` jusqu'à l'événement `open` du navigateur (et passe `Failed`
//!   sur un `error`/`close` survenu avant). Le navigateur n'a pas de délai
//!   de connexion réglable : c'est l'appelant (`AppState::poll_network`) qui
//!   applique `CONNECT_TIMEOUT` à un `Pending` qui s'éternise.

use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::NetClient;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::NetClient;

/// Délai maximal accordé à la poignée de main (roadmap post-audit UX v2
/// 2026-09-04, 2.1) : au-delà, la connexion est abandonnée avec
/// `CONNECT_TIMEOUT_REASON`. 5 s couvre largement un TLS + WebSocket vers le
/// VPS réel (~300-600 ms) tout en restant sous les 8 s du watchdog applicatif
/// (`app::network_client::NET_SILENCE_TIMEOUT`) — un serveur injoignable se
/// signale avant qu'un autre mécanisme ne s'en mêle. Appliqué côté natif par
/// `tokio::time::timeout` dans le thread réseau, et par l'appelant sur toute
/// cible (seul filet côté web, cf. la doc du module).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Raison rapportée quand `CONNECT_TIMEOUT` expire — la même chaîne des deux
/// côtés (thread natif et appelant), pour qu'un dépassement simultané ne
/// produise pas deux messages différents à l'écran.
pub const CONNECT_TIMEOUT_REASON: &str = "Serveur injoignable (délai dépassé)";

/// Issue de la poignée de main d'un `NetClient` (cf. la doc du module).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handshake {
    /// Connexion lancée, poignée de main en cours.
    Pending,
    /// Socket ouverte : le `Join` est parti, le serveur peut répondre.
    Open,
    /// Poignée de main échouée — raison lisible pour le joueur.
    Failed(String),
}
