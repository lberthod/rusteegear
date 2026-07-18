//! Fonctions pures utilisées par les passes de rendu : culling frustum, tri des
//! instances, géométrie statique de la grille et hash d'entrée du plan de dessin.
//! Extrait de `renderer.rs` (Sprint 113a) — aucun changement de comportement, les
//! signatures/corps sont identiques à ceux d'origine.

use super::renderer::GizmoVertex;
use crate::app::AppState;
use crate::scene::{MeshKind, Scene};

/// `true` si `mesh` référence un import glTF skinné — c'est-à-dire dont
/// `ImportedMesh::skeleton` est renseigné. Toujours `false` pour les primitives, qui ne
/// sont jamais skinnées.
pub(super) fn is_skinned(scene: &Scene, mesh: MeshKind) -> bool {
    match mesh {
        MeshKind::Imported(i) => scene
            .imported
            .get(i as usize)
            .is_some_and(|m| m.skeleton.is_some()),
        _ => false,
    }
}

/// Les 6 plans du frustum (méthode de Gribb-Hartmann) extraits de la view-projection.
/// Chaque plan `(a,b,c,d)` : un point `p` est dans le frustum si `a·px+b·py+c·pz+d ≥ 0`.
pub(super) fn frustum_planes(vp: glam::Mat4) -> [glam::Vec4; 6] {
    let m = vp.to_cols_array_2d(); // m[col][row]
    let row = |r: usize| glam::Vec4::new(m[0][r], m[1][r], m[2][r], m[3][r]);
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    [
        r3 + r0, // gauche
        r3 - r0, // droite
        r3 + r1, // bas
        r3 - r1, // haut
        r3 + r2, // près
        r3 - r2, // loin
    ]
}

/// Rayon de culling par distance (mètres) selon la catégorie de mesh — `None` = pas de
/// limite (bâtiments/créatures/décor imposant, dont la disparition à distance serait trop
/// visible). Complète le frustum culling (`aabb_visible`) : réduit la charge en vue
/// large/plongée en coupant tôt le feuillage/petit décor dense, avant même le tri en
/// plages contiguës. Catégorisation par sous-chaîne du nom de fichier importé — grossière
/// mais suffisante pour les packs d'assets `nature_*.glb` de ce projet (Phase C,
/// `sprintoptimation3daudit10h.md`).
const FOLIAGE_LOW_RADIUS_KEYWORDS: &[&str] = &[
    "grass_tuft",
    "fern",
    "reeds",
    "flowers",
    "daisies",
    "thistle",
    "cattails",
    "lavender",
    "irises",
    "lily",
    "sunflowers",
    "wheat",
    "rice",
    "mushrooms",
    "clover",
    "bramble",
];
const MEDIUM_RADIUS_KEYWORDS: &[&str] = &[
    "tree",
    "pine",
    "oak",
    "birch",
    "cypress",
    "sequoia",
    "palm",
    "willow",
    "maple",
    "poplar",
    "cherry_blossom",
    "magnolia",
    "ginkgo",
    "olive",
    "plum",
    "hazel",
    "rock",
    "stump",
    "mossy_log",
    "cairn",
    "menhir",
    "bush",
    "holly",
    "topiary",
];
const FOLIAGE_LOW_RADIUS: f32 = 45.0;
const MEDIUM_RADIUS: f32 = 110.0;

/// `true` si `word` apparaît dans `haystack` sur une frontière de mot (délimitée par
/// début/fin de chaîne ou un caractère non alphanumérique — `_`/`.` dans un nom de
/// fichier). Une simple sous-chaîne ferait matcher le mot-clé `rock` dans
/// `nature_rocking_chair.glb` (meuble, pas un rocher) — bug constaté à l'audit du
/// Sprint 4, corrigé ici plutôt qu'en retirant le mot-clé `rock` (nécessaire pour
/// `nature_rock.glb`).
fn contains_word(haystack: &str, word: &str) -> bool {
    let is_boundary = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric());
    haystack.match_indices(word).any(|(idx, _)| {
        let before = haystack[..idx].chars().next_back();
        let after = haystack[idx + word.len()..].chars().next();
        is_boundary(before) && is_boundary(after)
    })
}

