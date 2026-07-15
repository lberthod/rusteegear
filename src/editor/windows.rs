//! Fenêtres flottantes de l'éditeur : réglages, multijoueur (connexion, chat,
//! classement), scène assistée par IA, pipeline d'optimisation d'assets, éditeur
//! de scripts, navigateur d'assets, prévisualisation HUD. Extrait de
//! `editor/mod.rs`.

use crate::scene::{
    HudAnchor, HudBinding, HudWidget, HudWidgetKind, MAX_POINT_LIGHTS, MeshKind, PointLight, Scene,
};

use super::{HudPreview, Panels, StatusInfo, UiActions, export, readiness};

/// Fenêtres flottantes des menus « Aide » et « Outils ».
pub(super) fn tool_windows(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    export: &export::ExportPanel,
    status: &StatusInfo,
    console_input: &mut String,
    actions: &mut UiActions,
) {
    // --- Console (logs en mémoire + commandes) ---
    egui::Window::new("🖥  Console")
        .open(&mut panels.console)
        .default_size([460.0, 320.0])
        .show(ctx, |ui| {
            if ui.button("🧹  Effacer").clicked() {
                crate::log_buffer::clear();
            }
            ui.separator();
            // Champ de commande : `timescale <valeur>`, `pause`, `play`, `step`, `tp <x> <y> <z>`,
            // `net_stats` (cf. AppState::run_console_command — liste complète en survol).
            ui.horizontal(|ui| {
                let resp = ui
                    .add(
                        egui::TextEdit::singleline(console_input)
                            .hint_text("timescale 0.5 · pause · play · step · tp 0 1 0 · net_stats")
                            .desired_width(ui.available_width() - 70.0),
                    )
                    .on_hover_text(
                        "Commandes : timescale <v> · pause · play · stop · step · \
                         tp <x> <y> <z> · net_stats",
                    );
                let submit = ui.button("Exécuter").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if submit && !console_input.trim().is_empty() {
                    actions.console_command = Some(console_input.trim().to_string());
                    console_input.clear();
                    resp.request_focus();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in crate::log_buffer::snapshot() {
                        ui.monospace(line);
                    }
                });
        });

    // --- Profiler FPS (n'accumule l'historique que lorsque la fenêtre est ouverte) ---
    if !panels.profiler {
        panels.fps_history.clear();
    } else {
        panels.fps_history.push_back(status.fps);
        while panels.fps_history.len() > 120 {
            panels.fps_history.pop_front();
        }
    }
    let fps_hist = panels.fps_history.clone();
    egui::Window::new("📊  Profiler FPS")
        .open(&mut panels.profiler)
        .resizable(false)
        .show(ctx, |ui| {
            let avg = if fps_hist.is_empty() {
                0.0
            } else {
                fps_hist.iter().sum::<f32>() / fps_hist.len() as f32
            };
            let min = fps_hist.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = fps_hist.iter().cloned().fold(0.0_f32, f32::max);
            ui.label(format!("FPS actuel : {:.0}", status.fps));
            ui.label(format!(
                "min {:.0} · moy {:.0} · max {:.0}",
                if min.is_finite() { min } else { 0.0 },
                avg,
                max
            ));
            ui.label(format!("🧊 {} objets", scene.objects.len()));
            ui.separator();
            // Sparkline simple : barres verticales normalisées sur 60 FPS.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(240.0, 60.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
            let n = fps_hist.len().max(1);
            let bar_w = rect.width() / n as f32;
            for (i, &f) in fps_hist.iter().enumerate() {
                let h = (f / 60.0).clamp(0.0, 1.0) * rect.height();
                let x = rect.left() + i as f32 * bar_w;
                let color = if f >= 55.0 {
                    egui::Color32::from_rgb(80, 200, 120)
                } else if f >= 30.0 {
                    egui::Color32::from_rgb(220, 180, 60)
                } else {
                    egui::Color32::from_rgb(220, 90, 80)
                };
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x, rect.bottom() - h),
                        egui::pos2(x + bar_w.max(1.0), rect.bottom()),
                    ),
                    0.0,
                    color,
                );
            }
            // --- Profiler mémoire (estimation) ---
            ui.separator();
            ui.strong("Mémoire (estimation)");
            let (obj_b, mesh_b, n_tex) = scene.memory_estimate();
            let kb = |b: usize| format!("{:.1} Ko", b as f32 / 1024.0);
            ui.label(format!("Objets : {}", kb(obj_b)));
            ui.label(format!(
                "Meshes importés : {} ({} modèle(s))",
                kb(mesh_b),
                scene.imported.len()
            ));
            ui.label(format!("Textures : {n_tex} unique(s)"));
            // --- Profiler GPU (Sprint 112) : timestamp queries par passe + draw calls ---
            ui.separator();
            ui.strong("GPU (frame précédente)");
            if status.gpu_pass_timings_ms.is_empty() {
                ui.small(
                    "Aucune mesure encore — les timestamp queries démarrent dès que \
                     cette fenêtre est ouverte (coût réel, pas actif sinon). Peut \
                     aussi rester vide si l'adaptateur ne les supporte pas.",
                );
            } else {
                for (name, ms) in status.gpu_pass_timings_ms {
                    ui.label(format!("{name} : {ms:.2} ms"));
                }
            }
            ui.label(format!(
                "🔺 ~{} draw calls (estimation)",
                status.gpu_draw_calls
            ));
        });

    // --- Contrôle qualité APK (APK Readiness Check) ---
    let mut do_analyze = false;
    egui::Window::new("✔  Contrôle qualité APK")
        .open(&mut panels.readiness)
        .default_size([420.0, 380.0])
        .show(ctx, |ui| {
            if panels.readiness_results.is_empty() {
                panels.readiness_results = readiness::analyze(scene, export.config());
            }
            let (ok, warn, fail) = readiness::summary(&panels.readiness_results);
            ui.horizontal(|ui| {
                ui.label(format!("✅ {ok}"));
                ui.label(format!("⚠ {warn}"));
                ui.label(format!("❌ {fail}"));
                if ui.button("🔄  Ré-analyser").clicked() {
                    do_analyze = true;
                }
            });
            if fail == 0 {
                ui.colored_label(
                    egui::Color32::from_rgb(80, 200, 120),
                    "Prêt pour l'export Android 🎉",
                );
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 90, 80),
                    format!("{fail} blocage(s) à corriger avant l'export"),
                );
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for c in &panels.readiness_results {
                        ui.horizontal(|ui| {
                            ui.label(c.status.icon());
                            ui.label(&c.label);
                        });
                    }
                });
        });
    if do_analyze {
        panels.readiness_results = readiness::analyze(scene, export.config());
    }

    egui::Window::new("⌨  Raccourcis clavier")
        .open(&mut panels.shortcuts)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("shortcuts_grid")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for (k, v) in [
                        ("W", "Déplacer (translation)"),
                        ("E", "Tourner (rotation)"),
                        ("R", "Redimensionner (échelle)"),
                        ("F", "Recentrer la caméra sur la sélection"),
                        ("Cmd/Ctrl + Z", "Annuler"),
                        ("Cmd/Ctrl + Maj + Z", "Rétablir"),
                        ("Cmd/Ctrl + D", "Dupliquer la sélection"),
                        ("Suppr", "Supprimer la sélection"),
                    ] {
                        ui.strong(k);
                        ui.label(v);
                        ui.end_row();
                    }
                });
        });

    egui::Window::new("🩺  Diagnostic système")
        .open(&mut panels.diagnostic)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Environnement de build Android :");
            ui.separator();
            let check = |ui: &mut egui::Ui, label: &str, ok: bool| {
                ui.horizontal(|ui| {
                    ui.label(if ok { "✅" } else { "❌" });
                    ui.label(label);
                });
            };
            // Le binaire tourne forcément via la toolchain Rust.
            check(ui, "Rust / Cargo", true);
            let android_sdk = std::env::var("ANDROID_HOME")
                .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
                .is_ok();
            let android_ndk = std::env::var("ANDROID_NDK_HOME")
                .or_else(|_| std::env::var("NDK_HOME"))
                .is_ok();
            check(ui, "Android SDK (ANDROID_HOME)", android_sdk);
            check(ui, "Android NDK (ANDROID_NDK_HOME)", android_ndk);
            ui.separator();
            ui.label(format!(
                "🖥  Backend graphique : {}",
                if cfg!(target_os = "macos") {
                    "Metal"
                } else if cfg!(target_os = "windows") {
                    "DX12 / Vulkan"
                } else {
                    "Vulkan"
                }
            ));
        });

    egui::Window::new("ℹ  À propos de RusteeGear")
        .open(&mut panels.about)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("RusteeGear");
            ui.label("Éditeur 3D en Rust orienté export Android natif.");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.hyperlink_to(
                "github.com/lberthod/rusteegear",
                "https://github.com/lberthod/rusteegear",
            );
        });
}

