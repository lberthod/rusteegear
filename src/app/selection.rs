//! Opérations d'édition sur la sélection courante : sélection simple/multiple,
//! presse-papiers (copier/couper/coller), alignement/distribution, groupes,
//! undo/redo, et manipulation d'objets (ajout, suppression, duplication, reset).
//! Extrait de `app/mod.rs` — logique d'éditeur pure, sans dépendance
//! au mode Play/scripts/réseau.

use glam::{Quat, Vec3};

use super::{AppState, SceneSnapshot};
use crate::scene::{MeshKind, SceneObject, Transform};

impl AppState {
    /// Décale tous les objets sélectionnés (échange d'ordre) — réordonnancement simple.
    pub fn move_selected_in_list(&mut self, down: bool) {
        let Some(i) = self.selection else { return };
        let n = self.scene.objects.len();
        let j = if down {
            if i + 1 >= n {
                return;
            }
            i + 1
        } else {
            if i == 0 {
                return;
            }
            i - 1
        };
        self.push_undo();
        self.scene.objects.swap(i, j);
        self.select_single(j);
    }

    /// Déplace l'objet `from` juste avant l'objet `to` dans l'ordre global
    /// (glisser-déposer de réordonnancement dans la hiérarchie). Passe par l'historique.
    pub fn reorder_object(&mut self, from: usize, to: usize) {
        let n = self.scene.objects.len();
        if from >= n || to >= n || from == to {
            return;
        }
        self.push_undo();
        let obj = self.scene.objects.remove(from);
        // Après le retrait, l'index cible se décale si `from` était avant lui.
        let dest = if from < to { to - 1 } else { to };
        self.scene.objects.insert(dest, obj);
        self.select_single(dest);
    }

    // --- sélection (primaire + ensemble) ---

    /// Mémorise les transforms d'origine de la sélection + leur centroïde (pivot),
    /// pour les manipulations multi-objets rotate/scale.
    pub(super) fn capture_drag_selection(&mut self) {
        self.drag_orig_transforms = self
            .selected
            .iter()
            .filter_map(|&i| self.scene.objects.get(i).map(|o| (i, o.transform)))
            .collect();
        let n = self.drag_orig_transforms.len().max(1) as f32;
        let sum: Vec3 = self
            .drag_orig_transforms
            .iter()
            .map(|(_, t)| t.position)
            .sum();
        self.drag_pivot = sum / n;
    }

    /// Sélectionne un seul objet (remplace l'ensemble).
    pub fn select_single(&mut self, i: usize) {
        self.selection = Some(i);
        self.selected = vec![i];
    }