/// Rayon de culling par distance pour `mesh`, `None` si aucune limite ne s'applique
/// (catégorie « bâtiments/créatures » du plan Phase C, ou primitive codée).
pub(super) fn culling_radius_for(scene: &Scene, mesh: MeshKind) -> Option<f32> {
    let MeshKind::Imported(i) = mesh else {
        return None;
    };
    let path = scene.imported.get(i as usize)?.path.to_ascii_lowercase();
    if FOLIAGE_LOW_RADIUS_KEYWORDS
        .iter()
        .any(|k| contains_word(&path, k))
    {
        Some(FOLIAGE_LOW_RADIUS)
    } else if MEDIUM_RADIUS_KEYWORDS
        .iter()
        .any(|k| contains_word(&path, k))
    {
        Some(MEDIUM_RADIUS)
    } else {
        None
    }
}

/// `true` si la position `world_pos` est à moins de `radius` de `eye` — `radius = None`
/// signifie toujours visible (pas de limite de distance pour cette catégorie).
pub(super) fn distance_visible(
    eye: glam::Vec3,
    world_pos: glam::Vec3,
    radius: Option<f32>,
) -> bool {
    match radius {
        Some(r) => eye.distance_squared(world_pos) <= r * r,
        None => true,
    }
}

/// Teste si l'AABB locale `[lmin, lmax]` (transformée par `model`) est au moins
/// partiellement dans le frustum. Conservateur : peut garder un objet juste hors champ.
pub(super) fn aabb_visible(
    planes: &[glam::Vec4; 6],
    model: glam::Mat4,
    lmin: glam::Vec3,
    lmax: glam::Vec3,
) -> bool {
    // AABB monde à partir des 8 coins transformés.
    let mut wmin = glam::Vec3::splat(f32::INFINITY);
    let mut wmax = glam::Vec3::splat(f32::NEG_INFINITY);
    for sx in [lmin.x, lmax.x] {
        for sy in [lmin.y, lmax.y] {
            for sz in [lmin.z, lmax.z] {
                let p = (model * glam::Vec3::new(sx, sy, sz).extend(1.0)).truncate();
                wmin = wmin.min(p);
                wmax = wmax.max(p);
            }
        }
    }
    // Pour chaque plan, on teste le coin « positif » (le plus avancé vers le plan).
    for pl in planes {
        let n = pl.truncate();
        let positive = glam::Vec3::new(
            if n.x >= 0.0 { wmax.x } else { wmin.x },
            if n.y >= 0.0 { wmax.y } else { wmin.y },
            if n.z >= 0.0 { wmax.z } else { wmin.z },
        );
        if n.dot(positive) + pl.w < 0.0 {
            return false; // entièrement du mauvais côté d'un plan → hors champ
        }
    }
    true
}

/// Géométrie statique de la grille de référence (plan XZ, -10..10).
/// Axes X (rougeâtre) et Z (bleuté) accentués, lignes secondaires grises.
pub(super) fn build_grid_verts() -> Vec<GizmoVertex> {
    const N: i32 = 10;
    let mut v = Vec::new();
    for i in -N..=N {
        let f = i as f32;
        let cx = if i == 0 {
            [0.6, 0.3, 0.3]
        } else {
            [0.26, 0.26, 0.3]
        };
        let cz = if i == 0 {
            [0.3, 0.3, 0.6]
        } else {
            [0.26, 0.26, 0.3]
        };
        v.push(GizmoVertex {
            position: [f, 0.0, -N as f32],
            color: cx,
        });
        v.push(GizmoVertex {
            position: [f, 0.0, N as f32],
            color: cx,
        });
        v.push(GizmoVertex {
            position: [-N as f32, 0.0, f],
            color: cz,
        });
        v.push(GizmoVertex {
            position: [N as f32, 0.0, f],
            color: cz,
        });
    }
    v
}

/// Clé d'ordonnancement stable d'un type de mesh (pour grouper les instances).
pub(super) fn mesh_key(m: MeshKind) -> u32 {
    match m {
        MeshKind::Cube => 0,
        MeshKind::Sphere => 1,
        MeshKind::Plane => 2,
        MeshKind::Cylinder => 3,
        MeshKind::Capsule => 4,
        MeshKind::Terrain => 5,
        MeshKind::Billboard => 6,
        MeshKind::Imported(i) => 100 + i,
    }
}

