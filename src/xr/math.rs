//! Conversions OpenXR → matrices de rendu wgpu.
//!
//! Fonctions pures (glam seul) : compilées et testées sur toutes les cibles, même
//! sans la feature `vr` — c'est ici que se joue la justesse stéréoscopique, et le
//! casque n'est pas là pour la vérifier à chaque `cargo test`.
//!
//! Conventions : OpenXR est main droite, Y en haut, regard vers −Z (comme
//! `look_at_mat4` du moteur). wgpu attend une profondeur NDC dans \[0, 1\].

use glam::{Mat4, Quat, Vec3, Vec4};

/// Champ de vision d'un œil tel que le rend `xrLocateViews` (`XrFovf`) : quatre
/// angles en radians, **asymétriques** (l'œil n'est pas au centre de sa lentille),
/// `left`/`down` négatifs en pratique.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fov {
    pub left: f32,
    pub right: f32,
    pub up: f32,
    pub down: f32,
}

/// Projection perspective asymétrique à partir d'un [`Fov`] OpenXR, profondeur wgpu
/// \[0, 1\] (`near` → 0, `far` → 1). `perspective_rh` de glam ne sait faire que des
/// frustums symétriques, inutilisables ici : l'image paraîtrait décalée et la
/// fusion stéréo échouerait (double vision).
pub fn projection_from_fov(fov: Fov, near: f32, far: f32) -> Mat4 {
    let l = fov.left.tan();
    let r = fov.right.tan();
    let u = fov.up.tan();
    let d = fov.down.tan();
    let w = r - l;
    let h = u - d;
    Mat4::from_cols(
        Vec4::new(2.0 / w, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 2.0 / h, 0.0, 0.0),
        Vec4::new((r + l) / w, (u + d) / h, far / (near - far), -1.0),
        Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
    )
}

/// Matrice de vue d'un œil : inverse de sa pose (orientation + position dans
/// l'espace de référence XR).
pub fn view_from_pose(orientation: Quat, position: Vec3) -> Mat4 {
    Mat4::from_rotation_translation(orientation.normalize(), position).inverse()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEAR: f32 = 0.05;
    const FAR: f32 = 100.0;

    fn project(m: Mat4, p: Vec3) -> Vec3 {
        let c = m * p.extend(1.0);
        c.truncate() / c.w
    }

    /// Champ de vision typique d'un œil gauche de Quest 3 (valeurs de
    /// `xrLocateViews`, arrondies) : plus large côté extérieur (gauche).
    fn quest3_left_eye() -> Fov {
        Fov {
            left: -0.942,
            right: 0.698,
            up: 0.768,
            down: -0.960,
        }
    }

    #[test]
    fn near_and_far_planes_map_to_wgpu_depth_range() {
        let m = projection_from_fov(quest3_left_eye(), NEAR, FAR);
        assert!(project(m, Vec3::new(0.0, 0.0, -NEAR)).z.abs() < 1e-5);
        assert!((project(m, Vec3::new(0.0, 0.0, -FAR)).z - 1.0).abs() < 1e-4);
    }

    #[test]
    fn frustum_edges_land_on_ndc_borders_even_when_asymmetric() {
        let fov = quest3_left_eye();
        let m = projection_from_fov(fov, NEAR, FAR);
        let z = -3.0;
        // Un point sur le bord gauche du frustum (à distance 3 m) → x_ndc = −1, etc.
        let left = project(m, Vec3::new(fov.left.tan() * 3.0, 0.0, z));
        let right = project(m, Vec3::new(fov.right.tan() * 3.0, 0.0, z));
        let up = project(m, Vec3::new(0.0, fov.up.tan() * 3.0, z));
        let down = project(m, Vec3::new(0.0, fov.down.tan() * 3.0, z));
        assert!((left.x + 1.0).abs() < 1e-5, "{left:?}");
        assert!((right.x - 1.0).abs() < 1e-5, "{right:?}");
        assert!((up.y - 1.0).abs() < 1e-5, "{up:?}");
        assert!((down.y + 1.0).abs() < 1e-5, "{down:?}");
    }

    #[test]
    fn symmetric_fov_matches_glam_perspective() {
        let half = 0.6_f32;
        let fov = Fov {
            left: -half,
            right: half,
            up: half,
            down: -half,
        };
        let ours = projection_from_fov(fov, NEAR, FAR);
        let glam = Mat4::perspective_rh(2.0 * half, 1.0, NEAR, FAR);
        assert!(ours.abs_diff_eq(glam, 1e-5), "{ours:?}\n{glam:?}");
    }

    #[test]
    fn view_matrix_brings_the_eye_to_the_origin() {
        let pos = Vec3::new(0.03, 1.6, 0.2);
        let rot = Quat::from_rotation_y(0.4);
        let v = view_from_pose(rot, pos);
        assert!(v.transform_point3(pos).length() < 1e-5);
        // Un point droit devant l'œil (−Z local) reste sur l'axe −Z en espace vue.
        let ahead = pos + rot * Vec3::new(0.0, 0.0, -2.0);
        assert!((v.transform_point3(ahead) - Vec3::new(0.0, 0.0, -2.0)).length() < 1e-5);
    }
}
