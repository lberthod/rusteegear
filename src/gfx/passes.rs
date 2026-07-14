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
