//! Prefabs : sauvegarde d'un objet comme prefab réutilisable, instanciation, et
//! resynchronisation des instances existantes avec leur prefab source. Extrait de
//! `scene/mod.rs`.

use glam::Vec3;

use super::{PrefabInstance, Scene, SceneObject};

impl Scene {
    /// Sauvegarde `obj` comme prefab dans `assets_dir()/prefabs/<name>.json`, enregistré
    /// dans le manifeste d'assets pour une référence stable — c'est ce qui permet de
    /// renommer le fichier prefab sans casser les instances qui le référencent. `Err` si
    /// `assets_dir()` est indisponible (pas de `$HOME`) ou si l'écriture disque échoue.
    pub fn save_prefab(obj: &SceneObject, name: &str) -> Result<String, String> {
        let json = serde_json::to_string_pretty(obj).map_err(|e| e.to_string())?;
        let dir = crate::assets::assets_dir()
            .ok_or_else(|| "pas de dossier d'assets (HOME absent)".to_string())?
            .join("prefabs");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file_name = format!("{name}.json");
        std::fs::write(dir.join(&file_name), json).map_err(|e| e.to_string())?;
        Ok(crate::assets::register_asset(&format!(
            "prefabs/{file_name}"
        )))
    }

    /// Crée une nouvelle instance du prefab référencé par `asset_id`, positionnée à
    /// `at` sous le nom `name`. `transform` et `name` sont surchargés dès la création
    /// (chaque instance a naturellement sa propre position et un nom distinct dans la
    /// hiérarchie) ; tout le reste suit le template tant qu'aucune autre surcharge n'est
    /// ajoutée. `None` si le prefab est introuvable ou son JSON invalide.
    pub fn instantiate_prefab(
        asset_id: &str,
        name: impl Into<String>,
        at: Vec3,
    ) -> Option<SceneObject> {
        let mut obj = load_prefab_object(asset_id)?;
        obj.name = name.into();
        obj.transform.position = at;
        obj.prefab = Some(PrefabInstance {
            asset_id: asset_id.to_string(),
            overrides: vec!["transform".to_string(), "name".to_string()],
        });
        Some(obj)
    }

    /// Resynchronise toutes les instances de prefab de la scène : pour chaque objet lié
    /// (`obj.prefab.is_some()`), copie depuis le template chaque champ **non listé** dans
    /// `PrefabInstance::overrides` — modifier un prefab « gemme » met à jour toutes ses
    /// instances, sauf leurs surcharges. Fusion au niveau JSON (`serde_json::Value`)
    /// plutôt que champ Rust par champ : `SceneObject`
    /// a des dizaines de champs, et une fusion générique évite d'avoir à étendre cette
    /// fonction à chaque nouveau champ ajouté au type. Un template introuvable (fichier
    /// prefab supprimé/déplacé) laisse l'instance telle quelle — pas d'erreur bruyante
    /// pour un cas qui peut survenir en édition normale.
    pub fn sync_prefab_instances(&mut self) {
        let mut cache: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        for obj in &mut self.objects {
            let Some(prefab) = obj.prefab.clone() else {
                continue;
            };
            let template = match cache.get(&prefab.asset_id) {
                Some(v) => v.clone(),
                None => {
                    let Some(v) = load_prefab_value(&prefab.asset_id) else {
                        continue;
                    };
                    cache.insert(prefab.asset_id.clone(), v.clone());
                    v
                }
            };
            let Ok(mut instance_value) = serde_json::to_value(&*obj) else {
                continue;
            };
            if let (Some(template_map), Some(instance_map)) =
                (template.as_object(), instance_value.as_object_mut())
            {
                for (key, val) in template_map {
                    // `prefab` : jamais copié depuis le template (préserverait le lien
                    // et les surcharges de l'instance, pas ceux — généralement absents
                    // — du template lui-même).
                    if key == "prefab" || prefab.overrides.iter().any(|o| o == key) {
                        continue;
                    }
                    instance_map.insert(key.clone(), val.clone());
                }
            }
            if let Ok(merged) = serde_json::from_value::<SceneObject>(instance_value) {
                *obj = merged;
            }
        }
    }
}

/// Charge et parse le JSON d'un prefab depuis sa référence stable (`asset-id://<uuid>`
/// ou tout autre schéma reconnu par `assets::read_bytes`).
fn load_prefab_value(asset_id: &str) -> Option<serde_json::Value> {
    let bytes = crate::assets::read_bytes(asset_id)?;
    let text = String::from_utf8(bytes).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_prefab_object(asset_id: &str) -> Option<SceneObject> {
    serde_json::from_value(load_prefab_value(asset_id)?).ok()
}
