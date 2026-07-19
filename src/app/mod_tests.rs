use super::*;
// Sprint 105a-1 : `simulation`/`scripting` sont des sous-modules de `app`,
// pas ré-exportés par `use super::*` (qui ne remonte que le contenu de
// `app` lui-même) — import explicite des symboles `pub(super)` que ces
// tests appellent directement (par nom, pas via `AppState::advance_play`).
use super::multiplayer::PlayerClass;
use super::scripting::run_script;

/// Noms français (générateurs procéduraux) reconnus par catégorie.
#[test]
fn classify_decor_recognizes_french_object_names() {
    assert_eq!(
        classify_decor("Halte Sud-Ouest arbre", ""),
        Some(MinimapDecorKind::Forest)
    );
    assert_eq!(
        classify_decor("Mur nord du hameau", ""),
        Some(MinimapDecorKind::Wall)
    );
    assert_eq!(
        classify_decor("Rive Est", "shore_bank_a.glb"),
        Some(MinimapDecorKind::Water)
    );
}

/// Chemins d'assets glTF (générés en anglais) reconnus par catégorie.
#[test]
fn classify_decor_recognizes_asset_paths() {
    assert_eq!(
        classify_decor("Maison 3", "hamlet_house_a.glb"),
        Some(MinimapDecorKind::Building)
    );
    assert_eq!(
        classify_decor("Objet 12", "nature_tree_windswept.glb"),
        Some(MinimapDecorKind::Forest)
    );
}

/// Le décor scatter non catégorisable (herbe, rochers isolés…) ne doit
/// jamais se voir attribuer une catégorie au hasard.
#[test]
fn classify_decor_returns_none_for_unrecognized_names() {
    assert_eq!(classify_decor("Rocher 4", "nature_rock_medium.glb"), None);
    assert_eq!(classify_decor("Touffe d'herbe", ""), None);
}

/// Un mot-clé ne doit matcher qu'en mot entier, pas en sous-chaîne — « eau »
/// apparaît dans « hameau »/« château », qui n'ont rien à voir avec de l'eau
/// (bug constaté : « Mur nord du hameau » se classait « Eau »).
#[test]
fn classify_decor_does_not_match_substrings_inside_other_words() {
    assert_eq!(
        classify_decor("Mur nord du hameau", ""),
        Some(MinimapDecorKind::Wall)
    );
    assert_eq!(classify_decor("Château en ruine", ""), None);
}

/// Une centaine d'arbres serrés dans un même coin de carte (forêt dense)
/// doit s'effondrer en une poignée de marqueurs, pas rester un point par
/// arbre — sinon la carte devient un nuage de points illisible.
#[test]
fn thin_decor_collapses_a_dense_cluster_to_few_markers() {
    let bounds = (-50.0, -50.0, 50.0, 50.0);
    let decor: Vec<MinimapDecor> = (0..100)
        .map(|i| MinimapDecor {
            x: 10.0 + (i % 10) as f32 * 0.1,
            z: 10.0 + (i / 10) as f32 * 0.1,
            kind: MinimapDecorKind::Forest,
        })
        .collect();
    let thinned = thin_decor(decor, bounds, 4.0);
    assert!(
        thinned.len() < 10,
        "attendu une forte réduction, obtenu {} marqueurs",
        thinned.len()
    );
    assert!(!thinned.is_empty());
}

/// Deux catégories au même endroit (ex. un mur et une maison voisins) ne
/// doivent jamais se fondre en un seul marqueur — le dédoublonnage est
/// par (catégorie, cellule), pas par cellule seule.
#[test]
fn thin_decor_keeps_distinct_categories_in_the_same_cell() {
    let bounds = (-50.0, -50.0, 50.0, 50.0);
    let decor = vec![
        MinimapDecor {
            x: 0.0,
            z: 0.0,
            kind: MinimapDecorKind::Wall,
        },
        MinimapDecor {
            x: 0.05,
            z: 0.05,
            kind: MinimapDecorKind::Building,
        },
    ];
    let thinned = thin_decor(decor, bounds, 4.0);
    assert_eq!(thinned.len(), 2);
}

/// Un décor déjà épars (un marqueur par grande zone) ne doit rien perdre :
/// `thin_decor` ne doit jamais supprimer un marqueur isolé.
#[test]
fn thin_decor_keeps_sparse_markers_untouched() {
    let bounds = (-50.0, -50.0, 50.0, 50.0);
    let decor = vec![
        MinimapDecor {
            x: -40.0,
            z: -40.0,
            kind: MinimapDecorKind::Water,
        },
        MinimapDecor {
            x: 40.0,
            z: 40.0,
            kind: MinimapDecorKind::Water,
        },
    ];
    let thinned = thin_decor(decor, bounds, 4.0);
    assert_eq!(thinned.len(), 2);
}

/// Le marqueur gardé est recalé sur le centre de sa cellule (rendu en
/// régions continues, cf. doc de `thin_decor`), pas laissé à la position
/// brute de l'objet — sinon les pastilles voisines ne s'alignent pas.
#[test]
fn thin_decor_snaps_kept_markers_to_cell_centers() {
    let bounds = (0.0, 0.0, 100.0, 100.0);
    let decor = vec![MinimapDecor {
        x: 11.0, // cellule 2 (pas 4.0) : [8, 12), centre attendu 10.0
        z: 9.0,
        kind: MinimapDecorKind::Forest,
    }];
    let thinned = thin_decor(decor, bounds, 4.0);
    assert_eq!(thinned.len(), 1);
    assert!((thinned[0].x - 10.0).abs() < 0.001);
    assert!((thinned[0].z - 10.0).abs() < 0.001);
}

/// `minimap_data` doit distinguer les créatures de la manche affichée
/// (`AppState::wave`) des autres — demande utilisateur (« où sont les
/// monstres de la vague qui attaque ? »). Seul le monstre dont
/// `combat.wave` correspond à la manche courante est `active_wave`.
#[test]
fn minimap_data_flags_only_current_wave_creatures_as_active() {
    let mut app = AppState::new();
    app.wave = 2;
    app.scene.objects.push(SceneObject {
        name: "Traqueuse vague 2".to_string(),
        transform: Transform {
            position: Vec3::new(5.0, 0.0, 5.0),
            ..Default::default()
        },
        ai_chaser: Some(crate::scene::AiChaser::default()),
        combat: Some(crate::scene::Combat {
            wave: 2,
            ..Default::default()
        }),
        visible: true,
        ..Default::default()
    });
    app.scene.objects.push(SceneObject {
        name: "Traqueuse vague 1 (passée)".to_string(),
        transform: Transform {
            position: Vec3::new(-5.0, 0.0, -5.0),
            ..Default::default()
        },
        ai_chaser: Some(crate::scene::AiChaser::default()),
        combat: Some(crate::scene::Combat {
            wave: 1,
            ..Default::default()
        }),
        visible: true,
        ..Default::default()
    });
    let minimap = app.minimap_data();
    assert_eq!(minimap.creatures.len(), 2);
    let active_count = minimap.creatures.iter().filter(|c| c.active_wave).count();
    assert_eq!(
        active_count, 1,
        "une seule créature (vague 2) doit être marquée active_wave"
    );
}

/// Nom de prefab unique par appel (horloge + pid) — surtout utile quand plusieurs
/// runs se partagent le même dossier temporaire d'assets.
fn unique_test_prefab_name(tag: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("test_{tag}_{}_{}", std::process::id(), nanos)
}

/// Dossier temporaire unique par test, à passer à
/// `assets::override_assets_dir_for_test` — même patron que
/// `scene::tests::temp_assets_dir` : ce test exerce `spawn()` côté Lua
/// (`scripting.rs`), qui appelle la variante **globale**
/// `Scene::instantiate_prefab` (pas de point d'injection de répertoire dans le
/// binding Lua), d'où le besoin d'une redirection de `assets_dir()` plutôt que
/// d'une variante `_at`.
fn temp_assets_dir_for_test(tag: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "rusteegear_app_assets_test_{tag}_{}_{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_door_opens_on_score_3_without_direct_coupling() {
    // Bout en bout (App réel) : une « porte » scriptée
    // s'ouvre quand le score atteint 3, sans référencer ni les pièces ni le
    // joueur — elle n'écoute que l'événement `score:3` émis par le moteur
    // (`add_score`). Les 3 pièces sont sur le joueur : toutes ramassées le même
    // tick, précisément le cas où émettre seulement la valeur *finale* du score
    // ferait rater l'événement.
    let mut app = AppState::new();
    let mut scene = crate::scene::Scene::default();
    scene.objects.push(crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    });
    for i in 0..3 {
        scene.objects.push(crate::scene::SceneObject {
            name: format!("Pièce {i}"),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0))
                .with_scale(Vec3::splat(0.3)),
            tap_action: crate::scene::TapAction::Hide,
            ..Default::default()
        });
    }
    // Une 4e pièce hors de portée : sans elle, ramasser les 3 premières gagne la
    // partie le même tick — et le jeu **gèle** une fois gagné (cf. `advance_play`),
    // l'événement `score:3` ne serait jamais délivré. Le livrable vise une porte
    // qui s'ouvre *en cours de partie*, pas à l'écran de victoire.
    scene.objects.push(crate::scene::SceneObject {
        name: "Pièce lointaine".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(50.0, 1.0, 50.0))
            .with_scale(Vec3::splat(0.3)),
        tap_action: crate::scene::TapAction::Hide,
        ..Default::default()
    });
    scene.objects.push(crate::scene::SceneObject {
        name: "Porte".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 1.0, 0.0)),
        script: "if on_event('score:3') then obj.y = 10 end".into(),
        ..Default::default()
    });
    app.scene = scene;
    app.playing = true;
    for _ in 0..10 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert_eq!(app.score, 3, "les 3 pièces doivent être ramassées");
    let door = app
        .scene
        .objects
        .iter()
        .find(|o| o.name == "Porte")
        .unwrap();
    assert!(
        (door.transform.position.y - 10.0).abs() < 1e-4,
        "la porte devait s'ouvrir sur l'événement score:3 (y = {})",
        door.transform.position.y
    );
}

