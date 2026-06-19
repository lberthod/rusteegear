//! Panneau « Build & Export » : lance les scripts de packaging (`.dmg` / `.apk` / `.ipa`)
//! depuis l'UI, en thread de fond, avec log streamé. Sprint 19.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

use crate::app::build_config::BuildConfig;
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
}

impl Target {
    fn label(self) -> &'static str {
        match self {
            Target::Macos => "macOS · .dmg",
            Target::Android => "Android · .apk",
            Target::Ios => "iOS · .ipa",
        }
    }

    /// Script de packaging (relatif à la racine projet).
    fn script(self) -> &'static str {
        match self {
            Target::Macos => "packaging/build_dmg.sh",
            Target::Android => "packaging/build_apk.sh",
            Target::Ios => "packaging/build_ios.sh",
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
    prereqs: [(Target, Result<(), String>); 3],
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
    fn start(&mut self, target: Target, scene: &Scene) {
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
                Target::Macos => false,
            };
        self.rx = Some(run(target, cfg, install));
        self.running = Some(target);
        self.last_ok = false;
    }

    /// Construit la fenêtre egui (à appeler chaque frame avec le contexte).
    pub fn ui(&mut self, ctx: &egui::Context, scene: &Scene) {
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
                ui.add_space(4.0);
                egui::Grid::new("build_cfg")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Nom de l'app");
                        ui.text_edit_singleline(&mut self.config.app_name);
                        ui.end_row();
                        ui.label("Bundle id");
                        ui.text_edit_singleline(&mut self.config.bundle_id);
                        ui.end_row();
                        ui.label("Version");
                        ui.text_edit_singleline(&mut self.config.version);
                        ui.end_row();
                        ui.label("Build #");
                        ui.label(self.config.build_number.to_string());
                        ui.end_row();
                    });
                if let Err(e) = self.config.validate() {
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 120), format!("⚠ {e}"));
                }
                ui.collapsing("Signature iOS (optionnel)", |ui| {
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
                                #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
                let targets = [Target::Macos, Target::Android, Target::Ios];
                for t in targets {
                    self.card(ui, t, scene);
                }

                // Export groupé : enfile toutes les cibles prêtes, jouées une par une.
                let busy = self.running.is_some() || !self.queue.is_empty();
                if ui
                    .add_enabled(!busy, egui::Button::new("🚀 Tout exporter (cibles prêtes)"))
                    .clicked()
                {
                    self.queue = targets.into_iter().filter(|t| self.prereq(*t).is_ok()).collect();
                }
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
            self.start(next, scene);
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, target: Target, scene: &Scene) {
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
        }
        if launch {
            self.start(target, scene);
        }
    }
}

/// Détecte les pré-requis d'une cible. `Ok` = prêt à exporter.
fn detect(target: Target) -> Result<(), String> {
    // L'export se pilote depuis le desktop ; rien à sonder sur mobile (pas de processus).
    if cfg!(any(target_os = "ios", target_os = "android")) {
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
    }
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
fn has_cmd(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    search_dirs().iter().any(|dir| {
        let p = std::path::Path::new(dir).join(name);
        p.metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
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
    use std::path::Path;
    let dir = Path::new(PROJECT_ROOT).join("assets/bundle");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::write(dir.join(".gitkeep"), b"");

    let mut val = serde_json::to_value(scene).map_err(|e| e.to_string())?;
    let mut warns = Vec::new();

    if let Some(arr) = val.get_mut("imported").and_then(|v| v.as_array_mut()) {
        for (i, m) in arr.iter_mut().enumerate() {
            if let Some(p) = m.get("path").and_then(|v| v.as_str()).map(str::to_string) {
                match copy_to_bundle(&dir, &p, &format!("m{i}")) {
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
                .get("audio_clip")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                match copy_to_bundle(&dir, &p, &format!("a{i}")) {
                    Ok(Some(key)) => o["audio_clip"] = serde_json::Value::String(key),
                    Ok(None) => {}
                    Err(e) => warns.push(e),
                }
            }
            if let Some(p) = o
                .get("texture")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                match copy_to_bundle(&dir, &p, &format!("t{i}")) {
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

/// Copie un asset disque dans le bundle ; renvoie sa clé `bundle://…`.
/// `Ok(None)` si le chemin est vide / déjà embarqué ; `Err` si fichier introuvable.
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
    std::fs::copy(src, dir.join(&key)).map_err(|e| format!("copie de {path} : {e}"))?;
    Ok(Some(format!("{}{key}", crate::assets::SCHEME)))
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
            .env("BUNDLE_ID", &cfg.bundle_id)
            .env("APP_VERSION", &cfg.version)
            .env("BUILD_NUMBER", cfg.build_number.to_string())
            .env("INSTALL_DEVICE", if install { "1" } else { "0" })
            .env("PLAYER_BUILD", "1") // exporte un player jouable (cf. build_dmg.sh)
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
