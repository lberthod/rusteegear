//! Persistance : sauvegarde/chargement de scène (JSON), sauvegarde de partie
//! (`SaveGame`), import glTF en tâche de fond, redémarrage de partie
//! (`restart_game`) et score. Extrait de `app/mod.rs`.

use glam::{Quat, Vec3};

use super::simulation::{DEFAULT_CHASE_DISTANCE, DEFAULT_CHASE_PITCH, PLAYER_CAMERA_HEIGHT_OFFSET};
use super::{AppState, scene_path};
use crate::gfx::mesh::MeshData;
use crate::scene::{ImportedMesh, MeshKind, Scene, SceneObject, Transform};

impl AppState {
    /// Recommence la partie en cours (mode Play) : restaure la scène d'origine,
    /// reconstruit la physique et remet à zéro chrono/victoire/défaite. Permet de
    /// « Rejouer » depuis le jeu lui-même (essentiel sur APK, sans bouton Stop éditeur).
    pub fn restart_game(&mut self) {
        if self.play_snapshot.is_empty() {
            return;
        }
        self.scene.objects = self.play_snapshot.clone();
        // cf. AUDIT_MMORPG.md §4.2 : `play_snapshot` ne connaît pas les objets
        // ajoutés en cours de partie par `spawn_network_player` — sans ce
        // nettoyage, `network_players` pointerait vers des indices obsolètes
        // après la restauration.
        self.clear_network_players();
        // Même raison pour les boules de feu : le pool visuel vit dans
        // `scene.objects`, ajouté en cours de partie — indices obsolètes après
        // restauration (cf. `clear_fireballs`).
        self.clear_fireballs();
        self.clear_creature_shots();
        self.time = 0.0;
        self.sim_accumulator = 0.0;
        self.sim_prev_poses.clear();
        self.sim_curr_poses.clear();
        self.sim_render_poses.clear();
        self.win_time = None;
        self.lost = false;
        // Redémarrer depuis le menu pause (Phase J) doit aussi lever la pause —
        // sinon `advance_play` resterait gelé juste après la restauration.
        self.paused = false;
        self.score = 0;
        self.game_events.clear();
        self.trigger_prev.clear();
        self.furtive_awake.clear();
        self.lua_vars.clear();
        self.respawn_queue.clear();
        self.inventory.clear();
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.camera_shake = 0.0;
        self.ally_down_flash = 0.0;
        self.death_cause = None;
        self.attack_flash = 0.0;
        self.round_summary = None;
        self.round_contract_label = None;
        self.wave_banner_flash = 0.0;
        self.attack_cooldown_remaining = 0.0;
        self.attack_projectile = None;
        self.attack_charge = None;
        self.stagger.clear();
        self.tapped_obj = None;
        // Remet la manche 1 (révèle ses monstres, masque les suivantes) *avant* de
        // reconstruire la physique, pour que les corps rigides des monstres masqués ne
        // soient pas créés (cf. `init_waves`).
        self.init_waves();
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        if self.scene.camera_follow
            && let Some(p) = self.player_position()
        {
            self.camera.target = p + Vec3::new(0.0, PLAYER_CAMERA_HEIGHT_OFFSET, 0.0);
            if self.scene.game_camera.is_none() {
                self.camera.pitch = DEFAULT_CHASE_PITCH;
                self.camera.distance = DEFAULT_CHASE_DISTANCE;
            }
        }
    }

    /// A-t-on gagné le niveau (toutes les pièces-objectif ramassées) ?
    pub fn has_won(&self) -> bool {
        self.win_time.is_some()
    }

    /// Score courant (pièces ramassées) — affiché au HUD.
    pub fn score(&self) -> u32 {
        self.score
    }

    /// Capture l'état de partie courant dans une `SaveGame` : score,
    /// position de chaque objet, variables de script (`save.get`/`save.set` en Lua).
    pub fn capture_save(&self) -> crate::runtime::savegame::SaveGame {
        crate::runtime::savegame::SaveGame {
            version: crate::runtime::savegame::SaveGame::CURRENT_VERSION,
            score: self.score,
            positions: self
                .scene
                .objects
                .iter()
                .map(|o| o.transform.position.to_array())
                .collect(),
            lua_vars: self.lua_vars.clone(),
        }
    }

