//! Notifications non bloquantes de l'éditeur (roadmap post-audit UX
//! 2026-09-04, 1.2).
//!
//! Avant : aucun retour à l'écran — sauvegarde réussie ou échouée, import glTF
//! refusé, projet introuvable, script Lua en erreur… tout partait dans
//! `log::` et n'était visible que dans la fenêtre Console, fermée par défaut.
//! Ici, on relit le journal capturé par [`crate::log_buffer`] à chaque frame et
//! on affiche en toast (coin bas-droit, au-dessus de la barre d'état) :
//!
//! - toute ligne `error`/`warn` émise par le moteur (`motor3derust::…`, pas le
//!   bruit de wgpu/egui/naga) ;
//! - les lignes `info` des modules qui parlent à l'utilisateur (persistance,
//!   projets, autosave) — « Scène sauvegardée dans … » mérite d'être vu.
//!
//! Une ligne identique répétée (erreur de script à chaque frame, par exemple)
//! ne crée pas un toast par occurrence : le toast existant est rafraîchi et
//! compte « ×N ». Un clic ferme un toast ; ils expirent seuls sinon. La barre
//! d'état affiche en plus le nombre d'erreurs survenues depuis la dernière
//! ouverture de la Console (`unseen_errors`), cliquable pour l'ouvrir.

use crate::log_buffer::LogEvent;
use crate::time_compat::Instant;
use egui::{Color32, RichText};

/// Durée d'affichage par niveau.
const TTL_ERROR_S: f32 = 10.0;
const TTL_WARN_S: f32 = 6.0;
const TTL_INFO_S: f32 = 4.0;
/// Nombre maximal de toasts visibles simultanément (les plus récents).
const MAX_VISIBLE: usize = 5;

/// Un toast à l'écran.
struct Toast {
    level: log::Level,
    text: String,
    /// Occurrences fusionnées (même texte, cf. module).
    count: u32,
    /// Dernière occurrence — l'expiration se compte depuis là.
    refreshed: Instant,
    ttl_s: f32,
}

/// État des toasts de l'éditeur, cf. module.
pub struct Toasts {
    items: Vec<Toast>,
    /// Dernier `LogEvent::seq` consommé (cf. `log_buffer::events_since`).
    last_seq: u64,
    /// Erreurs survenues depuis la dernière ouverture de la Console.
    pub unseen_errors: usize,
}

impl Default for Toasts {
    fn default() -> Self {
        // On démarre sur le journal courant, sans rejouer ce qui a été émis avant
        // la création de l'éditeur (init GPU, messages de démarrage).
        Toasts {
            items: Vec::new(),
            last_seq: crate::log_buffer::latest_seq(),
            unseen_errors: 0,
        }
    }
}

/// Un toast est un résumé : au plus `MAX_LINES` lignes et `MAX_CHARS`
/// caractères (une pile d'appels Lua tient dans la Console et sous le champ
/// Script de l'inspecteur, pas dans un coin de l'écran).
const MAX_LINES: usize = 3;
const MAX_CHARS: usize = 220;
fn shorten(text: &str) -> String {
    let mut out = String::new();
    let mut truncated = false;
    for (n, line) in text.lines().enumerate() {
        if n >= MAX_LINES {
            truncated = true;
            break;
        }
        if n > 0 {
            out.push('\n');
        }
        out.push_str(line.trim_end());
    }
    if out.chars().count() > MAX_CHARS {
        out = out.chars().take(MAX_CHARS).collect();
        truncated = true;
    }
    if truncated {
        out.push('…');
    }
    out
}

/// Cible de log à utiliser pour une confirmation explicite adressée à
/// l'utilisateur depuis n'importe quel module (`log::info!(target:
/// FEEDBACK_TARGET, …)`) — « Diagnostic copié », « Journal de crash copié »…
/// Toujours affichée en toast (roadmap post-audit UX v2 2026-09-04, 6.2),
/// et dans la Console comme toute ligne de journal.
pub const FEEDBACK_TARGET: &str = "motor3derust::editor::feedback";

/// Cible de log **jamais** affichée en toast au niveau `info` : la sortie des
/// commandes de la Console (`> help …`), lue dans la Console elle-même
/// (roadmap 6.2 — avant, chaque commande produisait un toast).
pub const CONSOLE_TARGET: &str = "motor3derust::console";

/// Un événement de log mérite-t-il un toast ? Règle documentée en tête de module.
fn wants_toast(ev: &LogEvent) -> bool {
    if !ev.target.starts_with("motor3derust") {
        return false;
    }
    match ev.level {
        log::Level::Error | log::Level::Warn => true,
        log::Level::Info => {
            ev.target == FEEDBACK_TARGET
                || ev.target.contains("persistence")
                || ev.target.contains("autosave")
                || ev.target.contains("renderer::frame")
                || ev.target.contains("project")
        }
        _ => false,
    }
}

