//! Panneau de hiérarchie de la scène : arbre des objets (groupes, drag & drop
//! de réordonnancement), filtre par nom, badges d'état (physique, script, tag).
//! Extrait de `editor/mod.rs`.

use crate::runtime::physics::PhysicsKind;
use crate::scene::{MeshKind, Scene};

use super::UiActions;

/// Catégorie (libellé + icône) d'un type de mesh, pour le regroupement de la hiérarchie.
fn mesh_category(mesh: MeshKind) -> (&'static str, &'static str) {
    match mesh {
        MeshKind::Cube => ("Cubes", "🧊"),
        MeshKind::Sphere => ("Sphères", "⚪"),
        MeshKind::Plane => ("Plans", "▦"),
        MeshKind::Cylinder => ("Cylindres", "🛢"),
        MeshKind::Capsule => ("Capsules", "💊"),
        MeshKind::Terrain => ("Terrains", "⛰"),
        MeshKind::Imported(_) => ("Modèles", "📦"),
    }
}

/// Badges compacts d'un objet : physique / script / audio.
fn object_badges(obj: &crate::scene::SceneObject) -> String {
    let mut b = String::new();
    match obj.physics {
        PhysicsKind::Static => b.push_str(" 🧱"),
        PhysicsKind::Dynamic => b.push_str(" ⚙"),
        PhysicsKind::Kinematic => b.push_str(" 🚶"),
        PhysicsKind::None => {}
    }
    if !obj.script.trim().is_empty() {
        b.push_str(" 📜");
    }
    if obj.audio.as_ref().is_some_and(|a| !a.clip.is_empty()) {
        b.push_str(" 🔊");
    }
    b
}