/// Rectangle (points egui) de la zone de jeu : écran de téléphone centré dans la
/// région `central` si l'aperçu mobile est actif, sinon `central` en entier.
pub(super) fn play_area_rect(central: egui::Rect, preview: bool, portrait: bool) -> egui::Rect {
    if !preview {
        return central;
    }
    let (x, y, w, h) = crate::app::device_rect(central.width(), central.height(), portrait);
    egui::Rect::from_min_size(central.min + egui::vec2(x, y), egui::vec2(w, h))
}

/// Dessine le cadre « téléphone » (biseau arrondi + encoche) autour de la zone de jeu.
pub(super) fn device_bezel(ctx: &egui::Context, rect: egui::Rect) {
    use egui::{Color32, Stroke};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("device_bezel"),
    ));
    // Contour épais arrondi, façon châssis de smartphone.
    painter.rect_stroke(
        rect.expand(6.0),
        22.0,
        Stroke::new(10.0, Color32::from_rgb(20, 20, 24)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        16.0,
        Stroke::new(1.5, Color32::from_white_alpha(40)),
        egui::StrokeKind::Inside,
    );
    // Encoche centrale en haut.
    let notch = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 9.0),
        egui::vec2(rect.width().min(160.0) * 0.45, 14.0),
    );
    painter.rect_filled(notch, 7.0, Color32::from_rgb(20, 20, 24));
}

