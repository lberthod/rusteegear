//! Dessin de debug : segments visibles une frame, consommés par le
//! renderer puis vidés — `debug_line` est la primitive, `debug_box`/`debug_sphere`
//! la composent pour des formes usuelles (AABB, capteur sphérique). Extrait de
//! `app/mod.rs` : aucune dépendance au reste de la boucle de jeu.

use glam::Vec3;

use super::AppState;

impl AppState {
    /// Dessine un segment de debug, visible pendant exactement une frame de rendu.
    /// Ex. visualiser un raycast, une ligne de vue, une trajectoire.
    pub fn debug_line(&mut self, a: Vec3, b: Vec3, color: [f32; 3]) {
        self.debug_lines.push((a, b, color));
    }

    /// Dessine les 12 arêtes d'une boîte alignée aux axes, en fil de fer.
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

    /// Dessine une sphère en fil de fer (3 anneaux orthogonaux), à `segments` côtés chacun.
    /// Même construction que les anneaux de rotation du gizmo (`RING_SEGMENTS`
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

#[cfg(test)]
mod tests {
    use super::super::DebugView;
    use super::*;

    #[test]
    fn debug_line_accumulates_and_is_owned_by_the_caller_to_clear() {
        // `AppState` ne se vide jamais elle-même : c'est `Renderer::render` qui lit et
        // vide `debug_lines` après dessin — vérifié ici côté accumulation
        // pure, sans dépendre du GPU.
        let mut app = AppState::new();
        assert!(app.debug_lines.is_empty());
        app.debug_line(Vec3::ZERO, Vec3::X, [1.0, 0.0, 0.0]);
        app.debug_line(Vec3::Y, Vec3::Z, [0.0, 1.0, 0.0]);
        assert_eq!(app.debug_lines.len(), 2);
        assert_eq!(app.debug_lines[0], (Vec3::ZERO, Vec3::X, [1.0, 0.0, 0.0]));
    }

    #[test]
    fn debug_box_draws_exactly_twelve_edges() {
        let mut app = AppState::new();
        app.debug_box(Vec3::ZERO, Vec3::splat(1.0), [1.0, 1.0, 1.0]);
        assert_eq!(app.debug_lines.len(), 12, "une boîte a 12 arêtes");
        // Chaque sommet du segment doit être à distance `sqrt(3)` du centre (un coin
        // d'un cube de demi-taille 1), à l'exception près qu'un segment relie deux coins
        // adjacents — on vérifie plutôt que toutes les coordonnées valent ±1.
        for (a, b, _) in &app.debug_lines {
            for p in [a, b] {
                assert!(p.x.abs() == 1.0 && p.y.abs() == 1.0 && p.z.abs() == 1.0);
            }
        }
    }

    #[test]
    fn debug_sphere_draws_three_rings_of_segments_all_on_the_radius() {
        let mut app = AppState::new();
        let center = Vec3::new(2.0, 0.0, 0.0);
        app.debug_sphere(center, 3.0, [0.2, 0.6, 1.0]);
        // 3 anneaux × 16 segments (SEGMENTS interne) = 48 segments.
        assert_eq!(app.debug_lines.len(), 48);
        for (a, b, _) in &app.debug_lines {
            assert!(((*a - center).length() - 3.0).abs() < 1e-4);
            assert!(((*b - center).length() - 3.0).abs() < 1e-4);
        }
    }

    #[test]
    fn debug_view_defaults_to_shaded_and_encodes_distinct_uniform_values() {
        // `AppState::new()` doit démarrer en rendu normal (pas en vue de debug par
        // surprise) ; les 3 vues doivent être distinguables côté shader (main.wgsl
        // branche sur `> 0.5` / `> 1.5`), donc strictement croissantes.
        assert_eq!(AppState::new().debug_view, DebugView::Shaded);
        let shaded = DebugView::Shaded.as_uniform();
        let normals = DebugView::Normals.as_uniform();
        let depth = DebugView::Depth.as_uniform();
        assert!(shaded < normals && normals < depth);
    }
}
