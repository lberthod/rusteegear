//! HUD de jeu (mode Play) : vie, arme équipée, manches, classement multijoueur,
//! réticule, overlay tactile mobile. Extrait de `editor/mod.rs`.

use crate::scene::{HudAnchor, HudBinding, HudLayout, HudWidgetKind, ItemKind, Scene};

use super::{HudPreview, UiActions};

/// Valeurs de jeu consultables par un `HudBinding` — snapshot pris une fois par
/// frame côté appelant (`AppState`), cette couche de rendu ne connaît pas `AppState`
/// directement (cf. le reste du module, purement fonction de `area`/`scene`).
pub(super) struct HudWidgetValues {
    pub health: f32,
    pub score: u32,
    pub kills: u32,
    pub wave: u32,
}

fn hud_binding_value(binding: HudBinding, v: &HudWidgetValues) -> f32 {
    match binding {
        HudBinding::None => 0.0,
        HudBinding::Health => v.health,
        HudBinding::Score => v.score as f32,
        HudBinding::Kills => v.kills as f32,
        HudBinding::Wave => v.wave as f32,
    }
}

/// Cache de textures pour les widgets `Image` : une texture GPU par chemin
/// d'asset, chargée une fois — `egui::Context::load_texture` ne dé-duplique pas par
/// nom, une ré-upload par frame ferait chuter le FPS. `None` mémorise un chemin
/// invalide pour ne pas retenter le décodage à chaque frame.
pub(super) type HudImageCache = std::collections::HashMap<String, Option<egui::TextureHandle>>;

fn hud_image<'a>(
    ctx: &egui::Context,
    cache: &'a mut HudImageCache,
    path: &str,
) -> Option<&'a egui::TextureHandle> {
    cache
        .entry(path.to_string())
        .or_insert_with(|| {
            let (rgba, w, h) = crate::gfx::pipelines::load_rgba(path)?;
            let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            Some(ctx.load_texture(path, color, egui::TextureOptions::default()))
        })
        .as_ref()
}

/// Widgets déclaratifs `Scene::hud_widgets` (texte, image, jauge, bouton), ancrés à
/// un coin de la zone de jeu via `HudAnchor` — cf. doc de `HudWidget`. Renvoie
/// l'`action` de chaque bouton cliqué ce frame, à transmettre à
/// `AppState::push_hud_event` (même mécanisme que `emit()` côté Lua : lu au tick
/// suivant via `on_event("hud:<action>")`).
pub(super) fn hud_widgets(
    ctx: &egui::Context,
    area: egui::Rect,
    scene: &Scene,
    values: &HudWidgetValues,
    image_cache: &mut HudImageCache,
) -> Vec<String> {
    let mut clicked = Vec::new();
    for widget in &scene.hud_widgets {
        let (fx, fy) = widget.anchor.fraction();
        let anchor_pos = egui::pos2(
            area.left() + area.width() * fx + widget.offset[0],
            area.top() + area.height() * fy + widget.offset[1],
        );
        let pivot = match widget.anchor {
            HudAnchor::TopLeft => egui::Align2::LEFT_TOP,
            HudAnchor::TopRight => egui::Align2::RIGHT_TOP,
            HudAnchor::BottomLeft => egui::Align2::LEFT_BOTTOM,
            HudAnchor::BottomRight => egui::Align2::RIGHT_BOTTOM,
            HudAnchor::Center => egui::Align2::CENTER_CENTER,
        };
        egui::Area::new(egui::Id::new(("hud_widget", widget.id.as_str())))
            .fixed_pos(anchor_pos)
            .pivot(pivot)
            .order(egui::Order::Foreground)
            .movable(false)
            .show(ctx, |ui| match &widget.kind {
                HudWidgetKind::Text { content, binding } => {
                    let text = match binding {
                        HudBinding::None => content.clone(),
                        b => format!("{content} {}", hud_binding_value(*b, values) as i64),
                    };
                    ui.colored_label(egui::Color32::WHITE, text);
                }
                HudWidgetKind::Image { path } => {
                    if let Some(tex) = hud_image(ctx, image_cache, path) {
                        let size = if widget.size == [0.0, 0.0] {
                            tex.size_vec2()
                        } else {
                            egui::vec2(widget.size[0], widget.size[1])
                        };
                        ui.image((tex.id(), size));
                    }
                }
                HudWidgetKind::Gauge {
                    binding,
                    max,
                    color,
                } => {
                    let frac = if *max > 0.0 {
                        (hud_binding_value(*binding, values) / max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let w = if widget.size[0] > 0.0 {
                        widget.size[0]
                    } else {
                        160.0
                    };
                    let h = if widget.size[1] > 0.0 {
                        widget.size[1]
                    } else {
                        16.0
                    };
                    let col = egui::Color32::from_rgb(
                        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(w)
                            .desired_height(h)
                            .fill(col),
                    );
                }
                HudWidgetKind::Button { label, action } => {
                    let w = if widget.size[0] > 0.0 {
                        widget.size[0]
                    } else {
                        120.0
                    };
                    let h = if widget.size[1] > 0.0 {
                        widget.size[1]
                    } else {
                        32.0
                    };
                    if ui
                        .add_sized([w, h], egui::Button::new(label.as_str()))
                        .clicked()
                    {
                        clicked.push(action.clone());
                    }
                }
            });
    }
    clicked
}

/// Flash rouge plein écran quand la vie baisse (contact ennemi) : retour immédiat, même
/// sans regarder la barre de vie. `intensity` (1 = pic du coup) décroît vers 0 côté App.
pub(super) fn damage_vignette(ctx: &egui::Context, area: egui::Rect, intensity: f32) {
    use egui::Color32;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_damage_flash"),
    ));
    let alpha = (70.0 * intensity.clamp(0.0, 1.0)) as u8;
    painter.rect_filled(
        area,
        0.0,
        Color32::from_rgba_unmultiplied(220, 20, 20, alpha),
    );
}

