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
    let logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
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
