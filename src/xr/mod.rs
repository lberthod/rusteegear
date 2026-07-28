//! OpenXR — première brique du spike Phase 0 (`docs/XR_PORTAIL_ARENE.md`).
//!
//! Compilé uniquement sur `target_os = "android"` avec `--features xr`.
//! Écrit sans accès à la cible `aarch64-linux-android` ni à un runtime
//! OpenXR dans l'environnement où ce fichier a été rédigé : **jamais compilé
//! ni exécuté**. Premier geste sur une machine avec le NDK Android + un
//! Quest branché : `cargo apk build --features xr` (ou `xbuild` équivalent)
//! et corriger ce que le compilateur signale — en particulier l'init du
//! loader Android ci-dessous, où l'API exacte d'`openxr` 0.21.1 n'a pas pu
//! être vérifiée contre la doc en ligne depuis ici (marqué `TODO(device)`).
//!
//! Ne couvre que l'instance + le system Quest. La session Vulkan (partagée
//! avec `wgpu` via `wgpu-hal`) et le swapchain stéréo viennent une fois
//! cette étape validée sur device — pas avant, pour garder un point de
//! vérification court entre deux inconnues techniques.

use openxr as xr;

/// Instance OpenXR + system Quest (form factor casque). Point de départ du
/// spike : si ça se crée sans erreur sur device, le loader/manifest Android
/// sont corrects et on peut avancer vers la session/swapchain.
pub struct XrEntryPoint {
    pub instance: xr::Instance,
    pub system: xr::SystemId,
}

impl XrEntryPoint {
    /// `vm`/`activity` : pointeurs JNI (`JavaVM*`, `jobject`) fournis par
    /// `android-activity` au démarrage (`AndroidApp::vm_as_ptr()` /
    /// `activity_as_ptr()` — noms à confirmer contre android-activity 0.6.1
    /// sur device, cf. `android_main` dans `src/lib.rs`).
    pub fn new(
        vm: *mut std::ffi::c_void,
        activity: *mut std::ffi::c_void,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // TODO(device) : openxrs initialise le loader Android en interne à
        // partir de (vm, activity) (cf. recherche du 2026-07-28,
        // `docs/XR_PORTAIL_ARENE.md` § écosystème) — nom de fonction exact
        // à confirmer une fois le NDK disponible. `vm`/`activity` sont pris
        // en paramètres dès maintenant pour ne pas avoir à retoucher la
        // signature de `new` quand l'appel réel sera branché.
        let _ = (vm, activity);
        let entry = xr::Entry::load()?;

        let available = entry.enumerate_extensions()?;
        let mut required = xr::ExtensionSet::default();
        required.khr_vulkan_enable2 = true;
        // Passthrough demandé si le runtime l'expose ; sinon on continue sans
        // (mode stéréo simple, sans passthrough) plutôt que d'échouer dur ici
        // — la Phase 1 vérifiera explicitement que `fb_passthrough` a bien
        // été activée avant de composer la layer passthrough.
        if available.fb_passthrough {
            required.fb_passthrough = true;
        }

        let instance = entry.create_instance(
            &xr::ApplicationInfo {
                application_name: "RusteeGear",
                application_version: 0,
                engine_name: "RusteeGear",
                engine_version: 0,
                api_version: xr::Version::new(1, 0, 0),
            },
            &required,
            &[],
        )?;

        let system = instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;

        log::info!("OpenXR : instance + system Quest créés (spike Phase 0 OK jusqu'ici)");

        Ok(Self { instance, system })
    }
}
