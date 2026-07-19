//! Backend Lua du player web (Sprint 137) : `mlua` (bindings C) ne compile pas pour
//! `wasm32-unknown-unknown` (cf. Cargo.toml, `scripting.rs`), remplacé ici par
//! `rilua`, un interpréteur Lua 5.1 pur Rust qui cible cette plateforme.
//!
//! Contrairement à `mlua`, l'API de `rilua` n'expose que des pointeurs de fonction
//! nus (`RustFn = fn(&mut LuaState) -> LuaResult<u32>`), pas de fermetures
//! capturantes : impossible d'y faire ce que `scripting.rs` fait pour chaque
//! fonction hôte (`move |_, ..| { events_out.push(..) }`). L'état partagé entre les
//! ~15 fonctions hôtes et `run_script_web` transite donc par un accumulateur
//! `thread_local!` (`ScriptAccum`, entièrement possédé — aucun `unsafe` requis) plutôt
//! que par capture, à l'exception de `physics` (un emprunt `&Physics`, porté par
//! `PHYSICS_PTR` le temps de l'appel, cf. sa doc).
//!
//! Port volontairement parallèle à `scripting::run_script` : même signature, mêmes
//! noms d'API côté script (`obj.*`, `emit`, `spawn`, `save.get/set`…) — un script
//! écrit pour l'un tourne à l'identique sur l'autre (cf. les tests différentiels
//! dans `mod tests`). Pas de breakpoints ici (fonctionnalité éditeur ; le player web
//! n'a pas d'éditeur), et Lua 5.1 (pas 5.4) : pas de `goto`, `//`, opérateurs bit à
//! bit natifs ni entiers — l'API du moteur (`obj.*`, `emit`, `spawn`…) reste du Lua
//! commun aux deux versions.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use glam::{EulerRot, Quat, Vec3};
use rilua::vm::state::LuaState;
use rilua::{FromLua, Function, Lua, LuaApi, LuaApiMut, LuaResult, Table, Val, runtime_error};

use super::PlayerInput;
use crate::runtime::physics::Physics;
use crate::scene::{Transform, canonical_euler_xyz};

/// Hash stable d'une source de script, clé du cache de chunks compilés — copie de
/// `scripting::script_key` (le module `scripting` est `cfg(not(wasm32))`, cette
/// fonction de 3 lignes est plus simple à dupliquer qu'à sortir du cfg).
pub(super) fn script_key(src: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// Ancre une fonction compilée dans la table `registry` de `rilua` (`LuaState::
/// registry`, racine scannée par `mark_roots` — comme les globales) — **sans ça**,
/// une `Function` gardée uniquement dans le `HashMap<u64, Function>` côté Rust
/// (`AppState::script_cache_web`) n'est visible d'aucune racine connue du GC : une
/// collecte complète (`maybe_collect_garbage`) la ramasse, et le prochain
/// `lua.call_function` sur ce handle périmé échoue avec « invalid function
/// reference » — constaté en prod juste après le passage aux collectes complètes
/// périodiques (Sprint 137). La clé (hex du hash) n'entre jamais en collision avec
/// un nom de champ de script normal, donc partage sans risque la même table que
/// d'éventuelles autres entrées de registre (mécanismes internes de `rilua`).
pub(super) fn anchor_compiled_function(lua: &mut Lua, key: u64, func: Function) -> LuaResult<()> {
    let registry = Table::from_gc_ref(lua.state_mut().registry);
    let reg_key = lua.create_string(format!("rustee_script_{key:016x}").as_bytes());
    lua.table_raw_set(&registry, reg_key, Val::Function(func.gc_ref()))
}

// ---------------------------------------------------------------------------
// État partagé entre `run_script_web` et les fonctions hôtes
// ---------------------------------------------------------------------------

/// Accumulateur partagé entre `run_script_web` et les fonctions hôtes. Entièrement
/// possédé (`'static`) — reconstruit/vidé à chaque appel de `run_script_web`, jamais
/// partagé entre deux scripts (l'exécution est synchrone et non ré-entrante : un
/// script ne peut pas en invoquer un autre).
#[derive(Default)]
struct ScriptAccum {
    destroy: bool,
    vib: Vec<f32>,
    reverb: Vec<f32>,
    /// Vie du HUD : initialisée à `health_out` (ou 1.0) avant l'appel, modifiée par
    /// `set_health`/`damage` — même sémantique « base + dégâts cumulés » que
    /// `scripting::run_script`.
    hud: f32,
    debug_lines: Vec<(Vec3, Vec3, [f32; 3])>,
    events_out: Vec<String>,
    spawn_out: Vec<(String, Vec3)>,
    /// Demandes `add_item(kind, n)` accumulées (clé texte, nombre), décodées en
    /// `(ItemKind, u32)` après l'appel via `ItemKind::from_key` — cf.
    /// `scripting::run_script` (même contrat).
    item_add: Vec<(String, u32)>,
    received_events: HashSet<String>,
    tagged: Vec<(String, Vec3)>,
    vars: HashMap<String, f64>,
}

thread_local! {
    static ACCUM: RefCell<ScriptAccum> = RefCell::new(ScriptAccum::default());
    /// Emprunt de `Physics` pour `raycast`/`overlap_sphere`, valide seulement
    /// pendant l'appel courant de `run_script_web` — posé juste avant l'exécution du
    /// chunk, retiré par `PhysicsGuard` (RAII, y compris en cas d'erreur `?`) avant
    /// que l'emprunt d'origine ne puisse expirer. Équivalent du `lua.scope` de
    /// `scripting::run_script` (fermetures scopées empruntant `&Physics`), impossible
    /// ici faute de fermetures capturantes côté `rilua`.
    static PHYSICS_PTR: Cell<*const Physics> = const { Cell::new(std::ptr::null()) };
    /// Compteur d'appels à `run_script_web`, pour espacer les collectes complètes
    /// (`GC_PERIOD`) — cf. la doc de `maybe_collect_garbage`.
    static GC_TICKS: Cell<u32> = const { Cell::new(0) };
}

/// Nombre d'appels à `run_script_web` entre deux collectes complètes du GC `rilua`
/// (un appel par objet scripté par tick — quelques secondes de marge avec une poignée
/// d'objets scriptés à 60 Hz). Cf. `maybe_collect_garbage`.
const GC_PERIOD: u32 = 240;

/// Déclenche une collecte complète (`Lua::gc_collect`) tous les `GC_PERIOD` appels.
/// Le GC incrémental de `rilua` est désactivé une fois pour toutes à la création de
/// `Lua` (`AppState::new`, `lua_web.gc_stop()`) : l'API bas niveau utilisée par ce
/// module (`table_raw_set`/`create_string`) n'applique pas le write barrier qu'exige
/// un GC incrémental (une valeur fraîchement écrite dans une table déjà scannée peut
/// être ramassée avant d'être relue — constaté en prod, cf. Sprint 137). Une collecte
/// **complète** repart de zéro (marquage entier depuis les racines) et n'a pas cet
/// écueil ; l'appeler périodiquement plutôt qu'à chaque tick borne son coût.
fn maybe_collect_garbage(lua: &mut Lua) {
    let n = GC_TICKS.with(|c| {
        let n = c.get() + 1;
        c.set(n);
        n
    });
    if n >= GC_PERIOD {
        GC_TICKS.with(|c| c.set(0));
        if let Err(e) = lua.gc_collect() {
            log::warn!("Collecte GC Lua (web) : {e}");
        }
        // `gc_collect` (`full_gc`) réinitialise `gc_threshold` en sortie
        // (`update_threshold`), ce qui réactiverait le pas incrémental (et donc le
        // bug de write barrier ci-dessus) dès la prochaine allocation — on redésactive
        // immédiatement, comme à la création de `Lua` (`AppState::new`).
        lua.gc_stop();
    }
}

struct PhysicsGuard;
impl Drop for PhysicsGuard {
    fn drop(&mut self) {
        PHYSICS_PTR.with(|p| p.set(std::ptr::null()));
    }
}

fn cur_physics() -> Option<&'static Physics> {
    PHYSICS_PTR.with(|p| {
        let ptr = p.get();
        if ptr.is_null() {
            None
        } else {
            // Sûr : `ptr` n'est non-null que le temps de l'appel de `run_script_web`
            // qui l'a posé (`PhysicsGuard` le vide avant que l'emprunt d'origine,
            // garanti valide pour cette durée, ne puisse expirer).
            Some(unsafe { &*ptr })
        }
    })
}