/// Hiérarchie ergonomique : recherche, **groupes définis par l'utilisateur** avec
/// glisser-déposer des objets, icônes et badges (physique/script/audio).
#[allow(clippy::too_many_arguments)] // panneau d'UI : états distincts à muter
pub(super) fn hierarchy_panel(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
    selected_light: &mut Option<usize>,
    filter: &mut String,
    new_group: &mut String,
    rename: &mut Option<(usize, String)>,
    actions: &mut UiActions,
) {
    ui.horizontal(|ui| {
        ui.heading("Hiérarchie");
        ui.weak(format!("({})", scene.objects.len()));
    });
    ui.small("Glisser un objet sur un autre : réordonner · sur un groupe : ranger.");
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text("🔎 filtrer…")
            .desired_width(f32::INFINITY),
    );
    // Création d'un groupe.
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_group)
                .hint_text("nouveau groupe")
                .desired_width(130.0),
        );
        if ui.button("➕ Groupe").clicked() {
            let n = new_group.trim().to_string();
            if !n.is_empty() && !scene.groups.contains(&n) {
                scene.groups.push(n);
            }
            new_group.clear();
        }
    });
    ui.separator();

    let needle = filter.trim().to_lowercase();
    // Sections : groupes utilisateur (ordre conservé) puis « Sans groupe ».
    let mut sections: Vec<Option<String>> = scene.groups.iter().cloned().map(Some).collect();
    sections.push(None);

    // Mutations différées (appliquées après l'UI pour éviter les conflits d'emprunt).
    let mut moves: Vec<(usize, String)> = Vec::new();
    let mut delete_group: Option<String> = None;
    let mut commit_rename: Vec<(usize, String)> = Vec::new();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for sec in &sections {
                let gname = sec.clone().unwrap_or_default();
                let title = match sec {
                    Some(g) => format!("📁 {g}"),
                    None => "🗂 Sans groupe".to_string(),
                };
                let items: Vec<(usize, &crate::scene::SceneObject)> = scene
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| {
                        o.group == gname
                            && (needle.is_empty() || o.name.to_lowercase().contains(&needle))
                    })
                    .collect();

                // La zone de dépôt couvre tout le groupe : déposer un objet l'y assigne.
                let (_, payload) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::default(), |ui| {
                    ui.horizontal(|ui| {
                        egui::CollapsingHeader::new(format!("{title}  ({})", items.len()))
                            .default_open(true)
                            .id_salt(("grp", &gname))
                            .show(ui, |ui| {
                                for (i, obj) in &items {
                                    let i = *i;
                                    // Mode renommage inline pour cette ligne.
                                    if let Some((ri, buf)) =
                                        rename.as_mut().filter(|(ri, _)| *ri == i)
                                    {
                                        let r = ui.add(
                                            egui::TextEdit::singleline(buf)
                                                .desired_width(f32::INFINITY),
                                        );
                                        r.request_focus();
                                        // Valide à la perte de focus (Entrée ou clic ailleurs).
                                        if r.lost_focus() {
                                            commit_rename.push((*ri, buf.clone()));
                                        }
                                        continue;
                                    }
                                    let is_sel = selected.contains(&i);
                                    let label = format!(
                                        "{} {}{}",
                                        mesh_category(obj.mesh).1,
                                        obj.name,
                                        object_badges(obj)
                                    );
                                    // Ligne cliquable ET glissable via un seul widget aux deux
                                    // sens : egui attend alors un vrai mouvement du pointeur
                                    // avant de trancher clic vs glisser. Surtout ne pas revenir
                                    // à `dnd_drag_source` : son enveloppe ne capte que le
                                    // glisser, et un widget « drag seul » est marqué dragged dès
                                    // l'appui (sans seuil) — la ligne partait sur une couche
                                    // Tooltip aux réponses vides et le clic de sélection se
                                    // perdait, sauf tap tenant dans une seule frame (d'où la
                                    // sélection aléatoire : « ça fait juste du drag and drop »).
                                    let resp = ui
                                        .selectable_label(is_sel, label)
                                        .interact(egui::Sense::drag());
                                    // Un glisser avéré emporte l'index comme payload…
                                    resp.dnd_set_drag_payload(i);
                                    // …déposé sur une autre ligne : réordonner avant elle.
                                    if let Some(src) = resp.dnd_release_payload::<usize>()
                                        && *src != i
                                    {
                                        actions.reorder = Some((*src, i));
                                    }
                                    // Clic gauche : sélectionne + recentre la caméra (comportement
                                    // standard d'une hiérarchie) ; clic droit : menu d'options.
                                    if resp.clicked() {
                                        let m = ui.input(|inp| inp.modifiers);
                                        // La sélection d'un objet exclut celle d'une lumière :
                                        // l'inspecteur et le gizmo suivent une seule entité.
                                        *selected_light = None;
                                        if m.command || m.shift {
                                            // toggle dans l'ensemble (sélection multiple)
                                            if let Some(p) = selected.iter().position(|&x| x == i) {
                                                selected.remove(p);
                                                *selection = selected.last().copied();
                                            } else {
                                                selected.push(i);
                                                *selection = Some(i);
                                            }
                                        } else {
                                            *selection = Some(i);
                                            *selected = vec![i];
                                            actions.focus_selection = true;
                                        }
                                    }
                                    if resp.double_clicked() {
                                        *rename = Some((i, obj.name.clone()));
                                    }
                                    egui::Popup::context_menu(&resp).show(|ui| {
                                        ui.set_min_width(180.0);
                                        if ui.button("🎯 Sélectionner et centrer").clicked() {
                                            *selected_light = None;
                                            *selection = Some(i);
                                            *selected = vec![i];
                                            actions.focus_selection = true;
                                        }
                                        if ui.button("✏ Renommer").clicked() {
                                            *rename = Some((i, obj.name.clone()));
                                        }
                                        if ui.button("📄 Dupliquer").clicked() {
                                            *selected_light = None;
                                            *selection = Some(i);
                                            *selected = vec![i];
                                            actions.duplicate = true;
                                        }
                                        ui.separator();
                                        if ui.button("🗑 Supprimer").clicked() {
                                            actions.delete = Some(i);
                                        }
                                    });
                                }
                                if items.is_empty() {
                                    ui.weak("  (déposer ici)");
                                }
                            });
                        // Bouton de suppression pour les groupes utilisateur.
                        if sec.is_some() && ui.small_button("🗑").clicked() {
                            delete_group = Some(gname.clone());
                        }
                    });
                });
                if let Some(idx) = payload {
                    moves.push((*idx, gname.clone()));
                }
            }
            if scene.objects.is_empty() {
                ui.weak("(scène vide)");
            }
        });

    // Application des mutations.
    for (idx, g) in moves {
        if let Some(o) = scene.objects.get_mut(idx) {
            o.group = g;
        }
    }
    if let Some(g) = delete_group {
        scene.groups.retain(|x| x != &g);
        for o in &mut scene.objects {
            if o.group == g {
                o.group.clear();
            }
        }
    }
    // Renommage validé.
    if let Some((idx, name)) = commit_rename.into_iter().next() {
        if let Some(o) = scene.objects.get_mut(idx) {
            o.name = name;
        }
        *rename = None;
    }

    // Section « Lumières » : liste les lumières ponctuelles comme des entités.
    if !scene.point_lights.is_empty() {
        ui.separator();
        egui::CollapsingHeader::new(format!("💡 Lumières ({})", scene.point_lights.len()))
            .default_open(true)
            .show(ui, |ui| {
                let mut remove = None;
                for i in 0..scene.point_lights.len() {
                    let spot = scene.point_lights[i].spot_angle > 0.0;
                    let label = if spot {
                        format!("🔦 Spot {i}")
                    } else {
                        format!("💡 Point {i}")
                    };
                    ui.horizontal(|ui| {
                        let resp = ui.selectable_label(*selected_light == Some(i), label);
                        if resp.clicked() {
                            *selected_light = Some(i);
                            *selection = None;
                            selected.clear();
                            actions.focus_selection = true;
                        }
                        if resp.secondary_clicked() && *selected_light != Some(i) {
                            *selected_light = Some(i);
                            *selection = None;
                            selected.clear();
                            actions.focus_selection = true;
                        }
                        resp.context_menu(|ui| {
                            if ui.button("🎯 Centrer la vue").clicked() {
                                actions.focus_selection = true;
                                ui.close();
                            }
                            if ui.button("🗑 Supprimer").clicked() {
                                remove = Some(i);
                                ui.close();
                            }
                        });
                        if ui.small_button("🗑").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    scene.point_lights.remove(i);
                    if *selected_light == Some(i) {
                        *selected_light = None;
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    /// Rejoue un clic réaliste (appui à une frame, relâchement à la suivante,
    /// pointeur immobile) sur un widget construit par `build`, et rapporte
    /// `clicked()` observé à la frame du relâchement.
    // `Context::run`/`CentralPanel::show` sont dépréciés mais restent le harnais
    // headless le plus simple pour rejouer des frames dans un test.
    #[allow(deprecated)]
    fn clicked_after_two_frame_press(
        build: impl Fn(&mut egui::Ui) -> egui::Response + Copy,
    ) -> bool {
        let ctx = egui::Context::default();
        let pos = egui::pos2(20.0, 15.0); // dans la ligne, marges du panneau incluses
        let mut clicked = false;
        let run = |events: Vec<egui::Event>, record: &mut bool| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    *record = build(ui).clicked();
                });
            });
        };
        // Frame de mise en place (les hit-tests d'egui lisent les rects de la
        // frame précédente), puis appui, puis relâchement — sans mouvement.
        run(vec![egui::Event::PointerMoved(pos)], &mut clicked);
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(vec![press(true)], &mut clicked);
        assert!(!clicked, "pas de clic tant que le bouton est enfoncé");
        run(vec![press(false)], &mut clicked);
        clicked
    }

    /// Bout en bout sur le *vrai* panneau : un clic deux-frames quelque part sur
    /// la ligne du seul objet de la scène doit le sélectionner ET demander le
    /// recentrage caméra. Comme la position exacte de la ligne dépend du style,
    /// on balaye verticalement : au moins une ordonnée doit toucher la ligne, et
    /// chaque fois que la sélection se déclenche, le recentrage doit suivre.
    #[test]
    #[allow(deprecated)] // même harnais headless que `clicked_after_two_frame_press`
    fn clicking_a_row_of_the_real_hierarchy_panel_selects_and_focuses() {
        let mut any_hit = false;
        for y in (60..280).step_by(4) {
            let ctx = egui::Context::default();
            let pos = egui::pos2(60.0, y as f32);
            let mut scene = crate::scene::Scene::default();
            scene.objects.push(crate::scene::SceneObject {
                name: "Cible".into(),
                mesh: crate::scene::MeshKind::Cube,
                ..Default::default()
            });
            let mut selection = None;
            let mut selected = Vec::new();
            let mut selected_light = None;
            let (mut filter, mut new_group, mut rename) = (String::new(), String::new(), None);
            let mut actions = super::super::UiActions::default();
            let mut run = |events: Vec<egui::Event>,
                           scene: &mut crate::scene::Scene,
                           selection: &mut Option<usize>,
                           selected: &mut Vec<usize>,
                           selected_light: &mut Option<usize>,
                           actions: &mut super::super::UiActions| {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        super::hierarchy_panel(
                            ui,
                            scene,
                            selection,
                            selected,
                            selected_light,
                            &mut filter,
                            &mut new_group,
                            &mut rename,
                            actions,
                        );
                    });
                });
            };
            let press = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(
                vec![egui::Event::PointerMoved(pos)],
                &mut scene,
                &mut selection,
                &mut selected,
                &mut selected_light,
                &mut actions,
            );
            run(
                vec![press(true)],
                &mut scene,
                &mut selection,
                &mut selected,
                &mut selected_light,
                &mut actions,
            );
            run(
                vec![press(false)],
                &mut scene,
                &mut selection,
                &mut selected,
                &mut selected_light,
                &mut actions,
            );
            if selection == Some(0) {
                any_hit = true;
                assert_eq!(selected, vec![0], "l'ensemble sélectionné doit suivre");
                assert!(
                    actions.focus_selection,
                    "sélectionner depuis la hiérarchie doit demander le recentrage caméra"
                );
            }
        }
        assert!(
            any_hit,
            "aucune ordonnée du balayage n'a sélectionné l'objet : le clic de \
             sélection est perdu dans le vrai panneau"
        );
    }

    /// Preuve du correctif : une ligne de hiérarchie = un seul widget qui capte
    /// clic ET glisser. Le clic immobile est alors fiable, là où l'ancienne
    /// enveloppe `dnd_drag_source` (drag seul, marquée « dragged » dès l'appui)
    /// le perdait dès que appui et relâchement tombaient sur deux frames.
    #[test]
    fn hierarchy_row_click_survives_a_press_spanning_two_frames() {
        assert!(
            clicked_after_two_frame_press(|ui| {
                ui.selectable_label(false, "Sphère")
                    .interact(egui::Sense::drag())
            }),
            "le clic immobile doit sélectionner la ligne (pattern clic + glisser)"
        );
        assert!(
            !clicked_after_two_frame_press(|ui| {
                ui.dnd_drag_source(egui::Id::new("row"), 0usize, |ui| {
                    ui.selectable_label(false, "Sphère")
                })
                .inner
            }),
            "témoin : l'enveloppe dnd_drag_source perd bien ce même clic — si ce \
             témoin casse un jour (egui corrigé), le pattern simple reste valable"
        );
    }
}