/// Fenêtre « Paramètres » : clé API DeepSeek, volumes audio (persistés à
/// chaque modification).
pub(super) fn settings_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    settings: &mut crate::app::settings::Settings,
    actions: &mut super::UiActions,
) {
    let mut open = panels.settings;
    egui::Window::new("⚙  Paramètres")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("IA — génération de scripts");
            ui.label("Clé API DeepSeek");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut settings.deepseek_api_key)
                    .password(true)
                    .hint_text("sk-…")
                    .desired_width(280.0),
            );
            if resp.lost_focus() || resp.changed() {
                settings.save();
            }
            ui.label(if settings.deepseek_api_key.trim().is_empty() {
                "❌ Aucune clé : génération IA désactivée"
            } else {
                "✅ Clé enregistrée"
            });
            ui.add_space(6.0);
            ui.label("Modèle");
            ui.horizontal(|ui| {
                for m in ["deepseek-chat", "deepseek-reasoner"] {
                    if ui
                        .selectable_label(settings.deepseek_model == m, m)
                        .clicked()
                    {
                        settings.deepseek_model = m.to_string();
                        settings.save();
                    }
                }
            });
            let resp_m = ui.add(
                egui::TextEdit::singleline(&mut settings.deepseek_model)
                    .hint_text("id du modèle (ex. deepseek-chat)")
                    .desired_width(280.0),
            );
            if resp_m.lost_focus() || resp_m.changed() {
                settings.save();
            }
            ui.small(
                "« deepseek-chat » pointe vers la dernière version. Saisis un id précis au besoin.",
            );
            ui.add_space(6.0);
            ui.label("Température (0 = précis, 1 = créatif)");
            if ui
                .add(egui::Slider::new(
                    &mut settings.deepseek_temperature,
                    0.0..=1.0,
                ))
                .drag_stopped()
            {
                settings.save();
            }
            ui.add_space(6.0);
            ui.hyperlink_to(
                "Obtenir une clé DeepSeek",
                "https://platform.deepseek.com/api_keys",
            );

            ui.add_space(12.0);
            ui.separator();
            ui.heading("Multijoueur — comptes (Firebase)");
            ui.label("Clé API Web Firebase");
            let resp_fb_key = ui.add(
                egui::TextEdit::singleline(&mut settings.firebase_api_key)
                    .password(true)
                    .hint_text("AIza…")
                    .desired_width(280.0),
            );
            if resp_fb_key.lost_focus() || resp_fb_key.changed() {
                settings.save();
            }
            ui.label("URL Realtime Database");
            let resp_fb_url = ui.add(
                egui::TextEdit::singleline(&mut settings.firebase_database_url)
                    .hint_text("https://xxx-default-rtdb.firebaseio.com")
                    .desired_width(280.0),
            );
            if resp_fb_url.lost_focus() || resp_fb_url.changed() {
                settings.save();
            }
            ui.label(
                if settings.firebase_api_key.trim().is_empty()
                    || settings.firebase_database_url.trim().is_empty()
                {
                    "❌ Configuration incomplète : comptes multijoueur désactivés"
                } else {
                    "✅ Configuration enregistrée"
                },
            );
            ui.small(
                "Clé publique par conception côté Firebase — la sécurité vient des règles \
                 de la Realtime Database, pas du secret de cette clé (cf. SPRINT_MMORPG.md).",
            );

            ui.add_space(12.0);
            ui.separator();
            ui.heading("Audio");
            ui.label("Musique / ambiance");
            if ui
                .add(egui::Slider::new(&mut settings.music_volume, 0.0..=1.0))
                .drag_stopped()
            {
                settings.save();
                actions.music_volume = Some(settings.music_volume);
            }
            ui.label("Effets sonores");
            if ui
                .add(egui::Slider::new(&mut settings.sfx_volume, 0.0..=1.0))
                .drag_stopped()
            {
                settings.save();
                actions.sfx_volume = Some(settings.sfx_volume);
            }

            ui.add_space(12.0);
            ui.separator();
            ui.heading("🌐 Langue (jeu)");
            ui.small("Texte affiché en Play (HUD) — pas l'éditeur, qui reste en français.");
            ui.horizontal(|ui| {
                use crate::app::locale::Locale;
                let mut changed = false;
                changed |= ui
                    .selectable_value(&mut settings.locale, Locale::Fr, "Français")
                    .changed();
                changed |= ui
                    .selectable_value(&mut settings.locale, Locale::En, "English")
                    .changed();
                if changed {
                    settings.save();
                    actions.locale = Some(settings.locale);
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.heading("🎮 Manette");
            ui.small(
                "Stick gauche : déplacement « tank » (même axes que A/D/W/S). \
                 Boutons ci-dessous, remappables sur toute manette branchée \
                 (Xbox/PlayStation/Switch Pro — noms génériques par position).",
            );
            let mut changed = false;
            changed |= gamepad_binding_row(ui, "Saut", &mut settings.gamepad.jump);
            changed |= gamepad_binding_row(ui, "Attaque", &mut settings.gamepad.attack);
            changed |= gamepad_binding_row(ui, "Tir", &mut settings.gamepad.fire);
            changed |= gamepad_binding_row(ui, "Soin", &mut settings.gamepad.heal);
            if changed {
                settings.save();
            }
        });
    panels.settings = open;
}

/// Une ligne de remapping manette : libellé d'action + menu déroulant des boutons
/// assignables (`app::input::GAMEPAD_BUTTON_NAMES`). Renvoie `true` si la valeur a
/// changé ce frame (l'appelant décide alors de persister).
fn gamepad_binding_row(ui: &mut egui::Ui, action_label: &str, bound: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(action_label);
        egui::ComboBox::from_id_salt(("gamepad_binding", action_label))
            .selected_text(bound.as_str())
            .show_ui(ui, |ui| {
                for name in crate::app::input::GAMEPAD_BUTTON_NAMES {
                    if ui.selectable_label(bound == name, *name).clicked() && bound != name {
                        *bound = (*name).to_string();
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// Overlay Multijoueur minimal pour le mode Player (mobile/APK) : adresse +
/// pseudo + connecter/déconnecter, replié par défaut pour ne pas gêner le
/// joystick. Pas de compte Firebase/chat/classement ici (cf.
/// `multiplayer_window`, l'équivalent complet côté éditeur desktop).
pub(super) fn mobile_multiplayer_overlay(
    ctx: &egui::Context,
    server_url: &mut String,
    name: &mut String,
    net_status: &str,
    net_connected: bool,
    actions: &mut UiActions,
) {
    egui::Window::new("🌐")
        .id(egui::Id::new("mobile_multiplayer"))
        .collapsible(true)
        .default_open(false)
        .resizable(false)
        // Décalage vertical généreux (pas seulement 8 px) : en plein écran immersif
        // (NativeActivity Android), la zone de rendu passe sous la barre de statut
        // système — un petit décalage laisserait l'icône 🌐 cachée dessous, invisible
        // et donc impossible à toucher.
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 56.0))
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.label("Adresse du serveur");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(server_url).hint_text("ws://192.168.1.x:7777"),
            );
            ui.label("Pseudo");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(name).hint_text("Joueur"),
            );
            ui.add_space(4.0);
            if net_connected {
                if ui.button("🔌 Se déconnecter").clicked() {
                    actions.disconnect_from_server = true;
                }
            } else {
                let can_connect = !server_url.trim().is_empty() && !name.trim().is_empty();
                if ui
                    .add_enabled(can_connect, egui::Button::new("▶ Se connecter"))
                    .clicked()
                {
                    actions.connect_to_server = Some((server_url.clone(), name.clone()));
                }
            }
            ui.add_space(4.0);
            ui.small(if net_status.is_empty() {
                "Non connecté"
            } else {
                net_status
            });
        });
}