#[test]
fn push_hud_event_reaches_scripts_prefixed_with_hud_via_on_event() {
    // Cf. `editor::hud::hud_widgets` : un widget `Button` cliqué appelle
    // `AppState::push_hud_event(action)`, qui doit se lire côté script exactement
    // comme un `emit()` Lua préfixé `hud:` — même file d'événements
    // (`AppState::game_events`), un script ne doit pas pouvoir distinguer les deux
    // sources.
    let mut app = AppState::new();
    let mut scene = crate::scene::Scene::default();
    scene.objects.push(crate::scene::SceneObject {
        name: "Porte HUD".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        script: "if on_event('hud:jump') then obj.y = 9.0 end".into(),
        ..Default::default()
    });
    app.scene = scene;
    app.playing = true;
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    // Transition Edit→Play d'abord : elle vide `game_events` (nouvelle partie), donc
    // le clic HUD doit être poussé après, sans quoi il serait perdu avant même
    // d'atteindre un script.
    app.advance_play();
    app.push_hud_event("jump");
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    let porte = app
        .scene
        .objects
        .iter()
        .find(|o| o.name == "Porte HUD")
        .unwrap();
    assert!(
        (porte.transform.position.y - 9.0).abs() < 1e-4,
        "le clic HUD devait se lire via on_event('hud:jump') (y = {})",
        porte.transform.position.y
    );
}

#[test]
fn script_calling_obj_destroy_soft_deletes_via_visible_false() {
    // `obj:destroy()` doit se traduire par `visible = false` — une
    // suppression douce, pas un retrait de `scene.objects` (cf. la doc de
    // `run_script`, cette dernière casserait les indices retenus ailleurs).
    let mut app = AppState::new();
    let mut scene = crate::scene::Scene::default();
    scene.objects.push(crate::scene::SceneObject {
        name: "Éphémère".into(),
        script: "obj:destroy()".into(),
        ..Default::default()
    });
    app.scene = scene;
    app.playing = true;
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    assert!(!app.scene.objects[0].visible, "l'objet devait être masqué");
    // Toujours dans `scene.objects` : ce n'est pas un vrai retrait.
    assert_eq!(app.scene.objects.len(), 1);
}

#[test]
fn a_spawned_enemy_via_lua_joins_the_scene_and_can_be_found_by_tag() {
    // Un script peut faire apparaître un ennemi depuis un
    // prefab (`spawn`), et cet ennemi devient trouvable par `find_tag` (au tick
    // suivant : `find_tag` lit un instantané pris avant la boucle des scripts).
    // `spawn()` passe par la variante globale `Scene::instantiate_prefab`, donc
    // `assets_dir()` est redirigé vers un dossier temporaire pour ce thread
    // plutôt que d'écrire dans le vrai `~/.motor3derust/assets/`.
    let _dir_guard =
        crate::assets::override_assets_dir_for_test(temp_assets_dir_for_test("spawn_lua"));
    let name = unique_test_prefab_name("ennemi97");
    let template = crate::scene::SceneObject {
        name: "Ennemi".into(),
        mesh: crate::scene::MeshKind::Cube,
        tag: "ennemi".into(),
        ..Default::default()
    };
    let asset_id =
        crate::scene::Scene::save_prefab(&template, &name, &crate::assets::PrefabScope::General)
            .unwrap();

    let mut app = AppState::new();
    let mut scene = crate::scene::Scene::default();
    scene.objects.push(crate::scene::SceneObject {
        name: "Générateur".into(),
        script: format!("if time < 0.02 then spawn('{asset_id}', 3.0, 0.0, 4.0) end"),
        ..Default::default()
    });
    app.scene = scene;
    app.playing = true;
    for _ in 0..3 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert_eq!(
        app.scene.objects.len(),
        2,
        "le spawn doit ajouter exactement un objet"
    );
    let spawned = &app.scene.objects[1];
    assert_eq!(spawned.tag, "ennemi", "l'instance doit suivre le template");
    assert!((spawned.transform.position - Vec3::new(3.0, 0.0, 4.0)).length() < 1e-4);
}

/// Dossier temporaire unique par test (Sprint 105a-3, isolation des
/// tests système) — même schéma que `assets::tests::temp_assets_dir` :
/// aucune dépendance au vrai `$HOME`, sûr sous exécution parallèle.
fn temp_save_dir(tag: &str) -> std::path::PathBuf {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    tag.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let dir = std::env::temp_dir().join(format!("rusteegear_appsave_test_{:x}", hasher.finish()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn saving_and_loading_a_game_restores_score_position_and_lua_vars() {
    // La progression (score, positions, variables de
    // script) doit survivre à une sauvegarde puis un chargement — testé bout en
    // bout via `AppState::save_game_at`/`load_game_at`, qui écrivent réellement
    // sur disque (comme le ferait le jeu réel sur desktop ou Android), mais dans
    // un dossier temporaire isolé plutôt que le vrai `user://`.
    let dir = temp_save_dir("roundtrip");
    let slot = "roundtrip";
    let mut app = AppState::new();
    app.scene = crate::scene::Scene::default();
    app.scene.objects.push(crate::scene::SceneObject {
        name: "Joueur".into(),
        transform: Transform::from_pos(Vec3::new(3.0, 1.0, -2.0)),
        ..Default::default()
    });
    app.score = 7;
    app.lua_vars.insert("niveau".to_string(), 4.0);

    app.save_game_at(slot, &dir).expect("sauvegarde impossible");

    // Simule une reprise de partie : score/position/variables sont remis à zéro
    // avant le chargement (ex. l'app vient de redémarrer).
    app.score = 0;
    app.scene.objects[0].transform.position = Vec3::ZERO;
    app.lua_vars.clear();

    app.load_game_at(slot, &dir).expect("chargement impossible");

    assert_eq!(app.score, 7);
    assert_eq!(
        app.scene.objects[0].transform.position,
        Vec3::new(3.0, 1.0, -2.0)
    );
    assert_eq!(app.lua_vars.get("niveau"), Some(&4.0));
}

#[test]
fn an_anim_notify_gates_the_combat_hit_window() {
    // Le coup ne doit « toucher » (ici : le script met
    // `in_window` à 1 via `save.set`) que pendant la fenêtre d'animation délimitée
    // par deux marqueurs (`hit_open`/`hit_close`), pas avant, pas après.
    let mut imported = crate::scene::ImportedMesh {
        name: "Guerrier".into(),
        ..Default::default()
    };
    imported
        .clips
        .push(crate::scene::import::Clip::without_tracks("attaque", 1.0));
    imported.notifies.insert(
        "attaque".to_string(),
        vec![
            (0.3, "hit_open".to_string()),
            (0.6, "hit_close".to_string()),
        ],
    );
    let mut scene = crate::scene::Scene::default();
    scene.imported.push(imported);
    scene.objects.push(crate::scene::SceneObject {
        name: "Guerrier".into(),
        mesh: crate::scene::MeshKind::Imported(0),
        animation: Some(crate::scene::AnimationState {
            clip: "attaque".into(),
            time: 0.0,
            speed: 1.0,
            prev_clip: String::new(),
            prev_time: 0.0,
            blend: 1.0,
        }),
        script: "\
                if on_event('anim:hit_open') then save.set('in_window', 1) end\n\
                if on_event('anim:hit_close') then save.set('in_window', 0) end"
            .into(),
        ..Default::default()
    });
    let mut app = AppState::new();
    app.scene = scene;
    app.playing = true;

    let advance_one_tick = |app: &mut AppState| {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
        app.advance_play();
    };

    // ~0.2 s : avant `hit_open` (0.3 s), la fenêtre ne doit pas encore être ouverte.
    for _ in 0..12 {
        advance_one_tick(&mut app);
    }
    assert_eq!(
        app.lua_vars.get("in_window"),
        None,
        "la fenêtre ne doit pas encore être ouverte avant 0.3 s"
    );

    // ~0.35 s : après `hit_open`, avant `hit_close` — fenêtre ouverte.
    for _ in 0..9 {
        advance_one_tick(&mut app);
    }
    assert_eq!(
        app.lua_vars.get("in_window"),
        Some(&1.0),
        "la fenêtre doit être ouverte entre 0.3 s et 0.6 s"
    );

    // ~0.8 s : après `hit_close` — fenêtre refermée.
    for _ in 0..27 {
        advance_one_tick(&mut app);
    }
    assert_eq!(
        app.lua_vars.get("in_window"),
        Some(&0.0),
        "la fenêtre doit être refermée après 0.6 s"
    );
}

#[test]
fn script_setting_obj_anim_starts_a_crossfade() {
    // Exposition Lua : `obj.anim = "run"` doit atterrir dans
    // `AnimationState` via `set_clip`, avec le fondu enchaîné qu'il déclenche
    // (`prev_clip` retient l'ancien clip, `blend` repart à 0).
    use crate::scene::AnimationState;
    let lua = Lua::new();
    let src = "obj.anim = 'run'";
    let func = lua.load(src).into_function().unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0; 3];
    let mut anim = Some(AnimationState {
        clip: "idle".into(),
        time: 1.5,
        speed: 1.0,
        prev_clip: String::new(),
        prev_time: 0.0,
        blend: 1.0,
    });
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut anim,
        0.016,
        0.0,
        &PlayerInput::default(),
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    let state = anim.unwrap();
    assert_eq!(state.clip, "run");
    assert_eq!(state.prev_clip, "idle");
    assert_eq!(state.blend, 0.0);
}

#[test]
fn script_leaving_obj_anim_untouched_does_not_reset_clip() {
    // Sans écriture de `obj.anim` par le script, le clip courant ne doit pas être
    // relancé (sinon `set_clip` redémarrerait un fondu à chaque frame sans raison).
    use crate::scene::AnimationState;
    let lua = Lua::new();
    let src = "obj.x = obj.x"; // script sans rapport avec l'animation
    let func = lua.load(src).into_function().unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0; 3];
    let mut anim = Some(AnimationState {
        clip: "run".into(),
        time: 0.4,
        speed: 1.0,
        prev_clip: String::new(),
        prev_time: 0.0,
        blend: 1.0,
    });
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut anim,
        0.016,
        0.0,
        &PlayerInput::default(),
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    let state = anim.unwrap();
    assert_eq!(state.clip, "run");
    assert_eq!(state.time, 0.4);
    assert_eq!(state.blend, 1.0);
}

#[test]
fn script_reacts_to_tap_and_changes_color() {
    // Au tap, l'objet vire au rouge.
    let lua = Lua::new();
    let src = "if obj.tapped then obj.r = 1.0; obj.g = 0.0; obj.b = 0.0 end";
    let func = lua.load(src).into_function().unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [0.5, 0.5, 0.5];
    let input = PlayerInput::default();
    // pas de tap : couleur inchangée
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(col, [0.5, 0.5, 0.5]);
    // tap : passe au rouge
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        true,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(col, [1.0, 0.0, 0.0]);
}

#[test]
fn script_reacts_to_trigger() {
    // obj.y monte quand le joueur entre dans la zone.
    let lua = Lua::new();
    let src = "if obj.triggered then obj.y = 9.0 end";
    let func = lua.load(src).into_function().unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0, 1.0, 1.0];
    let input = PlayerInput::default();
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(t.position.y, 0.0);
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        true,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(t.position.y, 9.0);
}

