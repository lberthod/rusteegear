//! Panneau « Build & Export » : lance les scripts de packaging (`.dmg` / `.apk` / `.ipa`)
//! depuis l'UI, en thread de fond, avec log streamé.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

use crate::app::build_config::{BuildConfig, RenderQuality};
use crate::app::settings::Settings;
use crate::scene::Scene;

/// Racine du projet, figée à la compilation : les scripts `packaging/*.sh` y résident,
/// quel que soit le répertoire courant (qui vaut « / » quand l'app tourne en `.app`).
const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Plateforme cible d'un export.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Macos,
    Android,
    Ios,
    Web,
}

impl Target {
    fn label(self) -> &'static str {
        match self {
            // « non re-vérifié » (Phase D5, sprint.19matin.md) : cibles qui
            // buildent historiquement mais n'ont pas été re-validées pour la
            // Developer Preview 1 — l'absence de garantie est volontaire et
            // affichée, cf. docs/KNOWN_LIMITATIONS.md. macOS et Web ont été
            // re-vérifiés (Phase C6).
            Target::Macos => "macOS · .dmg",
            Target::Android => "Android · .apk — non re-vérifié (préversion)",
            Target::Ios => "iOS · .ipa — non re-vérifié (préversion)",
            Target::Web => "Web · .zip",
        }
    }

    /// Script de packaging (relatif à la racine projet).
    fn script(self) -> &'static str {
        match self {
            Target::Macos => "packaging/build_dmg.sh",
            Target::Android => "packaging/build_apk.sh",
            Target::Ios => "packaging/build_ios.sh",
            Target::Web => "packaging/build_web.sh",
        }
    }
}

/// Message remonté du thread de build vers l'UI.
enum LogMsg {
    Line(String),
    Done(bool),
}

/// État persistant du panneau (vit dans `Editor`).
pub struct ExportPanel {
    pub open: bool,
    /// Config de build éditable, persistée dans `~/.motor3derust/`.
    config: BuildConfig,
    log: Vec<String>,
    rx: Option<Receiver<LogMsg>>,
    running: Option<Target>,
    /// Pré-requis détectés une fois au démarrage : `Ok` = prêt, `Err` = ce qui manque.
    prereqs: [(Target, Result<(), String>); 4],
    /// Android : installer l'APK sur l'appareil branché (adb) après le build.
    install_device: bool,
    /// `adb` est-il disponible (sinon l'option d'installation est grisée).
    adb_available: bool,
    /// Le dernier export s'est-il terminé avec succès (affiche « Révéler le dossier »).
    last_ok: bool,
    /// Nom du préréglage à enregistrer.
    preset_name: String,
    /// Préréglages disponibles (rafraîchi à l'enregistrement).
    presets: Vec<String>,
    /// File d'attente d'exports (« Tout exporter ») démarrés un par un.
    queue: Vec<Target>,
}

impl ExportPanel {
    /// Configuration de build courante (lecture seule), pour le contrôle qualité APK.
    pub fn config(&self) -> &BuildConfig {
        &self.config
    }
}

impl Default for ExportPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportPanel {
    pub fn new() -> Self {
        ExportPanel {
            open: false,
            config: BuildConfig::load(),
            log: Vec::new(),
            rx: None,
            running: None,
            prereqs: [
                (Target::Macos, detect(Target::Macos)),
                (Target::Android, detect(Target::Android)),
                (Target::Ios, detect(Target::Ios)),
                (Target::Web, detect(Target::Web)),
            ],
            install_device: false,
            adb_available: has_cmd("adb"),
            last_ok: false,
            preset_name: "Démo".into(),
            presets: BuildConfig::list_presets(),
            queue: Vec::new(),
        }
    }

    fn prereq(&self, target: Target) -> Result<(), String> {
        self.prereqs
            .iter()
            .find(|(t, _)| *t == target)
            .map(|(_, r)| r.clone())
            .unwrap_or(Ok(()))
    }

    /// Démarre un export en arrière-plan (un seul à la fois).
    /// Valide la config, l'incrémente/persiste, écrit la scène à embarquer, puis build.
    fn start(&mut self, target: Target, scene: &Scene, settings: &Settings) {
        self.log.clear();
        if let Err(e) = self.config.validate() {
            self.log.push(format!("❌ Config invalide : {e}"));
            return;
        }

        // Embarque la scène + ses assets (modèles glTF, sons) dans le binaire.
        let scene_path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/player_scene.json");
        let bundled = bundle_scene_json(scene).and_then(|(json, warns)| {
            std::fs::write(scene_path, json)
                .map(|_| warns)
                .map_err(|e| e.to_string())
        });
        match bundled {
            Ok(warns) => {
                self.log.push("✓ Scène + assets embarqués.".into());
                for w in warns {
                    self.log.push(format!("⚠ {w}"));
                }
            }
            Err(e) => {
                self.log
                    .push(format!("❌ Impossible d'embarquer la scène : {e}"));
                return;
            }
        }

        // Sprint 3 (PHASE A, config hors éditeur) : embarque la config Firebase courante dans
        // le bundle, pour qu'un `.app`/APK exporté fonctionne sans saisie manuelle (cf.
        // `app::settings::Settings::load`, `assets::default_settings_json`).
        let bundle_dir = std::path::Path::new(PROJECT_ROOT).join("assets/bundle");
        if bake_default_settings_at(&bundle_dir, settings) {
            self.log
                .push("✓ Config Firebase courante embarquée (settings.json par défaut).".into());
        }

        // Incrémente le numéro de build et persiste la config.
        self.config.build_number += 1;
        self.config.save();

        let cfg = self.config.clone();
        self.log.push(format!(
            "▶ Export « {} » v{} (build {}) — {}…",
            cfg.safe_name(),
            cfg.version,
            cfg.build_number,
            target.label()
        ));
        // installation device : case cochée (Android requiert adb, iOS devicectl/Xcode).
        let install = self.install_device
            && match target {
                Target::Android => self.adb_available,
                Target::Ios => true,
                Target::Macos | Target::Web => false,
            };
        self.rx = Some(run(target, cfg, install));
        self.running = Some(target);
        self.last_ok = false;
    }

