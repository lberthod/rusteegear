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
                Err(e) => log::error!(
                    "Rechargement de {} échoué : {e} — l'objet restera sans géométrie. \
                     Réimportez le fichier (📥 Importer glTF…) ou replacez-le dans le \
                     dossier d'assets du projet.",
                    m.path
                ),
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

    /// Sauvegarde atomique, avec backup de la version précédente (Sprint 6,
    /// audit du 19 juillet 2026). Avant ce sprint, cette fonction faisait un
    /// `fs::write` direct sur `path` : une coupure en pleine écriture (crash,
    /// disque plein, `kill -9`) laissait le fichier tronqué/corrompu, sans
    /// aucune version de secours. Désormais :
    /// 1. si `path` existe déjà, sa version courante est copiée vers
    ///    `<path>.backup` (une seule génération — suffisant pour l'alpha) ;
    ///    un échec de backup n'empêche pas la sauvegarde (juste un
    ///    avertissement) — perdre le backup est nettement moins grave que
    ///    perdre la scène ;
    /// 2. le JSON est écrit dans `<path>.tmp` **dans le même dossier** (donc
    ///    même volume — un `rename` inter-volumes échouerait) ;
    /// 3. `<path>.tmp` est renommé vers `path` : un `rename` est atomique sur
    ///    un même système de fichiers, donc `path` contient soit l'ancienne
    ///    version complète, soit la nouvelle — jamais un mélange tronqué.
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let path = std::path::Path::new(path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() {
            let backup = backup_path(path);
            if let Err(e) = std::fs::copy(path, &backup) {
                log::warn!(
                    "sauvegarde : backup {} impossible ({e}) — la sauvegarde continue quand même",
                    backup.display()
                );
            }
        }
        let tmp = tmp_path(path);
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
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

/// Chemin du fichier temporaire d'écriture atomique (Sprint 6) — dans le même
/// dossier que `path`, jamais un dossier séparé (`std::env::temp_dir()` peut
/// être sur un volume différent, ce qui ferait échouer le `rename` final).
fn tmp_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    std::path::PathBuf::from(s)
}

/// Chemin du backup de la version précédente (Sprint 6).
fn backup_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".backup");
    std::path::PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_scene_path(name: &str) -> std::path::PathBuf {
        // Sous-dossier dédié par test sous `target/`, jamais `$HOME` — même
        // isolation que les autres tests d'écriture du dépôt.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-scene-save")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("création du dossier de test");
        dir.join("scene.json")
    }

    #[test]
    fn save_leaves_no_temp_file_behind_on_success() {
        let path = temp_scene_path("pas-de-tmp-residuel");
        let scene = Scene::default();
        scene.save(path.to_str().unwrap()).expect("sauvegarde");
        assert!(path.exists());
        assert!(
            !tmp_path(&path).exists(),
            "le fichier .tmp intermédiaire ne doit pas survivre à une sauvegarde réussie"
        );
    }

    #[test]
    fn a_failed_write_to_the_temp_file_leaves_the_original_untouched() {
        // Simule l'échec « coupure en pleine écriture » : le fichier cible
        // existe déjà (première sauvegarde réussie), puis on force l'échec de
        // l'écriture du .tmp (répertoire remplacé par un fichier, qui ne peut
        // pas accueillir <path>.tmp) — `save` doit renvoyer une erreur SANS
        // toucher au contenu déjà présent dans `path`.
        let path = temp_scene_path("coupure-en-cours");
        let original = Scene::default();
        original
            .save(path.to_str().unwrap())
            .expect("première sauvegarde");
        let original_bytes = std::fs::read(&path).expect("lecture de l'original");

        // `<path>.tmp` est un dossier plutôt qu'un fichier : `fs::write` sur
        // ce chemin échoue systématiquement (EISDIR), avant même d'atteindre
        // le `rename` — reproduit fidèlement une écriture interrompue.
        std::fs::create_dir_all(tmp_path(&path)).expect("piège .tmp");

        let mut modified = Scene::default();
        modified.light.color = [9.0, 9.0, 9.0];
        let result = modified.save(path.to_str().unwrap());

        assert!(result.is_err(), "l'écriture du .tmp doit échouer");
        let untouched = std::fs::read(&path).expect("lecture après échec");
        assert_eq!(
            untouched, original_bytes,
            "path ne doit jamais contenir un mélange tronqué : soit l'ancienne \
             version complète, soit la nouvelle — ici l'ancienne, puisque l'écriture a échoué"
        );
    }

    #[test]
    fn saving_twice_leaves_the_previous_version_in_a_backup_file() {
        let path = temp_scene_path("backup-version-precedente");
        let mut scene = Scene::default();
        scene.light.color = [1.0, 0.0, 0.0];
        scene
            .save(path.to_str().unwrap())
            .expect("première sauvegarde");
        let first_version = std::fs::read_to_string(&path).expect("lecture v1");

        scene.light.color = [0.0, 1.0, 0.0];
        scene
            .save(path.to_str().unwrap())
            .expect("seconde sauvegarde");

        let backup = backup_path(&path);
        assert!(
            backup.exists(),
            ".backup doit exister après une 2ᵉ sauvegarde"
        );
        let backup_content = std::fs::read_to_string(&backup).expect("lecture du backup");
        assert_eq!(
            backup_content, first_version,
            "le backup doit contenir la version N-1, pas la version courante"
        );
        // Et la version courante (path) est bien la N, pas un mélange.
        let reloaded = Scene::load(path.to_str().unwrap()).expect("relecture");
        assert_eq!(reloaded.light.color, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn saving_once_creates_no_backup() {
        // Pas de N-1 à sauvegarder au tout premier enregistrement.
        let path = temp_scene_path("pas-de-backup-au-premier-coup");
        Scene::default()
            .save(path.to_str().unwrap())
            .expect("première sauvegarde");
        assert!(!backup_path(&path).exists());
    }
}