/// Indicateur de manche (haut-centre), pour les scènes à système de manches (cf.
/// `Combat::wave`/`AppState::wave`) — style « Vague N/M » (Call of Zombies). N'affiche
/// rien si `wave == 0` (pas de système de manches dans la scène courante).
/// HUD de l'arme à distance équipée (bas-centre, entre le pavé tank et les
/// boutons tactiles) : libellé + rappel des raccourcis. Texte ASCII/latin
/// uniquement — pas d'emoji, absents de la fonte egui embarquée sur Android
/// Point d'ancrage réglable pour un élément HUD peint au pinceau (pas une
/// `egui::Window`) : renvoie la position finale (`base` décalé par `offset`,
/// cf. `Scene::hud_layout`) et, si `draggable` (🖐 Repositionner du panneau 👁
/// Aperçu HUD), rend un hit-test glissable de taille `hit_size` centré dessus
/// qui met `offset` à jour. `Ui::interact` (seul moyen d'obtenir une réponse
/// de glisser sur un rect arbitraire — `Context::interact` n'existe pas dans
/// egui) exige un `Ui` : on en emprunte un via une `egui::Area` invisible et
/// fixe (repositionnée nous-mêmes chaque frame depuis `offset`, plutôt que de
/// laisser egui mémoriser sa propre position — sinon changer de scène ne
/// réinitialiserait pas la position affichée à celle de la nouvelle scène).
pub(super) fn hud_anchor(
    ctx: &egui::Context,
    id_source: &str,
    base: egui::Pos2,
    offset: &mut [f32; 2],
    hit_size: egui::Vec2,
    draggable: bool,
) -> egui::Pos2 {
    if draggable {
        let pos = base + egui::vec2(offset[0], offset[1]);
        let rect = egui::Rect::from_center_size(pos, hit_size);
        let id = egui::Id::new(id_source);
        egui::Area::new(id.with("area"))
            .fixed_pos(rect.min)
            .movable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let response = ui.interact(rect, id, egui::Sense::drag());
                if response.dragged() {
                    offset[0] += response.drag_delta().x;
                    offset[1] += response.drag_delta().y;
                }
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 210, 90)),
                    egui::StrokeKind::Outside,
                );
            });
    }
    base + egui::vec2(offset[0], offset[1])
}

/// (même contrainte de fonte que le pavé W/A/S/D de `mobile_overlay` : les
/// glyphes manquants sur Android rendent des carrés vides).
pub(super) fn weapon_hud(
    ctx: &egui::Context,
    area: egui::Rect,
    label: &str,
    offset: &mut [f32; 2],
    draggable: bool,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let base = egui::pos2(area.center().x, area.bottom() - 24.0);
    let box_size = egui::vec2(320.0, 44.0) * scale;
    let center = hud_anchor(ctx, "hud_weapon", base, offset, box_size, draggable);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_weapon"),
    ));
    // Plaque de fond semi-transparente (comme `health_bar`) : sans elle, le
    // texte devient illisible sur un sol clair (vert olive, sable...). Une
    // seule plaque sous les deux lignes, pas une par ligne : plus net
    // visuellement qu'un empilement de rectangles.
    let bg = egui::Rect::from_center_size(center, box_size);
    painter.rect_filled(bg, 6.0, Color32::from_black_alpha(110));
    painter.text(
        center + egui::vec2(0.0, -10.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::weapon_label(locale, label),
        FontId::proportional(16.0 * scale),
        Color32::from_rgb(255, 170, 80),
    );
    painter.text(
        center + egui::vec2(0.0, 10.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::fire_hint(locale),
        FontId::proportional(11.0 * scale),
        Color32::from_white_alpha(180),
    );
}

/// Frags (haut-droite) : compteur individualisé en multijoueur (brique de
/// progression pour un futur MMORPG, cf. `AppState::displayed_kill_count`),
/// ou simplement le score solo hors ligne — un seul nombre, la distinction
/// entre les deux modes est déjà résolue par `displayed_kill_count`.
/// Indépendant de `collectibles_hud` (plus bas), qui ne s'affiche que si la
/// scène a des collectibles — la carte multijoueur n'en a pas, donc sans ce
/// HUD dédié, aucun score n'était jamais visible en ligne.
///
/// **Position sous l'overlay Multijoueur** : la fenêtre repliable
/// `mobile_multiplayer_overlay` occupe déjà le coin haut-droite (ancrée à
/// `y=56`, ~30 px de haut une fois repliée) — `y=112` garde une vraie marge
/// en dessous (cf. docs/audits/editor.md pour le premier réglage qui la
/// chevauchait encore).
#[allow(clippy::too_many_arguments)]
pub(super) fn kills_hud(
    ctx: &egui::Context,
    area: egui::Rect,
    kills: u32,
    assists: u32,
    offset: &mut [f32; 2],
    draggable: bool,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    // Boîte alignée à droite avec une marge fixe (8 px) plutôt que centrée sur un
    // point à distance fixe du bord : centrer débordait de ~55 px au-delà de `area`
    // (donc par-dessus l'Inspecteur en mode Édition), la largeur de la boîte n'étant
    // pas prise en compte dans le calcul du centre.
    let box_size = egui::vec2(150.0, 30.0) * scale;
    let base = egui::pos2(area.right() - 8.0 - box_size.x / 2.0, area.top() + 112.0);
    let pos = hud_anchor(ctx, "hud_kills", base, offset, box_size, draggable);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_kills"),
    ));
    let bg = egui::Rect::from_center_size(pos, box_size);
    painter.rect_filled(bg, 6.0, Color32::from_black_alpha(110));
    // Détail frags/assists (Phase L Sprint 3, `sprint2audijeu0718.md`) : les
    // deux valeurs séparées plutôt qu'un seul total — un assist n'est pas un
    // frag (cf. `app::multiplayer::credit_assists_on_kill`), la contribution
    // en solo (jamais d'assist) reste lisible telle quelle (« 0 🤝 »).
    painter.text(
        pos,
        Align2::CENTER_CENTER,
        crate::app::locale::kills_and_assists(locale, kills, assists),
        FontId::proportional(18.0 * scale),
        Color32::from_rgb(255, 170, 130),
    );
}

/// Inventaire d'armes à distance (fenêtre repliable, même mécanisme que
/// `mobile_multiplayer_overlay` — l'état plié/déplié est géré par egui lui-même,
/// pas par un champ dédié). Liste chaque arme connue (pastille de couleur +
/// nom), surligne l'arme équipée, et permet d'en équiper une autre d'un clic
/// — un vrai panneau d'inventaire plutôt que le simple cycle du bouton
/// tactile « Arme ». N'apparaît que si la scène a un joueur équipé d'une arme
/// à distance (cf. `scene_has_ranged_weapon`).
///
/// Positionné par rapport à `area` (la zone de jeu : cadre téléphone en
/// Aperçu mobile, ou tout l'écran en player autonome) et non par rapport à
/// l'écran de l'éditeur — sinon la fenêtre atterrit sur les panneaux
/// Hiérarchie/Inspecteur au lieu de rester dans la scène de jeu. `offset`
/// (cf. `Scene::hud_layout`) reste appliqué même sans glisser, pour que la
/// fenêtre revienne exactement là où elle a été placée après un changement
/// de scène — une petite poignée 🖐 (via `hud_anchor`) permet de l'ajuster
/// en mode Repositionner, plutôt que la barre de titre (qu'egui gérerait en
/// interne et qu'on écraserait chaque frame avec `fixed_pos`).
#[allow(clippy::too_many_arguments)]
pub(super) fn weapon_inventory_panel(
    ctx: &egui::Context,
    area: egui::Rect,
    weapons: &[(&str, [f32; 3])],
    selected: usize,
    offset: &mut [f32; 2],
    draggable: bool,
    actions: &mut UiActions,
    locale: crate::app::locale::Locale,
) {
    let pos = hud_anchor(
        ctx,
        "hud_inventory",
        area.min + egui::vec2(8.0, 8.0),
        offset,
        egui::vec2(24.0, 24.0),
        draggable,
    );
    egui::Window::new("🎒 Inventaire")
        .id(egui::Id::new("weapon_inventory"))
        .collapsible(true)
        .default_open(false)
        .resizable(false)
        .fixed_pos(pos)
        .default_width(200.0)
        .show(ctx, |ui| {
            for (i, (label, color)) in weapons.iter().enumerate() {
                let equipped = i == selected;
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        3.0,
                        egui::Color32::from_rgb(
                            (color[0] * 255.0) as u8,
                            (color[1] * 255.0) as u8,
                            (color[2] * 255.0) as u8,
                        ),
                    );
                    let text = if equipped {
                        crate::app::locale::equipped_suffix(locale, label)
                    } else {
                        label.to_string()
                    };
                    if ui.add_enabled(!equipped, egui::Button::new(text)).clicked() {
                        actions.select_weapon = Some(i);
                    }
                });
            }
        });
}

