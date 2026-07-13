//! Console développeur (Sprint 82) et contrôle du pas fixe de simulation (Sprint 81). Extrait de `app/mod.rs` (Sprint 103a).

use glam::Vec3;

use super::AppState;

impl AppState {
    /// Demande l'exécution d'exactement un pas fixe de simulation à la prochaine frame,
    /// même en pause (Sprint 81 : bouton « ⏭ » de la toolbar). Sans effet si l'app n'est
    /// pas en Play — la pause n'a alors aucun sens.
    pub fn request_step(&mut self) {
        if self.playing {
            self.step_requested = true;
        }
    }

    /// Console développeur (Sprint 82) : exécute une commande texte, retourne le
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
