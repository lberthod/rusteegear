//! Table unique des raccourcis (roadmap post-audit UX 2026-09-04, 4.1).
//!
//! Avant : cinq inventaires divergents (README, QUICKSTART, FIRST_GAME,
//! fenêtre « Raccourcis clavier » à 8 entrées, code réel à ~25). Cette table
//! alimente la fenêtre **Aide › ⌨ Raccourcis clavier** de l'éditeur et un test
//! vérifie que [docs/CONTROLS.md](../../docs/CONTROLS.md) cite chaque entrée —
//! ajouter un raccourci ici sans le documenter fait échouer `cargo test`.
//!
//! Les touches sont écrites telles qu'elles apparaissent dans CONTROLS.md
//! (`Cmd` pour Mac ; `Ctrl` ailleurs, cf. `lib.rs` qui accepte les deux).

/// Où le raccourci s'applique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Éditeur hors Play.
    Editor,
    /// Souris dans l'éditeur.
    Mouse,
    /// En jeu (Play dans l'éditeur, mode Player, web).
    Play,
    /// Tactile (mobile, web sur écran tactile).
    Touch,
}

impl Scope {
    pub fn title(self) -> &'static str {
        match self {
            Scope::Editor => "Éditeur",
            Scope::Mouse => "Souris (éditeur)",
            Scope::Play => "Jeu",
            Scope::Touch => "Tactile",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shortcut {
    /// Touche(s), format Markdown de CONTROLS.md (backticks compris).
    pub keys: &'static str,
    pub action: &'static str,
    pub scope: Scope,
}

const fn sc(scope: Scope, keys: &'static str, action: &'static str) -> Shortcut {
    Shortcut {
        keys,
        action,
        scope,
    }
}

/// La table, dans l'ordre d'affichage.
pub const SHORTCUTS: &[Shortcut] = &[
    sc(Scope::Editor, "`Q`", "Outil Main (déplacer la vue)"),
    sc(Scope::Editor, "`W`", "Outil Déplacer (gizmo)"),
    sc(Scope::Editor, "`E`", "Outil Tourner (gizmo)"),
    sc(Scope::Editor, "`R`", "Outil Échelle (gizmo)"),
    sc(Scope::Editor, "`T`", "Outil Orbite (caméra)"),
    sc(Scope::Editor, "`Y`", "Outil Loupe (zoom)"),
    sc(Scope::Editor, "`F`", "Cadrer la sélection"),
    sc(
        Scope::Editor,
        "`G`",
        "Caméra libre (vol) — flèches + Espace/C",
    ),
    sc(Scope::Editor, "`Cmd+Z` / `Cmd+Maj+Z`", "Annuler / Rétablir"),
    sc(Scope::Editor, "`Cmd+D`", "Dupliquer la sélection"),
    sc(
        Scope::Editor,
        "`Cmd+C` / `Cmd+X` / `Cmd+V`",
        "Copier / Couper / Coller",
    ),
    sc(Scope::Editor, "`Cmd+A`", "Tout sélectionner"),
    sc(
        Scope::Editor,
        "`Suppr` / `Retour arrière`",
        "Supprimer la sélection",
    ),
    sc(
        Scope::Editor,
        "`Cmd+S`",
        "Enregistrer (scène du projet ouvert)",
    ),
    sc(Scope::Editor, "`Cmd+Maj+S`", "Enregistrer sous…"),
    sc(Scope::Editor, "`Cmd+O`", "Ouvrir une scène ou un projet…"),
    sc(Scope::Editor, "`Cmd+N`", "Nouveau projet…"),
    sc(
        Scope::Mouse,
        "Clic gauche",
        "Sélectionner ; `Cmd`/`Maj` + clic : sélection additive",
    ),
    sc(
        Scope::Mouse,
        "Clic gauche + glisser",
        "Tourner la caméra ; sur une poignée : gizmo",
    ),
    sc(
        Scope::Mouse,
        "Clic milieu + glisser, ou `Maj` + glisser",
        "Déplacer la vue (pan)",
    ),
    sc(Scope::Mouse, "Molette", "Zoom"),
    sc(
        Scope::Mouse,
        "`Ctrl` pendant un glissé de gizmo",
        "Inverser l'aimantation (snap)",
    ),
    sc(Scope::Mouse, "Double-clic dans la hiérarchie", "Renommer"),
    sc(
        Scope::Play,
        "`W A S D` ou flèches",
        "Se déplacer (relatif à la caméra)",
    ),
    sc(Scope::Play, "`Espace`", "Sauter"),
    sc(Scope::Play, "`J`", "Attaque de mêlée"),
    sc(Scope::Play, "`K`", "Tirer (arme à distance)"),
    sc(Scope::Play, "`H`", "Soigner l'allié blessé le plus proche"),
    sc(Scope::Play, "`1` `2` `3`", "Choisir l'arme"),
    sc(Scope::Play, "`Échap`", "Pause"),
    sc(Scope::Play, "`M`", "Carte plein écran"),
    sc(Scope::Play, "`Tab`", "Paramètres (mode Player)"),
    sc(Scope::Touch, "Stick gauche", "Se déplacer (deux axes)"),
    sc(
        Scope::Touch,
        "Glisser sur la moitié droite de l'écran",
        "Tourner la caméra",
    ),
    sc(Scope::Touch, "⏸ (haut-droite)", "Pause"),
    sc(Scope::Touch, "Carte (haut-droite)", "Carte plein écran"),
];

/// Les raccourcis d'une portée, dans l'ordre de la table.
pub fn by_scope(scope: Scope) -> impl Iterator<Item = &'static Shortcut> {
    SHORTCUTS.iter().filter(move |s| s.scope == scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROLS_MD: &str = include_str!("../../docs/CONTROLS.md");

    /// La doc cite chaque raccourci de la table (cf. en-tête du module).
    #[test]
    fn docs_controls_lists_every_shortcut() {
        let missing: Vec<&str> = SHORTCUTS
            .iter()
            .filter(|s| !CONTROLS_MD.contains(s.keys))
            .map(|s| s.keys)
            .collect();
        assert!(
            missing.is_empty(),
            "docs/CONTROLS.md ne cite pas : {missing:?} — ajouter la ligne au tableau"
        );
    }

    #[test]
    fn every_scope_has_entries() {
        for scope in [Scope::Editor, Scope::Mouse, Scope::Play, Scope::Touch] {
            assert!(by_scope(scope).next().is_some(), "{scope:?} vide");
        }
    }
}