#[test]
fn script_reads_tilt() {
    let lua = Lua::new();
    let func = lua
        .load("obj.x = obj.x + tilt.x; obj.z = obj.z + tilt.y")
        .into_function()
        .unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0; 3];
    let input = PlayerInput {
        tilt: (1.0, -1.0),
        ..Default::default()
    };
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert!((t.position.x - 1.0).abs() < 1e-5);
    assert!((t.position.z + 1.0).abs() < 1e-5);
}

#[test]
fn script_sets_health() {
    let lua = Lua::new();
    let func = lua.load("set_health(0.5)").into_function().unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0; 3];
    let input = PlayerInput::default();
    let mut health = None;
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut health,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(health, Some(0.5));
}

#[test]
fn script_damage_is_relative_and_stacks_across_objects_same_frame() {
    // `damage(v)` doit soustraire de la vie déjà accumulée cette frame (par d'autres
    // objets), contrairement à `set_health` (valeur absolue) qui écraserait les dégâts
    // d'un ennemi précédent si un autre script s'exécutait après lui sans le vouloir.
    let lua = Lua::new();
    let func = lua.load("damage(0.3)").into_function().unwrap();
    let input = PlayerInput::default();
    // Aucun système de vie démarré : la base par défaut est pleine vie (1.0).
    let mut health = None;
    run_script(
        &lua,
        &func,
        &mut Transform::from_pos(Vec3::ZERO),
        &mut [1.0; 3],
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut health,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(health, Some(0.7));
    // Un deuxième objet inflige des dégâts la même frame : doit partir de 0.7, pas de 1.0.
    run_script(
        &lua,
        &func,
        &mut Transform::from_pos(Vec3::ZERO),
        &mut [1.0; 3],
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut health,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert!(
        (health.unwrap() - 0.4).abs() < 1e-5,
        "les dégâts de deux objets la même frame doivent s'additionner : {health:?}"
    );
    // Clampé à 0, ne descend pas en négatif.
    for _ in 0..10 {
        run_script(
            &lua,
            &func,
            &mut Transform::from_pos(Vec3::ZERO),
            &mut [1.0; 3],
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut health,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
    }
    assert_eq!(health, Some(0.0));
}

#[test]
fn controller_demo_enemy_scripts_compile_and_patrol() {
    // Les ennemis de la démo contrôleur sont scriptés (patrouille + pulsation rouge) :
    // vérifie que leurs scripts compilent et déplacent réellement l'objet dans le temps
    // (sinon un ennemi "mort" resterait immobile, silencieusement cassé).
    let scene = crate::scene::Scene::controller_demo();
    let enemies: Vec<_> = scene
        .objects
        .iter()
        .filter(|o| o.name.starts_with("Ennemi"))
        .collect();
    assert!(enemies.len() >= 3, "au moins 3 ennemis dans la démo");
    let lua = Lua::new();
    for e in enemies {
        assert!(
            e.trigger && !e.deadly,
            "un ennemi doit infliger des dégâts progressifs (trigger), pas tuer \
                 instantanément (deadly) : {}",
            e.name
        );
        let func = lua.load(&e.script).into_function().unwrap();
        let mut t0 = e.transform;
        let mut col = e.color;
        let input = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t0,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let mut t1 = e.transform;
        let mut col1 = e.color;
        run_script(
            &lua,
            &func,
            &mut t1,
            &mut col1,
            &mut None,
            0.016,
            1.0,
            &input,
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(
            (t0.position - t1.position).length() > 0.01,
            "l'ennemi {} doit bouger avec le temps",
            e.name
        );
    }
}

/// Scène synthétique minimale (sol + joueur + un danger `trigger`+`damage()` couvrant
/// tout le sol) : isole la mécanique vie/dégâts de l'équilibrage d'un niveau réel.
/// La démo contrôleur n'est pas réutilisée ici : sa patrouille est conçue pour un
/// contact *intermittent* (l'ennemi s'éloigne), ce qui ne conviendrait pas pour
/// tester un contact permanent sans coupler le test à ce détail d'équilibrage.
fn synthetic_damage_scene() -> crate::scene::Scene {
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            attack_button: "Attaque".into(),
            attack_range: 2.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(16.0, 1.0, 16.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let mut danger = crate::scene::SceneObject {
        name: "Danger".into(),
        mesh: crate::scene::MeshKind::Cube,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, 0.0))
            .with_scale(Vec3::splat(3.0)),
        trigger: true,
        combat: Some(crate::scene::Combat {
            attackable: true,
            ..Default::default()
        }),
        respawn_delay: 100.0,
        script: "if obj.triggered then damage(2.0 * dt) end".into(),
        ..Default::default()
    };
    danger.color = [1.0; 3];
    let mut fx = crate::scene::SceneObject {
        name: "FX Attaque".into(),
        mesh: crate::scene::MeshKind::Sphere,
        combat: Some(crate::scene::Combat {
            is_attack_fx: true,
            ..Default::default()
        }),
        visible: false,
        ..Default::default()
    };
    fx.color = [1.0; 3];
    crate::scene::Scene {
        objects: vec![sol, joueur, danger, fx],
        ..Default::default()
    }
}

#[test]
fn sustained_enemy_contact_drains_health_and_ends_the_game() {
    // Bout en bout (App réel, pas juste `run_script`) : un contact **permanent** avec
    // un danger `trigger` + `damage()` doit finir par vaincre le joueur via le nouveau
    // check de défaite sur `hud_health <= 0`, malgré la régénération passive.
    let mut app = AppState::new();
    app.scene = synthetic_damage_scene();
    app.playing = true;
    let mut ended = false;
    for _ in 0..80 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        if app.lost {
            ended = true;
            break;
        }
    }
    assert!(
        ended,
        "un contact soutenu doit finir par vaincre le joueur (vie = {:?})",
        app.hud_health
    );
}

#[test]
fn attacking_defeats_enemy_and_stops_further_damage() {
    // Bout en bout : appuyer sur « Attaque » (bouton nommé) alors qu'un ennemi
    // `attackable` est à portée doit le vaincre (masquer) et augmenter le score.
    // Verrouille aussi la correction du filtre `triggered` (doit exclure les objets
    // invisibles) : un ennemi vaincu ne doit plus pouvoir blesser le joueur ensuite.
    let mut app = AppState::new();
    app.scene = synthetic_damage_scene();
    app.playing = true;
    app.input_state.buttons.insert("Attaque".into());
    // Laisse le temps à la préparation (attack_windup) puis au missile d'arriver
    // (l'attaque n'est plus instantanée, cf. `AttackCharge`/`AttackProjectile`).
    for _ in 0..10 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert_eq!(
        app.score, 1,
        "l'attaque doit vaincre l'ennemi à portée (score += 1)"
    );
    assert!(
        !app.scene
            .objects
            .iter()
            .find(|o| o.name == "Danger")
            .unwrap()
            .visible,
        "l'ennemi vaincu doit devenir invisible"
    );
    // Le joueur ne prend plus de dégâts une fois l'ennemi vaincu, même en restant
    // dessus (sans la correction du filtre `triggered`, le script du danger continuerait
    // à appeler `damage()` malgré `visible = false`).
    app.input_state.buttons.clear();
    let health_after_defeat = app.hud_health;
    for _ in 0..20 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert!(
        !app.lost,
        "un ennemi vaincu ne doit plus pouvoir vaincre le joueur (vie = {:?} → {:?})",
        health_after_defeat, app.hud_health
    );
}

#[test]
fn attack_cooldown_blocks_rapid_refire_but_allows_it_once_expired() {
    // Trouvaille de l'audit gameplay : sans temps de recharge, maintenir le bouton
    // d'attaque défaisait instantanément tout ce qui entrait en portée, sans le
    // moindre risque — le combat était trivial. Verrouille le correctif : une
    // deuxième cible à portée n'est PAS vaincue dans la fenêtre de recharge, mais
    // l'est une fois celle-ci expirée.
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            attack_button: "Attaque".into(),
            attack_range: 50.0,
            attack_cooldown: 0.5,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut cible1 = crate::scene::SceneObject {
        name: "Cible 1".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.5, 0.0)),
        combat: Some(crate::scene::Combat {
            attackable: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    cible1.color = [1.0; 3];
    // Hors de portée au départ : n'est PAS touchée par la première attaque (portée
    // 50 mais la cible 2 démarre à 100 unités). Téléportée à portée juste après.
    let mut cible2 = crate::scene::SceneObject {
        name: "Cible 2".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(100.0, 0.5, 0.0)),
        combat: Some(crate::scene::Combat {
            attackable: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    cible2.color = [1.0; 3];

    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![joueur, cible1, cible2],
        ..Default::default()
    };
    app.playing = true;
    app.input_state.buttons.insert("Attaque".into());

    // Tir sur la cible 1 (seule à portée), puis laisse le temps à la préparation
    // (attack_windup, défaut 0,25 s) et au missile d'arriver (le coup n'est plus
    // instantané, cf. `AttackCharge`/`AttackProjectile`) — sans dépasser la fenêtre
    // de recharge (0,5 s), sans quoi l'assertion suivante (cible 2 protégée par la
    // recharge) ne serait plus valide.
    for _ in 0..8 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert!(
        !app.scene.objects[1].visible,
        "cible 1 vaincue après l'arrivée du missile"
    );
    assert!(
        app.scene.objects[2].visible,
        "cible 2 encore debout (hors de portée)"
    );

    // La cible 2 entre à portée juste après (ex. un monstre qui s'approche) — toujours
    // dans la fenêtre de recharge de 0,5 s : le bouton reste enfoncé mais ne doit PAS
    // tirer un nouveau missile sur elle à cet instant.
    app.scene.objects[2].transform.position = Vec3::new(1.0, 0.5, 0.0);
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    assert!(
        app.scene.objects[2].visible,
        "sans recharge écoulée, aucun missile ne doit être tiré sur la cible 2"
    );

    // Laisse la recharge s'écouler (0,5 s) puis le missile arriver : l'attaque
    // suivante doit alors porter.
    for _ in 0..15 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert!(
        !app.scene.objects[2].visible,
        "la recharge écoulée et le missile arrivé, la cible 2 doit finir par être vaincue"
    );
}

#[test]
fn attack_shows_and_hides_the_visual_fx_anchor() {
    // Une attaque qui porte doit rendre visible l'ancre `is_attack_fx`, la téléporter
    // sur la cible touchée, puis la faire disparaître une fois `attack_flash` retombé
    // à 0 — sinon l'effet resterait affiché indéfiniment après un coup.
    let mut app = AppState::new();
    app.scene = synthetic_damage_scene();
    let target_pos = app
        .scene
        .objects
        .iter()
        .find(|o| o.name == "Danger")
        .unwrap()
        .transform
        .position;
    app.playing = true;
    app.input_state.buttons.insert("Attaque".into());
    // Laisse le temps à la préparation puis au missile d'arriver (le coup n'est plus
    // instantané, cf. `AttackCharge`/`AttackProjectile`).
    for _ in 0..10 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }

    fn fx(app: &AppState) -> crate::scene::SceneObject {
        app.scene
            .objects
            .iter()
            .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
            .unwrap()
            .clone()
    }
    assert!(
        fx(&app).visible,
        "l'ancre FX doit être visible après un coup"
    );
    assert!(
        (fx(&app).transform.position - target_pos).length() < 1e-4,
        "l'ancre FX doit être téléportée sur la cible touchée"
    );
    assert!(app.attack_flash > 0.0);

    app.input_state.buttons.clear();
    for _ in 0..30 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        if app.attack_flash <= 0.0 {
            break;
        }
    }
    assert_eq!(
        app.attack_flash, 0.0,
        "le flash d'attaque doit finir par retomber à 0"
    );
    assert!(
        !fx(&app).visible,
        "l'ancre FX doit disparaître une fois le flash retombé"
    );
}

#[test]
fn auto_run_speed_advances_the_player_with_zero_input() {
    // Cœur du style « Temple Run » : un joueur `auto_run_speed > 0` doit avancer en +Z
    // même sans la moindre entrée (ni joystick, ni clavier) — contrairement au
    // déplacement classique (`move_speed` seul), purement piloté par l'entrée.
    let mut app = AppState::new();
    app.scene = crate::scene::Scene::temple_run_demo();
    app.playing = true;
    // `input_state` reste à ses valeurs par défaut (aucune entrée).
    let z0 = app.player_position().unwrap().z;
    for _ in 0..40 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    let z1 = app.player_position().unwrap().z;
    assert!(
        z1 > z0 + 1.0,
        "la course automatique doit avancer le joueur sans entrée (z0={z0}, z1={z1})"
    );
}

#[test]
fn ai_chaser_actively_closes_distance_to_the_player() {
    // Cœur du « jeu local vs IA » : contrairement aux patrouilles scriptées à
    // trajectoire fixe (prévisibles, évitables par pattern), un `AiChaser` doit
    // se rapprocher réellement de la position courante du joueur, recalculée
    // chaque frame — une poursuite réactive.
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(30.0, 1.0, 30.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let mut chaser = crate::scene::SceneObject {
        name: "Chasseur".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(10.0, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 3.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    chaser.color = [1.0; 3];

    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, joueur, chaser],
        ..Default::default()
    };
    app.playing = true;
    let dist0 = (app.scene.objects[2].transform.position - Vec3::new(0.0, 1.0, 0.0)).length();
    for _ in 0..60 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    let player_pos = app.player_position().unwrap();
    let dist1 = (app.scene.objects[2].transform.position - player_pos).length();
    assert!(
        dist1 < dist0 - 1.0,
        "le chasseur doit se rapprocher du joueur (dist0={dist0}, dist1={dist1})"
    );
}

/// Vérifie que sur 3 chasseurs visant la même cible, seuls les
/// `MAX_ACTIVE_CHASERS_PER_TARGET` (2) plus proches avancent réellement ;
/// le 3e reste sur place ce tick (cf. GAMEDESIGN_EN_LIGNE.md).
#[test]
fn only_the_nearest_chasers_up_to_the_cap_advance_on_a_single_target() {
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(60.0, 1.0, 60.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        color: [1.0; 3],
        ..Default::default()
    };
    // Trois chasseurs à distances croissantes de la même cible : le
    // troisième (le plus loin) doit être celui relégué par le plafond.
    let chaser_at = |x: f32| crate::scene::SceneObject {
        name: format!("Chasseur {x}"),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(x, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 3.0,
            ..Default::default()
        }),
        color: [1.0; 3],
        ..Default::default()
    };
    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![
            sol,
            joueur,
            chaser_at(6.0),
            chaser_at(10.0),
            chaser_at(14.0),
        ],
        ..Default::default()
    };
    app.playing = true;
    let start: Vec<Vec3> = (2..5)
        .map(|i| app.scene.objects[i].transform.position)
        .collect();
    for _ in 0..30 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    let moved = |i: usize| (app.scene.objects[i].transform.position - start[i - 2]).length();
    assert!(
        moved(2) > 0.5,
        "le chasseur le plus proche doit avancer : déplacement {}",
        moved(2)
    );
    assert!(
        moved(3) > 0.5,
        "le 2e chasseur le plus proche doit aussi avancer : déplacement {}",
        moved(3)
    );
    assert!(
        moved(4) < 0.2,
        "au-delà du plafond, le 3e chasseur ne doit pas avancer ce tick : déplacement {}",
        moved(4)
    );
}

/// Même après le plafond par cible, avec une seule cible réseau vivante
/// connectée, les chasseurs finissent par tous converger (le plafond étale
/// l'arrivée dans le temps, il ne l'empêche pas). Vérifie
/// qu'un chasseur **hors de portée de détection** (`CHASER_DETECT_RANGE`)
/// reste totalement immobile face à un unique joueur réseau, même s'il
/// serait autrement le seul/le plus proche (donc jamais relégué par le
/// plafond). Un joueur **réseau**, pas local : la portée de détection est
/// volontairement limitée au cas réseau (cf. le commentaire sur
/// `CHASER_DETECT_RANGE` dans la boucle de pilotage IA) pour ne pas casser
/// le ring-out de `Scene::brawl_demo` en solo.
#[test]
fn a_chaser_beyond_detection_range_never_moves_towards_a_lone_network_player() {
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(60.0, 1.0, 60.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let gabarit = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        color: [1.0; 3],
        ..Default::default()
    };
    // Bien au-delà de CHASER_DETECT_RANGE (9 m) : seule cible sur la carte,
    // donc jamais relégué par le plafond — sans la portée de détection, il
    // se rapprocherait quand même.
    let chaser = crate::scene::SceneObject {
        name: "Chasseur lointain".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(20.0, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 3.0,
            ..Default::default()
        }),
        color: [1.0; 3],
        ..Default::default()
    };
    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, gabarit, chaser],
        ..Default::default()
    };
    app.hide_local_player_template();
    app.spawn_network_player(1, PlayerClass::Assault);
    app.playing = true;
    let start = app.scene.objects[2].transform.position;
    for _ in 0..60 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    let moved = (app.scene.objects[2].transform.position - start).length();
    assert!(
        moved < 0.2,
        "un chasseur hors de portée de détection ne doit pas se rapprocher \
             de l'unique joueur réseau, aussi loin soit-il : déplacement {moved}"
    );
}

