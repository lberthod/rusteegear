//! Tactile réel (roadmap post-audit UX v2 2026-09-04, vague 5) : géométrie des
//! zones de jeu, stick flottant, rôle de chaque doigt et marges sûres — pur
//! calcul, sans egui ni winit, pour être testable.
//!
//! Deux consommateurs :
//! - `lib.rs` (`App::handle_player_touch`) suit **chaque doigt** par son
//!   identifiant winit et lui donne un rôle selon la zone où il s'est posé
//!   (`TouchZones::role_at`) — egui, lui, ne suit qu'un pointeur, donc stick +
//!   orbite ou stick + Feu simultanés passent forcément par ici (5.1) ;
//! - `editor::hud::mobile_overlay` dessine les contrôles depuis le même
//!   état (`PlayerInput::touch_stick`, `PlayerInput::buttons`) et publie les
//!   zones de la frame (`PlayerInput::touch_zones`) pour l'événement suivant.
//!
//! Coordonnées : points egui (physiques ÷ `pixels_per_point`), comme les
//! rectangles du HUD.

use egui::{Pos2, Rect, Vec2, pos2, vec2};

/// Zone morte du stick (5.2) : fraction du rayon sous laquelle le stick ne
/// produit rien — un pouce posé n'est jamais parfaitement immobile.
pub const STICK_DEAD_ZONE: f32 = 0.12;

/// Marge de repli (sans insets système connus) : 6 % du petit côté, au plus
/// 28 pt — l'ancienne règle de `mobile_overlay`, désormais appliquée à tout le
/// HUD (5.4).
pub const FALLBACK_INSET_RATIO: f32 = 0.06;
pub const FALLBACK_INSET_MAX: f32 = 28.0;

/// Rayon du stick (5.2) : 9 % du petit côté de l'écran, borné à 44..90 pt (un
/// pouce couvre ~44 pt, au-delà de 90 le geste devient un bras de levier),
/// puis mis à l'échelle du HUD.
pub fn stick_radius(short_side: f32, hud_scale: f32) -> f32 {
    (short_side * 0.09).clamp(44.0, 90.0) * hud_scale.clamp(0.5, 3.0)
}

/// Vecteur du stick dans [-1, 1]² depuis le centre (`origin`, là où le doigt
/// s'est posé) et la position courante du doigt (5.2) : rien sous la zone
/// morte, linéaire au-delà jusqu'au rayon, saturé à 1 au-delà — le doigt garde
/// le contrôle en sortant du cercle, la direction reste celle du doigt. `y`
/// est inversé (haut de l'écran = +1) pour rester le canal `PlayerInput::joy`
/// des flèches/WASD.
pub fn stick_vector(origin: Pos2, pos: Pos2, radius: f32) -> (f32, f32) {
    let off = pos - origin;
    let len = off.length();
    let radius = radius.max(1.0);
    let dead = STICK_DEAD_ZONE * radius;
    if len <= dead || len == 0.0 {
        return (0.0, 0.0);
    }
    let magnitude = ((len - dead) / (radius - dead)).min(1.0);
    let dir = off / len;
    (dir.x * magnitude, -dir.y * magnitude)
}

/// Position du pommeau dessiné : la position du doigt, ramenée sur le cercle
/// s'il en est sorti (le vecteur, lui, reste saturé — cf. `stick_vector`).
pub fn stick_knob(origin: Pos2, pos: Pos2, radius: f32) -> Pos2 {
    let off = pos - origin;
    if off.length() > radius {
        origin + off.normalized() * radius
    } else {
        pos
    }
}

/// Stick flottant en cours : centre posé au premier contact, doigt courant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchStick {
    pub origin: Pos2,
    pub pos: Pos2,
}

/// Touche du pavé « tank » W/A/S/D (`MobileControls::dpad`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKey {
    Up,
    Down,
    Left,
    Right,
}

impl PadKey {
    pub fn label(self) -> &'static str {
        match self {
            PadKey::Up => "W",
            PadKey::Down => "S",
            PadKey::Left => "A",
            PadKey::Right => "D",
        }
    }
}

/// Rôle d'un doigt posé, décidé une fois à son contact et gardé jusqu'au
/// relâchement (5.1) : un doigt qui a commencé sur le stick reste le stick
/// même s'il glisse sur la zone d'orbite.
#[derive(Clone, Debug, PartialEq)]
pub enum TouchRole {
    Stick,
    Orbit,
    Button(String),
    Pad(PadKey),
    /// Hors des zones de jeu (ou sur une fenêtre egui) : laissé à egui.
    None,
}