    /// Vide toute la sélection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selected.clear();
    }

    /// Ajoute/retire un objet de l'ensemble sélectionné (clic Cmd/Maj).
    pub fn toggle_select(&mut self, i: usize) {
        if let Some(pos) = self.selected.iter().position(|&x| x == i) {
            self.selected.remove(pos);
            self.selection = self.selected.last().copied();
        } else {
            self.selected.push(i);
            self.selection = Some(i);
        }
    }

    /// Facteur de surbrillance d'un objet : primaire = 1.0, autre sélectionné = 0.55.
    pub fn highlight_of(&self, i: usize) -> f32 {
        if self.selection == Some(i) {
            1.0
        } else if self.selected.contains(&i) {
            0.55
        } else {
            0.0
        }
    }

    /// Copie les objets sélectionnés dans le presse-papiers.
    pub fn copy_selected(&mut self) {
        self.clipboard = self
            .selected
            .iter()
            .filter_map(|&i| self.scene.objects.get(i).cloned())
            .collect();
    }

    /// Couper : copie la sélection puis la supprime.
    pub fn cut_selected(&mut self) {
        self.copy_selected();
        self.delete_selected();
    }

    /// Sélectionne tous les objets de la scène.
    pub fn select_all(&mut self) {
        self.selected = (0..self.scene.objects.len()).collect();
        self.selection = self.selected.last().copied();
    }

    /// Répartit les objets sélectionnés à intervalles égaux le long d'un axe
    /// (extrémités conservées). Nécessite au moins 3 objets.
    pub fn distribute_selection_axis(&mut self, axis: usize) {
        let comp = |p: Vec3| match axis {
            0 => p.x,
            1 => p.y,
            _ => p.z,
        };
        // (index, valeur sur l'axe), triés par valeur.
        let mut items: Vec<(usize, f32)> = self
            .selected
            .iter()
            .filter_map(|&i| {
                self.scene
                    .objects
                    .get(i)
                    .map(|o| (i, comp(o.transform.position)))
            })
            .collect();
        if items.len() < 3 {
            return;
        }
        items.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (min, max) = (items[0].1, items[items.len() - 1].1);
        let step = (max - min) / (items.len() - 1) as f32;
        self.push_undo();
        for (rank, (idx, _)) in items.iter().enumerate() {
            let v = min + step * rank as f32;
            if let Some(o) = self.scene.objects.get_mut(*idx) {
                match axis {
                    0 => o.transform.position.x = v,
                    1 => o.transform.position.y = v,
                    _ => o.transform.position.z = v,
                }
            }
        }
    }

    /// Aligne la position des objets sélectionnés sur celle de la primaire, le long
    /// d'un axe (0 = X, 1 = Y, 2 = Z).
    pub fn align_selection_axis(&mut self, axis: usize) {
        let Some(primary) = self.selection else {
            return;
        };
        if self.selected.len() < 2 {
            return;
        }
        let Some(target) = self
            .scene
            .objects
            .get(primary)
            .map(|o| o.transform.position)
        else {
            return;
        };
        self.push_undo();
        for &i in &self.selected {
            if let Some(o) = self.scene.objects.get_mut(i) {
                match axis {
                    0 => o.transform.position.x = target.x,
                    1 => o.transform.position.y = target.y,
                    _ => o.transform.position.z = target.z,
                }
            }
        }
    }

    /// Regroupe les objets sélectionnés dans un nouveau groupe nommé automatiquement.
    pub fn group_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let name = format!("Groupe {}", self.scene.groups.len() + 1);
        for &i in &self.selected {
            if let Some(o) = self.scene.objects.get_mut(i) {
                o.group = name.clone();
            }
        }
        if !self.scene.groups.contains(&name) {
            self.scene.groups.push(name);
        }
    }

    /// Retire les objets sélectionnés de leur groupe (« Sans groupe »).
    pub fn ungroup_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        for &i in &self.selected {
            if let Some(o) = self.scene.objects.get_mut(i) {
                o.group.clear();
            }
        }
    }

    /// Colle le presse-papiers (décalé), et sélectionne les nouveaux objets.
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.push_undo();
        let start = self.scene.objects.len();
        let clips = self.clipboard.clone();
        for o in clips {
            let mut c = o.clone();
            c.name = format!("{} (copie)", c.name);
            c.transform.position += Vec3::new(0.6, 0.0, 0.6);
            self.scene.objects.push(c);
        }
        self.selected = (start..self.scene.objects.len()).collect();
        self.selection = self.selected.last().copied();
    }

    /// Supprime tous les objets sélectionnés (indices décroissants).
    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut idx = self.selected.clone();
        idx.sort_unstable();
        idx.dedup();
        for &i in idx.iter().rev() {
            if i < self.scene.objects.len() {
                self.scene.objects.remove(i);
            }
        }
        self.clear_selection();
    }

    // --- historique ---

    /// Capture l'état courant de la scène avant une modification (vide la pile redo).
    pub fn push_undo(&mut self) {
        self.undo_stack
            .push_back(SceneSnapshot::capture(&self.scene));
        if self.undo_stack.len() > 50 {
            self.undo_stack.pop_front(); // O(1), contrairement à Vec::remove(0)
        }
        self.redo_stack.clear();
    }

    /// Vide l'historique undo/redo — utilisé au démarrage de l'éditeur pour que
    /// la scène d'ouverture ne soit pas « annulable » vers la scène vide interne.
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop_back() {
            self.redo_stack.push(SceneSnapshot::capture(&self.scene));
            prev.restore(&mut self.scene);
            self.clear_selection();
            self.selected_light = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack
                .push_back(SceneSnapshot::capture(&self.scene));
            next.restore(&mut self.scene);
            self.clear_selection();
            self.selected_light = None;
        }
    }

    // --- édition d'objets (avec historique) ---

    pub fn add_object(&mut self, kind: MeshKind) {
        self.push_undo();
        let name = format!("{} {}", kind.label(), self.scene.objects.len());
        self.scene.objects.push(SceneObject {
            name,
            transform: Transform::from_pos(Vec3::ZERO),
            mesh: kind,
            script: String::new(),
            physics: crate::runtime::physics::PhysicsKind::None,
            collider_shape: crate::runtime::physics::ColliderShape::Auto,
            group: String::new(),
            color: [1.0, 1.0, 1.0],
            texture: String::new(),
            tappable: false,
            metallic: 0.0,
            roughness: 0.6,
            emissive: 0.0,
            trigger: false,
            ..Default::default()
        });
        self.select_single(self.scene.objects.len() - 1);
    }

    /// Recentre la caméra sur l'objet (ou la lumière) sélectionné (« frame selected », touche F).
    pub fn frame_selected(&mut self) {
        let target = self
            .selection
            .and_then(|i| self.scene.objects.get(i))
            .map(|o| o.transform.position)
            .or_else(|| {
                self.selected_light
                    .and_then(|i| self.scene.point_lights.get(i))
                    .map(|pl| Vec3::from_array(pl.position))
            });
        if let Some(t) = target {
            self.camera.target = t;
        }
    }

    /// Nouveau projet : vide la scène (avec historique pour pouvoir annuler).
    pub fn new_scene(&mut self) {
        self.push_undo();
        self.scene.objects.clear();
        self.scene.imported.clear();
        self.scene.groups.clear();
        self.clear_selection();
    }

    /// Pose la base des objets sélectionnés sur le plan du sol (y = 0).
    pub fn align_to_ground(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        for &i in &self.selected.clone() {
            if let Some(o) = self.scene.objects.get(i) {
                let (lmin, _) = self.scene.local_aabb(o.mesh);
                let base_offset = lmin.y * o.transform.scale.y;
                if let Some(o) = self.scene.objects.get_mut(i) {
                    o.transform.position.y = -base_offset;
                }
            }
        }
    }

    /// Réinitialise rotation et échelle des objets sélectionnés (position conservée).
    pub fn reset_transform(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        for &i in &self.selected.clone() {
            if let Some(o) = self.scene.objects.get_mut(i) {
                o.transform.rotation = Quat::IDENTITY;
                o.transform.scale = Vec3::ONE;
            }
        }
    }

    pub fn delete_object(&mut self, i: usize) {
        if i < self.scene.objects.len() {
            self.push_undo();
            self.scene.objects.remove(i);
            self.clear_selection();
        }
    }

    pub fn duplicate_selected(&mut self) {
        let mut idx = self.selected.clone();
        idx.sort_unstable();
        idx.dedup();
        idx.retain(|&i| i < self.scene.objects.len());
        if idx.is_empty() {
            return;
        }
        self.push_undo();
        let start = self.scene.objects.len();
        for i in idx {
            let mut copy = self.scene.objects[i].clone();
            copy.name = format!("{} (copie)", copy.name);
            copy.transform.position += Vec3::new(0.6, 0.0, 0.6);
            self.scene.objects.push(copy);
        }
        self.selected = (start..self.scene.objects.len()).collect();
        self.selection = self.selected.last().copied();
    }

    /// Sprint 96 (câblage UI, mécanisme livré au sprint lui-même) : sauvegarde l'objet
    /// sélectionné (primaire) comme prefab réutilisable — cf. `Scene::save_prefab`. Le
    /// nom de fichier dérive du nom de l'objet, nettoyé comme `BuildConfig::safe_name`
    /// (alphanumérique/`-`/`_`). L'objet source **n'est pas** transformé en instance
    /// liée : juste un nouvel asset créé depuis son état courant, comme un « enregistrer
    /// sous » plutôt qu'un « déplacer vers ». `scope` choisit entre un prefab général
    /// (visible depuis toute scène) et un prefab propre à une scène/projet nommé —
    /// complément UI : validation visuelle après création. Renvoie le nom effectif du
    /// prefab (pour le message de confirmation), pas juste `()`.
    pub fn save_selected_as_prefab(
        &mut self,
        scope: crate::assets::PrefabScope,
    ) -> Result<String, String> {
        let dir = crate::assets::assets_dir()
            .ok_or_else(|| "pas de dossier d'assets (HOME absent)".to_string())?;
        self.save_selected_as_prefab_at(&dir, scope)
    }

    /// Cœur de `save_selected_as_prefab`, paramétré par `dir` (testable sans toucher
    /// `~/.motor3derust/assets/` — même raison que `Scene::save_prefab_at`).
    pub(crate) fn save_selected_as_prefab_at(
        &mut self,
        dir: &std::path::Path,
        scope: crate::assets::PrefabScope,
    ) -> Result<String, String> {
        let obj = self
            .selection
            .and_then(|i| self.scene.objects.get(i))
            .ok_or_else(|| "aucun objet sélectionné".to_string())?;
        let name = sanitize_prefab_name(&obj.name);
        crate::scene::Scene::save_prefab_at(dir, obj, &name, &scope)?;
        Ok(name)
    }

    /// Instancie le prefab `asset_id` (cf. `Scene::instantiate_prefab`) à l'origine —
    /// même position de départ que `add_object`, à repositionner ensuite — et sélectionne
    /// la nouvelle instance. Sans effet si le prefab est introuvable/invalide.
    pub fn instantiate_prefab(&mut self, asset_id: &str) {
        let Some(dir) = crate::assets::assets_dir() else {
            return;
        };
        self.instantiate_prefab_at(&dir, asset_id);
    }

    /// Cœur de `instantiate_prefab`, paramétré par `dir` — même raison que
    /// `save_selected_as_prefab_at`.
    pub(crate) fn instantiate_prefab_at(&mut self, dir: &std::path::Path, asset_id: &str) {
        let name = format!("Instance {}", self.scene.objects.len());
        let Some(obj) = crate::scene::Scene::instantiate_prefab_at(dir, asset_id, name, Vec3::ZERO)
        else {
            return;
        };
        self.push_undo();
        self.scene.objects.push(obj);
        self.select_single(self.scene.objects.len() - 1);
    }

    /// Resynchronise toutes les instances de prefab de la scène avec leur template
    /// (cf. `Scene::sync_prefab_instances`) — mécanisme moteur livré au Sprint 96,
    /// jamais câblé à l'UI jusqu'ici (aucun appelant avant ce sprint).
    pub fn sync_prefab_instances(&mut self) {
        self.push_undo();
        self.scene.sync_prefab_instances();
    }
}

