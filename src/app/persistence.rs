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
        self.sim_poses.sim_accumulator = 0.0;
        self.sim_poses.sim_prev_poses.clear();
        self.sim_poses.sim_curr_poses.clear();
        self.sim_poses.sim_render_poses.clear();
        self.win_time = None;
        self.lost = false;
        // Nouvelle partie = nouvelle grâce d'apparition (roadmap v2 0.1 bis).
        self.play_grace = crate::app::health::SPAWN_GRACE_S;
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
        self.fx.damage_flash = 0.0;
        self.fx.camera_shake = 0.0;
        self.fx.ally_down_flash = 0.0;
        self.death_cause = None;
        self.fx.attack_flash = 0.0;
        self.round_summary = None;
        self.round_contract_label = None;
        self.fx.wave_banner_flash = 0.0;
        self.attack.attack_cooldown_remaining = 0.0;
        self.attack.attack_projectile = None;
        self.attack.attack_charge = None;
        self.attack.stagger.clear();
        self.touch.tapped_obj = None;
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
        self.async_load.imported_dirty = true;
        self.is_leveled_demo = true;
        // Repart « en jeu » sur le nouveau niveau.
        self.play_snapshot = self.scene.objects.clone();
        self.restart_game();
    }

    /// Applique les réglages de vue persistés (roadmap post-audit UX v2
    /// 2026-09-04, 6.4) — au démarrage de l'éditeur, cf. `Settings::editor_view`.
    pub fn apply_editor_view(&mut self, view: &super::settings::EditorView) {
        self.show_grid = view.grid;
        self.snap = view.snap;
        self.gizmo_mode = view.gizmo;
        self.debug_view = view.debug_view;
    }

    /// Cible de « 💾 Enregistrer » / Cmd+S : la scène de démarrage du projet
    /// ouvert (roadmap post-audit UX 2026-09-04, 1.1), sinon le fichier auquel
    /// la scène est liée (« Ouvrir… » / « Enregistrer sous… », roadmap
    /// post-audit UX v2 2026-09-04, 3.2). `None` : scène sans fichier —
    /// avant, Cmd+S écrivait alors en silence dans `~/motor3derust_scene.json`,
    /// et la barre de titre affichait ce nom même quand le fichier n'existait pas.
    pub fn save_target(&self) -> Option<String> {
        match &self.current_project {
            Some(project) => Some(project.main_scene_path.to_string_lossy().into_owned()),
            None => self
                .scene_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }

    /// Nom de fichier proposé par « Enregistrer sous… » : celui de la cible
    /// courante s'il y en a une, sinon un nom générique (roadmap 3.2 — avant,
    /// toujours « scene.json »).
    pub fn suggested_save_name(&self) -> String {
        self.save_target()
            .and_then(|t| {
                std::path::Path::new(&t)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "scene.json".to_string())
    }

    /// Sauvegarde rapide vers `save_target()`. Sans cible (scène sans fichier),
    /// demande à l'éditeur d'ouvrir « Enregistrer sous… » (roadmap 3.2) —
    /// `scene_dirty` reste posé tant que rien n'a été écrit.
    pub fn save(&mut self) {
        match self.save_target() {
            Some(path) => self.save_to(&path),
            None => self.pending_shortcut = Some(super::EditorShortcut::SaveAs),
        }
    }

    /// Titre de la fenêtre de l'éditeur (roadmap post-audit UX 2026-09-04, 1.4) :
    /// « Projet — scène.json • RusteeGear », le point signalant des
    /// modifications non sauvegardées (convention macOS). Sans fichier :
    /// « Sans titre » (roadmap 3.2). Mode player : le nom du produit seul.
    pub fn window_title(&self) -> String {
        if self.player {
            return "RusteeGear".to_string();
        }
        let file = self.display_scene_name();
        let dirty = if self.scene_dirty { " •" } else { "" };
        match &self.current_project {
            Some(project) => format!("{} — {file}{dirty} · RusteeGear", project.name),
            None => format!("{file}{dirty} · RusteeGear"),
        }
    }

    /// Nom court de la scène pour le titre et la barre d'état : le nom du
    /// fichier cible, ou « Sans titre » (roadmap 3.2).
    pub fn display_scene_name(&self) -> String {
        self.save_target()
            .map(|target| {
                std::path::Path::new(&target)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or(target)
            })
            .unwrap_or_else(|| "Sans titre".to_string())
    }

    /// Sauvegarde la scène en JSON vers un chemin donné (« Enregistrer sous »).
    /// Ne baisse le drapeau « non sauvegardé » (`scene_dirty`) que sur succès :
    /// après un échec, fermer la fenêtre doit continuer d'alerter. Sur succès,
    /// hors projet, le chemin devient la cible des Cmd+S suivants (roadmap 3.2).
    pub fn save_to(&mut self, path: &str) {
        match self.scene.save(path) {
            Ok(()) => {
                self.scene_dirty = false;
                if self.current_project.is_none() {
                    self.scene_file = Some(std::path::PathBuf::from(path));
                }
                log::info!("Scène sauvegardée dans {path}");
            }
            Err(e) => log::error!("Échec sauvegarde : {e}"),
        }
    }

    /// Empreinte des réglages de scène **hors objets** (lumière, ciel, contrôles,
    /// caméra de jeu, HUD, groupes) — ce que Stop ne restaure pas depuis
    /// `play_snapshot`. Comparée entre l'entrée en Play et le Stop pour
    /// rendre au drapeau « modifié » sa valeur d'avant Play sans masquer une
    /// édition faite en pause (roadmap post-audit UX v2 2026-09-04, 3.4).
    pub(crate) fn scene_settings_fingerprint(&self) -> String {
        let s = &self.scene;
        serde_json::to_string(&(
            &s.light,
            &s.sky,
            &s.point_lights,
            &s.mobile,
            s.camera_follow,
            &s.game_camera,
            &s.hud_layout,
            &s.hud_widgets,
            &s.groups,
        ))
        .unwrap_or_default()
    }

    /// Empreinte JSON des parties de la scène éditables directement par les widgets
    /// egui (Inspecteur, éditeur de widgets HUD…), qui mutent la scène sans passer
    /// par `push_undo`. Comparée juste avant/après la construction de l'UI d'une
    /// frame pour poser `scene_dirty` sur une édition de champ. Volontairement
    /// limitée à l'objet sélectionné + aux réglages de scène (pas `objects` entier
    /// ni `imported`) : les opérations structurelles passent, elles, par `push_undo`,
    /// et sérialiser toute la scène à chaque frame serait un coût inutile.
    pub fn ui_scene_fingerprint(&self) -> String {
        let s = &self.scene;
        let selected_obj = self.selection.and_then(|i| s.objects.get(i));
        serde_json::to_string(&(
            selected_obj,
            &s.light,
            &s.sky,
            &s.point_lights,
            &s.mobile,
            s.camera_follow,
            &s.game_camera,
            &s.hud_layout,
            &s.hud_widgets,
            &s.groups,
        ))
        .unwrap_or_default()
    }

    /// Charge la scène depuis l'emplacement par défaut.
    pub fn load(&mut self) {
        self.load_from(&scene_path());
    }

    /// Charge une scène depuis un chemin JSON donné, en thread de fond (sans bloquer
    /// le rendu). Le résultat est appliqué dans `poll_imports`, qui lie alors la
    /// scène à ce chemin (`scene_file`, roadmap 3.2).
    pub fn load_from(&mut self, path: &str) {
        let tx = self.async_load.scene_load_tx.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            let sent_path = path.clone();
            // Erreur enrichie du chemin et d'une piste de réparation (Phase C5,
            // sprint.19matin.md) : « Échec chargement : expected value » sans le
            // fichier concerné n'est pas diagnosticable par l'utilisateur.
            let res = Scene::load(&path)
                .map_err(|e| {
                    format!(
                        "{path} : {e} — le fichier est-il bien une scène JSON \
                         (produite par 💾 Enregistrer) ? La scène actuelle est conservée."
                    )
                })
                .map(|mut s| {
                    s.reload_imported();
                    (sent_path, s)
                });
            let _ = tx.send(res);
        });
    }

    /// Variante **synchrone** de `load_from`, pour le pont de pilotage
    /// (`crate::pilot`) : sous App Nap (fenêtre masquée), le thread de fond de
    /// `load_from` peut être étranglé plusieurs secondes par macOS — un pilote
    /// externe a besoin d'une réponse « scène chargée » fiable et immédiate
    /// (~15 ms pour la scène du hameau). Même intégration que `poll_imports`.
    pub fn load_from_blocking(&mut self, path: &str) -> Result<usize, String> {
        let mut s = Scene::load(path).map_err(|e| format!("{path} : {e}"))?;
        s.reload_imported();
        self.scene = s;
        self.scene_file = Some(std::path::PathBuf::from(path));
        self.clear_selection();
        self.async_load.imported_dirty = true;
        self.scene_dirty = false;
        Ok(self.scene.objects.len())
    }

    /// Lit le manifeste d'un projet et charge sa scène de démarrage — la partie
    /// **sans état** de l'ouverture, partagée par `open_project` (synchrone) et
    /// `open_project_async` (thread de fond, roadmap post-audit UX v2
    /// 2026-09-04, 6.1). Ne touche pas `self` : peut tourner hors du thread UI.
    fn read_project(dir: &std::path::Path) -> Result<(crate::project::ProjectRoot, Scene), String> {
        let manifest = crate::project::ProjectManifest::load(dir)?;
        let scene_path = manifest.resolve_main_scene(dir)?;
        let path = scene_path.to_string_lossy().into_owned();
        let mut scene = Scene::load(&path).map_err(|e| format!("{path} : {e}"))?;
        scene.reload_imported();
        Ok((
            crate::project::ProjectRoot {
                name: manifest.name,
                root: dir.to_path_buf(),
                main_scene_path: scene_path,
            },
            scene,
        ))
    }

    /// Installe un projet lu par `read_project` : scène, fichier lié, sélection,
    /// drapeaux, `current_project`. Renvoie le nombre d'objets.
    fn install_project(&mut self, project: crate::project::ProjectRoot, scene: Scene) -> usize {
        self.scene = scene;
        self.scene_file = Some(project.main_scene_path.clone());
        self.clear_selection();
        self.async_load.imported_dirty = true;
        self.scene_dirty = false;
        self.current_project = Some(project);
        self.scene.objects.len()
    }

    /// Ouvre un projet **en thread de fond** (roadmap post-audit UX v2
    /// 2026-09-04, 6.1) : la barre d'état affiche « Ouverture de X… » et les
    /// panneaux sont grisés (`busy_label`) jusqu'à ce que `poll_imports`
    /// installe la scène — avant, `open_project` bloquait le rendu le temps de
    /// lire une scène de plusieurs Mo. Le résultat (récents, modale d'erreur)
    /// est à relever par `take_project_open_outcome`. Ignoré si une ouverture
    /// ou une duplication est déjà en cours.
    pub fn open_project_async(&mut self, dir: &std::path::Path) {
        if self.async_load.busy.is_some() {
            log::warn!("Une opération de projet est déjà en cours — ouverture ignorée.");
            return;
        }
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        self.async_load.busy = Some(format!("Ouverture de {name}…"));
        let tx = self.async_load.project_tx.clone();
        let dir = dir.to_path_buf();
        std::thread::spawn(move || {
            let res = Self::read_project(&dir).map_err(|e| (dir, e));
            let _ = tx.send(res);
        });
    }

    /// Résultat de la dernière `open_project_async` terminée : `Ok(nombre
    /// d'objets)` ou `Err((dossier, message))` — consommé une fois par
    /// `gfx::renderer` pour noter le projet dans les récents ou ouvrir la
    /// modale d'erreur (même suite qu'après `open_project`).
    pub fn take_project_open_outcome(
        &mut self,
    ) -> Option<Result<usize, (std::path::PathBuf, String)>> {
        self.async_load.project_outcome.take()
    }

    /// Libellé de la tâche de fond en cours pour la barre d'état (roadmap
    /// 6.1/6.2) : ouverture/duplication de projet d'abord (bloquantes), sinon
    /// « Import de X… » (le premier des imports glTF en cours, avec le nombre
    /// des autres), sinon `None`.
    pub fn busy_label(&self) -> Option<String> {
        if let Some(busy) = &self.async_load.busy {
            return Some(busy.clone());
        }
        let first = self.async_load.importing.first()?;
        let others = self.async_load.importing.len() - 1;
        Some(if others == 0 {
            format!("Import de {first}…")
        } else {
            format!("Import de {first}… (+{others})")
        })
    }

    /// Un libellé de `busy_label` désigne-t-il une tâche qui va remplacer la
    /// scène (ouverture/duplication de projet) ? L'éditeur grise alors ses
    /// panneaux ; un import glTF ne bloque rien.
    pub fn busy_label_blocks_ui(label: &str) -> bool {
        label.starts_with("Ouverture de ") || label.starts_with("Duplication de ")
    }

    /// Ouvre un projet (Sprint 3) : charge et valide
    /// `<dir>/project.rusteegear.json`, résout sa scène de démarrage, la charge,
    /// puis pose `current_project`. Synchrone comme `load_from_blocking` — pour
    /// le démarrage (dernier projet rouvert), la création de projet (scène
    /// tout juste écrite, lecture immédiate) et les tests ; l'éditeur, lui,
    /// passe par `open_project_async` (roadmap 6.1).
    pub fn open_project(&mut self, dir: &std::path::Path) -> Result<usize, String> {
        let (project, scene) = Self::read_project(dir)?;
        Ok(self.install_project(project, scene))
    }

    /// Crée un projet (Sprint 4) : dossier `<location>/<nom assaini>/`, peuplé
    /// d'un template en mémoire puis sauvegardé comme scène de démarrage, plus
    /// son manifeste. Termine en rouvrant le projet fraîchement créé (réutilise
    /// `open_project` — pose `current_project`, évite de dupliquer cette
    /// logique). Refuse d'écraser un dossier déjà existant : un nom de projet
    /// en collision doit être choisi explicitement par l'utilisateur, jamais
    /// mélangé silencieusement avec un contenu préexistant.
    pub fn create_project(
        &mut self,
        location: &std::path::Path,
        name: &str,
        template: crate::project::ProjectTemplate,
    ) -> Result<std::path::PathBuf, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("le nom du projet ne peut pas être vide".to_string());
        }
        let root = location.join(crate::project::sanitize_folder_name(trimmed));
        if root.exists() {
            return Err(format!(
                "{} existe déjà — choisis un autre nom ou un autre emplacement",
                root.display()
            ));
        }
        std::fs::create_dir_all(root.join("scenes"))
            .map_err(|e| format!("création du dossier de projet impossible : {e}"))?;
        std::fs::create_dir_all(root.join("scripts"))
            .map_err(|e| format!("création du dossier de scripts impossible : {e}"))?;

        match template {
            crate::project::ProjectTemplate::Empty => self.new_scene(),
            crate::project::ProjectTemplate::Controller => self.load_controller_demo(),
            crate::project::ProjectTemplate::CombatDemo => self.load_zombies_demo(),
        }

        let main_scene = "scenes/main.scene.json";
        let scene_path = root.join(main_scene);
        self.scene
            .save(scene_path.to_str().ok_or("chemin de projet non UTF-8")?)
            .map_err(|e| format!("écriture de la scène de démarrage impossible : {e}"))?;

        let manifest = crate::project::ProjectManifest {
            format: 1,
            name: trimmed.to_string(),
            main_scene: main_scene.to_string(),
            build: None,
        };
        manifest.write(&root)?;

        self.open_project(&root)?;
        Ok(root)
    }

    /// Ferme le projet ouvert : revient à l'état « sans projet » (scène vide),
    /// sans jamais perdre de modifications non sauvegardées en silence — cf.
    /// `request_close_project`, qui vérifie `scene_dirty` avant d'appeler ceci.
    pub fn close_project(&mut self) {
        self.current_project = None;
        self.new_scene();
        self.confirm_close_project = false;
    }

    /// Demande la fermeture du projet courant (Sprint 4). Si la scène a des
    /// modifications non sauvegardées, ouvre la confirmation
    /// (`confirm_close_project`, même esprit que `request_quit`/`confirm_quit`
    /// mais pour fermer le projet plutôt que l'application entière) au lieu de
    /// fermer directement. Sans effet si aucun projet n'est ouvert.
    pub fn request_close_project(&mut self) {
        if self.current_project.is_none() {
            return;
        }
        if self.scene_dirty {
            self.confirm_close_project = true;
        } else {
            self.close_project();
        }
    }

    /// Duplique le projet ouvert dans un dossier `<nom> copie` (même parent),
    /// avec un manifeste renommé — le projet ouvert dans l'éditeur n'est pas
    /// affecté (pas de bascule automatique sur la copie, comme un « Dupliquer »
    /// de Finder). Erreur si aucun projet n'est ouvert ou si la destination
    /// existe déjà. Synchrone (tests, pilotage) ; l'éditeur passe par
    /// `duplicate_project_async` (roadmap 6.1).
    pub fn duplicate_project(&mut self) -> Result<std::path::PathBuf, String> {
        let project = self
            .current_project
            .clone()
            .ok_or("aucun projet ouvert à dupliquer")?;
        Self::copy_project(&project)
    }

    /// Copie du projet sur disque (`<nom> copie`, manifeste renommé) — partie
    /// sans état de `duplicate_project`, exécutable hors du thread UI.
    fn copy_project(project: &crate::project::ProjectRoot) -> Result<std::path::PathBuf, String> {
        let new_name = format!("{} copie", project.name);
        let parent = project
            .root
            .parent()
            .ok_or("le projet n'a pas de dossier parent")?;
        let dst = parent.join(crate::project::sanitize_folder_name(&new_name));
        if dst.exists() {
            return Err(format!("{} existe déjà", dst.display()));
        }
        crate::project::copy_dir_recursive(&project.root, &dst)?;

        let mut manifest = crate::project::ProjectManifest::load(&dst)?;
        manifest.name = new_name;
        manifest.write(&dst)?;
        Ok(dst)
    }

    /// `duplicate_project` en thread de fond (roadmap post-audit UX v2
    /// 2026-09-04, 6.1) : « Duplication de X… » dans la barre d'état, panneaux
    /// grisés, résultat journalisé (toast) par `poll_imports` — copier un
    /// dossier d'assets de plusieurs centaines de Mo figeait la fenêtre.
    pub fn duplicate_project_async(&mut self) {
        let Some(project) = self.current_project.clone() else {
            log::error!("Duplication du projet échouée : aucun projet ouvert à dupliquer");
            return;
        };
        if self.async_load.busy.is_some() {
            log::warn!("Une opération de projet est déjà en cours — duplication ignorée.");
            return;
        }
        self.async_load.busy = Some(format!("Duplication de {}…", project.name));
        let tx = self.async_load.duplicate_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Self::copy_project(&project));
        });
    }

    /// Nom court d'un fichier pour la barre d'état (« Import de X… »).
    fn short_file_name(path: &str) -> String {
        std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    }

    /// Lance l'import d'un modèle glTF/GLB en thread de fond (sans bloquer le
    /// rendu). « Import de X… » reste dans la barre d'état jusqu'au résultat
    /// (roadmap post-audit UX v2 2026-09-04, 6.2).
    pub fn import_gltf(&mut self, path: &str) {
        let tx = self.async_load.import_tx.clone();
        let p = path.to_string();
        self.async_load.importing.push(Self::short_file_name(path));
        std::thread::spawn(move || {
            // Même principe que `load_from` (Phase C5) : l'erreur cite le fichier
            // et une piste — un « Import glTF échoué : invalid magic » sans chemin
            // ne dit pas quel fichier réessayer.
            let res = crate::scene::import::load_gltf(&p)
                .map(|(d, mn, mx)| (p.clone(), d, mn, mx))
                .map_err(|e| {
                    format!(
                        "{p} : {e} — le fichier est-il un .glb/.gltf valide \
                         (export Blender : glTF 2.0) ? La scène n'a pas été modifiée."
                    )
                });
            let _ = tx.send((p, res));
        });
    }

    /// Récupère les imports terminés et les ajoute à la scène (appelé chaque frame).
    pub(super) fn poll_imports(&mut self) {
        while let Ok((requested, res)) = self.async_load.import_rx.try_recv() {
            let name = Self::short_file_name(&requested);
            if let Some(pos) = self.async_load.importing.iter().position(|n| *n == name) {
                self.async_load.importing.remove(pos);
            }
            match res {
                Ok((path, data, min, max)) => self.finish_import(path, data, min, max),
                Err(e) => log::error!("Import glTF échoué : {e}"),
            }
        }
        // projet ouvert en arrière-plan (roadmap 6.1) prêt cette frame
        while let Ok(res) = self.async_load.project_rx.try_recv() {
            self.async_load.busy = None;
            self.async_load.project_outcome = Some(match res {
                Ok((project, scene)) => Ok(self.install_project(project, scene)),
                Err(e) => Err(e),
            });
        }
        // duplication de projet en arrière-plan (roadmap 6.1) terminée
        while let Ok(res) = self.async_load.duplicate_rx.try_recv() {
            self.async_load.busy = None;
            match res {
                Ok(dst) => log::info!("Projet dupliqué dans {}", dst.display()),
                Err(e) => log::error!("Duplication du projet échouée : {e}"),
            }
        }
        // scènes chargées en arrière-plan (Load) prêtes cette frame
        while let Ok(res) = self.async_load.scene_load_rx.try_recv() {
            match res {
                Ok((path, s)) => {
                    self.scene = s;
                    self.scene_file = Some(std::path::PathBuf::from(path));
                    self.clear_selection();
                    self.async_load.imported_dirty = true;
                    // Une scène fraîchement chargée depuis le disque est, par
                    // définition, identique à sa version sauvegardée.
                    self.scene_dirty = false;
                }
                Err(e) => log::error!("Échec chargement : {e}"),
            }
        }
    }

    pub(super) fn finish_import(&mut self, path: String, data: MeshData, min: Vec3, max: Vec3) {
        // Annulable et marqué modifié (roadmap post-audit UX v2 2026-09-04,
        // 3.5) — avant, un import n'entrait pas dans l'historique et ne posait
        // pas le drapeau « non sauvegardé ».
        self.push_undo();
        let name = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Modèle")
            .to_string();
        // Toast de fin d'import (roadmap post-audit UX v2 2026-09-04, 6.2) —
        // ce module est dans le filtre des toasts (`editor::toasts`).
        log::info!("« {name} » importé ({} triangles)", data.indices.len() / 3);
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