/// Construit une scène minimale sol + joueur immobile + un unique `AiChaser`
/// (à `chaser_x` du joueur, sur l'archétype donné), fait tourner `steps` ticks
/// de 0.05 s, et renvoie la distance parcourue par le chasseur. Isole chaque
/// scénario dans son propre `AppState` (un seul chasseur, une seule cible) pour
/// ne jamais retomber sur `MAX_ACTIVE_CHASERS_PER_TARGET`, qui plafonnerait à 2
/// des chasseurs multiples visant le même joueur et fausserait la comparaison.
fn chaser_distance_moved(chaser_x: f32, archetype: crate::scene::Archetype, steps: u32) -> f32 {
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(60.0, 1.0, 60.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        color: [1.0; 3],
        ..Default::default()
    };
    let chaser = crate::scene::SceneObject {
        name: format!("{archetype:?}"),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(chaser_x, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 3.0,
            archetype,
        }),
        color: [1.0; 3],
        ..Default::default()
    };
    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, joueur, chaser],
        ..Default::default()
    };
    app.playing = true;
    let start = app.scene.objects[2].transform.position;
    for _ in 0..steps {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    (app.scene.objects[2].transform.position - start).length()
}

/// GDD_MMORPG.md §5.4, archétype Furtive : « éveil réduit (< 9 m) », appliqué
/// **même en solo** (contrairement à `CHASER_DETECT_RANGE`, réseau uniquement,
/// cf. `a_chaser_beyond_detection_range_never_moves_towards_a_lone_network_player`).
#[test]
fn furtive_archetype_stays_asleep_until_the_player_enters_its_shorter_wake_radius() {
    // 7 m : sous CHASER_DETECT_RANGE (9 m, non appliqué en solo de toute façon),
    // mais au-delà de FURTIVE_DETECT_RANGE (5 m) — doit rester endormie.
    let asleep = chaser_distance_moved(7.0, crate::scene::Archetype::Furtive, 30);
    assert!(
        asleep < 0.2,
        "une Furtive hors de sa portée d'éveil réduite ne doit pas bouger : \
             déplacement {asleep}"
    );

    // 3 m : sous FURTIVE_DETECT_RANGE (5 m) — doit foncer, et plus vite qu'une
    // Traqueuse standard partie de la même distance (vitesse accrue éveillée).
    let furtive_awake = chaser_distance_moved(3.0, crate::scene::Archetype::Furtive, 30);
    let traqueuse = chaser_distance_moved(3.0, crate::scene::Archetype::Traqueuse, 30);
    assert!(
        furtive_awake > 0.5,
        "une fois dans sa portée d'éveil, la Furtive doit se rapprocher : \
             déplacement {furtive_awake}"
    );
    assert!(
        furtive_awake > traqueuse,
        "éveillée, la Furtive doit avancer plus vite qu'une Traqueuse standard : \
             {furtive_awake} <= {traqueuse}"
    );
}

