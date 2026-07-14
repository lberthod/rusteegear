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