    /// Toolbar « Run Device » : ouvre le panneau, force l'installation sur appareil
    /// et lance un build Android (si l'appareil/adb sont prêts).
    pub fn run_on_device(&mut self, scene: &Scene, settings: &Settings) {
        self.open = true;
        if self.running.is_some() {
            return; // un build est déjà en cours
        }
        self.install_device = true;
        if self.adb_available {
            self.start(Target::Android, scene, settings);
        } else {
            self.log
                .push("❌ Aucun appareil Android détecté (adb). Branche un téléphone.".into());
        }
    }

    /// Actions « Logs ADB » : récupère les dernières lignes du logcat de l'appareil
    /// branché et les ajoute au journal (utile pour diagnostiquer un crash mobile).
    fn dump_adb_logs(&mut self) {
        if !self.adb_available {
            self.log.push("❌ adb introuvable.".into());
            return;
        }
        self.log.push("▶ adb logcat (300 dernières lignes)…".into());
        match Command::new("adb")
            .args(["logcat", "-d", "-t", "300"])
            .env("PATH", augmented_path())
            .output()
        {
            Ok(out) if out.status.success() => {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    self.log.push(line.to_string());
                }
                while self.log.len() > 1000 {
                    self.log.remove(0);
                }
            }
            Ok(_) => self
                .log
                .push("⚠ adb logcat : aucun appareil ou accès refusé.".into()),
            Err(e) => self.log.push(format!("❌ adb logcat : {e}")),
        }
    }

    /// Construit la fenêtre egui (à appeler chaque frame avec le contexte).
    pub fn ui(&mut self, ctx: &egui::Context, scene: &Scene, settings: &Settings) {
        // Récupère les lignes de log produites par le thread de build.
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    LogMsg::Line(l) => {
                        self.log.push(l);
                        if self.log.len() > 1000 {
                            self.log.remove(0);
                        }
                    }
                    LogMsg::Done(ok) => {
                        self.log.push(
                            if ok {
                                "✅ Export terminé."
                            } else {
                                "❌ Export échoué (voir le journal)."
                            }
                            .to_string(),
                        );
                        self.running = None;
                        self.last_ok = ok;
                    }
                }
            }
        }

        let mut open = self.open;
        egui::Window::new("📦 Build & Export")
            .open(&mut open)
            .resizable(true)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label("Exporte un player jouable du jeu créé (scène embarquée).");
                // Récap de CE qui sera embarqué : évite de builder la mauvaise scène.
                let controllable = scene
                    .objects
                    .iter()
                    .filter(|o| o.controller.as_ref().is_some_and(|c| c.input || c.gyro))
                    .count();
                let scripted = scene
                    .objects
                    .iter()
                    .filter(|o| !o.script.trim().is_empty())
                    .count();
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new("📦 Scène embarquée (au Build)").strong());
                    ui.label(format!(
                        "{} objet(s) · joystick {} · {} pilotable(s) · {} scripté(s)",
                        scene.objects.len(),
                        if scene.mobile.joystick { "ON" } else { "OFF" },
                        controllable,
                        scripted,
                    ));
                    if controllable == 0 && scripted == 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 170, 60),
                            "⚠ Aucun objet pilotable ni scripté : rien ne bougera en jeu.",
                        );
                    }
                });
                ui.add_space(4.0);

                // --- 📱 Application ---
                egui::CollapsingHeader::new("📱  Application")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("build_cfg")
                            .num_columns(2)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("Nom de l'app");
                                ui.text_edit_singleline(&mut self.config.app_name);
                                ui.end_row();
                                ui.label("Bundle id / package");
                                ui.text_edit_singleline(&mut self.config.bundle_id);
                                ui.end_row();
                                ui.label("Version");
                                ui.text_edit_singleline(&mut self.config.version);
                                ui.end_row();
                                ui.label("Build #");
                                ui.label(self.config.build_number.to_string());
                                ui.end_row();
                                ui.label("Orientation");
                                egui::ComboBox::from_id_salt("orient_cb")
                                    .selected_text(self.config.orientation.label())
                                    .show_ui(ui, |ui| {
                                        use crate::app::build_config::Orientation as O;
                                        for o in [O::Sensor, O::Portrait, O::Landscape] {
                                            ui.selectable_value(
                                                &mut self.config.orientation,
                                                o,
                                                o.label(),
                                            );
                                        }
                                    });
                                ui.end_row();
                                ui.label("min SDK");
                                ui.add(egui::DragValue::new(&mut self.config.min_sdk).range(21..=35));
                                ui.end_row();
                                ui.label("target SDK");
                                ui.add(
                                    egui::DragValue::new(&mut self.config.target_sdk).range(21..=35),
                                );
                                ui.end_row();
                            });
                        // Icône + splash : sélection de fichiers PNG.
                        asset_picker(ui, "Icône (PNG)", &mut self.config.icon_path);
                        asset_picker(ui, "Splash (PNG)", &mut self.config.splash_path);
                    });

                // --- 🎨 Rendu ---
                egui::CollapsingHeader::new("🎨  Rendu")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Backend :");
                            ui.strong("Vulkan");
                            ui.weak("(wgpu sur Android)");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Qualité");
                            egui::ComboBox::from_id_salt("quality_cb")
                                .selected_text(self.config.render_quality.label())
                                .show_ui(ui, |ui| {
                                    use crate::app::build_config::RenderQuality as Q;
                                    for q in [Q::Low, Q::Medium, Q::High] {
                                        ui.selectable_value(
                                            &mut self.config.render_quality,
                                            q,
                                            q.label(),
                                        );
                                    }
                                });
                        });
                        ui.add(
                            egui::Slider::new(&mut self.config.target_fps, 30..=120).text("FPS cible"),
                        );
                        ui.checkbox(&mut self.config.shadows, "Ombres");
                        ui.horizontal(|ui| {
                            ui.label("MSAA");
                            for n in [1u32, 2, 4] {
                                let label = if n == 1 { "off".to_string() } else { format!("×{n}") };
                                ui.selectable_value(&mut self.config.msaa, n, label);
                            }
                        });
                        ui.checkbox(&mut self.config.bloom, "Bloom").on_hover_text(
                            "Halo autour des zones surexposées ; coupé \
                             automatiquement en qualité Basse même si coché ici.",
                        );
                        ui.weak("Persisté + transmis au build ; appliqué par le player là où c'est pris en charge.");
                        if ui
                            .button("⚡ Préréglage performance")
                            .on_hover_text("Qualité basse, ombres off, MSAA off, bloom off, 60 FPS")
                            .clicked()
                        {
                            self.config.render_quality = RenderQuality::Low;
                            self.config.shadows = false;
                            self.config.msaa = 1;
                            self.config.bloom = false;
                            self.config.target_fps = 60;
                        }
                    });

                // --- 📦 Assets ---
                egui::CollapsingHeader::new("📦  Assets")
                    .default_open(false)
                    .show(ui, |ui| {
                        let n_tex = scene.objects.iter().filter(|o| !o.texture.is_empty()).count();
                        let n_audio = scene
                            .objects
                            .iter()
                            .filter(|o| o.audio.as_ref().is_some_and(|a| !a.clip.is_empty()))
                            .count();
                        ui.label(format!("{} modèle(s) importé(s)", scene.imported.len()));
                        ui.label(format!("{n_tex} texture(s), {n_audio} son(s)"));
                        ui.weak(
                            "Les assets référencés sont embarqués (bundle://) au moment du build.",
                        );
                    });

                if let Err(e) = self.config.validate() {
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 120), format!("⚠ {e}"));
                }
                ui.collapsing("🔏  Signature iOS (optionnel)", |ui| {
                    egui::Grid::new("ios_sign").num_columns(2).show(ui, |ui| {
                        ui.label("Team ID");
                        ui.text_edit_singleline(&mut self.config.ios_team_id);
                        ui.end_row();
                        ui.label("Identité");
                        ui.text_edit_singleline(&mut self.config.ios_identity);
                        ui.end_row();
                        ui.label("Profil");
                        ui.horizontal(|ui| {
                            if ui.button("Choisir .mobileprovision…").clicked() {
                                #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("Profil", &["mobileprovision"])
                                    .pick_file()
                                {
                                    self.config.ios_profile = p.to_string_lossy().into_owned();
                                }
                            }
                            let prof = std::path::Path::new(&self.config.ios_profile)
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "(aucun)".into());
                            ui.label(prof);
                        });
                        ui.end_row();
                    });
                    ui.weak("Vides = identité/équipe par défaut du script. Profil requis pour installer sur device.");
                });

                // Préréglages : charger / enregistrer une BuildConfig nommée.
                ui.horizontal(|ui| {
                    ui.label("Préréglage :");
                    egui::ComboBox::from_id_salt("preset_cb")
                        .selected_text(if self.preset_name.is_empty() {
                            "—".to_string()
                        } else {
                            self.preset_name.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in self.presets.clone() {
                                if ui.selectable_label(false, &name).clicked()
                                    && let Some(cfg) = BuildConfig::load_preset(&name)
                                {
                                    self.config = cfg;
                                    self.preset_name = name;
                                }
                            }
                        });
                    ui.text_edit_singleline(&mut self.preset_name);
                    if ui.button("💾").on_hover_text("Enregistrer le préréglage").clicked()
                        && !self.preset_name.trim().is_empty()
                    {
                        self.config.save_preset(self.preset_name.trim());
                        self.presets = BuildConfig::list_presets();
                    }
                });

                ui.add_space(4.0);
                ui.strong("⚙  Actions");
                let targets = [Target::Macos, Target::Android, Target::Ios, Target::Web];
                for t in targets {
                    self.card(ui, t, scene, settings);
                }

                let busy = self.running.is_some() || !self.queue.is_empty();
                ui.horizontal(|ui| {
                    // Export groupé : enfile toutes les cibles prêtes, jouées une par une.
                    if ui
                        .add_enabled(!busy, egui::Button::new("🚀 Tout exporter"))
                        .clicked()
                    {
                        self.queue =
                            targets.into_iter().filter(|t| self.prereq(*t).is_ok()).collect();
                    }
                    // Build + install + lancement Android sur l'appareil branché.
                    if ui
                        .add_enabled(!busy, egui::Button::new("📲 Run"))
                        .on_hover_text("Build Android + installation sur le téléphone (adb)")
                        .clicked()
                    {
                        self.run_on_device(scene, settings);
                    }
                    // Logcat de l'appareil (diagnostic crash mobile).
                    if ui
                        .add_enabled(self.adb_available, egui::Button::new("📋 Logs ADB"))
                        .on_hover_text("Dernières lignes du logcat de l'appareil branché")
                        .clicked()
                    {
                        self.dump_adb_logs();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if self.last_ok && ui.button("📂 Révéler le dossier").clicked() {
                        let _ = Command::new("open")
                            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/target/export"))
                            .spawn();
                    }
                    ui.label("Journal :");
                });
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.log.is_empty() {
                            ui.weak("(aucun export lancé)");
                        }
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });
        self.open = open;

        // Avance la file « Tout exporter » : démarre la cible suivante dès que libre.
        if self.running.is_none() && !self.queue.is_empty() {
            let next = self.queue.remove(0);
            self.start(next, scene, settings);
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, target: Target, scene: &Scene, settings: &Settings) {
        let prereq = self.prereq(target);
        let busy = self.running.is_some();
        let mut launch = false;
        ui.horizontal(|ui| {
            ui.strong(target.label());
            match &prereq {
                Ok(()) => {
                    ui.colored_label(egui::Color32::from_rgb(80, 200, 120), "✓ prêt");
                }
                Err(msg) => {
                    ui.colored_label(egui::Color32::from_rgb(220, 170, 60), format!("⚠ {msg}"));
                }
            }
            let enabled = prereq.is_ok() && !busy;
            if ui
                .add_enabled(enabled, egui::Button::new("Exporter"))
                .clicked()
            {
                launch = true;
            }
            if self.running == Some(target) {
                ui.spinner();
            }
        });
        // Android / iOS : option d'installation directe sur l'appareil branché.
        match target {
            Target::Android => {
                ui.indent("adb_opt", |ui| {
                    ui.add_enabled_ui(self.adb_available, |ui| {
                        ui.checkbox(&mut self.install_device, "Installer sur l'appareil (adb)");
                    });
                    if !self.adb_available {
                        ui.weak("adb introuvable — installe les Platform-Tools Android.");
                    }
                });
            }
            Target::Ios => {
                ui.indent("ios_opt", |ui| {
                    ui.checkbox(
                        &mut self.install_device,
                        "Installer sur l'iPhone branché (devicectl)",
                    );
                });
            }
            Target::Macos => {}
            Target::Web => {
                ui.indent("web_opt", |ui| {
                    ui.weak(
                        "Zip prêt à servir (HTTP statique, Chrome/WebGPU). Limite web \
                         actuelle : musique en flux absente.",
                    );
                });
            }
        }
        if launch {
            self.start(target, scene, settings);
        }
    }
}

