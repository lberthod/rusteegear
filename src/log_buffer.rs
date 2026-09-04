//! Capture des logs en mémoire pour la Console intégrée.
//!
//! On installe un logger qui *tee* : il délègue à `env_logger` (sortie stderr
//! habituelle) **et** conserve les dernières lignes dans un tampon circulaire que
//! la fenêtre « Console » de l'éditeur peut afficher.

use std::collections::VecDeque;
use std::sync::Mutex;

use log::{Log, Metadata, Record};

/// Nombre maximum de lignes conservées en mémoire.
const CAPACITY: usize = 500;

/// Tampon circulaire partagé des dernières lignes de log.
static BUFFER: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Une ligne de log typée, pour les toasts de l'éditeur (roadmap post-audit
/// UX 2026-09-04, 1.2) — `seq` est un compteur global croissant, ce qui permet
/// à un consommateur de ne lire que « ce qui est arrivé depuis ».
#[derive(Clone, Debug)]
pub struct LogEvent {
    pub seq: u64,
    pub level: log::Level,
    /// Module émetteur (`motor3derust::app::persistence`, `wgpu_core::…`).
    pub target: String,
    pub text: String,
}

/// Événements récents (capacité `EVENTS_CAPACITY`) et prochain `seq`.
static EVENTS: Mutex<(u64, VecDeque<LogEvent>)> = Mutex::new((0, VecDeque::new()));
const EVENTS_CAPACITY: usize = 200;

/// Logger qui écrit à la fois sur stderr (`env_logger`) et dans le tampon mémoire.
struct CaptureLogger {
    inner: env_logger::Logger,
}

impl Log for CaptureLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if self.inner.enabled(record.metadata()) {
            if let Ok(mut buf) = BUFFER.lock() {
                if buf.len() >= CAPACITY {
                    buf.pop_front();
                }
                buf.push_back(format!("[{}] {}", record.level(), record.args()));
            }
            push_event(record.level(), record.target(), record.args().to_string());
        }
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Installe le logger de capture (remplace `env_logger::init`). Sans effet si un
/// logger global est déjà posé.
pub fn install() {
    // `egui_wgpu::renderer` émet au démarrage un warning cosmétique sur le
    // format sRGB du framebuffer (préférence interne d'egui, sans conséquence
    // visible ici) : rétrogradé par défaut pour un premier lancement propre
    // (Phase A, sprint.19matin.md). `RUST_LOG` reprend la main si posé.
    let logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,egui_wgpu=error"),
    )
    .build();
    log::set_max_level(logger.filter());
    let _ = log::set_boxed_logger(Box::new(CaptureLogger { inner: logger }));
}

/// Ajoute un événement typé (cf. `LogEvent`). Séparé de `CaptureLogger::log`
/// pour être appelable par les tests sans logger global.
pub(crate) fn push_event(level: log::Level, target: &str, text: String) {
    if let Ok(mut guard) = EVENTS.lock() {
        let (next, events) = &mut *guard;
        if events.len() >= EVENTS_CAPACITY {
            events.pop_front();
        }
        events.push_back(LogEvent {
            seq: *next,
            level,
            target: target.to_string(),
            text,
        });
        *next += 1;
    }
}

/// Événements de `seq >= since` (dans l'ordre), et le prochain `seq` à passer
/// au prochain appel. Si plus de `EVENTS_CAPACITY` lignes sont arrivées entre
/// deux appels, les plus anciennes sont perdues — acceptable pour des toasts.
pub fn events_since(since: u64) -> (Vec<LogEvent>, u64) {
    EVENTS
        .lock()
        .map(|g| {
            let (next, events) = &*g;
            (
                events.iter().filter(|e| e.seq >= since).cloned().collect(),
                *next,
            )
        })
        .unwrap_or((Vec::new(), since))
}

/// Prochain `seq` (= nombre total d'événements émis) — point de départ d'un
/// consommateur qui ne veut pas rejouer le passé.
pub fn latest_seq() -> u64 {
    EVENTS.lock().map(|g| g.0).unwrap_or(0)
}

/// Clés déjà journalisées par `warn_once` (cf. ci-dessous).
static ONCE: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

