//! API Lua exposée aux scripts d'objet (Sprint 105a-1, extrait de `app/mod.rs`
//! — code inchangé, seulement déplacé). Aucune dépendance à `AppState` : ces
//! fonctions manipulent uniquement les valeurs qu'on leur passe par référence
//! (transform, couleur, animation, événements…), appelées depuis `AppState::
//! sim_step` (`app::simulation`).

use std::hash::{Hash, Hasher};

use glam::{EulerRot, Quat, Vec3};
use mlua::Lua;

use super::PlayerInput;
use crate::scene::Transform;

/// Hash stable d'une source de script, clé du cache de chunks compilés.
pub(super) fn script_key(src: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// Exécute le chunk Lua **déjà compilé** d'un objet : expose `obj` (x,y,z,
/// rx,ry,rz en °, sx,sy,sz, r,g,b, tapped), `dt`, `time` et `input`, puis relit
/// les champs modifiés.
#[allow(clippy::too_many_arguments)] // contexte d'exécution d'un script : champs distincts
pub(super) fn run_script(
    lua: &Lua,
    func: &mlua::Function,
    t: &mut Transform,
    color: &mut [f32; 3],
    anim: &mut Option<crate::scene::AnimationState>,
    dt: f32,
    time: f32,
    input: &PlayerInput,
    tapped: bool,
    triggered: bool,
    events_in: &[String],
    events_out: &mut Vec<String>,
    tagged: &[(String, Vec3)],
    spawn_out: &mut Vec<(String, Vec3)>,
    destroy_out: &mut bool,
    vars: &mut std::collections::HashMap<String, f64>,
    vib_out: &mut Vec<f32>,
    health_out: &mut Option<f32>,
    debug_out: &mut Vec<(Vec3, Vec3, [f32; 3])>,
    exited: bool,
    physics: Option<&crate::runtime::physics::Physics>,
    reverb_out: &mut Vec<f32>,
) -> mlua::Result<()> {
    let (rx, ry, rz) = t.rotation.to_euler(EulerRot::XYZ);
    let obj = lua.create_table()?;
    obj.set("x", t.position.x)?;
    obj.set("y", t.position.y)?;
    obj.set("z", t.position.z)?;
    obj.set("rx", rx.to_degrees())?;
    obj.set("ry", ry.to_degrees())?;
    obj.set("rz", rz.to_degrees())?;
    obj.set("sx", t.scale.x)?;
    obj.set("sy", t.scale.y)?;
    obj.set("sz", t.scale.z)?;
    obj.set("r", color[0])?;
    obj.set("g", color[1])?;
    obj.set("b", color[2])?;
    obj.set("tapped", tapped)?;
    obj.set("triggered", triggered)?;
    // `obj.exited` : symétrique de `triggered` — vrai le tick où le
    // contact avec cette zone `trigger` vient de cesser (cf. `AppState::trigger_prev`
    // dans `sim_step`), pas juste « pas en contact » (qui vaudrait aussi tant que le
    // joueur n'est jamais entré).
    obj.set("exited", exited)?;
    // `obj.anim` : clip actuellement joué, lu en écriture après l'appel pour
    // piloter la FSM depuis Lua (`obj.anim = "run"` démarre un fondu enchaîné vers ce
    // clip). N'existe que pour les objets skinnés ; ignoré silencieusement sinon, comme
    // `hud` reste vide tant qu'aucun script n'y touche.
    obj.set("anim", anim.as_ref().map(|a| a.clip.as_str()).unwrap_or(""))?;

    // `obj:destroy()` : suppression **douce** — `visible = false`, comme
    // les monstres vaincus (`Scene::attack_at`) ou les collectibles ramassés
    // (`Scene::collect_at`) — pas un vrai retrait de `scene.objects` (ça invaliderait
    // les indices que d'autres systèmes retiennent d'une frame à l'autre : réseau,
    // undo, IA — pas de handles générationnels pour l'instant).
    // Appelée en syntaxe méthode (`obj:destroy()`) : Lua passe `obj` lui-même comme
    // premier argument, ignoré ici (la fermeture sait déjà quel objet elle sert).
    let destroy_tbl = lua.create_table()?;
    let destroy_ref = destroy_tbl.clone();
    let destroy_fn = lua.create_function(move |_, _self: mlua::Table| {
        destroy_ref.set("d", true)?;
        Ok(())
    })?;
    obj.set("destroy", destroy_fn)?;

    // Contrôles tactiles : `input.jx`, `input.jy` (joystick) et `input.btn.<nom>` (booléens).
    let input_tbl = lua.create_table()?;
    input_tbl.set("jx", input.joy.0)?;
    input_tbl.set("jy", input.joy.1)?;
    let btns = lua.create_table()?;
    for name in &input.buttons {
        btns.set(name.as_str(), true)?;
    }
    input_tbl.set("btn", btns)?;

    // `vibrate(ms)` : empile les durées de vibration demandées par le script.
    let vib = lua.create_table()?;
    let vib_ref = vib.clone();
    let vibrate = lua.create_function(move |_, ms: f32| {
        vib_ref.push(ms)?;
        Ok(())
    })?;

    // `reverb(mix)` (Sprint 121) : demande un mélange sec/mouillé (0..1) pour la
    // réverbération du bus SFX — typiquement appelé depuis le script d'une zone
    // `trigger` (Sprint 89) à l'entrée/sortie (`obj.triggered`/`obj.exited`), pas
    // un nouveau type de zone natif. Empilé comme `vibrate` : le dernier appel du
    // tick l'emporte côté Rust (cf. `AppState::sim_step`), pas de fusion ici.
    let reverb_tbl = lua.create_table()?;
    let reverb_ref = reverb_tbl.clone();
    let reverb_fn = lua.create_function(move |_, mix: f32| {
        reverb_ref.push(mix.clamp(0.0, 1.0))?;
        Ok(())
    })?;

    // Inclinaison (gyroscope) : `tilt.x`, `tilt.y`.
    let tilt = lua.create_table()?;
    tilt.set("x", input.tilt.0)?;
    tilt.set("y", input.tilt.1)?;

    // `set_health(v)` : pilote la barre de vie du HUD (0..1), valeur absolue.
    // La table `hud` reste vide tant qu'aucun script n'y touche (opt-in : les scripts
    // sans rapport avec la vie — décor animé, etc. — ne font pas apparaître la barre).
    let hud = lua.create_table()?;
    let hud_ref = hud.clone();
    let set_health = lua.create_function(move |_, v: f32| {
        hud_ref.set("h", v.clamp(0.0, 1.0))?;
        Ok(())
    })?;
    // `damage(v)` : soustrait `v` à la vie courante (accumulée depuis le début de la
    // frame, entre objets inclus) plutôt que de l'écraser — plusieurs ennemis peuvent
    // infliger des dégâts la même frame sans s'annuler mutuellement comme le ferait
    // `set_health` (valeur absolue). Base = vie déjà régénérée/endommagée cette frame,
    // ou pleine vie par défaut si le système de vie n'a jamais démarré.
    let base_health = health_out.unwrap_or(1.0);
    let hud_ref_dmg = hud.clone();
    let damage = lua.create_function(move |_, v: f32| {
        let cur: f32 = hud_ref_dmg.get("h").unwrap_or(base_health);
        hud_ref_dmg.set("h", (cur - v).clamp(0.0, 1.0))?;
        Ok(())
    })?;

    // `debug.line(x1,y1,z1,x2,y2,z2,r,g,b)` : visualise un raycast, une ligne
    // de vue, une trajectoire — visible une frame, comme `AppState::debug_line` côté Rust.
    // Accumule un segment de 9 nombres par appel, décodé après `func.call`.
    let debug_tbl = lua.create_table()?;
    let debug_ref = debug_tbl.clone();
    let debug_line =
        lua.create_function(
            move |_,
                  (x1, y1, z1, x2, y2, z2, r, g, b): (
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
            )| {
                debug_ref.push(x1)?;
                debug_ref.push(y1)?;
                debug_ref.push(z1)?;
                debug_ref.push(x2)?;
                debug_ref.push(y2)?;
                debug_ref.push(z2)?;
                debug_ref.push(r)?;
                debug_ref.push(g)?;
                debug_ref.push(b)?;
                Ok(())
            },
        )?;
    let debug_api = lua.create_table()?;
    debug_api.set("line", debug_line)?;

    // Événements de gameplay. `emit("nom")` accumule (délivré à tous les
    // scripts au tick suivant, cf. `AppState::game_events`) ; `on_event("nom")` teste
    // les événements reçus ce tick. Un ensemble (nom → true) plutôt qu'une liste à
    // parcourir côté Lua : `on_event` est le geste attendu dans un script — « ce tick,
    // est-ce arrivé ? » — pas l'itération sur tout ce qui s'est passé.
    let emit_tbl = lua.create_table()?;
    let emit_ref = emit_tbl.clone();
    let emit = lua.create_function(move |_, name: String| {
        emit_ref.push(name)?;
        Ok(())
    })?;
    let received = lua.create_table()?;
    for name in events_in {
        received.set(name.as_str(), true)?;
    }
    let on_event = lua.create_function(move |_, name: String| {
        Ok(received.get::<bool>(name.as_str()).unwrap_or(false))
    })?;

    // `spawn(prefab_ref, x, y, z)` : accumule une demande (référence de
    // prefab `asset-id://…`, position), appliquée **après** la boucle des scripts par
    // `AppState::sim_step` — jamais pendant, `scene.objects` est en cours d'itération
    // mutable à ce moment-là. Les nouveaux objets sont ajoutés en fin de tableau : les
    // indices existants (réseau, undo, IA) restent valides, contrairement à une
    // suppression (cf. `obj:destroy()`, volontairement plus prudente pour la même
    // raison — pas de retrait/réutilisation de slots pour l'instant).
    let spawn_tbl = lua.create_table()?;
    let spawn_ref = spawn_tbl.clone();
    let spawn = lua.create_function(move |lua, (prefab, x, y, z): (String, f32, f32, f32)| {
        let entry = lua.create_table()?;
        entry.set("prefab", prefab)?;
        entry.set("x", x)?;
        entry.set("y", y)?;
        entry.set("z", z)?;
        spawn_ref.push(entry)?;
        Ok(())
    })?;

    // `find_tag("nom")` : instantané pris **avant** la boucle des scripts
    // (`AppState::sim_step`) — un objet tout juste spawné/détruit ce même tick n'y
    // apparaît donc pas encore/plus, disponible seulement au tick suivant. Ne renvoie
    // que la position (pas de référence vivante à l'objet : les scripts n'ont accès
    // qu'à leur propre `obj`, jamais directement à celui d'un autre).
    let tagged_snapshot: Vec<(String, Vec3)> = tagged.to_vec();
    let find_tag = lua.create_function(move |lua, tag: String| {
        let out = lua.create_table()?;
        let mut n = 1;
        for (t, pos) in &tagged_snapshot {
            if t != &tag {
                continue;
            }
            let entry = lua.create_table()?;
            entry.set("x", pos.x)?;
            entry.set("y", pos.y)?;
            entry.set("z", pos.z)?;
            out.set(n, entry)?;
            n += 1;
        }
        Ok(out)
    })?;

    // `save.get("clé")`/`save.set("clé", valeur)` : état de script
    // persistant, capturé par `runtime::savegame::SaveGame` avec le score et les
    // positions. Partagé (pas par objet) : les scripts s'exécutent séquentiellement
    // dans la boucle de `sim_step`, donc un script voit déjà ce qu'un précédent a
    // écrit ce même tick — cohérent avec l'ordre naturel d'exécution, pas besoin
    // d'un décalage d'un tick comme pour les événements de gameplay (ceux-là
    // *doivent* attendre le tick suivant pour être indépendants de l'ordre des
    // scripts ; ici l'ordre séquentiel est simplement accepté tel quel).
    let vars_cell = std::rc::Rc::new(std::cell::RefCell::new(std::mem::take(vars)));
    let vars_get = vars_cell.clone();
    let save_get =
        lua.create_function(move |_, key: String| Ok(vars_get.borrow().get(&key).copied()))?;
    let vars_set = vars_cell.clone();
    let save_set = lua.create_function(move |_, (key, val): (String, f64)| {
        vars_set.borrow_mut().insert(key, val);
        Ok(())
    })?;
    let save_api = lua.create_table()?;
    save_api.set("get", save_get)?;
    save_api.set("set", save_set)?;

    let g = lua.globals();
    g.set("obj", &obj)?;
    g.set("dt", dt)?;
    g.set("time", time)?;
    g.set("input", input_tbl)?;
    g.set("tilt", tilt)?;
    g.set("vibrate", vibrate)?;
    g.set("reverb", reverb_fn)?;
    g.set("set_health", set_health)?;
    g.set("damage", damage)?;
    g.set("emit", emit)?;
    g.set("on_event", on_event)?;
    g.set("spawn", spawn)?;
    g.set("find_tag", find_tag)?;
    g.set("save", save_api)?;
    g.set("debug", debug_api)?;
    // `raycast`/`overlap_sphere` : requêtes spatiales via le `QueryPipeline`
    // de rapier (`Physics::raycast`/`overlap_sphere`) — capteur de sol (rayon vers le
    // bas), cône de vision (ligne de vue vers une cible trouvée par `find_tag`), etc.
    // Fermetures **scopées** (`lua.scope`, contrairement aux autres fonctions Lua
    // ci-dessus qui ne capturent que des valeurs possédées/clonées) : elles empruntent
    // `physics` (`&Physics`, pas `'static`, coûteux à cloner par script/tick). Une
    // fermeture scopée expire à la fin du bloc `lua.scope` — `func.call` doit donc
    // avoir lieu *dans* ce bloc, pas après. `physics` vaut `None` hors mode Play (pas
    // de monde physique construit) : les deux fonctions renvoient alors « rien touché »
    // plutôt que de planter.
    let call_result = lua.scope(|scope| {
        let raycast_fn = scope.create_function(
            |lua,
             (ox, oy, oz, dx, dy, dz, max_dist, mask): (
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                Option<u32>,
            )| {
                let hit = physics.and_then(|p| {
                    p.raycast(
                        Vec3::new(ox, oy, oz),
                        Vec3::new(dx, dy, dz),
                        max_dist,
                        mask.unwrap_or(u32::MAX),
                    )
                });
                match hit {
                    Some(h) => {
                        let out = lua.create_table()?;
                        out.set("x", h.point.x)?;
                        out.set("y", h.point.y)?;
                        out.set("z", h.point.z)?;
                        out.set("dist", h.distance)?;
                        Ok(mlua::Value::Table(out))
                    }
                    None => Ok(mlua::Value::Nil),
                }
            },
        )?;
        let overlap_sphere_fn = scope.create_function(
            |_, (x, y, z, radius, mask): (f32, f32, f32, f32, Option<u32>)| {
                let count = physics
                    .map(|p| {
                        p.overlap_sphere(Vec3::new(x, y, z), radius, mask.unwrap_or(u32::MAX))
                            .len()
                    })
                    .unwrap_or(0);
                Ok(count as i64)
            },
        )?;
        g.set("raycast", raycast_fn)?;
        g.set("overlap_sphere", overlap_sphere_fn)?;
        func.call::<()>(())
    });

    // Recopié même si le script a levé une erreur ensuite (propager `?` sans reposer
    // `vars` perdrait tout ce que ce script avait écrit avant l'erreur — cf.
    // `std::mem::take` plus haut, qui a vidé `*vars` dans la cellule). `.clone()`
    // plutôt que `Rc::try_unwrap` : `lua` réutilise la même table de globales d'un
    // appel à l'autre, rien ne garantit que le garbage collector Lua ait déjà
    // libéré les fermetures `save.get`/`save.set` de cet appel précis au moment où
    // on arrive ici — `try_unwrap` échouerait alors silencieusement.
    *vars = vars_cell.borrow().clone();
    call_result?;

    if destroy_tbl.get::<bool>("d").unwrap_or(false) {
        *destroy_out = true;
    }
    for name in emit_tbl.sequence_values::<String>().flatten() {
        events_out.push(name);
    }
    for entry in spawn_tbl.sequence_values::<mlua::Table>().flatten() {
        let prefab: String = entry.get("prefab").unwrap_or_default();
        let x: f32 = entry.get("x").unwrap_or(0.0);
        let y: f32 = entry.get("y").unwrap_or(0.0);
        let z: f32 = entry.get("z").unwrap_or(0.0);
        spawn_out.push((prefab, Vec3::new(x, y, z)));
    }

    for v in vib.sequence_values::<f32>().flatten() {
        vib_out.push(v);
    }
    for v in reverb_tbl.sequence_values::<f32>().flatten() {
        reverb_out.push(v);
    }
    if let Ok(h) = hud.get::<f32>("h") {
        *health_out = Some(h);
    }
    let flat: Vec<f32> = debug_tbl.sequence_values::<f32>().flatten().collect();
    for chunk in flat.chunks_exact(9) {
        debug_out.push((
            Vec3::new(chunk[0], chunk[1], chunk[2]),
            Vec3::new(chunk[3], chunk[4], chunk[5]),
            [chunk[6], chunk[7], chunk[8]],
        ));
    }

    t.position = Vec3::new(obj.get("x")?, obj.get("y")?, obj.get("z")?);
    let (rx, ry, rz): (f32, f32, f32) = (obj.get("rx")?, obj.get("ry")?, obj.get("rz")?);
    t.rotation = Quat::from_euler(
        EulerRot::XYZ,
        rx.to_radians(),
        ry.to_radians(),
        rz.to_radians(),
    );
    t.scale = Vec3::new(obj.get("sx")?, obj.get("sy")?, obj.get("sz")?);
    *color = [obj.get("r")?, obj.get("g")?, obj.get("b")?];
    if let Some(state) = anim {
        let requested: String = obj.get("anim")?;
        if !requested.is_empty() {
            state.set_clip(requested);
        }
    }
    Ok(())
}

/// Un arrêt sur une ligne de breakpoint (Sprint 128) : capturé côté hook `mlua`,
/// consommé par l'éditeur (fenêtre à venir) pour afficher où/quand un script
/// s'est arrêté. `source` : nom du chunk tel que rapporté par `mlua` (souvent
/// `[string "..."]` pour un chunk chargé depuis une chaîne, comme le sont tous
/// les scripts d'objet ici — pas un nom de fichier).
#[derive(Clone, Debug, PartialEq)]
pub struct BreakpointHit {
    pub line: u32,
    pub source: String,
}

/// Breakpoints Lua basiques (Sprint 128) : un hook `mlua` (`HookTriggers::EVERY_LINE`)
/// installé une fois sur l'instance `Lua` partagée (`AppState::lua`, une par
/// `AppState`, tous les scripts d'objet la partagent) interrompt l'exécution du
/// script en cours dès qu'une ligne marquée est atteinte, en renvoyant une erreur
/// distinctive plutôt que `VmState::Continue`.
///
/// **Portée volontairement limitée** : ceci n'est *pas* un vrai débogueur pas-à-pas
/// (inspection interactive des variables, reprise après pause) — l'exécution des
/// scripts ici est synchrone dans la boucle de jeu (`AppState::sim_step`, appelée
/// une fois par tick), pas une coroutine qu'on pourrait suspendre puis reprendre
/// plus tard sans redesign. « Se mettre en pause à une ligne donnée » se traduit
/// donc concrètement par : l'exécution du script s'arrête **avant** cette ligne
/// pour ce tick (rien après ne s'exécute, aucun champ `obj.*` n'est réécrit,
/// cf. `run_script`), et l'arrêt est enregistré (`take_hits`) pour que l'éditeur
/// puisse l'afficher — reproductible tick après tick tant que le breakpoint reste
/// actif, ce qui suffit à repérer/isoler une ligne qui pose problème.
///
/// `Arc<Mutex<..>>` : le hook `mlua` doit être `'static` (ne peut pas emprunter
/// `&mut AppState`), donc l'état partagé transite par une poignée clonable plutôt
/// que par référence.
#[derive(Clone, Default)]
pub(super) struct LuaBreakpoints(std::sync::Arc<std::sync::Mutex<BreakpointState>>);

#[derive(Default)]
struct BreakpointState {
    lines: std::collections::HashSet<u32>,
    hits: Vec<BreakpointHit>,
}

fn lock_breakpoints(
    state: &std::sync::Mutex<BreakpointState>,
) -> std::sync::MutexGuard<'_, BreakpointState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl LuaBreakpoints {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Active/désactive un breakpoint à `line` (numéro de ligne 1-indexé, comme
    /// affiché par un éditeur de texte — même convention que `mlua::Debug::current_line`).
    pub(super) fn set(&self, line: u32, enabled: bool) {
        let mut state = lock_breakpoints(&self.0);
        if enabled {
            state.lines.insert(line);
        } else {
            state.lines.remove(&line);
        }
    }

    pub(super) fn is_set(&self, line: u32) -> bool {
        lock_breakpoints(&self.0).lines.contains(&line)
    }

    pub(super) fn lines(&self) -> Vec<u32> {
        let mut lines: Vec<u32> = lock_breakpoints(&self.0).lines.iter().copied().collect();
        lines.sort_unstable();
        lines
    }

    /// Retire et renvoie tous les arrêts enregistrés depuis le dernier appel —
    /// consommé une fois (par l'éditeur), pas relu indéfiniment.
    pub(super) fn take_hits(&self) -> Vec<BreakpointHit> {
        std::mem::take(&mut lock_breakpoints(&self.0).hits)
    }

    /// Installe le hook sur `lua` — à appeler une fois par instance `Lua` (typiquement
    /// dans `AppState::new()`, juste après `Lua::new()`). `Err` uniquement si `mlua`
    /// refuse le hook lui-même (jamais observé en pratique sur ce backend) ; pas une
    /// erreur de script — celles-ci ne peuvent survenir qu'une fois le hook actif,
    /// via son propre retour `Err` sur un breakpoint atteint.
    pub(super) fn install(&self, lua: &Lua) -> mlua::Result<()> {
        let shared = self.0.clone();
        lua.set_hook(mlua::HookTriggers::EVERY_LINE, move |_lua, debug| {
            let Some(line) = debug.current_line() else {
                return Ok(mlua::VmState::Continue);
            };
            let line = line as u32;
            let mut state = lock_breakpoints(&shared);
            if state.lines.contains(&line) {
                let source = debug
                    .source()
                    .source
                    .map(|s| s.into_owned())
                    .unwrap_or_default();
                state.hits.push(BreakpointHit { line, source });
                return Err(mlua::Error::RuntimeError(format!(
                    "⏸ breakpoint (ligne {line})"
                )));
            }
            Ok(mlua::VmState::Continue)
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl super::AppState {
    /// Active/désactive un breakpoint Lua à `line` — cf. la doc de
    /// `LuaBreakpoints` pour ce que ça déclenche concrètement.
    pub fn toggle_lua_breakpoint(&mut self, line: u32) {
        let enabled = !self.lua_breakpoints.is_set(line);
        self.lua_breakpoints.set(line, enabled);
    }

    /// Lignes actuellement marquées (triées), pour l'affichage éditeur.
    pub fn lua_breakpoint_lines(&self) -> Vec<u32> {
        self.lua_breakpoints.lines()
    }

    /// Arrêts survenus depuis le dernier appel (consommés une fois).
    pub fn take_lua_breakpoint_hits(&mut self) -> Vec<BreakpointHit> {
        self.lua_breakpoints.take_hits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{MeshKind, Scene, SceneObject};

    /// Sprint 128 : un breakpoint sur une ligne précise interrompt l'exécution
    /// **avant** cette ligne (rien après ne s'exécute) et enregistre l'arrêt —
    /// une ligne non marquée n'a aucun effet, même exécutée normalement.
    #[test]
    fn breakpoint_stops_execution_before_the_marked_line_and_records_the_hit() {
        let lua = Lua::new();
        let breakpoints = LuaBreakpoints::new();
        breakpoints.install(&lua).unwrap();

        let src = "\
            obj.x = 1\n\
            obj.x = 2\n\
            obj.x = 3\n";
        let func = lua.load(src).into_function().unwrap();

        // Ligne 2 marquée : `obj.x = 1` doit s'exécuter, `obj.x = 2`/`obj.x = 3` non.
        breakpoints.set(2, true);
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let input = PlayerInput::default();
        let result = run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        );
        assert!(
            result.is_err(),
            "un breakpoint atteint doit remonter comme une erreur de script"
        );
        assert_eq!(
            t.position.x, 0.0,
            "obj.x = 1 n'a pas eu le temps d'être relu (le breakpoint stoppe avant \
             que run_script ne relise les champs modifiés, cf. sa doc)"
        );

        let hits = breakpoints.take_hits();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
        // `take_hits` consomme : un deuxième appel immédiat ne doit rien retrouver.
        assert!(breakpoints.take_hits().is_empty());
    }

    /// Une exécution sans aucun breakpoint marqué ne doit jamais être interrompue,
    /// même avec le hook installé — le hook doit rester un no-op tant qu'aucune
    /// ligne n'est marquée.
    #[test]
    fn no_breakpoints_means_normal_execution() {
        let lua = Lua::new();
        let breakpoints = LuaBreakpoints::new();
        breakpoints.install(&lua).unwrap();

        let func = lua.load("obj.x = 42").into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let input = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.x, 42.0);
        assert!(breakpoints.take_hits().is_empty());
    }

    #[test]
    fn toggling_a_breakpoint_twice_clears_it() {
        let breakpoints = LuaBreakpoints::new();
        assert!(!breakpoints.is_set(5));
        breakpoints.set(5, true);
        assert!(breakpoints.is_set(5));
        breakpoints.set(5, false);
        assert!(!breakpoints.is_set(5));
        assert!(breakpoints.lines().is_empty());
    }

    #[test]
    fn script_reads_mobile_input() {
        // Le script déplace l'objet selon le joystick et saute si le bouton « B1 » est pressé.
        let lua = Lua::new();
        let src = "obj.x = obj.x + input.jx; if input.btn.B1 then obj.y = 5 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let mut input = PlayerInput {
            joy: (0.5, 0.0),
            ..Default::default()
        };
        input.buttons.insert("B1".into());
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert!((t.position.x - 0.5).abs() < 1e-5);
        assert!((t.position.y - 5.0).abs() < 1e-5);

        // Sans bouton ni joystick : aucun mouvement.
        let mut t2 = Transform::from_pos(Vec3::ZERO);
        let empty = PlayerInput::default();
        run_script(
            &lua,
            &func,
            &mut t2,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &empty,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert!((t2.position.x).abs() < 1e-5);
        assert!((t2.position.y).abs() < 1e-5);
    }

    #[test]
    fn script_debug_line_is_read_back_into_debug_out() {
        // `debug.line(...)` côté Lua doit atterrir dans `debug_out`, avec les
        // mêmes coordonnées/couleur que ce que le script a passé — un appel par ligne de
        // script, deux appels ici pour vérifier qu'ils s'accumulent sans s'écraser.
        let lua = Lua::new();
        let src = "debug.line(0,0,0, 1,2,3, 1,0,0); debug.line(-1,0,0, 0,0,0, 0,1,0)";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut debug_out = Vec::new();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut debug_out,
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(debug_out.len(), 2);
        assert_eq!(
            debug_out[0],
            (Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0), [1.0, 0.0, 0.0])
        );
        assert_eq!(
            debug_out[1],
            (Vec3::new(-1.0, 0.0, 0.0), Vec3::ZERO, [0.0, 1.0, 0.0])
        );
    }

    #[test]
    fn script_emit_lands_in_events_out_and_on_event_reads_events_in() {
        // `emit("x")` doit atterrir dans `events_out` (délivré au tick
        // suivant par `sim_step`), et `on_event` doit refléter exactement `events_in`
        // — vrai pour un nom reçu, faux pour tout le reste (y compris ce qu'on est en
        // train d'émettre : pas de livraison intra-tick, cf. doc de `game_events`).
        let lua = Lua::new();
        let src = "emit('porte'); if on_event('score:3') then obj.y = 9 end; \
                   if on_event('porte') then obj.x = 9 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut events_out = Vec::new();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &["score:3".to_string()],
            &mut events_out,
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(events_out, vec!["porte".to_string()]);
        assert!(
            (t.position.y - 9.0).abs() < 1e-5,
            "on_event('score:3') devait être vrai (événement reçu)"
        );
        assert!(
            t.position.x.abs() < 1e-5,
            "on_event('porte') devait être faux : un emit de ce tick n'est pas \
             délivré au même tick"
        );
    }

    #[test]
    fn find_tag_returns_positions_of_matching_visible_objects() {
        // `find_tag` doit renvoyer la position de chaque objet visible
        // portant le tag demandé, aucun autre — testé directement sur `run_script`
        // (pas besoin d'un `AppState` complet pour cette brique).
        let lua = Lua::new();
        let src = "local hits = find_tag('ennemi'); obj.x = #hits; \
                   if #hits > 0 then obj.y = hits[1].y end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let tagged = vec![
            ("ennemi".to_string(), Vec3::new(1.0, 2.0, 3.0)),
            ("ennemi".to_string(), Vec3::new(4.0, 5.0, 6.0)),
            ("allié".to_string(), Vec3::new(9.0, 9.0, 9.0)),
        ];
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &tagged,
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            t.position.x, 2.0,
            "seuls les 2 ennemis doivent être trouvés"
        );
        assert_eq!(t.position.y, 2.0);
    }

    #[test]
    fn lua_coroutines_work_out_of_the_box() {
        // `mlua::Lua::new()` charge la stdlib Lua complète, coroutines
        // incluses — rien à câbler côté moteur, juste à vérifier que ça tourne
        // réellement.
        let lua = Lua::new();
        let src = "\
            local co = coroutine.create(function()
                coroutine.yield(1)
                return 2
            end)
            local ok1, v1 = coroutine.resume(co)
            local ok2, v2 = coroutine.resume(co)
            return ok1 and ok2 and v1 == 1 and v2 == 2";
        let result: bool = lua.load(src).eval().unwrap();
        assert!(
            result,
            "les coroutines Lua standard doivent fonctionner telles quelles"
        );
    }

    #[test]
    fn script_save_set_persists_and_save_get_reads_it_back() {
        // `save.set`/`save.get` doivent partager le même état que
        // `AppState::lua_vars` — c'est cet état que `SaveGame` capture/restaure.
        let lua = Lua::new();
        let src = "save.set('pv_max', 42.0); obj.x = save.get('pv_max')";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut vars = std::collections::HashMap::new();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut vars,
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.x, 42.0);
        assert_eq!(vars.get("pv_max"), Some(&42.0));
    }

    #[test]
    fn script_key_stable_and_distinct() {
        assert_eq!(script_key("obj.x = 1"), script_key("obj.x = 1"));
        assert_ne!(script_key("obj.x = 1"), script_key("obj.x = 2"));
    }

    /// Sol plat statique seul (comme `physics::tests::ground_and_wall_scene`, sans le
    /// mur) — sert au « capteur de sol » scripté en Lua.
    fn floor_only_scene() -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: MeshKind::Cube,
            transform: Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                .with_scale(Vec3::new(10.0, 1.0, 10.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        scene
    }

    #[test]
    fn raycast_lua_ground_sensor_reports_hit_point_and_draws_a_debug_line() {
        // Un « capteur de sol » scripté en Lua — un rayon vers
        // le bas via `raycast()`, visualisé au debug drawing jusqu'au point
        // d'impact renvoyé.
        let scene = floor_only_scene();
        let phys = crate::runtime::physics::Physics::build(&scene);
        let lua = Lua::new();
        let src = "local hit = raycast(0, 5, 0, 0, -1, 0, 100)\n\
                   if hit then\n\
                     obj.y = hit.dist\n\
                     debug.line(0, 5, 0, hit.x, hit.y, hit.z, 0, 1, 0)\n\
                   end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut debug_out = Vec::new();
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut debug_out,
            false,
            Some(&phys),
            &mut Vec::new(),
        )
        .unwrap();
        assert!(
            (t.position.y - 5.5).abs() < 0.05,
            "hit.dist doit valoir ~5.5 m (y={})",
            t.position.y
        );
        assert_eq!(debug_out.len(), 1, "le capteur doit dessiner une ligne");
        let (a, b, color) = debug_out[0];
        assert_eq!(a, Vec3::new(0.0, 5.0, 0.0));
        assert!((b.y - -0.5).abs() < 0.05, "point d'impact au sol (b={b})");
        assert_eq!(color, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn raycast_lua_returns_nil_when_nothing_is_hit() {
        let scene = floor_only_scene();
        let phys = crate::runtime::physics::Physics::build(&scene);
        let lua = Lua::new();
        // Vers le haut : rien au-dessus du sol, `raycast` doit renvoyer `nil`.
        let src = "if raycast(0, 5, 0, 0, 1, 0, 100) == nil then obj.x = 42.0 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            Some(&phys),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.x, 42.0);
    }

    #[test]
    fn raycast_lua_without_a_physics_world_returns_nil_instead_of_panicking() {
        // Hors mode Play (`self.physics == None`, cf. `AppState::advance_play`) : les
        // scripts qui appellent `raycast`/`overlap_sphere` ne doivent pas planter,
        // juste ne rien trouver.
        let lua = Lua::new();
        let src = "if raycast(0,0,0, 1,0,0, 10) == nil then obj.x = 1.0 end\n\
                   obj.y = overlap_sphere(0, 0, 0, 5)";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.x, 1.0);
        assert_eq!(t.position.y, 0.0);
    }

    /// Deux sphères statiques (proche/loin) — sert au « cône de vision » scripté en
    /// Lua (détection de proximité avant le test d'angle/ligne de vue).
    fn near_and_far_spheres_scene() -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Proche".into(),
            mesh: MeshKind::Sphere,
            transform: Transform::from_pos(Vec3::new(1.0, 0.0, 0.0)).with_scale(Vec3::splat(0.2)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Loin".into(),
            mesh: MeshKind::Sphere,
            transform: Transform::from_pos(Vec3::new(20.0, 0.0, 0.0)).with_scale(Vec3::splat(0.2)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        scene
    }

    #[test]
    fn overlap_sphere_lua_counts_only_colliders_within_radius() {
        // Brique du « cône de vision » : compter ce qui se trouve à portée avant même
        // de tester l'angle — `overlap_sphere()` renvoie un compte, pas une liste
        // d'objets (les scripts n'ont pas de handle direct sur les autres objets,
        // seulement des positions via `find_tag`).
        let scene = near_and_far_spheres_scene();
        let phys = crate::runtime::physics::Physics::build(&scene);
        let lua = Lua::new();
        let src = "obj.x = overlap_sphere(0, 0, 0, 2)\n\
                   obj.y = overlap_sphere(0, 0, 0, 25)";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            Some(&phys),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.x, 1.0, "seule la sphère proche est à 2 m");
        assert_eq!(t.position.y, 2.0, "les deux sphères sont à moins de 25 m");
    }

    #[test]
    fn obj_exited_is_true_only_on_the_tick_a_trigger_zone_is_left() {
        // Symétrique de `script_reacts_to_trigger` (`obj.triggered`) :
        // `obj.exited` doit être vrai exactement le tick où le contact
        // cesse, pas avant (encore en contact) ni après (déjà retombé à faux ailleurs).
        let lua = Lua::new();
        let src = "if obj.exited then obj.y = 9.0 end";
        let func = lua.load(src).into_function().unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let input = PlayerInput::default();
        // Encore en contact (`triggered`) : pas de sortie, `exited` doit être faux.
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            true,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.y, 0.0);
        // Le contact vient de cesser : `exited` doit être vrai ce tick.
        run_script(
            &lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &input,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut false,
            &mut std::collections::HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            true,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(t.position.y, 9.0);
    }
}