/// Sac du joueur (fenêtre repliable, même mécanisme et même positionnement
/// relatif à `area` que `weapon_inventory_panel`, juste en dessous) : une ligne
/// par sorte d'objet ramassée (pastille de couleur + nom + quantité), avec un
/// bouton « Utiliser » pour les consommables (`ItemKind::heal() > 0` — boire la
/// potion soigne, cf. `AppState::use_item`). N'apparaît que si le sac contient
/// quelque chose : contrairement à l'inventaire d'armes (fixe, connu d'avance),
/// le sac vide n'a rien à montrer.
pub(super) fn item_inventory_panel(
    ctx: &egui::Context,
    area: egui::Rect,
    items: &[(ItemKind, u32)],
    offset: &mut [f32; 2],
    draggable: bool,
    actions: &mut UiActions,
) {
    if items.is_empty() && !draggable {
        return;
    }
    let pos = hud_anchor(
        ctx,
        "hud_item_inventory",
        area.min + egui::vec2(8.0, 44.0),
        offset,
        egui::vec2(24.0, 24.0),
        draggable,
    );
    egui::Window::new("👜 Sac")
        .id(egui::Id::new("item_inventory"))
        .collapsible(true)
        .default_open(false)
        .resizable(false)
        .fixed_pos(pos)
        .default_width(200.0)
        .show(ctx, |ui| {
            if items.is_empty() {
                ui.weak("(vide)");
            }
            for &(kind, count) in items {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    let color = kind.color();
                    ui.painter().rect_filled(
                        rect,
                        3.0,
                        egui::Color32::from_rgb(
                            (color[0] * 255.0) as u8,
                            (color[1] * 255.0) as u8,
                            (color[2] * 255.0) as u8,
                        ),
                    );
                    ui.label(format!("{} ×{}", kind.label(), count));
                    if kind.heal() > 0.0 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Utiliser").clicked() {
                                actions.use_item = Some(kind);
                            }
                        });
                    }
                });
            }
        });
}

/// Entrée du tableau des joueurs en ligne, telle que produite par
/// `network_client::multiplayer_roster` : `(nom, vie 0..1 ou None avant le
/// premier snapshot, frags, soi-même ?)`.
pub type RosterEntry = (String, Option<f32>, Option<u32>, bool);

/// Pourcentage de vie affiché à côté de la mini barre du roster (PHASE I
/// Sprint 2, accessibilité §16.6) : repère non-couleur pour la même
/// information que le remplissage vert/jaune/rouge de `multiplayer_roster_panel`
/// — indispensable pour un joueur daltonien à qui les trois teintes peuvent se
/// confondre. `h` attendu déjà borné à `0.0..=1.0` par l'appelant.
pub(super) fn health_percent_label(h: f32) -> String {
    format!("{}%", (h * 100.0).round() as i32)
}

/// Classement à afficher dans `multiplayer_roster_panel` : trié par frags
/// décroissants (le tri est stable, donc à égalité l'ordre d'origine — soi
/// d'abord — est conservé), frags inconnus comptés 0.
pub(super) fn roster_display_order(roster: &[RosterEntry]) -> Vec<&RosterEntry> {
    let mut sorted: Vec<&RosterEntry> = roster.iter().collect();
    sorted.sort_by_key(|(_, _, kills, _)| std::cmp::Reverse(kills.unwrap_or(0)));
    sorted
}

/// Tableau des joueurs de la partie en ligne (fenêtre repliable, même
/// mécanisme que `weapon_inventory_panel` : l'état plié/déplié est géré par
/// egui). L'équivalent du « TAB » des FPS : pseudo, mini barre de vie et
/// frags de chaque joueur connecté, classés par frags décroissants, avec sa
/// propre ligne surlignée. `multiplayer_roster` (réseau) existait depuis
/// GAMEDESIGN_EN_LIGNE.md §3.4 mais n'était affiché nulle part — sans ce
/// panneau, impossible de savoir qui mène la partie. N'apparaît qu'en ligne
/// (roster vide sinon), à droite du bouton 🎒 Inventaire.
///
/// Positionné par rapport à `area` (zone de jeu), pas l'écran de l'éditeur —
/// même raison que `weapon_inventory_panel`, y compris la poignée 🖐 plutôt
/// que la barre de titre pour le glisser (cf. `hud_anchor`).
pub(super) fn multiplayer_roster_panel(
    ctx: &egui::Context,
    area: egui::Rect,
    roster: &[RosterEntry],
    offset: &mut [f32; 2],
    draggable: bool,
    locale: crate::app::locale::Locale,
) {
    use egui::Color32;
    if roster.is_empty() {
        return;
    }
    let pos = hud_anchor(
        ctx,
        "hud_roster",
        area.min + egui::vec2(216.0, 8.0),
        offset,
        egui::vec2(24.0, 24.0),
        draggable,
    );
    egui::Window::new("👥 Joueurs")
        .id(egui::Id::new("multiplayer_roster"))
        .collapsible(true)
        .default_open(false)
        .resizable(false)
        .fixed_pos(pos)
        .default_width(220.0)
        .show(ctx, |ui| {
            for (name, health, kills, is_self) in roster_display_order(roster) {
                ui.horizontal(|ui| {
                    // Mini barre de vie : fond gris, remplissage vert→rouge selon la vie.
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(36.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, Color32::from_gray(60));
                    if let Some(h) = health {
                        let h = h.clamp(0.0, 1.0);
                        let fill = egui::Rect::from_min_size(
                            rect.min,
                            egui::vec2(rect.width() * h, rect.height()),
                        );
                        let color = if h > 0.5 {
                            Color32::from_rgb(110, 200, 110)
                        } else if h > 0.25 {
                            Color32::from_rgb(230, 180, 80)
                        } else {
                            Color32::from_rgb(220, 90, 80)
                        };
                        ui.painter().rect_filled(fill, 2.0, color);
                        // PHASE I Sprint 2 (accessibilité §16.6, colorblind) : la
                        // couleur vert/jaune/rouge de la barre n'est ici que
                        // décorative — le pourcentage numérique porte la même
                        // information indépendamment de la teinte perçue.
                        ui.small(health_percent_label(h));
                    }
                    // Spectateur (0 PV, GDD §5.3/§9.1) : grisé — jamais la
                    // seule information (le nom et la barre de vie vide le
                    // disent aussi), juste une distinction visuelle du reste
                    // du roster, réclamée explicitement par le GDD.
                    let is_spectator = health.is_some_and(|h| h <= 0.0);
                    let display_name = if is_spectator {
                        format!("🕯 {name}")
                    } else {
                        name.clone()
                    };
                    if *is_self {
                        let text = crate::app::locale::you_suffix(locale, &display_name);
                        if is_spectator {
                            ui.label(egui::RichText::new(text).color(Color32::from_gray(140)));
                        } else {
                            ui.strong(text);
                        }
                    } else if is_spectator {
                        ui.label(egui::RichText::new(display_name).color(Color32::from_gray(140)));
                    } else {
                        ui.label(display_name);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("💀 {}", kills.unwrap_or(0)));
                    });
                });
            }
        });
}

