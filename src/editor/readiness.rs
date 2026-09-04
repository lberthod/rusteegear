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
    /// Indices (`Scene::objects`) des objets concernés, quand la vérification
    /// en désigne — la ligne devient cliquable vers l'objet fautif (roadmap
    /// post-audit UX v2 2026-09-04, 6.3). Vide pour les vérifications
    /// globales (éclairage, identité de build…).
    pub objects: Vec<usize>,
}

impl Check {
    fn new(status: Status, label: impl Into<String>) -> Self {
        Self {
            status,
            label: label.into(),
            objects: Vec::new(),
        }
    }

    /// Même chose, en désignant les objets concernés (cf. `objects`).
    fn with_objects(status: Status, label: impl Into<String>, objects: Vec<usize>) -> Self {
        Self {
            status,
            label: label.into(),
            objects,
        }
    }
}

/// Indices des objets dont `pred` est vraie — pour désigner les fautifs
/// d'une vérification (cf. `Check::objects`).
fn objects_where(scene: &Scene, pred: impl Fn(&crate::scene::SceneObject) -> bool) -> Vec<usize> {
    scene
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| pred(o))
        .map(|(i, _)| i)
        .collect()
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
    let without_collider = objects_where(scene, |o| o.physics == PhysicsKind::None);
    let no_collider = without_collider.len();
    if no_collider > 0 {
        checks.push(Check::with_objects(
            Status::Warn,
            format!("{no_collider} objet(s) sans collider (pas de physique)"),
            without_collider,
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
    let mut too_big_textures: Vec<&str> = Vec::new();
    let mut missing_textures: Vec<&str> = Vec::new();
    for tex in &textures {
        match texture_dimensions(tex) {
            Some((w, h)) => {
                if w > MAX_TEXTURE_PX || h > MAX_TEXTURE_PX {
                    too_big_textures.push(tex);
                }
            }
            None => missing_textures.push(tex),
        }
    }
    let (too_big, missing) = (too_big_textures.len(), missing_textures.len());
    // Objets qui utilisent l'une des textures fautives (roadmap 6.3).
    let users_of = |paths: &[&str]| objects_where(scene, |o| paths.contains(&o.texture.trim()));
    if missing > 0 {
        checks.push(Check::with_objects(
            Status::Fail,
            format!("{missing} texture(s) introuvable(s) sur le disque"),
            users_of(&missing_textures),
        ));
    }
    if too_big > 0 {
        checks.push(Check::with_objects(
            Status::Fail,
            format!("{too_big} texture(s) > {MAX_TEXTURE_PX} px (incompatibles mobile)"),
            users_of(&too_big_textures),
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
    let heavy_indices: Vec<u32> = scene
        .imported
        .iter()
        .enumerate()
        .filter(|(_, m)| m.data.indices.len() / 3 > MAX_TRIS_PER_MESH)
        .map(|(i, _)| i as u32)
        .collect();
    let heavy_meshes: Vec<&str> = heavy_indices
        .iter()
        .filter_map(|&i| scene.imported.get(i as usize))
        .map(|m| m.name.as_str())
        .collect();
    if !heavy_meshes.is_empty() {
        checks.push(Check::with_objects(
            Status::Warn,
            format!(
                "{} mesh(es) > {MAX_TRIS_PER_MESH} triangles : {}",
                heavy_meshes.len(),
                heavy_meshes.join(", ")
            ),
            objects_where(
                scene,
                |o| matches!(o.mesh, MeshKind::Imported(i) if heavy_indices.contains(&i)),
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
        let heavy_imports: Vec<u32> = scene
            .imported
            .iter()
            .enumerate()
            .filter(|(_, m)| oversized.contains(&m.path.trim()))
            .map(|(i, _)| i as u32)
            .collect();
        let users = objects_where(scene, |o| {
            oversized.contains(&o.texture.trim())
                || o.audio
                    .as_ref()
                    .is_some_and(|a| oversized.contains(&a.clip.trim()))
                || matches!(o.mesh, MeshKind::Imported(i) if heavy_imports.contains(&i))
        });
        checks.push(Check::with_objects(
            Status::Warn,
            format!(
                "{} asset(s) > {} Mio : {}",
                oversized.len(),
                MAX_ASSET_BYTES / (1024 * 1024),
                oversized.join(", ")
            ),
            users,
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

    fn check<'a>(checks: &'a [Check], contains: &str) -> &'a Check {
        checks
            .iter()
            .find(|c| c.label.contains(contains))
            .unwrap_or_else(|| panic!("aucun check ne contient « {contains} »"))
    }

    /// Une scène sans le moindre objet doit bloquer l'export (rien à afficher) —
    /// c'est le premier check, celui qu'un « Analyser » sur un projet vide doit
    /// toujours faire apparaître en rouge.
    #[test]
    fn empty_scene_fails() {
        let scene = Scene::default();
        let checks = analyze(&scene, &BuildConfig::default());
        assert_eq!(check(&checks, "Scène vide").status, Status::Fail);
    }

    /// Un seul objet par défaut (`MeshKind::Cube`, jamais `Plane`) : pas de sol,
    /// juste un avertissement (une scène flottante reste exportable, contrairement
    /// à une scène vide).
    #[test]
    fn scene_without_a_plane_warns_about_missing_ground() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject::default());
        let checks = analyze(&scene, &BuildConfig::default());
        assert_eq!(check(&checks, "Aucun sol").status, Status::Warn);
    }

    /// Une lumière entièrement nulle (ambiante, couleur et direction) ne doit
    /// jamais passer inaperçue : la scène exportée serait rendue noire au premier
    /// lancement, un des rares cas bloquants (`Fail`) plutôt qu'un simple `Warn`.
    #[test]
    fn scene_without_any_light_fails() {
        let mut scene = Scene::default();
        scene.light.ambient = 0.0;
        scene.light.color = [0.0, 0.0, 0.0];
        scene.light.dir = [0.0, 0.0, 0.0];
        let checks = analyze(&scene, &BuildConfig::default());
        assert_eq!(check(&checks, "Aucune lumière").status, Status::Fail);
    }

    /// Une scène entièrement statique (aucun objet avec un script) reste
    /// exportable — juste un avertissement, pas un blocage : certaines démos
    /// (galerie, showroom) n'ont légitimement aucune interactivité.
    #[test]
    fn scene_without_any_script_warns_about_static_scene() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject::default());
        let checks = analyze(&scene, &BuildConfig::default());
        assert_eq!(check(&checks, "Aucun script").status, Status::Warn);
    }

    /// Les objets sans collider (`PhysicsKind::None`, le défaut) sont comptés
    /// nommément, pas juste signalés en bloc — le nombre doit apparaître dans le
    /// message pour que le créateur sache l'ampleur du problème avant d'aller
    /// fouiller la scène objet par objet.
    #[test]
    fn objects_without_collider_are_counted_in_the_warning() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject::default());
        scene.objects.push(SceneObject::default());
        let checks = analyze(&scene, &BuildConfig::default());
        let c = check(&checks, "sans collider");
        assert_eq!(c.status, Status::Warn);
        assert!(c.label.contains('2'));
        // Les fautifs sont désignés (ligne cliquable, roadmap 6.3).
        assert_eq!(c.objects, vec![0, 1]);
    }

    /// Roadmap 6.3 : une texture introuvable désigne les objets qui l'utilisent,
    /// pas les autres ; les vérifications globales ne désignent personne.
    #[test]
    fn missing_texture_check_points_at_the_objects_using_it() {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject::default());
        scene.objects.push(SceneObject {
            texture: "/nulle/part/absente.png".into(),
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            texture: "/nulle/part/absente.png".into(),
            ..Default::default()
        });
        let checks = analyze(&scene, &BuildConfig::default());
        let c = check(&checks, "introuvable(s) sur le disque");
        assert_eq!(c.status, Status::Fail);
        assert_eq!(c.objects, vec![1, 2]);
        assert!(check(&checks, "Package ID").objects.is_empty());
    }

    /// Un `bundle_id` invalide (ici sans point, donc un seul segment) doit
    /// bloquer l'export Android — `cargo-apk`/le Play Store le refuseraient de
    /// toute façon, mieux vaut le dire avant le build qu'après.
    #[test]
    fn invalid_bundle_id_fails() {
        let config = BuildConfig {
            bundle_id: "pasunidentifiantvalide".into(),
            ..Default::default()
        };
        let checks = analyze(&Scene::controller_demo(), &config);
        assert_eq!(check(&checks, "Package ID invalide").status, Status::Fail);
    }

    /// Un nom d'application vide (ou uniquement des espaces) doit bloquer
    /// l'export : c'est le nom affiché sur l'appareil, un APK sans nom est
    /// inacceptable en l'état.
    #[test]
    fn empty_app_name_fails() {
        let config = BuildConfig {
            app_name: "   ".into(),
            ..Default::default()
        };
        let checks = analyze(&Scene::controller_demo(), &config);
        assert_eq!(
            check(&checks, "Nom de l'application manquant").status,
            Status::Fail
        );
    }

    /// Une version vide doit bloquer l'export — sans elle, `versionName` finirait
    /// vide dans le manifeste Android généré, invalide pour publier une mise à
    /// jour (le store ne saurait pas si c'est plus récent que la précédente).
    #[test]
    fn empty_version_fails() {
        let config = BuildConfig {
            version: String::new(),
            ..Default::default()
        };
        let checks = analyze(&Scene::controller_demo(), &config);
        assert_eq!(check(&checks, "Version manquante").status, Status::Fail);
    }

    /// `min_sdk` au-delà de `target_sdk` est une configuration Android
    /// intrinsèquement incohérente (un appareil ciblé ne pourrait jamais
    /// satisfaire le minimum imposé) : bloquant, pas un simple avertissement.
    #[test]
    fn min_sdk_above_target_sdk_fails() {
        let config = BuildConfig {
            min_sdk: 34,
            target_sdk: 33,
            ..Default::default()
        };
        let checks = analyze(&Scene::controller_demo(), &config);
        let c = check(&checks, "min SDK");
        assert_eq!(c.status, Status::Fail);
        assert!(c.label.contains("target SDK"));
    }

    /// `min_sdk` cohérent mais trop bas (< 24, le plancher recommandé pour
    /// Vulkan) : n'empêche pas l'export, juste un avertissement — l'app tournera,
    /// simplement pas au mieux sur les appareils les plus anciens couverts.
    #[test]
    fn min_sdk_below_24_warns() {
        let config = BuildConfig {
            min_sdk: 21,
            target_sdk: 33,
            ..Default::default()
        };
        let checks = analyze(&Scene::controller_demo(), &config);
        assert_eq!(check(&checks, "min SDK").status, Status::Warn);
    }

    /// Un chemin d'icône renseigné mais qui ne pointe vers aucun fichier réel
    /// (faute de frappe, fichier déplacé) doit bloquer l'export — distinct du cas
    /// « aucune icône fournie », qui n'est qu'un avertissement (icône par défaut).
    #[test]
    fn icon_path_pointing_to_a_missing_file_fails() {
        let config = BuildConfig {
            icon_path: "/chemin/qui/nexiste/vraiment/pas.png".into(),
            ..Default::default()
        };
        let checks = analyze(&Scene::controller_demo(), &config);
        assert_eq!(check(&checks, "Icône introuvable").status, Status::Fail);
    }

    /// `summary` doit répartir chaque check dans le bon compartiment (ok, warn,
    /// fail) sans en perdre ni en dupliquer — vérifié sur un jeu de statuts choisi
    /// à la main plutôt que sur une vraie scène, pour ne dépendre d'aucun détail
    /// de `analyze` ci-dessus.
    #[test]
    fn summary_counts_each_status_bucket() {
        let checks = vec![
            Check::new(Status::Ok, "a"),
            Check::new(Status::Ok, "b"),
            Check::new(Status::Warn, "c"),
            Check::new(Status::Fail, "d"),
            Check::new(Status::Fail, "e"),
            Check::new(Status::Fail, "f"),
        ];
        assert_eq!(summary(&checks), (2, 1, 3));
    }
}
