//! Console développeur et contrôle du pas fixe de simulation. Extrait de `app/mod.rs`.

use glam::Vec3;

use super::AppState;

/// Aide de la Console (`help`, roadmap post-audit UX v2 2026-09-04, 6.5) :
/// une ligne par commande, dans l'ordre de `run_console_command`.
pub const CONSOLE_HELP: &str = "Commandes de la console :\n\
  help                       cette liste\n\
  timescale <v>              vitesse du temps simulé (0 = figé, 1 = normal, max 8)\n\
  pause · play · stop        piloter le mode Play\n\
  step [n]                   avancer de n pas fixes en pause\n\
  tp <x> <y> <z>             téléporter la sélection\n\
  select <nom>               sélectionner un objet par son nom\n\
  spawn <prefab> <x> <y> <z> instancier un prefab\n\
  health                     état de la barre de vie\n\
  weapon <i>                 choisir l'arme i\n\
  demo <nom> [--force]       charger une démo (mmorpg, gameplay, controleur, tower, temple, zombies, mobile, roguelike, brawl, boss, escorte, survie, components, hameau)\n\
  restart                    recommencer la manche\n\
  undo · redo                annuler / rétablir\n\
  music <0..1> · sfx <0..1>  volumes\n\
  net_stats                  état de la connexion réseau";

/// Noms acceptés par `demo <nom>` (cf. `run_console_command`).
const DEMO_NAMES: &[&str] = &[
    "mmorpg",
    "gameplay",
    "controleur",
    "controller",
    "tower",
    "temple",
    "zombies",
    "mobile",
    "roguelike",
    "brawl",
    "boss",
    "escorte",
    "survie",
    "components",
    "hameau",
    "player",
];

impl AppState {
    /// Demande l'exécution d'exactement un pas fixe de simulation à la prochaine frame,
    /// même en pause (bouton « ⏭ » de la toolbar). Sans effet si l'app n'est
    /// pas en Play — la pause n'a alors aucun sens.
    pub fn request_step(&mut self) {
        if self.playing {
            self.step_requested = true;
        }
    }