/// Dessine les overlays cochés dans `HudPreview` par-dessus la zone de jeu, en
/// mode Édition. Les éléments qui dépendent de l'état d'une partie en cours
/// (frags, joueurs en ligne) utilisent des valeurs d'exemple plutôt que l'état
/// réel (toujours à zéro/vide hors Play) — sinon l'aperçu n'aurait jamais rien
/// à montrer. `hud_layout` est celui de la scène (`Scene::hud_layout`) : en
/// mode 🖐 Repositionner (`preview.reposition`), glisser un élément ici écrit
/// directement dedans, donc s'applique aussi en Play et à l'export.
#[allow(clippy::too_many_arguments)]
pub(super) fn hud_preview_overlays(
    ctx: &egui::Context,
    area: egui::Rect,
    preview: &HudPreview,
    hud_layout: &mut HudLayout,
    weapon_label: &str,
    weapon_inventory: &[(&str, [f32; 3])],
    selected_weapon: usize,
    actions: &mut UiActions,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    let drag = preview.reposition;
    if preview.weapon_hud {
        weapon_hud(
            ctx,
            area,
            weapon_label,
            &mut hud_layout.weapon_hud,
            drag,
            locale,
            scale,
        );
    }
    if preview.kills {
        kills_hud(ctx, area, 3, 1, &mut hud_layout.kills, drag, locale, scale);
    }
    if preview.crosshair {
        crosshair(ctx, area, &mut hud_layout.crosshair, drag, scale);
    }
    if preview.weapon_inventory {
        weapon_inventory_panel(
            ctx,
            area,
            weapon_inventory,
            selected_weapon,
            &mut hud_layout.weapon_inventory,
            drag,
            actions,
            locale,
        );
    }
    if preview.item_inventory {
        // Sac d'exemple (hors Play, le vrai sac est toujours vide — cf. la doc
        // de la fonction : mêmes valeurs factices que le roster ci-dessous).
        let sample = [(ItemKind::Potion, 2), (ItemKind::Cle, 1)];
        item_inventory_panel(
            ctx,
            area,
            &sample,
            &mut hud_layout.item_inventory,
            drag,
            actions,
        );
    }
    if preview.roster {
        let sample: Vec<RosterEntry> = vec![
            ("Vous".to_string(), Some(0.8), Some(3), true),
            ("Alice".to_string(), Some(0.45), Some(5), false),
            ("Bob".to_string(), Some(1.0), Some(1), false),
        ];
        multiplayer_roster_panel(ctx, area, &sample, &mut hud_layout.roster, drag, locale);
    }
}

pub(super) fn scene_has_ranged_weapon(scene: &Scene) -> bool {
    scene.objects.iter().any(|o| {
        o.controller
            .as_ref()
            .is_some_and(|c| c.input && !c.fire_button.is_empty())
    })
}

/// Réticule de visée (centre de l'écran) : petite croix + point central,
/// dessinée en Play dès que la scène a un contrôleur d'arme à distance
/// (`fire_button` non vide) — sans lui, viser une cible avec la boule de feu
/// n'a aucun repère visuel. Discrète (fines lignes blanches
/// semi-transparentes), pour ne jamais gêner la lecture de la scène.
pub(super) fn crosshair(
    ctx: &egui::Context,
    area: egui::Rect,
    offset: &mut [f32; 2],
    draggable: bool,
    scale: f32,
) {
    use egui::{Color32, Stroke};
    let scale = clamp_hud_scale(scale);
    let c = hud_anchor(
        ctx,
        "hud_crosshair",
        area.center(),
        offset,
        egui::vec2(24.0, 24.0) * scale,
        draggable,
    );
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_crosshair"),
    ));
    let stroke = Stroke::new(1.5_f32 * scale, Color32::from_white_alpha(200));
    let gap = 5.0 * scale;
    let len = 7.0 * scale;
    painter.line_segment(
        [egui::pos2(c.x - gap - len, c.y), egui::pos2(c.x - gap, c.y)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x + gap, c.y), egui::pos2(c.x + gap + len, c.y)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x, c.y - gap - len), egui::pos2(c.x, c.y - gap)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x, c.y + gap), egui::pos2(c.x, c.y + gap + len)],
        stroke,
    );
    painter.circle_filled(c, 1.5 * scale, Color32::from_white_alpha(230));
}

