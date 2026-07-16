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
//! - **`native`** (desktop/Android) : `tokio` + `tokio_tungstenite`, thread de
//!   fond dédié qui `block_on` la connexion entière.
//! - **`web`** (Sprint 116, wasm32) : `web_sys::WebSocket`, l'API native du
//!   navigateur — ni `tokio` ni threads OS n'existent sur cette cible. La
//!   différence de fond n'est pas cosmétique : `web_sys::WebSocket::new` ne
//!   **bloque jamais** (la connexion TCP/TLS est gérée par le navigateur,
//!   invisible depuis Rust), alors que `native::NetClient::connect` bloque
//!   jusqu'à la poignée de main WebSocket. `connect` réussit donc toujours côté
//!   web tant que l'URL est syntaxiquement valide — l'échec réel (serveur
//!   injoignable, `wss://` requis en HTTPS, etc.) arrive plus tard, via
//!   `is_connected()`/`net_status` au lieu d'un `Err` immédiat.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::NetClient;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::NetClient;
