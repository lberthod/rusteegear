//! Modèle de scène (sans ECS) : un Vec d'objets, chacun avec un Transform et un type de mesh.

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::gfx::mesh::{self, MeshData};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub fn from_pos(position: Vec3) -> Self {
        Self { position, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MeshKind {
    Cube,
    Sphere,
    Plane,
}

impl MeshKind {
    pub const ALL: [MeshKind; 3] = [MeshKind::Cube, MeshKind::Sphere, MeshKind::Plane];

    /// Données CPU du mesh, couleur par défaut selon le type.
    pub fn mesh_data(self) -> MeshData {
        match self {
            MeshKind::Cube => mesh::cube([0.8, 0.45, 0.2]),
            MeshKind::Sphere => mesh::sphere([0.3, 0.55, 0.85]),
            MeshKind::Plane => mesh::plane([0.35, 0.4, 0.35]),
        }
    }

    /// AABB local du mesh (avant application du Transform).
    pub fn local_aabb(self) -> (Vec3, Vec3) {
        match self {
            MeshKind::Cube | MeshKind::Sphere => {
                (Vec3::splat(-0.5), Vec3::splat(0.5))
            }
            // plan fin : un peu d'épaisseur en Y pour faciliter le picking
            MeshKind::Plane => (Vec3::new(-0.5, -0.02, -0.5), Vec3::new(0.5, 0.02, 0.5)),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SceneObject {
    pub name: String,
    pub transform: Transform,
    pub mesh: MeshKind,
}

#[derive(Serialize, Deserialize)]
pub struct Scene {
    pub objects: Vec<SceneObject>,
}

impl Scene {
    /// Scène de démonstration : un sol, un cube, une sphère.
    pub fn demo() -> Self {
        Scene {
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                        .with_scale(Vec3::new(10.0, 1.0, 10.0)),
                    mesh: MeshKind::Plane,
                },
                SceneObject {
                    name: "Cube".into(),
                    transform: Transform::from_pos(Vec3::new(-1.2, -0.5, 0.0)),
                    mesh: MeshKind::Cube,
                },
                SceneObject {
                    name: "Sphère".into(),
                    transform: Transform::from_pos(Vec3::new(1.2, -0.5, 0.0)),
                    mesh: MeshKind::Sphere,
                },
            ],
        }
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &str) -> std::io::Result<Scene> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}