/// Ligne « libellé + sélecteur de fichier PNG + nom court + effacer » pour icône/splash.
fn asset_picker(ui: &mut egui::Ui, label: &str, path: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.button("Choisir…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Image PNG", &["png"])
                .pick_file()
            {
                *path = p.to_string_lossy().into_owned();
            }
        }
        if !path.is_empty() && ui.button("✕").clicked() {
            path.clear();
        }
        let name = if path.is_empty() {
            "(aucun)".to_string()
        } else {
            std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        ui.label(name);
    });
}

/// Détecte les pré-requis d'une cible. `Ok` = prêt à exporter.
fn detect(target: Target) -> Result<(), String> {
    // L'export se pilote depuis le desktop ; rien à sonder sur mobile (pas de processus).
    if cfg!(any(
        target_os = "ios",
        target_os = "android",
        target_arch = "wasm32"
    )) {
        return Err("export depuis le desktop".into());
    }
    match target {
        Target::Macos => {
            if !cfg!(target_os = "macos") {
                return Err("disponible sur macOS uniquement".into());
            }
            if !has_cmd("cargo-bundle") {
                return Err("cargo install cargo-bundle".into());
            }
            Ok(())
        }
        Target::Android => {
            if !has_cmd("cargo-apk") {
                return Err("cargo install cargo-apk".into());
            }
            if find_ndk().is_none() {
                return Err("NDK introuvable (installer via Android Studio)".into());
            }
            if !rust_target_installed("aarch64-linux-android") {
                return Err("rustup target add aarch64-linux-android".into());
            }
            Ok(())
        }
        Target::Ios => {
            if !cfg!(target_os = "macos") {
                return Err("disponible sur macOS uniquement".into());
            }
            if !has_cmd("xcodegen") {
                return Err("brew install xcodegen".into());
            }
            if !rust_target_installed("aarch64-apple-ios") {
                return Err("rustup target add aarch64-apple-ios".into());
            }
            if !has_signing_identity() {
                return Err("aucune identité « Apple Development » dans le trousseau".into());
            }
            Ok(())
        }
        Target::Web => {
            if !rust_target_installed("wasm32-unknown-unknown") {
                return Err("rustup target add wasm32-unknown-unknown".into());
            }
            if !has_cmd("wasm-bindgen") {
                return Err("cargo install wasm-bindgen-cli".into());
            }
            // `wasm-bindgen-cli` doit être à la version EXACTE de la crate du
            // lockfile (contrainte de l'outil lui-même, cf. build_web.sh) — un écart
            // ferait échouer le build avec un message cryptique en plein export.
            if let Some(expected) = lockfile_wasm_bindgen_version()
                && let Some(installed) = installed_wasm_bindgen_version()
                && expected != installed
            {
                return Err(format!(
                    "cargo install wasm-bindgen-cli --version {expected} (installé : {installed})"
                ));
            }
            Ok(())
        }
    }
}

