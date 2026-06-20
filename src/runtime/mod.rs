pub mod audio;
pub mod physics;

/// Retour haptique : déclenche une vibration de `ms` millisecondes.
/// Natif sur Android (à brancher au Vibrator via JNI) ; sur desktop, journalisé.
pub fn vibrate(ms: f32) {
    let ms = ms.clamp(0.0, 2000.0);
    if ms <= 0.0 {
        return;
    }
    #[cfg(target_os = "android")]
    {
        // TODO : appeler android.os.Vibrator via JNI (ndk-context). Journalisé pour l'instant.
        log::info!("Vibration {ms:.0} ms (hook natif Android à brancher)");
    }
    #[cfg(not(target_os = "android"))]
    {
        log::info!("Vibration {ms:.0} ms (desktop : pas de moteur haptique)");
    }
}