pub(super) fn wave_hud(
    ctx: &egui::Context,
    area: egui::Rect,
    scene: &Scene,
    wave: u32,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    let scale = clamp_hud_scale(scale);
    if wave == 0 {
        return;
    }
    let max_wave = scene
        .objects
        .iter()
        .filter_map(|o| o.combat.as_ref())
        .map(|c| c.wave)
        .max()
        .unwrap_or(0);
    if max_wave == 0 {
        return;
    }
    let remaining = scene
        .objects
        .iter()
        .filter(|o| o.visible && o.combat.as_ref().is_some_and(|c| c.wave == wave))
        .count();
    use egui::{Align2, Color32, FontId};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_wave"),
    ));
    painter.text(
        egui::pos2(area.center().x, area.top() + 22.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::wave(locale, wave, max_wave),
        FontId::proportional(22.0 * scale),
        Color32::from_rgb(230, 120, 90),
    );
    painter.text(
        egui::pos2(area.center().x, area.top() + 44.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::remaining(locale, remaining as u32),
        FontId::proportional(14.0 * scale),
        Color32::from_white_alpha(200),
    );
}

/// Borne `Settings::hud_scale` à une plage sûre (au cas où un `settings.json`
/// écrit à la main ou par une future version sort de la plage `0.6..=2.0` du
/// curseur des Paramètres) : les fonctions de dessin du HUD ci-dessous
/// appellent toutes cette fonction en entrée plutôt que de faire confiance à
/// la valeur brute.
pub(super) fn clamp_hud_scale(scale: f32) -> f32 {
    scale.clamp(0.5, 3.0)
}

/// Barre de vie du HUD (haut de la zone de jeu), pilotée par `set_health` côté script.
pub(super) fn health_bar(ctx: &egui::Context, area: egui::Rect, h: f32, scale: f32) {
    use egui::{Color32, Stroke};
    let scale = clamp_hud_scale(scale);
    let h = h.clamp(0.0, 1.0);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_health"),
    ));
    let w = (area.width() * 0.4).min(220.0) * scale;
    let bar_h = 16.0 * scale;
    let bg = egui::Rect::from_min_size(
        egui::pos2(area.left() + 20.0, area.top() + 16.0),
        egui::vec2(w, bar_h),
    );
    painter.rect_filled(bg, 4.0, Color32::from_black_alpha(140));
    let fill = egui::Rect::from_min_size(bg.min, egui::vec2(w * h, bar_h));
    let col = Color32::from_rgb(((1.0 - h) * 220.0) as u8 + 30, (h * 200.0) as u8 + 30, 50);
    painter.rect_filled(fill, 4.0, col);
    painter.rect_stroke(
        bg,
        4.0,
        Stroke::new(1.5_f32, Color32::from_white_alpha(120)),
        egui::StrokeKind::Inside,
    );
}

/// HUD des collectibles (haut-droite) : « ⭐ ramassés / total », et bannière « Gagné ! »
/// quand tout est ramassé.
#[allow(clippy::too_many_arguments)]
pub(super) fn collectibles_hud(
    ctx: &egui::Context,
    area: egui::Rect,
    collected: usize,
    total: usize,
    time: Option<f32>,
    score: u32,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_collectibles"),
    ));
    let pos = egui::pos2(area.right() - 20.0, area.top() + 18.0);
    painter.text(
        pos,
        Align2::RIGHT_CENTER,
        format!("⭐ {collected} / {total}"),
        FontId::proportional(20.0 * scale),
        Color32::from_rgb(255, 220, 90),
    );
    painter.text(
        egui::pos2(area.right() - 20.0, area.top() + 42.0 * scale),
        Align2::RIGHT_CENTER,
        format!("🏆 {score}"),
        FontId::proportional(16.0 * scale),
        Color32::from_rgb(150, 220, 255),
    );
    if let Some(t) = time {
        painter.text(
            egui::pos2(area.right() - 20.0, area.top() + 64.0 * scale),
            Align2::RIGHT_CENTER,
            format!("⏱ {t:.1}s"),
            FontId::proportional(16.0 * scale),
            Color32::from_white_alpha(200),
        );
    }
    if collected == total && total > 0 {
        let msg = crate::app::locale::won(locale, time);
        painter.text(
            area.center(),
            Align2::CENTER_CENTER,
            msg,
            FontId::proportional(40.0 * scale),
            Color32::from_rgb(120, 230, 140),
        );
    }
}

/// Bannière de défaite « 💀 Perdu ! » au centre de la zone de jeu.
pub(super) fn lose_banner(
    ctx: &egui::Context,
    area: egui::Rect,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_lose"),
    ));
    painter.text(
        area.center(),
        Align2::CENTER_CENTER,
        crate::app::locale::lost(locale),
        FontId::proportional(44.0 * scale),
        Color32::from_rgb(230, 90, 80),
    );
}

/// Bannière « Vaincu » pour un joueur réseau à 0 PV (GAMEDESIGN_EN_LIGNE.md
/// §3.1) — sans retour visuel persistant, un joueur à 0 PV se retrouverait
/// face à un écran figé/vide, indiscernable d'un bug. Distincte de
/// `lose_banner` (`self.lost`, pensé pour un joueur local unique touchant une
/// zone mortelle) : ici la manche **continue** pour les autres, ce n'est pas
/// une défaite de salon — pas de bouton Rejouer, juste l'attente.
pub(super) fn defeated_banner(
    ctx: &egui::Context,
    area: egui::Rect,
    cause: Option<crate::net::protocol::DeathCause>,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_defeated"),
    ));
    painter.text(
        area.center(),
        Align2::CENTER_CENTER,
        crate::app::locale::defeated_spectator(locale),
        FontId::proportional(36.0 * scale),
        Color32::from_rgb(230, 90, 80),
    );
    // Diagnostic de mort (Sprint 2, `sprint10audit.md`, GDD §16.5) : cause
    // résumée entre le titre et le texte d'attente, sur sa propre ligne pour
    // ne pas concurrencer visuellement le titre (36pt) ni le sous-texte (15pt).
    let waiting_y = if let Some(c) = cause {
        painter.text(
            egui::pos2(area.center().x, area.center().y + 34.0 * scale),
            Align2::CENTER_CENTER,
            crate::app::locale::death_cause(locale, c.kind, c.distinct_attackers),
            FontId::proportional(18.0 * scale),
            Color32::from_white_alpha(230),
        );
        area.center().y + 60.0 * scale
    } else {
        area.center().y + 34.0 * scale
    };
    painter.text(
        egui::pos2(area.center().x, waiting_y),
        Align2::CENTER_CENTER,
        crate::app::locale::waiting_next_round(locale),
        FontId::proportional(15.0 * scale),
        Color32::from_white_alpha(200),
    );
}

/// Bannière brève (haut de l'écran, ne recouvre jamais le réticule — GDD
/// §16.3) quand un **autre** joueur réseau tombe à 0 PV : `intensity`
/// (1 = pic) décroît vers 0 côté App (`AppState::ally_down_flash`), même
/// mécanisme que `damage_vignette`. Distincte de `defeated_banner` (qui
/// s'affiche à *nous*, plein écran, tant qu'on est spectateur) : ici c'est un
/// signal ponctuel pour le reste du groupe, pas un état qui dure.
pub(super) fn ally_down_banner(
    ctx: &egui::Context,
    area: egui::Rect,
    intensity: f32,
    locale: crate::app::locale::Locale,
    scale: f32,
    marker: Option<(glam::Mat4, glam::Vec3)>,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_ally_down"),
    ));
    let alpha = (230.0 * intensity.clamp(0.0, 1.0)) as u8;
    painter.text(
        egui::pos2(area.center().x, area.top() + 48.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::ally_down(locale),
        FontId::proportional(22.0 * scale),
        Color32::from_rgba_unmultiplied(230, 90, 80, alpha),
    );
    if let Some((view_proj, world_pos)) = marker
        && let Some(edge) = offscreen_edge_position(area, view_proj * world_pos.extend(1.0))
    {
        let dir = (edge - area.center()).normalized();
        let size = 16.0 * scale;
        let side = egui::vec2(-dir.y, dir.x) * size * 0.6;
        let back = edge - dir * size * 1.6;
        painter.add(egui::Shape::convex_polygon(
            vec![edge, back + side, back - side],
            Color32::from_rgba_unmultiplied(230, 90, 80, alpha),
            egui::Stroke::NONE,
        ));
    }
}