/// Version de la crate `wasm-bindgen` figée dans `Cargo.lock` — la CLI doit être
/// à la même version exacte (contrainte de `wasm-bindgen` lui-même).
fn lockfile_wasm_bindgen_version() -> Option<String> {
    let lock =
        std::fs::read_to_string(std::path::Path::new(PROJECT_ROOT).join("Cargo.lock")).ok()?;
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"wasm-bindgen\"" {
            let version = lines
                .next()?
                .trim()
                .strip_prefix("version = \"")?
                .strip_suffix('"')?;
            return Some(version.to_string());
        }
    }
    None
}

/// Version de la CLI `wasm-bindgen` installée (`wasm-bindgen --version`).
fn installed_wasm_bindgen_version() -> Option<String> {
    let out = Command::new("wasm-bindgen")
        .arg("--version")
        .env("PATH", augmented_path())
        .output()
        .ok()?;
    // Format : « wasm-bindgen 0.2.xx »
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// Dossiers où chercher les outils, même quand l'app est lancée depuis le Finder
/// (PATH minimal hérité, sans `~/.cargo/bin` ni Homebrew).
fn search_dirs() -> Vec<String> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(format!("{home}/.cargo/bin"));
    }
    dirs.push("/opt/homebrew/bin".into()); // Homebrew Apple Silicon
    dirs.push("/usr/local/bin".into()); // Homebrew Intel / installs manuels
    if let Ok(path) = std::env::var("PATH") {
        dirs.extend(path.split(':').map(str::to_string));
    }
    dirs
}

