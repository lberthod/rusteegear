//! Dessin de debug (Sprint 83) : segments visibles une frame, consommés par le
//! renderer puis vidés — `debug_line` est la primitive, `debug_box`/`debug_sphere`
//! la composent pour des formes usuelles (AABB, capteur sphérique). Extrait de
//! `app/mod.rs` (Sprint 103a) : aucune dépendance au reste de la boucle de jeu.

use glam::Vec3;

use super::AppState;

impl AppState {
    /// Dessine un segment de debug, visible pendant exactement une frame de rendu
    /// (Sprint 83). Ex. visualiser un raycast, une ligne de vue, une trajectoire.
    pub fn debug_line(&mut self, a: Vec3, b: Vec3, color: [f32; 3]) {
        self.debug_lines.push((a, b, color));
    }

    /// Dessine les 12 arêtes d'une boîte alignée aux axes, en fil de fer (Sprint 83).
    /// `half_extents` : demi-tailles sur chaque axe (toujours positives).
    pub fn debug_box(&mut self, center: Vec3, half_extents: Vec3, color: [f32; 3]) {
        let h = half_extents.abs();
        let corners: [Vec3; 8] = [
            Vec3::new(-h.x, -h.y, -h.z),
            Vec3::new(h.x, -h.y, -h.z),
            Vec3::new(h.x, -h.y, h.z),
            Vec3::new(-h.x, -h.y, h.z),
            Vec3::new(-h.x, h.y, -h.z),
            Vec3::new(h.x, h.y, -h.z),
            Vec3::new(h.x, h.y, h.z),
            Vec3::new(-h.x, h.y, h.z),
        ]
        .map(|o| center + o);
        // Face du bas, face du haut, puis les 4 montants verticaux.
        const EDGES: [(usize, usize); 12] = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        for (i, j) in EDGES {
            self.debug_line(corners[i], corners[j], color);
        }
    }

    /// Dessine une sphère en fil de fer (3 anneaux orthogonaux), à `segments` côtés chacun
    /// (Sprint 83). Même construction que les anneaux de rotation du gizmo (`RING_SEGMENTS`
    /// dans `gfx::renderer`), dupliquée ici volontairement : cette méthode vit côté
    /// gameplay (`AppState`), sans dépendance au module GPU.
    pub fn debug_sphere(&mut self, center: Vec3, radius: f32, color: [f32; 3]) {
        const SEGMENTS: usize = 16;
        let radius = radius.abs();
        // Un anneau par plan (XY, XZ, YZ) : couvre la sphère par 3 grands cercles.
        let planes: [(Vec3, Vec3); 3] =
            [(Vec3::X, Vec3::Y), (Vec3::X, Vec3::Z), (Vec3::Y, Vec3::Z)];
        for (u, v) in planes {
            for k in 0..SEGMENTS {
                let a0 = std::f32::consts::TAU * k as f32 / SEGMENTS as f32;
                let a1 = std::f32::consts::TAU * (k + 1) as f32 / SEGMENTS as f32;
                let p0 = center + (u * a0.cos() + v * a0.sin()) * radius;
                let p1 = center + (u * a1.cos() + v * a1.sin()) * radius;
                self.debug_line(p0, p1, color);
            }
        }
    }
}