/// Fenêtre « Multijoueur » : adresse du serveur + pseudo, connexion/déconnexion
/// (SPRINT_MMORPG.md). Le joueur local reste piloté comme en solo ; les autres
/// joueurs connectés apparaissent comme des objets fantômes une fois reçus par
/// `Snapshot` (cf. `app::network_client`).
#[allow(clippy::too_many_arguments)]
pub(super) fn multiplayer_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    server_url: &mut String,
    name: &mut String,
    email: &mut String,
    password: &mut String,
    lobby_code: &mut String,
    chat_input: &mut String,
    settings: &crate::app::settings::Settings,
    net_status: &str,
    net_connected: bool,
    chat_messages: &[crate::app::network_client::ChatLine],
    has_firebase_account: bool,
    leaderboard: &[crate::app::network_client::LeaderboardLine],
    actions: &mut UiActions,
) {
    let mut open = panels.multiplayer;
    egui::Window::new("🌐  Multijoueur")
        .open(&mut open)
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.label("Adresse du serveur");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(server_url).hint_text("ws://127.0.0.1:7777"),
            );
            ui.label("Pseudo");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(name).hint_text("Joueur"),
            );
            ui.add_space(6.0);
            if net_connected {
                if ui.button("🔌  Se déconnecter").clicked() {
                    actions.disconnect_from_server = true;
                }
            } else {
                let can_connect = !server_url.trim().is_empty() && !name.trim().is_empty();
                if ui
                    .add_enabled(can_connect, egui::Button::new("▶  Se connecter"))
                    .clicked()
                {
                    actions.connect_to_server = Some((server_url.clone(), name.clone()));
                }
                if !can_connect {
                    ui.small("Adresse et pseudo requis.");
                }
            }
            ui.add_space(6.0);
            ui.label(if net_status.is_empty() {
                "Non connecté"
            } else {
                net_status
            });
            ui.add_space(6.0);
            ui.small(
                "Lance d'abord un serveur (`cargo run --bin server`), puis connecte-toi \
                 depuis chaque instance de l'éditeur/du player avec la même adresse.",
            );

            ui.add_space(12.0);
            ui.separator();
            ui.heading("Compte (optionnel)");
            let firebase_configured = !settings.firebase_api_key.trim().is_empty()
                && !settings.firebase_database_url.trim().is_empty();
            if !firebase_configured {
                ui.small(
                    "Configure d'abord une clé API et une URL Database dans \
                     ⚙ Paramètres pour activer les comptes (progression persistante).",
                );
            } else {
                ui.label("Email");
                ui.add(egui::TextEdit::singleline(email).hint_text("toi@example.com"));
                ui.label("Mot de passe");
                ui.add(egui::TextEdit::singleline(password).password(true));
                let can_auth = !email.trim().is_empty() && !password.trim().is_empty();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_auth, egui::Button::new("Se connecter (compte)"))
                        .clicked()
                    {
                        actions.firebase_sign_in = Some((email.clone(), password.clone()));
                    }
                    if ui
                        .add_enabled(can_auth, egui::Button::new("Créer un compte"))
                        .clicked()
                    {
                        actions.firebase_sign_up = Some((email.clone(), password.clone()));
                    }
                });
                ui.small(
                    "Se connecter avant de rejoindre un salon relie ta progression \
                     (XP, classement) à ce compte, cf. SPRINT_MMORPG.md.",
                );
            }

            if firebase_configured {
                ui.add_space(12.0);
                ui.separator();
                ui.heading("Chat");
                ui.label("Salon");
                ui.add(egui::TextEdit::singleline(lobby_code).hint_text("default"));
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        if chat_messages.is_empty() {
                            ui.small("Aucun message pour l'instant.");
                        }
                        for line in chat_messages {
                            ui.label(format!("{} : {}", line.sender, line.text));
                        }
                    });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(chat_input)
                            .hint_text("Message…")
                            .desired_width(180.0),
                    );
                    let can_send = has_firebase_account
                        && !chat_input.trim().is_empty()
                        && !lobby_code.trim().is_empty();
                    if ui
                        .add_enabled(can_send, egui::Button::new("Envoyer"))
                        .clicked()
                    {
                        actions.send_chat_message =
                            Some((lobby_code.clone(), name.clone(), chat_input.clone()));
                        chat_input.clear();
                    }
                });
                if !has_firebase_account {
                    ui.small("Connecte-toi d'abord à un compte pour envoyer des messages.");
                }
                if ui.button("🔄  Rafraîchir").clicked() && !lobby_code.trim().is_empty() {
                    actions.refresh_chat = Some(lobby_code.clone());
                }

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Classement");
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        if leaderboard.is_empty() {
                            ui.small("Aucun score pour l'instant.");
                        }
                        for (rank, entry) in leaderboard.iter().enumerate() {
                            ui.label(format!("{}. {} — {}", rank + 1, entry.name, entry.score));
                        }
                    });
                if ui.button("🔄  Rafraîchir le classement").clicked() {
                    actions.refresh_leaderboard = true;
                }
            }
        });
    panels.multiplayer = open;
}

