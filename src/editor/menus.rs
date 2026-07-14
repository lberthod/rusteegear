//! Barre de menus de l'éditeur (Fichier/Édition/Ajouter/Outils/Aide) : nouveau/
//! ouvrir/enregistrer/exporter, undo/redo/copier-coller, ajout de primitives et de
//! démos, réglages rapides (gizmo, physique). Extrait de `editor/mod.rs`.

use crate::app::GizmoMode;
use crate::scene::{MeshKind, Scene};

use super::{Panels, UiActions, export};

/// Menu « Fichier » : sauvegarde, ouverture, import, export.
pub(super) fn menu_fichier(
    ui: &mut egui::Ui,
    export: &mut export::ExportPanel,
    actions: &mut UiActions,
) {
    ui.menu_button("Fichier", |ui| {
        if ui.button("✨  Nouveau projet").clicked() {
            actions.new_scene = true;
            ui.close();
        }
        if ui
            .button("🎮  Démo mobile (jouable)")
            .on_hover_text("Charge une scène : joystick + bouton Saut + personnage scripté")
            .clicked()
        {
            actions.load_demo = true;
            ui.close();
        }
        if ui
            .button("🎯  Démo gameplay (complète)")
            .on_hover_text("Joystick + gyroscope + saut + zone de danger + barre de vie + tap")
            .clicked()
        {
            actions.load_gameplay = true;
            ui.close();
        }
        if ui
            .button("🕹  Démo contrôleur (joystick + saut, sans script)")
            .on_hover_text(
                "Joueur pilotable au joystick, saut sur bouton, collisions avec le décor",
            )
            .clicked()
        {
            actions.load_controller = true;
            ui.close();
        }
        if ui
            .button("🗼  Démo Tour d'ascension (platforming)")
            .on_hover_text(
                "Style différent : grimpe la tour en spirale, aucune arme ni combat, éviter le vide",
            )
            .clicked()
        {
            actions.load_tower = true;
            ui.close();
        }
        if ui
            .button("🏃  Démo Course infinie (style Temple Run)")
            .on_hover_text(
                "Course automatique + changement de voie + saut : esquive les obstacles, ramasse les pièces",
            )
            .clicked()
        {
            actions.load_temple_run = true;
            ui.close();
        }
        if ui
            .button("🧩  Scène exemple (composants Controller/Audio/Combat)")
            .on_hover_text(
                "Référence minimale : un objet par composant optionnel, pas un niveau de jeu",
            )
            .clicked()
        {
            actions.load_components_demo = true;
            ui.close();
        }
        if ui
            .button("🧟  Démo Vagues de zombies (local, sans réseau)")
            .on_hover_text(
                "Manches de monstres (Rôdeur/Coureur/Brute) qui poursuivent le joueur, style Call of Zombies",
            )
            .clicked()
        {
            actions.load_ai_duel = true;
            ui.close();
        }
        if ui
            .button("🌐  Démo MMORPG (test multijoueur PC ↔ mobile)")
            .on_hover_text(
                "Arène minimale sans monstres/manches : joueur pilotable (joystick + saut), \
                 pensée pour voir un client desktop et un APK se déplacer l'un par rapport à l'autre",
            )
            .clicked()
        {
            actions.load_mmorpg = true;
            ui.close();
        }
        if ui
            .button("🗡  Démo Donjon (roguelike, 3 salles)")
            .on_hover_text(
                "3 salles à vider une à une (porte fermée jusqu'à la précédente vidée), arme de départ tirée au sort",
            )
            .clicked()
        {
            actions.load_roguelike = true;
            ui.close();
        }
        if ui
            .button("🥊  Démo Duel (façon Tekken/Smash Bros)")
            .on_hover_text(
                "Arène flottante, un rival à plusieurs coups avant de tomber, ring out possible (le vide sous l'arène est mortel)",
            )
            .clicked()
        {
            actions.load_brawl = true;
            ui.close();
        }
        if ui
            .button("✨  Générer une scène (IA)…")
            .on_hover_text("Crée une scène complète depuis une description (DeepSeek)")
            .clicked()
        {
            actions.open_ai_scene = true;
            ui.close();
        }
        ui.separator();
        if ui.button("💾  Enregistrer").clicked() {
            actions.save = true;
            ui.close();
        }
        if ui.button("💾  Enregistrer sous…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Scène JSON", &["json"])
                .set_file_name("scene.json")
                .save_file()
            {
                actions.save_path = Some(p.to_string_lossy().into_owned());
            }
            ui.close();
        }
        if ui.button("📂  Ouvrir…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Scène JSON", &["json"])
                .pick_file()
            {
                actions.load_path = Some(p.to_string_lossy().into_owned());
            }
            #[cfg(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))]
            {
                actions.load = true;
            }
            ui.close();
        }
        ui.separator();
        if ui.button("📥  Importer glTF…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("glTF", &["glb", "gltf"])
                .pick_file()
            {
                actions.import = Some(p.to_string_lossy().into_owned());
            }
            ui.close();
        }
        ui.separator();
        if ui.button("📦  Build & Export…").clicked() {
            export.open = true;
            ui.close();
        }
        if ui
            .button("⚙  Paramètres projet…")
            .on_hover_text("Nom, package, version, build, orientation (panneau de build)")
            .clicked()
        {
            export.open = true;
            actions.open_settings = true;
            ui.close();
        }
        ui.separator();
        if ui.button("🚪  Quitter").clicked() {
            actions.quit = true;
            ui.close();
        }
    });
}