/// `PATH` augmenté à transmettre aux scripts de build (sinon `cargo`/`xcodegen` introuvables).
fn augmented_path() -> String {
    search_dirs().join(":")
}

/// Vrai si une commande exécutable existe dans l'un des dossiers de recherche.
/// `std::os::unix::fs::PermissionsExt` : indisponible sur wasm32 (pas de bit
/// exécutable ni de build tooling à détecter dans un navigateur, cf. `detect`
/// juste plus bas qui traite déjà tout mobile/web comme « export depuis le
/// desktop uniquement », Sprint 114).
#[cfg(not(target_arch = "wasm32"))]
fn has_cmd(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    search_dirs().iter().any(|dir| {
        let p = std::path::Path::new(dir).join(name);
        p.metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

#[cfg(target_arch = "wasm32")]
fn has_cmd(_name: &str) -> bool {
    false
}

/// Localise le NDK Android (variables d'env usuelles, puis l'emplacement par défaut
/// d'Android Studio). `Some(chemin)` si trouvé.
fn find_ndk() -> Option<String> {
    for var in ["ANDROID_NDK_ROOT", "ANDROID_NDK_HOME", "NDK_HOME"] {
        if let Ok(p) = std::env::var(var)
            && !p.is_empty()
            && std::path::Path::new(&p).exists()
        {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let ndk = std::path::Path::new(&home).join("Library/Android/sdk/ndk");
    let first = std::fs::read_dir(ndk).ok()?.flatten().next()?;
    Some(first.path().to_string_lossy().into_owned())
}

/// Vrai si le trousseau contient au moins une identité de signature de code Apple.
fn has_signing_identity() -> bool {
    Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Apple Development"))
        .unwrap_or(false)
}

/// Vrai si la cible Rust est installée (`rustup target list --installed`).
fn rust_target_installed(target: &str) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .env("PATH", augmented_path())
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == target)
        })
        .unwrap_or(false)
}

/// Réinitialise `assets/bundle/`, y copie les assets référencés par la scène
/// (modèles glTF, sons) et renvoie le JSON de la scène avec les chemins réécrits en
/// `bundle://…`, plus la liste des assets introuvables (avertissements).
fn bundle_scene_json(scene: &Scene) -> Result<(String, Vec<String>), String> {
    let dir = std::path::Path::new(PROJECT_ROOT).join("assets/bundle");
    bundle_scene_json_at(&dir, scene)
}

/// Variante paramétrée par le dossier bundle, pour rester testable sans toucher au
/// vrai `assets/bundle/` du dépôt (même patron que `write_default_settings`).
fn bundle_scene_json_at(
    dir: &std::path::Path,
    scene: &Scene,
) -> Result<(String, Vec<String>), String> {
    let mut val = serde_json::to_value(scene).map_err(|e| e.to_string())?;
    let mut warns = Vec::new();

    // Garde-fou (audit 2026-07-20, risque A1) : une scène déjà exportée — le player
    // rechargé, typiquement — ne référence QUE des chemins `bundle://…`, que
    // `copy_to_bundle` ignore (`Ok(None)`). Sans cette sauvegarde, le
    // `remove_dir_all` ci-dessous vidait le bundle sans rien y recopier et le
    // binaire suivant n'avait plus aucun asset. On garde donc en mémoire, AVANT de
    // vider le dossier, les octets (déjà compressés zstd) de chaque clé bundle
    // référencée — depuis le disque, sinon depuis le bundle embarqué au build.
    let preserved = preserve_bundled(dir, &val, &mut warns);

    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let _ = std::fs::write(dir.join(".gitkeep"), b"");
    for (key, bytes) in &preserved {
        std::fs::write(dir.join(key), bytes)
            .map_err(|e| format!("réécriture de bundle://{key} : {e}"))?;
    }

    if let Some(arr) = val.get_mut("imported").and_then(|v| v.as_array_mut()) {
        for (i, m) in arr.iter_mut().enumerate() {
            if let Some(p) = m.get("path").and_then(|v| v.as_str()).map(str::to_string) {
                match copy_to_bundle(dir, &p, &format!("m{i}")) {
                    Ok(Some(key)) => m["path"] = serde_json::Value::String(key),
                    Ok(None) => {}
                    Err(e) => warns.push(e),
                }
            }
        }
    }
    if let Some(arr) = val.get_mut("objects").and_then(|v| v.as_array_mut()) {
        for (i, o) in arr.iter_mut().enumerate() {
            if let Some(p) = o
                .get("audio")
                .and_then(|a| a.get("clip"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                match copy_to_bundle(dir, &p, &format!("a{i}")) {
                    Ok(Some(key)) => o["audio"]["clip"] = serde_json::Value::String(key),
                    Ok(None) => {}
                    Err(e) => warns.push(e),
                }
            }
            if let Some(p) = o
                .get("texture")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                match copy_to_bundle(dir, &p, &format!("t{i}")) {
                    Ok(Some(key)) => o["texture"] = serde_json::Value::String(key),
                    Ok(None) => {}
                    Err(e) => warns.push(e),
                }
            }
        }
    }

    let json = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
    Ok((json, warns))
}

/// Relève dans la scène sérialisée toutes les clés `bundle://…` (mêmes trois champs
/// que `bundle_scene_json_at` : imports, clips audio, textures) et renvoie leurs
/// octets compressés — lus depuis le dossier bundle sur disque, ou recompressés
/// depuis le bundle embarqué (`assets::bundle_bytes`) si le disque ne les a plus.
/// Une clé introuvable des deux côtés devient un avertissement (asset perdu).
fn preserve_bundled(
    dir: &std::path::Path,
    val: &serde_json::Value,
    warns: &mut Vec<String>,
) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut keep = |path: &str| {
        let Some(key) = crate::assets::strip_scheme(path) else {
            return;
        };
        if out.contains_key(key) {
            return;
        }
        let bytes = std::fs::read(dir.join(key))
            .ok()
            .or_else(|| crate::assets::bundle_bytes(key).and_then(|raw| compress(&raw).ok()));
        match bytes {
            Some(b) => {
                out.insert(key.to_string(), b);
            }
            None => warns.push(format!("asset bundlé introuvable, perdu : {path}")),
        }
    };
    if let Some(arr) = val.get("imported").and_then(|v| v.as_array()) {
        for m in arr {
            if let Some(p) = m.get("path").and_then(|v| v.as_str()) {
                keep(p);
            }
        }
    }
    if let Some(arr) = val.get("objects").and_then(|v| v.as_array()) {
        for o in arr {
            if let Some(p) = o
                .get("audio")
                .and_then(|a| a.get("clip"))
                .and_then(|v| v.as_str())
            {
                keep(p);
            }
            if let Some(p) = o.get("texture").and_then(|v| v.as_str()) {
                keep(p);
            }
        }
    }
    out
}

