//! Export réalité virtuelle (Meta Quest et casques OpenXR Android) — roadmap
//! `docs/roadmapExportVRQuest24septembre.md`.
//!
//! `math` (projections stéréo) compile partout et se teste sur le poste de dev ;
//! la session OpenXR elle-même n'existe que dans l'APK VR (Android + feature `vr`,
//! produit par `packaging/build_quest.sh`).

pub mod math;

#[cfg(all(target_os = "android", feature = "vr"))]
pub mod hello;