/// Phase O Sprint 1 (`sprint2audijeu0718.md`, GDD §10.4 rang 3) : `Sfx::CreatureWake`
/// doit être signalé exactement une fois par éveil, au tick où la Furtive franchit
/// `FURTIVE_DETECT_RANGE` — pas à chaque frame tant qu'elle reste éveillée, et pas du
/// tout tant qu'elle reste endormie. `furtive_awake` (le registre qui pilote ce
/// signal, cf. sa doc) est la seule sortie observable ici sans mocker `Audio`.
#[test]
fn a_furtive_is_marked_awake_exactly_once_when_it_crosses_its_wake_radius() {
    let sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(60.0, 1.0, 60.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    let joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    // 7 m : hors FURTIVE_DETECT_RANGE (5 m) — endormie au départ.
    let chaser = crate::scene::SceneObject {
        name: "Furtive".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(7.0, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 3.0,
            archetype: crate::scene::Archetype::Furtive,
        }),
        ..Default::default()
    };
    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, joueur, chaser],
        ..Default::default()
    };
    app.playing = true;
    for _ in 0..10 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert!(
        app.furtive_awake.is_empty(),
        "endormie hors de portée, aucun éveil ne doit être enregistré : {:?}",
        app.furtive_awake
    );

    // Rapproche-la manuellement sous FURTIVE_DETECT_RANGE (3 m) puis avance : elle
    // doit franchir la portée d'éveil et être enregistrée exactement une fois.
    app.scene.objects[2].transform.position = Vec3::new(3.0, 0.5, 0.0);
    for _ in 0..30 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert_eq!(
        app.furtive_awake,
        std::collections::HashSet::from([2]),
        "la Furtive doit être marquée éveillée après être entrée dans sa portée"
    );

    // D'autres ticks éveillée ne doivent pas dupliquer l'entrée (un `HashSet` ne le
    // permettrait de toute façon pas, mais confirme qu'aucun autre indice n'est
    // ajouté par erreur au passage).
    for _ in 0..30 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert_eq!(app.furtive_awake, std::collections::HashSet::from([2]));
}

/// GDD_MMORPG.md §5.4 : les 4 archétypes doivent être « distinguables en Play »
/// — au minimum par leur vitesse de poursuite effective une fois la même
/// distance/temps de simulation appliqués. Vérifie l'ordre attendu
/// Colosse (ralenti) < Traqueuse (standard) < Meute (accéléré), cf.
/// `Archetype::speed_multiplier`.
#[test]
fn creature_archetypes_produce_visibly_different_chase_speeds() {
    let colosse = chaser_distance_moved(4.0, crate::scene::Archetype::Colosse, 20);
    let traqueuse = chaser_distance_moved(4.0, crate::scene::Archetype::Traqueuse, 20);
    let meute = chaser_distance_moved(4.0, crate::scene::Archetype::Meute, 20);
    assert!(
        colosse < traqueuse,
        "le Colosse doit avancer plus lentement que la Traqueuse : {colosse} >= {traqueuse}"
    );
    assert!(
        traqueuse < meute,
        "la Meute doit avancer plus vite que la Traqueuse : {traqueuse} >= {meute}"
    );
}

/// GAMEDESIGN_EN_LIGNE.md §3.2 (audit) : avant ce correctif, `chase_target`
/// était un point unique (`self.player_position()`) — sur un serveur
/// headless avec plusieurs joueurs réseau, cela désignait toujours le
/// premier joueur à avoir rejoint (le premier objet visible piloté trouvé
/// dans `scene.objects`), jamais le second même s'il était bien plus
/// proche. Un monstre doit désormais poursuivre le joueur réseau **vivant**
/// le plus proche, recalculé chaque frame.
#[test]
fn ai_chaser_pursues_the_nearest_network_player_not_just_the_first_joined() {
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(60.0, 1.0, 60.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut chaser = crate::scene::SceneObject {
        name: "Chasseur".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 0.5, -20.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 3.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    chaser.color = [1.0; 3];

    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, joueur, chaser],
        ..Default::default()
    };
    app.playing = true;
    app.hide_local_player_template();
    let p1 = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
    let p2 = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
    let chaser_idx = 2; // sol=0, joueur(masqué)=1, chasseur=2, puis p1/p2 ajoutés ensuite.
    // Repositionne explicitement les deux joueurs (plutôt que de dépendre
    // de la géométrie de spawn de `spawn_network_player`, qui les place
    // proches l'un de l'autre sans garantir lequel est le plus près du
    // chasseur) : p1 loin de tout, p2 juste devant le chasseur.
    app.scene.objects[p1].transform.position = Vec3::new(0.0, 1.0, 30.0);
    app.scene.objects[p2].transform.position = Vec3::new(0.0, 1.0, -15.0);
    // Reconstruit la physique après avoir déplacé les objets « à la main » :
    // sans ça, les corps rigides (créés par `spawn_network_player` avec
    // l'ancienne position) écraseraient ce repositionnement dès le premier
    // pas de simulation (`Physics::step` recopie la pose du corps rigide
    // dans `transform`, jamais l'inverse).
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
    let dist_to_p2_before = (app.scene.objects[chaser_idx].transform.position
        - app.scene.objects[p2].transform.position)
        .length();

    for _ in 0..60 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }

    let chaser_pos = app.scene.objects[chaser_idx].transform.position;
    let dist_to_p1 = (chaser_pos - app.scene.objects[p1].transform.position).length();
    let dist_to_p2 = (chaser_pos - app.scene.objects[p2].transform.position).length();
    assert!(
        dist_to_p2 < dist_to_p2_before - 1.0,
        "le chasseur doit se rapprocher du joueur réseau le plus proche (p2) : \
             avant={dist_to_p2_before}, après={dist_to_p2}"
    );
    assert!(
        dist_to_p2 < dist_to_p1,
        "le chasseur doit finir plus proche de p2 (le plus proche au départ) que de \
             p1 (le premier à avoir rejoint) : dist_p1={dist_to_p1}, dist_p2={dist_to_p2}"
    );
}

#[test]
fn wave_system_reveals_next_wave_then_wins_on_the_last() {
    // 2 manches synthétiques d'un seul monstre chacune : ne doit révéler la manche 2
    // qu'une fois la manche 1 vidée, et gagner une fois la manche 2 vidée à son tour.
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            attack_button: "Attaque".into(),
            attack_range: 50.0, // portée large : le test cible la logique de manches, pas la précision d'attaque.
            attack_cooldown: 0.0, // pas de recharge : le test cible les manches, pas le rythme de combat.
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(30.0, 1.0, 30.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    let mut m1 = crate::scene::SceneObject {
        name: "Monstre Vague1".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 1.0,
            ..Default::default()
        }),
        combat: Some(crate::scene::Combat {
            attackable: true,
            wave: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    m1.color = [1.0; 3];
    let mut m2 = crate::scene::SceneObject {
        name: "Monstre Vague2".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(-5.0, 0.5, 0.0)),
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 1.0,
            ..Default::default()
        }),
        combat: Some(crate::scene::Combat {
            attackable: true,
            wave: 2,
            ..Default::default()
        }),
        ..Default::default()
    };
    m2.color = [1.0; 3];

    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, joueur, m1, m2],
        ..Default::default()
    };
    app.playing = true;
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play(); // entrée en Play : `init_waves` doit s'exécuter.

    assert_eq!(app.wave, 1, "démarre à la manche 1");
    assert!(
        app.scene.objects[2].visible,
        "manche 1 : le monstre 1 est révélé"
    );
    assert!(
        !app.scene.objects[3].visible,
        "manche 1 : le monstre 2 reste masqué"
    );

    // Attaque : tire sur le monstre de la manche 1 (portée large, toujours à portée),
    // puis laisse le temps au missile d'arriver (le coup n'est plus instantané, cf.
    // `AttackProjectile`) et à `update_waves` de détecter la manche vidée.
    app.input_state.buttons.insert("Attaque".into());
    for _ in 0..20 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        if app.wave == 2 {
            // S'arrête dès la révélation de la manche 2, avant qu'un nouveau missile
            // (bouton toujours enfoncé) n'ait le temps de la vaincre aussi.
            break;
        }
    }
    app.input_state.buttons.clear();

    assert_eq!(app.wave, 2, "la manche 1 vidée doit révéler la manche 2");
    assert!(
        app.scene.objects[3].visible,
        "manche 2 : le monstre 2 est révélé"
    );
    assert!(
        app.win_time.is_none(),
        "pas encore gagné, la manche 2 reste à vider"
    );

    // Vainc le monstre de la manche 2 : dernière manche ⇒ victoire.
    app.input_state.buttons.insert("Attaque".into());
    for _ in 0..20 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    app.input_state.buttons.clear();

    assert!(
        app.win_time.is_some(),
        "toutes les manches vidées ⇒ victoire"
    );
}

