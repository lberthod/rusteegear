//! Réseau multijoueur (SPRINT_MMORPG.md, phases N/O/P) : protocole, transport
//! WebSocket, puis Firebase RTDB en backend annexe (Sprint 56+). Ce module ne
//! dépend jamais de `gfx`/`egui`/`winit` : il doit rester utilisable tel quel
//! depuis `src/bin/server.rs` (headless).
//!
//! `server_loop`/`client` sont desktop-only (comme `rfd`/`ureq`, cf. `Cargo.toml`) :
//! `tokio`/`tokio-tungstenite` ne sont pas encore visés sur mobile — seul le
//! `protocol` (types + sérialisation, sans I/O) est compilé partout, y compris
//! dans la lib `cdylib` Android/iOS.

pub mod interpolation;
pub mod protocol;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod client;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod server_loop;