    /// Console développeur : exécute une commande texte, retourne le
    /// message à afficher dans la Console (jamais vide, y compris en cas d'erreur —
    /// pas de panique sur une saisie invalide, juste un message explicite).
    ///
    /// Commandes : `timescale <v>`, `pause`, `play`, `stop`, `step [n]`,
    /// `tp <x> <y> <z>`, `select <nom>`, `spawn <prefab> <x> <y> <z>`,
    /// `health`, `weapon <i>`, `demo <nom>`, `undo`, `redo`, `music <v>`,
    /// `sfx <v>`, `net_stats` — et `help`, qui les liste (`CONSOLE_HELP`).
    pub fn run_console_command(&mut self, cmd: &str) -> String {
        let mut parts = cmd.split_whitespace();
        let Some(name) = parts.next() else {
            return String::new();
        };
        let args: Vec<&str> = parts.collect();
        match name {
            "help" | "?" | "aide" => CONSOLE_HELP.to_string(),
            "timescale" => match args.first().and_then(|a| a.parse::<f32>().ok()) {
                Some(v) => {
                    self.time_scale = v.clamp(0.0, 8.0);
                    format!("time_scale = {:.2}", self.time_scale)
                }
                None => "usage : timescale <valeur> (ex. timescale 0.5)".into(),
            },
            "pause" => {
                if !self.playing {
                    "impossible : pas en Play".into()
                } else {
                    self.paused = true;
                    "en pause".into()
                }
            }
            // `play` démarre le mode Play s'il n'est pas actif (équivalent du bouton
            // ▶ de la toolbar — le snapshot/restauration est géré par les fronts de
            // `advance_play`, cf. `simulation`), sinon reprend après une pause.
            // Indispensable au pilotage externe (pont `pilot`) : sans ça, aucune
            // commande ne permettait d'*entrer* en Play, seulement d'en sortir.
            "play" | "resume" => {
                if !self.playing {
                    self.playing = true;
                    self.paused = false;
                    "Play démarré".into()
                } else {
                    self.paused = false;
                    "reprise".into()
                }
            }
            "stop" => {
                self.playing = false;
                self.paused = false;
                "arrêté".into()
            }
            // `step [n]` : n pas fixes de 1/60 s exécutés immédiatement
            // (`advance_steps`, déterministe — indépendant de l'horloge réelle),
            // en pause seulement : hors pause la simulation avance déjà toute
            // seule, des pas supplémentaires fausseraient le temps.
            "step" => {
                if !self.playing || !self.paused {
                    "usage : step ne fonctionne qu'en pause (essayez d'abord `pause`)".into()
                } else {
                    let n = match args.first() {
                        None => 1,
                        Some(a) => match a.parse::<u32>() {
                            Ok(n) if n >= 1 => n.min(3600),
                            _ => return "usage : step [n] (entier ≥ 1, ex. step 60)".into(),
                        },
                    };
                    self.advance_steps(n);
                    format!("{n} pas de 1/60 s exécutés")
                }
            }
            "tp" => {
                if args.len() != 3 {
                    return "usage : tp <x> <y> <z>".into();
                }
                let parsed: Option<Vec<f32>> = args.iter().map(|a| a.parse::<f32>().ok()).collect();
                let Some(xyz) = parsed else {
                    return "usage : tp <x> <y> <z> (nombres attendus)".into();
                };
                let Some(target) = self.player_index().or(self.selection) else {
                    return "aucun objet cible : sélectionnez un objet ou lancez le Play".into();
                };
                let pos = Vec3::new(xyz[0], xyz[1], xyz[2]);
                // Hors Play, la téléportation est une édition comme une autre :
                // annulable et marquée modifiée (roadmap post-audit UX v2
                // 2026-09-04, 3.5). En Play, les objets sont restaurés au Stop.
                if !self.playing {
                    self.push_undo();
                }
                self.scene.objects[target].transform.position = pos;
                format!(
                    "« {} » téléporté à ({:.2}, {:.2}, {:.2})",
                    self.scene.objects[target].name, pos.x, pos.y, pos.z
                )
            }
            "select" => {
                if args.is_empty() {
                    return "usage : select <nom d'objet>".into();
                }
                let name = args.join(" ");
                match self.scene.objects.iter().position(|o| o.name == name) {
                    Some(i) => {
                        // Même chemin que le clic (roadmap 3.5) : `selection` et
                        // `selected` restent cohérents pour le gizmo et l'inspecteur.
                        self.selected_light = None;
                        self.select_single(i);
                        format!("« {name} » sélectionné (index {i})")
                    }
                    None => format!("aucun objet nommé « {name} »"),
                }
            }
            "spawn" => {
                if args.len() != 4 {
                    return "usage : spawn <asset-id de prefab> <x> <y> <z>".into();
                }
                let parsed: Option<Vec<f32>> =
                    args[1..].iter().map(|a| a.parse::<f32>().ok()).collect();
                let Some(xyz) = parsed else {
                    return "usage : spawn <asset-id de prefab> <x> <y> <z> (nombres attendus)"
                        .into();
                };
                let before = self.scene.objects.len();
                self.instantiate_prefab(args[0]);
                if self.scene.objects.len() > before {
                    let pos = Vec3::new(xyz[0], xyz[1], xyz[2]);
                    let obj = self
                        .scene
                        .objects
                        .last_mut()
                        .expect("objet tout juste ajouté");
                    obj.transform.position = pos;
                    format!(
                        "« {} » instancié à ({:.2}, {:.2}, {:.2})",
                        obj.name, pos.x, pos.y, pos.z
                    )
                } else {
                    format!("prefab introuvable : « {} »", args[0])
                }
            }
            "health" => match self.hud_health {
                Some(h) => format!("vie : {:.2}", h),
                None => "système de vie inactif (aucun script n'a appelé set_health)".into(),
            },
            "weapon" => match args.first().and_then(|a| a.parse::<usize>().ok()) {
                Some(i) => {
                    self.select_weapon(i);
                    format!("arme : {}", self.selected_weapon_label())
                }
                None => format!(
                    "usage : weapon <0-2> — actuelle : {}",
                    self.selected_weapon_label()
                ),
            },
            // `restart` : rejouer la manche courante (équivalent du bouton
            // « Rejouer » de l'écran de fin) — indispensable au pilotage : une
            // victoire/défaite gèle toute la simulation jusqu'à ce geste.
            "restart" => {
                if !self.playing {
                    "impossible : pas en Play".into()
                } else if self.request_round_restart() {
                    // En ligne (roadmap post-audit UX v2 2026-09-04, 0.2) :
                    // même chemin que le bouton « Rejouer », la manche est
                    // celle du serveur.
                    "nouvelle manche demandée au serveur".into()
                } else {
                    self.restart_game();
                    "manche redémarrée".into()
                }
            }
            "undo" => {
                self.undo();
                "annulé".into()
            }
            "redo" => {
                self.redo();
                "rétabli".into()
            }
            "music" => match args.first().and_then(|a| a.parse::<f32>().ok()) {
                Some(v) => {
                    self.set_music_volume(v.clamp(0.0, 1.0));
                    format!("volume musique : {:.2}", v.clamp(0.0, 1.0))
                }
                None => "usage : music <0..1>".into(),
            },
            "sfx" => match args.first().and_then(|a| a.parse::<f32>().ok()) {
                Some(v) => {
                    self.set_sfx_volume(v.clamp(0.0, 1.0));
                    format!("volume effets : {:.2}", v.clamp(0.0, 1.0))
                }
                None => "usage : sfx <0..1>".into(),
            },
            // `demo <nom>` : charge une des scènes de démo du menu Fichier —
            // même liste que `src/app/demos.rs`, pour piloter les audits sans UI.
            // Même garde que le menu (roadmap post-audit UX v2 2026-09-04, 3.5) :
            // une scène modifiée n'est pas écrasée sans un mot — ici pas de
            // modale (la console sert aussi au pilotage automatisé), on demande
            // `--force` explicitement.
            // En Play, une démo remplace la scène : on sort proprement de Play
            // d'abord (`stop_play`, roadmap 3.4 — sinon le front de sortie
            // réécrivait les objets de l'ancienne scène par-dessus la nouvelle),
            // puis la commande repasse par les gardes ci-dessous.
            "demo"
                if (self.playing || self.was_playing)
                    && args.first().is_some_and(|n| DEMO_NAMES.contains(n)) =>
            {
                self.stop_play();
                self.run_console_command(cmd)
            }
            "demo" if self.scene_dirty && !self.playing && !args.contains(&"--force") => {
                "scène modifiée et non enregistrée : enregistre d'abord (Cmd+S), \
                 ou `demo <nom> --force` pour l'écraser"
                    .into()
            }
            "demo" => match args.first().copied() {
                Some("mmorpg") => {
                    self.load_mmorpg_demo();
                    "démo MMORPG chargée".into()
                }
                Some("gameplay") => {
                    self.load_gameplay_demo();
                    "démo gameplay chargée".into()
                }
                Some("controleur") | Some("controller") => {
                    self.load_controller_demo();
                    "démo contrôleur chargée".into()
                }
                Some("tower") => {
                    self.load_tower_demo();
                    "démo tour chargée".into()
                }
                Some("temple") => {
                    self.load_temple_run_demo();
                    "démo temple run chargée".into()
                }
                Some("zombies") => {
                    self.load_zombies_demo();
                    "démo zombies chargée".into()
                }
                Some("mobile") => {
                    self.load_mobile_demo();
                    "démo mobile chargée".into()
                }
                Some("roguelike") => {
                    self.load_roguelike_demo();
                    "démo roguelike chargée".into()
                }
                Some("brawl") => {
                    self.load_brawl_demo();
                    "démo brawl chargée".into()
                }
                Some("boss") => {
                    self.load_boss_demo();
                    "démo boss chargée".into()
                }
                Some("escorte") => {
                    self.load_escorte_demo();
                    "démo escorte chargée".into()
                }
                Some("survie") => {
                    self.load_survie_demo();
                    "démo survie chargée".into()
                }
                Some("components") => {
                    self.load_components_demo();
                    "démo composants chargée".into()
                }
                Some("hameau") | Some("player") => {
                    self.load_embedded_player_scene();
                    "scène du jeu (hameau) chargée".into()
                }
                other => format!(
                    "usage : demo <nom> — mmorpg, gameplay, controleur, tower, temple, zombies, \
                     mobile, roguelike, brawl, boss, escorte, survie, components, hameau{}",
                    other
                        .map(|o| format!(" (reçu : « {o} »)"))
                        .unwrap_or_default()
                ),
            },
            "net_stats" => {
                if self.is_connected() {
                    format!(
                        "connecté · {} joueur(s) réseau · latence {} · statut : {}",
                        self.network_player_count(),
                        // Roadmap post-audit UX v2 2026-09-04, 2.6.
                        self.net_rtt_ms()
                            .map_or("inconnue".to_string(), |ms| format!("{ms} ms")),
                        if self.net_conn.net_status.is_empty() {
                            "ok"
                        } else {
                            &self.net_conn.net_status
                        }
                    )
                } else {
                    "non connecté".into()
                }
            }
            other => format!(
                "commande inconnue : « {other} » — tape help pour la liste (timescale, pause, \
                 play, stop, step, tp, select, spawn, health, weapon, demo, restart, undo, redo, \
                 music, sfx, net_stats)"
            ),
        }
    }

