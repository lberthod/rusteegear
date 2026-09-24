//! « Rig » VR : place la pièce réelle du joueur (espace `STAGE` d'OpenXR,
//! origine au sol) dans le monde du jeu — une origine et un lacet. Les poses
//! d'yeux fournies par le casque (ou le simulateur) sont exprimées dans la
//! pièce ; le rig les convertit en coordonnées monde pour le `Renderer`.
//!
//! Phase 1 : rig fixe, posé derrière le personnage au chargement (vue
//! « spectateur » à l'échelle réelle). La phase 4 le fera suivre le joueur
//! (locomotion) et ajoutera la vue première personne.

use glam::{Quat, Vec3};

use super::math::{EyeView, Fov};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rig {
    /// Point du monde où se trouve l'origine de la pièce (au sol).
    pub origin: Vec3,
    /// Rotation de la pièce autour de +Y (rad) : le « devant » de la pièce
    /// (−Z) pointe vers `Quat::from_rotation_y(yaw) * −Z` dans le monde.
    pub yaw: f32,
}

impl Default for Rig {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            yaw: 0.0,
        }
    }
}

impl Rig {
    /// Lacet qui oriente le devant de la pièce (−Z) selon `forward` (projeté à
    /// l'horizontale). `forward` nul → 0.
    pub fn yaw_facing(forward: Vec3) -> f32 {
        let f = Vec3::new(forward.x, 0.0, forward.z);
        if f.length_squared() < 1e-8 {
            return 0.0;
        }
        (-f.x).atan2(-f.z)
    }

    /// Rig posé `distance` mètres derrière `target` (même direction que
    /// `forward`), sol à `ground_y` — le joueur voit `target` droit devant lui.
    pub fn behind(target: Vec3, forward: Vec3, distance: f32, ground_y: f32) -> Self {
        let yaw = Self::yaw_facing(forward);
        let fwd = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
        let p = target - fwd * distance;
        Self {
            origin: Vec3::new(p.x, ground_y, p.z),
            yaw,
        }
    }

    /// Rig de départ de la phase 1 dans une partie en cours (après au moins un
    /// `advance_play`, qui construit le monde physique) : 2,5 m derrière le
    /// joueur, face à la direction de la caméra de jeu, sol de la pièce posé sur
    /// le terrain sous ce point (rayon physique), à défaut à la hauteur du
    /// joueur.
    pub fn spectator(app: &crate::app::AppState) -> Self {
        let target = app.player_position().unwrap_or(app.camera.target);
        let forward = app.camera.target - app.camera.eye();
        let rig = Self::behind(target, forward, 2.5, target.y);
        let ground = app
            .ground_height_at(rig.origin.x, rig.origin.z)
            .unwrap_or(target.y);
        Self {
            origin: Vec3::new(rig.origin.x, ground, rig.origin.z),
            ..rig
        }
    }

    /// Rig « à la place de la caméra de jeu » : la tête (debout, `eye_height`
    /// au-dessus du sol de la pièce) se trouve à l'œil de la caméra et regarde
    /// sa cible — pour les scènes sans personnage à incarner, cadrées comme un
    /// écran (rééducation Mouvéo : on se tient face au miroir, en relief).
    pub fn from_camera(app: &crate::app::AppState, eye_height: f32) -> Self {
        let eye = app.camera.eye();
        Self {
            origin: Vec3::new(eye.x, eye.y - eye_height, eye.z),
            yaw: Self::yaw_facing(app.camera.target - eye),
        }
    }

    /// Pose d'un œil de la pièce vers le monde.
    pub fn to_world(&self, eye: EyeView) -> EyeView {
        let r = Quat::from_rotation_y(self.yaw);
        EyeView {
            orientation: r * eye.orientation,
            position: self.origin + r * eye.position,
            fov: eye.fov,
        }
    }
}