// ---------------------------------------------------------------------------
// Aides table/champ (au niveau `Lua`, hors des fonctions hôtes)
// ---------------------------------------------------------------------------

fn set_num(lua: &mut Lua, table: &Table, name: &str, v: f64) -> LuaResult<()> {
    let key = lua.create_string(name.as_bytes());
    lua.table_raw_set(table, key, Val::Num(v))
}

fn set_str(lua: &mut Lua, table: &Table, name: &str, v: &str) -> LuaResult<()> {
    let key = lua.create_string(name.as_bytes());
    let val = lua.create_string(v.as_bytes());
    lua.table_raw_set(table, key, val)
}

fn set_bool(lua: &mut Lua, table: &Table, name: &str, v: bool) -> LuaResult<()> {
    let key = lua.create_string(name.as_bytes());
    lua.table_raw_set(table, key, Val::Bool(v))
}

fn get_num(lua: &mut Lua, table: &Table, name: &str) -> LuaResult<f64> {
    let key = lua.create_string(name.as_bytes());
    match lua.table_raw_get(table, key)? {
        Val::Num(n) => Ok(n),
        other => Err(runtime_error(format!(
            "obj.{name} : nombre attendu, reçu {}",
            other.type_name()
        ))),
    }
}

fn get_str(lua: &mut Lua, table: &Table, name: &str) -> LuaResult<String> {
    let key = lua.create_string(name.as_bytes());
    let val = lua.table_raw_get(table, key)?;
    String::from_lua(val, lua)
}

fn set_table(lua: &mut Lua, table: &Table, name: &str, val: Table) -> LuaResult<()> {
    let key = lua.create_string(name.as_bytes());
    lua.table_raw_set(table, key, Val::Table(val.gc_ref()))
}

// ---------------------------------------------------------------------------
// Aides argument (dans les fonctions hôtes, au niveau `&mut LuaState`)
// ---------------------------------------------------------------------------

fn arg_num(state: &LuaState, i: usize) -> LuaResult<f64> {
    match state.stack_get(state.base + i) {
        Val::Num(n) => Ok(n),
        other => Err(runtime_error(format!(
            "argument {}: nombre attendu, reçu {}",
            i + 1,
            other.type_name()
        ))),
    }
}

fn arg_f32(state: &LuaState, i: usize) -> LuaResult<f32> {
    arg_num(state, i).map(|n| n as f32)
}

/// Nombre optionnel : `Nil`/absent → `default` (équivalent d'`Option<u32>` côté mlua).
fn arg_num_or(state: &LuaState, i: usize, default: f64) -> f64 {
    match state.stack_get(state.base + i) {
        Val::Num(n) => n,
        _ => default,
    }
}

fn arg_str(state: &mut LuaState, i: usize) -> LuaResult<String> {
    let val = state.stack_get(state.base + i);
    String::from_lua(val, state)
}

// ---------------------------------------------------------------------------
// Fonctions hôtes (RustFn = fn(&mut LuaState) -> LuaResult<u32>, pas de closures)
// ---------------------------------------------------------------------------

fn host_destroy(_state: &mut LuaState) -> LuaResult<u32> {
    ACCUM.with(|a| a.borrow_mut().destroy = true);
    Ok(0)
}

fn host_vibrate(state: &mut LuaState) -> LuaResult<u32> {
    let ms = arg_f32(state, 0)?;
    ACCUM.with(|a| a.borrow_mut().vib.push(ms));
    Ok(0)
}

