//! Sauvegarde/chargement JSON de la scène (avec migration de version) et rechargement
//! des meshes importés après un `Load`. Extrait de `scene/mod.rs`.

use glam::Vec3;

use super::{
    HudLayout, Light, MeshKind, MobileControls, Scene, SceneObject, SceneSpec, Sky, Transform,
    import,
};
use crate::runtime::physics::PhysicsKind;

impl Scene {
    /// Recharge la géométrie des meshes importés depuis leurs fichiers (après désérialisation).
    pub fn reload_imported(&mut self) {
        for m in &mut self.imported {
            match import::load_gltf(&m.path) {
                Ok((data, min, max)) => {
                    m.data = data;
                    m.aabb_min = min;
                    m.aabb_max = max;
                }
                Err(e) => log::error!("Rechargement de {} échoué : {e}", m.path),
            }
            m.load_skinning();
        }
    }

    /// Construit une scène depuis le JSON contraint produit par l'IA (cf. `app::ai`).
    pub fn from_ai_json(json: &str) -> Result<Scene, String> {
        let spec: SceneSpec =
            serde_json::from_str(json).map_err(|e| format!("JSON de scène invalide : {e}"))?;
        let objects: Vec<SceneObject> = spec
            .objects
            .into_iter()
            .map(|o| SceneObject {
                name: o.name,
                transform: Transform::from_pos(Vec3::new(o.x, o.y, o.z)),
                mesh: match o.mesh.as_str() {
                    "sphere" => MeshKind::Sphere,
                    "plane" => MeshKind::Plane,
                    "cylinder" => MeshKind::Cylinder,
                    "capsule" => MeshKind::Capsule,
                    _ => MeshKind::Cube,
                },
                script: o.script,
                physics: match o.physics.as_str() {
                    "static" => PhysicsKind::Static,
                    "dynamic" => PhysicsKind::Dynamic,
                    _ => PhysicsKind::None,
                },
                collider_shape: crate::runtime::physics::ColliderShape::Auto,
                group: String::new(),
                color: o.color,
                texture: String::new(),
                tappable: o.tappable,
                metallic: 0.0,
                roughness: 0.6,
                emissive: 0.0,
                trigger: false,
                ..Default::default()
            })
            .collect();
        if objects.is_empty() {
            return Err("La scène générée ne contient aucun objet".into());
        }
        Ok(Scene {
            objects,
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: spec.joystick,
                buttons: spec.buttons,
                ..Default::default()
            },
            camera_follow: spec.camera_follow,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            version: Scene::CURRENT_VERSION,
        })
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, json)
    }

    pub fn load(path: &str) -> std::io::Result<Scene> {
        let json = std::fs::read_to_string(path)?;
        let mut scene: Scene = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        scene.migrate();
        Ok(scene)
    }

    /// Version courante du schéma JSON de `Scene`.
    pub const CURRENT_VERSION: u32 = 1;

    /// Met `self` à jour au schéma courant : applique les migrations manquantes selon
    /// `version`, puis stampe `CURRENT_VERSION`. Idempotente (une scène déjà à jour ne
    /// fait rien) — appelée par `load` après toute désérialisation depuis le disque ;
    /// inutile pour une scène construite en mémoire (démos, `Scene::default()`), déjà
    /// valide sans historique à corriger.
    ///
    /// **Aucune migration réelle n'existe encore** dans ce projet (rien n'a encore
    /// changé de forme au point de dépasser un simple `#[serde(default)]`) : le seul
    /// correctif appliqué ici — dédoublonner `groups` — est une vraie correction
    /// d'hygiène pour un JSON ancien ou modifié à la main (des doublons de groupe ne
    /// plantent rien mais dupliquent l'entrée dans la hiérarchie de l'éditeur), pas une
    /// migration de schéma inventée après coup pour la forme. Le prochain vrai
    /// changement de schéma cassant ajoutera un bras `if self.version < N` ici.
    pub(super) fn migrate(&mut self) {
        if self.version == 0 {
            let mut seen = std::collections::HashSet::new();
            self.groups.retain(|g| seen.insert(g.clone()));
        }
        self.version = Self::CURRENT_VERSION;
    }
}