    /// Le monde physique du mode Play est-il construit ? (Diagnostic du pont de
    /// pilotage : `advance_steps` simule sans effet joueur tant que le front
    /// d'entrée en Play — qui construit la physique — n'a pas tourné.)
    pub fn physics_ready(&self) -> bool {
        self.physics.is_some()
    }

    /// Évalue une chaîne Lua arbitraire sur l'instance partagée (pont de pilotage
    /// externe, cf. `crate::pilot`) et renvoie les valeurs produites formatées en
    /// texte. Les globales visibles sont celles laissées par le dernier tick de
    /// scripts (`scripting::run_script`) : `find_tag`, `spawn`, `emit`… restent
    /// appelables entre deux ticks, mais `raycast`/`overlap_sphere` (fermetures
    /// scopées, expirées à la fin de chaque tick) répondent une erreur explicite
    /// hors d'un tick — pas un plantage.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn eval_lua(&mut self, src: &str) -> Result<String, String> {
        let values = self
            .scripting
            .lua
            .load(src)
            .eval::<mlua::MultiValue>()
            .map_err(|e| e.to_string())?;
        if values.is_empty() {
            return Ok("(aucune valeur)".into());
        }
        Ok(values
            .iter()
            .map(format_lua_value)
            .collect::<Vec<_>>()
            .join("\t"))
    }
}

