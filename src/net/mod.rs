//! Réseau multijoueur (SPRINT_MMORPG.md, phases N/O/P) : protocole, puis transport
//! (Sprint 53), client réseau (Sprint 54+), et Firebase RTDB en backend annexe
//! (Sprint 56+). Ce module ne dépend jamais de `gfx`/`egui`/`winit` : il doit rester
//! utilisable tel quel depuis `src/bin/server.rs` (headless).

pub mod protocol;
