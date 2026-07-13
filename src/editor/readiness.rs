//! Contrôle qualité APK / « APK Readiness Check » (Sprint 32).
//!
//! Analyse la scène et la configuration de build pour signaler ce qui empêcherait
//! un export Android propre : scène vide, objets sans collider, textures trop
//! grandes ou introuvables, identité de bundle invalide… Chaque vérification
//! renvoie un statut + un message lisible.

use std::collections::BTreeSet;

use crate::app::build_config::{BuildConfig, valid_bundle_id};
use crate::runtime::physics::PhysicsKind;
use crate::scene::{MeshKind, Scene};

/// Texture au-delà de laquelle on alerte (limite courante des GPU mobiles).
const MAX_TEXTURE_PX: u32 = 4096;

/// Verdict d'une vérification individuelle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    pub fn icon(self) -> &'static str {
        match self {
            Status::Ok => "✅",
            Status::Warn => "⚠",
            Status::Fail => "❌",
        }
    }
}

/// Résultat d'une vérification : statut + libellé affiché.
pub struct Check {
    pub status: Status,
    pub label: String,
}

impl Check {
    fn new(status: Status, label: impl Into<String>) -> Self {
        Self {
            status,
            label: label.into(),
        }
    }
}

/// Analyse complète. La lecture des dimensions de textures touche le disque, donc
/// on n'appelle cette fonction qu'à la demande (bouton « Analyser »).
pub fn analyze(scene: &Scene, config: &BuildConfig) -> Vec<Check> {
    let mut checks = Vec::new();

    // --- Scène ---
    if scene.objects.is_empty() {
        checks.push(Check::new(
            Status::Fail,
            "Scène vide : aucun objet à afficher",
        ));
    } else {
        checks.push(Check::new(
            Status::Ok,
            format!("{} objet(s) dans la scène", scene.objects.len()),
        ));
    }

    let has_ground = scene
        .objects
        .iter()
        .any(|o| matches!(o.mesh, MeshKind::Plane));
    checks.push(if has_ground {
        Check::new(Status::Ok, "Sol présent (plan)")
    } else {
        Check::new(
            Status::Warn,
            "Aucun sol : les objets risquent de tomber dans le vide",
        )
    });

    // --- Éclairage ---
    let lit = scene.light.ambient > 0.0
        || scene.light.color.iter().any(|&c| c > 0.0)
        || scene.light.dir.iter().any(|&d| d.abs() > f32::EPSILON);
    checks.push(if lit {
        Check::new(Status::Ok, "Éclairage configuré")
    } else {
        Check::new(Status::Fail, "Aucune lumière : la scène sera noire")
    });

    // --- Jouabilité ---
    let scripted = scene
        .objects
        .iter()
        .filter(|o| !o.script.trim().is_empty())
        .count();
    checks.push(if scripted > 0 {
        Check::new(
            Status::Ok,
            format!("{scripted} objet(s) avec script (interactivité)"),
        )
    } else {
        Check::new(Status::Warn, "Aucun script : la scène sera statique")
    });

    // --- Physique / colliders ---
    let no_collider = scene
        .objects
        .iter()
        .filter(|o| o.physics == PhysicsKind::None)
        .count();
    if no_collider > 0 {
        checks.push(Check::new(
            Status::Warn,
            format!("{no_collider} objet(s) sans collider (pas de physique)"),
        ));
    } else if !scene.objects.is_empty() {
        checks.push(Check::new(Status::Ok, "Tous les objets ont un collider"));
    }

    // --- Textures (lecture des dimensions sur disque) ---
    let textures: BTreeSet<&str> = scene
        .objects
        .iter()
        .map(|o| o.texture.trim())
        .filter(|t| !t.is_empty())
        .collect();
    let mut too_big = 0;
    let mut missing = 0;
    for tex in &textures {
        match texture_dimensions(tex) {
            Some((w, h)) => {
                if w > MAX_TEXTURE_PX || h > MAX_TEXTURE_PX {
                    too_big += 1;
                }
            }
            None => missing += 1,
        }
    }
    if missing > 0 {
        checks.push(Check::new(
            Status::Fail,
            format!("{missing} texture(s) introuvable(s) sur le disque"),
        ));
    }
    if too_big > 0 {
        checks.push(Check::new(
            Status::Fail,
            format!("{too_big} texture(s) > {MAX_TEXTURE_PX} px (incompatibles mobile)"),
        ));
    }
    if missing == 0 && too_big == 0 {
        checks.push(Check::new(
            Status::Ok,
            format!("{} texture(s) compatibles mobile", textures.len()),
        ));
    }

    // --- Identité de build ---
    checks.push(if config.app_name.trim().is_empty() {
        Check::new(Status::Fail, "Nom de l'application manquant")
    } else {
        Check::new(Status::Ok, format!("Nom : {}", config.app_name.trim()))
    });

    checks.push(if valid_bundle_id(&config.bundle_id) {
        Check::new(Status::Ok, format!("Package ID : {}", config.bundle_id))
    } else {
        Check::new(
            Status::Fail,
            format!("Package ID invalide : {}", config.bundle_id),
        )
    });

    checks.push(if config.version.trim().is_empty() {
        Check::new(Status::Fail, "Version manquante")
    } else {
        Check::new(Status::Ok, format!("Version : {}", config.version.trim()))
    });

    // --- Application Android (Sprint 39) ---
    checks.push(if config.min_sdk > config.target_sdk {
        Check::new(
            Status::Fail,
            format!(
                "min SDK ({}) > target SDK ({})",
                config.min_sdk, config.target_sdk
            ),
        )
    } else if config.min_sdk < 24 {
        Check::new(
            Status::Warn,
            format!(
                "min SDK {} bas (≥ 24 recommandé pour Vulkan)",
                config.min_sdk
            ),
        )
    } else {
        Check::new(
            Status::Ok,
            format!("SDK {} → {}", config.min_sdk, config.target_sdk),
        )
    });

    checks.push(if config.icon_path.trim().is_empty() {
        Check::new(Status::Warn, "Aucune icône : icône par défaut utilisée")
    } else if std::path::Path::new(config.icon_path.trim()).is_file() {
        Check::new(Status::Ok, "Icône fournie")
    } else {
        Check::new(Status::Fail, "Icône introuvable sur le disque")
    });

    checks.push(Check::new(
        Status::Ok,
        format!(
            "Orientation : {} · {} FPS · MSAA ×{}{}",
            config.orientation.label(),
            config.target_fps,
            config.msaa,
            if config.shadows { " · ombres" } else { "" }
        ),
    ));

    checks
}

/// Dimensions d'une texture, en résolvant les schémas `asset://` / `bundle://`
/// (lecture mémoire) ou un chemin disque (lecture de l'en-tête seule). `None` si introuvable.
fn texture_dimensions(path: &str) -> Option<(u32, u32)> {
    if crate::assets::is_known_scheme(path) {
        let bytes = crate::assets::read_bytes(path)?;
        return image::load_from_memory(&bytes)
            .ok()
            .map(|img| (img.width(), img.height()));
    }
    image::image_dimensions(path).ok()
}

/// Compte des vérifications par statut : (ok, warn, fail).
pub fn summary(checks: &[Check]) -> (usize, usize, usize) {
    let mut counts = (0, 0, 0);
    for c in checks {
        match c.status {
            Status::Ok => counts.0 += 1,
            Status::Warn => counts.1 += 1,
            Status::Fail => counts.2 += 1,
        }
    }
    counts
}
