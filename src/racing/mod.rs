//! HerRoad — un jeu de course arcade façon Trackmania : circuit de plaques de route en
//! rubans (dévers, collines, pont, sauts), voiture à physique maison, contre-la-montre en
//! trois tours avec points de passage, fantôme et médailles.
//!
//! Ce module est **pur** (glam + types de mesh) : circuit, voiture, règles et pilote de test
//! se vérifient sans GPU. Le branchement au moteur (scène, entrées manette, caméra, HUD)
//! vit dans `scene::demos::herroad` et `app::race`.

pub mod bot;
pub mod car;
pub mod layout;
pub mod race;
pub mod terrain;
pub mod track;

pub use car::{Car, CarInput};
pub use race::{Medal, Phase, Race, RaceEvent};
pub use track::{Track, TrackSpec};
