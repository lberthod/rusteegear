//! Export réalité virtuelle (Meta Quest et casques OpenXR Android) — roadmap
//! `docs/roadmapExportVRQuest24septembre.md`.
//!
//! - `math` : projections stéréo (partout, testé sur le poste de dev) ;
//! - `sim` : casque simulé (profil Quest 3, tête/manettes au clavier-souris),
//!   utilisé par le simulateur desktop `cargo run --bin quest_sim` ;
//! - `test_scene` : scène de test commune au simulateur et à l'APK ;
//! - `rig` : place la pièce du joueur dans le monde du jeu ;
//! - `content` : ce que la session affiche (cubes ou partie Rivière), commun
//!   à l'APK et au simulateur ;
//! - `hello` : session OpenXR réelle, seulement dans l'APK VR (Android +
//!   feature `vr`, produit par `packaging/build_quest.sh`).

pub mod balls;
pub mod content;
pub mod hands;
pub mod input;
pub mod locomotion;
pub mod math;
pub mod quality;
pub mod rig;
pub mod sim;
pub mod test_scene;
pub mod ui;

#[cfg(all(target_os = "android", feature = "vr"))]
pub mod actions;
#[cfg(all(target_os = "android", feature = "vr"))]
pub mod hello;