fn host_reverb(state: &mut LuaState) -> LuaResult<u32> {
    let mix = arg_f32(state, 0)?.clamp(0.0, 1.0);
    ACCUM.with(|a| a.borrow_mut().reverb.push(mix));
    Ok(0)
}

fn host_set_health(state: &mut LuaState) -> LuaResult<u32> {
    let v = arg_f32(state, 0)?.clamp(0.0, 1.0);
    ACCUM.with(|a| a.borrow_mut().hud = v);
    Ok(0)
}

fn host_damage(state: &mut LuaState) -> LuaResult<u32> {
    let v = arg_f32(state, 0)?;
    ACCUM.with(|a| {
        let mut a = a.borrow_mut();
        a.hud = (a.hud - v).clamp(0.0, 1.0);
    });
    Ok(0)
}

fn host_emit(state: &mut LuaState) -> LuaResult<u32> {
    let name = arg_str(state, 0)?;
    ACCUM.with(|a| a.borrow_mut().events_out.push(name));
    Ok(0)
}

fn host_on_event(state: &mut LuaState) -> LuaResult<u32> {
    let name = arg_str(state, 0)?;
    let hit = ACCUM.with(|a| a.borrow().received_events.contains(&name));
    state.push(Val::Bool(hit));
    Ok(1)
}

fn host_spawn(state: &mut LuaState) -> LuaResult<u32> {
    let prefab = arg_str(state, 0)?;
    let x = arg_f32(state, 1)?;
    let y = arg_f32(state, 2)?;
    let z = arg_f32(state, 3)?;
    ACCUM.with(|a| {
        a.borrow_mut().spawn_out.push((prefab, Vec3::new(x, y, z)));
    });
    Ok(0)
}

fn host_add_item(state: &mut LuaState) -> LuaResult<u32> {
    let kind = arg_str(state, 0)?;
    let n = arg_num(state, 1)? as u32;
    ACCUM.with(|a| a.borrow_mut().item_add.push((kind, n)));
    Ok(0)
}

fn host_find_tag(state: &mut LuaState) -> LuaResult<u32> {
    let tag = arg_str(state, 0)?;
    let hits: Vec<Vec3> = ACCUM.with(|a| {
        a.borrow()
            .tagged
            .iter()
            .filter(|(t, _)| t == &tag)
            .map(|(_, pos)| *pos)
            .collect()
    });
    let out = alloc_table(state);
    for (n, pos) in hits.iter().enumerate() {
        let entry = alloc_table(state);
        let kx = str_val(state, "x");
        entry.raw_set(state, kx, Val::Num(pos.x as f64))?;
        let ky = str_val(state, "y");
        entry.raw_set(state, ky, Val::Num(pos.y as f64))?;
        let kz = str_val(state, "z");
        entry.raw_set(state, kz, Val::Num(pos.z as f64))?;
        out.raw_set(state, Val::Num((n + 1) as f64), Val::Table(entry.gc_ref()))?;
    }
    state.push(Val::Table(out.gc_ref()));
    Ok(1)
}

fn host_save_get(state: &mut LuaState) -> LuaResult<u32> {
    let key = arg_str(state, 0)?;
    let val = ACCUM.with(|a| a.borrow().vars.get(&key).copied());
    match val {
        Some(v) => state.push(Val::Num(v)),
        None => state.push(Val::Nil),
    }
    Ok(1)
}

fn host_save_set(state: &mut LuaState) -> LuaResult<u32> {
    let key = arg_str(state, 0)?;
    let val = arg_num(state, 1)?;
    ACCUM.with(|a| {
        a.borrow_mut().vars.insert(key, val);
    });
    Ok(0)
}

fn host_debug_line(state: &mut LuaState) -> LuaResult<u32> {
    let vals: Vec<f32> = (0..9)
        .map(|i| arg_f32(state, i))
        .collect::<LuaResult<_>>()?;
    let a = Vec3::new(vals[0], vals[1], vals[2]);
    let b = Vec3::new(vals[3], vals[4], vals[5]);
    let color = [vals[6], vals[7], vals[8]];
    ACCUM.with(|acc| acc.borrow_mut().debug_lines.push((a, b, color)));
    Ok(0)
}

fn host_raycast(state: &mut LuaState) -> LuaResult<u32> {
    let ox = arg_f32(state, 0)?;
    let oy = arg_f32(state, 1)?;
    let oz = arg_f32(state, 2)?;
    let dx = arg_f32(state, 3)?;
    let dy = arg_f32(state, 4)?;
    let dz = arg_f32(state, 5)?;
    let max_dist = arg_f32(state, 6)?;
    let mask = arg_num_or(state, 7, u32::MAX as f64) as u32;
    let hit = cur_physics()
        .and_then(|p| p.raycast(Vec3::new(ox, oy, oz), Vec3::new(dx, dy, dz), max_dist, mask));
    match hit {
        Some(h) => {
            let out = alloc_table(state);
            let kx = str_val(state, "x");
            out.raw_set(state, kx, Val::Num(h.point.x as f64))?;
            let ky = str_val(state, "y");
            out.raw_set(state, ky, Val::Num(h.point.y as f64))?;
            let kz = str_val(state, "z");
            out.raw_set(state, kz, Val::Num(h.point.z as f64))?;
            let kdist = str_val(state, "dist");
            out.raw_set(state, kdist, Val::Num(h.distance as f64))?;
            state.push(Val::Table(out.gc_ref()));
        }
        None => state.push(Val::Nil),
    }
    Ok(1)
}

fn host_overlap_sphere(state: &mut LuaState) -> LuaResult<u32> {
    let x = arg_f32(state, 0)?;
    let y = arg_f32(state, 1)?;
    let z = arg_f32(state, 2)?;
    let radius = arg_f32(state, 3)?;
    let mask = arg_num_or(state, 4, u32::MAX as f64) as u32;
    let count = cur_physics()
        .map(|p| p.overlap_sphere(Vec3::new(x, y, z), radius, mask).len())
        .unwrap_or(0);
    state.push(Val::Num(count as f64));
    Ok(1)
}