/// Point sur le bord de `area` (marge de 28 px) dans la direction de
/// `clip` (position homogène caméra, `Mat4::project_point`-like avant
/// division par `w`), ou `None` si `clip` tombe déjà à l'écran — plafonné à
/// un seul marqueur (l'appelant ne fournit que l'allié le plus proche, cf.
/// `AppState::nearest_downed_ally_position`), pas un par allié à terre
/// (lisibilité en combat dense, Phase L Sprint 2 risques). Technique
/// standard d'indicateur hors-écran : un point derrière la caméra (`w <=
/// 0`) a un signe de projection inversé, on inverse `x`/`y` pour rester du
/// bon côté visuellement plutôt que de le laisser apparaître à l'opposé.
fn offscreen_edge_position(area: egui::Rect, clip: glam::Vec4) -> Option<egui::Pos2> {
    let (x, y) = if clip.w <= 0.0 {
        (-clip.x, -clip.y)
    } else {
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        if ndc_x.abs() <= 1.0 && ndc_y.abs() <= 1.0 {
            return None;
        }
        (clip.x, clip.y)
    };
    // NDC Y vers le haut, écran egui Y vers le bas.
    let dir = egui::vec2(x, -y);
    if dir.length_sq() < 1e-6 {
        return None;
    }
    let dir = dir.normalized();
    let margin = 28.0;
    let half = egui::vec2(
        (area.width() / 2.0 - margin).max(1.0),
        (area.height() / 2.0 - margin).max(1.0),
    );
    let scale = (half.x / dir.x.abs().max(1e-6)).min(half.y / dir.y.abs().max(1e-6));
    Some(area.center() + dir * scale)
}

/// Menu pause (Phase J, `sprintreflecion.md`) : titre + deux boutons
/// (Reprendre / Redémarrer), affiché uniquement quand `AppState::paused` est
/// vrai (cf. `run_player_overlay`). Sur le modèle de `restart_button` mais
/// avec deux actions distinctes en sortie plutôt qu'un seul bouton.
/// Renvoie `(resume_clicked, restart_clicked)`.
pub(super) fn pause_menu(
    ctx: &egui::Context,
    area: egui::Rect,
    locale: crate::app::locale::Locale,
    scale: f32,
) -> (bool, bool) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_pause_title"),
    ));
    painter.text(
        egui::pos2(area.center().x, area.center().y - 60.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::pause_title(locale),
        FontId::proportional(36.0 * scale),
        Color32::WHITE,
    );
    let mut resume_clicked = false;
    let mut restart_clicked = false;
    let btn_size = [170.0 * scale, 46.0 * scale];
    egui::Area::new("pause_resume_btn".into())
        .fixed_pos(egui::pos2(
            area.center().x - 85.0 * scale,
            area.center().y - 10.0 * scale,
        ))
        .show(ctx, |ui| {
            let label = crate::app::locale::resume_button_label(locale);
            let btn = egui::Button::new(egui::RichText::new(label).size(20.0 * scale));
            if ui.add_sized(btn_size, btn).clicked() {
                resume_clicked = true;
            }
        });
    egui::Area::new("pause_restart_btn".into())
        .fixed_pos(egui::pos2(
            area.center().x - 85.0 * scale,
            area.center().y + 46.0 * scale,
        ))
        .show(ctx, |ui| {
            let label = crate::app::locale::restart_button_label(locale, false);
            let btn = egui::Button::new(egui::RichText::new(label).size(20.0 * scale));
            if ui.add_sized(btn_size, btn).clicked() {
                restart_clicked = true;
            }
        });
    (resume_clicked, restart_clicked)
}

/// Bouton tactile « 🔄 Rejouer » centré sous la bannière de fin de partie.
/// Renvoie `true` s'il est cliqué (pour relancer la partie, y compris sur APK).
pub(super) fn restart_button(
    ctx: &egui::Context,
    area: egui::Rect,
    won: bool,
    locale: crate::app::locale::Locale,
    scale: f32,
) -> bool {
    let scale = clamp_hud_scale(scale);
    let mut clicked = false;
    let label = crate::app::locale::restart_button_label(locale, won);
    egui::Area::new("restart_btn".into())
        .fixed_pos(egui::pos2(
            area.center().x - 85.0 * scale,
            area.center().y + 40.0 * scale,
        ))
        .show(ctx, |ui| {
            let btn = egui::Button::new(egui::RichText::new(label).size(20.0 * scale));
            if ui.add_sized([170.0 * scale, 46.0 * scale], btn).clicked() {
                clicked = true;
            }
        });
    clicked
}

/// Anneau de retour visuel à l'endroit touché (simulation tactile), dans `area`.
pub(super) fn touch_feedback(ctx: &egui::Context, area: egui::Rect) {
    use egui::{Color32, Stroke};
    let down = ctx.input(|i| i.pointer.primary_down());
    if !down {
        return;
    }
    let Some(p) = ctx.pointer_interact_pos() else {
        return;
    };
    if !area.contains(p) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("touch_feedback"),
    ));
    painter.circle_stroke(
        p,
        24.0,
        Stroke::new(3.0_f32, Color32::from_white_alpha(150)),
    );
    painter.circle_filled(p, 7.0, Color32::from_white_alpha(90));
}