/// Journalise `msg` en `warn` une seule fois par `key` pour toute la durée du
/// process — pour les diagnostics émis depuis un chemin appelé à chaque frame
/// (roadmap post-audit UX 2026-09-04, lot 1.B : `local_aabb` inondait la
/// Console à ~160 lignes/s sur `examples/broken_scene`). Retourne `true` si la
/// ligne a été émise.
pub fn warn_once(key: &str, msg: &str) -> bool {
    let fresh = ONCE
        .lock()
        .map(|mut g| {
            g.get_or_insert_with(Default::default)
                .insert(key.to_string())
        })
        .unwrap_or(true);
    if fresh {
        log::warn!("{msg}");
    }
    fresh
}

/// Copie les lignes de log actuellement en mémoire (de la plus ancienne à la plus récente).
pub fn snapshot() -> Vec<String> {
    BUFFER
        .lock()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default()
}

/// Vide le tampon (bouton « Effacer » de la Console).
pub fn clear() {
    if let Ok(mut b) = BUFFER.lock() {
        b.clear();
    }
}

/// Nombre de lignes de log incluses dans `diagnostic_report` — assez pour
/// couvrir un incident récent, pas tout le tampon (un rapport collé dans une
/// issue doit rester lisible).
const REPORT_LOG_LINES: usize = 30;

/// Rapport de diagnostic prêt à coller dans une issue (Phase E4,
/// sprint.19matin.md) : version/commit, OS, format de scène, puis les
/// dernières lignes de log (elles contiennent déjà la bannière et la ligne
/// GPU du démarrage). Le chemin du dossier personnel est remplacé par `~`
/// partout — un rapport ne doit pas divulguer le nom d'utilisateur.
pub fn diagnostic_report() -> String {
    let logs: Vec<String> = {
        let all = snapshot();
        let skip = all.len().saturating_sub(REPORT_LOG_LINES);
        all.into_iter().skip(skip).collect()
    };
    let mut out = format!(
        "RusteeGear {} — Developer Preview 1\n\
         Commit : {}\n\
         OS : {} ({})\n\
         Format de scène : v{}\n\
         --- Derniers logs ({} lignes max) ---\n{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("RUSTEEGEAR_COMMIT").unwrap_or("build local"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        crate::scene::Scene::CURRENT_VERSION,
        REPORT_LOG_LINES,
        logs.join("\n"),
    );
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        out = out.replace(&home, "~");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warn_once_emits_a_given_key_a_single_time() {
        assert!(warn_once("test-key-unique", "première fois"));
        assert!(!warn_once("test-key-unique", "deuxième fois"));
        assert!(warn_once("test-key-autre", "autre clé"));
    }

    #[test]
    fn events_since_returns_only_new_events_and_advances_the_cursor() {
        let start = latest_seq();
        push_event(log::Level::Warn, "motor3derust::t", "a".into());
        push_event(log::Level::Error, "motor3derust::t", "b".into());
        let (events, next) = events_since(start);
        assert!(events.len() >= 2, "au moins nos deux événements");
        let ours: Vec<_> = events
            .iter()
            .filter(|e| e.target == "motor3derust::t")
            .collect();
        assert_eq!(ours[0].text, "a");
        assert_eq!(ours[1].level, log::Level::Error);
        assert!(next >= start + 2);
        let (again, _) = events_since(next);
        assert!(again.is_empty());
    }
    #[test]
    fn diagnostic_report_names_version_os_and_redacts_the_home_directory() {
        super::install();
        log::info!(
            "ligne de test dans {}/exemple",
            std::env::var("HOME").unwrap_or_default()
        );
        let report = super::diagnostic_report();
        assert!(report.contains("RusteeGear"));
        assert!(report.contains("Developer Preview 1"));
        assert!(report.contains(std::env::consts::OS));
        assert!(report.contains("Format de scène : v2"));
        if let Ok(home) = std::env::var("HOME")
            && !home.is_empty()
        {
            assert!(
                !report.contains(&home),
                "le chemin du dossier personnel doit être remplacé par ~"
            );
        }
    }
}