/// Fenêtre « Générer une scène (IA) » : consigne → scène (remplacer ou ajouter) via DeepSeek.
#[allow(clippy::too_many_arguments)]
pub(super) fn ai_scene_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    settings: &crate::app::settings::Settings,
    prompt: &mut String,
    replace: &mut bool,
    history: &mut Vec<String>,
    status: &StatusInfo,
    actions: &mut UiActions,
) {
    let mut open = panels.ai_scene;
    egui::Window::new("✨  Générer une scène (IA)")
        .open(&mut open)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.label("Décris la scène à générer :");
            ui.add(
                egui::TextEdit::multiline(prompt)
                    .desired_rows(3)
                    .desired_width(340.0)
                    .hint_text(
                        "ex : « un sol, un personnage capsule piloté au joystick, 3 cubes \
                         tactiles colorés et une caméra qui suit »",
                    ),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(replace, true, "Remplacer");
                ui.selectable_value(replace, false, "Ajouter à la scène");
            });
            let has_key = !settings.deepseek_api_key.trim().is_empty();
            let can = has_key && !status.ai_busy && !prompt.trim().is_empty();
            ui.horizontal(|ui| {
                let label = if *replace {
                    "✨ Générer (remplace)"
                } else {
                    "✨ Générer (ajoute)"
                };
                if ui.add_enabled(can, egui::Button::new(label)).clicked() {
                    let p = prompt.trim().to_string();
                    // Historique : consigne en tête, sans doublon, max 8.
                    history.retain(|h| h != &p);
                    history.insert(0, p.clone());
                    history.truncate(8);
                    actions.ai_generate_scene = Some((
                        crate::app::ai::AiRequest {
                            api_key: settings.deepseek_api_key.clone(),
                            model: settings.deepseek_model.clone(),
                            temperature: settings.deepseek_temperature,
                            prompt: p,
                        },
                        *replace,
                    ));
                }
                if status.ai_busy {
                    ui.spinner();
                    ui.label("génération…");
                } else if !has_key {
                    ui.label("clé API requise (⚙ Paramètres)");
                }
            });
            if !history.is_empty() {
                ui.separator();
                ui.label("Consignes récentes :");
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for h in history.iter() {
                            let short: String = h.chars().take(60).collect();
                            if ui.selectable_label(false, short).clicked() {
                                *prompt = h.clone();
                            }
                        }
                    });
            }
        });
    panels.ai_scene = open;
}

/// Fenêtre « Optimisation mobile » : actions concrètes pour alléger la scène.
pub(super) fn optimize_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    actions: &mut UiActions,
) {
    let mut open = panels.optimize;
    egui::Window::new("🪶  Optimisation mobile")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let n_tex = scene
                .objects
                .iter()
                .filter(|o| !o.texture.is_empty())
                .count();
            ui.label(format!(
                "{n_tex} objet(s) texturé(s), {} lumière(s) ponctuelle(s)",
                scene.point_lights.len()
            ));
            ui.separator();
            ui.label("Réduire les textures (côté le plus long) :");
            ui.horizontal(|ui| {
                for max in [1024u32, 2048, 4096] {
                    if ui.button(format!("≤ {max} px")).clicked() {
                        actions.optimize_textures = Some(max);
                    }
                }
            });
            ui.small("Écrit des copies …_optN.png et met à jour les objets (annulable).");
            if ui
                .button("🧱 Convertir en puissances de 2")
                .on_hover_text("Redimensionne les textures en POT (mip-mapping/compression GPU)")
                .clicked()
            {
                actions.convert_textures_pot = true;
            }
            ui.separator();
            if scene.point_lights.len() > 4 && ui.button("Limiter à 4 lumières").clicked() {
                actions.limit_lights = Some(4);
            }
            if !scene.point_lights.is_empty()
                && ui
                    .button("💡 Bake lighting (figer les lumières)")
                    .on_hover_text(
                        "Fige les lumières ponctuelles en émission statique puis les supprime",
                    )
                    .clicked()
            {
                actions.bake_lighting = true;
            }
            ui.separator();
            // Évolutions de rendu non encore implémentées : grisées et explicitées.
            ui.add_enabled(
                false,
                egui::Button::new("🔻 Fusionner les meshes statiques"),
            )
            .on_hover_text("À venir : fusion des géométries statiques (réduction des draw calls)");
            ui.add_enabled(
                false,
                egui::Button::new("📉 Activer LOD / occlusion culling"),
            )
            .on_hover_text(
                "À venir : niveaux de détail et culling d'occlusion (sous-systèmes de rendu)",
            );
            ui.separator();
            if ui
                .button("⚡ Mode performance Android")
                .on_hover_text("Réduit les textures à ≤ 1024 px et limite à 4 lumières en une fois")
                .clicked()
            {
                actions.perf_mode = true;
            }
            ui.separator();
            ui.label("Préset qualité (Sprint 126) :");
            ui.horizontal(|ui| {
                use crate::app::asset_ops::QualityPreset;
                if ui
                    .button("🖥 Desktop")
                    .on_hover_text("Aucune réduction — machine de bureau")
                    .clicked()
                {
                    actions.apply_quality_preset = Some(QualityPreset::Desktop);
                }
                if ui
                    .button("📱 Mobile (léger)")
                    .on_hover_text("Réduction légère : textures ≤ 2048 px seulement")
                    .clicked()
                {
                    actions.apply_quality_preset = Some(QualityPreset::MobileHigh);
                }
                if ui
                    .button("📱 Mobile (agressif)")
                    .on_hover_text(
                        "Réduction complète : textures ≤ 1024 px, 4 lumières max, POT, bake lighting",
                    )
                    .clicked()
                {
                    actions.apply_quality_preset = Some(QualityPreset::MobileLow);
                }
            });
            ui.small("💡 Astuce : utilise « Contrôle qualité APK » pour vérifier les gains.");
        });
    panels.optimize = open;
}

