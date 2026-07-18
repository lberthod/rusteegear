//! Sélection de LOD géométrique pour le feuillage dense (Phase D, `sprintD_optimisation10h.md`
//! / `sprintoptimation3daudit10h.md`). Fonction pure, indépendante du pipeline de rendu —
//! câblée dans `Renderer::render` (`src/gfx/renderer.rs`) via `InstanceDraw::mesh`.

use crate::scene::{MeshKind, Scene};

/// Sous-chaînes de nom de fichier identifiant le feuillage le plus instancié
/// (`nature_grass_tuft.glb` ×112, `nature_fern.glb` ×69, `nature_reeds.glb` ×19 — mesure
/// `optimisation3D.Analys.md`), candidat à un impostor à distance plutôt qu'à son mesh
/// glTF complet.
pub(super) const FOLIAGE_LOD_KEYWORDS: &[&str] = &["grass_tuft", "fern", "reeds"];

/// Distance caméra au-delà de laquelle le feuillage dense (`FOLIAGE_LOD_KEYWORDS`) est
/// dessiné avec `MeshKind::Billboard` (impostor croix vertical bon marché, déjà présent dans
/// le cache de meshes GPU des primitives) plutôt que son mesh glTF complet.
pub(super) const FOLIAGE_LOD_DISTANCE: f32 = 40.0;

/// Résout le mesh à dessiner pour `mesh` compte tenu de la distance caméra — substitution
/// en `MeshKind::Billboard` pour le feuillage dense au-delà de `FOLIAGE_LOD_DISTANCE`,
/// inchangé sinon (primitives, imports hors liste, distance proche). Pure fonction de
/// résolution : ne modifie ni la scène ni le plan de dessin.
pub(super) fn foliage_lod_mesh(scene: &Scene, mesh: MeshKind, camera_distance: f32) -> MeshKind {
    if camera_distance <= FOLIAGE_LOD_DISTANCE {
        return mesh;
    }
    let MeshKind::Imported(i) = mesh else {
        return mesh;
    };
    let is_dense_foliage = scene
        .imported
        .get(i as usize)
        .map(|m| m.path.to_ascii_lowercase())
        .is_some_and(|path| {
            // `_sway` = variante animée (ex. `nature_reeds_sway.glb`, distincte de
            // `nature_reeds.glb`) — probablement skinnée, jamais substituable par un
            // impostor statique sans perdre son animation. Exclue même si un mot-clé
            // matche, plutôt que de se fier à `is_skinned` en aval (cet appelant n'a
            // pas cette information et cette fonction doit rester sûre isolément).
            !path.contains("_sway") && FOLIAGE_LOD_KEYWORDS.iter().any(|k| path.contains(k))
        });
    if is_dense_foliage {
        MeshKind::Billboard
    } else {
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ImportedMesh;

    fn scene_with_import(path: &str) -> Scene {
        let mut scene = Scene::default();
        scene.imported.push(ImportedMesh {
            path: path.to_string(),
            ..Default::default()
        });
        scene
    }

    fn is_billboard(mesh: MeshKind) -> bool {
        matches!(mesh, MeshKind::Billboard)
    }

    #[test]
    fn close_foliage_keeps_its_full_mesh() {
        let scene = scene_with_import("assets/nature_grass_tuft.glb");
        let mesh = foliage_lod_mesh(&scene, MeshKind::Imported(0), 5.0);
        assert!(matches!(mesh, MeshKind::Imported(0)));
    }

    #[test]
    fn far_dense_foliage_becomes_a_billboard_impostor() {
        let scene = scene_with_import("assets/nature_fern.glb");
        let mesh = foliage_lod_mesh(&scene, MeshKind::Imported(0), 100.0);
        assert!(is_billboard(mesh));
    }

    #[test]
    fn far_reeds_and_grass_tuft_also_become_a_billboard() {
        for path in ["assets/nature_reeds.glb", "assets/nature_grass_tuft.glb"] {
            let scene = scene_with_import(path);
            let mesh = foliage_lod_mesh(&scene, MeshKind::Imported(0), 41.0);
            assert!(
                is_billboard(mesh),
                "{path} devrait devenir un impostor croix"
            );
        }
    }

    #[test]
    fn far_non_foliage_import_is_unaffected() {
        let scene = scene_with_import("assets/building_wall.glb");
        let mesh = foliage_lod_mesh(&scene, MeshKind::Imported(0), 100.0);
        assert!(matches!(mesh, MeshKind::Imported(0)));
    }

    #[test]
    fn primitives_are_never_substituted() {
        let scene = Scene::default();
        let mesh = foliage_lod_mesh(&scene, MeshKind::Cube, 1000.0);
        assert!(matches!(mesh, MeshKind::Cube));
    }

    #[test]
    fn exactly_at_the_threshold_keeps_the_full_mesh() {
        let scene = scene_with_import("assets/nature_fern.glb");
        let mesh = foliage_lod_mesh(&scene, MeshKind::Imported(0), FOLIAGE_LOD_DISTANCE);
        assert!(matches!(mesh, MeshKind::Imported(0)));
    }

    #[test]
    fn animated_sway_variant_is_never_substituted_even_far_away() {
        // `nature_reeds_sway.glb` (rive du lac, `src/scene/demos.rs`) contient le
        // mot-clé "reeds" mais est une variante animée distincte de `nature_reeds.glb` —
        // la substituer par un impostor statique lui ferait perdre son animation.
        let scene = scene_with_import("assets/nature_reeds_sway.glb");
        let mesh = foliage_lod_mesh(&scene, MeshKind::Imported(0), 100.0);
        assert!(matches!(mesh, MeshKind::Imported(0)));
    }
}
