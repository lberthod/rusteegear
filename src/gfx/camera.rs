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

    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, 0.1, 100.0);
        proj * view
    }
}
