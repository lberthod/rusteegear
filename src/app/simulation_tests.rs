    use super::super::PlayerInput;
    use super::*;
    use crate::scene::SceneObject;

    #[test]
    fn rotate_towards_smooth_eases_toward_the_target_the_short_way() {
        // Progresse vers la cible sans jamais la dépasser (amorti, pas d'oscillation).
        let r = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 60.0);
        assert!(r > 0.0 && r < 1.0, "r={r}");
        // De 3.0 vers -3.0 : le chemin direct (-6.0 rad) est plus long que par le
        // « dos » du cercle (~0.28 rad) — ne doit jamais tourner du mauvais côté.
        let r = rotate_towards_smooth(3.0, -3.0, 10.0, 1.0 / 60.0);
        assert!(r > 3.0, "doit passer par le dos du cercle (r={r})");
        // Ease-out : le pas suivant, plus proche de la cible, est plus petit — la
        // rotation ralentit à l'approche au lieu de « claquer » à vitesse constante.
        let first = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 60.0);
        let second = rotate_towards_smooth(first, 1.0, 10.0, 1.0 / 60.0) - first;
        assert!(
            second < first,
            "le pas doit décroître (1er={first}, 2e={second})"
        );
    }

    /// PHASE I Sprint 1 (accessibilité §16.6) : `reduce_shake` coupe le recul
    /// caméra à zéro même avec `camera_shake` au pic, sans le remettre à zéro
    /// lui-même (d'autres systèmes, ex. le flash de dégâts, en dépendent
    /// indépendamment — cf. la doc de `camera_shake_offset`).
    #[test]
    fn camera_shake_offset_is_zero_when_reduce_shake_is_set() {
        let mut app = AppState::new();
        app.camera_shake = 1.0;
        app.reduce_shake = false;
        // `t=0` annulerait le jitter sinusoïdal indépendamment de `reduce_shake`
        // (sin(0) = 0) — un instant non nul isole bien la cause testée ici.
        app.time = 1.0;
        assert_ne!(app.camera_shake_offset(), Vec3::ZERO);

        app.reduce_shake = true;
        assert_eq!(app.camera_shake_offset(), Vec3::ZERO);
        assert_eq!(
            app.camera_shake, 1.0,
            "reduce_shake ne doit pas muter camera_shake"
        );
    }

    #[test]
    fn rotate_towards_smooth_is_framerate_independent() {
        // Deux pas de dt/2 doivent donner (quasi) le même angle qu'un pas de dt :
        // le lissage ne doit pas dépendre de la cadence de rendu/simulation.
        let one_step = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 30.0);
        let half = rotate_towards_smooth(0.0, 1.0, 10.0, 1.0 / 60.0);
        let two_steps = rotate_towards_smooth(half, 1.0, 10.0, 1.0 / 60.0);
        assert!(
            (one_step - two_steps).abs() < 1e-4,
            "1 pas de dt ({one_step}) doit égaler 2 pas de dt/2 ({two_steps})"
        );
    }

    #[test]
    fn fly_cam_moves_the_orbit_target_forward_and_up_while_editing() {
        // Caméra libre : flèche haut + Espace doivent avancer ET monter, sans
        // toucher à rien hors Play (`update_fly_cam` n'est appelé que si
        // `!playing && fly_cam`, cf. `advance_play`).
        let mut app = AppState::new();
        app.fly_cam = true;
        app.camera.yaw = 0.0;
        let before = app.camera.target;
        app.input_state.key_move = (0.0, 1.0);
        app.input_state.fly_vertical = 1.0;
        app.update_fly_cam(1.0 / 60.0);
        let after = app.camera.target;
        assert!(
            after.z < before.z,
            "flèche haut doit avancer (yaw=0 pointe vers -Z)"
        );
        assert!(after.y > before.y, "Espace doit faire monter la caméra");
    }

    #[test]
    fn toggle_fly_cam_is_a_no_op_while_playing() {
        // La caméra libre est un outil d'édition : `G` ne doit rien faire en Play,
        // sinon la caméra de jeu et la caméra libre se battraient pour `camera.target`.
        let mut app = AppState::new();
        app.playing = true;
        app.toggle_fly_cam();
        assert!(!app.fly_cam, "toggle_fly_cam doit être un no-op en Play");
    }

    #[test]
    fn entering_play_turns_off_fly_cam() {
        // Repasser en Play doit désactiver la caméra libre laissée active en
        // éditeur, sinon `update_fly_cam` et la caméra de suivi du joueur se
        // disputeraient `camera.target` (cf. `advance_play`).
        let mut app = AppState::new();
        app.fly_cam = true;
        app.playing = true;
        app.advance_play();
        assert!(
            !app.fly_cam,
            "advance_play doit désactiver fly_cam à l'entrée en Play"
        );
    }

    #[test]
    fn hand_tool_pan_is_not_snapped_back_by_the_player_follow_cam() {
        // Sprint : sans la garde `!self.gizmo_mode.is_nav()`, `advance_play`
        // écrasait chaque frame le pan appliqué par l'outil 🖐 Main (Q), rendant
        // la caméra impossible à déplacer en mode Play (cf. le rattrapage
        // exponentiel de `camera.target` sur le joueur ci-dessus).
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        app.scene.camera_follow = true;
        app.playing = true;
        // Première frame de Play : consomme le cadrage initial sur le joueur
        // (cf. plus haut, hors de la garde testée ici) avant de simuler le pan.
        app.advance_play();
        app.gizmo_mode = crate::app::GizmoMode::Pan;
        // Simule le pan que `PickingController::handle_input` applique en glissant
        // avec l'outil Main actif.
        app.camera.pan(50.0, 0.0);
        let panned_target = app.camera.target;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(
            app.camera.target, panned_target,
            "la caméra de suivi ne doit pas re-cibler le joueur pendant un pan à l'outil Main"
        );
    }

    #[test]
    fn orbit_tool_yaw_is_not_pulled_back_towards_a_ranged_players_facing() {
        // Même classe de bug que le pan (cf. le test ci-dessus), pour l'outil 🔄
        // Orbite : un personnage équipé d'une arme à distance fait pivoter la
        // caméra vers son orientation de tir chaque frame (`rotate_towards_smooth`
        // ci-dessus) — sans la garde `!self.gizmo_mode.is_nav()`, ce rattrapage
        // écraserait l'orbite manuelle de l'utilisateur.
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(crate::scene::Controller {
                input: true,
                fire_button: "Feu".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        app.scene.camera_follow = true;
        app.playing = true;
        app.advance_play();
        app.gizmo_mode = crate::app::GizmoMode::Orbit;
        // Simule l'orbite manuelle que `PickingController::handle_input` applique
        // en glissant avec l'outil Orbite actif.
        app.camera.orbit(300.0, 0.0);
        let orbited_yaw = app.camera.yaw;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(
            app.camera.yaw, orbited_yaw,
            "la caméra de suivi ne doit pas re-pivoter vers le joueur pendant une orbite manuelle"
        );
    }

    #[test]
    fn entering_play_clears_a_selection_left_over_from_the_editor() {
        // Une sélection/gizmo laissé actif depuis l'éditeur resterait cliquable en
        // Play et modifierait `transform` en concurrence avec la physique qui
        // pilote désormais le même objet (cf. `clear_selection` à la sortie de
        // Play, symétrique).
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Caisse".into(),
            mesh: crate::scene::MeshKind::Cube,
            ..Default::default()
        });
        app.select_single(0);
        assert!(app.selection.is_some());
        app.playing = true;
        app.advance_play();
        assert!(
            app.selection.is_none(),
            "advance_play doit vider la sélection éditeur à l'entrée en Play"
        );
    }

    #[test]
    fn player_input_combines_keyboard_and_touch_tank_axes() {
        // Le pavé tactile W/A/S/D et le clavier alimentent les mêmes axes « tank »
        // sans s'écraser : cumulés, bornés à [-1, 1].
        let inp = PlayerInput {
            key_thrust: 1.0,
            touch_thrust: 1.0,
            key_turn: -1.0,
            touch_turn: 0.5,
            ..Default::default()
        };
        assert_eq!(inp.thrust(), 1.0, "le cumul doit rester borné à 1");
        assert!((inp.turn() - -0.5).abs() < 1e-6, "les sources se cumulent");
        let touch_only = PlayerInput {
            touch_thrust: -1.0,
            touch_turn: 1.0,
            ..Default::default()
        };
        assert_eq!(touch_only.thrust(), -1.0, "le pavé seul suffit (APK)");
        assert_eq!(touch_only.turn(), 1.0);
    }

    #[test]
    fn camera_relative_move_matches_world_axes_at_zero_yaw() {
        // yaw=0 : comportement d'origine inchangé (droite=+X, haut=-Z), sinon tout
        // déplacement solo/existant tournerait sans qu'aucune caméra n'ait bougé.
        let (wx, wz) = camera_relative_move(1.0, 0.0, 0.0);
        assert!((wx - 1.0).abs() < 1e-5 && wz.abs() < 1e-5);
        let (wx, wz) = camera_relative_move(0.0, 1.0, 0.0);
        assert!(wx.abs() < 1e-5 && (wz - -1.0).abs() < 1e-5);
    }

    #[test]
    fn apply_deadzone_zeroes_a_residual_stick_reading() {
        // Un joystick qui ne revient pas exactement au centre au repos ne doit pas
        // faire dériver le personnage.
        let (mx, my) = apply_deadzone((0.05, 0.02), JOYSTICK_DEADZONE);
        assert!(mx.abs() < 1e-6 && my.abs() < 1e-6);
    }

    #[test]
    fn apply_deadzone_preserves_direction_and_full_push() {
        // Poussée franche : direction conservée, plein débattement (longueur 1) intact.
        let (mx, my) = apply_deadzone((1.0, 0.0), JOYSTICK_DEADZONE);
        assert!((mx - 1.0).abs() < 1e-5 && my.abs() < 1e-6);
        let (mx, my) = apply_deadzone((0.5, 0.3), JOYSTICK_DEADZONE);
        // Remappée (donc un peu plus courte que l'entrée brute) mais même direction.
        assert!(mx > 0.0 && my > 0.0, "même quadrant que l'entrée");
        assert!((my / mx - 0.3 / 0.5).abs() < 1e-5, "direction conservée");
        let len = (mx * mx + my * my).sqrt();
        assert!(len > 0.0 && len < (0.5f32 * 0.5 + 0.3 * 0.3).sqrt());
    }

    #[test]
    fn apply_deadzone_starts_from_zero_at_the_edge_of_the_deadzone() {
        // Continuité au bord du rayon mort : juste au-dessus du seuil, l'entrée doit
        // être quasi nulle (départ progressif), pas sauter d'un coup à ~0.15 — le
        // « cran » perceptible que le remappage supprime.
        let (mx, my) = apply_deadzone((JOYSTICK_DEADZONE + 0.01, 0.0), JOYSTICK_DEADZONE);
        let len = (mx * mx + my * my).sqrt();
        assert!(
            len < 0.05,
            "l'entrée doit démarrer près de zéro au bord du rayon mort (len={len})"
        );
    }

    #[test]
    fn blend_render_poses_interpolates_between_the_last_two_sim_steps() {
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        // Delta de 0,1 m par pas (6 m/s : un déplacement normal, sous le seuil de
        // téléportation) : à mi-accumulateur, le rendu doit être à mi-chemin.
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(Vec3::new(0.1, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.5);
        let p = app.scene.objects[0].transform.position;
        assert!(
            (p.x - 0.05).abs() < 1e-6,
            "à mi-accumulateur, le rendu doit afficher la pose à mi-chemin (x={})",
            p.x
        );
    }

    /// Audit du 16 juillet 2026 : le bilan de perf périodique doit retenir la
    /// **pire** frame de la fenêtre (c'est elle qui fait les à-coups), ignorer
    /// les `dt` aberrants (throttle/veille), repartir de zéro à chaque fenêtre
    /// écoulée, et ne rien accumuler hors Play (frames volontairement throttlées).
    #[test]
    fn the_perf_log_window_tracks_the_worst_frame_and_resets_each_window() {
        let mut app = AppState::new();
        app.playing = true;
        let t0 = Instant::now();
        app.perf_window_start = t0;

        let d = std::time::Duration::from_secs;
        app.log_perf_window(t0 + d(1), 1.0 / 60.0);
        app.log_perf_window(t0 + d(2), 0.050); // à-coup réel : doit être retenu
        app.log_perf_window(t0 + d(3), 1.0 / 60.0);
        assert!(
            (app.perf_window_worst_dt - 0.050).abs() < 1e-6,
            "la pire frame de la fenêtre doit être retenue (worst={})",
            app.perf_window_worst_dt
        );
        // dt aberrant (> 0,5 s : throttle, mise en veille) : ignoré.
        app.log_perf_window(t0 + d(4), 2.0);
        assert!(
            (app.perf_window_worst_dt - 0.050).abs() < 1e-6,
            "un dt aberrant ne doit pas polluer la pire frame"
        );
        // Fenêtre écoulée : bilan flushé, la suivante repart de zéro.
        app.log_perf_window(t0 + d(11), 1.0 / 60.0);
        assert_eq!(
            app.perf_window_worst_dt, 0.0,
            "la fenêtre doit repartir de zéro après le bilan"
        );
        // Hors Play : rien n'est accumulé (les frames sont throttlées exprès).
        app.playing = false;
        app.log_perf_window(t0 + d(12), 0.2);
        assert_eq!(
            app.perf_window_worst_dt, 0.0,
            "hors Play, aucune frame ne doit être comptée"
        );
    }

    #[test]
    fn blend_render_poses_snaps_on_teleport_instead_of_streaking() {
        // Une téléportation (respawn, ancre FX déplacée sur sa cible) ne doit pas être
        // interpolée : le rendu claque directement sur la pose finale, sans traînée.
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        let target = Vec3::new(5.0, 0.5, -3.0);
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(target, Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.5);
        assert!(
            (app.scene.objects[0].transform.position - target).length() < 1e-6,
            "au-delà du seuil de téléportation, la pose finale doit être affichée telle quelle"
        );
    }

    #[test]
    fn restore_sim_poses_undoes_the_visual_blend_before_simulating() {
        // La pose affichée (mélangée) ne doit jamais servir d'état de départ à la
        // simulation : `restore_sim_poses` doit rétablir la pose exacte du dernier pas.
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        let curr = Vec3::new(0.2, 0.0, -0.1);
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(curr, Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.25);
        assert!((app.scene.objects[0].transform.position - curr * 0.25).length() < 1e-6);
        app.restore_sim_poses();
        assert!(
            (app.scene.objects[0].transform.position - curr).length() < 1e-6,
            "la pose de simulation exacte doit être rétablie avant le pas suivant"
        );
    }

    #[test]
    fn restore_sim_poses_respects_an_external_transform_write() {
        // Une écriture externe du transform (réconciliation réseau, test, futur gizmo
        // en Play) entre deux frames ne doit pas être annulée par la restauration :
        // c'est une intention, pas un artefact de mélange.
        let mut app = AppState::new();
        let n = app.scene.objects.len();
        app.sim_prev_poses = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
        app.sim_curr_poses = vec![(Vec3::new(0.1, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE); n];
        app.blend_render_poses(0.5);
        let moved = Vec3::new(50.0, 0.5, 50.0);
        app.scene.objects[0].transform.position = moved;
        app.restore_sim_poses();
        assert!(
            (app.scene.objects[0].transform.position - moved).length() < 1e-6,
            "une pose écrite de l'extérieur doit survivre à la restauration"
        );
        // Un objet non touché, lui, est bien restauré sur la pose de simulation.
        if n > 1 {
            assert!((app.scene.objects[1].transform.position.x - 0.1).abs() < 1e-6);
        }
    }

    #[test]
    fn blend_render_poses_is_a_no_op_without_matching_snapshots() {
        // Début de Play (instantanés vides) ou objet ajouté en cours de partie :
        // le mélange ne doit pas écrire des poses obsolètes dans la scène.
        let mut app = AppState::new();
        let before = app.scene.objects[0].transform.position;
        app.blend_render_poses(0.5);
        assert_eq!(app.scene.objects[0].transform.position, before);
    }

    #[test]
    fn clamp_move_vector_leaves_a_single_axis_unchanged() {
        let (mx, my) = clamp_move_vector(1.0, 0.0);
        assert!((mx - 1.0).abs() < 1e-6 && my.abs() < 1e-6);
    }

    #[test]
    fn clamp_move_vector_normalizes_a_diagonal_to_unit_length() {
        // Avant le correctif : (1.0, 1.0) restait tel quel (clamp par axe), donnant
        // une longueur √2 — un déplacement en diagonale ~41 % plus rapide qu'en
        // ligne droite. Le vecteur doit maintenant être ramené à une longueur de 1.
        let (mx, my) = clamp_move_vector(1.0, 1.0);
        let len = (mx * mx + my * my).sqrt();
        assert!((len - 1.0).abs() < 1e-5, "longueur={len}");
        // Toujours dans la même direction (diagonale), pas juste raccourci n'importe où.
        assert!((mx - my).abs() < 1e-6);
    }

    #[test]
    fn clamp_move_vector_never_amplifies_a_short_vector() {
        // Un joystick à mi-course (longueur < 1) ne doit pas être gonflé à 1 —
        // seuls les vecteurs qui dépassent 1 sont ramenés à cette longueur.
        let (mx, my) = clamp_move_vector(0.3, 0.0);
        assert!((mx - 0.3).abs() < 1e-6 && my.abs() < 1e-6);
    }

    #[test]
    fn camera_relative_move_rotates_forward_with_the_camera() {
        // À 90° (caméra tournée d'un quart de tour), « avancer » (my=1) ne doit
        // plus pointer vers -Z mais vers -X : le joystick doit suivre la caméra,
        // pas rester bloqué sur les axes du monde (façon caméra de suivi à la Zelda).
        let (wx, wz) = camera_relative_move(0.0, 1.0, std::f32::consts::FRAC_PI_2);
        assert!((wx - -1.0).abs() < 1e-4, "wx={wx}");
        assert!(wz.abs() < 1e-4, "wz={wz}");
    }

    #[test]
    fn creature_is_server_synced_stays_false_without_any_snapshot_ever_received() {
        // Room jointe sans succès côté serveur (gabarit introuvable) ou scène
        // désynchronisée : la créature n'a jamais reçu le moindre `Snapshot`. Sans
        // filet, elle resterait figée pour toujours — la synchro doit rester
        // fausse (et donc le script local continuer de tourner) tant qu'aucune
        // mise à jour n'est jamais arrivée, peu importe le timeout.
        let now = Instant::now();
        let timeout = std::time::Duration::from_millis(2500);
        assert!(!creature_is_server_synced(None, now, timeout));
    }

    #[test]
    fn creature_is_server_synced_true_right_after_a_fresh_snapshot() {
        let now = Instant::now();
        let timeout = std::time::Duration::from_millis(2500);
        assert!(creature_is_server_synced(Some(now), now, timeout));
    }

    #[test]
    fn creature_is_server_synced_resumes_local_simulation_once_snapshots_go_stale() {
        // Le serveur diffusait, puis s'arrête (déconnexion silencieuse,
        // redémarrage) : passé le délai de grâce, on ne doit plus considérer la
        // créature comme synchronisée — sinon elle resterait figée à sa dernière
        // position serveur pour toujours au lieu de reprendre son script local.
        let last_snapshot = Instant::now();
        let timeout = std::time::Duration::from_millis(2500);
        let still_fresh = last_snapshot + std::time::Duration::from_millis(2400);
        let now_stale = last_snapshot + std::time::Duration::from_millis(2600);
        assert!(creature_is_server_synced(
            Some(last_snapshot),
            still_fresh,
            timeout
        ));
        assert!(!creature_is_server_synced(
            Some(last_snapshot),
            now_stale,
            timeout
        ));
    }

    #[test]
    fn fixed_substeps_is_framerate_independent() {
        let fixed = 1.0 / 60.0;
        // 60 FPS : 1 frame = 1 pas, reliquat ~0.
        let (n, acc) = fixed_substeps(0.0, fixed, fixed, 5);
        assert_eq!(n, 1);
        assert!(acc.abs() < 1e-6);
        // 30 FPS : une frame longue = 2 pas fixes (rattrapage).
        let (n, _) = fixed_substeps(0.0, 1.0 / 30.0, fixed, 5);
        assert_eq!(n, 2);
        // 120 FPS : frame trop courte → 0 pas, le temps s'accumule.
        let (n, acc) = fixed_substeps(0.0, 1.0 / 120.0, fixed, 5);
        assert_eq!(n, 0);
        assert!(acc > 0.0);
        // Deux frames à 120 FPS finissent par produire un pas.
        let (n2, _) = fixed_substeps(acc, 1.0 / 120.0, fixed, 5);
        assert_eq!(n2, 1);
        // Gel long : borné par le cap (pas de spirale), accumulateur remis à 0.
        let (n, acc) = fixed_substeps(0.0, 5.0, fixed, 5);
        assert_eq!(n, 5);
        assert_eq!(acc, 0.0);
    }

    #[test]
    fn step_requested_advances_exactly_one_fixed_tick_while_paused() {
        // Le bouton « ⏭ » doit avancer d'exactement un pas fixe en pause,
        // ni plus (pas de rattrapage), ni moins (pas d'attente supplémentaire), puis
        // regeler la simulation tant qu'aucune nouvelle demande n'arrive.
        let mut app = AppState::new();
        app.playing = true;
        app.paused = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play(); // transition Edit→Play + première frame gelée
        assert_eq!(
            app.time, 0.0,
            "en pause sans demande, le temps ne doit pas avancer"
        );

        app.request_step();
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let fixed_dt = 1.0 / 60.0;
        assert!(
            (app.time - fixed_dt).abs() < 1e-5,
            "un seul pas fixe attendu : time={}, attendu≈{fixed_dt}",
            app.time
        );

        // Sans nouvelle demande, la pause suivante ne doit pas avancer davantage.
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert!(
            (app.time - fixed_dt).abs() < 1e-5,
            "sans nouvelle demande, le temps ne doit plus avancer : time={}",
            app.time
        );
    }

    #[test]
    fn sim_step_advances_animation_time_scaled_by_speed() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            animation: Some(crate::scene::AnimationState {
                clip: "Run".into(),
                time: 0.0,
                speed: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        });
        app.sim_step(0.1);
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert!(
            (anim.time - 0.2).abs() < 1e-6,
            "0.1s à vitesse 2x doit avancer time de 0.2s, obtenu {}",
            anim.time
        );
    }

    /// Hors Play, les clips squelettaux tournent quand même (prévisualisation
    /// d'édition) : `advance_play` avance `anim.time` au dt de frame même quand
    /// `playing == false` — sans ça, tout GLB riggé reste figé en pose de liaison
    /// (T-pose) dans la vue d'édition tant qu'on ne lance pas Play.
    #[test]
    fn edit_mode_still_advances_skeletal_clips() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            animation: Some(crate::scene::AnimationState {
                clip: "Idle".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(!app.playing, "AppState démarre en mode édition");
        // Simule ~50 ms écoulés depuis la frame précédente (l'horloge réelle
        // d'`advance_play` ne verrait que quelques µs entre deux appels de test).
        app.last_frame = Instant::now() - std::time::Duration::from_millis(50);
        app.advance_play();
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert!(
            anim.time >= 0.04,
            "~50 ms hors Play doivent avancer la lecture d'autant, obtenu {}",
            anim.time
        );
    }

    #[test]
    fn sim_step_leaves_objects_without_animation_untouched() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject::default());
        app.sim_step(0.1);
        assert!(app.scene.objects[0].animation.is_none());
    }

    /// Preuve jouable : la « créature » (assets/models/creature.glb, rig
    /// Root/Body/Head/LegL/LegR exporté depuis Blender via le connecteur MCP, clips
    /// `Idle`/`Walk`) se déplace réellement via un script Lua de wander, pas seulement
    /// en apparence (animation qui tourne sans que `transform.position` bouge). Le
    /// script alterne 3s de marche (`obj.anim = "Walk"`, position qui dérive en cercle)
    /// puis 1s d'arrêt (`obj.anim = "Idle"`, position figée) — mêmes mécanismes que
    /// `AiChaser`/`Combat` (Lua pilote `obj.x/z` et `obj.anim`, lus par `run_script` en
    /// fin d'appel), mais en patrouille scriptée plutôt qu'en poursuite du joueur (cf.
    /// la doc de `AiChaser` sur cette distinction).
    #[test]
    fn scripted_creature_wanders_then_idles_using_the_imported_walk_and_idle_clips() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/models/creature.glb");
        let (data, aabb_min, aabb_max) =
            crate::scene::import::load_gltf(path).expect("creature.glb doit être un glTF valide");
        let mut imported = crate::scene::ImportedMesh {
            path: path.to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        imported.load_skinning();
        assert!(
            imported.skeleton.is_some(),
            "creature.glb doit être skinné (rig Blender exporté avec Export Skins)"
        );
        let clip_names: Vec<&str> = imported.clips.iter().map(|c| c.name.as_str()).collect();
        assert!(clip_names.contains(&"Idle") && clip_names.contains(&"Walk"));

        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.imported.clear();
        app.scene.imported.push(imported);
        app.scene.objects.push(SceneObject {
            mesh: crate::scene::MeshKind::Imported(0),
            animation: Some(crate::scene::AnimationState {
                clip: "Idle".into(),
                ..Default::default()
            }),
            script: r#"
                local t = time % 4.0
                if t < 3.0 then
                    obj.x = obj.x + math.sin(time * 1.5) * 0.6 * dt
                    obj.z = obj.z + math.cos(time * 1.5) * 0.6 * dt
                    obj.anim = "Walk"
                else
                    obj.anim = "Idle"
                end
            "#
            .into(),
            ..Default::default()
        });

        let dt = 1.0 / 60.0;
        for _ in 0..(3 * 60) {
            app.sim_step(dt);
        }
        let after_walk = app.scene.objects[0].transform.position;
        assert!(
            after_walk.distance(Vec3::ZERO) > 0.1,
            "après 3s de phase Walk, la créature doit s'être déplacée (position={after_walk:?})"
        );
        assert_eq!(
            app.scene.objects[0].animation.as_ref().unwrap().clip,
            "Walk"
        );

        for _ in 0..60 {
            app.sim_step(dt);
        }
        let after_idle = app.scene.objects[0].transform.position;
        assert!(
            (after_idle - after_walk).length() < 1e-5,
            "en phase Idle la position ne doit plus bouger : avant={after_walk:?}, après={after_idle:?}"
        );
        assert_eq!(
            app.scene.objects[0].animation.as_ref().unwrap().clip,
            "Idle"
        );
    }

    /// Preuve jouable, avec la **vraie** physique (raycasts réels contre les murs) :
    /// la créature de `Scene::mmorpg_demo` ne doit jamais rester collée contre un mur
    /// à jouer son animation « Walk » sans avancer. Bug observé en jeu (corrigé après
    /// cette preuve) : le déclenchement du virage anticipé (`near_edge`) et le clamp
    /// dur de fin de script comparaient tous deux `obj.x`/`obj.z` à la même borne
    /// (`BOUND`) — sans marge, un rayon manquant un mur en approche tangente laissait
    /// la créature dériver jusqu'au clamp, s'y faire plaquer chaque frame (jamais
    /// `> BOUND` une fois clampée, donc `near_edge` restait faux), et y rester bloquée
    /// en boucle d'animation. Ce test fait tourner ~30 s simulées du vrai script de
    /// production (`Scene::mmorpg_demo`, pas une version simplifiée) contre le vrai
    /// monde physique (`Physics::build`, murs/repères inclus) et échoue si la
    /// créature passe plus d'1 s d'affilée collée à moins de 20 cm d'une borne
    /// d'arène sans progresser.
    #[test]
    fn mmorpg_creature_never_gets_stuck_walking_into_a_wall() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature")
            .expect(
                "la démo MMORPG doit contenir une « Créature » (creature.glb chargé, \
                 cf. Scene::mmorpg_demo)",
            );
        for (name, glb) in [
            ("Créature 2", "creature2.glb"),
            ("Créature 3", "creature3.glb"),
            ("Créature 4", "creature4.glb"),
            ("Créature 5", "creature5.glb"),
            ("Créature 6", "creature6.glb"),
            ("Créature 7", "creature7.glb"),
            ("Créature 8", "creature8.glb"),
            ("Créature 9", "creature9.glb"),
            ("Créature 10", "creature10.glb"),
            ("Créature 11", "creature11.glb"),
            ("Créature 12", "creature12.glb"),
            ("Créature 13", "creature13.glb"),
            ("Créature 14", "creature14.glb"),
            ("Créature 15", "creature15.glb"),
            ("Créature 16", "creature16.glb"),
            ("Créature 17", "creature17.glb"),
            ("Créature 18", "creature18.glb"),
            ("Créature 19", "creature19.glb"),
            ("Créature 20", "creature20.glb"),
            ("Créature 21", "creature21.glb"),
            ("Créature 22", "creature22.glb"),
            ("Créature 23", "creature23.glb"),
            ("Créature 24", "creature24.glb"),
            ("Créature 25", "creature25.glb"),
            ("Créature 26", "creature26.glb"),
        ] {
            assert!(
                app.scene.objects.iter().any(|o| o.name == name),
                "la démo MMORPG doit aussi contenir la « {name} » ({glb}, généré \
                 sous Blender — cf. Scene::mmorpg_demo)"
            );
        }
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        // Bornes réelles de l'arène (cf. `Scene::MMORPG_HALF`) moins une petite
        // marge : au-delà, la créature est effectivement pressée contre un mur.
        let arena_limit = crate::scene::Scene::MMORPG_HALF - 0.6;
        let dt = 1.0 / 60.0;
        let mut pinned_frames = 0u32;
        let max_pinned_frames = 60; // 1 s d'affilée collée à un bord = bug
        let mut prev_yaw: Option<f32> = None;
        let mut prev_pos = app.scene.objects[idx].transform.position;
        let mut idle_frames = 0u32;

        for step in 0..(30 * 60) {
            app.sim_step(dt);
            let obj = &app.scene.objects[idx];
            let pos = obj.transform.position;
            assert!(
                pos.x.abs() <= arena_limit + 0.05 && pos.z.abs() <= arena_limit + 0.05,
                "step {step} : la créature est sortie de l'arène (position={pos:?})"
            );
            let pinned = pos.x.abs() > arena_limit - 0.2 || pos.z.abs() > arena_limit - 0.2;
            pinned_frames = if pinned { pinned_frames + 1 } else { 0 };
            assert!(
                pinned_frames <= max_pinned_frames,
                "step {step} : la créature semble bloquée contre un mur \
                 ({pinned_frames} frames d'affilée près d'un bord, position={pos:?})"
            );

            // Pas de pivot brusque d'une frame à l'autre : cf. la doc de
            // `creature_wander_script` (3ᵉ version) — un virage-cible instantané
            // donnait des demi-tours visibles d'une frame à l'autre.
            let (_, yaw, _) = obj.transform.rotation.to_euler(glam::EulerRot::XYZ);
            if let Some(prev) = prev_yaw {
                let mut delta = (yaw - prev).to_degrees();
                delta = ((delta + 180.0).rem_euclid(360.0)) - 180.0;
                assert!(
                    delta.abs() < 20.0,
                    "step {step} : virage brusque d'une frame à l'autre ({delta:.1}°) — \
                     devrait tourner progressivement, jamais faire un demi-tour instantané"
                );
            }
            prev_yaw = Some(yaw);

            if (pos - prev_pos).length() < 1e-4 {
                idle_frames += 1;
            }
            prev_pos = pos;
        }

        // Ne doit pas passer un temps disproportionné à l'arrêt (l'ancienne version
        // s'arrêtait 1 s sur 4 sur un minuteur fixe, plus l'arrêt en cours de virage) :
        // une patrouille naturelle marche la grande majorité du temps.
        let idle_ratio = idle_frames as f32 / (30.0 * 60.0);
        assert!(
            idle_ratio < 0.15,
            "la créature est restée immobile {:.0}% du temps (attendu < 15%) — \
             trop d'arrêts pour une patrouille censée avancer en continu",
            idle_ratio * 100.0
        );
    }

    /// Preuve dédiée à la Créature 13 (méduse, `creature_drift_script`) : une
    /// fois le Lac muré (murs d'eau invisibles, cf. `mmorpg_demo`), elle dérive
    /// dans un rayon local autour de son spawn plutôt que de viser le centre de
    /// l'arène — bug corrigé après une trace de 60 s qui la montrait plaquée
    /// contre le mur est du lac (`x≈-12`, jamais assez proche du centre de
    /// l'arène pour déclencher l'ancien rappel absolu). Même critère
    /// d'immobilité que le test générique ci-dessus, sur 60 s pour laisser le
    /// temps à plusieurs allers-retours dans le lac.
    #[test]
    fn mmorpg_creature_13_drifts_in_its_lake_without_getting_stuck() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature 13")
            .expect("la démo MMORPG doit contenir la « Créature 13 »");
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        let dt = 1.0 / 60.0;
        let mut idle_frames = 0u32;
        let mut prev_pos = app.scene.objects[idx].transform.position;
        const STEPS: u32 = 60 * 60;
        for _ in 0..STEPS {
            app.sim_step(dt);
            let pos = app.scene.objects[idx].transform.position;
            if (pos - prev_pos).length() < 1e-4 {
                idle_frames += 1;
            }
            prev_pos = pos;
        }
        let idle_ratio = idle_frames as f32 / STEPS as f32;
        assert!(
            idle_ratio < 0.15,
            "la Créature 13 est restée immobile {:.0}% du temps (attendu < 15%) — \
             probablement plaquée contre un mur d'eau",
            idle_ratio * 100.0
        );
    }

    /// Sprint 25 (Phase K, `sprintreflecion.md`) : la bande de collines à
    /// l'ouest (`gfx::mesh::MMORPG_HILL_STRIP_X_LOCAL`, x monde ∈[-36,-33])
    /// n'est pas qu'un décor visuel — un obstacle réel pour la sonde IA de
    /// patrouille. Ne réutilise pas juste une créature existante (elles
    /// évitent déjà la bande par construction, cf. `MMORPG_CREATURES`) : on
    /// reprend directement le vrai script de production
    /// (`scene::demos::creature_wander_script`, rendu accessible aux tests
    /// via `pub(crate)`) planté DANS le plateau (x=-35,25, pleine amplitude,
    /// cf. `mmorpg_terrain_local_height`), avec un cap plein nord (le long de
    /// la bande, pas perpendiculaire) : contrairement à l'axe X (rampe du
    /// plateau à 0 en seulement 0,5 m — bien trop raide pour un
    /// `KinematicCharacterController` de créature, qui n'a PAS d'`autostep`
    /// contrairement au joueur, cf. `resolve_scripted_moves`), le relief
    /// varie doucement le long de Z (fréquence bien plus basse dans
    /// `mmorpg_terrain_local_height`), ce qui laisse la patrouille suivre
    /// réellement la pente au lieu de buter dessus comme sur un mur.
    /// Vérifie : (1) jamais figée plus d'1 s d'affilée (même piège que
    /// documenté dans la mémoire projet : un obstacle non visible au
    /// raycast à 0,6 m fige la patrouille) pendant qu'elle chevauche la
    /// bande, et (2) sa hauteur `y` suit bien le relief sous elle — même
    /// gabarit de tolérance que
    /// `runtime::physics::tests::a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height`
    /// (`y` jamais bien en-dessous du sol attendu, jamais en lévitation
    /// franche au-dessus), et une variation de hauteur bien réelle sur le
    /// trajet — preuve qu'elle « négocie » la pente plutôt que de rester
    /// plaquée à une hauteur constante.
    #[test]
    fn mmorpg_creature_wander_crosses_the_west_hill_band_without_getting_stuck() {
        use crate::gfx::mesh::{MMORPG_HILL_STRIP_X_LOCAL, mmorpg_terrain_local_height};
        use crate::scene::demos::creature_wander_script;

        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let half = crate::scene::Scene::MMORPG_HALF;
        let idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature")
            .expect("la démo MMORPG doit contenir une « Créature »");

        // Bornes monde de la bande de collines (cf. `MMORPG_HILL_STRIP_X_LOCAL`,
        // en coordonnées locales ∈[-0.5,0.5] à reconvertir en mètres via ×72).
        let (band_x_lo, band_x_hi) = MMORPG_HILL_STRIP_X_LOCAL;
        let world_size = 2.0 * half;
        let band_x_lo_m = band_x_lo * world_size;
        let band_x_hi_m = band_x_hi * world_size;
        assert!(
            band_x_lo_m < -33.0 && band_x_hi_m > -35.6,
            "bande attendue autour de x∈[-36,-33] (obtenu [{band_x_lo_m},{band_x_hi_m}])"
        );

        {
            let obj = &mut app.scene.objects[idx];
            // x=-35.25 : centre du plateau à pleine amplitude (x∈[-35.5,-35.0]).
            // z=-25 : loin de la route (coupée à plat entre z=9 et 19) et loin
            // du mur nord (retombée à 0 dès |z|>33) — relief bien réel des deux
            // côtés du trajet (cf. le calcul de `mmorpg_terrain_local_height`
            // le long de cet axe). Cap plein nord (heading=0°, cf.
            // `creature_wander_script` : fwd = (sin(h), cos(h)), donc fwd=(0,1)
            // à 0°).
            // Départ tout près du relief attendu à ce point (calculé par la
            // même fonction que le sol) plutôt qu'à y=0 : sinon la créature
            // démarre enfoncée sous le relief (ici ~1,1 m) et toute la
            // patrouille se limiterait à remonter cette chute de rattrapage
            // au lieu de longer la bande — l'objectif ici est de mesurer la
            // variation de hauteur PENDANT la patrouille, pas pendant un
            // rattrapage de spawn (contrairement à
            // `a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height`,
            // qui mesure justement une chute).
            let spawn_x = -35.25_f32;
            let spawn_z = -25.0_f32;
            let spawn_h =
                mmorpg_terrain_local_height(spawn_x / (2.0 * half), spawn_z / (2.0 * half));
            obj.transform.position = Vec3::new(spawn_x, spawn_h + 0.05, spawn_z);
            let ray_mask = !obj.collision_layer;
            obj.script = creature_wander_script(half, "sprint25_hill_test_", ray_mask, 0.0, 0.0);
        }
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let dt = 1.0 / 60.0;

        const STEPS: u32 = 20 * 60;
        let mut prev_pos = app.scene.objects[idx].transform.position;
        let mut pinned_frames = 0u32;
        let mut entered_band = false;
        let mut min_y_in_band = f32::MAX;
        let mut max_y_in_band = f32::MIN;

        for step in 0..STEPS {
            app.sim_step(dt);
            let pos = app.scene.objects[idx].transform.position;

            if (pos - prev_pos).length() < 1e-4 {
                pinned_frames += 1;
            } else {
                pinned_frames = 0;
            }
            assert!(
                pinned_frames <= 60,
                "step {step} : la créature semble figée plus d'1 s d'affilée dans/près \
                 de la bande de collines (position={pos:?})"
            );

            if pos.x >= band_x_lo_m && pos.x <= band_x_hi_m {
                entered_band = true;
                let expected_h =
                    mmorpg_terrain_local_height(pos.x / world_size, pos.z / world_size);
                let y = pos.y;
                // Même gabarit de tolérance que
                // `a_dynamic_body_settles_on_the_terrain_hill_at_the_right_height` :
                // ni traversée du relief vers le bas, ni lévitation franche
                // au-dessus (un peu plus large ici pour la marche, pas une
                // chute libre qui se stabilise).
                assert!(
                    y > expected_h - 0.5,
                    "step {step} : la créature a traversé le relief vers le bas \
                     (y={y}, sol attendu ≈{expected_h}, position={pos:?})"
                );
                assert!(
                    y < expected_h + 1.5,
                    "step {step} : la créature lévite au-dessus du relief \
                     (y={y}, sol attendu ≈{expected_h}, position={pos:?})"
                );
                min_y_in_band = min_y_in_band.min(y);
                max_y_in_band = max_y_in_band.max(y);
            }

            prev_pos = pos;
        }

        assert!(
            entered_band,
            "la créature n'a jamais chevauché la bande de collines (x∈[{band_x_lo_m},\
             {band_x_hi_m}]) — le test ne prouve rien"
        );
        assert!(
            max_y_in_band - min_y_in_band > 0.3,
            "la créature a parcouru la bande sans que sa hauteur ne varie \
             (min={min_y_in_band}, max={max_y_in_band}) — suspect sur un relief \
             avec plusieurs dizaines de cm d'amplitude le long de ce trajet"
        );
    }

    /// Audit gameplay « gros sauts / déplacements illogiques » : preuve que les
    /// **20** créatures de la démo MMORPG bougent continûment, sans téléportation
    /// ni pivot brutal, avec la vraie physique. Bugs observés en jeu (corrigés
    /// après cette preuve) : le griffon (n°16) et le kraken (n°17) écrivaient
    /// leur position **en absolu** sur une courbe paramétrique — saut initial de
    /// tout le rayon au premier tick, et bond de rattrapage après chaque blocage
    /// (l'angle continuait d'avancer pendant que `resolve_scripted_moves`
    /// rabotait le déplacement) ; le félin (n°12) et la lanterne (n°19)
    /// claquaient vitesse et cap de 90-180° pile sur leurs seuils de distance ;
    /// l'escargot (n°14) se retournait de 180° en une frame en bout de navette.
    /// Chaque frame, pour chaque créature : déplacement horizontal ≤ vitesse max
    /// (3,2 m/s, la charge du ver) × dt × marge, et pivot ≤ 25° (sauf le ver n°18,
    /// dont le cap de charge « sous le sable » est un surgissement assumé).
    #[test]
    fn mmorpg_creatures_never_teleport_nor_snap_turn() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let creatures: Vec<usize> = app
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name.starts_with("Créature"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(creatures.len(), 26, "la démo doit garder ses 26 créatures");

        let dt = 1.0 / 60.0;
        // 3,2 m/s (charge du ver, la plus rapide) + marge : au-delà en une frame,
        // c'est une téléportation, pas un déplacement.
        let max_step = 3.2 * dt * 1.7;
        let mut prev: Vec<(glam::Vec3, Option<f32>)> = creatures
            .iter()
            .map(|&i| (app.scene.objects[i].transform.position, None))
            .collect();

        for step in 0..(20 * 60) {
            app.sim_step(dt);
            for (k, &i) in creatures.iter().enumerate() {
                let obj = &app.scene.objects[i];
                let pos = obj.transform.position;
                let (prev_pos, prev_yaw) = prev[k];
                let d_xz = (glam::Vec2::new(pos.x, pos.z)
                    - glam::Vec2::new(prev_pos.x, prev_pos.z))
                .length();
                assert!(
                    d_xz <= max_step,
                    "step {step} : « {} » a sauté de {d_xz:.3} m en une frame \
                     (max {max_step:.3}) — téléportation ({prev_pos:?} → {pos:?})",
                    obj.name
                );
                let (_, yaw, _) = obj.transform.rotation.to_euler(glam::EulerRot::XYZ);
                // Les 5 premières frames absorbent l'orientation initiale (un
                // script qui démarre pose son premier cap d'un coup, sans
                // historique — pas un défaut visible en jeu).
                if step >= 5
                    && obj.name != "Créature 18"
                    && let Some(py) = prev_yaw
                {
                    let mut delta = (yaw - py).to_degrees();
                    delta = ((delta + 180.0).rem_euclid(360.0)) - 180.0;
                    assert!(
                        delta.abs() < 25.0,
                        "step {step} : « {} » a pivoté de {delta:.1}° en une frame — \
                         demi-tour brutal",
                        obj.name
                    );
                }
                prev[k] = (pos, Some(yaw));
            }
        }
    }

    /// Preuve du correctif « les créatures partent toutes dans la même direction
    /// et restent collées au mur » : le script de patrouille était déterministe
    /// **et identique** pour toutes les instances (cap initial 0° pour toutes,
    /// bruit de méandre fonction du seul `time` global — cf. la doc de
    /// `creature_wander_script`, paramètres `heading0`/`phase`). Les 5 créatures
    /// avançaient en bloc vers +Z jusqu'au mur, où le braquage anti-mur, lui
    /// aussi identique, ne les décollait pas. Ce test rejoue 2 s du vrai script
    /// de production avec la vraie physique et échoue si les directions de
    /// déplacement des créatures restent groupées (écart angulaire maximal
    /// entre deux déplacements < 60°).
    #[test]
    fn mmorpg_creatures_do_not_all_walk_in_the_same_direction() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        // Créature 20 (tortue-canon) exclue : tourelle délibérément stationnaire
        // (`creature_turret_script`, cf. sa doc) — elle pivote sur place mais ne
        // se déplace jamais, ce qui est le comportement voulu, pas le bug que ce
        // test traque (des créatures *censées patrouiller* qui restent bloquées
        // ensemble contre un mur).
        let creature_indices: Vec<usize> = app
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name.starts_with("Créature") && o.name != "Créature 20")
            .map(|(i, _)| i)
            .collect();
        assert!(
            creature_indices.len() >= 2,
            "la démo MMORPG doit contenir plusieurs créatures (trouvé {})",
            creature_indices.len()
        );
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let spawns: Vec<Vec3> = creature_indices
            .iter()
            .map(|&i| app.scene.objects[i].transform.position)
            .collect();

        let dt = 1.0 / 60.0;
        for _ in 0..(2 * 60) {
            app.sim_step(dt);
        }

        // Direction de déplacement (XZ) de chaque créature depuis son spawn.
        let headings: Vec<glam::Vec2> = creature_indices
            .iter()
            .zip(&spawns)
            .map(|(&i, spawn)| {
                let pos = app.scene.objects[i].transform.position;
                let d = glam::Vec2::new(pos.x - spawn.x, pos.z - spawn.z);
                assert!(
                    d.length() > 0.3,
                    "après 2 s, « {} » doit s'être déplacée (déplacement={d:?})",
                    app.scene.objects[i].name
                );
                d.normalize()
            })
            .collect();

        let max_angle = headings
            .iter()
            .enumerate()
            .flat_map(|(a, ha)| headings[a + 1..].iter().map(move |hb| ha.angle_to(*hb)))
            .fold(0.0_f32, |acc, angle| acc.max(angle.abs()));
        assert!(
            max_angle.to_degrees() > 60.0,
            "les créatures partent toutes dans la même direction (écart angulaire \
             maximal entre deux déplacements : {:.1}°, attendu > 60°)",
            max_angle.to_degrees()
        );
    }

    /// Preuve de la demande gameplay « la Créature 1 doit avoir une attaque et la
    /// faire parfois » (`scene::demos::creature_bite_script`) : un contact
    /// **continu** de 20 s avec le joueur doit infliger au moins une morsure, mais
    /// pas à chaque frame — contrairement au pattern des dangers existants
    /// (`if obj.triggered then damage(dps*dt) end`, dégâts fractionnaires à
    /// chaque tick), l'attaque se déclenche par salves discrètes (~`BITE_DAMAGE`
    /// nets), espacées d'au moins `BITE_COOLDOWN`. Le joueur est réaligné sur la
    /// créature après chaque pas (elle patrouille toujours, cf. `creature_wander_
    /// script`) pour garantir un contact ininterrompu sans dépendre de la
    /// trajectoire réelle.
    #[test]
    fn creature_1_bites_the_player_sometimes_not_on_every_contact_tick() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let creature_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Créature")
            .expect("la démo MMORPG doit contenir une « Créature »");
        let player_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .expect("la démo MMORPG doit contenir un « Joueur »");

        // Isole la Créature 1 : depuis les créatures 6-10, d'autres attaques
        // (morsures 6/7, tirs 3/8/9/10) peuvent toucher le joueur pendant les
        // 20 s de contact — leurs chutes de vie se cumuleraient à la morsure
        // mesurée ici et fausseraient l'assertion « une salve ≈ 0.115 ». Les
        // masquer suffit : un objet invisible n'est jamais `triggered` (cf. le
        // filtre `visible` de `sim_step`) et les attaques à distance ignorent
        // les créatures masquées (cf. `update_creature_ranged_attacks`).
        for obj in app.scene.objects.iter_mut() {
            if obj.name.starts_with("Créature ") {
                obj.visible = false;
            }
        }

        let start = app.scene.objects[creature_idx].transform.position;
        app.scene.objects[player_idx].transform.position = start;
        app.hud_health = Some(1.0);
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        app.physics
            .as_mut()
            .unwrap()
            .set_position(player_idx, start);

        let dt = 1.0 / 60.0;
        let mut bites = 0u32;
        let mut prev_health = app.hud_health.unwrap();
        for step in 0..(20 * 60) {
            app.sim_step(dt);
            let health = app
                .hud_health
                .expect("damage() doit faire apparaître la vie du HUD");
            if health < prev_health - 1e-4 {
                bites += 1;
                let drop = prev_health - health;
                assert!(
                    (0.08..0.13).contains(&drop),
                    "step {step} : chute de vie {drop:.3} inattendue (attendu ≈ 0.115, \
                     une salve nette moins la régénération passive du tick, pas une \
                     fraction continue par frame)"
                );
            }
            prev_health = health;

            // Contact permanent : replace le joueur exactement sur la créature
            // (qui a continué de patrouiller ce tick) avant le prochain pas.
            let pos = app.scene.objects[creature_idx].transform.position;
            app.physics.as_mut().unwrap().set_position(player_idx, pos);
            app.scene.objects[player_idx].transform.position = pos;
        }

        assert!(
            bites > 0,
            "20 s de contact continu avec la Créature 1 auraient dû déclencher \
             au moins une morsure"
        );
        assert!(
            bites < 20,
            "{bites} morsures en 20 s pour un cooldown de 2,2 s — l'attaque semble \
             se déclencher en continu plutôt que « parfois »"
        );
    }

    /// Contre-épreuve de portée : le contact seul ne suffit pas à mordre — sans
    /// contact (joueur loin), la vie ne doit jamais baisser malgré 20 s de
    /// simulation (aucune tolérance de flakiness possible ici, contrairement au
    /// test précédent : `obj.triggered` est structurellement faux tout du long).
    #[test]
    fn creature_1_never_bites_without_contact() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let player_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .expect("la démo MMORPG doit contenir un « Joueur »");
        // Loin de toute créature/mur (cf. les spawns `Vec3::new(±3.0, 0.0, ±3.0)`
        // et le pourtour à `half = 12.0` dans `Scene::mmorpg_demo`).
        app.scene.objects[player_idx].transform.position = Vec3::new(0.0, 1.0, 9.0);
        app.hud_health = Some(1.0);
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let dt = 1.0 / 60.0;
        for _ in 0..(20 * 60) {
            app.sim_step(dt);
        }
        assert_eq!(
            app.hud_health,
            Some(1.0),
            "sans contact, la vie ne doit jamais baisser (aucune créature ne mord à distance)"
        );
    }

    /// Verrouille la répartition des attaques par créature : au contact
    /// (`creature_bite_script`, script Lua + `trigger`) pour les n°1 (morsure),
    /// 6 (chauve-souris) et 7 (crabe) ; à distance (natif, par nom — cf.
    /// `creature_attack::RANGED_CREATURE_ATTACKS`) pour les n°3, 8, 9 et 10 ;
    /// et rien du tout pour les pacifiques n°2, 4 et 5. Vérifié statiquement
    /// sur les scripts (pas d'appel `damage(`) plutôt qu'en rejouant une scène
    /// de contact par créature — plus rapide, tout aussi précis.
    #[test]
    fn creature_attacks_are_scoped_to_the_intended_creatures() {
        let scene = crate::scene::Scene::mmorpg_demo();
        let by_name = |name: &str| {
            scene
                .objects
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {name} »"))
        };
        for name in ["Créature", "Créature 6", "Créature 7"] {
            let obj = by_name(name);
            assert!(
                obj.script.contains("damage("),
                "« {name} » devrait avoir une attaque au contact (cf. creature_bite_script)"
            );
            assert!(
                obj.trigger,
                "« {name} » doit avoir `trigger = true` pour que `obj.triggered` \
                 fonctionne dans son script d'attaque"
            );
        }
        for name in [
            "Créature 2",
            "Créature 3",
            "Créature 4",
            "Créature 5",
            "Créature 8",
            "Créature 9",
            "Créature 10",
            "Créature 11",
            "Créature 12",
            "Créature 13",
            "Créature 14",
            "Créature 15",
            "Créature 16",
            "Créature 17",
            "Créature 18",
            "Créature 19",
            "Créature 20",
        ] {
            let obj = by_name(name);
            assert!(
                !obj.script.contains("damage("),
                "« {name} » ne devrait pas attaquer via son script (script : {:?})",
                obj.script
            );
        }
        // Les attaques à distance sont natives, déclenchées par nom : chaque
        // créature de la table doit exister dans la démo (une entrée orpheline
        // serait une attaque silencieusement morte).
        for cfg_name in ["Créature 3", "Créature 8", "Créature 9", "Créature 10"] {
            by_name(cfg_name);
        }
    }

    /// Preuve (bug observé en jeu sur la créature MMORPG : « les bras et la tête
    /// partent en couille dès qu'elle tourne », silhouette dédoublée) : un script qui
    /// ne réécrit que `obj.ry` doit produire un cap **stable** d'un tick à l'autre,
    /// y compris au-delà de ±90°. Avant le correctif (`scripting::
    /// canonical_euler_xyz`), `to_euler(XYZ)` représentait un yaw de -117° comme
    /// (rx=180°, ry=-63°, rz=180°) ; le script écrasait `ry` seul et la
    /// recomposition gardait les flips ±180° de rx/rz → la rotation alternait entre
    /// -117° et -63° un tick sur deux (écart 2×(117−90) = 54°, jusqu'à 180° plein
    /// sud) — invisible en marche vers le « nord » (|cap| < 90°, aucun flip), d'où
    /// le symptôme « en ligne droite ça va, dès qu'il tourne ça casse ».
    #[test]
    fn script_rewriting_only_ry_keeps_a_stable_heading_beyond_90_degrees() {
        for target in [-179.0f32, -117.0, -95.0, 95.0, 150.0, 179.0] {
            let mut app = AppState::new();
            app.scene.objects.clear();
            app.scene.objects.push(SceneObject {
                script: format!("obj.ry = {target}"),
                ..Default::default()
            });
            let dt = 1.0 / 60.0;
            for tick in 0..6 {
                app.sim_step(dt);
                // Yaw lu en YXZ (yaw en premier : plage complète ±180°, pas de
                // représentation à flips comme le XYZ contraint à ±90° au milieu).
                let (yaw, _, _) = app.scene.objects[0]
                    .transform
                    .rotation
                    .to_euler(glam::EulerRot::YXZ);
                let mut diff = yaw.to_degrees() - target;
                diff = ((diff + 180.0).rem_euclid(360.0)) - 180.0;
                assert!(
                    diff.abs() < 0.01,
                    "tick {tick} : cap affiché {:.2}° pour obj.ry = {target}° — \
                     le cap doit rester exactement celui écrit par le script, \
                     sans alternance d'un tick à l'autre",
                    yaw.to_degrees()
                );
            }
        }
    }

    /// Sprint 111 (hot-reload) : `script_cache` est clé par hash du **contenu** du
    /// script (`scripting::script_key`), pas par identité d'objet — retoucher le
    /// texte d'un script en cours de Play (panneau « Scripts », ou IA) doit donc
    /// prendre effet dès le tick suivant, sans repasser par Stop/Play. Même principe
    /// que les textures, cf. `gfx::renderer::tests::invalidate_asset_textures_
    /// forces_a_reload_from_disk_on_the_next_sync` — mais ici aucune invalidation
    /// n'est nécessaire : la clé change d'elle-même avec le texte.
    #[test]
    fn editing_an_objects_script_mid_play_takes_effect_on_the_next_tick_without_restarting_play() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            script: "obj.x = 1".into(),
            ..Default::default()
        });
        app.sim_step(0.1);
        assert_eq!(app.scene.objects[0].transform.position.x, 1.0);

        app.scene.objects[0].script = "obj.x = 2".into();
        app.sim_step(0.1);
        assert_eq!(
            app.scene.objects[0].transform.position.x, 2.0,
            "le nouveau texte du script doit s'appliquer dès le tick suivant, sans redémarrer Play"
        );
    }

    #[test]
    fn sim_step_advances_a_crossfade_towards_completion_and_stops() {
        use crate::scene::AnimationState;
        let mut app = AppState::new();
        app.scene.objects.clear();
        let mut anim = AnimationState {
            clip: "Idle".into(),
            ..Default::default()
        };
        assert_eq!(anim.blend, 1.0, "pas de transition en cours au départ");
        anim.set_clip("Run"); // démarre le fondu
        assert_eq!(anim.blend, 0.0);
        assert_eq!(anim.prev_clip, "Idle");
        app.scene.objects.push(SceneObject {
            animation: Some(anim),
            ..Default::default()
        });

        // CROSSFADE_SECONDS = 0.2s : un pas de 0.1s doit avancer blend à ~0.5, pas plus.
        app.sim_step(0.1);
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert!(
            (anim.blend - 0.5).abs() < 1e-4,
            "blend attendu ≈0.5 après 0.1s de fondu (durée 0.2s), obtenu {}",
            anim.blend
        );
        assert!(
            anim.prev_time > 0.0,
            "le clip quitté doit continuer d'avancer pendant le fondu"
        );

        // Encore 0.2s (au-delà de la durée du fondu) : blend clampé à 1.0, jamais au-delà.
        app.sim_step(0.2);
        let anim = app.scene.objects[0].animation.as_ref().unwrap();
        assert_eq!(anim.blend, 1.0, "blend ne doit jamais dépasser 1.0");

        // Transition terminée : encore un pas, prev_time ne doit plus avancer.
        let prev_time_after = anim.prev_time;
        app.sim_step(0.1);
        assert_eq!(
            app.scene.objects[0].animation.as_ref().unwrap().prev_time,
            prev_time_after,
            "prev_time ne doit plus bouger une fois la transition terminée"
        );
    }

    #[test]
    fn tank_controls_turn_then_thrust_move_the_player_along_its_own_facing() {
        // Bout en bout : A/D (rotation manuelle) et W/S (avance/recul) doivent piloter le
        // joueur indépendamment de la caméra, contrairement au joystick/flèches
        // (contrôles « tank »).
        let mut app = AppState::new();
        app.load_controller_demo();
        app.playing = true;
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la démo contrôleur a un joueur pilotable");

        // D tenue (tourner à gauche, cf. doc `PlayerInput::key_turn`) : le yaw doit
        // augmenter par rapport à sa valeur de départ (0). Peu de pas : avec
        // `MANUAL_TURN_SPEED` (3 rad/s), rester bien en-deçà de π pour ne pas
        // « boucler » et fausser la lecture (`to_scaled_axis` ramène l'angle dans
        // (-π, π]).
        app.input_state.key_turn = 1.0;
        for _ in 0..5 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        }
        app.input_state.key_turn = 0.0;
        let yaw = app.scene.objects[pi]
            .transform
            .rotation
            .to_euler(EulerRot::YXZ)
            .0;
        assert!(
            yaw > 0.1,
            "D doit tourner le joueur vers la gauche, yaw={yaw}"
        );

        // Puis W tenue : le joueur doit avancer le long de cette orientation, pas vers
        // le -Z monde qu'utiliserait un déplacement caméra-relative.
        let p0 = app.scene.objects[pi].transform.position;
        app.input_state.key_thrust = 1.0;
        for _ in 0..30 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        }
        let moved = app.scene.objects[pi].transform.position - p0;
        let expected_dir = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        assert!(
            moved.length() > 0.3,
            "W doit faire avancer le joueur, déplacement={moved:?}"
        );
        assert!(
            moved.normalize().dot(expected_dir) > 0.8,
            "l'avance doit suivre l'orientation du joueur (yaw={yaw}), pas la caméra : \
             déplacement={moved:?}, attendu≈{expected_dir:?}"
        );
    }

    #[test]
    fn tank_controls_reversing_never_spins_the_player_around() {
        // Garde-fou : l'orientation doit rester fixe pendant S (recul), pas se
        // remettre à tourner vers le vecteur de vitesse (cf. docs/audits/app-mod.md).
        let mut app = AppState::new();
        app.load_controller_demo();
        app.playing = true;
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la démo contrôleur a un joueur pilotable");
        let yaw0 = app.scene.objects[pi]
            .transform
            .rotation
            .to_euler(EulerRot::YXZ)
            .0;

        app.input_state.key_thrust = -1.0; // S tenue
        for _ in 0..90 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            app.advance_play();
        }
        let yaw1 = app.scene.objects[pi]
            .transform
            .rotation
            .to_euler(EulerRot::YXZ)
            .0;
        assert!(
            (yaw1 - yaw0).abs() < 1e-3,
            "reculer (S) ne doit jamais faire tourner le personnage : yaw0={yaw0}, yaw1={yaw1}"
        );
    }

    /// Garde-fou du piège respawn + PV : un ennemi à plusieurs PV et
    /// `respawn_delay > 0` doit revenir avec ses PV d'origine, pas avec les 0 PV
    /// où il les a laissés (sinon il réapparaît « déjà vaincu » : re-masqué au
    /// premier coup, sans jamais encaisser sa barre de vie). Cf. `Combat::max_hp`
    /// (capture au premier coup dans `Scene::damage_attackable_by`) et
    /// `process_respawns` (restauration).
    #[test]
    fn a_respawning_enemy_comes_back_with_its_original_hp() {
        let mut app = AppState::new();
        app.scene.objects.push(crate::scene::SceneObject {
            name: "Brute".into(),
            combat: Some(crate::scene::Combat {
                attackable: true,
                hp: 3,
                ..Default::default()
            }),
            respawn_delay: 2.0,
            ..Default::default()
        });
        let i = app.scene.objects.len() - 1;

        // Trois coups pour le vaincre : les deux premiers blessent, le troisième
        // l'achève (masqué) — mise en file de respawn comme le fait `update_attack`
        // (cf. `app::combat`) au moment de la mise à mort.
        assert!(!app.scene.damage_attackable(i));
        assert!(!app.scene.damage_attackable(i));
        assert!(app.scene.damage_attackable(i), "3e coup = mise à mort");
        assert!(!app.scene.objects[i].visible, "vaincu ⇒ masqué");
        let delay = app.scene.objects[i].respawn_delay;
        app.respawn_queue.push((i, app.time + delay));

        // Délai non écoulé : rien ne bouge.
        app.process_respawns(app.time + delay * 0.5);
        assert!(!app.scene.objects[i].visible);

        // Délai écoulé : il réapparaît avec ses 3 PV d'origine…
        app.process_respawns(app.time + delay);
        assert!(app.scene.objects[i].visible, "délai écoulé ⇒ réapparu");
        assert_eq!(
            app.scene.objects[i].combat.as_ref().unwrap().hp,
            3,
            "le respawn doit restaurer les PV d'origine, pas laisser 0"
        );
        // …et redevient un adversaire entier : un coup le blesse sans le vaincre.
        assert!(
            !app.scene.damage_attackable(i),
            "après respawn, un seul coup ne doit plus suffire"
        );
        assert!(app.scene.objects[i].visible);
    }
