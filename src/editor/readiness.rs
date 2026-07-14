//! Contrôle qualité APK / « APK Readiness Check ».
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
/// Budget triangles par mesh importé (Sprint 126) — au-delà, un modèle mobile
/// courant commence à peser sur le temps de trame, même seul (avant tout
/// instancing/LOD). Alerte, pas un blocage : certains modèles hero justifient
/// de dépasser, mais pas en silence.
const MAX_TRIS_PER_MESH: usize = 65_000;
/// Poids sur disque au-delà duquel un asset individuel pèse sur la taille finale
/// de l'APK (Sprint 126) — 8 Mio pour une texture/un son/un mesh unique est déjà
/// beaucoup vu le nombre d'assets qu'une scène modeste accumule.
const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024;

/// Verdict d'une vérification individuelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    // --- Références d'assets stables (Sprint 126) : une référence `asset-id://`
    // (texture, audio, mesh importé, image de widget HUD) dont l'uuid ne résout
    // plus (asset renommé hors de ce mécanisme, ou supprimé) casse silencieusement
    // à l'export sinon — `Scene::asset_references` donne la description lisible de
    // chaque endroit concerné, pas juste « un asset manque » comme le check
    // textures ci-dessus (qui ne couvre que les textures, pas les 3 autres champs).
    let mut broken_refs: Vec<String> = Vec::new();
    for (uuid, used_by) in scene.asset_references() {
        let id = format!("{}{uuid}", crate::assets::ASSET_ID_SCHEME);
        if crate::assets::resolve_asset_id(&id).is_none() {
            broken_refs.extend(used_by);
        }
    }
    if !broken_refs.is_empty() {
        broken_refs.sort();
        checks.push(Check::new(
            Status::Fail,
            format!(
                "{} référence(s) d'asset cassée(s) (renommé/supprimé) : {}",
                broken_refs.len(),
                broken_refs.join(", ")
            ),
        ));
    }

    // --- Budget polycount (Sprint 126) ---
    let heavy_meshes: Vec<&str> = scene
        .imported
        .iter()
        .filter(|m| m.data.indices.len() / 3 > MAX_TRIS_PER_MESH)
        .map(|m| m.name.as_str())
        .collect();
    if !heavy_meshes.is_empty() {
        checks.push(Check::new(
            Status::Warn,
            format!(
                "{} mesh(es) > {MAX_TRIS_PER_MESH} triangles : {}",
                heavy_meshes.len(),
                heavy_meshes.join(", ")
            ),
        ));
    } else if !scene.imported.is_empty() {
        checks.push(Check::new(
            Status::Ok,
            format!(
                "{} mesh(es) importé(s) sous le budget triangles",
                scene.imported.len()
            ),
        ));
    }

    // --- Budget taille sur disque (Sprint 126) : un même chemin peut apparaître
    // plusieurs fois (plusieurs objets partageant une texture) — dédupliqué avant
    // de lire les octets, la taille sur disque ne dépend pas du nombre de
    // référencements.
    let mut asset_paths: BTreeSet<&str> = textures.clone();
    for m in &scene.imported {
        if !m.path.trim().is_empty() {
            asset_paths.insert(m.path.trim());
        }
    }
    for o in &scene.objects {
        if let Some(a) = &o.audio
            && !a.clip.trim().is_empty()
        {
            asset_paths.insert(a.clip.trim());
        }
    }
    let oversized: Vec<&str> = asset_paths
        .iter()
        .filter(|p| asset_byte_len(p).is_some_and(|len| len > MAX_ASSET_BYTES))
        .copied()
        .collect();
    if !oversized.is_empty() {
        checks.push(Check::new(
            Status::Warn,
            format!(
                "{} asset(s) > {} Mio : {}",
                oversized.len(),
                MAX_ASSET_BYTES / (1024 * 1024),
                oversized.join(", ")
            ),
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

    // --- Application Android ---
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

/// Taille sur disque d'un asset (Sprint 126, budget taille), en résolvant les mêmes
/// schémas que `texture_dimensions` — mais lit le fichier entier (pas seulement
/// l'en-tête) puisque rien ne donne la taille sans l'ouvrir. `None` si introuvable
/// (déjà signalé par le check « références cassées » ci-dessus le cas échéant, pas
/// la peine de dupliquer l'alerte ici).
fn asset_byte_len(path: &str) -> Option<u64> {
    if crate::assets::is_known_scheme(path) {
        return crate::assets::read_bytes(path).map(|b| b.len() as u64);
    }
    std::fs::metadata(path).ok().map(|m| m.len())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImportedMesh, SceneObject};

    /// Sprint 126 : une texture `asset-id://` dont l'uuid n'est enregistré dans
    /// aucun manifeste (jamais importée dans ce test, donc forcément introuvable)
    /// doit produire un `Fail` nommant l'objet concerné, pas juste disparaître
    /// silencieusement des vérifications.
    #[test]
    fn broken_asset_id_reference_is_reported_by_name() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Statue".into(),
            texture: "asset-id://uuid-jamais-enregistre".into(),
            ..Default::default()
        });
        let checks = analyze(&scene, &BuildConfig::default());
        let broken = checks
            .iter()
            .find(|c| c.label.contains("cassée"))
            .expect("une référence cassée doit produire un check dédié");
        assert_eq!(broken.status, Status::Fail);
        assert!(broken.label.contains("Statue"));
    }

    /// Sprint 126 : un mesh importé au-delà de `MAX_TRIS_PER_MESH` doit être
    /// signalé nommément (`Warn`, pas bloquant — un modèle hero peut le justifier).
    #[test]
    fn oversized_mesh_triangle_count_is_flagged() {
        let mut scene = Scene::default();
        scene.imported.push(ImportedMesh {
            name: "Cathédrale".into(),
            path: "asset://cathedrale.glb".into(),
            data: crate::gfx::mesh::MeshData {
                vertices: Vec::new(),
                indices: vec![0u32; (MAX_TRIS_PER_MESH + 1) * 3],
            },
            ..Default::default()
        });
        let checks = analyze(&scene, &BuildConfig::default());
        let heavy = checks
            .iter()
            .find(|c| c.label.contains("triangles"))
            .expect("un mesh au-dessus du budget doit produire un check dédié");
        assert!(matches!(heavy.status, Status::Warn));
        assert!(heavy.label.contains("Cathédrale"));
    }

    /// Une scène sans aucune référence `asset-id://` ni mesh en dépassement ne doit
    /// produire ni le check « références cassées » ni le check « polycount » — pas
    /// de faux positif sur une scène qui n'utilise simplement pas ces mécanismes.
    #[test]
    fn scene_without_asset_id_or_heavy_meshes_has_no_spurious_warnings() {
        let scene = Scene::controller_demo();
        let checks = analyze(&scene, &BuildConfig::default());
        assert!(!checks.iter().any(|c| c.label.contains("cassée")));
        assert!(
            !checks.iter().any(|c| c.label.contains("triangles")
                && matches!(c.status, Status::Warn | Status::Fail))
        );
    }
}
