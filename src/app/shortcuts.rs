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
    // Roadmap post-audit UX v2 2026-09-04, 4.3.
    sc(Scope::Editor, "`Cmd+P`", "Play / Stop"),
    sc(
        Scope::Editor,
        "`Cmd+Q`",
        "Quitter (confirmation si modifications non enregistrées)",
    ),
    sc(Scope::Editor, "`F1`", "Fenêtre Raccourcis clavier"),
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
    sc(
        Scope::Mouse,
        "Clic droit",
        "Menu contextuel : cadrer, dupliquer, supprimer, ajouter",
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
    // Roadmap post-audit UX v2 2026-09-04, 1.6 : Tab ouvrait les Paramètres
    // (et egui la consommait) ; Paramètres passent par le menu pause.
    sc(Scope::Play, "`Tab`", "Classement (maintenu, mode Player)"),
    sc(Scope::Play, "`F1`", "Aide en jeu"),
    sc(Scope::Touch, "Stick gauche", "Se déplacer (deux axes)"),
    sc(
        Scope::Touch,
        "Glisser sur la moitié droite de l'écran",
        "Tourner la caméra",
    ),
    sc(Scope::Touch, "Saut (vaincu)", "Allié spectateur suivant"),
    sc(Scope::Touch, "⏸ (haut-droite)", "Pause"),
    sc(Scope::Touch, "Carte (haut-droite)", "Carte plein écran"),
    sc(Scope::Touch, "? (haut-droite)", "Aide en jeu"),
];

/// Les raccourcis d'une portée, dans l'ordre de la table.
pub fn by_scope(scope: Scope) -> impl Iterator<Item = &'static Shortcut> {
    SHORTCUTS.iter().filter(move |s| s.scope == scope)
}

/// Entrées de menu dont l'indication de raccourci (texte grisé à droite du
/// libellé, `Button::shortcut_text`) est **dérivée** de `SHORTCUTS` plutôt que
/// recopiée à la main (roadmap post-audit UX v2 2026-09-04, 4.3) — la table
/// reste la seule source, un raccourci changé ici change aussi dans les menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    NewProject,
    Save,
    SaveAs,
    Open,
    Play,
    Quit,
    Shortcuts,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
}

impl MenuItem {
    pub const ALL: [MenuItem; 15] = [
        MenuItem::NewProject,
        MenuItem::Save,
        MenuItem::SaveAs,
        MenuItem::Open,
        MenuItem::Play,
        MenuItem::Quit,
        MenuItem::Shortcuts,
        MenuItem::Undo,
        MenuItem::Redo,
        MenuItem::Cut,
        MenuItem::Copy,
        MenuItem::Paste,
        MenuItem::Duplicate,
        MenuItem::Delete,
        MenuItem::SelectAll,
    ];

    /// Libellé exact de l'action dans `SHORTCUTS` et rang de la touche quand
    /// `keys` en regroupe plusieurs (« `Cmd+C` / `Cmd+X` / `Cmd+V` »).
    const fn source(self) -> (&'static str, usize) {
        match self {
            MenuItem::NewProject => ("Nouveau projet…", 0),
            MenuItem::Save => ("Enregistrer (scène du projet ouvert)", 0),
            MenuItem::SaveAs => ("Enregistrer sous…", 0),
            MenuItem::Open => ("Ouvrir une scène ou un projet…", 0),
            MenuItem::Play => ("Play / Stop", 0),
            MenuItem::Quit => (
                "Quitter (confirmation si modifications non enregistrées)",
                0,
            ),
            MenuItem::Shortcuts => ("Fenêtre Raccourcis clavier", 0),
            MenuItem::Undo => ("Annuler / Rétablir", 0),
            MenuItem::Redo => ("Annuler / Rétablir", 1),
            MenuItem::Copy => ("Copier / Couper / Coller", 0),
            MenuItem::Cut => ("Copier / Couper / Coller", 1),
            MenuItem::Paste => ("Copier / Couper / Coller", 2),
            MenuItem::Duplicate => ("Dupliquer la sélection", 0),
            MenuItem::Delete => ("Supprimer la sélection", 0),
            MenuItem::SelectAll => ("Tout sélectionner", 0),
        }
    }
}

/// Raccourci d'une entrée de menu, prêt à afficher : « ⌘⇧Z » sur Mac,
/// « Ctrl+Maj+Z » ailleurs. Chaîne vide si la table ne le connaît pas (le
/// test `every_menu_item_has_a_hint` garantit que ça n'arrive pas).
pub fn menu_hint(item: MenuItem) -> String {
    let (action, part) = item.source();
    SHORTCUTS
        .iter()
        .find(|s| s.action == action)
        .and_then(|s| s.keys.split(" / ").nth(part))
        .map(|key| display_keys(&key.replace('`', "")))
        .unwrap_or_default()
}

/// Notation d'affichage d'une touche de la table (« Cmd+Maj+Z ») selon la
/// plateforme : symboles Mac, sinon « Ctrl » à la place de « Cmd ».
pub fn display_keys(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("Cmd+", "⌘").replace("Maj+", "⇧")
    } else {
        keys.replace("Cmd", "Ctrl")
    }
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

    /// Chaque entrée de menu retrouve sa touche dans la table (roadmap v2 4.3) :
    /// renommer une action dans `SHORTCUTS` sans mettre `MenuItem::source` à
    /// jour ferait disparaître l'indication du menu en silence.
    #[test]
    fn every_menu_item_has_a_hint() {
        for item in MenuItem::ALL {
            let hint = menu_hint(item);
            assert!(!hint.is_empty(), "{item:?} sans raccourci dans SHORTCUTS");
            assert!(!hint.contains('`'), "{item:?} : backticks non retirés");
        }
        assert_ne!(menu_hint(MenuItem::Undo), menu_hint(MenuItem::Redo));
        assert_ne!(menu_hint(MenuItem::Copy), menu_hint(MenuItem::Paste));
        assert_ne!(menu_hint(MenuItem::Save), menu_hint(MenuItem::SaveAs));
    }

    #[test]
    fn display_keys_follows_the_platform() {
        let shown = display_keys("Cmd+Maj+Z");
        if cfg!(target_os = "macos") {
            assert_eq!(shown, "⌘⇧Z");
        } else {
            assert_eq!(shown, "Ctrl+Maj+Z");
        }
    }

    #[test]
    fn every_scope_has_entries() {
        for scope in [Scope::Editor, Scope::Mouse, Scope::Play, Scope::Touch] {
            assert!(by_scope(scope).next().is_some(), "{scope:?} vide");
        }
    }
}
