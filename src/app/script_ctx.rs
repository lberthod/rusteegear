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
    /// Le script courant lit-il `pose`/`hand` ? (`AppState::run_object_scripts`,
    /// d'après sa source) — sinon les deux tables ne sont pas reconstruites pour
    /// lui : ~70 objets du mannequin par pas n'en ont pas besoin.
    static POSE_WANTED: Cell<bool> = const { Cell::new(true) };
    /// État VR de l'image (tête, manettes) exposé aux scripts par la table `vr`
    /// (roadmap VR, phase 6) — `None` hors VR.
    static VR: Cell<Option<VrScriptState>> = const { Cell::new(None) };
    /// Le script en cours lit-il `vr` ? (même rôle que `POSE_WANTED`.)
    static VR_WANTED: Cell<bool> = const { Cell::new(true) };
    /// Vibrations demandées par `vr.haptic(côté, intensité, durée)` depuis le
    /// dernier `take_vr_haptics` : (0 = gauche / 1 = droite, intensité, s).
    static VR_HAPTICS: RefCell<Vec<(usize, f32, f32)>> = const { RefCell::new(Vec::new()) };
    /// Directions d'os poussées par `bone(nom, dx, dy, dz)` pendant le script
    /// courant, reprises dans `SceneObject::bone_dirs` juste après.
    static BONES: RefCell<Vec<(String, glam::Vec3)>> = const { RefCell::new(Vec::new()) };
    /// Émetteur de particules posé par `particles(...)` (Sprint 132) sur
    /// l'objet dont le script vient de s'exécuter — même patron que `BONES` :
    /// `None` tant que le script ne l'a pas appelé ce tick, auquel cas
    /// `SceneObject::particle_emitter` **n'est pas touché** (un émetteur posé
    /// dans l'inspecteur sur un objet par ailleurs scripté n'est jamais
    /// écrasé par un script qui n'a rien à voir avec les particules).
    static PARTICLE_REQUEST: RefCell<Option<crate::runtime::particles::ParticleEmitter>> =
        const { RefCell::new(None) };
    /// Cause et position de la dernière mort (mode plateformer 2D) : nom de
    /// l'objet mortel touché, `chute` sous `kill_y`, `vie` (santé à zéro) —
    /// pour des messages de mort contextuels (`death_cause`, `death_x`, `death_y`).
    static DEATH_CAUSE: RefCell<String> = const { RefCell::new(String::new()) };
    static DEATH_POS: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
    /// Coop (plateformer 2D à deux) : mode actif, joueur mort en dernier (1/2),
    /// morts par joueur — Lua `coop`, `death_player`, `deaths_p1`, `deaths_p2`.
    static COOP_INFO: Cell<(bool, u8, [u32; 2])> = const { Cell::new((false, 0, [0, 0])) };
    static VISIBLE_IN: Cell<bool> = const { Cell::new(true) };
    /// `obj.emissive` / `obj.opacity` (lecture/écriture, mode plateformer 2D :
    /// halos qui pulsent, spectres qui s'estompent) — même mécanique que `visible`.
    static FX_IN: Cell<(f32, f32)> = const { Cell::new((0.0, 1.0)) };
    static FX_OUT: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
    static VISIBLE_OUT: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Cause et position de la dernière mort, posées avec `set_deaths`.
pub(crate) fn set_death_info(cause: &str, x: f32, y: f32) {
    DEATH_CAUSE.with(|c| {
        let mut c = c.borrow_mut();
        if *c != cause {
            c.clear();
            c.push_str(cause);
        }
    });
    DEATH_POS.with(|p| p.set((x, y)));
}

pub(crate) fn death_cause() -> String {
    DEATH_CAUSE.with(|c| c.borrow().clone())
}

pub(crate) fn death_pos() -> (f32, f32) {
    DEATH_POS.with(|p| p.get())
}

/// Encode `sfx(nom)` : `sys:sfx:<nom>` — un effet sonore synthétisé du moteur
/// (`runtime::sfx::Sfx`), joué par `AppState::apply_script_outcomes`.
pub(crate) fn sfx_event(name: &str) -> String {
    format!("{SYS_EVENT_PREFIX}sfx:{name}")
}