/// Dessine les contrôles tactiles (joystick virtuel + boutons) à l'intérieur de
/// `area` et met à jour l'état d'entrée lu par les scripts Lua.
pub(super) fn mobile_overlay(
    ctx: &egui::Context,
    area: egui::Rect,
    cfg: &crate::scene::MobileControls,
    input: &mut crate::app::PlayerInput,
) {
    use egui::{Color32, Sense, Stroke, Vec2};

    input.joy = (0.0, 0.0);
    input.touch_thrust = 0.0;
    input.touch_turn = 0.0;
    input.buttons.clear();

    // Screen Safe Area : rentre les contrôles dans une marge sûre (encoche/bords).
    let area = if cfg.safe_area {
        let inset = (area.width().min(area.height()) * 0.06).min(28.0);
        area.shrink(inset)
    } else {
        area
    };

    let margin = 32.0;

    // --- Zone tactile plein écran : un tap n'importe où expose input.btn.touch ---
    if cfg.touch_zone {
        let down = ctx.input(|i| i.pointer.primary_down());
        if let Some(p) = ctx.pointer_interact_pos()
            && down
            && area.contains(p)
        {
            input.buttons.insert("touch".to_string());
        }
    }

    // --- Pavé « tank » W/A/S/D (bas-gauche), à la place du joystick si activé :
    // mêmes contrôles que le clavier desktop — W/S avance/recule le long de
    // l'orientation *actuelle* du personnage, A/D le fait pivoter. L'ancienne
    // croix directionnelle écrivait `input.joy` (déplacement caméra-relatif),
    // un simple doublon discret du joystick — le pavé tank apporte, lui, le
    // second schéma de contrôle du jeu au tactile.
    if cfg.dpad {
        let btn = 56.0;
        let gap = 6.0;
        let size = Vec2::splat(btn * 3.0 + gap * 2.0);
        let pos = egui::pos2(area.left() + margin, area.bottom() - margin - size.y);
        egui::Area::new("mobile_dpad".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let cell = |col: f32, row: f32| {
                    egui::Rect::from_min_size(
                        rect.min + Vec2::new(col * (btn + gap), row * (btn + gap)),
                        Vec2::splat(btn),
                    )
                };
                // Lettres ASCII plutôt que ▲▼◀▶ : les triangles haut/bas manquent de
                // la fonte embarquée sur Android (rendus en carrés vides).
                let up = ui.put(cell(1.0, 0.0), egui::Button::new("W").corner_radius(10.0));
                let left = ui.put(cell(0.0, 1.0), egui::Button::new("A").corner_radius(10.0));
                let right = ui.put(cell(2.0, 1.0), egui::Button::new("D").corner_radius(10.0));
                let down = ui.put(cell(1.0, 2.0), egui::Button::new("S").corner_radius(10.0));

                let mut thrust = 0.0f32;
                let mut turn = 0.0f32;
                if up.is_pointer_button_down_on() {
                    thrust += 1.0;
                }
                if down.is_pointer_button_down_on() {
                    thrust -= 1.0;
                }
                // Convention « tank » du pavé tactile (`touch_turn`) : A = -1,
                // D = +1 — le clavier de bureau n'utilise plus ce canal depuis le
                // passage au style « action moderne » (cf. `PlayerInput::key_move`),
                // mais le pavé mobile le conserve pour ce contrôle dédié.
                if left.is_pointer_button_down_on() {
                    turn -= 1.0;
                }
                if right.is_pointer_button_down_on() {
                    turn += 1.0;
                }
                // Canaux tactiles dédiés (cf. `PlayerInput::thrust`/`turn`) :
                // réécrits chaque frame (0 au relâchement), cumulés avec le
                // clavier sans jamais écraser son état, tenu par événements.
                input.touch_thrust = thrust;
                input.touch_turn = turn;
            });
    } else if cfg.dual_stick {
        let radius = 55.0;
        // Gauche : avance/recul uniquement (axe vertical, tank thrust) — le
        // pouce ne dévie jamais latéralement le déplacement.
        let left_pos = egui::pos2(area.left() + margin, area.bottom() - margin - radius * 2.0);
        egui::Area::new("mobile_joystick_left".into())
            .fixed_pos(left_pos)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0), Sense::drag());
                let center = rect.center();
                let painter = ui.painter();
                painter.circle_filled(center, radius, Color32::from_black_alpha(110));
                painter.circle_stroke(
                    center,
                    radius,
                    Stroke::new(2.0_f32, Color32::from_white_alpha(120)),
                );
                let mut knob = center;
                if let Some(p) = resp.interact_pointer_pos() {
                    let mut off = p - center;
                    if off.length() > radius {
                        off = off.normalized() * radius;
                    }
                    knob = center + egui::vec2(0.0, off.y); // verrouillé sur l'axe vertical
                    input.touch_thrust = (-off.y / radius).clamp(-1.0, 1.0); // haut = avance
                }
                painter.circle_filled(knob, 22.0, Color32::from_white_alpha(200));
            });
    } else if cfg.joystick {
        let radius = 55.0;
        let pos = egui::pos2(area.left() + margin, area.bottom() - margin - radius * 2.0);
        egui::Area::new("mobile_joystick".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0), Sense::drag());
                let center = rect.center();
                let painter = ui.painter();
                painter.circle_filled(center, radius, Color32::from_black_alpha(110));
                painter.circle_stroke(
                    center,
                    radius,
                    Stroke::new(2.0_f32, Color32::from_white_alpha(120)),
                );
                let mut knob = center;
                if let Some(p) = resp.interact_pointer_pos() {
                    let mut off = p - center;
                    if off.length() > radius {
                        off = off.normalized() * radius;
                    }
                    // Bridé à l'axe vertical (avance/recul) — même principe que le
                    // stick gauche de `dual_stick` : dévier le pouce latéralement ne
                    // doit ni faire pivoter le personnage ni entraîner la caméra
                    // (cf. `camera.yaw` asservi à l'orientation du joueur en mode
                    // arme à distance, ligne ~630). Retour explicite : le stick
                    // gauche sert uniquement à avancer/reculer, jamais à tourner.
                    knob = center + egui::vec2(0.0, off.y);
                    input.joy = (0.0, -off.y / radius); // y inversé : haut = +1
                }
                painter.circle_filled(knob, 22.0, Color32::from_white_alpha(200));
            });
    }

    // --- Boutons (bas-droite de la zone de jeu) ---
    if !cfg.buttons.is_empty() {
        let btn = 64.0;
        let spacing = 8.0;
        // Grille (2 colonnes max) plutôt qu'une seule rangée qui s'allonge avec
        // le nombre de boutons : au-delà de Saut/Attaque (2 boutons), une rangée
        // unique — Saut/Feu/Arme/Soin, 4 boutons — déborde assez à gauche pour
        // chevaucher le pavé tank W/A/S/D sur un téléphone de largeur courante.
        // Une grille qui pousse en hauteur, jamais en largeur, garde une
        // empreinte horizontale fixe (2 colonnes) quel que soit le nombre de
        // boutons.
        const COLS: usize = 2;
        let cols = cfg.buttons.len().min(COLS);
        let rows = cfg.buttons.len().div_ceil(cols);
        let width = cols as f32 * (btn + spacing) - spacing;
        let height = rows as f32 * (btn + spacing) - spacing;
        let pos = egui::pos2(
            area.right() - margin - width,
            area.bottom() - margin - height,
        );
        egui::Area::new("mobile_buttons".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                for (i, name) in cfg.buttons.iter().enumerate() {
                    let (col, row) = (i % cols, i / cols);
                    let cell = egui::Rect::from_min_size(
                        rect.min
                            + Vec2::new(col as f32 * (btn + spacing), row as f32 * (btn + spacing)),
                        Vec2::splat(btn),
                    );
                    let resp = ui.put(cell, egui::Button::new(name).corner_radius(32.0));
                    // Bouton « maintenu » : actif tant que le pointeur est enfoncé dessus.
                    if resp.is_pointer_button_down_on() {
                        input.buttons.insert(name.clone());
                    }
                }
            });
    }
}