/// Copie un asset disque dans le bundle, compressé zstd (Sprint 127, décompressé côté
/// lecture par `assets::bundle_bytes`) ; renvoie sa clé `bundle://…`.
/// `Ok(None)` si le chemin est vide / déjà embarqué ; `Err` si fichier introuvable/illisible.
fn copy_to_bundle(
    dir: &std::path::Path,
    path: &str,
    prefix: &str,
) -> Result<Option<String>, String> {
    if path.is_empty() || path.starts_with(crate::assets::SCHEME) {
        return Ok(None);
    }
    let src = std::path::Path::new(path);
    if !src.is_file() {
        return Err(format!("asset introuvable, ignoré : {path}"));
    }
    let fname = src.file_name().and_then(|s| s.to_str()).unwrap_or("asset");
    let key = format!("{prefix}_{fname}");
    let data = std::fs::read(src).map_err(|e| format!("lecture de {path} : {e}"))?;
    let compressed = compress(&data)?;
    std::fs::write(dir.join(&key), compressed).map_err(|e| format!("copie de {path} : {e}"))?;
    Ok(Some(format!("{}{key}", crate::assets::SCHEME)))
}

/// Écrit (ou retire) `<bundle_dir>/default_settings.json` selon la config Firebase courante
/// (Sprint 3 de PHASE A, config hors éditeur) — lu au premier lancement par `Settings::load`
/// via `assets::default_settings_json`. Renvoie `true` si un fichier a été écrit (utilisé par
/// l'appelant pour logger). Paramétré par `bundle_dir` pour rester testable sans toucher au
/// vrai `assets/bundle/` du dépôt (même patron que `copy_to_bundle`).
///
/// Retire le fichier plutôt que d'écrire une config vide si Firebase n'est pas configuré :
/// `assets/bundle/` est régénéré à chaque export mais jamais vidé entre deux, donc un export
/// sans clé après un export avec clé laisserait sinon une clé désormais retirée par
/// l'utilisateur traîner, embarquée à son insu dans le prochain build.
fn bake_default_settings_at(bundle_dir: &std::path::Path, settings: &Settings) -> bool {
    let path = bundle_dir.join(crate::assets::DEFAULT_SETTINGS_FILE);
    if settings.firebase_api_key.trim().is_empty()
        || settings.firebase_database_url.trim().is_empty()
    {
        let _ = std::fs::remove_file(&path);
        return false;
    }
    let baked = serde_json::json!({
        "firebase_api_key": settings.firebase_api_key,
        "firebase_database_url": settings.firebase_database_url,
    });
    let Ok(json) = serde_json::to_string_pretty(&baked) else {
        return false;
    };
    if std::fs::create_dir_all(bundle_dir).is_err() {
        return false;
    }
    std::fs::write(&path, json).is_ok()
}