/// Caméra « centrale » englobant les deux yeux, pour tout ce que le
/// `Renderer` calcule une seule fois par image (culling, cascades d'ombre,
/// tri des translucides, lumières) : placée légèrement en retrait du milieu des
/// yeux, regard moyen, demi-angle couvrant les coins des deux frustums **quel
/// que soit le roulis de la tête** (d'où la diagonale). Renvoie
/// (position, direction de regard, FOV vertical, rapport largeur/hauteur = 1).
pub fn cull_camera(eyes: &[EyeView; 2]) -> (Vec3, Vec3, f32) {
    let center = (eyes[0].position + eyes[1].position) * 0.5;
    let forward = (eyes[0].orientation * Vec3::NEG_Z + eyes[1].orientation * Vec3::NEG_Z)
        .normalize_or(Vec3::NEG_Z);
    let max_tan = |f: &Fov| {
        let h = f.left.tan().abs().max(f.right.tan().abs());
        let v = f.up.tan().abs().max(f.down.tan().abs());
        h.hypot(v)
    };
    let half_tan = max_tan(&eyes[0].fov).max(max_tan(&eyes[1].fov));
    // Recul : l'œil le plus excentré (±IPD/2) reste dans le cône central.
    let ipd = eyes[0].position.distance(eyes[1].position);
    let back = ipd / half_tan.max(1e-3);
    let fovy = 2.0 * half_tan.atan();
    (
        center - forward * back,
        forward,
        fovy.min(179f32.to_radians()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xr::sim::{QuestProfile, SimHead};

    #[test]
    fn behind_places_the_target_straight_ahead() {
        let target = Vec3::new(10.0, 3.0, -4.0);
        let forward = Vec3::new(1.0, 0.2, 1.0);
        let rig = Rig::behind(target, forward, 2.5, 1.0);
        assert!((rig.origin.y - 1.0).abs() < 1e-6);
        let flat = Vec3::new(target.x, 1.0, target.z);
        assert!((rig.origin.distance(flat) - 2.5).abs() < 1e-4);
        // Tête par défaut (regard −Z de la pièce) : la cible est droit devant.
        let eye = rig.to_world(SimHead::default().eye_views(&QuestProfile::QUEST3)[0]);
        let dir = (flat - rig.origin).normalize();
        let look = eye.orientation * Vec3::NEG_Z;
        assert!(look.dot(dir) > 0.999, "{look:?} vs {dir:?}");
    }

    #[test]
    fn to_world_keeps_eye_spacing() {
        let rig = Rig {
            origin: Vec3::new(5.0, 2.0, 1.0),
            yaw: 1.2,
        };
        let [l, r] = SimHead::default().eye_views(&QuestProfile::QUEST3);
        let (wl, wr) = (rig.to_world(l), rig.to_world(r));
        assert!((wl.position.distance(wr.position) - 0.063).abs() < 1e-5);
        assert!((wl.position.y - (2.0 + crate::xr::sim::STANDING_EYE_HEIGHT)).abs() < 1e-5);
    }

    #[test]
    fn cull_camera_contains_every_eye_frustum_corner_even_rolled() {
        let mut head = SimHead::default();
        head.look(0.4, -0.3);
        let mut eyes = head.eye_views(&QuestProfile::QUEST3);
        // Roulis de 30° (tête penchée) : absent de SimHead, ajouté à la main.
        let roll = Quat::from_rotation_z(0.52);
        for e in &mut eyes {
            e.orientation *= roll;
        }
        let (pos, fwd, fovy) = cull_camera(&eyes);
        let cos_half = (fovy * 0.5).cos();
        for e in &eyes {
            let f = e.fov;
            for (x, y) in [
                (f.left.tan(), f.up.tan()),
                (f.right.tan(), f.up.tan()),
                (f.left.tan(), f.down.tan()),
                (f.right.tan(), f.down.tan()),
            ] {
                // Point à 5 m sur l'arête du frustum de l'œil.
                let p = e.position + e.orientation * (Vec3::new(x, y, -1.0).normalize() * 5.0);
                let d = (p - pos).normalize();
                assert!(d.dot(fwd) >= cos_half - 1e-4, "coin hors du cône central");
            }
        }
    }
}
