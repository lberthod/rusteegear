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

/// Logger qui écrit à la fois sur stderr (`env_logger`) et dans le tampon mémoire.
struct CaptureLogger {
    inner: env_logger::Logger,
}

impl Log for CaptureLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if self.inner.enabled(record.metadata())
            && let Ok(mut buf) = BUFFER.lock()
        {
            if buf.len() >= CAPACITY {
                buf.pop_front();
            }
            buf.push_back(format!("[{}] {}", record.level(), record.args()));
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