impl Toasts {
    /// Relit les nouvelles lignes du journal et fait vieillir les toasts. À
    /// appeler une fois par frame, avant `show`. `console_open` remet le compteur
    /// d'erreurs non vues à zéro (la Console les montre toutes).
    pub fn pump(&mut self, console_open: bool) {
        let (events, latest) = crate::log_buffer::events_since(self.last_seq);
        self.last_seq = latest;
        let now = Instant::now();
        for ev in events {
            if !wants_toast(&ev) {
                continue;
            }
            if ev.level == log::Level::Error {
                self.unseen_errors += 1;
            }
            let short = shorten(&ev.text);
            if let Some(t) = self.items.iter_mut().find(|t| t.text == short) {
                t.count = t.count.saturating_add(1);
                t.refreshed = now;
                continue;
            }
            let ttl_s = match ev.level {
                log::Level::Error => TTL_ERROR_S,
                log::Level::Warn => TTL_WARN_S,
                _ => TTL_INFO_S,
            };
            self.items.push(Toast {
                level: ev.level,
                text: shorten(&ev.text),
                count: 1,
                refreshed: now,
                ttl_s,
            });
        }
        self.items
            .retain(|t| now.duration_since(t.refreshed).as_secs_f32() < t.ttl_s);
        if console_open {
            self.unseen_errors = 0;
        }
    }

    /// Dessine les toasts (coin bas-droit, empilés vers le haut). `bottom_inset`
    /// = hauteur à réserver au-dessus du bord bas (barre d'état).
    pub fn show(&mut self, ctx: &egui::Context, bottom_inset: f32) {
        if self.items.is_empty() {
            return;
        }
        let mut dismiss: Option<usize> = None;
        let first = self.items.len().saturating_sub(MAX_VISIBLE);
        egui::Area::new(egui::Id::new("editor-toasts"))
            .anchor(
                egui::Align2::RIGHT_BOTTOM,
                egui::vec2(-12.0, -(bottom_inset + 8.0)),
            )
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                // Le plus récent en bas, contre la barre d'état.
                for (i, t) in self.items.iter().enumerate().skip(first) {
                    let (fill, stripe, icon) = match t.level {
                        log::Level::Error => (
                            Color32::from_rgb(62, 31, 29),
                            Color32::from_rgb(240, 115, 106),
                            "⛔",
                        ),
                        log::Level::Warn => (
                            Color32::from_rgb(59, 46, 20),
                            Color32::from_rgb(224, 169, 74),
                            "⚠",
                        ),
                        _ => (
                            Color32::from_rgb(25, 50, 37),
                            Color32::from_rgb(111, 195, 146),
                            "✓",
                        ),
                    };
                    let resp = egui::Frame::new()
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0_f32, stripe))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon).color(stripe));
                                let mut text = t.text.clone();
                                if t.count > 1 {
                                    text.push_str(&format!("  ×{}", t.count));
                                }
                                ui.add(
                                    egui::Label::new(RichText::new(text).color(Color32::WHITE))
                                        .wrap(),
                                );
                            });
                        })
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_text("Cliquer pour fermer");
                    if resp.clicked() {
                        dismiss = Some(i);
                    }
                    ui.add_space(4.0);
                }
            });
        if let Some(i) = dismiss {
            self.items.remove(i);
        }
    }

    /// Nombre de toasts actuellement affichés (tests).
    #[cfg(test)]
    fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(level: log::Level, target: &str, text: &str) -> LogEvent {
        LogEvent {
            seq: 0,
            level,
            target: target.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn engine_errors_and_warnings_toast_but_dependency_noise_does_not() {
        assert!(wants_toast(&ev(
            log::Level::Error,
            "motor3derust::app::persistence",
            "x"
        )));
        assert!(wants_toast(&ev(
            log::Level::Warn,
            "motor3derust::scene::import",
            "x"
        )));
        assert!(!wants_toast(&ev(
            log::Level::Warn,
            "egui_wgpu::renderer",
            "x"
        )));
        assert!(!wants_toast(&ev(
            log::Level::Error,
            "wgpu_core::device",
            "x"
        )));
    }

    #[test]
    fn only_user_facing_modules_toast_at_info_level() {
        assert!(wants_toast(&ev(
            log::Level::Info,
            "motor3derust::app::persistence",
            "Scène sauvegardée"
        )));
        assert!(wants_toast(&ev(
            log::Level::Info,
            "motor3derust::gfx::renderer::frame",
            "Projet dupliqué"
        )));
        assert!(!wants_toast(&ev(
            log::Level::Info,
            "motor3derust::gfx::renderer::resources",
            "GPU : …"
        )));
        assert!(!wants_toast(&ev(
            log::Level::Info,
            "motor3derust",
            "RusteeGear 0.1.0"
        )));
        // Roadmap 6.2 : confirmations explicites oui, sortie console non.
        assert!(wants_toast(&ev(
            log::Level::Info,
            FEEDBACK_TARGET,
            "Diagnostic copié"
        )));
        assert!(!wants_toast(&ev(
            log::Level::Info,
            CONSOLE_TARGET,
            "> help"
        )));
    }

    #[test]
    fn long_messages_are_shortened_to_a_few_lines() {
        let long = "l1\nl2\nl3\nl4\nl5";
        assert_eq!(shorten(long), "l1\nl2\nl3…");
        let wide = "x".repeat(300);
        assert_eq!(shorten(&wide).chars().count(), MAX_CHARS + 1);
        assert_eq!(shorten("court"), "court");
    }

    #[test]
    fn repeated_identical_lines_merge_into_one_toast_with_a_counter() {
        let mut toasts = Toasts::default();
        // Sans logger global dans les tests : on alimente le tampon directement.
        for _ in 0..3 {
            crate::log_buffer::push_event(
                log::Level::Error,
                "motor3derust::app::simulation",
                "Script 'Cube' : boom".to_string(),
            );
        }
        toasts.pump(false);
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts.items[0].count, 3);
        assert_eq!(toasts.unseen_errors, 3);
        toasts.pump(true);
        assert_eq!(toasts.unseen_errors, 0);
    }
}