    /// Restaure une `SaveGame` sur la scène **actuellement chargée** : les
    /// positions s'appliquent objet par objet dans l'ordre, jusqu'au plus court des
    /// deux tableaux — une scène qui a changé depuis la sauvegarde (objets ajoutés/
    /// retirés) ne plante pas, elle restaure juste ce qui correspond encore.
    pub fn apply_save(&mut self, save: &crate::runtime::savegame::SaveGame) {
        self.score = save.score;
        for (obj, pos) in self.scene.objects.iter_mut().zip(&save.positions) {
            obj.transform.position = Vec3::from_array(*pos);
        }
        self.lua_vars = save.lua_vars.clone();
    }

    /// Sauvegarde la partie courante dans le slot `slot` (`user://save_<slot>.json`).
    pub fn save_game(&self, slot: &str) -> Result<(), String> {
        self.capture_save().save_to_slot(slot)
    }

    /// Comme `save_game`, mais avec un dossier explicite plutôt que le vrai
    /// `user_dir()` (Sprint 105a-3, isolation des tests) — même patron que
    /// `SaveGame::save_to_slot_at`.
    pub fn save_game_at(&self, slot: &str, dir: &std::path::Path) -> Result<(), String> {
        self.capture_save().save_to_slot_at(slot, dir)
    }

    /// Charge le slot `slot` et l'applique à la scène actuellement chargée. `Err` si
    /// le slot est vide/introuvable ou le JSON invalide — la scène n'est alors pas
    /// modifiée (l'erreur est renvoyée avant tout appel à `apply_save`).
    pub fn load_game(&mut self, slot: &str) -> Result<(), String> {
        let save = crate::runtime::savegame::SaveGame::load_from_slot(slot)?;
        self.apply_save(&save);
        Ok(())
    }

    /// Comme `load_game`, mais avec un dossier explicite (Sprint 105a-3,
    /// isolation des tests) — cf. la doc de `save_game_at`.
    pub fn load_game_at(&mut self, slot: &str, dir: &std::path::Path) -> Result<(), String> {
        let save = crate::runtime::savegame::SaveGame::load_from_slot_at(slot, dir)?;
        self.apply_save(&save);
        Ok(())
    }

    /// Incrémente le score de `n` points en émettant un événement `score:N` par valeur
    /// **traversée** — pas seulement la valeur finale : deux pièces
    /// ramassées le même tick ne doivent pas faire sauter `score:3` pour un script qui
    /// l'attend via `on_event`. Point de passage unique de **tous** les gains de score
    /// (pièces, armes, attaques, boule de feu, zones mortelles) : c'est ce qui rend
    /// l'événement fiable — un script n'a pas à savoir *comment* le point a été marqué.
    pub(crate) fn add_score(&mut self, n: u32) {
        for _ in 0..n {
            self.score += 1;
            self.game_events.push(format!("score:{}", self.score));
        }
    }

    /// Émet l'événement de gameplay `hud:<action>` pour un clic sur un widget HUD
    /// `Button` (`Scene::hud_widgets`, cf. Sprint 109) — même file que `emit()` côté
    /// Lua (`AppState::game_events`), lu au tick suivant via `on_event("hud:<action>")`.
    /// Le préfixe évite toute collision avec un nom d'événement choisi par un script.
    pub(crate) fn push_hud_event(&mut self, action: &str) {
        self.game_events.push(format!("hud:{action}"));
    }

    /// Passe au niveau suivant (boucle au niveau 1 après le dernier) et le charge en Play.
    pub fn next_level(&mut self) {
        self.level = self.level % crate::scene::CONTROLLER_LEVELS + 1;
        self.scene = crate::scene::Scene::controller_level(self.level);
        self.imported_dirty = true;
        self.is_leveled_demo = true;
        // Repart « en jeu » sur le nouveau niveau.
        self.play_snapshot = self.scene.objects.clone();
        self.restart_game();
    }