/// Empreinte de **toutes** les entrées qui déterminent le buffer d'instances et le plan
/// de dessin : matrice caméra (frustum) + par objet (transform, couleur, matériau,
/// surbrillance, mesh, texture, visibilité). Sert au skip-rebuild : hash identique ⇒
/// sortie identique ⇒ rien à reconstruire. Capte tout changement → pas de frame périmée.
pub(super) fn render_input_hash(app: &AppState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in app.camera.view_proj().to_cols_array() {
        h.write_u32(v.to_bits());
    }
    h.write_usize(app.scene.objects.len());
    for (i, o) in app.scene.objects.iter().enumerate() {
        let t = &o.transform;
        let floats = [
            t.position.x,
            t.position.y,
            t.position.z,
            t.rotation.x,
            t.rotation.y,
            t.rotation.z,
            t.rotation.w,
            t.scale.x,
            t.scale.y,
            t.scale.z,
            o.color[0],
            o.color[1],
            o.color[2],
            o.metallic,
            o.roughness,
            o.emissive,
            app.highlight_of(i),
        ];
        for v in floats {
            h.write_u32(v.to_bits());
        }
        o.mesh.hash(&mut h);
        h.write(o.texture.as_bytes());
        h.write_u8(o.visible as u8);
    }
    h.finish()
}

#[cfg(test)]
mod culling_distance_tests {
    use super::*;
    use crate::scene::{ImportedMesh, Scene};

    fn scene_with_mesh(path: &str) -> Scene {
        let mut scene = Scene::default();
        scene.imported.push(ImportedMesh {
            path: path.to_string(),
            ..Default::default()
        });
        scene
    }

    /// Preuve Phase C (Sprint 4, `sprintoptimation3daudit10h.md`) : le feuillage bas
    /// (herbe/fougères) reçoit un rayon de culling court, distinct des arbres/rochers
    /// (rayon moyen) et des bâtiments/créatures (aucune limite) — évite qu'un réglage
    /// mal placé fasse disparaître un bâtiment à mi-distance sans que rien ne le signale.
    #[test]
    fn categorizes_foliage_trees_and_unbounded_correctly() {
        let grass = scene_with_mesh("assets/models/nature_grass_tuft.glb");
        let tree = scene_with_mesh("assets/models/nature_oak.glb");
        let building = scene_with_mesh("assets/models/nature_cabin.glb");
        let creature = scene_with_mesh("assets/models/creature.glb");

        let grass_r = culling_radius_for(&grass, MeshKind::Imported(0)).unwrap();
        let tree_r = culling_radius_for(&tree, MeshKind::Imported(0)).unwrap();
        assert!(
            grass_r < tree_r,
            "l'herbe doit avoir un rayon plus court que les arbres"
        );
        assert_eq!(culling_radius_for(&building, MeshKind::Imported(0)), None);
        assert_eq!(culling_radius_for(&creature, MeshKind::Imported(0)), None);
        // Une primitive codée (pas de mesh importé) n'a jamais de limite de distance.
        assert_eq!(culling_radius_for(&grass, MeshKind::Cube), None);
    }

    /// Régression : `nature_rocking_chair.glb` (meuble) ne doit pas être catégorisé comme
    /// « rocher » à cause d'une sous-chaîne `rock` non bornée — corrigé par `contains_word`.
    #[test]
    fn rocking_chair_is_not_matched_by_rock_keyword() {
        let chair = scene_with_mesh("assets/models/nature_rocking_chair.glb");
        assert_eq!(culling_radius_for(&chair, MeshKind::Imported(0)), None);
    }

    /// Preuve que `distance_visible` respecte le rayon fourni et que `None` ne coupe
    /// jamais rien, même à très grande distance (bâtiments/créatures).
    #[test]
    fn distance_visible_respects_radius_and_none_is_unbounded() {
        let eye = glam::Vec3::ZERO;
        let near = glam::Vec3::new(10.0, 0.0, 0.0);
        let far = glam::Vec3::new(1000.0, 0.0, 0.0);
        assert!(distance_visible(eye, near, Some(FOLIAGE_LOW_RADIUS)));
        assert!(!distance_visible(eye, far, Some(FOLIAGE_LOW_RADIUS)));
        assert!(distance_visible(eye, far, None));
    }
}