/// Compression zstd (niveau par défaut) — jamais appelée en pratique sur wasm32
/// (l'éditeur n'y tourne pas), mais `editor::export` est compilé pour toutes les
/// cibles (`pub mod editor` inconditionnel dans `src/lib.rs`) : repli sans
/// compression pour cette cible plutôt que de tirer `zstd` (bindings C, cf.
/// `Cargo.toml`) dans le build wasm32.
#[cfg(not(target_arch = "wasm32"))]
fn compress(data: &[u8]) -> Result<Vec<u8>, String> {
    zstd::stream::encode_all(data, 0).map_err(|e| format!("compression zstd : {e}"))
}

#[cfg(target_arch = "wasm32")]
fn compress(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

/// Lance le script de packaging en thread de fond ; renvoie le canal de log.
/// La `BuildConfig` est transmise via variables d'environnement.
fn run(target: Target, cfg: BuildConfig, install: bool) -> Receiver<LogMsg> {
    let (tx, rx) = channel();
    // iOS + installation = chemin xcodebuild/devicectl dédié (build + signature + install).
    let script = if target == Target::Ios && install {
        "packaging/install_ios_device.sh"
    } else {
        target.script()
    };
    std::thread::spawn(move || {
        let mut cmd = Command::new("bash");
        cmd.arg(script)
            .current_dir(PROJECT_ROOT)
            .env("PATH", augmented_path())
            .env("OUTPUT_NAME", cfg.safe_name())
            .env("APP_NAME", &cfg.app_name)
            .env("BUNDLE_ID", &cfg.bundle_id)
            .env("APP_VERSION", &cfg.version)
            .env("BUILD_NUMBER", cfg.build_number.to_string())
            .env("INSTALL_DEVICE", if install { "1" } else { "0" })
            .env("PLAYER_BUILD", "1") // exporte un player jouable (cf. build_dmg.sh)
            // --- Application Android ---
            .env("ANDROID_ORIENTATION", cfg.orientation.manifest_value())
            .env("MIN_SDK", cfg.min_sdk.to_string())
            .env("TARGET_SDK", cfg.target_sdk.to_string())
            .env("ICON_PATH", &cfg.icon_path)
            // --- Rendu mobile (consommé par le player là où c'est pris en charge) ---
            .env("TARGET_FPS", cfg.target_fps.to_string())
            .env("SHADOWS", if cfg.shadows { "1" } else { "0" })
            .env("MSAA", cfg.msaa.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Signature iOS : ne surcharge que les champs renseignés.
        if !cfg.ios_team_id.is_empty() {
            cmd.env("TEAM_ID", &cfg.ios_team_id);
        }
        if !cfg.ios_identity.is_empty() {
            cmd.env("IDENTITY", &cfg.ios_identity);
        }
        if !cfg.ios_profile.is_empty() {
            cmd.env("PROFILE", &cfg.ios_profile);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(LogMsg::Line(format!(
                    "Échec du lancement de {script} : {e}"
                )));
                let _ = tx.send(LogMsg::Done(false));
                return;
            }
        };

        // stderr lu dans un thread parallèle, fusionné au flux principal.
        let stderr = child.stderr.take();
        let tx_err = tx.clone();
        let err_handle = stderr.map(|err| {
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    let _ = tx_err.send(LogMsg::Line(line));
                }
            })
        });

        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(LogMsg::Line(line));
            }
        }
        if let Some(h) = err_handle {
            let _ = h.join();
        }

        let ok = child.wait().map(|s| s.success()).unwrap_or(false);
        let _ = tx.send(LogMsg::Done(ok));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dossier temporaire unique par test — même raison que `assets::tests::
    /// temp_assets_dir` (tests parallèles, pas de mutation d'état global).
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        tag.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        let dir =
            std::env::temp_dir().join(format!("rusteegear_export_test_{:x}", hasher.finish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Sprint 3 (PHASE A, config hors éditeur) : une config Firebase renseignée est
    /// embarquée telle quelle, et redevient absente du bundle si l'utilisateur vide la clé
    /// avant un export suivant — pas de clé retirée qui traînerait embarquée à son insu.
    #[test]
    fn bake_default_settings_writes_firebase_fields_and_removes_them_once_cleared() {
        let dir = temp_dir("bake_default_settings");
        let mut settings = Settings {
            firebase_api_key: "AIzaTest".to_string(),
            firebase_database_url: "https://x-default-rtdb.firebaseio.com".to_string(),
            ..Settings::default()
        };

        assert!(bake_default_settings_at(&dir, &settings));
        let path = dir.join(crate::assets::DEFAULT_SETTINGS_FILE);
        let json = std::fs::read_to_string(&path).expect("fichier écrit");
        assert!(json.contains("AIzaTest"));
        assert!(json.contains("x-default-rtdb.firebaseio.com"));
        // Le JSON écrit doit rester lisible par `Settings` (mêmes défauts `#[serde(default)]`
        // que le reste du fichier réel — cf. `an_old_settings_file_without_gamepad_field_
        // loads_with_default_bindings` côté `app::settings`).
        let parsed: Settings = serde_json::from_str(&json).expect("JSON valide pour Settings");
        assert_eq!(parsed.firebase_api_key, "AIzaTest");

        settings.firebase_api_key.clear();
        assert!(!bake_default_settings_at(&dir, &settings));
        assert!(
            !path.exists(),
            "clé Firebase vidée : le settings.json par défaut doit être retiré du bundle"
        );
    }

    #[test]
    fn copy_to_bundle_writes_a_real_zstd_frame_that_shrinks_and_round_trips() {
        let work = temp_dir("copy_to_bundle");
        let bundle = work.join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        let original: Vec<u8> = b"contenu de test tres repetitif "
            .iter()
            .cycle()
            .take(4096)
            .copied()
            .collect();
        let src = work.join("modele.glb");
        std::fs::write(&src, &original).unwrap();

        let key = copy_to_bundle(&bundle, src.to_str().unwrap(), "m0")
            .expect("copie attendue")
            .expect("chemin non vide, pas déjà embarqué");
        assert_eq!(key, format!("{}m0_modele.glb", crate::assets::SCHEME));

        let written = std::fs::read(bundle.join("m0_modele.glb")).unwrap();
        assert!(
            written.len() < original.len(),
            "un contenu répétitif doit rétrécir : {} -> {}",
            original.len(),
            written.len()
        );
        let decoded = zstd::stream::decode_all(&written[..]).expect("flux zstd valide attendu");
        assert_eq!(decoded, original);
    }

    /// Garde-fou A1 (audit 2026-07-20) : ré-exporter une scène dont les imports sont
    /// déjà `bundle://…` (le player rechargé) ne doit PAS vider le bundle — avant ce
    /// correctif, `remove_dir_all` effaçait tout et `copy_to_bundle` ne recopiait
    /// rien (`Ok(None)` sur les chemins déjà bundlés).
    #[test]
    fn reexporting_an_already_bundled_scene_preserves_the_bundle() {
        let work = temp_dir("bundle_reexport_guard");
        let original = b"GLBFAKE_octets_du_modele".to_vec();
        let compressed = compress(&original).expect("compression zstd");
        std::fs::write(work.join("m0_tree.glb"), &compressed).expect("écriture asset");

        let mut scene = crate::scene::Scene::default();
        scene.imported.push(crate::scene::ImportedMesh {
            name: "tree".into(),
            path: format!("{}m0_tree.glb", crate::assets::SCHEME),
            ..Default::default()
        });

        let (json, warns) = bundle_scene_json_at(&work, &scene).expect("export réussi");

        assert!(
            warns.is_empty(),
            "aucun asset ne doit être perdu : {warns:?}"
        );
        let kept = std::fs::read(work.join("m0_tree.glb"))
            .expect("l'asset bundlé doit survivre au ré-export");
        let decoded = zstd::stream::decode_all(&kept[..]).expect("flux zstd valide");
        assert_eq!(decoded, original, "octets préservés à l'identique");
        assert!(
            json.contains("bundle://m0_tree.glb"),
            "la clé bundle doit rester stable dans le JSON exporté"
        );
    }

    #[test]
    fn copy_to_bundle_reports_missing_files_instead_of_panicking() {
        let work = temp_dir("copy_to_bundle_missing");
        let err = copy_to_bundle(&work, "/chemin/qui/n/existe/pas.glb", "m0")
            .expect_err("fichier absent attendu");
        assert!(err.contains("introuvable"));
    }

    /// Outil : complète `assets/bundle/` (compressé zstd) pour chaque clé
    /// `bundle://mNN_<fichier>` référencée par `assets/player_scene.json` mais
    /// absente du dossier — sans passer par `bundle_scene_json` (qui *vide*
    /// tout le dossier avant de le repeupler depuis la scène ouverte dans
    /// l'éditeur, donc inutilisable ici sans un éditeur lancé). Retrouve le
    /// fichier source dans `assets/models/<fichier>` (en retirant le préfixe
    /// `mNN_` de la clé, cf. `sync_embedded_scene_hameau_from_the_demo`) et le
    /// copie compressé sous la clé exacte attendue. N'écrase jamais une
    /// entrée déjà présente.
    #[test]
    #[ignore = "outil : complète assets/bundle/, à lancer explicitement"]
    fn bundle_missing_assets_referenced_by_the_embedded_scene() {
        let scene_path = std::path::Path::new(PROJECT_ROOT).join("assets/player_scene.json");
        let json = std::fs::read_to_string(&scene_path).expect("player_scene.json lisible");
        let scene: crate::scene::Scene = serde_json::from_str(&json).expect("scène valide");
        let bundle_dir = std::path::Path::new(PROJECT_ROOT).join("assets/bundle");
        let models_dir = std::path::Path::new(PROJECT_ROOT).join("assets/models");

        let mut added = Vec::new();
        let mut missing_source = Vec::new();
        for m in &scene.imported {
            let Some(key) = m.path.strip_prefix(crate::assets::SCHEME) else {
                continue;
            };
            let dest = bundle_dir.join(key);
            if dest.exists() {
                continue;
            }
            // Retire le préfixe numérique `mNN_` pour retrouver le nom de
            // fichier d'origine dans assets/models/.
            let file = key
                .strip_prefix('m')
                .and_then(|rest| rest.find('_').map(|us| &rest[us + 1..]))
                .unwrap_or(key);
            let src = models_dir.join(file);
            if !src.is_file() {
                missing_source.push(key.to_string());
                continue;
            }
            let data = std::fs::read(&src).expect("lecture du modèle source");
            let compressed = compress(&data).expect("compression zstd");
            std::fs::write(&dest, compressed).expect("écriture dans assets/bundle/");
            added.push(key.to_string());
        }

        println!("Ajoutés au bundle : {added:?}");
        assert!(
            missing_source.is_empty(),
            "sources introuvables dans assets/models/ pour : {missing_source:?}"
        );
    }
}
