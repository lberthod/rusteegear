//! Caméra orbitale : produit la matrice view-projection (compatible NDC wgpu, z in \[0,1\]).

use glam::{Mat4, Vec3};

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
    pub fovy: f32,
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
        }
    }

    pub fn eye(&self) -> Vec3 {
        let pitch = self.pitch.clamp(-1.54, 1.54);
        let x = self.distance * pitch.cos() * self.yaw.sin();
        let y = self.distance * pitch.sin();
        let z = self.distance * pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Vue+projection pour le rendu, avec un décalage additif appliqué à `target`
    /// (recul caméra en pixels-monde) — n'affecte que la matrice produite ici,
    /// jamais `self.target` : la caméra de jeu (suivi joueur, IA, réseau) reste
    /// inchangée, seul le rendu de la frame courante tressaute (Sprint 1,
    /// `sprint10audit.md` — retour d'encaissement de coup).
    pub fn view_proj_shaken(&self, shake_offset: Vec3) -> Mat4 {
        let view = Mat4::look_at_rh(
            self.eye() + shake_offset,
            self.target + shake_offset,
            Vec3::Y,
        );
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, 0.1, 100.0);
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

    /// Zoom au glisser (outil 🔍) : vers le haut = avant, vers le bas = arrière.
    /// Mêmes bornes de distance que la molette (cf. `InputEvent::Scroll`).
    pub fn zoom_drag(&mut self, dy: f32) {
        self.distance = (self.distance + dy * 0.05).clamp(1.5, 50.0);
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, 0.1, 100.0);
        proj * view
    }
}