/// Menu « Édition » : historique et opérations sur la sélection.
pub(super) fn menu_edition(ui: &mut egui::Ui, selection: &Option<usize>, actions: &mut UiActions) {
    ui.menu_button("Édition", |ui| {
        if ui.button("↩  Annuler").clicked() {
            actions.undo = true;
            ui.close();
        }
        if ui.button("↪  Rétablir").clicked() {
            actions.redo = true;
            ui.close();
        }
        ui.separator();
        let has = selection.is_some();
        if ui
            .add_enabled(has, egui::Button::new("✂  Couper"))
            .clicked()
        {
            actions.cut = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("⧉  Copier"))
            .clicked()
        {
            actions.copy = true;
            ui.close();
        }
        if ui.button("📋  Coller").clicked() {
            actions.paste = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("⧉  Dupliquer"))
            .clicked()
        {
            actions.duplicate = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗑  Supprimer"))
            .clicked()
        {
            actions.delete = *selection;
            ui.close();
        }
        ui.separator();
        if ui.button("☑  Tout sélectionner").clicked() {
            actions.select_all = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗂  Grouper"))
            .clicked()
        {
            actions.group = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗂  Dégrouper"))
            .clicked()
        {
            actions.ungroup = true;
            ui.close();
        }
        ui.menu_button("📐  Aligner sur…", |ui| {
            ui.label("Aligne la sélection sur l'objet primaire");
            for (axis, label) in [(0, "Axe X"), (1, "Axe Y"), (2, "Axe Z")] {
                if ui.button(label).clicked() {
                    actions.align_axis = Some(axis);
                    ui.close();
                }
            }
        });
        ui.menu_button("📏  Distribuer sur…", |ui| {
            ui.label("Espace régulièrement la sélection (≥ 3 objets)");
            for (axis, label) in [(0, "Axe X"), (1, "Axe Y"), (2, "Axe Z")] {
                if ui.button(label).clicked() {
                    actions.distribute_axis = Some(axis);
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui
            .add_enabled(has, egui::Button::new("⬇  Aligner au sol"))
            .on_hover_text("Pose la base de l'objet sur le plan (y = 0)")
            .clicked()
        {
            actions.align_ground = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("↺  Réinitialiser transform"))
            .on_hover_text("Rotation et échelle par défaut (position conservée)")
            .clicked()
        {
            actions.reset_transform = true;
            ui.close();
        }
    });
}

/// Menu « Ajouter » façon Unity : objets 3D, lumières, caméras, physique, audio, UI mobile.
pub(super) fn menu_ajouter(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    selection: Option<usize>,
    actions: &mut UiActions,
) {
    use crate::scene::{MAX_POINT_LIGHTS, PointLight};
    ui.menu_button("Ajouter", |ui| {
        // --- Objet 3D ---
        ui.menu_button("🧱  Objet 3D", |ui| {
            for (kind, label) in [
                (MeshKind::Cube, "🧊  Cube"),
                (MeshKind::Sphere, "⚪  Sphère"),
                (MeshKind::Plane, "▦  Plan"),
                (MeshKind::Cylinder, "🛢  Cylindre"),
                (MeshKind::Capsule, "💊  Capsule"),
                (MeshKind::Terrain, "⛰  Terrain"),
            ] {
                if ui.button(label).clicked() {
                    actions.add = Some(kind);
                    ui.close();
                }
            }
        });

        // --- Lumières ---
        ui.menu_button("💡  Lumière", |ui| {
            let can_add = scene.point_lights.len() < MAX_POINT_LIGHTS;
            if ui
                .add_enabled(can_add, egui::Button::new("💡  Ponctuelle (point)"))
                .clicked()
            {
                scene.point_lights.push(PointLight::default());
                ui.close();
            }
            if ui
                .add_enabled(can_add, egui::Button::new("🔦  Spot (cône)"))
                .clicked()
            {
                scene.point_lights.push(PointLight {
                    spot_angle: 30.0,
                    ..PointLight::default()
                });
                ui.close();
            }
            ui.separator();
            if ui
                .button("☀  Directionnelle (réinitialiser)")
                .on_hover_text(
                    "Lumière directionnelle de la scène (une seule) — valeurs par défaut",
                )
                .clicked()
            {
                scene.light.dir = [0.5, 1.0, 0.3];
                scene.light.color = [1.0, 1.0, 1.0];
                ui.close();
            }
            if ui
                .button("🌙  Ambiante +0,1")
                .on_hover_text("Augmente la lumière ambiante de la scène")
                .clicked()
            {
                scene.light.ambient = (scene.light.ambient + 0.1).min(1.0);
                ui.close();
            }
        });

        // --- Caméras ---
        ui.menu_button("🎥  Caméra", |ui| {
            if ui
                .button("🎥  Principale (vue actuelle)")
                .on_hover_text("Fige le point de vue de jeu sur la caméra actuelle (Play)")
                .clicked()
            {
                actions.set_game_camera = true;
                ui.close();
            }
            if ui
                .selectable_label(scene.camera_follow, "🎯  Caméra de suivi (mobile)")
                .on_hover_text("En Play, la caméra suit l'objet scripté (joueur)")
                .clicked()
            {
                scene.camera_follow = !scene.camera_follow;
                ui.close();
            }
        });

        ui.separator();

        // --- Physique (s'applique à la sélection) ---
        let sel = selection.filter(|&i| i < scene.objects.len());
        ui.add_enabled_ui(sel.is_some(), |ui| {
            ui.menu_button("⚙  Physique (sélection)", |ui| {
                use crate::runtime::physics::PhysicsKind as P;
                if let Some(i) = sel {
                    if ui.button("🧱  Corps statique").clicked() {
                        scene.objects[i].physics = P::Static;
                        ui.close();
                    }
                    if ui.button("⚙  Rigidbody (dynamique)").clicked() {
                        scene.objects[i].physics = P::Dynamic;
                        ui.close();
                    }
                    if ui.button("🎯  Zone de déclenchement (trigger)").clicked() {
                        scene.objects[i].trigger = true;
                        ui.close();
                    }
                    if ui.button("✕  Aucune physique").clicked() {
                        scene.objects[i].physics = P::None;
                        ui.close();
                    }
                }
            });
        });

        // --- Audio (s'applique à la sélection) ---
        ui.add_enabled_ui(sel.is_some(), |ui| {
            if ui
                .button("🔊  Source audio (sélection)…")
                .on_hover_text("Choisit un son joué par l'objet sélectionné")
                .clicked()
            {
                #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                if let Some(i) = sel
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Audio", &["wav", "ogg", "flac", "mp3"])
                        .pick_file()
                {
                    scene.objects[i]
                        .audio
                        .get_or_insert_with(crate::scene::AudioSource::default)
                        .clip = p.to_string_lossy().into_owned();
                }
                ui.close();
            }
        });

        ui.separator();

        // --- UI mobile ---
        ui.menu_button("📱  UI mobile", |ui| {
            let m = &mut scene.mobile;
            if ui
                .selectable_label(m.joystick, "🕹  Joystick virtuel")
                .clicked()
            {
                m.joystick = !m.joystick;
                // Mutuellement exclusif avec la croix directionnelle : les deux se
                // dessinent dans le même coin de l'écran, jamais les deux à la fois.
                if m.joystick {
                    m.dpad = false;
                }
                ui.close();
            }
            if ui
                .selectable_label(m.dpad, "🎮  Pavé W/A/S/D (contrôles tank)")
                .on_hover_text(
                    "Mêmes contrôles que le clavier : W/S avance/recule le long de \
                     l'orientation du personnage, A/D le fait pivoter",
                )
                .clicked()
            {
                m.dpad = !m.dpad;
                if m.dpad {
                    m.joystick = false;
                }
                ui.close();
            }
            if ui.button("🔘  Bouton tactile").clicked() {
                let n = m.buttons.len() + 1;
                m.buttons.push(format!("B{n}"));
                ui.close();
            }
            if !m.buttons.is_empty() && ui.button("✕  Retirer le dernier bouton").clicked() {
                m.buttons.pop();
                ui.close();
            }
            if ui
                .selectable_label(m.touch_zone, "👆  Zone tactile (plein écran)")
                .on_hover_text("Un tap n'importe où expose input.btn.touch au script")
                .clicked()
            {
                m.touch_zone = !m.touch_zone;
                ui.close();
            }
            if ui
                .selectable_label(m.health_bar, "❤  Barre de vie (HUD)")
                .on_hover_text("Affiche une barre de vie pilotée par set_health() côté script")
                .clicked()
            {
                m.health_bar = !m.health_bar;
                ui.close();
            }
            if ui
                .selectable_label(m.safe_area, "🛡  Zone sûre (safe area)")
                .on_hover_text("Rentre les contrôles dans une marge sûre (encoche, bords arrondis)")
                .clicked()
            {
                m.safe_area = !m.safe_area;
                ui.close();
            }
        });
    });
}

/// Menu « Outils » : mode de manipulation du gizmo + diagnostics.
pub(super) fn menu_outils(
    ui: &mut egui::Ui,
    gizmo_mode: &mut GizmoMode,
    export: &mut export::ExportPanel,
    panels: &mut Panels,
) {
    ui.menu_button("Outils", |ui| {
        ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔  Déplacer (W)");
        ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻  Tourner (E)");
        ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⤢  Redimensionner (R)");
        ui.separator();
        if ui.button("🖥  Console").clicked() {
            panels.console = true;
            ui.close();
        }
        if ui.button("📊  Profiler FPS").clicked() {
            panels.profiler = true;
            ui.close();
        }
        if ui.button("📜  Gestionnaire de scripts Lua").clicked() {
            panels.scripts = true;
            ui.close();
        }
        ui.separator();
        if ui.button("🤖  Build Android…").clicked() {
            export.open = true;
            ui.close();
        }
        if ui.button("📁  Gestionnaire d'assets").clicked() {
            panels.assets = true;
            ui.close();
        }
        if ui.button("🪶  Optimisation mobile").clicked() {
            panels.optimize = true;
            ui.close();
        }
        if ui.button("✔  Contrôle qualité APK").clicked() {
            panels.readiness = true;
            panels.readiness_results.clear(); // forcer une nouvelle analyse à l'ouverture
            ui.close();
        }
        if ui.button("🩺  Diagnostic système").clicked() {
            panels.diagnostic = true;
            ui.close();
        }
        if ui.button("🌐  Multijoueur").clicked() {
            panels.multiplayer = true;
            ui.close();
        }
        ui.separator();
        if ui.button("⚙  Paramètres").clicked() {
            panels.settings = true;
            ui.close();
        }
    });
}

/// Menu « Aide » : raccourcis, guide export, diagnostic, à propos.
pub(super) fn menu_aide(ui: &mut egui::Ui, panels: &mut Panels) {
    ui.menu_button("Aide", |ui| {
        if ui.button("⌨  Raccourcis clavier").clicked() {
            panels.shortcuts = true;
            ui.close();
        }
        if ui.button("🩺  Diagnostic système").clicked() {
            panels.diagnostic = true;
            ui.close();
        }
        if ui.button("🩹  Journal de crash").clicked() {
            panels.crash_log = true;
            ui.close();
        }
        ui.separator();
        ui.hyperlink_to(
            "📖  Guide export APK",
            "https://github.com/lberthod/rusteegear",
        );
        ui.hyperlink_to("🐙  Dépôt GitHub", "https://github.com/lberthod/rusteegear");
        ui.separator();
        if ui.button("ℹ  À propos de RusteeGear").clicked() {
            panels.about = true;
            ui.close();
        }
    });
}
