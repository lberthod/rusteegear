//! Fonctions pures utilisées par les passes de rendu : culling frustum, tri des
//! instances, géométrie statique de la grille et hash d'entrée du plan de dessin.
//! Extrait de `renderer.rs` (Sprint 113a) — aucun changement de comportement, les
//! signatures/corps sont identiques à ceux d'origine.

use super::renderer::{CASCADE_COUNT, GizmoVertex};
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

/// Distances caméra de fin de chaque cascade d'ombre (m), pour un frustum
/// `[near, far]` : schéma « practical split » (mélange logarithmique/linéaire,
/// `lambda`) — la cascade proche est serrée (ombres nettes autour du joueur),
/// la lointaine couvre le reste. Avec near = 0,1, far = 100 et lambda = 0,75 :
/// ≈ 9 m, 24 m, 100 m.
pub(super) fn cascade_split_distances(near: f32, far: f32) -> [f32; CASCADE_COUNT] {
    const LAMBDA: f32 = 0.75;
    let n = CASCADE_COUNT as f32;
    std::array::from_fn(|i| {
        let p = (i as f32 + 1.0) / n;
        let log = near * (far / near).powf(p);
        let lin = near + (far - near) * p;
        LAMBDA * log + (1.0 - LAMBDA) * lin
    })
}

/// Les 8 coins monde de la tranche `[d0, d1]` du frustum de `cam`.
fn frustum_slice_corners(
    cam: &crate::gfx::camera::OrbitCamera,
    d0: f32,
    d1: f32,
) -> [glam::Vec3; 8] {
    let eye = cam.eye();
    let forward = (cam.target - eye).normalize_or(glam::Vec3::NEG_Z);
    let right = forward.cross(glam::Vec3::Y).normalize_or(glam::Vec3::X);
    let up = right.cross(forward);
    let tan_half = (cam.fovy * 0.5).tan();
    let mut out = [glam::Vec3::ZERO; 8];
    for (k, d) in [d0, d1].into_iter().enumerate() {
        let hh = d * tan_half;
        let hw = hh * cam.aspect;
        let c = eye + forward * d;
        out[k * 4] = c + right * hw + up * hh;
        out[k * 4 + 1] = c - right * hw + up * hh;
        out[k * 4 + 2] = c + right * hw - up * hh;
        out[k * 4 + 3] = c - right * hw - up * hh;
    }
    out
}