pub(crate) fn parse_sfx_event(event: &str) -> Option<&str> {
    event.strip_prefix(SYS_EVENT_PREFIX)?.strip_prefix("sfx:")
}

/// Encode `set_sky(hr, hg, hb, zr, zg, zb)` : couleurs horizon et zénith du ciel
/// (linéaires), appliquées à `Scene::sky` — un ciel par monde dans une scène unique.
pub(crate) fn sky_event(horizon: [f32; 3], zenith: [f32; 3]) -> String {
    format!(
        "{SYS_EVENT_PREFIX}sky:{},{},{},{},{},{}",
        horizon[0], horizon[1], horizon[2], zenith[0], zenith[1], zenith[2]
    )
}

pub(crate) fn parse_sky_event(event: &str) -> Option<([f32; 3], [f32; 3])> {
    let rest = event.strip_prefix(SYS_EVENT_PREFIX)?.strip_prefix("sky:")?;
    let v: Vec<f32> = rest
        .split(',')
        .map(|s| s.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    if v.len() != 6 {
        return None;
    }
    Some(([v[0], v[1], v[2]], [v[3], v[4], v[5]]))
}

/// Nombre de morts de la partie en cours, exposé aux scripts en lecture seule
/// (`deaths`). Posé une fois par tick avant la boucle des scripts.
pub(crate) fn set_deaths(n: u32) {
    DEATHS.with(|d| d.set(n));
}

pub(crate) fn deaths() -> u32 {
    DEATHS.with(|d| d.get())
}

/// Contexte coop du tick (cf. `COOP_INFO`), posé avec `set_deaths`.
pub(crate) fn set_coop_info(coop: bool, death_player: u8, by_slot: [u32; 2]) {
    COOP_INFO.with(|c| c.set((coop, death_player, by_slot)));
}

pub(crate) fn coop_info() -> (bool, u8, [u32; 2]) {
    COOP_INFO.with(|c| c.get())
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

/// Un script mentionne-t-il `pose` ou `hand` ? Posé avant chaque appel de
/// `run_script`/`run_script_web` ; les backends sautent la construction des
/// tables quand c'est faux (elles gardent alors la valeur du script précédent,
/// que ce script ne lit pas).
pub(crate) fn set_pose_wanted(wanted: bool) {
    POSE_WANTED.with(|c| c.set(wanted));
}

pub(crate) fn pose_wanted() -> bool {
    POSE_WANTED.with(|c| c.get())
}

/// Une manette vue par les scripts (`vr.left`/`vr.right`), coordonnées monde.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VrHandScript {
    pub pos: glam::Vec3,
    pub trigger: f32,
    pub grip: f32,
    /// A (droite) / X (gauche).
    pub primary: bool,
    /// B (droite) / Y (gauche).
    pub secondary: bool,
}

/// État VR d'une image vu par les scripts (table `vr`), coordonnées monde.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VrScriptState {
    pub head: glam::Vec3,
    /// Lacet du regard (rad, même convention que `obj.ry`).
    pub yaw: f32,
    pub hands: [Option<VrHandScript>; 2],
}

/// Publie l'état VR de l'image pour les scripts (`None` hors VR).
pub(crate) fn set_vr(state: Option<VrScriptState>) {
    VR.with(|c| c.set(state));
}

pub(crate) fn vr_state() -> Option<VrScriptState> {
    VR.with(|c| c.get())
}

pub(crate) fn set_vr_wanted(wanted: bool) {
    VR_WANTED.with(|c| c.set(wanted));
}

pub(crate) fn vr_wanted() -> bool {
    VR_WANTED.with(|c| c.get())
}

/// Vrai si la source du script peut lire la table `vr`.
pub(crate) fn script_reads_vr(src: &str) -> bool {
    src.contains("vr")
}

/// `vr.haptic(côté, intensité, durée)` côté script (les deux backends) :
/// `side` = `"left"` ou autre (droite).
pub(crate) fn push_vr_haptic(side: &str, amplitude: f32, seconds: f32) {
    let hand = usize::from(side != "left");
    VR_HAPTICS.with(|h| {
        h.borrow_mut()
            .push((hand, amplitude.clamp(0.0, 1.0), seconds.clamp(0.0, 2.0)));
    });
}

