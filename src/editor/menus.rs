//! Barre de menus de l'éditeur (Fichier/Édition/Ajouter/Outils/Aide) : nouveau/
//! ouvrir/enregistrer/exporter, undo/redo/copier-coller, ajout de primitives et de
//! démos, réglages rapides (gizmo, physique). Extrait de `editor/mod.rs`.

use crate::app::GizmoMode;
use crate::app::settings::RecentProject;
use crate::app::shortcuts::{MenuItem, menu_hint};
use crate::scene::{MeshKind, Scene};

use super::file_dialogs::{DialogRequest, DialogTarget, FileDialogs};
use super::{Panels, StatusInfo, UiActions, export};

/// Entrée de menu avec son raccourci grisé à droite (`Button::shortcut_text`),
/// lu dans la table `shortcuts::SHORTCUTS` plutôt que recopié dans chaque
/// libellé (roadmap post-audit UX v2 2026-09-04, 4.3).
fn item(label: &str, shortcut: MenuItem) -> egui::Button<'static> {
    egui::Button::new(label.to_owned()).shortcut_text(menu_hint(shortcut))
}

/// Infobulle de « ⭐ Premier jeu » (menu Fichier, sous-menu Démos, écran
/// d'accueil).
pub(super) const FIRST_GAME_HINT: &str = "Ouvre le projet tutoriel `examples/first_game` : sol, joueur pilotable, \
     caisses, cube tournant scripté, zone d'éveil, pièces à ramasser — la \
     meilleure porte d'entrée pour découvrir l'éditeur";

/// « ⭐ Premier jeu » : ouvre le projet tutoriel — dépôt source si présent,
/// sinon copie embarquée dans le dossier utilisateur (cf.
/// `project::first_game_dir`). Partagé entre le menu Fichier (premier niveau
/// et sous-menu Démos) et l'écran d'accueil (roadmap post-audit UX v2
/// 2026-09-04, 3.1).
pub(super) fn open_first_game(actions: &mut UiActions) {
    match crate::project::first_game_dir() {
        Ok(path) => {
            actions.open_project_path = Some(path.to_string_lossy().into_owned());
        }
        Err(e) => log::error!("Projet « Premier jeu » indisponible : {e}"),
    }
}

