//! Caméra orbitale : produit la matrice view-projection (compatible NDC wgpu, z in \[0,1\]).

use glam::camera::rh::{proj::directx, view::look_at_mat4};
use glam::{Mat4, Vec3};

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
    pub fovy: f32,
    /// Distance effective imposée par la collision de caméra (`AppState::
    /// update_camera_collision`, décor solide entre `target` et l'œil) — `None`
    /// tant que la voie est libre, auquel cas `eye()` retombe sur `distance`. Ne
    /// mute jamais `distance` elle-même : le zoom désiré (molette, réglages) reste
    /// intact dès que l'obstacle disparaît, sans qu'il faille le mémoriser à part.
    pub collision_distance: Option<f32>,
}

impl OrbitCamera {
    pub fn new(aspect: f32) -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 6.0,
            yaw: 0.7,
            pitch: 0.5,
            aspect,
            fovy: 45f32.to_radians(),
            collision_distance: None,
        }
    }

    pub fn eye(&self) -> Vec3 {
        let pitch = self.pitch.clamp(-1.54, 1.54);
        let dist = self.collision_distance.unwrap_or(self.distance);
        let x = dist * pitch.cos() * self.yaw.sin();
        let y = dist * pitch.sin();
        let z = dist * pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Vue+projection pour le rendu, avec un décalage additif appliqué à `target`
    /// (recul caméra en pixels-monde) — n'affecte que la matrice produite ici,
    /// jamais `self.target` : la caméra de jeu (suivi joueur, IA, réseau) reste
    /// inchangée, seul le rendu de la frame courante tressaute (Sprint 1,
    /// `sprint10audit.md` — retour d'encaissement de coup).
    pub fn view_proj_shaken(&self, shake_offset: Vec3) -> Mat4 {
        let view = look_at_mat4(
            self.eye() + shake_offset,
            self.target + shake_offset,
            Vec3::Y,
        );
        let proj = directx::perspective(self.fovy, self.aspect, 0.1, 100.0);
        proj * view
    }

    /// Pan « outil Main » : glisse `target` dans le plan écran de la caméra.
    /// `dx`/`dy` en pixels ; le contenu suit le curseur (glisser à droite =
    /// la scène part à droite). Échelle proportionnelle à `distance` pour un
    /// déplacement perçu constant quel que soit le zoom.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        let s = self.distance * 0.0015;
        self.target += (up * dy - right * dx) * s;
    }

    /// Orbite libre (outil 🔄) : yaw **et** pitch, pitch borné pour ne jamais
    /// passer la verticale (le repère haut/bas resterait sinon instable).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.5, 1.5);
    }

    /// Regard libre « à la première personne » (clic droit tenu dans l'éditeur,
    /// analyse comparative 2026-09-04 — caméra en vol façon Unity/Godot) : tourne
    /// yaw **et** pitch autour de l'**œil**, pas autour de `target` comme `orbit`.
    /// L'œil reste rigoureusement immobile ; `target` est recalculé pour que la
    /// nouvelle direction de vue parte du même point — sans ça, regarder à droite
    /// ferait tourner la caméra *autour* du décor au lieu de tourner la tête.
    /// Mêmes bornes de pitch qu'`orbit` (jamais la verticale).
    pub fn look_around(&mut self, dx: f32, dy: f32) {
        let eye = self.eye();
        self.yaw -= dx * 0.004;
        self.pitch = (self.pitch + dy * 0.004).clamp(-1.5, 1.5);
        // `eye() = target + offset(yaw, pitch, dist)` ⇒ `target = eye - offset`.
        let offset = self.eye() - self.target;
        self.target = eye - offset;
    }

    /// Zoom au glisser (outil 🔍) : vers le haut = avant, vers le bas = arrière.
    /// Mêmes bornes de distance que la molette (cf. `InputEvent::Scroll`).
    pub fn zoom_drag(&mut self, dy: f32) {
        self.distance = (self.distance + dy * 0.05).clamp(1.5, 50.0);
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = look_at_mat4(self.eye(), self.target, Vec3::Y);
        let proj = directx::perspective(self.fovy, self.aspect, 0.1, 100.0);
        proj * view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_around_keeps_the_eye_in_place_and_moves_the_target() {
        let mut cam = OrbitCamera::new(1.5);
        cam.target = Vec3::new(3.0, 1.0, -2.0);
        let eye = cam.eye();
        let target = cam.target;
        cam.look_around(120.0, -40.0);
        assert!(cam.eye().distance(eye) < 1e-4, "œil fixe");
        assert!(cam.target.distance(target) > 0.1, "la cible suit le regard");
        // La distance œil→cible est préservée (on tourne la tête, on ne zoome pas).
        assert!((cam.eye().distance(cam.target) - cam.distance).abs() < 1e-3);
    }

    #[test]
    fn look_around_never_crosses_the_vertical() {
        let mut cam = OrbitCamera::new(1.0);
        cam.look_around(0.0, 100_000.0);
        assert!(cam.pitch <= 1.5);
        cam.look_around(0.0, -100_000.0);
        assert!(cam.pitch >= -1.5);
    }
}
