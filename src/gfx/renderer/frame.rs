use super::*;

impl Renderer {
    pub fn render(&mut self, app: &mut AppState) {
        // 0. Acquérir la surface EN PREMIER. Si indisponible, on sort avant de lancer
        //    egui : sinon on jetterait le `textures_delta` de la frame (atlas de police),
        //    ce qui désynchronise le renderer egui (panic).
        let Some(surface) = self.surface.as_ref() else {
            return; // rendu headless : `render_scene_headless` est le chemin utilisé
        };
        use wgpu::CurrentSurfaceTexture as C;
        let frame = match surface.get_current_texture() {
            C::Success(t) | C::Suboptimal(t) => t,
            C::Outdated | C::Lost => {
                surface.configure(&self.device, &self.config);
                return;
            }
            C::Timeout | C::Occluded => return,
            C::Validation => {
                log::error!("surface validation error");
                return;
            }
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        // Profiler GPU (Sprint 112) : n'écrit les timestamp queries que si le
        // panneau est ouvert **et** le device les supporte — cf. doc de `GpuProfiler`.
        //
        // Désactivé (2026-07-18, Phase 0 de `sprintoptimation3daudit10h.md`) : sur cette
        // machine (Metal, Apple M1), `read_gpu_pass_timings` ne revient jamais dans un
        // délai raisonnable dès que le Profiler est ouvert — chaque frame attend jusqu'au
        // timeout borné (1s, cf. sa doc), ce qui rend l'éditeur perçu comme figé tant que
        // le panneau reste ouvert. FPS/draw calls/`skinned_dropped` restent mesurables
        // sans cette fonctionnalité (non gatée par `gpu_profiling`, cf. plus bas) ; seul le
        // détail des temps GPU par passe (Ombres/Scène/HDR+Bloom/UI) est perdu. À
        // ré-investiguer avec un vrai débogueur GPU avant de réactiver.
        let gpu_profiling = false;

        // 1. Construire l'UI éditeur. En mode player : pas de panneaux, mais on
        //    dessine quand même les contrôles tactiles (joystick + boutons).
        // Calculé avant les appels mutant `app` (évite un conflit d'emprunt au site d'appel).
        let game_time = app.hud_timer();
        let score = app.score();
        let lost = app.is_lost();
        let won = app.has_won();
        let wave = app.wave;
        let mut restart = false;
        let mut resume = false;
        let mut player_net_actions = None;
        let full_output = if app.player {
            if app.scene.mobile.any() {
                let net_status = app.net_status.clone();
                let net_connected = app.is_connected();
                let weapon_label = app.selected_weapon_label();
                let defeated = app.is_locally_defeated();
                let kills = app.displayed_kill_count();
                let assists = app.displayed_assist_count();
                let weapon_inventory = app.ranged_weapon_display_info();
                let selected_weapon = app.selected_weapon();
                let item_inventory = app.inventory_items().to_vec();
                let roster = app.multiplayer_roster();
                let ally_marker = app
                    .nearest_downed_ally_position()
                    .map(|p| (app.camera.view_proj(), p));
                let minimap = app.minimap_data();
                let (output, actions) = editor.run_player_overlay(
                    &window,
                    &app.scene,
                    &mut app.input_state,
                    app.device_preview,
                    app.device_portrait,
                    app.hud_health,
                    app.damage_flash,
                    app.ally_down_flash,
                    ally_marker,
                    game_time,
                    score,
                    lost,
                    won,
                    wave,
                    &mut restart,
                    app.paused,
                    &mut resume,
                    &net_status,
                    net_connected,
                    weapon_label,
                    defeated,
                    app.death_cause,
                    kills,
                    assists,
                    &weapon_inventory,
                    selected_weapon,
                    &item_inventory,
                    &roster,
                    app.round_summary.as_deref(),
                    app.round_summary_won,
                    app.round_contract_label,
                    app.wave_banner_flash,
                    app.wave_banner_wave,
                    &minimap,
                    app.locale,
                );
                if let Some(i) = actions.select_weapon {
                    app.select_weapon(i);
                }
                if let Some(kind) = actions.use_item {
                    app.use_item(kind);
                }
                for action in &actions.hud_clicks {
                    app.push_hud_event(action);
                }
                player_net_actions = Some(actions);
                Some(output)
            } else {
                None
            }
        } else {
            let (gpu_pass_timings_ms, gpu_draw_calls) = self.gpu_profiler_info();
            let status = crate::editor::StatusInfo {
                fps: app.fps(),
                backend: &self.backend,
                ai_busy: app.ai_busy,
                grid: app.show_grid,
                snap: app.snap,
                debug_view: app.debug_view,
                gpu_pass_timings_ms,
                gpu_draw_calls,
                skinned_dropped: self.skinned_dropped_count(),
            };
            let net_status = app.net_status.clone();
            let net_connected = app.is_connected();
            let has_firebase_account = app.has_firebase_account();
            let weapon_label = app.selected_weapon_label();
            let defeated = app.is_locally_defeated();
            let kills = app.displayed_kill_count();
            let assists = app.displayed_assist_count();
            let weapon_inventory = app.ranged_weapon_display_info();
            let selected_weapon = app.selected_weapon();
            let item_inventory = app.inventory_items().to_vec();
            let roster = app.multiplayer_roster();
            let minimap = app.minimap_data();
            let ally_marker = app
                .nearest_downed_ally_position()
                .map(|p| (app.camera.view_proj(), p));
            // Détection d'édition de champs UI (Inspecteur…) pour le drapeau
            // « scène modifiée » : les widgets egui mutent la scène directement,
            // sans passer par `push_undo` — on compare une empreinte des parties
            // éditables juste avant/après la construction de l'UI de la frame.
            let ui_fingerprint_before = app.ui_scene_fingerprint();
            let (full_output, actions) = editor.run(
                &window,
                &mut app.scene,
                &mut app.selection,
                &mut app.selected,
                &mut app.selected_light,
                &mut app.playing,
                &mut app.paused,
                &mut app.time_scale,
                &mut app.gizmo_mode,
                &mut app.input_state,
                &mut app.device_preview,
                &mut app.device_portrait,
                &mut app.view_rect_px,
                app.hud_health,
                app.damage_flash,
                app.ally_down_flash,
                ally_marker,
                game_time,
                score,
                lost,
                won,
                wave,
                status,
                &net_status,
                net_connected,
                &app.chat_messages,
                has_firebase_account,
                &app.leaderboard,
                &app.online_players,
                weapon_label,
                defeated,
                app.death_cause,
                kills,
                assists,
                &weapon_inventory,
                selected_weapon,
                &item_inventory,
                &roster,
                app.round_summary.as_deref(),
                app.round_summary_won,
                app.round_contract_label,
                app.wave_banner_flash,
                app.wave_banner_wave,
                &minimap,
                app.locale,
                app.confirm_quit,
                app.current_project.is_some(),
                app.confirm_close_project,
                app.pending_autosave_recovery.as_deref(),
            );
            if app.ui_scene_fingerprint() != ui_fingerprint_before {
                app.scene_dirty = true;
            }
            if let Some(kind) = actions.use_item {
                app.use_item(kind);
            }
            if let Some(i) = actions.select_weapon {
                app.select_weapon(i);
            }
            for action in &actions.hud_clicks {
                app.push_hud_event(action);
            }
            if actions.save {
                app.save();
            }
            if let Some(path) = actions.save_path {
                app.save_to(&path);
            }
            if actions.load {
                app.load(); // asynchrone : la scène est remplacée plus tard (cf. take_imported_dirty)
            }
            if let Some(path) = actions.load_path {
                app.load_from(&path);
            }
            if let Some(picked_path) = actions.open_project_path {
                // Accepte soit le manifeste (sélecteur générique « Ouvrir… »,
                // Sprint 3), soit directement le dossier racine du projet
                // (« Ouvrir un projet… », projets récents — Sprint 4).
                let picked = std::path::Path::new(&picked_path);
                let dir = if picked.file_name().and_then(|n| n.to_str())
                    == Some(crate::project::MANIFEST_FILE)
                {
                    picked.parent().unwrap_or(picked)
                } else {
                    picked
                };
                match app.open_project(dir) {
                    Ok(_) => {
                        if let Some(project) = &app.current_project {
                            editor.note_recent_project(&project.name, &project.root);
                        }
                    }
                    Err(e) => log::error!("Ouverture du projet échouée : {e}"),
                }
            }
            if let Some(req) = actions.create_project {
                match app.create_project(&req.location, &req.name, req.template) {
                    Ok(_) => {
                        if let Some(project) = &app.current_project {
                            editor.note_recent_project(&project.name, &project.root);
                        }
                    }
                    Err(e) => log::error!("Création du projet échouée : {e}"),
                }
            }
            if actions.close_project {
                app.request_close_project();
            }
            // Réponses à la modale « modifications non sauvegardées » de
            // fermeture de projet (Sprint 4) — mêmes noms que la modale de
            // Quitter, cf. plus bas.
            if actions.close_project_cancel {
                app.confirm_close_project = false;
            }
            if actions.close_project_discard {
                app.close_project();
            }
            if actions.close_project_save {
                if let Some(project) = app.current_project.clone() {
                    let path = project.main_scene_path.to_string_lossy().into_owned();
                    app.save_to(&path);
                    // `save_to` ne baisse `scene_dirty` que sur succès : en cas
                    // d'échec, on reste ouvert plutôt que de fermer en perdant
                    // la scène — même garde que `quit_save`.
                    if !app.scene_dirty {
                        app.close_project();
                    }
                } else {
                    app.confirm_close_project = false;
                }
            }
            if actions.duplicate_project {
                match app.duplicate_project() {
                    Ok(dst) => log::info!("Projet dupliqué dans {}", dst.display()),
                    Err(e) => log::error!("Duplication du projet échouée : {e}"),
                }
            }
            if actions.reveal_project_in_finder
                && let Some(project) = &app.current_project
            {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg("-R")
                        .arg(&project.root)
                        .spawn();
                }
                #[cfg(not(target_os = "macos"))]
                {
                    log::info!(
                        "Révéler dans le Finder n'est disponible que sur macOS ({})",
                        project.root.display()
                    );
                }
            }
            // Réponses à la modale de récupération après crash (Sprint 6).
            if actions.restore_autosave
                && let Some(path) = app.pending_autosave_recovery.take()
                && let Err(e) = app.restore_autosave(&path)
            {
                log::error!("Restauration de l'autosave échouée : {e}");
            }
            if actions.dismiss_autosave_recovery {
                app.pending_autosave_recovery = None;
            }
            if let Some(path) = actions.import {
                app.import_gltf(&path);
            }
            if let Some(kind) = actions.add {
                app.add_object(kind);
            }
            if let Some(i) = actions.delete {
                app.delete_object(i);
            }
            if actions.duplicate {
                app.duplicate_selected();
            }
            if actions.new_scene {
                app.new_scene();
            }
            if actions.load_demo {
                app.load_mobile_demo();
            }
            if actions.load_gameplay {
                app.load_gameplay_demo();
            }
            if actions.load_controller {
                app.load_controller_demo();
            }
            if actions.load_tower {
                app.load_tower_demo();
            }
            if actions.load_temple_run {
                app.load_temple_run_demo();
            }
            if actions.load_components_demo {
                app.load_components_demo();
            }
            if actions.load_ai_duel {
                app.load_zombies_demo();
            }
            if actions.load_mmorpg {
                app.load_mmorpg_demo();
            }
            if actions.load_roguelike {
                app.load_roguelike_demo();
            }
            if actions.load_brawl {
                app.load_brawl_demo();
            }
            if actions.load_boss {
                app.load_boss_demo();
            }
            if actions.load_escorte {
                app.load_escorte_demo();
            }
            if actions.load_survie {
                app.load_survie_demo();
            }
            if actions.restart {
                restart = true;
            }
            if let Some((url, name, class, room, objective)) = actions.connect_to_server {
                app.connect_to_server_as(&url, &name, class, &room, objective);
            }
            if actions.disconnect_from_server {
                app.disconnect_from_server();
            }
            // Serveur local (Sprint 7) : démarrer puis auto-connecter l'hôte
            // (7.4), avec les mêmes pseudo/classe/salon/mode que le bouton
            // « Se connecter » enverrait — sauf pseudo vide, auquel cas on
            // laisse l'utilisateur cliquer lui-même une fois renseigné.
            if actions.start_local_server {
                match editor.start_local_server() {
                    Ok(addr) => {
                        let url = format!("ws://{addr}");
                        let (url, name, class, room, objective) =
                            editor.multiplayer_connect_params(&url);
                        if name.trim().is_empty() {
                            log::info!(
                                "Serveur local démarré sur {addr} — renseigne un pseudo puis \
                                 clique ▶ Se connecter."
                            );
                        } else {
                            app.connect_to_server_as(&url, &name, class, &room, objective);
                        }
                    }
                    Err(e) => log::error!("Démarrage du serveur local échoué : {e}"),
                }
            }
            if actions.stop_local_server {
                editor.stop_local_server();
                // Le serveur auquel on était peut-être connecté vient de
                // disparaître : la connexion cliente doit suivre, pas rester
                // affichée comme active vers un process mort.
                app.disconnect_from_server();
            }
            if let Some((email, password)) = actions.firebase_sign_in {
                let settings = editor.settings();
                app.request_firebase_sign_in(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    email,
                    password,
                );
            }
            if let Some((email, password)) = actions.firebase_sign_up {
                let settings = editor.settings();
                app.request_firebase_sign_up(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    email,
                    password,
                );
            }
            if let Some((lobby_code, sender_name, text)) = actions.send_chat_message {
                let settings = editor.settings();
                app.request_send_chat_message(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    lobby_code,
                    sender_name,
                    text,
                );
            }
            if let Some(lobby_code) = actions.refresh_chat {
                let settings = editor.settings();
                app.request_refresh_chat(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    lobby_code,
                );
            }
            if actions.refresh_leaderboard {
                let settings = editor.settings();
                app.request_refresh_leaderboard(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                    10,
                );
            }
            if actions.refresh_online_players {
                let settings = editor.settings();
                app.request_refresh_online_players(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                );
            }
            if actions.presence_heartbeat {
                let settings = editor.settings();
                app.request_presence_heartbeat(
                    settings.firebase_api_key.clone(),
                    settings.firebase_database_url.clone(),
                );
            }
            if actions.align_ground {
                app.align_to_ground();
            }
            if actions.reset_transform {
                app.reset_transform();
            }
            if let Some(scope) = actions.save_as_prefab {
                let result = app.save_selected_as_prefab(scope);
                if let Err(e) = &result {
                    log::warn!("Création du prefab impossible : {e}");
                }
                editor.set_prefab_feedback(result);
            }
            if let Some(asset_id) = actions.instantiate_prefab {
                app.instantiate_prefab(&asset_id);
            }
            if actions.sync_prefab_instances {
                app.sync_prefab_instances();
            }
            if let Some((scope, name)) = actions.delete_prefab {
                crate::assets::delete_prefab(&scope, &name);
            }
            if actions.quit {
                app.request_quit();
            }
            // Réponses à la modale « modifications non sauvegardées ».
            if actions.quit_cancel {
                app.confirm_quit = false;
            }
            if actions.quit_discard {
                app.confirm_quit = false;
                app.should_quit = true;
            }
            if actions.quit_save {
                app.confirm_quit = false;
                app.save();
                // `save` ne baisse `scene_dirty` que sur succès : en cas d'échec
                // (disque plein, chemin illisible…), on reste ouvert plutôt que de
                // quitter en perdant la scène — l'erreur est visible dans la console.
                if !app.scene_dirty {
                    app.should_quit = true;
                }
            }
            if actions.launch_glb_viewer {
                crate::editor::launch_glb_viewer();
            }
            if actions.undo {
                app.undo();
            }
            if actions.redo {
                app.redo();
            }
            if actions.step_frame {
                app.request_step();
            }
            if let Some(cmd) = actions.console_command {
                let result = app.run_console_command(&cmd);
                log::info!("> {cmd}\n{result}");
            }
            if let Some(clip) = actions.play_audio {
                app.play_audio(&clip);
            }
            if let Some(v) = actions.music_volume {
                app.set_music_volume(v);
            }
            if let Some(v) = actions.sfx_volume {
                app.set_sfx_volume(v);
            }
            if let Some(l) = actions.locale {
                app.set_locale(l);
            }
            if let Some(v) = actions.reduce_shake {
                app.set_reduce_shake(v);
            }
            if let Some(down) = actions.move_in_list {
                app.move_selected_in_list(down);
            }
            if let Some((from, to)) = actions.reorder {
                app.reorder_object(from, to);
            }
            if actions.focus_selection {
                app.frame_selected();
            }
            if let Some((idx, req)) = actions.ai_generate {
                app.request_ai_script(idx, req);
            }
            if let Some((req, replace)) = actions.ai_generate_scene {
                app.request_ai_scene(req, replace);
            }
            if actions.set_game_camera {
                app.set_game_camera();
            }
            if actions.clear_game_camera {
                app.clear_game_camera();
            }
            if let Some(max) = actions.optimize_textures {
                let n = app.optimize_textures(max);
                log::info!("Optimisation : {n} texture(s) réduite(s) à ≤ {max} px");
            }
            if let Some(max) = actions.limit_lights {
                app.limit_point_lights(max);
            }
            if actions.convert_textures_pot {
                let n = app.convert_textures_pot();
                log::info!("Convertisseur : {n} texture(s) en puissances de 2");
            }
            if actions.bake_lighting {
                let n = app.bake_lighting();
                log::info!("Bake lighting : {n} lumière(s) ponctuelle(s) figée(s) en émission");
            }
            if actions.perf_mode {
                let t = app.optimize_textures(1024);
                app.limit_point_lights(4);
                log::info!("Mode performance Android : {t} texture(s) réduite(s), ≤ 4 lumières");
            }
            if let Some(preset) = actions.apply_quality_preset {
                app.apply_quality_preset(preset);
                log::info!("Préset qualité appliqué : {preset:?}");
            }
            if actions.collect_assets {
                let n = app.collect_assets();
                log::info!("Assets rassemblés : {n} chemin(s) → asset://");
            }
            if actions.cut {
                app.cut_selected();
            }
            if actions.copy {
                app.copy_selected();
            }
            if actions.paste {
                app.paste();
            }
            if actions.select_all {
                app.select_all();
            }
            if actions.group {
                app.group_selected();
            }
            if actions.ungroup {
                app.ungroup_selected();
            }
            if let Some(axis) = actions.align_axis {
                app.align_selection_axis(axis);
            }
            if let Some(axis) = actions.distribute_axis {
                app.distribute_selection_axis(axis);
            }
            if actions.toggle_grid {
                app.show_grid = !app.show_grid;
            }
            if actions.toggle_snap {
                app.snap = !app.snap;
            }
            if let Some(view) = actions.set_debug_view {
                app.debug_view = view;
            }
            Some(full_output)
        };

        if let Some(actions) = player_net_actions {
            if let Some((url, name, class, room, objective)) = actions.connect_to_server {
                app.connect_to_server_as(&url, &name, class, &room, objective);
            }
            if actions.disconnect_from_server {
                app.disconnect_from_server();
            }
        }

        // Bouton de fin de partie : « Niveau suivant » uniquement pour la démo contrôleur
        // à niveaux ; sinon « Rejouer » — y compris une victoire par manches (zombies) ou
        // par ligne d'arrivée (course infinie/tour), qui doivent juste relancer la scène.
        if restart {
            if app.has_won() && app.is_leveled_demo {
                app.next_level();
            } else {
                app.restart_game();
            }
        }
        // « Reprendre » du menu pause (Phase J) : lève la pause sans autre effet
        // de bord — `restart` gère déjà son propre cas ci-dessus.
        if resume {
            app.paused = false;
        }

        // 2. Comportements (Play), sync GPU, push des uniforms.
        // Chronométré pour le bilan de perf périodique (cf. `log_perf_window`) :
        // départage les à-coups côté simulation (scripts/physique/réseau) des
        // à-coups côté rendu/présentation (le reste de la frame).
        let sim_start = Instant::now();
        app.advance_play();
        app.note_sim_duration(sim_start.elapsed());
        // Une scène chargée en fond vient peut-être de remplacer l'actuelle :
        // reconstruire les meshes GPU importés depuis les nouvelles données.
        if app.take_imported_dirty() {
            self.imported_gpu.clear();
        }
        self.sync_objects(&app.scene);
        self.sync_imported(&app.scene);
        self.sync_textures(&app.scene);

        // Aperçu mobile : restreint la vue 3D à un écran de téléphone (letterbox).
        // L'aspect caméra doit suivre ce rectangle (sinon l'image serait étirée).
        let sw = self.config.width as f32;
        let sh = self.config.height as f32;
        let (dx, dy, dw, dh) = if app.device_preview {
            // Base : région centrale (hors panneaux) remontée par l'éditeur ; sinon plein écran.
            let (bx, by, bw, bh) = app.view_rect_px;
            let (bx, by, bw, bh) = if bw > 1.0 && bh > 1.0 {
                (bx, by, bw, bh)
            } else {
                (0.0, 0.0, sw, sh)
            };
            // Le viewport GPU est en Y depuis le haut, comme les coordonnées egui : pas d'inversion.
            let (rx, ry, rw, rh) = crate::app::device_rect(bw, bh, app.device_portrait);
            (bx + rx, by + ry, rw, rh)
        } else {
            (0.0, 0.0, sw, sh)
        };
        app.camera.aspect = dw / dh.max(1.0);
        self.write_uniforms(app);
        // Skinning GPU : joint_buf entièrement rempli AVANT la passe (comme
        // les lignes de debug ci-dessous) — `queue.write_buffer` n'est pas ordonné avec
        // les draw calls d'un encoder pas encore soumis, donc rien de tout ça ne peut
        // être fait entre deux `draw_indexed` de la passe principale plus bas.
        // Offsets dans `self.skinned_offsets_scratch` (tampon réutilisé, audit perf).
        self.prepare_skinned_draws(&app.scene);

        // Préparer les lignes du gizmo + marqueurs de lumières (jamais en player/aperçu mobile).
        let gizmo_count = if app.player || app.device_preview {
            0
        } else {
            let mut verts: Vec<GizmoVertex> = Vec::new();
            // Marqueur en croix 3D à chaque lumière ponctuelle, teinté par sa couleur.
            for pl in &app.scene.point_lights {
                let c = pl.position;
                let col = pl.color;
                let s = 0.3;
                for axis in 0..3 {
                    let d = axis_dir(axis) * s;
                    verts.push(GizmoVertex {
                        position: [c[0] - d.x, c[1] - d.y, c[2] - d.z],
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: [c[0] + d.x, c[1] + d.y, c[2] + d.z],
                        color: col,
                    });
                }
                // Spot : ligne depuis la lumière le long du cône (visualise l'orientation).
                if pl.spot_angle > 0.0 {
                    let dir = glam::Vec3::from_array(pl.spot_dir);
                    let dir = if dir.length_squared() > 1e-6 {
                        dir.normalize()
                    } else {
                        glam::Vec3::NEG_Y
                    };
                    let end = glam::Vec3::from_array(c) + dir * (pl.range * 0.4).max(0.5);
                    verts.push(GizmoVertex {
                        position: c,
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: end.to_array(),
                        color: col,
                    });
                }
            }
            // Marqueur cyan à la position de la caméra de jeu (si définie).
            if let Some(gc) = app.scene.game_camera {
                let pitch = gc.pitch.clamp(-1.54, 1.54);
                let eye = glam::Vec3::from_array(gc.target)
                    + glam::Vec3::new(
                        gc.distance * pitch.cos() * gc.yaw.sin(),
                        gc.distance * pitch.sin(),
                        gc.distance * pitch.cos() * gc.yaw.cos(),
                    );
                let col = [0.2, 0.85, 0.95];
                let s = 0.4;
                for axis in 0..3 {
                    let d = axis_dir(axis) * s;
                    verts.push(GizmoVertex {
                        position: [eye.x - d.x, eye.y - d.y, eye.z - d.z],
                        color: col,
                    });
                    verts.push(GizmoVertex {
                        position: [eye.x + d.x, eye.y + d.y, eye.z + d.z],
                        color: col,
                    });
                }
            }
            // Gizmo de translation d'une lumière sélectionnée (3 axes colorés).
            if let Some(li) = app.selected_light
                && !app.gizmo_mode.is_nav()
                && let Some(pl) = app.scene.point_lights.get(li)
            {
                let o = glam::Vec3::from_array(pl.position);
                let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
                for (axis, color) in colors.iter().enumerate() {
                    let end = o + axis_dir(axis) * GIZMO_LEN;
                    verts.push(GizmoVertex {
                        position: o.to_array(),
                        color: *color,
                    });
                    verts.push(GizmoVertex {
                        position: end.to_array(),
                        color: *color,
                    });
                }
            }
            // Gizmo de manipulation de l'objet sélectionné (aucun en outil de
            // navigation : Main/Orbite/Loupe n'éditent pas).
            if let Some(sel) = app.selection
                && !app.gizmo_mode.is_nav()
            {
                let o = app.scene.objects[sel].transform.position;
                let colors = [[0.9, 0.25, 0.25], [0.25, 0.9, 0.3], [0.3, 0.45, 1.0]];
                match app.gizmo_mode {
                    // Déplacer / Redimensionner : 3 segments d'axe.
                    GizmoMode::Translate | GizmoMode::Scale => {
                        for (axis, color) in colors.iter().enumerate() {
                            let end = o + axis_dir(axis) * GIZMO_LEN;
                            verts.push(GizmoVertex {
                                position: o.to_array(),
                                color: *color,
                            });
                            verts.push(GizmoVertex {
                                position: end.to_array(),
                                color: *color,
                            });
                        }
                    }
                    // Tourner : 3 anneaux (un par axe).
                    GizmoMode::Rotate => {
                        const N: usize = RING_SEGMENTS;
                        for (axis, color) in colors.iter().enumerate() {
                            let (u, w) = axis_basis(axis_dir(axis));
                            for j in 0..N {
                                let a0 = std::f32::consts::TAU * j as f32 / N as f32;
                                let a1 = std::f32::consts::TAU * (j + 1) as f32 / N as f32;
                                let p0 = o + (u * a0.cos() + w * a0.sin()) * GIZMO_LEN;
                                let p1 = o + (u * a1.cos() + w * a1.sin()) * GIZMO_LEN;
                                verts.push(GizmoVertex {
                                    position: p0.to_array(),
                                    color: *color,
                                });
                                verts.push(GizmoVertex {
                                    position: p1.to_array(),
                                    color: *color,
                                });
                            }
                        }
                    }
                    // Navigation (Main/Orbite/Loupe) : filtrée par le garde
                    // `is_nav()` ci-dessus, rien à dessiner.
                    GizmoMode::Pan | GizmoMode::Orbit | GizmoMode::Zoom => {}
                }
            }
            self.queue
                .write_buffer(&self.gizmo_vbuf, 0, bytemuck::cast_slice(&verts));
            verts.len() as u32
        };

        // Debug drawing : segments accumulés pendant la frame (picking,
        // gameplay), dessinés une fois puis vidés — jamais persistants d'une frame à
        // l'autre, contrairement aux gizmos de manipulation ci-dessus.
        let debug_count = {
            let verts: Vec<GizmoVertex> = app
                .debug_lines
                .iter()
                .flat_map(|&(a, b, color)| {
                    [
                        GizmoVertex {
                            position: a.to_array(),
                            color,
                        },
                        GizmoVertex {
                            position: b.to_array(),
                            color,
                        },
                    ]
                })
                .collect();
            app.debug_lines.clear();
            if !verts.is_empty() {
                self.ensure_debug_capacity(verts.len());
                self.queue
                    .write_buffer(&self.debug_vbuf, 0, bytemuck::cast_slice(&verts));
            }
            verts.len() as u32
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });

        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 0);
        }

        // Nombre de draw calls réellement émis par les passes ombre + scène (les
        // boucles ci-dessous l'incrémentent à chaque `draw_indexed`) — remplace
        // l'ancienne estimation `2 × (plan + plan skinné)`, qui surcomptait les
        // statiques (batchés en plages d'instances, pas un draw par objet).
        let mut scene_draw_calls: u32 = 0;

        // Passe d'ombre : profondeur de la scène depuis la lumière → carte d'ombre.
        {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(0, &self.camera_bind_group, &[]);
            spass.set_bind_group(1, &self.models_bind_group, &[]);
            // Passe d'ombre : rend les objets hors champ (pas de frustum culling), mais
            // **ignore les objets invisibles** (ex. pièce ramassée) pour ne pas laisser
            // d'ombre fantôme. Groupé par mesh, scindé en plages de visibles consécutifs.
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let mut j = i + 1;
                while j < plan.len() && plan[j].mesh == mi {
                    j += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    spass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    spass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut k = i;
                    while k < j {
                        if !objs[plan[k].obj].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < j && objs[plan[k].obj].visible {
                            k += 1;
                        }
                        spass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = j;
            }
            // Objets skinnés dans la carte d'ombre (audit du 17 juillet 2026) : pipeline
            // dédié profondeur seule + skinning, cf. `draw_skinned_shadows` — avant, la
            // passe d'ombre n'itérait que `draw_plan` et aucun objet skinné ne projetait
            // d'ombre.
            scene_draw_calls +=
                self.draw_skinned_shadows(&mut spass, &app.scene, &self.skinned_offsets_scratch);
        }
        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 1);
        }

        {
            // La passe principale dessine dans `hdr_view` (HDR_FORMAT),
            // pas directement dans `view` — `self.tonemap()` fait le dernier maillon
            // vers le format d'affichage, après cette passe. Si le MSAA est actif
            // (`msaa_color_view`), la passe dessine dans la cible multi-échantillonnée et
            // se résout vers `hdr_view` (`resolve_target`) — sinon comportement inchangé.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.msaa_color_view.as_ref().unwrap_or(&self.hdr_view),
                    resolve_target: self.msaa_color_view.as_ref().map(|_| &self.hdr_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Aperçu mobile : la scène ne se dessine que dans le rectangle « téléphone ».
            // (Le clear remplit toute la surface → bandes sombres autour = letterbox.)
            pass.set_viewport(dx, dy, dw, dh, 0.0, 1.0);
            pass.set_scissor_rect(dx as u32, dy as u32, dw as u32, dh as u32);

            // Ciel : dessiné en premier, derrière tout le reste.
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.draw(0..3, 0..1);

            // Grille de référence au sol (depth-testée), en mode édition uniquement.
            if app.show_grid && !app.player && !app.device_preview {
                pass.set_pipeline(&self.grid_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid_vbuf.slice(..));
                pass.draw(0..self.grid_count, 0..1);
            }

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &self.shadow_bind_group, &[]);
            pass.set_bind_group(1, &self.models_bind_group, &[]);

            // Rendu instancié : un draw par lot (mesh+texture), scindé en sous-plages
            // d'instances visibles consécutives (frustum culling).
            let plan = &self.draw_plan;
            let objs = &app.scene.objects;
            let mut i = 0;
            while i < plan.len() {
                let mi = plan[i].mesh;
                let tex_key = &objs[plan[i].obj].texture;
                let mut group_end = i + 1;
                while group_end < plan.len()
                    && plan[group_end].mesh == mi
                    && &objs[plan[group_end].obj].texture == tex_key
                {
                    group_end += 1;
                }
                if let Some(mesh) = self.resolve_mesh(mi) {
                    let tex = self
                        .textures
                        .get(tex_key)
                        .unwrap_or_else(|| &self.textures[""]);
                    pass.set_bind_group(3, tex, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    // Plages contiguës d'instances visibles dans le lot.
                    let mut k = i;
                    while k < group_end {
                        if !plan[k].visible {
                            k += 1;
                            continue;
                        }
                        let run = k;
                        while k < group_end && plan[k].visible {
                            k += 1;
                        }
                        pass.draw_indexed(0..mesh.num_indices, 0, run as u32..k as u32);
                        scene_draw_calls += 1;
                    }
                }
                i = group_end;
            }

            // Gizmo de l'objet sélectionné, par-dessus.
            if gizmo_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.gizmo_vbuf.slice(..));
                pass.draw(0..gizmo_count, 0..1);
            }

            // Debug drawing : même pipeline lignes, buffer dédié.
            if debug_count > 0 {
                pass.set_pipeline(&self.gizmo_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vbuf.slice(..));
                pass.draw(0..debug_count, 0..1);
            }

            // Objets skinnés : un draw individuel par objet, palettes déjà
            // envoyées au GPU par `prepare_skinned_draws` avant cette passe.
            scene_draw_calls +=
                self.draw_skinned_objects(&mut pass, &app.scene, &self.skinned_offsets_scratch);
        }
        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 2);
        }

        // Bloom : passes de seuil/downsample/upsample sautées entièrement
        // si désactivé (opt-out mobile, `RenderQuality::bloom_enabled`) — pas seulement
        // neutralisées côté shader, un vrai gain de perf sur le palier visé.
        let bloom_intensity = if app.bloom_enabled && app.render_quality.bloom_enabled() {
            app.scene.sky.bloom_intensity
        } else {
            0.0
        };
        if bloom_intensity > 0.0 {
            self.render_bloom(&mut encoder, &self.hdr_view, &self.bloom_mip_views);
        }
        // Tone mapping : HDR → `view` (format d'affichage réel), avant l'UI
        // (l'UI egui reste en LDR, peinte par-dessus l'image déjà tonemappée).
        self.tonemap(
            &mut encoder,
            &self.hdr_view,
            &self.bloom_mip_views[0],
            bloom_intensity,
            &view,
        );
        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 3);
        }

        // 3. Peindre l'UI egui par-dessus la scène (sauf en mode player).
        let extra = match full_output {
            Some(output) => editor.paint(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                [self.config.width, self.config.height],
                output,
            ),
            None => Vec::new(),
        };
        self.editor = Some(editor);

        // Nombre de draw calls des passes ombre + scène (cf. doc de
        // `last_frame_draw_calls`, bloom/tonemap/UI/ciel/grille/gizmos ajoutent
        // quelques draws fixes non comptés ici) : compté sur les `draw_indexed`
        // réellement émis — l'ancienne estimation `2 × (plan + plan skinné)`
        // surcomptait les statiques (batchés) et devinait au lieu de mesurer.
        self.last_frame_draw_calls = scene_draw_calls;

        if gpu_profiling && let Some(prof) = self.gpu_profiler.as_ref() {
            encoder.write_timestamp(&prof.query_set, 4);
            encoder.resolve_query_set(&prof.query_set, 0..GPU_PROFILER_MARKS, &prof.resolve_buf, 0);
            let buf_size = u64::from(GPU_PROFILER_MARKS) * 8;
            encoder.copy_buffer_to_buffer(&prof.resolve_buf, 0, &prof.readback_buf, 0, buf_size);
        }

        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        if gpu_profiling {
            self.read_gpu_pass_timings();
        }
        frame.present();
    }
}