    /// Sauvegarde rapide vers l'emplacement par défaut (`~/motor3derust_scene.json`).
    pub fn save(&self) {
        self.save_to(&scene_path());
    }

    /// Sauvegarde la scène en JSON vers un chemin donné (« Enregistrer sous »).
    pub fn save_to(&self, path: &str) {
        match self.scene.save(path) {
            Ok(()) => log::info!("Scène sauvegardée dans {path}"),
            Err(e) => log::error!("Échec sauvegarde : {e}"),
        }
    }

    /// Charge la scène depuis l'emplacement par défaut.
    pub fn load(&mut self) {
        self.load_from(&scene_path());
    }

    /// Charge une scène depuis un chemin JSON donné, en thread de fond (sans bloquer
    /// le rendu). Le résultat est appliqué dans `poll_imports`.
    pub fn load_from(&mut self, path: &str) {
        let tx = self.scene_load_tx.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            let res = Scene::load(&path).map_err(|e| e.to_string()).map(|mut s| {
                s.reload_imported();
                s
            });
            let _ = tx.send(res);
        });
    }

    /// Lance l'import d'un modèle glTF/GLB en thread de fond (sans bloquer le rendu).
    pub fn import_gltf(&mut self, path: &str) {
        let tx = self.import_tx.clone();
        let p = path.to_string();
        std::thread::spawn(move || {
            let res = crate::scene::import::load_gltf(&p).map(|(d, mn, mx)| (p.clone(), d, mn, mx));
            let _ = tx.send(res);
        });
    }

    /// Récupère les imports terminés et les ajoute à la scène (appelé chaque frame).
    pub(super) fn poll_imports(&mut self) {
        while let Ok(res) = self.import_rx.try_recv() {
            match res {
                Ok((path, data, min, max)) => self.finish_import(path, data, min, max),
                Err(e) => log::error!("Import glTF échoué : {e}"),
            }
        }
        // scènes chargées en arrière-plan (Load) prêtes cette frame
        while let Ok(res) = self.scene_load_rx.try_recv() {
            match res {
                Ok(s) => {
                    self.scene = s;
                    self.clear_selection();
                    self.imported_dirty = true;
                }
                Err(e) => log::error!("Échec chargement : {e}"),
            }
        }
    }

    fn finish_import(&mut self, path: String, data: MeshData, min: Vec3, max: Vec3) {
        let name = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Modèle")
            .to_string();
        let idx = self.scene.imported.len() as u32;
        let mut imported = ImportedMesh {
            name: name.clone(),
            path,
            data,
            aabb_min: min,
            aabb_max: max,
            skeleton: None,
            clips: Vec::new(),
            vertex_skins: Vec::new(),
            tangents: Vec::new(),
            notifies: std::collections::HashMap::new(),
        };
        // Squelette/clips + tangentes : reparse le fichier
        // séparément, cf. `ImportedMesh::load_skinning` — silencieux si le mesh est
        // statique (squelette).
        imported.load_skinning();
        // Un GLB riggé démarre sur son clip par défaut (« Idle » ou le premier) plutôt
        // qu'en pose de liaison figée : sans `AnimationState`, il ne s'animerait jamais
        // — même `obj.anim = ...` en Lua est ignoré sur un état absent.
        let animation = imported
            .default_clip()
            .map(|clip| crate::scene::AnimationState {
                clip: clip.to_string(),
                ..Default::default()
            });
        self.scene.imported.push(imported);
        // Recadrage auto : centrer à l'origine, mise à l'échelle ~2 u.
        let size = max - min;
        let s = 2.0 / size.max_element().max(1e-3);
        let center = (min + max) * 0.5;
        self.scene.objects.push(SceneObject {
            name,
            transform: Transform {
                position: -center * s,
                rotation: Quat::IDENTITY,
                scale: Vec3::splat(s),
            },
            mesh: MeshKind::Imported(idx),
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
            animation,
            ..Default::default()
        });
        self.select_single(self.scene.objects.len() - 1);
    }
}