/// Zones de la frame courante, en points, publiées par le HUD
/// (`PlayerInput::touch_zones`) pour que `lib.rs` attribue les rôles. Elles
/// dépendent de la zone de jeu (aperçu mobile, marges sûres) et du `hud_scale`,
/// que seul le HUD connaît.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchZones {
    /// Zone de jeu (marges sûres déjà retirées) — un tap n'importe où dedans
    /// vaut `input.btn.touch` si `touch_zone` est actif.
    pub area: Rect,
    /// Rayon du stick de cette frame (cf. `stick_radius`).
    pub stick_radius: f32,
    /// Zone où poser le doigt fait apparaître le stick flottant (moitié gauche,
    /// sous la barre de vie / mini-carte).
    pub stick: Option<Rect>,
    /// Moitié droite : glisser tourne la caméra.
    pub orbit: Option<Rect>,
    /// Boutons d'action nommés (grille bas-droite), rectangle de chaque cellule.
    pub buttons: Vec<(String, Rect)>,
    /// Cellules du pavé W/A/S/D.
    pub pad: Vec<(PadKey, Rect)>,
    /// `MobileControls::touch_zone`.
    pub touch_zone: bool,
}

impl Default for TouchZones {
    fn default() -> Self {
        TouchZones {
            area: Rect::ZERO,
            stick_radius: 0.0,
            stick: None,
            orbit: None,
            buttons: Vec::new(),
            pad: Vec::new(),
            touch_zone: false,
        }
    }
}

/// Marge entre les contrôles et le bord de la zone de jeu (points).
pub const CONTROL_MARGIN: f32 = 32.0;
/// Bouton d'action (points) : 64 ≥ 44 pt (5.3).
pub const ACTION_BUTTON: f32 = 64.0;
pub const ACTION_SPACING: f32 = 8.0;
/// Cellule du pavé W/A/S/D (points).
pub const PAD_BUTTON: f32 = 56.0;
pub const PAD_GAP: f32 = 6.0;
/// Colonnes de la grille d'action : pousse en hauteur, jamais en largeur (un
/// téléphone de largeur courante n'a pas la place pour 4 boutons en ligne à
/// côté du pavé).
pub const ACTION_COLS: usize = 2;

impl TouchZones {
    /// Calcule les zones depuis la zone de jeu (marges sûres déjà retirées) et
    /// la configuration de la scène — même géométrie que le dessin du HUD.
    pub fn layout(area: Rect, cfg: &crate::scene::MobileControls, hud_scale: f32) -> Self {
        let short_side = area.width().min(area.height());
        let radius = stick_radius(short_side, hud_scale);
        let m = CONTROL_MARGIN;
        let mut zones = TouchZones {
            area,
            stick_radius: radius,
            touch_zone: cfg.touch_zone,
            ..Default::default()
        };
        if cfg.dpad {
            let size = PAD_BUTTON * 3.0 + PAD_GAP * 2.0;
            let min = pos2(area.left() + m, area.bottom() - m - size);
            let cell = |col: f32, row: f32| {
                Rect::from_min_size(
                    min + vec2(col * (PAD_BUTTON + PAD_GAP), row * (PAD_BUTTON + PAD_GAP)),
                    Vec2::splat(PAD_BUTTON),
                )
            };
            zones.pad = vec![
                (PadKey::Up, cell(1.0, 0.0)),
                (PadKey::Left, cell(0.0, 1.0)),
                (PadKey::Right, cell(2.0, 1.0)),
                (PadKey::Down, cell(1.0, 2.0)),
            ];
        } else if cfg.dual_stick || cfg.joystick {
            // Sous le tiers haut (barre de vie, mini-carte, bannières), moitié
            // gauche : le stick apparaît là où le pouce se pose.
            zones.stick = Some(Rect::from_min_max(
                pos2(area.left(), area.top() + area.height() * 0.35),
                pos2(area.center().x, area.bottom()),
            ));
        }
        if cfg.dpad || cfg.dual_stick || cfg.joystick {
            zones.orbit = Some(Rect::from_min_max(
                pos2(area.center().x, area.top()),
                area.max,
            ));
        }
        if !cfg.buttons.is_empty() {
            let cols = cfg.buttons.len().min(ACTION_COLS);
            let rows = cfg.buttons.len().div_ceil(cols);
            let step = ACTION_BUTTON + ACTION_SPACING;
            let width = cols as f32 * step - ACTION_SPACING;
            let height = rows as f32 * step - ACTION_SPACING;
            let min = pos2(area.right() - m - width, area.bottom() - m - height);
            zones.buttons = cfg
                .buttons
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let (col, row) = (i % cols, i / cols);
                    let cell = Rect::from_min_size(
                        min + vec2(col as f32 * step, row as f32 * step),
                        Vec2::splat(ACTION_BUTTON),
                    );
                    (name.clone(), cell)
                })
                .collect();
        }
        zones
    }

    /// Rectangle englobant de la grille d'action (`None` sans bouton).
    pub fn buttons_rect(&self) -> Option<Rect> {
        self.buttons
            .iter()
            .map(|(_, r)| *r)
            .reduce(|a, b| a.union(b))
    }

    /// Rôle d'un doigt qui se pose en `p` : boutons et pavé d'abord (petites
    /// cibles au-dessus des grandes zones), puis stick, puis orbite.
    pub fn role_at(&self, p: Pos2) -> TouchRole {
        if let Some((name, _)) = self.buttons.iter().find(|(_, r)| r.contains(p)) {
            return TouchRole::Button(name.clone());
        }
        if let Some((key, _)) = self.pad.iter().find(|(_, r)| r.contains(p)) {
            return TouchRole::Pad(*key);
        }
        if self.stick.is_some_and(|r| r.contains(p)) {
            return TouchRole::Stick;
        }
        if self.orbit.is_some_and(|r| r.contains(p)) {
            return TouchRole::Orbit;
        }
        TouchRole::None
    }
}

