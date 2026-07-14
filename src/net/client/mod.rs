//! Transport WebSocket côté client : deux implémentations derrière la même API
//! publique (`NetClient::connect`/`connect_to_lobby`/`send`, champ `inbox`), pas
//! deux modules à connaître séparément pour les appelants (`app::network_client`).
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