/// Fenêtre « Gestionnaire de scripts Lua » : liste les objets scriptés, donne un
/// aperçu et permet de sélectionner l'objet (édition dans l'inspecteur).
pub(super) fn scripts_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
) {
    let mut open = panels.scripts;
    egui::Window::new("📜  Gestionnaire de scripts Lua")
        .open(&mut open)
        .default_size([420.0, 320.0])
        .show(ctx, |ui| {
            let scripted: Vec<usize> = scene
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.script.trim().is_empty())
                .map(|(i, _)| i)
                .collect();
            let total_lines: usize = scripted
                .iter()
                .map(|&i| scene.objects[i].script.lines().count())
                .sum();
            ui.label(format!(
                "{} script(s), {} ligne(s) au total",
                scripted.len(),
                total_lines
            ));
            ui.separator();
            if scripted.is_empty() {
                ui.weak("Aucun script. Sélectionne un objet et écris du Lua dans l'inspecteur.");
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for i in scripted {
                        let obj = &scene.objects[i];
                        let lines = obj.script.lines().count();
                        let header = format!("📜 {} ({lines} l.)", obj.name);
                        let is_sel = *selection == Some(i);
                        if ui.selectable_label(is_sel, header).clicked() {
                            *selection = Some(i);
                            *selected = vec![i];
                        }
                        // Aperçu : première ligne non vide du script.
                        if let Some(first) = obj.script.lines().find(|l| !l.trim().is_empty()) {
                            ui.indent(("preview", i), |ui| {
                                ui.weak(egui::RichText::new(first.trim()).monospace().small());
                            });
                        }
                    }
                });
        });
    panels.scripts = open;
}

/// Fenêtre « Gestionnaire d'assets » : liste les assets du projet + embarqués,
/// permet de rassembler les fichiers externes et d'assigner une texture à la sélection.
pub(super) fn asset_browser_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &mut Scene,
    selection: Option<usize>,
    actions: &mut UiActions,
) {
    let mut open = panels.assets;
    egui::Window::new("📁  Gestionnaire d'assets")
        .open(&mut open)
        .default_size([360.0, 320.0])
        .show(ctx, |ui| {
            if ui
                .button("📦 Rassembler les assets du projet")
                .on_hover_text(
                    "Copie les fichiers externes dans ~/.motor3derust/assets et utilise asset://",
                )
                .clicked()
            {
                actions.collect_assets = true;
            }
            ui.separator();
            let assets = crate::assets::list_assets();
            if assets.is_empty() {
                ui.label("Aucun asset. Importe une texture/un modèle, puis « Rassembler ».");
            } else {
                let sel_obj = selection.filter(|&i| i < scene.objects.len());
                ui.label(match sel_obj {
                    Some(_) => "Clique un asset image pour l'appliquer à l'objet sélectionné :",
                    None => "Sélectionne un objet pour assigner une texture.",
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for a in assets {
                            let is_img =
                                a.ends_with(".png") || a.ends_with(".jpg") || a.ends_with(".jpeg");
                            let resp = ui.selectable_label(false, &a);
                            if resp.clicked()
                                && is_img
                                && let Some(i) = sel_obj
                            {
                                scene.objects[i].texture = a.clone();
                            }
                        }
                    });
            }
            ui.separator();
            // Sprint 96 (câblage UI) : prefabs, listés séparément de `list_assets`
            // (qui ne descend pas dans `prefabs/`, cf. `assets::list_prefabs`).
            let prefabs = crate::assets::list_prefabs();
            ui.collapsing(format!("🧩 Prefabs ({})", prefabs.len()), |ui| {
                if prefabs.is_empty() {
                    ui.label(
                        "Aucun prefab. Sélectionne un objet puis « 🧊 Créer un prefab \
                         depuis la sélection » dans l'Inspecteur.",
                    );
                } else {
                    for (name, asset_id) in prefabs {
                        ui.horizontal(|ui| {
                            ui.label(&name);
                            if ui.button("➕ Instancier").clicked() {
                                actions.instantiate_prefab = Some(asset_id);
                            }
                        });
                    }
                }
            });
        });
    panels.assets = open;
}

/// La scène a-t-elle un joueur pilotable équipé d'une arme à distance
/// (`Controller::fire_button` non vide) ? Sert à n'afficher le réticule de
/// visée que quand il a un sens — pas dans une démo sans tir à distance.
/// Fenêtre « 👁 Aperçu HUD » : cases à cocher pour prévisualiser en
/// Édition les overlays normalement réservés à Play (réticule, inventaire,
/// joueurs…), sans lancer la simulation — utile pour ajuster leur position ou
/// leur lisibilité. État purement éditeur : rien ici n'est écrit dans la
/// scène (contrairement à `Controller::fire_button`, qui décide de leur
/// affichage réel en jeu).
pub(super) fn hud_preview_window(ctx: &egui::Context, preview: &mut HudPreview) {
    let mut open = preview.open;
    egui::Window::new("👁 Aperçu HUD")
        .open(&mut open)
        // Position fixe (coin stratégique, sous la toolbar) plutôt que
        // déplaçable : c'est un panneau de réglages, pas un élément de la
        // scène de jeu (contrairement aux overlays qu'il pilote, eux bien
        // glissables en 🖐 Repositionner) — inutile de la laisser traîner.
        .movable(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 90.0))
        .resizable(false)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.small("Affiche ces éléments en Édition, comme en Play :");
            ui.checkbox(&mut preview.crosshair, "🎯 Réticule");
            ui.checkbox(&mut preview.weapon_inventory, "🎒 Inventaire d'armes");
            ui.checkbox(&mut preview.weapon_hud, "Libellé de l'arme équipée");
            ui.checkbox(&mut preview.kills, "💀 Frags");
            ui.checkbox(&mut preview.roster, "👥 Joueurs (données d'exemple)");
            ui.add_space(4.0);
            ui.separator();
            ui.checkbox(
                &mut preview.reposition,
                "🖐 Repositionner (glisser les éléments cochés)",
            );
            ui.small(
                "La position de chaque élément est enregistrée dans la scène : elle \
                 s'applique aussi en Play et dans le jeu exporté (APK/player).",
            );
            ui.add_space(4.0);
            ui.small(
                "En jeu, réticule et inventaire ne s'affichent que si le joueur a un \
                 bouton 🔥 Feu configuré (Inspecteur › 🧩 Composants mobiles).",
            );
        });
    preview.open = open;
}

