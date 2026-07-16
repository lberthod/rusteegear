//! Types de données du HUD déclaratif (`Scene::hud_widgets`, Sprint 109) et des
//! décalages d'overlays historiques (`Scene::hud_layout`) — extrait de `scene/mod.rs`
//! (Sprint 113a) : pure donnée, aucune méthode ici ne touche `Scene` elle-même
//! (le rendu vit dans `editor::hud`, hors du crate `scene`).

use serde::{Deserialize, Serialize};

/// Point d'ancrage d'un `HudWidget` dans la zone de jeu (`play_rect`) : le widget
/// est positionné relativement à ce coin, `offset` s'ajoutant dans le sens qui
/// l'éloigne du bord (ex. `TopRight` + `offset [-10, 10]` reste à l'intérieur).
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Debug)]
pub enum HudAnchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl HudAnchor {
    /// Position du point d'ancrage en fraction de la zone de jeu (0..1 sur x et y).
    pub fn fraction(self) -> (f32, f32) {
        match self {
            HudAnchor::TopLeft => (0.0, 0.0),
            HudAnchor::TopRight => (1.0, 0.0),
            HudAnchor::BottomLeft => (0.0, 1.0),
            HudAnchor::BottomRight => (1.0, 1.0),
            HudAnchor::Center => (0.5, 0.5),
        }
    }
}

/// Valeur de jeu à laquelle lier le contenu d'un widget (texte formaté, remplissage
/// d'une jauge). `None` = contenu statique, aucune liaison.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Debug)]
pub enum HudBinding {
    #[default]
    None,
    Health,
    Score,
    Kills,
    Wave,
}

/// Contenu d'un `HudWidget`. Les 4 natures couvertes par le Sprint 109 —
/// texte, image, jauge, bouton — en plus de l'ancrage (`HudAnchor`) commun à tous.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum HudWidgetKind {
    /// `content` est affiché tel quel si `binding` vaut `None`, sinon la valeur
    /// liée est ajoutée après un espace (ex. `content = "Score"`, `binding = Score`
    /// → « Score 42 »).
    Text {
        content: String,
        binding: HudBinding,
    },
    /// Image chargée depuis un chemin d'asset (`assets::read_bytes`), mise en
    /// cache par le renderer — cf. `editor::hud::hud_widgets`.
    Image { path: String },
    /// Barre de progression : `binding.value() / max` (borné à [0, 1]).
    Gauge {
        binding: HudBinding,
        max: f32,
        color: [f32; 3],
    },
    /// Bouton cliquable : pousse l'événement de gameplay `hud:<action>` (lisible
    /// en Lua via `on_event`), même mécanisme que `emit()` — cf. `AppState::push_hud_event`.
    Button { label: String, action: String },
}

impl Default for HudWidgetKind {
    fn default() -> Self {
        HudWidgetKind::Text {
            content: String::new(),
            binding: HudBinding::None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Debug)]
#[serde(default)]
pub struct HudWidget {
    /// Identifiant stable (clé egui + cache image) : à choisir unique dans la scène,
    /// pas régénéré automatiquement (une scène éditée à la main doit pouvoir en
    /// fixer un lisible).
    pub id: String,
    pub anchor: HudAnchor,
    /// Décalage en pixels par rapport au point d'ancrage (cf. `HudAnchor::fraction`).
    pub offset: [f32; 2],
    /// Taille en pixels (image, jauge, bouton — ignorée pour le texte, qui se
    /// dimensionne à son contenu).
    pub size: [f32; 2],
    pub kind: HudWidgetKind,
}

/// Cf. `Scene::hud_layout`. Chaque champ est un décalage `[x, y]` en pixels par
/// rapport à la position par défaut de l'élément — `[0.0, 0.0]` (le défaut) donne
/// exactement le placement d'origine, donc les scènes existantes ne changent pas.
#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct HudLayout {
    pub crosshair: [f32; 2],
    pub weapon_hud: [f32; 2],
    pub kills: [f32; 2],
    pub weapon_inventory: [f32; 2],
    pub item_inventory: [f32; 2],
    pub roster: [f32; 2],
}
