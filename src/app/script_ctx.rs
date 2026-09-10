//! Contexte de tick partagé par les deux backends Lua (`scripting` natif/mlua et
//! `scripting_web`/rilua) pour le mode plateformer 2D (`Scene::platformer`) :
//! compteur de morts exposé en lecture (`deaths`) et visibilité de l'objet
//! courant, lue **et** réécrite par le script (`obj.visible`).
//!
//! Pourquoi un `thread_local!` plutôt que de nouveaux paramètres de `run_script` :
//! cette fonction a déjà ~27 paramètres et une cinquantaine de sites d'appel
//! (tests compris) ; un canal à côté, posé par `AppState::run_object_scripts`
//! juste avant chaque appel et relu juste après, ajoute la fonctionnalité sans
//! toucher à un seul de ces sites. Même patron que `cur_physics()` côté web.
//! Les scripts s'exécutent séquentiellement sur le thread de simulation : aucune
//! ré-entrance possible.

use std::cell::{Cell, RefCell};

use super::pose::{HandFrame, PoseFrame};

thread_local! {
    static DEATHS: Cell<u32> = const { Cell::new(0) };
    /// Dernière pose corporelle (démo Rééducation, cf. `app::pose`) : copiée une
    /// fois par tick avant la boucle des scripts, lue par les deux backends pour
    /// construire la table globale `pose`.
    static POSE: RefCell<PoseFrame> = RefCell::new(PoseFrame::default());
    /// Dernières mains (doigts, cf. `app::pose::HandFrame`), même cycle que `POSE`.
    static HANDS: RefCell<HandFrame> = RefCell::new(HandFrame::default());
    static VISIBLE_IN: Cell<bool> = const { Cell::new(true) };
    static VISIBLE_OUT: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Nombre de morts de la partie en cours, exposé aux scripts en lecture seule
/// (`deaths`). Posé une fois par tick avant la boucle des scripts.
pub(crate) fn set_deaths(n: u32) {
    DEATHS.with(|d| d.set(n));
}

pub(crate) fn deaths() -> u32 {
    DEATHS.with(|d| d.get())
}

/// Pose corporelle du tick (table Lua `pose`). Posée une fois par tick avant la
/// boucle des scripts, comme `set_deaths`.
pub(crate) fn set_pose(p: &PoseFrame) {
    POSE.with(|c| *c.borrow_mut() = p.clone());
}

/// Lit la pose du tick sans la copier (fermeture appelée sous l'emprunt).
pub(crate) fn with_pose<R>(f: impl FnOnce(&PoseFrame) -> R) -> R {
    POSE.with(|c| f(&c.borrow()))
}

/// Mains du tick (table Lua `hand`), cf. `set_pose`.
pub(crate) fn set_hands(h: &HandFrame) {
    HANDS.with(|c| *c.borrow_mut() = h.clone());
}

pub(crate) fn with_hands<R>(f: impl FnOnce(&HandFrame) -> R) -> R {
    HANDS.with(|c| f(&c.borrow()))
}

/// Visibilité de l'objet dont le script va s'exécuter (valeur initiale de
/// `obj.visible`). Posée avant chaque appel à `run_script`/`run_script_web`.
pub(crate) fn set_object_visible(v: bool) {
    VISIBLE_IN.with(|c| c.set(v));
    VISIBLE_OUT.with(|c| c.set(None));
}

pub(crate) fn object_visible() -> bool {
    VISIBLE_IN.with(|c| c.get())
}

/// Valeur de `obj.visible` relue après l'exécution du script (backend Lua).
pub(crate) fn report_visible(v: bool) {
    VISIBLE_OUT.with(|c| c.set(Some(v)));
}

/// Consomme la visibilité écrite par le script (`None` si le champ n'a pas pu
/// être relu, ex. script en erreur avant la relecture).
pub(crate) fn take_visible() -> Option<bool> {
    VISIBLE_OUT.with(|c| c.take())
}

/// Préfixe des événements « système » émis par les fonctions Lua `checkpoint()`
/// et `teleport()` : ils transitent par la même file que `emit()` (aucun
/// nouveau canal de sortie de script), mais sont interceptés et consommés par
/// `AppState::apply_script_outcomes` au lieu d'être livrés aux scripts.
pub(crate) const SYS_EVENT_PREFIX: &str = "sys:";

/// Encode une demande système `kind:x,y,z` (cf. `parse_sys_event`).
pub(crate) fn sys_event(kind: &str, x: f32, y: f32, z: f32) -> String {
    format!("{SYS_EVENT_PREFIX}{kind}:{x},{y},{z}")
}

/// Encode `hud_text(id, texte)` : `sys:hud:<id>:<texte>` (le texte peut contenir
/// des `:`, seul le premier après l'id sépare).
pub(crate) fn hud_event(id: &str, text: &str) -> String {
    format!("{SYS_EVENT_PREFIX}hud:{id}:{text}")
}

/// Décode un événement produit par `hud_event` : `(id, texte)`.
pub(crate) fn parse_hud_event(event: &str) -> Option<(&str, &str)> {
    let rest = event.strip_prefix(SYS_EVENT_PREFIX)?.strip_prefix("hud:")?;
    rest.split_once(':')
}

/// Décode un événement produit par `sys_event` : `("teleport", [x, y, z])`.
/// `None` si ce n'est pas un événement système ou s'il est malformé.
pub(crate) fn parse_sys_event(event: &str) -> Option<(&str, [f32; 3])> {
    let rest = event.strip_prefix(SYS_EVENT_PREFIX)?;
    let (kind, coords) = rest.split_once(':')?;
    let mut it = coords.split(',').map(|s| s.trim().parse::<f32>().ok());
    let x = it.next()??;
    let y = it.next()??;
    let z = it.next()??;
    if it.next().is_some() {
        return None;
    }
    Some((kind, [x, y, z]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sys_event_round_trips() {
        let e = sys_event("teleport", 12.5, -1.0, 0.0);
        assert_eq!(parse_sys_event(&e), Some(("teleport", [12.5, -1.0, 0.0])));
    }

    #[test]
    fn hud_event_round_trips_and_keeps_colons_in_text() {
        let e = hud_event("taunt", "Le sol : il était là.");
        assert_eq!(
            parse_hud_event(&e),
            Some(("taunt", "Le sol : il était là."))
        );
        assert_eq!(parse_hud_event("sys:teleport:1,2,3"), None);
        assert_eq!(parse_hud_event("hud:x:y"), None);
    }

    #[test]
    fn non_sys_events_are_ignored() {
        assert_eq!(parse_sys_event("score:3"), None);
        assert_eq!(parse_sys_event("sys:teleport:1,2"), None);
        assert_eq!(parse_sys_event("sys:teleport:a,b,c"), None);
    }

    #[test]
    fn visible_channel_is_consumed_once() {
        set_object_visible(true);
        assert!(object_visible());
        assert_eq!(take_visible(), None);
        report_visible(false);
        assert_eq!(take_visible(), Some(false));
        assert_eq!(take_visible(), None);
    }
}