/// Fenêtre « 🧩 Widgets HUD » : ajouter/éditer/supprimer les widgets déclaratifs de
/// `Scene::hud_widgets` (texte, image, jauge, bouton) — cf. Sprint 109. Contrairement
/// à `hud_preview_window` (bascules d'aperçu, état purement éditeur), tout ici est
/// écrit directement dans la scène : persisté, s'applique aussi en Play et dans le
/// jeu exporté.
pub(super) fn hud_widgets_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &mut Scene,
    new_id: &mut String,
) {
    let mut open = panels.hud_widgets_editor;
    egui::Window::new("🧩 Widgets HUD")
        .open(&mut open)
        .default_width(320.0)
        .default_height(420.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(new_id)
                        .hint_text("identifiant (ex. score_label)")
                        .desired_width(ui.available_width() - 90.0),
                );
                if ui.button("➕ Ajouter").clicked() && !new_id.trim().is_empty() {
                    scene.hud_widgets.push(HudWidget {
                        id: new_id.trim().to_string(),
                        anchor: HudAnchor::TopLeft,
                        offset: [10.0, 10.0],
                        size: [0.0, 0.0],
                        kind: HudWidgetKind::default(),
                    });
                    new_id.clear();
                }
            });
            ui.separator();
            let mut remove: Option<usize> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, w) in scene.hud_widgets.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        egui::CollapsingHeader::new(if w.id.is_empty() {
                            format!("(sans nom) #{i}")
                        } else {
                            w.id.clone()
                        })
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("id");
                                ui.text_edit_singleline(&mut w.id);
                            });
                            egui::ComboBox::from_label("ancre")
                                .selected_text(format!("{:?}", w.anchor))
                                .show_ui(ui, |ui| {
                                    for a in [
                                        HudAnchor::TopLeft,
                                        HudAnchor::TopRight,
                                        HudAnchor::BottomLeft,
                                        HudAnchor::BottomRight,
                                        HudAnchor::Center,
                                    ] {
                                        ui.selectable_value(&mut w.anchor, a, format!("{a:?}"));
                                    }
                                });
                            ui.horizontal(|ui| {
                                ui.label("décalage");
                                ui.add(egui::DragValue::new(&mut w.offset[0]).prefix("x "));
                                ui.add(egui::DragValue::new(&mut w.offset[1]).prefix("y "));
                            });
                            ui.horizontal(|ui| {
                                ui.label("taille (0 = auto)");
                                ui.add(egui::DragValue::new(&mut w.size[0]).prefix("l "));
                                ui.add(egui::DragValue::new(&mut w.size[1]).prefix("h "));
                            });
                            ui.separator();
                            let kind_label = match &w.kind {
                                HudWidgetKind::Text { .. } => "Texte",
                                HudWidgetKind::Image { .. } => "Image",
                                HudWidgetKind::Gauge { .. } => "Jauge",
                                HudWidgetKind::Button { .. } => "Bouton",
                            };
                            egui::ComboBox::from_label("nature")
                                .selected_text(kind_label)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(kind_label == "Texte", "Texte")
                                        .clicked()
                                    {
                                        w.kind = HudWidgetKind::Text {
                                            content: String::new(),
                                            binding: HudBinding::None,
                                        };
                                    }
                                    if ui
                                        .selectable_label(kind_label == "Image", "Image")
                                        .clicked()
                                    {
                                        w.kind = HudWidgetKind::Image {
                                            path: String::new(),
                                        };
                                    }
                                    if ui
                                        .selectable_label(kind_label == "Jauge", "Jauge")
                                        .clicked()
                                    {
                                        w.kind = HudWidgetKind::Gauge {
                                            binding: HudBinding::Health,
                                            max: 1.0,
                                            color: [0.8, 0.15, 0.15],
                                        };
                                    }
                                    if ui
                                        .selectable_label(kind_label == "Bouton", "Bouton")
                                        .clicked()
                                    {
                                        w.kind = HudWidgetKind::Button {
                                            label: String::new(),
                                            action: String::new(),
                                        };
                                    }
                                });
                            match &mut w.kind {
                                HudWidgetKind::Text { content, binding } => {
                                    ui.horizontal(|ui| {
                                        ui.label("contenu");
                                        ui.text_edit_singleline(content);
                                    });
                                    binding_combo(ui, binding);
                                }
                                HudWidgetKind::Image { path } => {
                                    ui.horizontal(|ui| {
                                        ui.label("chemin");
                                        ui.text_edit_singleline(path);
                                    });
                                }
                                HudWidgetKind::Gauge {
                                    binding,
                                    max,
                                    color,
                                } => {
                                    binding_combo(ui, binding);
                                    ui.horizontal(|ui| {
                                        ui.label("max");
                                        ui.add(egui::DragValue::new(max).speed(0.1));
                                    });
                                    ui.color_edit_button_rgb(color);
                                }
                                HudWidgetKind::Button { label, action } => {
                                    ui.horizontal(|ui| {
                                        ui.label("libellé");
                                        ui.text_edit_singleline(label);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("action");
                                        ui.text_edit_singleline(action);
                                    })
                                    .response
                                    .on_hover_text("Lu côté script via on_event(\"hud:<action>\")");
                                }
                            }
                            if ui.button("🗑 Supprimer ce widget").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
            });
            if scene.hud_widgets.is_empty() {
                ui.small(
                    "Aucun widget. Un widget « Bouton » émet l'événement de gameplay \
                     `hud:<action>` (lisible en Lua via on_event) quand il est cliqué.",
                );
            }
            if let Some(i) = remove {
                scene.hud_widgets.remove(i);
            }
        });
    panels.hud_widgets_editor = open;
}