/// Phase C (Sprint 6, `sprint10audit.md`) : contrairement à `RoundObjective::Vagues`
/// (test ci-dessus), vider l'unique manche en `Survie` doit la reboucler plutôt que
/// de gagner — la victoire ne se déclenche qu'au chrono (`SURVIE_DURATION_SECS`),
/// indépendamment de l'état des monstres.
#[test]
fn survie_mode_loops_the_wave_then_wins_once_the_timer_elapses() {
    let mut monstre = crate::scene::SceneObject {
        name: "Monstre".into(),
        mesh: crate::scene::MeshKind::Sphere,
        combat: Some(crate::scene::Combat {
            attackable: true,
            wave: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    monstre.color = [1.0; 3];

    let mut app = AppState::new();
    app.objective = crate::app::multiplayer::RoundObjective::Survie;
    app.scene = crate::scene::Scene {
        objects: vec![monstre],
        ..Default::default()
    };
    app.init_waves();
    assert_eq!(app.wave, 1, "démarre à la manche 1 comme Vagues");
    assert!(app.scene.objects[0].visible, "manche 1 révélée");

    // Vide la manche 1 (équivalent d'un monstre vaincu) : en Survie, ça ne
    // doit pas déclencher `win_time`, juste reboucler sur la manche 1.
    app.scene.objects[0].visible = false;
    app.update_round(1.0 / 60.0);
    assert!(
        app.win_time.is_none(),
        "vider l'unique manche ne doit pas gagner la partie en Survie"
    );
    assert_eq!(
        app.wave, 1,
        "reboucle sur la manche 1, pas de manche 2 à révéler"
    );
    assert!(
        app.scene.objects[0].visible,
        "la manche 1 est re-révélée après avoir bouclé"
    );

    // Chrono écoulé : la victoire doit se déclencher, peu importe l'état
    // des monstres visibles (la manche 1, toujours pleine ici, le prouve).
    app.time = 200.0; // > SURVIE_DURATION_SECS (180 s)
    app.update_round(1.0 / 60.0);
    assert!(
        app.win_time.is_some(),
        "le chrono écoulé doit déclencher la victoire en Survie"
    );
}

/// Phase C (Sprint 8, `sprint10audit.md`) : `Boss` est décrit au GDD §4 comme
/// « dernière vague : une créature unique » — une scène Boss n'a donc qu'une
/// manche contenant le boss, et `update_round` doit retomber sur le comportement
/// `Vagues` (victoire à la dernière manche vidée) : c'est exactement « boss
/// vaincu », sans logique dédiée à écrire.
#[test]
fn update_round_boss_wins_when_its_single_wave_is_cleared() {
    let mut boss = crate::scene::SceneObject {
        name: "Boss".into(),
        mesh: crate::scene::MeshKind::Sphere,
        combat: Some(crate::scene::Combat {
            attackable: true,
            wave: 1,
            hp: 12,
            ..Default::default()
        }),
        ..Default::default()
    };
    boss.color = [1.0; 3];

    let mut app = AppState::new();
    app.objective = crate::app::multiplayer::RoundObjective::Boss;
    app.scene = crate::scene::Scene {
        objects: vec![boss],
        ..Default::default()
    };
    app.init_waves();
    app.scene.objects[0].visible = false; // le boss vaincu vide l'unique manche

    app.update_round(1.0 / 60.0);
    assert!(
        app.win_time.is_some(),
        "la mort du boss (dernière et unique manche vidée) doit gagner la partie"
    );
}

/// Phase C (Sprint 7, `sprint10audit.md`) : le convoi avance en ligne droite vers
/// sa destination et la victoire se déclenche dès qu'il en est assez proche,
/// indépendamment de tout système de manches (`self.wave` reste à 0 ici).
#[test]
fn update_round_escorte_wins_once_the_convoy_reaches_its_destination() {
    let convoy = crate::scene::SceneObject {
        name: "Convoi".into(),
        mesh: crate::scene::MeshKind::Cube,
        convoy: Some(crate::scene::Convoy {
            destination: glam::Vec3::new(10.0, 0.0, 0.0),
            speed: 5.0,
        }),
        ..Default::default()
    };

    let mut app = AppState::new();
    app.objective = crate::app::multiplayer::RoundObjective::Escorte;
    app.scene = crate::scene::Scene {
        objects: vec![convoy],
        ..Default::default()
    };

    // Encore loin de la destination : pas de victoire, le convoi a avancé.
    app.update_round(1.0);
    assert!(app.win_time.is_none(), "trop loin pour arriver en un pas");
    assert!(
        app.scene.objects[0].transform.position.x > 0.0,
        "le convoi doit avancer vers sa destination"
    );

    // Assez de pas pour couvrir la distance restante : victoire.
    for _ in 0..10 {
        app.update_round(1.0);
    }
    assert!(
        app.win_time.is_some(),
        "le convoi doit finir par déclencher la victoire en approchant sa destination"
    );
}

/// Phase C (Sprint 7) : un convoi vaincu (`Combat::hp` à 0, masqué comme toute
/// autre cible d'attaque, cf. `Scene::damage_attackable`) doit compter comme une
/// défaite de salon même si des joueurs réseau sont encore vivants — contrairement
/// aux autres modes, où seule la mort de tous les joueurs compte
/// (`AppState::is_room_lost`).
#[test]
fn is_room_lost_true_when_the_escorte_convoy_is_destroyed_even_with_a_living_player() {
    let convoy = crate::scene::SceneObject {
        name: "Convoi".into(),
        mesh: crate::scene::MeshKind::Cube,
        visible: false, // vaincu
        convoy: Some(crate::scene::Convoy {
            destination: glam::Vec3::new(10.0, 0.0, 0.0),
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut app = AppState::new();
    app.objective = crate::app::multiplayer::RoundObjective::Escorte;
    app.scene = crate::scene::Scene {
        objects: vec![convoy],
        ..Default::default()
    };
    let player_id = 1;
    app.network_players.insert(player_id, 0);
    app.network_health.insert(player_id, 100.0);

    assert!(
        app.is_room_lost(),
        "convoi détruit ⇒ salon perdu, même joueur(s) vivant(s)"
    );
}

#[test]
fn zombies_demo_attack_range_stays_close_to_monster_bite_reach() {
    // Audit gameplay : la portée d'attaque totale (attack_range + rayon du monstre)
    // est un cercle qui **contient toujours** la boîte de morsure du monstre (rayon
    // ≈ son propre rayon) dès que `attack_range > 0` — un joueur qui fonce droit sur
    // un monstre gagnera donc structurellement la course à l'engagement, quelle que
    // soit sa vitesse. `attack_range` ne peut pas éliminer ce biais en 1 contre 1
    // frontal, seulement en réduire la marge (le vrai risque vient d'affronter
    // plusieurs monstres à la fois pendant la recharge). L'ancienne valeur (1,5 m)
    // donnait une marge de sécurité énorme (jusqu'à 4-5× le rayon du plus petit
    // monstre) ; verrouille qu'elle reste modeste désormais.
    let s = crate::scene::Scene::zombies_demo();
    let ctrl = s
        .objects
        .iter()
        .find_map(|o| o.controller.as_ref())
        .expect("un joueur pilotable");
    let smallest_monster_r = s
        .objects
        .iter()
        .filter(|o| o.ai_chaser.is_some())
        .map(|o| o.transform.scale.max_element() * 0.5)
        .fold(f32::INFINITY, f32::min);
    assert!(
        ctrl.attack_range <= smallest_monster_r + 0.5,
        "marge de sécurité trop généreuse : attack_range={} vs rayon du plus petit \
             monstre={smallest_monster_r} (marge > 0,5 m)",
        ctrl.attack_range
    );
}

#[test]
fn attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing() {
    // Suite de l'audit gameplay : `attack_at` vainquait TOUTES les cibles à portée en
    // un seul appel (balayage de zone). Une expérimentation poussée (3 archétypes
    // convergeant en cercle serré sur un joueur immobile qui attaque en continu) a
    // montré qu'ils entraient dans le rayon de mise à mort de façon quasi synchronisée
    // — leur taille (donc leur propre rayon, qui élargit d'autant le rayon de mise à
    // mort perçu) compense presque exactement leur différence de vitesse. Résultat :
    // un groupe entier disparaissait en un seul coup, sans qu'aucun n'ait jamais mordu.
    // `attack_at` ne vainc désormais que la cible la plus proche : un groupe de 3
    // exige donc 3 coups (et donc 3 fenêtres de recharge), pas un seul.
    //
    // Limite honnête, documentée plutôt que masquée par un test fragile : ceci ne
    // garantit pas qu'un joueur qui reste immobile et attaque prendra des dégâts —
    // sans temps de préparation sur l'attaque, la portée d'attaque englobera toujours
    // la portée de morsure d'un monstre qui approche en ligne droite (cf.
    // `zombies_demo_attack_range_stays_close_to_monster_bite_reach`), donc gagner la
    // course à l'engagement 1 contre 1 reste structurellement favorable au joueur.
    // Un vrai risque garanti demanderait un temps de préparation sur l'attaque
    // (fenêtre de vulnérabilité avant que le coup ne porte) — hors du périmètre de ce
    // sprint, noté dans audit_sprint.md pour une prochaine itération.
    let mut s = crate::scene::Scene::default();
    s.objects.push(crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    });
    for n in 0..3 {
        let mut m = crate::scene::SceneObject {
            name: format!("Monstre {n}"),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.2 * n as f32, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        m.color = [1.0; 3];
        s.objects.push(m);
    }
    // Les 3 sont groupés à moins de 0,5 m les uns des autres, largement à portée
    // d'une seule attaque à grand rayon.
    let hit = s.attack_at(Vec3::new(0.2, 0.5, 0.0), 5.0);
    assert_eq!(
        hit.len(),
        1,
        "une attaque ne vainc qu'une seule cible, pas tout le groupe"
    );
    let still_visible = s.objects[1..].iter().filter(|o| o.visible).count();
    assert_eq!(
        still_visible, 2,
        "les 2 autres cibles du groupe doivent survivre à ce coup"
    );
}

#[test]
fn attack_mode_zone_clears_a_whole_cluster_in_one_swing() {
    // Contrepoint direct de `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` :
    // ce dernier documente que le mode par défaut (`Single`) ne vainc qu'une cible à
    // la fois, précisément pour ne pas trivialiser un groupe convergent. Le mode
    // `AttackMode::Zone` (Marteau, cf. `Weapon::mode`) doit au contraire vaincre TOUT
    // le groupe d'un coup — c'est le point de payer une préparation/recharge plus
    // longues (cf. `WEAPONS`) : jamais d'état intermédiaire « 1 ou 2 des 3 vaincus ».
    let mut s = crate::scene::Scene::default();
    s.objects.push(crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    });
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.2, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            attack_button: "Attaque".into(),
            attack_range: 5.0,
            attack_cooldown: 1.0,
            attack_windup: 0.1,
            attack_mode: crate::scene::AttackMode::Zone,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    s.objects.push(joueur);
    for n in 0..3 {
        let mut m = crate::scene::SceneObject {
            name: format!("Monstre {n}"),
            mesh: crate::scene::MeshKind::Sphere,
            transform: crate::scene::Transform::from_pos(Vec3::new(0.2 * n as f32, 0.5, 0.0)),
            combat: Some(crate::scene::Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        m.color = [1.0; 3];
        s.objects.push(m);
    }

    let mut app = AppState::new();
    app.scene = s;
    app.playing = true;
    app.input_state.attack = true;
    let mut seen_counts: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for _ in 0..30 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let visible = app.scene.objects[2..].iter().filter(|o| o.visible).count();
        seen_counts.insert(visible);
        if visible == 0 {
            break;
        }
    }
    assert!(
        seen_counts.contains(&0),
        "le mode Zone doit finir par vaincre tout le groupe"
    );
    assert!(
        !seen_counts.contains(&1) && !seen_counts.contains(&2),
        "jamais d'état intermédiaire \"1 ou 2 vaincus\" : la résolution doit toucher \
             les 3 cibles du groupe dans le même appel, pas une par une (vu={seen_counts:?})"
    );
}

/// Duel 1 contre 1 : sol statique, joueur pilotable (attaque à préparation) et un
/// monstre-chasseur mordeur à 1 m. Le monstre a un **corps physique** (via
/// `ai_chaser` + `visible`, cf. `Physics::build`) : contrairement aux dangers
/// statiques de `synthetic_damage_scene`, sa collision solide repousse le joueur —
/// c'est précisément la configuration où la morsure « centre dans l'AABB » échouait.
fn duel_1v1_scene() -> crate::scene::Scene {
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            attack_button: "Attaque".into(),
            attack_range: 6.0,
            attack_cooldown: 0.5,
            attack_windup: 0.25,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(30.0, 1.0, 30.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    // À 1 m (rayon de morsure par défaut ≈ 0,5 m) et 4 m/s : atteint sa portée de
    // morsure en (1 - 0,5) / 4 = 0,125 s — avant la fin des 0,25 s de préparation,
    // donc avant même que le missile ne soit tiré.
    let mut monstre = crate::scene::SceneObject {
        name: "Monstre".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(1.0, 0.5, 0.0)),
        trigger: true,
        ai_chaser: Some(crate::scene::AiChaser {
            speed: 4.0,
            ..Default::default()
        }),
        combat: Some(crate::scene::Combat {
            attackable: true,
            ..Default::default()
        }),
        script: "if obj.triggered then damage(5.0 * dt) end".into(),
        ..Default::default()
    };
    monstre.color = [1.0; 3];

    crate::scene::Scene {
        objects: vec![sol, joueur, monstre],
        ..Default::default()
    }
}

#[test]
fn chasing_monster_with_solid_body_can_bite_the_player_on_contact() {
    // Régression du bug racine découvert par l'audit : la morsure testait « centre
    // du joueur dans l'AABB du monstre », or les colliders solides (joueur et
    // chasseur ont tous deux un corps rigide) empêchent toute interpénétration —
    // un monstre-chasseur ne mordait donc *jamais*, même en contact continu. Le
    // test de déclenchement est désormais une **intersection d'AABB** (cf.
    // `Scene::world_aabb_intersects`) : le contact suffit.
    let mut app = AppState::new();
    app.scene = duel_1v1_scene();
    app.playing = true;
    // Aucune attaque : on isole la collision physique pure (le joueur ne se défend
    // pas, le monstre doit finir par le mordre).
    app.input_state.attack = false;

    let mut took_damage = false;
    for _ in 0..40 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        if app.hud_health.is_some() {
            took_damage = true;
            break;
        }
    }
    assert!(
        took_damage,
        "un monstre-chasseur au corps solide doit pouvoir mordre au contact, \
             malgré la répulsion physique qui interdit l'interpénétration des centres"
    );
}

#[test]
fn attack_windup_finally_guarantees_risk_in_a_1v1() {
    // Clôt la limite documentée à répétition dans l'audit (le temps de vol du
    // missile seul ne suffisait pas à garantir un risque en 1 contre 1, cf.
    // `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing` et
    // `attack_is_a_missile_with_travel_time_not_an_instant_hit`) : un temps de
    // préparation (`Controller::attack_windup`) *avant même que le missile ne
    // parte* fonctionne, lui, indépendamment de la vitesse du missile — un monstre
    // déjà proche de sa propre portée de morsure au moment du tir peut mordre
    // pendant la préparation, avant qu'aucun projectile n'existe.
    let mut app = AppState::new();
    app.scene = duel_1v1_scene();
    app.playing = true;
    // Attaque maintenue dès la première frame : la préparation démarre aussitôt.
    app.input_state.attack = true;

    let mut bitten_before_kill = false;
    for _ in 0..40 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        if app.hud_health.is_some() && app.scene.objects[2].visible {
            bitten_before_kill = true;
        }
        if !app.scene.objects[2].visible {
            break;
        }
    }
    assert!(
        !app.scene.objects[2].visible,
        "le missile doit finir par vaincre le monstre (sinon le duel ne se résout pas)"
    );
    assert!(
        bitten_before_kill,
        "un monstre déjà proche de sa portée de morsure doit pouvoir mordre pendant \
             la préparation de l'attaque, avant que le missile ne le vainque — gagner \
             un 1 contre 1 doit coûter de la vie"
    );
}

#[test]
fn attack_is_a_missile_with_travel_time_not_an_instant_hit() {
    // L'attaque est désormais un missile homing avec un temps de vol (cf.
    // `AttackProjectile`), pas une résolution instantanée au tir : rend le coup
    // lisible en 3D (le missile se voit voyager, pas juste « la cible disparaît »).
    //
    // Limite honnête, re-vérifiée ici plutôt que survendue : le temps de vol NE
    // garantit PAS à lui seul un risque en 1 contre 1 — un missile homing tiré dès
    // l'entrée en portée arrive quasi toujours avant qu'un monstre qui fonce en
    // ligne droite n'ait eu le temps d'atteindre sa propre (bien plus courte) portée
    // de morsure, sauf à rendre le missile déraisonnablement lent. Le vrai risque
    // reste celui déjà documenté : affronter plusieurs monstres à la fois pendant la
    // recharge (cf. `attack_at_clears_a_cluster_one_target_at_a_time_not_in_one_swing`).
    // Ce test vérifie donc uniquement ce que le missile change réellement : un vol
    // progressif et homing, pas un « tout ou rien » au moment du tir.
    let mut joueur = crate::scene::SceneObject {
        name: "Joueur".into(),
        mesh: crate::scene::MeshKind::Capsule,
        transform: crate::scene::Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
        controller: Some(crate::scene::Controller {
            input: true,
            attack_button: "Attaque".into(),
            attack_range: 6.0,
            attack_cooldown: 0.5,
            ..Default::default()
        }),
        ..Default::default()
    };
    joueur.color = [1.0; 3];
    let mut sol = crate::scene::SceneObject {
        name: "Sol".into(),
        mesh: crate::scene::MeshKind::Plane,
        transform: crate::scene::Transform::from_pos(Vec3::ZERO)
            .with_scale(Vec3::new(30.0, 1.0, 30.0)),
        physics: crate::runtime::physics::PhysicsKind::Static,
        ..Default::default()
    };
    sol.color = [1.0; 3];
    // À 5 m : à portée du tir (6 m), le missile doit voyager plusieurs frames avant
    // d'arriver (pas de patrouille/chasse ici : isole le temps de vol lui-même).
    let mut monstre = crate::scene::SceneObject {
        name: "Monstre".into(),
        mesh: crate::scene::MeshKind::Sphere,
        transform: crate::scene::Transform::from_pos(Vec3::new(5.0, 0.5, 0.0)),
        combat: Some(crate::scene::Combat {
            attackable: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    monstre.color = [1.0; 3];
    let mut fx = crate::scene::SceneObject {
        name: "FX Attaque".into(),
        mesh: crate::scene::MeshKind::Sphere,
        combat: Some(crate::scene::Combat {
            is_attack_fx: true,
            ..Default::default()
        }),
        visible: false,
        ..Default::default()
    };
    fx.color = [1.0; 3];

    let mut app = AppState::new();
    app.scene = crate::scene::Scene {
        objects: vec![sol, joueur, monstre, fx],
        ..Default::default()
    };
    app.playing = true;
    app.input_state.attack = true;

    // Quelques pas : couvre la préparation (attack_windup, 0,25 s par défaut) sans
    // atteindre le temps de vol du missile (5 m à 10 m/s ≈ 0,5 s) — le monstre à 5 m
    // ne doit pas être vaincu si tôt.
    for _ in 0..6 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
    }
    assert!(
        app.scene.objects[2].visible,
        "le monstre à 5 m ne doit pas être vaincu dès la préparation/le tir : le \
             missile met du temps à arriver"
    );
    let fx_after_launch = app
        .scene
        .objects
        .iter()
        .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
        .map(|o| o.transform.position);

    // Quelques frames plus tard : le missile a progressé (pas téléporté).
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    let fx_mid_flight = app
        .scene
        .objects
        .iter()
        .find(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
        .map(|o| o.transform.position);
    assert_ne!(
        fx_after_launch, fx_mid_flight,
        "l'ancre visuelle doit progresser vers la cible, pas rester figée"
    );

    // Laisse le temps au missile d'arriver.
    for _ in 0..20 {
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        if !app.scene.objects[2].visible {
            break;
        }
    }
    assert!(
        !app.scene.objects[2].visible,
        "le missile doit finir par atteindre sa cible"
    );
}

#[test]
fn damage_triggers_flash_that_fades_and_resets_on_stop() {
    // Retour visuel du coup : `damage_flash` doit monter à 1.0 dès la première baisse
    // de vie détectée, puis décroître frame après frame (pas rester bloqué au pic).
    let mut app = AppState::new();
    app.scene = synthetic_damage_scene();
    app.playing = true;
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    // Le pic (1.0) est déclenché par le sim_step qui détecte le coup, mais cette même
    // frame applique déjà une frame de décroissance ensuite (comportement voulu : le
    // flash commence à s'estomper dès la frame du coup) — d'où la marge, pas `== 1.0`.
    let peak = app.damage_flash;
    assert!(
        peak > 0.8,
        "un coup doit déclencher un pic net du flash : {peak}"
    );
    // Sort du contact (sinon chaque frame retriggerait le pic à 1.0) pour vérifier la
    // décroissance en l'absence de nouveaux coups. Reconstruit le corps physique à sa
    // nouvelle position : sinon le pas de physique du même appel le ramènerait vers
    // l'ancienne pose (le corps rigide, lui, n'a pas bougé) et le contact reprendrait.
    if let Some(j) = app
        .scene
        .objects
        .iter_mut()
        .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
    {
        j.transform.position = Vec3::new(50.0, 0.5, 50.0);
    }
    app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    assert!(
        app.damage_flash < peak,
        "le flash doit continuer à décroître frame après frame hors contact"
    );
    // Sortir de Play remet tout à zéro (pas de flash résiduel visible en édition).
    app.playing = false;
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    assert_eq!(
        app.damage_flash, 0.0,
        "le flash est effacé à la sortie de Play"
    );
}

#[test]
fn controller_demo_lava_boil_script_preserves_collision_scale() {
    // La lave a un script de « bouillonnement » (pulsation de couleur) ajouté après coup ;
    // il ne doit surtout pas toucher à l'échelle Y, qui encode l'épaisseur de collision
    // nécessaire pour que la zone mortelle détecte un joueur debout (cf. test dédié dans
    // scene::tests). Une régression ici rendrait la lave inoffensive en silence.
    let scene = crate::scene::Scene::controller_demo();
    let lave = scene
        .objects
        .iter()
        .find(|o| o.name == "Lave")
        .expect("la lave existe");
    assert!(!lave.script.trim().is_empty(), "la lave doit être animée");
    let lua = Lua::new();
    let func = lua.load(&lave.script).into_function().unwrap();
    let mut t = lave.transform;
    let mut col = lave.color;
    let input = PlayerInput::default();
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        3.7,
        &input,
        false,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        t.scale, lave.transform.scale,
        "le script de la lave ne doit pas modifier l'échelle (collision)"
    );
    assert_eq!(
        t.position, lave.transform.position,
        "le script de la lave ne doit pas déplacer la mare"
    );
}

#[test]
fn script_can_request_vibration() {
    let lua = Lua::new();
    let func = lua
        .load("if obj.tapped then vibrate(80) end")
        .into_function()
        .unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0; 3];
    let input = PlayerInput::default();
    let mut vib = Vec::new();
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        true,
        false,
        false,
        false,
        false,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut vib,
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(vib, vec![80.0]);
}

/// Sprint 121 : `reverb(mix)` — typiquement appelé depuis le script d'une
/// zone `trigger` à l'entrée (`obj.triggered`) — empile la valeur demandée
/// dans `reverb_out`, même mécanisme que `vibrate`/`vib_out` ci-dessus.
#[test]
fn script_can_request_reverb() {
    let lua = Lua::new();
    let func = lua
        .load("if obj.triggered then reverb(0.6) end")
        .into_function()
        .unwrap();
    let mut t = Transform::from_pos(Vec3::ZERO);
    let mut col = [1.0; 3];
    let input = PlayerInput::default();
    let mut reverb_out = Vec::new();
    run_script(
        &lua,
        &func,
        &mut t,
        &mut col,
        &mut None,
        0.016,
        0.0,
        &input,
        false,
        false,
        false,
        false,
        true,
        &[],
        &mut Vec::new(),
        &[],
        &mut Vec::new(),
        &mut false,
        &mut std::collections::HashMap::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
        false,
        None,
        &mut reverb_out,
    )
    .unwrap();
    assert_eq!(reverb_out, vec![0.6]);
}

#[test]
fn restart_game_restores_scene_and_clears_flags() {
    let mut app = AppState::new();
    app.scene = crate::scene::Scene::controller_demo();
    app.play_snapshot = app.scene.objects.clone();
    // Simule une partie en cours : une gemme ramassée, perdu, chrono figé.
    if let Some(g) = app
        .scene
        .objects
        .iter_mut()
        .find(|o| o.tap_action == crate::scene::TapAction::Hide)
    {
        g.visible = false;
    }
    app.lost = true;
    app.win_time = Some(5.0);
    app.time = 5.0;

    app.restart_game();

    assert!(!app.lost, "défaite remise à zéro");
    assert!(app.win_time.is_none(), "victoire remise à zéro");
    assert_eq!(app.time, 0.0, "chrono remis à zéro");
    // Scopé aux gemmes (Hide) : d'autres objets sont légitimement invisibles par défaut
    // dans cette démo (ex. l'ancre `is_attack_fx`, masquée tant qu'aucun coup ne porte).
    assert!(
        app.scene
            .objects
            .iter()
            .filter(|o| o.tap_action == crate::scene::TapAction::Hide)
            .all(|o| o.visible),
        "toutes les gemmes redeviennent visibles"
    );
}

/// Phase J (Sprint 22, `sprintreflecion.md`) : `toggle_pause` ne doit rien
/// faire hors Play (rien à mettre en pause, même garde que `toggle_fly_cam`).
#[test]
fn toggle_pause_has_no_effect_outside_play() {
    let mut app = AppState::new();
    assert!(!app.playing);
    app.toggle_pause();
    assert!(
        !app.paused,
        "hors Play, toggle_pause ne doit pas armer la pause"
    );
}

#[test]
fn toggle_pause_toggles_while_playing() {
    let mut app = AppState::new();
    app.playing = true;
    app.toggle_pause();
    assert!(app.paused);
    app.toggle_pause();
    assert!(!app.paused);
}

/// Phase J (Sprint 22) : la pause doit geler la simulation sur le même
/// principe que `is_room_lost`/`win_time` — le chrono de
/// `RoundObjective::Survie` ne doit **pas** continuer à courir pendant la
/// pause, même si 30 s réelles s'écoulent pendant qu'elle est active.
#[test]
fn pausing_freezes_the_survie_timer() {
    let mut monstre = SceneObject {
        visible: true,
        ..Default::default()
    };
    monstre.color = [1.0; 3];

    let mut app = AppState::new();
    app.objective = crate::app::multiplayer::RoundObjective::Survie;
    app.scene = crate::scene::Scene {
        objects: vec![monstre],
        ..Default::default()
    };
    app.playing = true;
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play(); // entrée en Play : `init_waves` + `time` remis à 0.

    // 10 s avant la fin de la manche (`SURVIE_DURATION_SECS` = 180 s) : on pause.
    app.time = 170.0;
    app.toggle_pause();
    assert!(app.paused);

    // 30 s réelles s'écoulent pendant la pause.
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(30.0);
    app.advance_play();
    assert_eq!(
        app.time, 170.0,
        "le chrono ne doit pas avancer pendant la pause"
    );
    assert!(
        app.win_time.is_none(),
        "la manche ne doit pas s'être terminée pendant la pause"
    );

    // On reprend : la simulation doit repartir normalement.
    app.toggle_pause();
    assert!(!app.paused);
    app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
    app.advance_play();
    assert!(app.time > 170.0, "le chrono doit repartir après la reprise");
}

/// `touch_state_of` (Inspecteur, indicateurs live à côté de « Tactile
/// (cliquable) ») : reflète exactement `touch_started_obj`/`touched_obj`/
/// `touch_ended_obj`, un seul à la fois vrai ici (frames distinctes en vrai
/// jeu, mais la méthode ne fait que projeter l'état courant sans hypothèse
/// de mutuelle exclusivité).
#[test]
fn touch_state_of_reflects_started_touching_and_ended_independently() {
    let mut app = AppState::new();
    assert_eq!(app.touch_state_of(0), (false, false, false));

    app.touch_started_obj = Some(0);
    assert_eq!(app.touch_state_of(0), (true, false, false));
    assert_eq!(
        app.touch_state_of(1),
        (false, false, false),
        "un autre index ne doit rien voir allumé"
    );

    app.touch_started_obj = None;
    app.touched_obj = Some(0);
    assert_eq!(app.touch_state_of(0), (false, true, false));

    app.touched_obj = None;
    app.touch_ended_obj = Some(0);
    assert_eq!(app.touch_state_of(0), (false, false, true));
}

#[test]
fn axis_basis_is_orthonormal() {
    for axis in 0..3 {
        let a = axis_dir(axis);
        let (u, w) = axis_basis(a);
        assert!((u.length() - 1.0).abs() < 1e-5);
        assert!((w.length() - 1.0).abs() < 1e-5);
        assert!(u.dot(a).abs() < 1e-5);
        assert!(w.dot(a).abs() < 1e-5);
        assert!(u.dot(w).abs() < 1e-5);
    }
}