/// Marges sûres finales (points, `[haut, droite, bas, gauche]`) : les insets
/// **système** (encoche, barre d'état, indicateur d'accueil) sont un plancher ;
/// si la scène demande une marge (`MobileControls::safe_area`, `fallback`), la
/// marge de repli historique s'ajoute par côté au maximum des deux — un écran
/// sans insets connus (Android sans `WindowInsets`, aperçu desktop) garde
/// ainsi la marge d'avant.
pub fn safe_insets(area: Rect, system: Option<[f32; 4]>, fallback: bool) -> [f32; 4] {
    let fb = if fallback {
        (area.width().min(area.height()) * FALLBACK_INSET_RATIO).min(FALLBACK_INSET_MAX)
    } else {
        0.0
    };
    let sys = system.unwrap_or([0.0; 4]);
    let mut out = [0.0; 4];
    for (o, s) in out.iter_mut().zip(sys) {
        *o = s.max(0.0).max(fb);
    }
    out
}

/// Retire des marges `[haut, droite, bas, gauche]` à `area` sans jamais
/// l'inverser (un écran minuscule garde au moins un point de large).
pub fn inset_rect(area: Rect, insets: [f32; 4]) -> Rect {
    let [top, right, bottom, left] = insets;
    let min = pos2(area.left() + left, area.top() + top);
    let max = pos2(
        (area.right() - right).max(min.x + 1.0),
        (area.bottom() - bottom).max(min.y + 1.0),
    );
    Rect::from_min_max(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::MobileControls;

    fn cfg_dual(buttons: &[&str]) -> MobileControls {
        MobileControls {
            dual_stick: true,
            buttons: buttons.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn stick_radius_is_nine_percent_of_the_short_side_clamped_and_scaled() {
        assert!((stick_radius(700.0, 1.0) - 63.0).abs() < 1e-3);
        assert_eq!(stick_radius(300.0, 1.0), 44.0, "borne basse 44 pt");
        assert_eq!(stick_radius(2000.0, 1.0), 90.0, "borne haute 90 pt");
        assert!(
            (stick_radius(700.0, 2.0) - 126.0).abs() < 1e-3,
            "mis à l'échelle du HUD"
        );
    }

    #[test]
    fn stick_vector_is_zero_inside_the_dead_zone() {
        let o = pos2(100.0, 100.0);
        assert_eq!(stick_vector(o, o, 50.0), (0.0, 0.0));
        // 12 % de 50 = 6 pt : à 5 pt, encore dans la zone morte.
        assert_eq!(stick_vector(o, pos2(105.0, 100.0), 50.0), (0.0, 0.0));
    }

    #[test]
    fn stick_vector_is_linear_above_the_dead_zone_and_inverts_y() {
        let o = pos2(100.0, 100.0);
        // Mi-chemin entre la zone morte (6) et le rayon (50) : 28 pt.
        let (x, y) = stick_vector(o, pos2(128.0, 100.0), 50.0);
        assert!((x - 0.5).abs() < 1e-5, "{x}");
        assert_eq!(y, 0.0);
        // Doigt vers le haut de l'écran (y plus petit) → +1.
        let (x, y) = stick_vector(o, pos2(100.0, 50.0), 50.0);
        assert!(x.abs() < 1e-6);
        assert!((y - 1.0).abs() < 1e-5, "{y}");
    }

    #[test]
    fn stick_vector_keeps_control_outside_the_circle() {
        let o = pos2(100.0, 100.0);
        // Doigt à 3 rayons : saturé à 1, direction conservée, jamais lâché.
        let (x, y) = stick_vector(o, pos2(250.0, 100.0), 50.0);
        assert!((x - 1.0).abs() < 1e-5, "{x}");
        assert_eq!(y, 0.0);
        let knob = stick_knob(o, pos2(250.0, 100.0), 50.0);
        assert!(
            (knob.x - 150.0).abs() < 1e-4,
            "pommeau ramené sur le cercle"
        );
    }

    #[test]
    fn roles_follow_the_zones() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let z = TouchZones::layout(area, &cfg_dual(&["Saut", "Feu"]), 1.0);
        assert_eq!(z.role_at(pos2(100.0, 300.0)), TouchRole::Stick);
        assert_eq!(z.role_at(pos2(600.0, 100.0)), TouchRole::Orbit);
        // Haut-gauche : barre de vie / mini-carte, laissé à egui.
        assert_eq!(z.role_at(pos2(100.0, 50.0)), TouchRole::None);
        // Grille bas-droite : 2 colonnes, cellule de 64.
        let feu = z.buttons.iter().find(|(n, _)| n == "Feu").map(|(_, r)| *r);
        let feu = feu.expect("bouton Feu");
        assert_eq!(z.role_at(feu.center()), TouchRole::Button("Feu".into()));
        assert!(feu.right() <= area.right() - CONTROL_MARGIN + 1e-3);
        assert!(feu.bottom() <= area.bottom() - CONTROL_MARGIN + 1e-3);
        // Le bouton a priorité sur la zone d'orbite qu'il recouvre.
        assert_ne!(z.role_at(feu.center()), TouchRole::Orbit);
    }

    #[test]
    fn dpad_cells_take_the_stick_corner() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let cfg = MobileControls {
            dpad: true,
            ..Default::default()
        };
        let z = TouchZones::layout(area, &cfg, 1.0);
        assert!(z.stick.is_none());
        assert_eq!(z.pad.len(), 4);
        let up = z
            .pad
            .iter()
            .find(|(k, _)| *k == PadKey::Up)
            .map(|(_, r)| *r);
        assert_eq!(
            z.role_at(up.expect("W").center()),
            TouchRole::Pad(PadKey::Up)
        );
        // Le trou central du pavé n'est aucune touche.
        let center = pos2(
            area.left() + CONTROL_MARGIN + PAD_BUTTON * 1.5 + PAD_GAP,
            area.bottom() - CONTROL_MARGIN - PAD_BUTTON * 1.5 - PAD_GAP,
        );
        assert_eq!(z.role_at(center), TouchRole::None);
    }

    #[test]
    fn no_controls_means_no_gameplay_zone() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let z = TouchZones::layout(area, &MobileControls::default(), 1.0);
        assert_eq!(z.role_at(pos2(600.0, 300.0)), TouchRole::None);
        assert!(z.buttons_rect().is_none());
    }

    #[test]
    fn safe_insets_take_the_max_of_system_and_fallback_per_side() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        // Repli seul : 6 % de 400 = 24 pt partout.
        assert_eq!(safe_insets(area, None, true), [24.0; 4]);
        assert_eq!(safe_insets(area, None, false), [0.0; 4]);
        // Système seul (encoche en haut, indicateur en bas).
        assert_eq!(
            safe_insets(area, Some([47.0, 0.0, 34.0, 0.0]), false),
            [47.0, 0.0, 34.0, 0.0]
        );
        // Les deux : plancher système, repli sur les côtés sans inset.
        assert_eq!(
            safe_insets(area, Some([47.0, 0.0, 10.0, 0.0]), true),
            [47.0, 24.0, 24.0, 24.0]
        );
        // Une valeur négative (jamais attendue) ne dilate pas la zone.
        assert_eq!(safe_insets(area, Some([-5.0; 4]), false), [0.0; 4]);
    }

    #[test]
    fn fallback_inset_is_capped_at_28_points() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(2000.0, 1200.0));
        assert_eq!(safe_insets(area, None, true), [28.0; 4]);
    }

    #[test]
    fn inset_rect_shrinks_each_side_and_never_inverts() {
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 400.0));
        let r = inset_rect(area, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(r.min, pos2(14.0, 21.0));
        assert_eq!(r.max, pos2(808.0, 417.0));
        let tiny = inset_rect(area, [1000.0; 4]);
        assert!(tiny.width() >= 1.0 && tiny.height() >= 1.0);
    }
}