/// Écran de fin de manche détaillé (Phase H, Sprint 1, GDD §9.2/§17.4) :
/// titre Gagné/Perdu, une ligne par joueur (`AppState::round_summary`, reçu
/// via `GameEvent::Win`/`Lose`), et la bannière de Contrat du jour rempli
/// (Sprint 2) si `contract_label` est renseigné. Distinct de `lose_banner`
/// (texte fixe, sans détail par joueur) : celle-ci reste utilisée par les
/// démos solo sans salon réseau (`AppState::round_summary` vide dans ce cas,
/// `editor::mod` n'appelle donc cette fonction qu'en multijoueur).
pub(super) fn round_summary_banner(
    ctx: &egui::Context,
    area: egui::Rect,
    won: bool,
    summary: &[crate::net::protocol::RoundPlayerSummary],
    contract_label: Option<&str>,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_round_summary"),
    ));
    let title_color = if won {
        Color32::from_rgb(120, 230, 140)
    } else {
        Color32::from_rgb(230, 90, 80)
    };
    painter.text(
        egui::pos2(area.center().x, area.top() + area.height() * 0.28),
        Align2::CENTER_CENTER,
        crate::app::locale::round_outcome_title(locale, won),
        FontId::proportional(40.0 * scale),
        title_color,
    );
    let mut y = area.top() + area.height() * 0.28 + 46.0 * scale;
    for player in summary {
        painter.text(
            egui::pos2(area.center().x, y),
            Align2::CENTER_CENTER,
            crate::app::locale::round_summary_row(
                locale,
                &player.name,
                player.frags,
                player.assists,
                player.xp,
            ),
            FontId::proportional(18.0 * scale),
            Color32::from_white_alpha(230),
        );
        y += 26.0 * scale;
    }
    if let Some(label) = contract_label {
        painter.text(
            egui::pos2(area.center().x, y + 14.0 * scale),
            Align2::CENTER_CENTER,
            crate::app::locale::contract_completed(locale, label),
            FontId::proportional(18.0 * scale),
            Color32::from_rgb(255, 220, 90),
        );
    }
}

/// Bannière brève (haut de l'écran, sous le HUD de vie/vagues — GDD §17.2)
/// annonçant une nouvelle vague, déclenchée par `GameEvent::WaveStart`.
/// Même mécanisme de décroissance que `ally_down_banner` (`intensity` piloté
/// par `AppState::wave_banner_flash`).
pub(super) fn wave_start_banner(
    ctx: &egui::Context,
    area: egui::Rect,
    wave: u32,
    intensity: f32,
    locale: crate::app::locale::Locale,
    scale: f32,
) {
    use egui::{Align2, Color32, FontId};
    let scale = clamp_hud_scale(scale);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_wave_start"),
    ));
    let alpha = (230.0 * intensity.clamp(0.0, 1.0)) as u8;
    painter.text(
        egui::pos2(area.center().x, area.top() + 96.0 * scale),
        Align2::CENTER_CENTER,
        crate::app::locale::wave_start_banner(locale, wave),
        FontId::proportional(28.0 * scale),
        Color32::from_rgba_unmultiplied(255, 220, 90, alpha),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PHASE I Sprint 2 (accessibilité, colorblind) : le pourcentage doit
    /// rester lisible et cohérent aux trois bornes de teinte de la mini barre
    /// du roster (vert >50 %, jaune 25-50 %, rouge <=25 %) — la valeur
    /// numérique est le repère non-couleur, elle ne doit jamais dépendre de
    /// laquelle de ces trois bandes `h` tombe dedans.
    #[test]
    fn health_percent_label_matches_the_three_color_tiers() {
        assert_eq!(health_percent_label(1.0), "100%");
        assert_eq!(health_percent_label(0.8), "80%");
        assert_eq!(health_percent_label(0.5), "50%");
        assert_eq!(health_percent_label(0.3), "30%");
        assert_eq!(health_percent_label(0.1), "10%");
        assert_eq!(health_percent_label(0.0), "0%");
    }

    /// `clamp_hud_scale` protège les fonctions de dessin du HUD contre une
    /// valeur `Settings::hud_scale` hors de la plage `0.6..=2.0` du curseur
    /// des Paramètres (fichier édité à la main, ancienne valeur invalide...).
    #[test]
    fn clamp_hud_scale_bounds_extreme_values() {
        assert_eq!(clamp_hud_scale(1.0), 1.0);
        assert_eq!(clamp_hud_scale(0.0), 0.5);
        assert_eq!(clamp_hud_scale(100.0), 3.0);
    }

    /// Marqueur allié hors-écran (Phase L Sprint 2) : un allié déjà visible
    /// (au centre du frustum) ne doit produire aucun marqueur — la bannière
    /// texte seule suffit, cf. les risques listés dans
    /// `sprint2audijeu0718.md` (ne pas surcharger l'écran).
    #[test]
    fn offscreen_edge_position_is_none_when_the_point_is_on_screen() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let clip = glam::Vec4::new(0.2, 0.1, 0.5, 1.0);
        assert!(offscreen_edge_position(area, clip).is_none());
    }

    /// Un allié loin sur la droite (hors du frustum, `w > 0`) doit produire
    /// un marqueur sur le bord droit de `area`, pas au centre ni sur un bord
    /// vertical opposé.
    #[test]
    fn offscreen_edge_position_clamps_to_the_right_edge_when_far_to_the_right() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let clip = glam::Vec4::new(5.0, 0.0, 0.5, 1.0);
        let pos = offscreen_edge_position(area, clip).expect("hors écran attendu");
        // Bord droit moins la marge de 28 px (cf. `offscreen_edge_position`) : le
        // marqueur reste inséré dans `area`, pas collé au tout dernier pixel.
        assert!((pos.x - (area.right() - 28.0)).abs() < 1.0);
        assert!((pos.y - area.center().y).abs() < 1.0);
    }

    /// Un allié derrière la caméra (`w <= 0`) doit rester du bon côté visuel
    /// (le signe de `x`/`y` s'inverse à la division par `w` négatif) —
    /// technique standard d'indicateur hors-écran, cf. la doc de la fonction.
    #[test]
    fn offscreen_edge_position_flips_points_behind_the_camera() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        // Un point dont la coordonnée homogène `x` est positive mais `w`
        // négatif (derrière la caméra) doit apparaître à GAUCHE, pas à
        // droite comme le signe brut de `x` le suggérerait.
        let clip = glam::Vec4::new(5.0, 0.0, 0.5, -1.0);
        let pos = offscreen_edge_position(area, clip).expect("hors écran attendu");
        assert!((pos.x - (area.left() + 28.0)).abs() < 1.0);
    }
}
