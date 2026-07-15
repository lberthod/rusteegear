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
                                    let resp = ui
                                        .dnd_drag_source(egui::Id::new(("obj", i)), i, |ui| {
                                            let _ = ui.selectable_label(is_sel, label);
                                        })
                                        .response;
                                    // Dépôt d'un autre objet *sur* cette ligne → réordonner avant elle.
                                    if let Some(src) = resp.dnd_release_payload::<usize>()
                                        && *src != i
                                    {
                                        actions.reorder = Some((*src, i));
                                    }
                                    if resp.clicked() {
                                        let m = ui.input(|inp| inp.modifiers);
                                        if m.command || m.shift {
                                            // toggle dans l'ensemble
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
                                        }
                                    }
                                    if resp.double_clicked() {
                                        *rename = Some((i, obj.name.clone()));
                                    }
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
                        if ui
                            .selectable_label(*selected_light == Some(i), label)
                            .clicked()
                        {
                            *selected_light = Some(i);
                            *selection = None;
                            selected.clear();
                        }
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
