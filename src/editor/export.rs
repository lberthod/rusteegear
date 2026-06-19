//! Panneau « Build & Export » : lance les scripts de packaging (`.dmg` / `.apk` / `.ipa`)
//! depuis l'UI, en thread de fond, avec log streamé. Sprint 19.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

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
    log: Vec<String>,
    rx: Option<Receiver<LogMsg>>,
    running: Option<Target>,
    /// Pré-requis détectés une fois au démarrage : `Ok` = prêt, `Err` = ce qui manque.
    prereqs: [(Target, Result<(), String>); 3],
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
            log: Vec::new(),
            rx: None,
            running: None,
            prereqs: [
                (Target::Macos, detect(Target::Macos)),
                (Target::Android, detect(Target::Android)),
                (Target::Ios, detect(Target::Ios)),
            ],
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
    fn start(&mut self, target: Target) {
        self.log.clear();
        self.log
            .push(format!("▶ Export {} en cours…", target.label()));
        self.rx = Some(run(target));
        self.running = Some(target);
    }

    /// Construit la fenêtre egui (à appeler chaque frame avec le contexte).
    pub fn ui(&mut self, ctx: &egui::Context) {
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
                ui.label("Génère un livrable signé pour chaque plateforme.");
                ui.add_space(4.0);
                let targets = [Target::Macos, Target::Android, Target::Ios];
                for t in targets {
                    self.card(ui, t);
                }
                ui.separator();
                ui.label("Journal :");
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
    }

    fn card(&mut self, ui: &mut egui::Ui, target: Target) {
        let prereq = self.prereq(target);
        let busy = self.running.is_some();
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
                self.start(target);
            }
            if self.running == Some(target) {
                ui.spinner();
            }
        });
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
                return Err("cargo install cargo-apk + NDK".into());
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

/// Lance le script de packaging en thread de fond ; renvoie le canal de log.
fn run(target: Target) -> Receiver<LogMsg> {
    let (tx, rx) = channel();
    let script = target.script();
    std::thread::spawn(move || {
        let mut child = match Command::new("bash")
            .arg(script)
            .current_dir(PROJECT_ROOT)
            .env("PATH", augmented_path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
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