/// Formatte une valeur Lua pour la réponse texte d'`eval_lua` — plat pour les
/// scalaires, superficiel (une profondeur) pour les tables : le pont sert à
/// inspecter, pas à sérialiser fidèlement des structures arbitraires.
#[cfg(not(target_arch = "wasm32"))]
fn format_lua_value(v: &mlua::Value) -> String {
    match v {
        mlua::Value::Nil => "nil".into(),
        mlua::Value::Boolean(b) => b.to_string(),
        mlua::Value::Integer(i) => i.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        mlua::Value::String(s) => s.to_string_lossy().to_string(),
        mlua::Value::Table(t) => {
            let mut parts = Vec::new();
            for pair in t.clone().pairs::<mlua::Value, mlua::Value>() {
                let Ok((k, val)) = pair else { continue };
                let key = match &k {
                    mlua::Value::String(s) => s.to_string_lossy().to_string(),
                    mlua::Value::Integer(i) => i.to_string(),
                    other => format!("{other:?}"),
                };
                let shallow = match &val {
                    mlua::Value::Table(_) => "{…}".to_string(),
                    scalar => format_lua_value(scalar),
                };
                parts.push(format!("{key}={shallow}"));
            }
            format!("{{{}}}", parts.join(", "))
        }
        other => format!("<{}>", other.type_name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneObject;

    /// Roadmap 6.5 : `help` liste chaque commande connue, et une commande
    /// inconnue renvoie vers `help`.
    #[test]
    fn console_help_lists_every_command_and_unknown_points_to_it() {
        let mut app = AppState::new();
        let help = app.run_console_command("help");
        for cmd in [
            "timescale",
            "pause",
            "play",
            "stop",
            "step",
            "tp",
            "select",
            "spawn",
            "health",
            "weapon",
            "demo",
            "restart",
            "undo",
            "redo",
            "music",
            "sfx",
            "net_stats",
        ] {
            assert!(help.contains(cmd), "help ne mentionne pas {cmd}");
        }
        assert_eq!(app.run_console_command("aide"), help);
        assert!(app.run_console_command("zork").contains("help"));
    }

    #[test]
    fn console_timescale_sets_and_clamps() {
        let mut app = AppState::new();
        assert_eq!(
            app.run_console_command("timescale 0.5"),
            "time_scale = 0.50"
        );
        assert!((app.time_scale - 0.5).abs() < 1e-6);
        // Clampé à 8.0, pas d'erreur ni de valeur absurde sur une entrée extrême.
        assert_eq!(
            app.run_console_command("timescale 1000"),
            "time_scale = 8.00"
        );
        assert!((app.time_scale - 8.0).abs() < 1e-6);
        // Argument invalide : message d'usage, aucune panique, valeur inchangée.
        let before = app.time_scale;
        let msg = app.run_console_command("timescale abc");
        assert!(msg.starts_with("usage"), "message obtenu : {msg}");
        assert_eq!(app.time_scale, before);
    }

    #[test]
    fn console_pause_play_stop_step_drive_the_same_state_as_the_toolbar() {
        let mut app = AppState::new();
        assert_eq!(app.run_console_command("pause"), "impossible : pas en Play");
        app.playing = true;
        assert_eq!(app.run_console_command("pause"), "en pause");
        assert!(app.paused);
        assert_eq!(app.run_console_command("play"), "reprise");
        assert!(!app.paused);
        assert_eq!(
            app.run_console_command("step"),
            "usage : step ne fonctionne qu'en pause (essayez d'abord `pause`)"
        );
        app.run_console_command("pause");
        assert_eq!(app.run_console_command("step"), "1 pas de 1/60 s exécutés");
        // `step n` : n pas déterministes d'un coup ; argument invalide = usage.
        assert_eq!(
            app.run_console_command("step 30"),
            "30 pas de 1/60 s exécutés"
        );
        assert!(
            app.run_console_command("step zéro").starts_with("usage"),
            "argument non numérique : message d'usage"
        );
        assert_eq!(app.run_console_command("stop"), "arrêté");
        assert!(!app.playing && !app.paused);
    }

    #[test]
    fn console_tp_moves_the_selected_object() {
        let mut app = AppState::new();
        // Scène vidée : `AppState::new()` charge `Scene::demo()`, qui contient déjà un
        // objet joueur — `tp` le préférerait à la sélection (cf. `player_index().or(..)`),
        // rendant le test non déterministe sans ce nettoyage.
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject::default());
        app.selection = Some(0);
        assert_eq!(app.run_console_command("tp"), "usage : tp <x> <y> <z>");
        let msg = app.run_console_command("tp 1 2 3");
        assert!(
            msg.contains("téléporté à (1.00, 2.00, 3.00)"),
            "message : {msg}"
        );
        assert_eq!(
            app.scene.objects[0].transform.position,
            Vec3::new(1.0, 2.0, 3.0)
        );
    }

    #[test]
    fn console_tp_without_a_target_reports_the_problem_instead_of_panicking() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.selection = None;
        assert_eq!(
            app.run_console_command("tp 0 0 0"),
            "aucun objet cible : sélectionnez un objet ou lancez le Play"
        );
    }

    /// Pont de pilotage externe : `play` doit pouvoir *démarrer* le mode Play
    /// (équivalent du bouton ▶), pas seulement reprendre après une pause.
    #[test]
    fn console_play_starts_play_mode_when_not_playing() {
        let mut app = AppState::new();
        assert!(!app.playing);
        assert_eq!(app.run_console_command("play"), "Play démarré");
        assert!(app.playing && !app.paused);
        // Déjà en Play : `play` reste la reprise après pause, comme avant.
        app.paused = true;
        assert_eq!(app.run_console_command("play"), "reprise");
        assert!(!app.paused);
    }

    #[test]
    fn console_select_finds_an_object_by_name_or_reports_it_missing() {
        let mut app = AppState::new();
        app.scene.objects.clear();
        app.scene.objects.push(SceneObject {
            name: "Feu de camp".into(),
            ..Default::default()
        });
        app.selection = None;
        let msg = app.run_console_command("select Feu de camp");
        assert!(msg.contains("sélectionné"), "message : {msg}");
        assert_eq!(app.selection, Some(0));
        assert_eq!(
            app.run_console_command("select Licorne"),
            "aucun objet nommé « Licorne »"
        );
        assert_eq!(
            app.run_console_command("select"),
            "usage : select <nom d'objet>"
        );
    }

    #[test]
    fn console_spawn_with_an_unknown_prefab_reports_it_without_adding_anything() {
        // Dossier d'assets redirigé vers un chemin inexistant : aucun prefab
        // disponible, `spawn` doit le dire sans modifier la scène ni paniquer
        // (cf. `instantiate_prefab_with_an_unknown_id_does_nothing`).
        let missing = std::env::temp_dir().join(format!(
            "rusteegear_console_spawn_{}_{}",
            std::process::id(),
            line!()
        ));
        let _guard = crate::assets::override_assets_dir_for_test(missing);
        let mut app = AppState::new();
        let before = app.scene.objects.len();
        let msg = app.run_console_command("spawn fantome 0 1 0");
        assert!(msg.contains("introuvable"), "message : {msg}");
        assert_eq!(app.scene.objects.len(), before);
        assert!(
            app.run_console_command("spawn fantome 0 un 0")
                .starts_with("usage"),
            "coordonnée non numérique : message d'usage"
        );
    }

    #[test]
    fn console_health_reports_inactive_until_a_script_sets_it() {
        let mut app = AppState::new();
        let msg = app.run_console_command("health");
        assert!(msg.contains("inactif"), "message : {msg}");
        app.hud_health = Some(0.5);
        assert_eq!(app.run_console_command("health"), "vie : 0.50");
    }

    /// Pont de pilotage : l'éval Lua arbitraire doit renvoyer les valeurs
    /// produites, et remonter les erreurs Lua en `Err` lisible (audit C1 :
    /// gestion d'erreur observable de l'extérieur), jamais paniquer.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn eval_lua_returns_values_and_reports_errors() {
        let mut app = AppState::new();
        assert_eq!(app.eval_lua("return 1 + 1"), Ok("2".into()));
        assert_eq!(
            app.eval_lua("return 'a', true, nil"),
            Ok("a\ttrue\tnil".into())
        );
        assert_eq!(app.eval_lua("local x = 3"), Ok("(aucune valeur)".into()));
        let table = app.eval_lua("return {x = 1}").unwrap();
        assert_eq!(table, "{x=1}");
        let err = app.eval_lua("return boom(").unwrap_err();
        assert!(!err.is_empty());
        let runtime_err = app.eval_lua("error('exprès')").unwrap_err();
        assert!(runtime_err.contains("exprès"), "erreur : {runtime_err}");
    }

    /// Roadmap 3.5 : `demo` respecte la même garde que le menu — une scène
    /// modifiée n'est pas écrasée sans `--force`.
    #[test]
    fn console_demo_refuses_to_overwrite_a_dirty_scene_without_force() {
        let mut app = AppState::new();
        app.scene.objects.push(SceneObject {
            name: "Témoin".into(),
            ..Default::default()
        });
        app.scene_dirty = true;
        let msg = app.run_console_command("demo mobile");
        assert!(msg.contains("--force"), "message obtenu : {msg}");
        assert!(
            app.scene.objects.iter().any(|o| o.name == "Témoin"),
            "scène intacte"
        );
        assert_eq!(
            app.run_console_command("demo mobile --force"),
            "démo mobile chargée"
        );
        assert!(app.scene.objects.iter().all(|o| o.name != "Témoin"));
        assert_eq!(app.scene_file, None, "une démo n'est liée à aucun fichier");
    }

    /// Roadmap 3.4 : `demo` en Play sort d'abord proprement de Play, la scène
    /// de la démo n'est pas réécrite par le snapshot de l'ancienne.
    #[test]
    fn console_demo_during_play_stops_play_before_switching() {
        let mut app = AppState::new();
        app.run_console_command("demo mobile --force");
        app.scene_dirty = false;
        let before = app.scene.objects.clone();
        app.playing = true;
        app.advance_play();
        assert!(app.was_playing);
        assert_eq!(
            app.run_console_command("demo controleur"),
            "démo contrôleur chargée"
        );
        assert!(!app.playing && !app.was_playing);
        let after = app.scene.objects.clone();
        app.advance_play();
        assert_eq!(
            app.scene.objects.len(),
            after.len(),
            "aucun front de sortie tardif ne doit restaurer l'ancienne scène"
        );
        assert_ne!(before.len(), after.len());
    }

    #[test]
    fn console_net_stats_reports_disconnected_by_default() {
        let mut app = AppState::new();
        assert_eq!(app.run_console_command("net_stats"), "non connecté");
    }

    #[test]
    fn console_unknown_command_names_it_instead_of_silently_ignoring_it() {
        let mut app = AppState::new();
        let msg = app.run_console_command("frobnicate");
        assert!(msg.contains("frobnicate"), "message obtenu : {msg}");
    }
}