/// Fenêtre « 🩹 Journal de crash » (Sprint 113) : consultation **volontaire** d'une
/// trace de panic capturée par `crash_log::install` — aucun envoi automatique nulle
/// part, juste voir/copier le texte, et le supprimer une fois consulté. S'ouvre
/// automatiquement au lancement s'il y a quelque chose à montrer (cf. `Editor::new`),
/// sinon accessible depuis le menu Aide.
pub(super) fn crash_log_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    crash_log_text: &mut Option<String>,
) {
    let mut open = panels.crash_log;
    let mut clear = false;
    egui::Window::new("🩹 Journal de crash")
        .open(&mut open)
        .default_size([480.0, 360.0])
        .show(ctx, |ui| match crash_log_text {
            Some(text) => {
                ui.label(
                    "RusteeGear a planté lors d'une session précédente. Rien n'est \
                     envoyé automatiquement : copiez ce texte pour le joindre à un \
                     rapport de bug si vous le souhaitez, ou fermez pour l'oublier.",
                );
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(text)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace),
                        );
                    });
                ui.horizontal(|ui| {
                    if ui.button("📋 Copier").clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    if ui.button("🗑 Fermer et supprimer").clicked() {
                        clear = true;
                    }
                });
            }
            None => {
                ui.label("Aucun crash enregistré depuis la dernière suppression.");
            }
        });
    if clear {
        crate::crash_log::clear();
        *crash_log_text = None;
        open = false;
    }
    panels.crash_log = open;
}

/// Fenêtre « Nouveau projet » guidée (Sprint 113d) : au lieu de partir directement
/// d'une scène nue, propose un choix de template — la marche d'entrée d'un
/// utilisateur qui ne code pas. Chaque carte déclenche **exactement** l'action déjà
/// câblée par l'entrée de menu équivalente dans `menus::menu_fichier`
/// (`actions.new_scene`/`load_controller`/`load_ai_duel`, consommées dans
/// `gfx::renderer::render`) : aucune logique de chargement de scène n'est
/// réimplémentée ici, juste une présentation guidée en avant-plan.
pub(super) fn new_project_wizard_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    actions: &mut UiActions,
) {
    let mut open = panels.new_project_wizard;
    let mut close_after = false;
    egui::Window::new("✨  Nouveau projet")
        .open(&mut open)
        .resizable(false)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.label("Comment démarrer ?");
            ui.add_space(8.0);
            if ui
                .add_sized([320.0, 36.0], egui::Button::new("📄  Scène vide"))
                .on_hover_text(
                    "Repart de zéro, sans aucun objet — pour construire son propre niveau.",
                )
                .clicked()
            {
                actions.new_scene = true;
                close_after = true;
            }
            if ui
                .add_sized([320.0, 36.0], egui::Button::new("🕹  Démo contrôleur"))
                .on_hover_text(
                    "Joueur pilotable au joystick, saut sur bouton, collisions avec le décor — \
                     un bon point de départ pour explorer les contrôles sans écrire de script.",
                )
                .clicked()
            {
                actions.load_controller = true;
                close_after = true;
            }
            if ui
                .add_sized([320.0, 36.0], egui::Button::new("⚔  Niveau de combat"))
                .on_hover_text(
                    "Manches de monstres qui poursuivent le joueur (style Call of Zombies) — \
                     pour explorer combat/vagues/vie sans repartir de zéro.",
                )
                .clicked()
            {
                actions.load_ai_duel = true;
                close_after = true;
            }
            ui.add_space(4.0);
            ui.small(
                "D'autres démos (donjon, duel, course, MMORPG…) restent disponibles dans \
                 le menu Fichier.",
            );
        });
    if close_after {
        open = false;
    }
    panels.new_project_wizard = open;
}

/// Fenêtre « Ajouter un objet » simplifiée (Sprint 113d) : cartes avec icône pour
/// les actions les plus courantes du menu Ajouter (déjà riche depuis les Sprints
/// 40-41, cf. `menus::menu_ajouter`), en avant-plan plutôt que dans un sous-menu —
/// même mécanisme d'ajout (`actions.add`, `AppState::add_object`), pas de logique
/// dupliquée. Reste ouverte après un clic (contrairement à l'assistant « Nouveau
/// projet ») : ajouter plusieurs objets à la suite est le cas d'usage normal ici.
pub(super) fn add_object_cards_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &mut Scene,
    actions: &mut UiActions,
) {
    let mut open = panels.add_object_cards;
    egui::Window::new("🃏  Ajouter un objet")
        .open(&mut open)
        .resizable(false)
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.label("Objets 3D :");
            egui::Grid::new("add_object_cards_grid")
                .num_columns(3)
                .spacing([6.0, 6.0])
                .show(ui, |ui| {
                    for (i, (kind, icon, label)) in [
                        (MeshKind::Cube, "🧊", "Cube"),
                        (MeshKind::Sphere, "⚪", "Sphère"),
                        (MeshKind::Plane, "▦", "Plan"),
                        (MeshKind::Cylinder, "🛢", "Cylindre"),
                        (MeshKind::Capsule, "💊", "Capsule"),
                        (MeshKind::Terrain, "⛰", "Terrain"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if ui
                            .add_sized([76.0, 56.0], egui::Button::new(format!("{icon}\n{label}")))
                            .clicked()
                        {
                            actions.add = Some(kind);
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            ui.separator();
            ui.label("Lumière :");
            let can_add_light = scene.point_lights.len() < MAX_POINT_LIGHTS;
            if ui
                .add_enabled(
                    can_add_light,
                    egui::Button::new("💡  Ponctuelle").min_size([76.0, 32.0].into()),
                )
                .on_hover_text(if can_add_light {
                    "Éclaire dans toutes les directions depuis un point (comme une ampoule)."
                } else {
                    "Nombre maximal de lumières ponctuelles déjà atteint."
                })
                .clicked()
            {
                scene.point_lights.push(PointLight::default());
            }
        });
    panels.add_object_cards = open;
}

fn binding_combo(ui: &mut egui::Ui, binding: &mut HudBinding) {
    egui::ComboBox::from_label("liaison")
        .selected_text(format!("{binding:?}"))
        .show_ui(ui, |ui| {
            for b in [
                HudBinding::None,
                HudBinding::Health,
                HudBinding::Score,
                HudBinding::Kills,
                HudBinding::Wave,
            ] {
                ui.selectable_value(binding, b, format!("{b:?}"));
            }
        });
}