/// Consomme les vibrations demandées par les scripts depuis le dernier appel.
pub(crate) fn take_vr_haptics() -> Vec<(usize, f32, f32)> {
    VR_HAPTICS.with(|h| std::mem::take(&mut *h.borrow_mut()))
}

/// Vrai si la source du script peut lire `pose` ou `hand`.
pub(crate) fn script_reads_pose(src: &str) -> bool {
    src.contains("pose") || src.contains("hand")
}

/// `bone(nom, dx, dy, dz)` côté script (les deux backends).
pub(crate) fn push_bone(name: String, dir: glam::Vec3) {
    BONES.with(|b| b.borrow_mut().push((name, dir)));
}

/// Consomme les directions poussées par le script qui vient de s'exécuter.
pub(crate) fn take_bones() -> Vec<(String, glam::Vec3)> {
    BONES.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

/// `particles(rate, dx, dy, dz, spread, speed, size, r, g, b)` côté script
/// (les deux backends) : pose l'émetteur que `SceneObject::particle_emitter`
/// prendra si le script en cours en appelle un cette frame-ci.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_particles(
    rate: f32,
    dir: glam::Vec3,
    spread: f32,
    speed: f32,
    size: f32,
    color: [f32; 3],
) {
    let em = crate::runtime::particles::ParticleEmitter {
        enabled: rate > 0.0,
        rate: rate.max(0.0),
        direction: dir.into(),
        spread: spread.max(0.0),
        speed_min: (speed * 0.7).max(0.0),
        speed_max: (speed * 1.3).max(0.0),
        size_min: (size * 0.7).max(0.001),
        size_max: (size * 1.3).max(0.001),
        color,
        ..Default::default()
    };
    PARTICLE_REQUEST.with(|p| *p.borrow_mut() = Some(em));
}

/// Consomme l'émetteur posé par `particles(...)` pendant le script qui vient
/// de s'exécuter, `None` s'il n'a pas été appelé ce tick.
pub(crate) fn take_particle_request() -> Option<crate::runtime::particles::ParticleEmitter> {
    PARTICLE_REQUEST.with(|p| p.borrow_mut().take())
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

pub(crate) fn set_object_fx(emissive: f32, opacity: f32) {
    FX_IN.with(|c| c.set((emissive, opacity)));
    FX_OUT.with(|c| c.set(None));
}

pub(crate) fn object_fx() -> (f32, f32) {
    FX_IN.with(|c| c.get())
}

pub(crate) fn report_fx(emissive: f32, opacity: f32) {
    FX_OUT.with(|c| c.set(Some((emissive, opacity))));
}

pub(crate) fn take_fx() -> Option<(f32, f32)> {
    FX_OUT.with(|c| c.take())
}

/// `set_light(i, x, y, z, r, g, b, intensité, portée)` → `sys:light:i,x,y,z,r,g,b,i,p` ;
/// `set_ambient(a)` → `sys:ambient:a` ; `set_fx(bloom, brouillard, r, g, b)` → `sys:fx:…`.
pub(crate) fn light_event(i: u32, v: [f32; 8]) -> String {
    format!(
        "{SYS_EVENT_PREFIX}light:{i},{}",
        v.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub(crate) fn parse_light_event(event: &str) -> Option<(usize, [f32; 8])> {
    let rest = event
        .strip_prefix(SYS_EVENT_PREFIX)?
        .strip_prefix("light:")?;
    let v: Vec<f32> = rest
        .split(',')
        .map(|s| s.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    if v.len() != 9 {
        return None;
    }
    Some((
        v[0] as usize,
        [v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8]],
    ))
}

pub(crate) fn parse_scalar_list(event: &str, kind: &str, n: usize) -> Option<Vec<f32>> {
    let rest = event
        .strip_prefix(SYS_EVENT_PREFIX)?
        .strip_prefix(kind)?
        .strip_prefix(':')?;
    let v: Vec<f32> = rest
        .split(',')
        .map(|s| s.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    (v.len() == n).then_some(v)
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