/// Alloue une table Lua vide. Équivalent de `LuaApiMut::create_table`, qui a besoin
/// d'un `&mut Lua` — indisponible à l'intérieur d'une fonction hôte (`RustFn` ne
/// reçoit que `&mut LuaState`) ; même chemin, juste plus bas niveau.
fn alloc_table(state: &mut LuaState) -> Table {
    Table::from_gc_ref(state.gc.alloc_table(rilua::vm::table::Table::new()))
}

/// Interne une chaîne comme clé/valeur `Val::Str`. Nom volontairement court : appelé
/// à chaque champ construit dans les fonctions hôtes ci-dessus.
fn str_val(state: &mut LuaState, s: &str) -> Val {
    let r = state.gc.intern_string(s.as_bytes());
    Val::Str(r)
}

/// Convertit une `LuaResult` en `Result<_, String>` en propageant l'erreur —
/// `run_script_web` (appelé depuis `simulation.rs`, qui ne connaît pas `rilua`)
/// renvoie des erreurs en `String`, comme `mlua::Error` l'aurait fait via `Display`.
macro_rules! lua_try {
    ($e:expr) => {
        $e.map_err(|e| e.to_string())?
    };
}

/// Exécute le chunk Lua **déjà compilé** d'un objet, sur le backend `rilua`.
/// Symétrique de `scripting::run_script` (même contrat : expose `obj` (x,y,z,
/// rx,ry,rz en °, sx,sy,sz, r,g,b, tapped/touch_started/touching/touch_ended/
/// triggered/exited/anim), `dt`, `time`, `input`, `tilt`, `add_item(kind, n)`,
/// puis relit les champs modifiés) — cf. la doc de module pour ce qui diffère
/// en interne (pas de fermetures capturantes côté hôte).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_script_web(
    lua: &mut Lua,
    func: &Function,
    t: &mut Transform,
    color: &mut [f32; 3],
    anim: &mut Option<crate::scene::AnimationState>,
    dt: f32,
    time: f32,
    input: &PlayerInput,
    tapped: bool,
    touch_started: bool,
    touching: bool,
    touch_ended: bool,
    triggered: bool,
    events_in: &[String],
    events_out: &mut Vec<String>,
    tagged: &[(String, Vec3)],
    spawn_out: &mut Vec<(String, Vec3)>,
    item_add_out: &mut Vec<(crate::scene::ItemKind, u32)>,
    destroy_out: &mut bool,
    vars: &mut HashMap<String, f64>,
    vib_out: &mut Vec<f32>,
    health_out: &mut Option<f32>,
    debug_out: &mut Vec<(Vec3, Vec3, [f32; 3])>,
    exited: bool,
    physics: Option<&Physics>,
    reverb_out: &mut Vec<f32>,
) -> Result<(), String> {
    // Réinitialise l'accumulateur pour cet appel — jamais partagé entre deux
    // scripts (exécution synchrone, non ré-entrante).
    let base_health = health_out.unwrap_or(1.0);
    ACCUM.with(|a| {
        *a.borrow_mut() = ScriptAccum {
            hud: base_health,
            received_events: events_in.iter().cloned().collect(),
            tagged: tagged.to_vec(),
            vars: std::mem::take(vars),
            ..Default::default()
        };
    });
    // Emprunt de `physics` posé pour la durée de l'appel — `_physics_guard` le
    // retire (RAII) même si une erreur `?` interrompt la fonction plus bas, jamais
    // laissé pendre au-delà de ce que `physics: Option<&Physics>` garantit.
    PHYSICS_PTR.with(|p| p.set(physics.map_or(std::ptr::null(), |ph| ph as *const Physics)));
    let _physics_guard = PhysicsGuard;

    let (rx, ry, rz) = canonical_euler_xyz(t.rotation);
    let obj = lua.create_table();
    lua_try!(set_num(lua, &obj, "x", t.position.x as f64));
    lua_try!(set_num(lua, &obj, "y", t.position.y as f64));
    lua_try!(set_num(lua, &obj, "z", t.position.z as f64));
    lua_try!(set_num(lua, &obj, "rx", rx.to_degrees() as f64));
    lua_try!(set_num(lua, &obj, "ry", ry.to_degrees() as f64));
    lua_try!(set_num(lua, &obj, "rz", rz.to_degrees() as f64));
    lua_try!(set_num(lua, &obj, "sx", t.scale.x as f64));
    lua_try!(set_num(lua, &obj, "sy", t.scale.y as f64));
    lua_try!(set_num(lua, &obj, "sz", t.scale.z as f64));
    lua_try!(set_num(lua, &obj, "r", color[0] as f64));
    lua_try!(set_num(lua, &obj, "g", color[1] as f64));
    lua_try!(set_num(lua, &obj, "b", color[2] as f64));
    lua_try!(set_bool(lua, &obj, "tapped", tapped));
    lua_try!(set_bool(lua, &obj, "touch_started", touch_started));
    lua_try!(set_bool(lua, &obj, "touching", touching));
    lua_try!(set_bool(lua, &obj, "touch_ended", touch_ended));
    lua_try!(set_bool(lua, &obj, "triggered", triggered));
    lua_try!(set_bool(lua, &obj, "exited", exited));
    lua_try!(set_str(
        lua,
        &obj,
        "anim",
        anim.as_ref().map(|a| a.clip.as_str()).unwrap_or("")
    ));
    lua_try!(lua.table_set_function(&obj, "destroy", host_destroy));

    let input_tbl = lua.create_table();
    lua_try!(set_num(lua, &input_tbl, "jx", input.joy.0 as f64));
    lua_try!(set_num(lua, &input_tbl, "jy", input.joy.1 as f64));
    let btns = lua.create_table();
    for name in &input.buttons {
        lua_try!(set_bool(lua, &btns, name, true));
    }
    lua_try!(set_table(lua, &input_tbl, "btn", btns));

    let tilt = lua.create_table();
    lua_try!(set_num(lua, &tilt, "x", input.tilt.0 as f64));
    lua_try!(set_num(lua, &tilt, "y", input.tilt.1 as f64));

    let debug_api = lua.create_table();
    lua_try!(lua.table_set_function(&debug_api, "line", host_debug_line));

    let save_api = lua.create_table();
    lua_try!(lua.table_set_function(&save_api, "get", host_save_get));
    lua_try!(lua.table_set_function(&save_api, "set", host_save_set));

    lua_try!(lua.set_global("obj", obj));
    lua_try!(lua.set_global("dt", dt as f64));
    lua_try!(lua.set_global("time", time as f64));
    lua_try!(lua.set_global("input", input_tbl));
    lua_try!(lua.set_global("tilt", tilt));
    lua_try!(lua.set_global("debug", debug_api));
    lua_try!(lua.set_global("save", save_api));
    lua_try!(lua.register_function("vibrate", host_vibrate));
    lua_try!(lua.register_function("reverb", host_reverb));
    lua_try!(lua.register_function("set_health", host_set_health));
    lua_try!(lua.register_function("damage", host_damage));
    lua_try!(lua.register_function("emit", host_emit));
    lua_try!(lua.register_function("on_event", host_on_event));
    lua_try!(lua.register_function("spawn", host_spawn));
    lua_try!(lua.register_function("add_item", host_add_item));
    lua_try!(lua.register_function("find_tag", host_find_tag));
    lua_try!(lua.register_function("raycast", host_raycast));
    lua_try!(lua.register_function("overlap_sphere", host_overlap_sphere));

    let call_result = lua.call_function(func, &[]).map(|_| ());

    // `vars` recopié même en cas d'erreur du script (cf. `scripting::run_script`) :
    // un script qui plante après avoir écrit dans `save` ne doit pas perdre ces
    // écritures.
    *vars = ACCUM.with(|a| a.borrow().vars.clone());
    lua_try!(call_result);

    ACCUM.with(|a| {
        let a = a.borrow();
        if a.destroy {
            *destroy_out = true;
        }
        events_out.extend(a.events_out.iter().cloned());
        spawn_out.extend(a.spawn_out.iter().cloned());
        for (kind, n) in &a.item_add {
            if let Some(kind) = crate::scene::ItemKind::from_key(kind) {
                item_add_out.push((kind, *n));
            }
        }
        vib_out.extend(a.vib.iter().copied());
        reverb_out.extend(a.reverb.iter().copied());
        *health_out = Some(a.hud);
        debug_out.extend(a.debug_lines.iter().copied());
    });

    t.position = Vec3::new(
        lua_try!(get_num(lua, &obj, "x")) as f32,
        lua_try!(get_num(lua, &obj, "y")) as f32,
        lua_try!(get_num(lua, &obj, "z")) as f32,
    );
    let rx = lua_try!(get_num(lua, &obj, "rx")) as f32;
    let ry = lua_try!(get_num(lua, &obj, "ry")) as f32;
    let rz = lua_try!(get_num(lua, &obj, "rz")) as f32;
    t.rotation = Quat::from_euler(
        EulerRot::XYZ,
        rx.to_radians(),
        ry.to_radians(),
        rz.to_radians(),
    );
    t.scale = Vec3::new(
        lua_try!(get_num(lua, &obj, "sx")) as f32,
        lua_try!(get_num(lua, &obj, "sy")) as f32,
        lua_try!(get_num(lua, &obj, "sz")) as f32,
    );
    *color = [
        lua_try!(get_num(lua, &obj, "r")) as f32,
        lua_try!(get_num(lua, &obj, "g")) as f32,
        lua_try!(get_num(lua, &obj, "b")) as f32,
    ];
    if let Some(state) = anim {
        let requested = lua_try!(get_str(lua, &obj, "anim"));
        if !requested.is_empty() {
            state.set_clip(requested);
        }
    }
    maybe_collect_garbage(lua);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{MeshKind, Scene, SceneObject};

    #[allow(clippy::too_many_arguments)]
    fn run(
        lua: &mut Lua,
        src: &str,
        t: &mut Transform,
        color: &mut [f32; 3],
        input: &PlayerInput,
        tapped: bool,
        touch_started: bool,
        touching: bool,
        touch_ended: bool,
        triggered: bool,
        events_in: &[String],
        events_out: &mut Vec<String>,
        tagged: &[(String, Vec3)],
        vars: &mut HashMap<String, f64>,
        debug_out: &mut Vec<(Vec3, Vec3, [f32; 3])>,
        exited: bool,
        physics: Option<&Physics>,
    ) -> Result<(), String> {
        let func = lua.load(src).map_err(|e| e.to_string())?;
        let mut spawn_out = Vec::new();
        let mut item_add_out = Vec::new();
        let mut destroy_out = false;
        let mut vib_out = Vec::new();
        let mut health_out = None;
        let mut reverb_out = Vec::new();
        run_script_web(
            lua,
            &func,
            t,
            color,
            &mut None,
            0.016,
            0.0,
            input,
            tapped,
            touch_started,
            touching,
            touch_ended,
            triggered,
            events_in,
            events_out,
            tagged,
            &mut spawn_out,
            &mut item_add_out,
            &mut destroy_out,
            vars,
            &mut vib_out,
            &mut health_out,
            debug_out,
            exited,
            physics,
            &mut reverb_out,
        )
    }

    #[test]
    fn script_reads_mobile_input() {
        let mut lua = Lua::new().unwrap();
        let src = "obj.x = obj.x + input.jx; if input.btn.B1 then obj.y = 5 end";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let mut input = PlayerInput {
            joy: (0.5, 0.0),
            ..Default::default()
        };
        input.buttons.insert("B1".into());
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &input,
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut Vec::new(),
            false,
            None,
        )
        .unwrap();
        assert!((t.position.x - 0.5).abs() < 1e-5);
        assert!((t.position.y - 5.0).abs() < 1e-5);
    }

    #[test]
    fn script_emit_lands_in_events_out_and_on_event_reads_events_in() {
        let mut lua = Lua::new().unwrap();
        let src = "emit('porte'); if on_event('score:3') then obj.y = 9 end; \
                   if on_event('porte') then obj.x = 9 end";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut events_out = Vec::new();
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &["score:3".to_string()],
            &mut events_out,
            &[],
            &mut HashMap::new(),
            &mut Vec::new(),
            false,
            None,
        )
        .unwrap();
        assert_eq!(events_out, vec!["porte".to_string()]);
        assert!((t.position.y - 9.0).abs() < 1e-5);
        assert!(t.position.x.abs() < 1e-5);
    }

    #[test]
    fn find_tag_returns_positions_of_matching_visible_objects() {
        let mut lua = Lua::new().unwrap();
        let src = "local hits = find_tag('ennemi'); obj.x = #hits; \
                   if #hits > 0 then obj.y = hits[1].y end";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let tagged = vec![
            ("ennemi".to_string(), Vec3::new(1.0, 2.0, 3.0)),
            ("ennemi".to_string(), Vec3::new(4.0, 5.0, 6.0)),
            ("allié".to_string(), Vec3::new(9.0, 9.0, 9.0)),
        ];
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &tagged,
            &mut HashMap::new(),
            &mut Vec::new(),
            false,
            None,
        )
        .unwrap();
        assert_eq!(t.position.x, 2.0);
        assert_eq!(t.position.y, 2.0);
    }

    #[test]
    fn script_save_set_persists_and_save_get_reads_it_back() {
        let mut lua = Lua::new().unwrap();
        let src = "save.set('pv_max', 42.0); obj.x = save.get('pv_max')";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut vars = HashMap::new();
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut vars,
            &mut Vec::new(),
            false,
            None,
        )
        .unwrap();
        assert_eq!(t.position.x, 42.0);
        assert_eq!(vars.get("pv_max"), Some(&42.0));
    }

    #[test]
    fn script_debug_line_is_read_back_into_debug_out() {
        let mut lua = Lua::new().unwrap();
        let src = "debug.line(0,0,0, 1,2,3, 1,0,0); debug.line(-1,0,0, 0,0,0, 0,1,0)";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut debug_out = Vec::new();
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut debug_out,
            false,
            None,
        )
        .unwrap();
        assert_eq!(debug_out.len(), 2);
        assert_eq!(
            debug_out[0],
            (Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0), [1.0, 0.0, 0.0])
        );
    }

    #[test]
    fn obj_exited_is_true_only_on_the_tick_a_trigger_zone_is_left() {
        let mut lua = Lua::new().unwrap();
        let src = "if obj.exited then obj.y = 9.0 end";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0, 1.0, 1.0];
        let input = PlayerInput::default();
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &input,
            false,
            false,
            false,
            false,
            true,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut Vec::new(),
            false,
            None,
        )
        .unwrap();
        assert_eq!(t.position.y, 0.0);
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &input,
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut Vec::new(),
            true,
            None,
        )
        .unwrap();
        assert_eq!(t.position.y, 9.0);
    }

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
        let scene = floor_only_scene();
        let phys = crate::runtime::physics::Physics::build(&scene);
        let mut lua = Lua::new().unwrap();
        let src = "local hit = raycast(0, 5, 0, 0, -1, 0, 100)\n\
                   if hit then\n\
                     obj.y = hit.dist\n\
                     debug.line(0, 5, 0, hit.x, hit.y, hit.z, 0, 1, 0)\n\
                   end";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut debug_out = Vec::new();
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut debug_out,
            false,
            Some(&phys),
        )
        .unwrap();
        assert!(
            (t.position.y - 5.5).abs() < 0.05,
            "hit.dist doit valoir ~5.5 m (y={})",
            t.position.y
        );
        assert_eq!(debug_out.len(), 1);
    }

    #[test]
    fn raycast_lua_returns_nil_when_nothing_is_hit() {
        let scene = floor_only_scene();
        let phys = crate::runtime::physics::Physics::build(&scene);
        let mut lua = Lua::new().unwrap();
        let src = "if raycast(0, 5, 0, 0, 1, 0, 100) == nil then obj.x = 42.0 end";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut Vec::new(),
            false,
            Some(&phys),
        )
        .unwrap();
        assert_eq!(t.position.x, 42.0);
    }

    #[test]
    fn raycast_lua_without_a_physics_world_returns_nil_instead_of_panicking() {
        let mut lua = Lua::new().unwrap();
        let src = "if raycast(0,0,0, 1,0,0, 10) == nil then obj.x = 1.0 end\n\
                   obj.y = overlap_sphere(0, 0, 0, 5)";
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        run(
            &mut lua,
            src,
            &mut t,
            &mut col,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut HashMap::new(),
            &mut Vec::new(),
            false,
            None,
        )
        .unwrap();
        assert_eq!(t.position.x, 1.0);
        assert_eq!(t.position.y, 0.0);
    }

    #[test]
    fn script_key_stable_and_distinct() {
        assert_eq!(script_key("obj.x = 1"), script_key("obj.x = 1"));
        assert_ne!(script_key("obj.x = 1"), script_key("obj.x = 2"));
    }

    /// Régression (constaté en prod, Sprint 137) : `Table::raw_set`/`create_string`
    /// (l'API bas niveau utilisée par ce module) n'appliquent pas le write barrier
    /// qu'exige le GC incrémental de `rilua` — une valeur fraîchement écrite dans une
    /// table déjà scannée (ex. `obj.anim = "Walk"`) pouvait être ramassée avant d'être
    /// relue, dès qu'un cycle incrémental se déclenchait pendant l'exécution d'un
    /// script (« string expected, got collected string »). Boucle sur bien plus que
    /// `GC_PERIOD` appels, GC incrémental désactivé comme en production
    /// (`AppState::new`, `lua_web.gc_stop()`) — sans le correctif (désactivation +
    /// collectes complètes périodiques qui se redésactivent), ce test échoue en
    /// quelques dizaines d'itérations sur l'ancien comportement de rilua.
    #[test]
    fn many_script_calls_never_hit_a_gc_collected_string() {
        let mut lua = Lua::new().unwrap();
        lua.gc_stop();
        let src = "local t = time % 4.0\n\
                   if t < 3.0 then obj.anim = 'Walk' else obj.anim = 'Idle' end";
        let mut anim = Some(crate::scene::AnimationState {
            clip: "Idle".into(),
            ..Default::default()
        });
        for i in 0..(GC_PERIOD * 3) {
            let mut t = Transform::from_pos(Vec3::ZERO);
            let mut col = [1.0; 3];
            let func = lua.load(src).unwrap();
            let mut spawn_out = Vec::new();
            let mut destroy_out = false;
            let mut vib_out = Vec::new();
            let mut health_out = None;
            let mut reverb_out = Vec::new();
            let mut debug_out = Vec::new();
            run_script_web(
                &mut lua,
                &func,
                &mut t,
                &mut col,
                &mut anim,
                0.016,
                i as f32 * 0.016,
                &PlayerInput::default(),
                false,
                false,
                false,
                false,
                false,
                &[],
                &mut Vec::new(),
                &[],
                &mut spawn_out,
                &mut Vec::new(),
                &mut destroy_out,
                &mut HashMap::new(),
                &mut vib_out,
                &mut health_out,
                &mut debug_out,
                false,
                None,
                &mut reverb_out,
            )
            .unwrap_or_else(|e| panic!("itération {i} : {e}"));
        }
    }

    /// Régression (constaté en prod juste après le correctif ci-dessus, Sprint 137) :
    /// un `Function` compilé une fois puis réutilisé d'un tick à l'autre — comme le
    /// fait `AppState::script_cache_web` côté `simulation.rs` — doit rester appelable
    /// après une collecte complète, à condition d'avoir été ancré via
    /// `anchor_compiled_function`. Sans l'ancrage, `lua.call_function` sur ce handle
    /// échoue avec « invalid function reference » dès la première collecte : le GC de
    /// `rilua` ne connaît que ses propres racines (globales, pile, `registry`), pas un
    /// `HashMap<u64, Function>` gardé côté Rust en dehors de tout ça.
    #[test]
    fn a_cached_function_survives_a_full_gc_once_anchored_in_the_registry() {
        let mut lua = Lua::new().unwrap();
        lua.gc_stop();
        let src = "obj.x = obj.x + 1";
        let func = lua.load(src).unwrap();
        anchor_compiled_function(&mut lua, script_key(src), func).unwrap();

        // Beaucoup d'allocations sans lien avec `func` (comme d'autres scripts qui
        // tournent en parallèle) pour donner à la collecte complète de quoi ramasser.
        for _ in 0..1000 {
            let _ = lua.create_table();
        }
        lua.gc_collect().unwrap();

        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        run_script_web(
            &mut lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut false,
            &mut HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .expect("la fonction ancrée doit rester valide après une collecte complète");
        assert_eq!(t.position.x, 1.0);
    }

    #[test]
    fn destroy_sets_visible_false_via_method_call_syntax() {
        let mut lua = Lua::new().unwrap();
        let src = "obj:destroy()";
        let func = lua.load(src).unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut destroy_out = false;
        run_script_web(
            &mut lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut destroy_out,
            &mut HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(destroy_out);
    }

    #[test]
    fn spawn_accumulates_requests_with_position() {
        let mut lua = Lua::new().unwrap();
        let src = "spawn('asset-id://prefab1', 1, 2, 3)";
        let func = lua.load(src).unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut spawn_out = Vec::new();
        run_script_web(
            &mut lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut spawn_out,
            &mut Vec::new(),
            &mut false,
            &mut HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            spawn_out,
            vec![("asset-id://prefab1".to_string(), Vec3::new(1.0, 2.0, 3.0))]
        );
    }

    #[test]
    fn add_item_lands_in_item_add_out() {
        // `add_item("potion", 2)` doit atterrir dans `item_add_out`, décodé en
        // `(ItemKind, u32)` via `ItemKind::from_key` — même contrat que
        // `scripting::run_script`.
        let mut lua = Lua::new().unwrap();
        let src = "add_item('potion', 2)";
        let func = lua.load(src).unwrap();
        let mut t = Transform::from_pos(Vec3::ZERO);
        let mut col = [1.0; 3];
        let mut item_add_out = Vec::new();
        run_script_web(
            &mut lua,
            &func,
            &mut t,
            &mut col,
            &mut None,
            0.016,
            0.0,
            &PlayerInput::default(),
            false,
            false,
            false,
            false,
            false,
            &[],
            &mut Vec::new(),
            &[],
            &mut Vec::new(),
            &mut item_add_out,
            &mut false,
            &mut HashMap::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
            false,
            None,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(item_add_out, vec![(crate::scene::ItemKind::Potion, 2)]);
    }

    // -----------------------------------------------------------------------
    // Tests différentiels : même script, même état initial, exécuté sur les deux
    // backends (`mlua` natif et `rilua` web) — doivent produire le même résultat.
    // N'exécutable qu'en natif (`mlua` est `cfg(not(wasm32))`) : ces tests ne
    // tournent donc que via `cargo test`, jamais dans le player web lui-même — c'est
    // voulu, ils valident la parité des deux backends une fois pour toutes ici,
    // plutôt qu'un test à faire tourner dans un navigateur.
    #[cfg(not(target_arch = "wasm32"))]
    mod differential {
        use super::*;
        use crate::app::scripting as native;
        use mlua::Lua as NativeLua;

        #[allow(clippy::too_many_arguments)]
        fn run_native(src: &str, t: &mut Transform, col: &mut [f32; 3]) {
            let lua = NativeLua::new();
            let func = lua.load(src).into_function().unwrap();
            native::run_script(
                &lua,
                &func,
                t,
                col,
                &mut None,
                0.016,
                0.0,
                &PlayerInput::default(),
                false,
                false,
                false,
                false,
                false,
                &[],
                &mut Vec::new(),
                &[],
                &mut Vec::new(),
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
        }

        fn run_web(src: &str, t: &mut Transform, col: &mut [f32; 3]) {
            let mut lua = Lua::new().unwrap();
            let func = lua.load(src).unwrap();
            run_script_web(
                &mut lua,
                &func,
                t,
                col,
                &mut None,
                0.016,
                0.0,
                &PlayerInput::default(),
                false,
                false,
                false,
                false,
                false,
                &[],
                &mut Vec::new(),
                &[],
                &mut Vec::new(),
                &mut Vec::new(),
                &mut false,
                &mut HashMap::new(),
                &mut Vec::new(),
                &mut None,
                &mut Vec::new(),
                false,
                None,
                &mut Vec::new(),
            )
            .unwrap();
        }

        #[test]
        fn arithmetic_and_trig_script_matches_between_backends() {
            let src = "obj.x = obj.x + math.sin(1.5) * 0.6\n\
                       obj.y = math.cos(0.3) + 2\n\
                       obj.rz = obj.rz + 15.0";
            let mut t_native = Transform::from_pos(Vec3::ZERO);
            let mut t_web = Transform::from_pos(Vec3::ZERO);
            let mut col = [1.0; 3];
            run_native(src, &mut t_native, &mut col);
            run_web(src, &mut t_web, &mut col);
            assert!((t_native.position.x - t_web.position.x).abs() < 1e-5);
            assert!((t_native.position.y - t_web.position.y).abs() < 1e-5);
            assert!((t_native.rotation.dot(t_web.rotation)).abs() > 1.0 - 1e-5);
        }

        /// Variantes paramétrées de `run_native`/`run_web` pour les scripts
        /// officiels (`examples/scripts/*.lua`, Phase D2 sprint.19matin.md) :
        /// mêmes appels, mais `time`, `triggered` et `tagged` pilotables —
        /// `move_between_points` dépend de `time`, `trigger_door` de
        /// `obj.triggered`, `simple_enemy` de `find_tag`.
        #[allow(clippy::too_many_arguments)]
        fn run_native_at(
            src: &str,
            t: &mut Transform,
            time: f32,
            triggered: bool,
            tagged: &[(String, Vec3)],
        ) {
            let lua = NativeLua::new();
            let func = lua.load(src).into_function().unwrap();
            native::run_script(
                &lua,
                &func,
                t,
                &mut [1.0; 3],
                &mut None,
                0.016,
                time,
                &PlayerInput::default(),
                false,
                false,
                false,
                false,
                triggered,
                &[],
                &mut Vec::new(),
                tagged,
                &mut Vec::new(),
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
        }

        #[allow(clippy::too_many_arguments)]
        fn run_web_at(
            src: &str,
            t: &mut Transform,
            time: f32,
            triggered: bool,
            tagged: &[(String, Vec3)],
        ) {
            let mut lua = Lua::new().unwrap();
            let func = lua.load(src).unwrap();
            run_script_web(
                &mut lua,
                &func,
                t,
                &mut [1.0; 3],
                &mut None,
                0.016,
                time,
                &PlayerInput::default(),
                false,
                false,
                false,
                false,
                triggered,
                &[],
                &mut Vec::new(),
                tagged,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut false,
                &mut HashMap::new(),
                &mut Vec::new(),
                &mut None,
                &mut Vec::new(),
                false,
                None,
                &mut Vec::new(),
            )
            .unwrap();
        }

        #[test]
        fn official_scripts_match_between_backends() {
            // Les 4 scripts officiels publiés dans `examples/scripts/` doivent
            // produire le MÊME résultat sur les deux backends — c'est la
            // garantie que documente docs/LUA_PORTABLE.md. Lus depuis les
            // fichiers publiés (pas des copies) : impossible de dériver.
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scripts");
            let tagged = vec![("joueur".to_string(), Vec3::new(4.0, 0.0, -3.0))];
            for (file, time, triggered) in [
                ("rotate.lua", 0.0, false),
                ("move_between_points.lua", 1.7, false),
                ("trigger_door.lua", 0.0, true),
                ("trigger_door.lua", 0.0, false),
                ("simple_enemy.lua", 0.0, false),
            ] {
                let src = std::fs::read_to_string(dir.join(file))
                    .unwrap_or_else(|e| panic!("{file} illisible : {e}"));
                let start = Transform::from_pos(Vec3::new(1.0, 1.0, 1.0));
                let mut t_native = start;
                let mut t_web = start;
                run_native_at(&src, &mut t_native, time, triggered, &tagged);
                run_web_at(&src, &mut t_web, time, triggered, &tagged);
                assert!(
                    (t_native.position - t_web.position).length() < 1e-5,
                    "{file} (time={time}, triggered={triggered}) : positions divergentes \
                     natif {:?} vs web {:?}",
                    t_native.position,
                    t_web.position
                );
                assert!(
                    t_native.rotation.dot(t_web.rotation).abs() > 1.0 - 1e-5,
                    "{file} : rotations divergentes entre les deux backends"
                );
            }
        }

        #[test]
        fn string_and_table_script_matches_between_backends() {
            let src = "local t = {1, 2, 3}\n\
                       local sum = 0\n\
                       for i = 1, #t do sum = sum + t[i] end\n\
                       obj.x = sum\n\
                       obj.rz = string.len('hello') * 10.0";
            let mut t_native = Transform::from_pos(Vec3::ZERO);
            let mut t_web = Transform::from_pos(Vec3::ZERO);
            let mut col = [1.0; 3];
            run_native(src, &mut t_native, &mut col);
            run_web(src, &mut t_web, &mut col);
            assert!((t_native.position.x - t_web.position.x).abs() < 1e-5);
            assert!((t_native.rotation.dot(t_web.rotation)).abs() > 1.0 - 1e-5);
        }
    }
}