/// Menu « Fichier » : sauvegarde, ouverture, import, export.
///
/// `has_project` (Sprint 4) grise « Fermer le projet »/« Dupliquer »/« Révéler
/// dans le Finder » quand aucun projet n'est ouvert. `recents` (tous les
/// projets récents, avec « existe encore » — `Settings::recent_projects_with_status`)
/// alimente le sous-menu « Projets récents » : les disparus y sont grisés avec
/// « Localiser… / Retirer de la liste » (roadmap post-audit UX v2 2026-09-04,
/// 3.8). `suggested_save_name` : nom proposé par « Enregistrer sous… » (3.2).
/// Les sélecteurs de fichiers passent par `dialogs` (asynchrones, roadmap 6.1).
#[allow(clippy::too_many_arguments)] // menu : un paramètre par état distinct
pub(super) fn menu_fichier(
    ui: &mut egui::Ui,
    export: &mut export::ExportPanel,
    actions: &mut UiActions,
    dialogs: &mut FileDialogs,
    has_project: bool,
    recents: Vec<(&RecentProject, bool)>,
    suggested_save_name: &str,
    // Play en cours (roadmap post-audit UX v2 2026-09-04, 1.8) : « Enregistrer »
    // est grisé, comme 💾 et Cmd+S — enregistrer écrasait la scène du projet
    // avec l'état simulé.
    playing: bool,
) {
    ui.menu_button("Fichier", |ui| {
        // (les sélecteurs « Enregistrer sous… » / « Ouvrir… » sont dans
        // `dialog_save_as` / `dialog_open`, partagés avec les raccourcis clavier)
        if ui
            .add(item("✨  Nouveau projet", MenuItem::NewProject))
            .on_hover_text("Choix guidé de template (scène vide, démo, niveau de combat)")
            .clicked()
        {
            actions.open_new_project_wizard = true;
            ui.close();
        }
        // Au premier niveau (roadmap post-audit UX v2 2026-09-04, 3.1) : la
        // porte d'entrée ne se cache plus dans Démos ▸ Commencer.
        if ui
            .button("⭐  Premier jeu")
            .on_hover_text(FIRST_GAME_HINT)
            .clicked()
        {
            open_first_game(actions);
            ui.close();
        }
        // Les démos sont regroupées dans un sous-menu pour ne pas noyer les vraies
        // actions fichier ; la scène MMORPG (scène centrale du projet, chargée au
        // démarrage) est en tête pour pouvoir la recharger facilement.
        ui.menu_button("🎬  Démos", |ui| {
            ui.menu_button("⭐  Commencer", |ui| {
                if ui
                    .button("⭐  Premier jeu")
                    .on_hover_text(FIRST_GAME_HINT)
                    .clicked()
                {
                    open_first_game(actions);
                    ui.close();
                }
                if ui
                    .button("🌐  Démo MMORPG (scène centrale, multijoueur PC ↔ mobile)")
                    .on_hover_text(
                        "La scène chargée au démarrage de l'éditeur : hameau, ménagerie de créatures, \
                         joueur pilotable (joystick + saut), pensée pour voir un client desktop et un \
                         APK se déplacer l'un par rapport à l'autre",
                    )
                    .clicked()
                {
                    actions.load_mmorpg = true;
                    ui.close();
                }
            });
            ui.menu_button("🎮  Jeux jouables", |ui| {
                if ui
                    .button("🧟  Zombies (local, sans réseau)")
                    .on_hover_text(
                        "Manches de monstres (Rôdeur/Coureur/Brute) qui poursuivent le joueur, style Call of Zombies",
                    )
                    .clicked()
                {
                    actions.load_ai_duel = true;
                    ui.close();
                }
                if ui
                    .button("🗡  Donjon (roguelike, 3 salles)")
                    .on_hover_text(
                        "3 salles à vider une à une (porte fermée jusqu'à la précédente vidée), arme de départ tirée au sort",
                    )
                    .clicked()
                {
                    actions.load_roguelike = true;
                    ui.close();
                }
                if ui
                    .button("🗼  Tour d'ascension (platforming)")
                    .on_hover_text(
                        "Style différent : grimpe la tour en spirale, aucune arme ni combat, éviter le vide",
                    )
                    .clicked()
                {
                    actions.load_tower = true;
                    ui.close();
                }
                if ui
                    .button("🏃  Course infinie (style Temple Run)")
                    .on_hover_text(
                        "Course automatique + changement de voie + saut : esquive les obstacles, ramasse les pièces",
                    )
                    .clicked()
                {
                    actions.load_temple_run = true;
                    ui.close();
                }
                if ui
                    .button("🥊  Duel (façon Tekken/Smash Bros)")
                    .on_hover_text(
                        "Arène flottante, un rival à plusieurs coups avant de tomber, ring out possible (le vide sous l'arène est mortel)",
                    )
                    .clicked()
                {
                    actions.load_brawl = true;
                    ui.close();
                }
            });
            ui.menu_button("🌐  Modes multijoueur", |ui| {
                if ui
                    .button("🧟  Vagues (RoundObjective::Vagues)")
                    .on_hover_text(
                        "Manches successives jusqu'à la dernière vidée — mode par défaut des salons multijoueur",
                    )
                    .clicked()
                {
                    actions.load_ai_duel = true;
                    ui.close();
                }
                if ui
                    .button("❤  Survie (RoundObjective::Survie)")
                    .on_hover_text(
                        "Vagues qui recommencent en boucle une fois la dernière vidée : survivre le plus longtemps possible plutôt que gagner à la dernière manche",
                    )
                    .clicked()
                {
                    actions.load_survie = true;
                    ui.close();
                }
                if ui
                    .button("👑  Boss (RoundObjective::Boss)")
                    .on_hover_text(
                        "Arène fermée, un adversaire unique à PV massifs et contact doublé (RoundObjective::Boss)",
                    )
                    .clicked()
                {
                    actions.load_boss = true;
                    ui.close();
                }
                if ui
                    .button("🛒  Escorte (RoundObjective::Escorte)")
                    .on_hover_text(
                        "Convoi lent à mener d'un bout à l'autre d'un couloir, ciblé en priorité par les créatures (RoundObjective::Escorte)",
                    )
                    .clicked()
                {
                    actions.load_escorte = true;
                    ui.close();
                }
            });
            ui.menu_button("🧰  Exemples techniques", |ui| {
                if ui
                    .button("🕹  Contrôleur (joystick + saut, sans script)")
                    .on_hover_text(
                        "Joueur pilotable au joystick, saut sur bouton, collisions avec le décor",
                    )
                    .clicked()
                {
                    actions.load_controller = true;
                    ui.close();
                }
                if ui
                    .button("🎮  Mobile (jouable)")
                    .on_hover_text("Charge une scène : joystick + bouton Saut + personnage scripté")
                    .clicked()
                {
                    actions.load_demo = true;
                    ui.close();
                }
                if ui
                    .button("🎯  Gameplay/API (complète)")
                    .on_hover_text("Joystick + gyroscope + saut + zone de danger + barre de vie + tap")
                    .clicked()
                {
                    actions.load_gameplay = true;
                    ui.close();
                }
                if ui
                    .button("🧩  Composants (Controller/Audio/Combat)")
                    .on_hover_text(
                        "Référence minimale : un objet par composant optionnel, pas un niveau de jeu",
                    )
                    .clicked()
                {
                    actions.load_components_demo = true;
                    ui.close();
                }
            });
        });
        ui.separator();
        if ui
            .button("✨  Générer une scène (IA)… — expérimental")
            .on_hover_text("Crée une scène complète depuis une description (DeepSeek)")
            .clicked()
        {
            actions.open_ai_scene = true;
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!playing, item("💾  Enregistrer", MenuItem::Save))
            .on_hover_text(
                "Dans la scène du projet ouvert ou le fichier ouvert ; sans fichier, \
                 ouvre « Enregistrer sous… »",
            )
            .on_disabled_hover_text("Arrête la partie pour enregistrer")
            .clicked()
        {
            actions.save = true;
            ui.close();
        }
        if ui.add(item("💾  Enregistrer sous…", MenuItem::SaveAs)).clicked() {
            dialog_save_as(dialogs, suggested_save_name);
            ui.close();
        }
        if ui.add(item("📂  Ouvrir…", MenuItem::Open)).clicked() {
            dialog_open(dialogs, actions);
            ui.close();
        }
        if ui
            .button("📂  Ouvrir un projet…")
            .on_hover_text("Choisir directement le dossier d'un projet, sans passer par son manifeste")
            .clicked()
        {
            dialogs.open(DialogRequest::PickFolder, DialogTarget::OpenProjectFolder);
            ui.close();
        }
        if !recents.is_empty() {
            ui.menu_button("🕘  Projets récents", |ui| {
                for (recent, exists) in &recents {
                    if *exists {
                        if ui.button(&recent.name).on_hover_text(&recent.path).clicked() {
                            actions.open_project_path = Some(recent.path.clone());
                            ui.close();
                        }
                        continue;
                    }
                    // Projet déplacé ou sur un volume débranché (roadmap
                    // post-audit UX v2 2026-09-04, 3.8) : grisé et explicite,
                    // avec de quoi le retrouver ou l'oublier — avant, il
                    // disparaissait de la liste sans un mot.
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!("{} — Introuvable", recent.name)),
                    )
                    .on_disabled_hover_text(format!("Absent de {}", recent.path));
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        if ui
                            .small_button("Localiser…")
                            .on_hover_text("Choisir le dossier où ce projet se trouve maintenant")
                            .clicked()
                        {
                            // L'ancienne entrée est oubliée une fois le dossier
                            // choisi ; l'ouverture réussie enregistre la nouvelle
                            // (`note_recent_project`).
                            dialogs.open(
                                DialogRequest::PickFolder,
                                DialogTarget::LocateRecent {
                                    forget: recent.path.clone(),
                                },
                            );
                            ui.close();
                        }
                        if ui.small_button("Retirer de la liste").clicked() {
                            actions.forget_recent_project = Some(recent.path.clone());
                            ui.close();
                        }
                    });
                }
            });
        }
        ui.add_enabled_ui(has_project, |ui| {
            if ui.button("🗂  Fermer le projet").clicked() {
                actions.close_project = true;
                ui.close();
            }
            if ui.button("🧬  Dupliquer le projet").clicked() {
                actions.duplicate_project = true;
                ui.close();
            }
            #[cfg(target_os = "macos")]
            if ui.button("🔍  Révéler dans le Finder").clicked() {
                actions.reveal_project_in_finder = true;
                ui.close();
            }
        });
        ui.separator();
        if ui.button("📥  Importer glTF…").clicked() {
            dialogs.open(
                DialogRequest::PickFile {
                    filter: "glTF",
                    extensions: &["glb", "gltf"],
                },
                DialogTarget::ImportGltf,
            );
            ui.close();
        }
        ui.separator();
        // Un seul libellé pour l'export partout (menus, barre d'outils, titre du
        // panneau) — roadmap post-audit UX v2 2026-09-04, 4.1.
        if ui
            .button("📦  Compiler & exporter…")
            .on_hover_text("Panneau d'export : .dmg / .apk / .ipa / web")
            .clicked()
        {
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
        if ui.add(item("🚪  Quitter", MenuItem::Quit)).clicked() {
            actions.quit = true;
            ui.close();
        }
    });
}

