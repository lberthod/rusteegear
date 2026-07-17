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
        // Les clips viennent d'être rechargés : les objets skinnés sans état
        // d'animation (scènes anciennes, imports d'avant le clip par défaut) reçoivent
        // le leur maintenant — sinon ils resteraient figés en pose de liaison (T-pose).
        self.ensure_default_animations();
    }

    /// Donne un `AnimationState` par défaut (« Idle » ou premier clip, cf.
    /// `ImportedMesh::default_clip`) à tout objet dont le mesh importé a des clips mais
    /// qui n'en a pas encore. Sans état, un mesh skinné reste figé en pose de liaison
    /// (T-pose) et même `obj.anim = ...` en Lua est ignoré (cf. `app::scripting` :
    /// le script ne fait que changer le clip d'un état **existant**). Les phases de
    /// départ sont décalées d'un objet à l'autre pour éviter l'effet « armée
    /// synchronisée » quand plusieurs copies du même asset jouent le même clip
    /// (la lecture regarde `time % durée`, cf. `Clip::sample_joint` — n'importe
    /// quel temps positif est valide).
    pub fn ensure_default_animations(&mut self) {
        let imported = &self.imported;
        for (i, obj) in self.objects.iter_mut().enumerate() {
            if obj.animation.is_some() {
                continue;
            }
            let MeshKind::Imported(idx) = obj.mesh else {
                continue;
            };
            let Some(clip) = imported.get(idx as usize).and_then(|m| m.default_clip()) else {
                continue;
            };
            obj.animation = Some(super::AnimationState {
                clip: clip.to_string(),
                time: i as f32 * 0.37,
                ..Default::default()
            });
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
                    "kinematic" => PhysicsKind::Kinematic,
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
            hud_widgets: Vec::new(),
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
    pub const CURRENT_VERSION: u32 = 2;

    /// Met `self` à jour au schéma courant : applique les migrations manquantes selon
    /// `version`, puis stampe `CURRENT_VERSION`. Idempotente (une scène déjà à jour ne
    /// fait rien) — appelée par `load` après toute désérialisation depuis le disque ;
    /// inutile pour une scène construite en mémoire (démos, `Scene::default()`), déjà
    /// valide sans historique à corriger.
    ///
    /// **Migration v1 → v2 (Sprint 131), la première vraie migration de ce projet** :
    /// avant que l'inspecteur n'impose un plancher de 0,04 sur `SceneObject::roughness`
    /// (`ui.add(egui::Slider::new(&mut obj.roughness, 0.04..=1.0))`, `editor/mod.rs`),
    /// rien n'empêchait une scène d'être sauvée avec `roughness: 0.0` — une valeur
    /// **présente** dans le JSON, donc `#[serde(default = "default_roughness")]` ne
    /// s'applique pas (ce mécanisme ne comble que les champs *absents*, pas les valeurs
    /// hors plage). `roughness: 0.0` cause un artefact de rendu PBR classique (terme
    /// spéculaire dégénéré) — cette migration relève toute scène `version < 2` au
    /// plancher, sans toucher aux scènes déjà à jour (une valeur à 0,0 saisie
    /// délibérément après le Sprint 131 ne serait de toute façon plus possible via
    /// l'inspecteur, donc rien à corriger côté scènes récentes).
    pub(super) fn migrate(&mut self) {
        if self.version == 0 {
            let mut seen = std::collections::HashSet::new();
            self.groups.retain(|g| seen.insert(g.clone()));
        }
        if self.version < 2 {
            const MIN_ROUGHNESS: f32 = 0.04;
            for o in &mut self.objects {
                if o.roughness < MIN_ROUGHNESS {
                    o.roughness = MIN_ROUGHNESS;
                }
            }
        }
        self.version = Self::CURRENT_VERSION;
    }
}
