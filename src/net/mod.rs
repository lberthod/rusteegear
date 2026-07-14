//! Réseau multijoueur : protocole, transport WebSocket, puis Firebase RTDB en
//! backend annexe. Ce module ne dépend jamais de `gfx`/`egui`/`winit` : il
//! doit rester utilisable tel quel depuis `src/bin/server.rs` (headless).
//!
//! `server_loop` reste desktop-only (un serveur headless n'a pas de sens sur
//! mobile). `client` est desktop + Android (rejoindre un salon depuis un APK,
//! cf. `app::network_client`) — pas encore iOS. `firebase`
//! (comptes/chat/classement, `ureq`) reste desktop-only : `rfd`/`ureq` ne
//! ciblent pas mobile, cf. `Cargo.toml`. Le `protocol` (types + sérialisation,
//! sans I/O) est compilé partout, y compris dans la lib `cdylib` Android/iOS.

pub mod interpolation;
pub mod protocol;

#[cfg(not(target_os = "ios"))]
pub mod client;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod firebase;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod server_loop;