/// Menu « Édition » : historique et opérations sur la sélection.
///
/// Raccourcis affichés à droite et entrées grisées quand elles n'ont rien à
/// faire — Annuler / Rétablir sans historique, Coller sans presse-papiers
/// (`status.has_undo`…) — roadmap post-audit UX v2 2026-09-04, 4.3.
pub(super) fn menu_edition(
    ui: &mut egui::Ui,
    selection: &Option<usize>,
    status: &StatusInfo<'_>,
    actions: &mut UiActions,
) {
    ui.menu_button("Édition", |ui| {
        if ui
            .add_enabled(status.has_undo, item("↩  Annuler", MenuItem::Undo))
            .clicked()
        {
            actions.undo = true;
            ui.close();
        }
        if ui
            .add_enabled(status.has_redo, item("↪  Rétablir", MenuItem::Redo))
            .clicked()
        {
            actions.redo = true;
            ui.close();
        }
        ui.separator();
        let has = selection.is_some();
        if ui
            .add_enabled(has, item("✂  Couper", MenuItem::Cut))
            .clicked()
        {
            actions.cut = true;
            ui.close();
        }
        if ui
            .add_enabled(has, item("⧉  Copier", MenuItem::Copy))
            .clicked()
        {
            actions.copy = true;
            ui.close();
        }
        if ui
            .add_enabled(status.has_clipboard, item("📋  Coller", MenuItem::Paste))
            .clicked()
        {
            actions.paste = true;
            ui.close();
        }
        if ui
            .add_enabled(has, item("⧉  Dupliquer", MenuItem::Duplicate))
            .clicked()
        {
            actions.duplicate = true;
            ui.close();
        }
        if ui
            .add_enabled(has, item("🗑  Supprimer", MenuItem::Delete))
            .clicked()
        {
            actions.delete = *selection;
            ui.close();
        }
        ui.separator();
        if ui
            .add(item("☑  Tout sélectionner", MenuItem::SelectAll))
            .clicked()
        {
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
    dialogs: &mut FileDialogs,
) {
    use crate::scene::{MAX_POINT_LIGHTS, PointLight};
    ui.menu_button("Ajouter", |ui| {
        if ui
            .button("🃏  Ajouter (cartes)…")
            .on_hover_text(
                "Panneau simplifié avec icônes pour les objets/lumière les plus courants",
            )
            .clicked()
        {
            actions.open_add_object_cards = true;
            ui.close();
        }
        ui.separator();
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
                    // « Corps dynamique » plutôt que « Rigidbody » (roadmap post-audit UX
                    // v2 2026-09-04, 4.2).
                    if ui.button("⚙  Corps dynamique").clicked() {
                        scene.objects[i].physics = P::Dynamic;
                        ui.close();
                    }
                    if ui.button("🚶  Cinématique (déplacé par script)").clicked() {
                        scene.objects[i].physics = P::Kinematic;
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
                // Normalisation de loudness à l'import (Sprint 126) : mesurée au
                // retour du sélecteur (`Editor::apply_dialog_result`,
                // `ObjectAudio { normalize: true }`), cf. `AudioSource.gain`.
                if let Some(i) = sel {
                    dialogs.open(
                        DialogRequest::PickFile {
                            filter: "Audio",
                            extensions: &["wav", "ogg", "flac", "mp3"],
                        },
                        DialogTarget::ObjectAudio {
                            index: i,
                            normalize: true,
                        },
                    );
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
                // Mutuellement exclusif avec la croix directionnelle (cf.
                // `MobileControls::toggle_joystick`) : les deux se dessinent dans
                // le même coin de l'écran, jamais les deux à la fois.
                m.toggle_joystick();
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
                m.toggle_dpad();
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
    actions: &mut UiActions,
) {
    ui.menu_button("Outils", |ui| {
        // Même ordre que la barre d'outils : raccourcis Q W E R T Y.
        ui.selectable_value(gizmo_mode, GizmoMode::Pan, "✋  Main — pan caméra (Q)");
        ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔  Déplacer (W)");
        ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻  Tourner (E)");
        ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⛶  Redimensionner (R)");
        ui.selectable_value(gizmo_mode, GizmoMode::Orbit, "🔄  Orbite libre (T)");
        ui.selectable_value(gizmo_mode, GizmoMode::Zoom, "🔍  Loupe — zoom (Y)");
        ui.separator();
        if ui.button("🖥  Console").clicked() {
            panels.console = true;
            ui.close();
        }
        if ui.button("📊  Profiler FPS").clicked() {
            panels.profiler = true;
            ui.close();
        }
        if ui.button("🗺  Mini-carte").clicked() {
            panels.minimap = true;
            ui.close();
        }
        if ui.button("📜  Gestionnaire de scripts Lua").clicked() {
            panels.scripts = true;
            ui.close();
        }
        ui.separator();
        // Même libellé que Fichier (roadmap post-audit UX v2 2026-09-04, 4.1).
        if ui.button("📦  Compiler & exporter…").clicked() {
            export.open = true;
            ui.close();
        }
        if ui.button("📁  Gestionnaire d'assets").clicked() {
            panels.assets = true;
            ui.close();
        }
        // Fenêtre séparée (processus à part, cf. `super::launch_glb_viewer`),
        // pas un panneau flottant `Panels` comme les autres entrées de ce menu :
        // rien à garder « ouvert » côté éditeur, juste une action ponctuelle.
        if ui.button("🖼  Gestionnaire GLB").clicked() {
            actions.launch_glb_viewer = true;
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
        // « Diagnostic système » n'est plus qu'au menu Aide (roadmap post-audit
        // UX v2 2026-09-04, 4.1) : une seule porte d'entrée.
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
        if ui
            .add(item("⌨  Raccourcis clavier", MenuItem::Shortcuts))
            .clicked()
        {
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
        // Phase E4 (sprint.19matin.md) : tout le contexte utile à un rapport de
        // bug en un clic — version/commit/OS/format + derniers logs (bannière et
        // ligne GPU comprises), dossier personnel anonymisé. À coller tel quel
        // dans une issue GitHub.
        if ui
            .button("📋  Copier le diagnostic")
            .on_hover_text(
                "Copie version, commit, OS, GPU et derniers logs — à coller dans une issue",
            )
            .clicked()
        {
            ui.ctx().copy_text(crate::log_buffer::diagnostic_report());
            // Confirmé en toast (roadmap post-audit UX v2 2026-09-04, 6.2).
            log::info!(
                target: super::toasts::FEEDBACK_TARGET,
                "Diagnostic copié dans le presse-papiers."
            );
            ui.close();
        }
        ui.separator();
        // Pointe sur le guide lui-même, pas sur la racine du dépôt comme
        // « Dépôt GitHub » juste dessous (roadmap post-audit UX v2 2026-09-04, 4.1).
        ui.hyperlink_to(
            "📖  Guide export APK",
            "https://github.com/lberthod/rusteegear/blob/main/packaging/build_android.md",
        );
        ui.hyperlink_to("🐙  Dépôt GitHub", "https://github.com/lberthod/rusteegear");
        ui.separator();
        if ui.button("ℹ  À propos de RusteeGear").clicked() {
            panels.about = true;
            ui.close();
        }
    });
}

/// Sélecteur « Enregistrer sous… » — partagé entre le menu Fichier, Cmd+Maj+S
/// (roadmap post-audit UX 2026-09-04, 1.1) et Cmd+S sans fichier lié (roadmap
/// v2, 3.2). Propose `suggested_name` (le vrai nom de la scène courante, pas
/// toujours « scene.json »). Asynchrone (roadmap 6.1) : `actions.save_path`
/// est posé à la frame où le sélecteur se referme (`DialogTarget::SaveAs`) ;
/// sans effet sur mobile/web (pas de sélecteur natif).
pub(super) fn dialog_save_as(dialogs: &mut FileDialogs, suggested_name: &str) {
    dialogs.open(
        DialogRequest::SaveFile {
            filter: "Scène JSON",
            extensions: &["json"],
            file_name: suggested_name.to_string(),
        },
        DialogTarget::SaveAs,
    );
}

/// Sélecteur « Ouvrir… » — partagé entre le menu Fichier et Cmd+O. Sprint 3 :
/// une scène seule (comportement historique) et un projet (son manifeste
/// `project.rusteegear.json`) partagent ce même sélecteur — `load_path` /
/// `open_project_path` sont distingués au retour (`DialogTarget::OpenSceneOrProject`)
/// selon le nom du fichier choisi, et traités par `gfx::renderer`.
pub(super) fn dialog_open(dialogs: &mut FileDialogs, actions: &mut UiActions) {
    let request = DialogRequest::PickFile {
        filter: "Projet ou scène RusteeGear",
        extensions: &["json"],
    };
    let target = DialogTarget::OpenSceneOrProject;
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    {
        let _ = &actions;
        dialogs.open(request, target);
    }
    // Pas de sélecteur natif : la scène par défaut est rechargée, comme avant.
    #[cfg(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))]
    {
        let _ = (&dialogs, request, target);
        actions.load = true;
    }
}