/// Nettoyage d'un nom d'objet en nom de fichier prefab valide — même règle que
/// `BuildConfig::safe_name` (alphanumérique/`-`/`_`, défaut si le résultat est vide).
fn sanitize_prefab_name(name: &str) -> String {
    let n: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if n.is_empty() { "Prefab".into() } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::PointLight;

    /// Invariant : la primaire (si présente) appartient toujours à l'ensemble sélectionné.
    fn assert_selection_invariant(app: &AppState) {
        if let Some(p) = app.selection {
            assert!(
                app.selected.contains(&p),
                "primaire {p} absente de selected {:?}",
                app.selected
            );
        } else {
            assert!(
                app.selected.is_empty(),
                "selection None mais selected non vide"
            );
        }
    }

    /// Dossier temporaire unique par test — même schéma que
    /// `assets::tests::temp_assets_dir`/`savegame::tests::temp_save_dir` : aucune
    /// dépendance au vrai `$HOME`, sûr sous exécution parallèle.
    fn temp_prefabs_dir(tag: &str) -> std::path::PathBuf {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        tag.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        let dir =
            std::env::temp_dir().join(format!("rusteegear_selection_test_{:x}", hasher.finish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_selected_as_prefab_then_instantiate_round_trips_through_disk() {
        let dir = temp_prefabs_dir("round_trip");
        let mut app = AppState::new();
        app.scene.objects.push(SceneObject {
            name: "Gemme".to_string(),
            mesh: MeshKind::Sphere,
            color: [1.0, 1.0, 0.0],
            ..Default::default()
        });
        app.select_single(app.scene.objects.len() - 1);

        let saved_name = app
            .save_selected_as_prefab_at(&dir, crate::assets::PrefabScope::General)
            .expect("sauvegarde du prefab attendue");
        assert_eq!(
            saved_name, "Gemme",
            "le nom renvoyé doit être celui du fichier créé"
        );

        let (_, asset_id) =
            crate::assets::list_prefabs_at(&dir, &crate::assets::PrefabScope::General)
                .into_iter()
                .find(|(name, _)| name == "Gemme")
                .expect("le prefab doit apparaître dans list_prefabs");

        app.instantiate_prefab_at(&dir, &asset_id);
        let last = app.scene.objects.last().expect("une instance attendue");
        assert_eq!(last.color, [1.0, 1.0, 0.0], "couleur héritée du template");
        assert!(last.prefab.is_some(), "l'instance doit être liée au prefab");
        assert_eq!(
            app.selection,
            Some(app.scene.objects.len() - 1),
            "la nouvelle instance doit être sélectionnée"
        );
    }

    #[test]
    fn save_selected_as_prefab_with_a_scene_scope_does_not_leak_into_general() {
        // Complément portées (général vs scène) : un prefab créé pour une scène
        // nommée n'apparaît pas dans la liste générale, et réciproquement.
        let dir = temp_prefabs_dir("scoped");
        let scene_name = "MaScene".to_string();
        let mut app = AppState::new();
        app.scene.objects.push(SceneObject {
            name: "Scoped".to_string(),
            mesh: MeshKind::Cube,
            ..Default::default()
        });
        app.select_single(app.scene.objects.len() - 1);

        app.save_selected_as_prefab_at(&dir, crate::assets::PrefabScope::Scene(scene_name.clone()))
            .expect("sauvegarde du prefab attendue");

        assert!(
            crate::assets::list_prefabs_at(&dir, &crate::assets::PrefabScope::General)
                .iter()
                .all(|(name, _)| name != "Scoped"),
            "un prefab de scène ne doit pas apparaître dans la portée générale"
        );
        assert!(
            crate::assets::list_prefabs_at(&dir, &crate::assets::PrefabScope::Scene(scene_name))
                .iter()
                .any(|(name, _)| name == "Scoped"),
            "le prefab doit apparaître dans la portée de scène où il a été créé"
        );
    }

    #[test]
    fn instantiate_prefab_with_an_unknown_id_does_nothing() {
        let dir = temp_prefabs_dir("unknown_id");
        let mut app = AppState::new();
        let before = app.scene.objects.len();
        app.instantiate_prefab_at(&dir, "asset-id://inconnu");
        assert_eq!(
            app.scene.objects.len(),
            before,
            "un id inconnu ne doit rien ajouter à la scène"
        );
    }

    #[test]
    fn save_selected_as_prefab_without_selection_fails_gracefully() {
        let mut app = AppState::new();
        app.selection = None;
        assert!(
            app.save_selected_as_prefab(crate::assets::PrefabScope::General)
                .is_err()
        );
    }

    #[test]
    fn sync_prefab_instances_is_safe_with_no_prefabs_in_scene() {
        let mut app = AppState::new();
        let before = app.scene.objects.len();
        app.scene.objects.push(SceneObject::default());
        app.sync_prefab_instances(); // ne doit pas paniquer sans instance de prefab
        assert_eq!(app.scene.objects.len(), before + 1);
    }

    #[test]
    fn selection_helpers_keep_invariant() {
        let mut app = AppState::new();
        app.select_single(2);
        assert_eq!(app.selection, Some(2));
        assert_eq!(app.selected, vec![2]);
        assert_selection_invariant(&app);

        app.toggle_select(5); // ajoute
        assert_eq!(app.selection, Some(5));
        assert!(app.selected.contains(&2) && app.selected.contains(&5));
        assert_selection_invariant(&app);

        app.toggle_select(5); // retire → primaire repasse au dernier restant
        assert!(!app.selected.contains(&5));
        assert_eq!(app.selection, Some(2));
        assert_selection_invariant(&app);

        app.toggle_select(2); // retire le dernier → plus rien
        assert_eq!(app.selection, None);
        assert!(app.selected.is_empty());
        assert_selection_invariant(&app);

        app.select_single(0);
        app.clear_selection();
        assert_selection_invariant(&app);
    }

    #[test]
    fn highlight_levels() {
        let mut app = AppState::new();
        app.select_single(0);
        app.toggle_select(1);
        assert_eq!(app.highlight_of(1), 1.0); // primaire
        assert_eq!(app.highlight_of(0), 0.55); // autre sélectionné
        assert_eq!(app.highlight_of(2), 0.0); // non sélectionné
    }

    #[test]
    fn undo_covers_point_lights() {
        let mut app = AppState::new();
        let n0 = app.scene.point_lights.len();
        app.push_undo();
        app.scene.point_lights.push(PointLight::default());
        assert_eq!(app.scene.point_lights.len(), n0 + 1);
        app.undo();
        assert_eq!(app.scene.point_lights.len(), n0); // lumière retirée par l'undo
        app.redo();
        assert_eq!(app.scene.point_lights.len(), n0 + 1); // ré-ajoutée
    }

    #[test]
    fn distribute_spaces_evenly() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        for x in [0.0, 1.0, 9.0] {
            app.scene.objects.push(SceneObject {
                name: "o".into(),
                transform: Transform::from_pos(Vec3::new(x, 0.0, 0.0)),
                mesh: MeshKind::Cube,
                script: String::new(),
                physics: crate::runtime::physics::PhysicsKind::None,
                collider_shape: crate::runtime::physics::ColliderShape::Auto,
                group: String::new(),
                color: [1.0; 3],
                texture: String::new(),
                tappable: false,
                metallic: 0.0,
                roughness: 0.6,
                emissive: 0.0,
                trigger: false,
                ..Default::default()
            });
        }
        app.selected = vec![0, 1, 2];
        app.distribute_selection_axis(0);
        // extrémités conservées (0 et 9), celui du milieu recalé à 4.5
        let xs: Vec<f32> = app
            .scene
            .objects
            .iter()
            .map(|o| o.transform.position.x)
            .collect();
        assert!((xs[0] - 0.0).abs() < 1e-5);
        assert!((xs[1] - 4.5).abs() < 1e-5);
        assert!((xs[2] - 9.0).abs() < 1e-5);
    }
}