/// Ombres en cascade (analyse comparative 2026-09-04, emprunt Godot/Bevy) : une
/// caméra orthographique de lumière par tranche du frustum, ajustée sur la
/// **sphère englobante** de la tranche (stable en rotation de caméra, contrairement
/// à une boîte serrée qui « respire ») et **calée au texel** (sans ça, l'ombre
/// scintille à chaque déplacement de caméra). `light_dir` pointe **vers** la
/// lumière (convention de `Scene::light`). Renvoie les view-projections et les
/// distances de fin de cascade lues par `main.wgsl` pour choisir la couche.
pub(super) fn compute_cascades(
    cam: &crate::gfx::camera::OrbitCamera,
    light_dir: glam::Vec3,
    shadow_size: u32,
) -> ([glam::Mat4; CASCADE_COUNT], [f32; CASCADE_COUNT]) {
    // Mêmes plans que `OrbitCamera::view_proj`.
    const NEAR: f32 = 0.1;
    const FAR: f32 = 100.0;
    // Marge (m) derrière la tranche, côté lumière : un mur haut hors tranche doit
    // encore projeter son ombre dedans.
    const CASTER_MARGIN: f32 = 40.0;
    let splits = cascade_split_distances(NEAR, FAR);
    let dir = light_dir.normalize_or(glam::Vec3::Y);
    let up = if dir.x.abs() < 1e-3 && dir.z.abs() < 1e-3 {
        glam::Vec3::Z
    } else {
        glam::Vec3::Y
    };
    let mut vps = [glam::Mat4::IDENTITY; CASCADE_COUNT];
    let mut d0 = NEAR;
    for (i, &d1) in splits.iter().enumerate() {
        let corners = frustum_slice_corners(cam, d0, d1);
        let center = corners.iter().copied().sum::<glam::Vec3>() / 8.0;
        let radius = corners
            .iter()
            .map(|c| c.distance(center))
            .fold(0.0f32, f32::max)
            .max(0.5);
        // Rayon arrondi au décimètre : la sphère ne change pas de taille à chaque
        // micro-mouvement de caméra (le calage au texel ci-dessous suppose une
        // taille de texel stable).
        let radius = (radius * 10.0).ceil() / 10.0;
        let view = glam::camera::rh::view::look_at_mat4(
            center + dir * (radius + CASTER_MARGIN),
            center,
            up,
        );
        let proj = glam::camera::rh::proj::directx::orthographic(
            -radius,
            radius,
            -radius,
            radius,
            0.1,
            2.0 * radius + CASTER_MARGIN,
        );
        let mut vp = proj * view;
        // Calage au texel : translate la projection pour que l'origine monde tombe
        // sur un coin de texel — le contenu de la carte ne glisse alors que par
        // texels entiers quand la caméra bouge.
        let half = shadow_size as f32 * 0.5;
        let o = vp * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        let (tx, ty) = (o.x / o.w * half, o.y / o.w * half);
        let (dx, dy) = ((tx.round() - tx) / half, (ty.round() - ty) / half);
        vp = glam::Mat4::from_translation(glam::Vec3::new(dx, dy, 0.0)) * vp;
        vps[i] = vp;
        d0 = d1;
    }
    (vps, splits)
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
            o.opacity,
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

#[cfg(test)]
mod cascade_tests {
    use super::*;
    use crate::gfx::camera::OrbitCamera;

    #[test]
    fn split_distances_are_increasing_and_end_at_far() {
        let s = cascade_split_distances(0.1, 100.0);
        assert!(s[0] < s[1] && s[1] < s[2]);
        assert!((s[2] - 100.0).abs() < 1e-3);
        // La première cascade reste serrée (ombres nettes autour du joueur).
        assert!(s[0] < 15.0, "{s:?}");
    }

    #[test]
    fn every_cascade_contains_its_frustum_slice() {
        let mut cam = OrbitCamera::new(16.0 / 9.0);
        cam.target = glam::Vec3::new(4.0, 1.0, -3.0);
        cam.yaw = 1.2;
        cam.pitch = 0.4;
        let (vps, splits) = compute_cascades(&cam, glam::Vec3::new(0.5, 1.0, 0.3), 2048);
        let mut d0 = 0.1;
        for (i, &d1) in splits.iter().enumerate() {
            for c in frustum_slice_corners(&cam, d0, d1) {
                let p = vps[i] * c.extend(1.0);
                let ndc = p.truncate() / p.w;
                assert!(
                    ndc.x.abs() <= 1.0 + 1e-3 && ndc.y.abs() <= 1.0 + 1e-3,
                    "cascade {i} : coin {c} hors carte ({ndc})"
                );
                assert!(
                    (-1e-3..=1.0 + 1e-3).contains(&ndc.z),
                    "cascade {i} : z {ndc}"
                );
            }
            d0 = d1;
        }
    }

    #[test]
    fn cascades_are_texel_snapped_so_a_small_camera_move_shifts_by_whole_texels() {
        let mut cam = OrbitCamera::new(1.5);
        let dir = glam::Vec3::new(0.3, 1.0, 0.2);
        let (a, _) = compute_cascades(&cam, dir, 1024);
        cam.target += glam::Vec3::new(0.0123, 0.0, 0.0071);
        let (b, _) = compute_cascades(&cam, dir, 1024);
        // L'origine monde reste sur un coin de texel dans les deux cas.
        for vp in [a[0], b[0]] {
            let o = vp * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
            let tx = o.x / o.w * 512.0;
            assert!((tx - tx.round()).abs() < 1e-3, "{tx}");
        }
    }
}
