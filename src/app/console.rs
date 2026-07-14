//! Console développeur et contrôle du pas fixe de simulation. Extrait de `app/mod.rs`.

use glam::Vec3;

use super::AppState;

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
    /// Commandes : `timescale <v>`, `pause`, `play`, `stop`, `step`,
    /// `tp <x> <y> <z>`, `net_stats`.
    pub fn run_console_command(&mut self, cmd: &str) -> String {
        let mut parts = cmd.split_whitespace();
        let Some(name) = parts.next() else {
            return String::new();
        };
        let args: Vec<&str> = parts.collect();
        match name {
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
            "play" | "resume" => {
                if !self.playing {
                    "impossible : pas en Play".into()
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
            "step" => {
                if !self.playing || !self.paused {
                    "usage : step ne fonctionne qu'en pause (essayez d'abord `pause`)".into()
                } else {
                    self.request_step();
                    "pas unique demandé".into()
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
                self.scene.objects[target].transform.position = pos;
                format!(
                    "« {} » téléporté à ({:.2}, {:.2}, {:.2})",
                    self.scene.objects[target].name, pos.x, pos.y, pos.z
                )
            }
            "net_stats" => {
                if self.is_connected() {
                    format!(
                        "connecté · {} joueur(s) réseau · statut : {}",
                        self.network_player_count(),
                        if self.net_status.is_empty() {
                            "ok"
                        } else {
                            &self.net_status
                        }
                    )
                } else {
                    "non connecté".into()
                }
            }
            other => format!(
                "commande inconnue : « {other} » — timescale, pause, play, stop, step, tp, net_stats"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneObject;

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
        assert_eq!(app.run_console_command("step"), "pas unique demandé");
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
