//! Journal de crash (Sprint 113) : capture un panic dans `user://crash_log.txt`
//! pour qu'il survive à la fermeture de l'app — un panic Android n'a autrement
//! de trace que logcat, souvent inaccessible à l'utilisateur final qui rapporte le
//! bug. Écriture locale **uniquement** : aucun envoi automatique, par principe.
//! L'utilisateur choisit d'exporter/copier le texte depuis un écran dédié
//! (`editor::windows::crash_log_window`), jamais en tâche de fond.

const CRASH_LOG_FILE: &str = "crash_log.txt";

/// Installe le hook de panic : écrit message + emplacement + pile d'appels dans
/// `user://crash_log.txt`, puis délègue au hook précédent (comportement stderr
/// habituel inchangé). À appeler une fois au démarrage, **après**
/// `assets::set_android_data_dir` sur Android — sinon `user_dir()` est
/// indisponible et l'écriture échoue silencieusement (pas grave en soi : juste
/// pas de fichier cette fois, le hook par défaut s'exécute quand même).
pub fn install() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crate::assets::write_user_bytes(CRASH_LOG_FILE, format_panic(info).as_bytes());
        default_hook(info);
    }));
}

fn format_panic(info: &std::panic::PanicHookInfo) -> String {
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "emplacement inconnu".to_string());
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic sans message".to_string());
    let backtrace = std::backtrace::Backtrace::force_capture();
    format!(
        "RusteeGear a planté.\n\nEmplacement : {location}\nMessage : {message}\n\n\
         Pile d'appels :\n{backtrace}\n"
    )
}

/// Lit le journal de crash s'il existe — `None` si aucun crash depuis le dernier
/// `clear()` (ou si le fichier n'a jamais été écrit). Lu par l'écran d'envoi
/// volontaire au lancement suivant.
pub fn read() -> Option<String> {
    let bytes = crate::assets::read_user_bytes(CRASH_LOG_FILE)?;
    let text = String::from_utf8(bytes).ok()?;
    (!text.is_empty()).then_some(text)
}

/// Supprime le journal après consultation, pour ne pas le reproposer indéfiniment
/// une fois vu (bouton « Fermer » de l'écran dédié).
pub fn clear() {
    if let Some(dir) = crate::assets::user_dir() {
        let _ = std::fs::remove_file(dir.join(CRASH_LOG_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `format_panic` prend un `&PanicHookInfo`, un type sans constructeur public —
    /// la seule façon d'en obtenir un est de déclencher un vrai panic sous un hook
    /// (`catch_unwind` l'empêche de terminer le process, `panic = "abort"` du
    /// `Cargo.toml` ne s'applique qu'au profil `release`, pas à `cargo test`).
    /// Hook restauré immédiatement après pour ne pas affecter les autres tests
    /// (le hook est un état global du process).
    #[test]
    fn format_panic_includes_the_message_and_the_source_location() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured2 = captured.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            *captured2.lock().unwrap() = format_panic(info);
        }));
        let result = std::panic::catch_unwind(|| {
            panic!("message de test bien identifiable");
        });
        std::panic::set_hook(previous);

        assert!(result.is_err());
        let text = captured.lock().unwrap().clone();
        assert!(
            text.contains("message de test bien identifiable"),
            "le texte capturé doit contenir le message du panic : {text}"
        );
        assert!(
            text.contains("crash_log.rs"),
            "le texte capturé doit contenir le fichier source du panic : {text}"
        );
    }

    #[test]
    fn empty_crash_log_content_is_treated_as_no_crash() {
        // Reflète le comportement de `read()` sans toucher au vrai `user_dir()` :
        // `text.is_empty()` doit rendre `None`, jamais `Some("")`.
        let text = String::new();
        assert!((!text.is_empty()).then_some(text).is_none());
    }
}
