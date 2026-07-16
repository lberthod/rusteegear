//! Scènes de démo prêtes à jouer (`Scene::mobile_demo`, `Scene::zombies_demo`,
//! niveaux du contrôleur, etc.) et la scène embarquée du player exporté.
//! Extrait de `scene/mod.rs`.

use glam::Vec3;

use super::{
    AiChaser, AnimationState, AudioSource, Combat, Controller, GameCamera, HudAnchor, HudBinding,
    HudLayout, HudWidget, HudWidgetKind, ImportedMesh, ItemKind, ItemPickup, Light, MeshKind,
    MobileControls, PointLight, Scene, SceneObject, Sky, TapAction, Transform, WEAPONS,
    WeaponPickup, demo_obj,
};
use crate::runtime::physics::PhysicsKind;

/// Script Lua de la « créature » qui erre dans la démo MMORPG (cf.
/// `assets/models/creature.glb` — rig Root/Body/Head/ArmL/HandL/ArmR/HandR/LegL/LegR
/// et clips `Idle`/`Walk` exportés depuis Blender via le connecteur MCP) : patrouille
/// scriptée à évitement d'obstacles par raycast, pas une poursuite du joueur — voir
/// la doc de `AiChaser` sur cette distinction.
///
/// **6ᵉ version, sondes espacées dans le temps** — corrige un ralentissement
/// perceptible en jeu pendant les virages (fluide en marche tout droit) : les 3
/// rayons étaient relancés à **chaque frame en continu**, alors que
/// `Physics::raycast`/`query_broad_phase` (`runtime::physics.rs`) reconstruisent
/// toute la broad-phase à chaque appel — leur propre doc prévient explicitement que
/// c'est « acceptable à l'échelle d'un script par tick, pas d'un appel par frame et
/// par pixel ». 3 appels/frame en continu est exactement l'usage déconseillé ; le
/// coût passait inaperçu en ligne droite (les rayons touchent rarement quelque
/// chose de proche) mais devenait visible en virage, précisément quand ils butent
/// souvent sur un obstacle (table de résultat allouée côté Lua à chaque `hit`). Les
/// rayons ne se rafraîchissent plus qu'1 frame sur `PROBE_EVERY` (~15 Hz) ; le
/// virage reste fluide entre deux relevés grâce au lissage déjà en place
/// (`smooth_turn`, constante de temps ~0,33 s — bien plus lente que les ~67 ms entre
/// deux relevés à 15 Hz).
///
/// **5ᵉ version, vitesse binaire mais virage lissé** — la 4ᵉ version avait aussi
/// lissé exponentiellement la vitesse d'avance (`speed_mul`, 0→1 progressif à
/// l'arrêt/départ, même formule que le virage), en plus du virage — introduisant un
/// nouveau défaut observé en jeu, plus subtil que les précédents : le clip `Walk`
/// se lit à vitesse de lecture **fixe** (le script Lua n'a de prise que sur *quel*
/// clip joue, `obj.anim`, jamais sur `AnimationState.speed` — cf.
/// `app::scripting::run_script`, seul `obj.anim` est relu en sortie), donc une
/// avance à vitesse *progressive* (rampe sur ~0,33 s) désynchronisait le cycle des
/// pattes de la vitesse réelle au sol pendant toute la rampe — glissement des pieds
/// (« certains mouvements » avaient un souci, précisément les phases d'accélération/
/// décélération). Le virage n'a pas cette contrainte (aucune « animation de virage »
/// séparée à synchroniser), donc reste lissé ; l'avance redevient **binaire**
/// (`moving`, plein régime ou arrêt net) — un défaut évité à la source plutôt que
/// masqué, en attendant une éventuelle exposition de `AnimationState.speed` à Lua.
///
/// **Virage lissé exponentiellement** (`smooth_turn`, `1 - e^(-SMOOTH·dt)`, même
/// idiome que la caméra qui suit le joueur, cf. `AppState::sim_step`,
/// framerate-indépendant) vers sa valeur « brute » calculée depuis les 3 rayons —
/// hérité de la 4ᵉ version, toujours nécessaire : sans lissage, un rayon qui perd/
/// regagne sa cible d'une frame à l'autre (bord d'un obstacle, coin d'un mur) fait
/// dévier le cap d'un coup (3ᵉ version, « trop de mouvement », « ça bug »).
///
/// **Hystérésis sur l'arrêt/reprise** (`stop_now`, deux seuils distincts 0.5 m/0.9 m
/// plutôt qu'un seul) : avec un seuil unique, une distance qui oscille juste autour
/// de lui (bord d'obstacle, encore) faisait basculer `obj.anim` entre `"Walk"` et
/// `"Idle"` à chaque frame — chaque bascule relance un fondu enchaîné
/// (`AnimationState::set_clip`, cf. sa doc : ne redémarre que si le clip demandé
/// *diffère* du courant), donc un flip-flop redémarrait le fondu en boucle,
/// perceptible comme un bégaiement de l'animation. Un seuil pour s'arrêter, un
/// seuil plus large pour repartir : il faut un dégagement net, pas juste franchir
/// à nouveau la même ligne. `was_stopped` (état précédent) est persisté séparément
/// (`creature_stopped`) pour que cette hystérésis fonctionne : sans mémoire d'un
/// tick à l'autre, impossible de savoir quel seuil appliquer.
///
/// Reste inchangé depuis la 3ᵉ version : virage proportionnel aux 3 rayons
/// (`probe_dist`, devant ±`PROBE_ANGLE`), léger bruit de méandre, virage anticipé
/// vers le centre de l'arène en approche de bord (`SOFT_BOUND`, `math.atan` à deux
/// arguments — Lua 5.4 le supporte, vérifié), et garde-fou de bornes dur en toute
/// fin de script (`BOUND`, injecté depuis `half` pour ne pas diverger d'un futur
/// changement de taille d'arène).
///
/// État persistant (`save.get`/`save.set` — `{prefix}heading`, `{prefix}turn`,
/// `{prefix}stopped`) : la trajectoire, le lissage du virage et l'hystérésis
/// dépendent tous de l'historique, pas seulement de l'instant présent. `save` est un
/// espace **partagé entre tous les scripts** (pas par objet, cf. sa doc dans
/// `app::scripting::run_script`) : `prefix` isole les clés de chaque créature —
/// depuis la créature n°2 (renardeau, `creature2.glb`), deux instances de ce script
/// coexistent dans la même scène et se marcheraient dessus sans ce préfixe.
///
/// `ray_mask` : masque de couche passé aux `raycast` des sondes. Depuis que les
/// créatures ont un corps physique (`PhysicsKind::Kinematic`), leurs rayons
/// partent de **l'intérieur de leur propre collider** — sans un masque qui exclut
/// leur propre couche (`collision_layer`), chaque sonde toucherait la créature
/// elle-même à distance 0 et elle se croirait bloquée en permanence.
///
/// `heading0`/`phase` : cap initial et déphasage du bruit de méandre, propres à
/// chaque créature. Le script est délibérément déterministe (pas de
/// `math.random`, cf. le tirage de `creature_bite_script`) — mais sans ces deux
/// paramètres, il était déterministe **et identique** pour toutes les
/// instances : même cap de départ (0°, plein +Z) et même méandre
/// (`math.sin(time * 0.35)`, fonction du `time` global, donc identique pour
/// toutes à chaque tick). Toutes les créatures partaient alors en bloc dans la
/// même direction, atteignaient le même mur ensemble, et le braquage anti-mur —
/// lui aussi identique et symétrique en approche frontale — ne les décollait
/// jamais. Même famille de défaut que celle corrigée par le `salt` de
/// `creature_bite_script`.
///
/// Preuve du mouvement réel (pas juste l'animation qui tourne) :
/// `scripted_creature_wanders_then_idles_using_the_imported_walk_and_idle_clips`.
/// Preuve de non-blocage contre un mur + absence de virage brusque, avec la vraie
/// physique : `mmorpg_creature_never_gets_stuck_walking_into_a_wall`
/// (`app::simulation::tests`). Preuve des collisions (mur/joueur infranchissables) :
/// `runtime::physics::tests::a_scripted_kinematic_body_cannot_walk_through_walls_or_the_player`.
/// Preuve que deux instances divergent (caps/positions distincts) :
/// `mmorpg_creatures_do_not_all_walk_in_the_same_direction`.
fn creature_wander_script(
    arena_half: f32,
    prefix: &str,
    ray_mask: u32,
    heading0: f32,
    phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SPEED = 1.3
    local TURN_RATE = 70.0
    local RAY_DIST = 3.5
    local PROBE_ANGLE = 30.0
    local BOUND = {bound}
    local SOFT_BOUND = BOUND - 3.0
    local SMOOTH = 3.0

    local PROBE_EVERY = 4

    local heading = save.get("{prefix}heading") or {heading0}
    local smooth_turn = save.get("{prefix}turn") or 0.0
    local was_stopped = (save.get("{prefix}stopped") or 1.0) > 0.5

    -- Rafraîchit les 3 rayons 1 frame sur `PROBE_EVERY` (~15 Hz à 60 FPS), pas
    -- chaque frame : `Physics::raycast` reconstruit toute la broad-phase à chaque
    -- appel (cf. sa doc, `runtime::physics::Physics::query_broad_phase` —
    -- « acceptable à l'échelle d'un script par tick, pas d'un appel par frame et par
    -- pixel »). 3 rayons/frame en continu, exactement l'usage déconseillé, causait
    -- un ralentissement perceptible pendant les virages (justement quand les rayons
    -- touchent le plus souvent un obstacle). Le virage reste fluide malgré la
    -- lecture moins fréquente : `smooth_turn` le lisse déjà sur ~0,33 s (bien plus
    -- lent que les 4 frames/~67 ms entre deux relevés), et la créature n'avance que
    -- de quelques centimètres dans cet intervalle.
    local tick = (save.get("{prefix}probe_tick") or 0) + 1
    local center_d = save.get("{prefix}center_d")
    local left_d = save.get("{prefix}left_d")
    local right_d = save.get("{prefix}right_d")
    if tick >= PROBE_EVERY or center_d == nil then
        tick = 0
        local function probe_dist(deg)
            local rad = math.rad(heading + deg)
            local dx, dz = math.sin(rad), math.cos(rad)
            local hit = raycast(obj.x, obj.y + 0.6, obj.z, dx, 0, dz, RAY_DIST, {ray_mask})
            if hit then
                return hit.dist
            end
            return RAY_DIST
        end
        center_d = probe_dist(0)
        left_d = probe_dist(-PROBE_ANGLE)
        right_d = probe_dist(PROBE_ANGLE)
        save.set("{prefix}center_d", center_d)
        save.set("{prefix}left_d", left_d)
        save.set("{prefix}right_d", right_d)
    end
    save.set("{prefix}probe_tick", tick)

    -- Virage proportionnel « brut » : penche vers le côté le plus dégagé (loin),
    -- s'écarte du plus bloqué (proche) — lissé juste après, pas appliqué tel quel.
    local raw_turn = (right_d - left_d) / RAY_DIST
    if center_d < RAY_DIST * 0.85 then
        local side = (right_d >= left_d) and 1.0 or -1.0
        raw_turn = raw_turn + side * (1.0 - center_d / RAY_DIST) * 2.5
    end
    raw_turn = raw_turn + math.sin(time * 0.35 + {phase}) * 0.15

    if math.abs(obj.x) > SOFT_BOUND or math.abs(obj.z) > SOFT_BOUND then
        local to_center = math.deg(math.atan(-obj.x, -obj.z))
        local diff = ((to_center - heading + 180) % 360) - 180
        raw_turn = raw_turn + (diff > 0 and 1.0 or -1.0) * 1.5
    end
    raw_turn = math.max(-2.5, math.min(2.5, raw_turn))

    -- Lissage exponentiel du virage (cf. la doc de cette fonction) : la valeur
    -- appliquée suit `raw_turn` progressivement, pas instantanément.
    local smoothing = 1.0 - math.exp(-SMOOTH * dt)
    smooth_turn = smooth_turn + (raw_turn - smooth_turn) * smoothing
    heading = heading + smooth_turn * TURN_RATE * dt

    local rad = math.rad(heading)
    local fwd_x, fwd_z = math.sin(rad), math.cos(rad)

    -- Hystérésis arrêt/reprise (cf. la doc de cette fonction) : le seuil de reprise
    -- (0.9) est plus large que celui d'arrêt (0.5) pour ne pas flip-flopper pile au
    -- bord d'un obstacle. Binaire — pas de rampe lissée comme pour `smooth_turn` :
    -- le clip `Walk` joue à vitesse de lecture fixe (le script n'a pas de prise sur
    -- `AnimationState.speed`, seulement sur le clip joué), donc une avance à vitesse
    -- *progressive* désynchronise le cycle des jambes de la vitesse réelle au sol —
    -- glissement des pieds pendant toute la rampe. Un arrêt/départ net évite le
    -- problème à la source plutôt que de le masquer.
    local stop_now
    if was_stopped then
        stop_now = center_d < 0.9
    else
        stop_now = center_d < 0.5
    end
    local moving = not stop_now

    if moving then
        obj.x = obj.x + fwd_x * SPEED * dt
        obj.z = obj.z + fwd_z * SPEED * dt
    end
    obj.anim = moving and "Walk" or "Idle"

    -- Garde-fou final, cf. la doc de ce script : quoi qu'il arrive, ne jamais
    -- sortir de l'arène.
    obj.x = math.max(-BOUND, math.min(BOUND, obj.x))
    obj.z = math.max(-BOUND, math.min(BOUND, obj.z))

    obj.ry = heading
    save.set("{prefix}heading", heading)
    save.set("{prefix}turn", smooth_turn)
    save.set("{prefix}stopped", stop_now and 1.0 or 0.0)
"#
    )
}

/// Attaque au contact — réservée à la Créature n°1, ajoutée en plus de
/// `creature_wander_script` (pas un remplacement). Volontairement distincte du
/// pattern des dangers existants (`if obj.triggered then damage(dps*dt) end`,
/// dégâts continus tant que le contact dure — cf. `roguelike_demo`/
/// `zombies_demo`) : ici l'attaque ne se déclenche qu'au rythme de
/// `BITE_COOLDOWN` et pas à coup sûr (`BITE_CHANCE`), pour une morsure
/// occasionnelle plutôt qu'un contact systématiquement punitif — un PNJ qui
/// erre, pas un danger de zone de combat.
///
/// Tirage déterministe (hachage de `time`, même idiome que le bruit de
/// méandre de `creature_wander_script`) plutôt que `math.random` : ce projet
/// s'est délibérément débarrassé des graines non reproductibles au profit
/// d'un RNG déterministe (`runtime::rng`, Sprint 131) — un script Lua non
/// seedable romprait cette garantie et resterait, de toute façon, impossible
/// à tester exactement (cf. les tests `creature_1_*` dans
/// `app::simulation::tests`, qui rejouent le même calcul côté Rust).
///
/// Nécessite `SceneObject::trigger = true` sur la créature (sinon
/// `obj.triggered` reste toujours faux, cf. la détection de contact dans
/// `AppState::sim_step`) — n'affecte pas son collider physique
/// (`PhysicsKind::Kinematic`), `trigger` est un indicateur de scène pur, lu
/// uniquement côté script.
/// `cooldown`/`chance`/`damage` paramètrent le tempérament : la Créature 1
/// (morsure, 2.2 s/0.4/0.12) a servi de gabarit, la chauve-souris (n°6) mord
/// plus vite mais plus faible, le crabe (n°7) pince rarement mais fort — cf.
/// `MMORPG_CREATURES`. `salt` décale la phase du tirage pseudo-aléatoire :
/// sans lui, toutes les créatures au contact du joueur au même tick
/// réussiraient/rateraient leur attaque exactement ensemble (même hachage du
/// même `time`).
fn creature_bite_script(
    prefix: &str,
    cooldown: f32,
    chance: f32,
    damage: f32,
    salt: f32,
) -> String {
    format!(
        r#"
    local BITE_COOLDOWN = {cooldown}
    local BITE_CHANCE = {chance}
    local BITE_DAMAGE = {damage}

    local bite_cd = (save.get("{prefix}bite_cd") or 0.0) - dt
    if obj.triggered and bite_cd <= 0.0 then
        bite_cd = BITE_COOLDOWN
        local roll = math.sin(time * {salt}) * 43758.5453
        roll = roll - math.floor(roll)
        if roll < BITE_CHANCE then
            damage(BITE_DAMAGE)
        end
    end
    save.set("{prefix}bite_cd", bite_cd)
"#
    )
}

/// Sentinelle (Créature 11, golem) : garde son poste — orbite lentement autour
/// de son point d'apparition (mémorisé au premier tick dans `save`) plutôt que
/// d'errer dans toute l'arène. Cohérent avec son attaque en éventail
/// (`AttackStyle::Fan`) : elle tient une position, le joueur vient à elle.
fn creature_guard_script(
    _arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    format!(
        r#"
    local SPEED = 0.9
    local ORBIT = 3.0

    local sx = save.get("{prefix}spawn_x")
    local sz = save.get("{prefix}spawn_z")
    if sx == nil then
        sx = obj.x; sz = obj.z
        save.set("{prefix}spawn_x", sx)
        save.set("{prefix}spawn_z", sz)
    end
    local ang = (save.get("{prefix}ang") or 0.0) + 0.35 * dt
    save.set("{prefix}ang", ang)
    local tx = sx + math.cos(ang) * ORBIT
    local tz = sz + math.sin(ang) * ORBIT
    local dx, dz = tx - obj.x, tz - obj.z
    local d = math.sqrt(dx * dx + dz * dz)
    if d > 0.08 then
        local step = math.min(SPEED * dt, d)
        obj.x = obj.x + dx / d * step
        obj.z = obj.z + dz / d * step
        obj.ry = math.deg(math.atan(dx, dz))
        obj.anim = "Walk"
    else
        obj.anim = "Idle"
    end
"#
    )
}

/// Rôdeur (Créature 12, félin d'ombre) : maintient sa distance au joueur
/// (repéré via `find_tag("joueur")`) — approche au-delà de FAR, recule sous
/// NEAR, et tourne autour de lui entre les deux. Cohérent avec sa rafale
/// (`AttackStyle::Burst`) : il reste dans sa fourchette de tir idéale.
fn creature_kite_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SPEED = 1.8
    local NEAR = 4.0
    local FAR = 6.0
    local SMOOTH = 4.0
    local BOUND = {bound}

    -- Audit gameplay (girouette) : la bascule approche/orbite/recul se faisait
    -- d'un tick à l'autre, sans transition — pile sur un seuil (d ≈ NEAR/FAR),
    -- la vitesse ET `obj.ry` claquaient de 90° à chaque frame (tremblement sur
    -- place, demi-tours illogiques). La vitesse *désirée* reste calculée par
    -- zone, mais la vitesse *appliquée* la suit par lissage exponentiel (même
    -- idiome que `smooth_turn` du script d'errance) et `obj.ry` suit le
    -- mouvement réel, pas le joueur : le passage d'une zone à l'autre est un
    -- virage, plus un claquement.
    local svx = save.get("{prefix}svx") or 0.0
    local svz = save.get("{prefix}svz") or 0.0
    local wx, wz = 0.0, 0.0
    local players = find_tag("joueur")
    if #players > 0 then
        local p = players[1]
        local dx, dz = p.x - obj.x, p.z - obj.z
        local d = math.sqrt(dx * dx + dz * dz)
        if d > 0.01 then
            local nx, nz = dx / d, dz / d
            if d > FAR then
                wx, wz = nx, nz
            elseif d < NEAR then
                wx, wz = -nx, -nz
            else
                -- Dans la fourchette : tourne autour du joueur (perpendiculaire).
                wx, wz = -nz, nx
            end
        end
    else
        -- Pas de joueur repéré : petit va-et-vient nerveux sur place.
        wx = math.sin(time * 1.7) * 0.25
    end
    local k = 1.0 - math.exp(-SMOOTH * dt)
    svx = svx + (wx * SPEED - svx) * k
    svz = svz + (wz * SPEED - svz) * k
    local sp = math.sqrt(svx * svx + svz * svz)
    obj.x = obj.x + svx * dt
    obj.z = obj.z + svz * dt
    -- Cap borné en vitesse de rotation : quand la vitesse lissée passe près de
    -- zéro (inversion de sens), sa direction peut sauter de 180° d'un tick à
    -- l'autre — le corps, lui, pivote au plus à TURN_RATE.
    local TURN_RATE = 240.0
    local hry = save.get("{prefix}ry") or obj.ry
    if sp > 0.2 then
        local want = math.deg(math.atan(svx, svz))
        local diff = ((want - hry + 180) % 360) - 180
        hry = hry + math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
    end
    obj.ry = hry
    obj.anim = sp > 0.2 and "Walk" or "Idle"
    obj.x = math.max(-BOUND, math.min(BOUND, obj.x))
    obj.z = math.max(-BOUND, math.min(BOUND, obj.z))
    save.set("{prefix}ry", hry)
    save.set("{prefix}svx", svx)
    save.set("{prefix}svz", svz)
    save.set("{prefix}seen", #players)
"#
    )
}

/// Dérive fuyante (Créature 13, méduse) : flotte en dérive sinusoïdale lente,
/// et fuit si le joueur approche à moins de FLEE — l'inverse d'un chasseur.
/// Cohérent avec son orbe à tête chercheuse (`AttackStyle::Homing`) : elle
/// n'a pas besoin d'être près, son tir la venge de loin.
fn creature_drift_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local DRIFT = 0.6
    local FLEE_SPEED = 1.6
    local FLEE = 3.0
    local BOUND = {bound}

    -- Audit gameplay : le cap de fuite (et le rappel vers le centre) était posé
    -- d'un coup (`heading = atan(...)`) — pivot de 180° en une frame quand le
    -- joueur surgissait. Le cap **cible** reste instantané, mais le cap appliqué
    -- le rejoint à vitesse bornée (TURN_RATE) : la méduse se détourne vite,
    -- sans claquer.
    local TURN_RATE = 240.0
    local heading = (save.get("{prefix}heading") or 0.0) + math.sin(time * 0.4) * 25.0 * dt
    local target = nil
    local fleeing = false
    local players = find_tag("joueur")
    if #players > 0 then
        local p = players[1]
        local dx, dz = obj.x - p.x, obj.z - p.z
        local d = math.sqrt(dx * dx + dz * dz)
        if d < FLEE and d > 0.01 then
            target = math.deg(math.atan(dx / d, dz / d))
            fleeing = true
        end
    end
    -- Rappel doux vers le centre en approche de bord (fuir dans un mur n'a pas de sens).
    if math.abs(obj.x) > BOUND - 2.0 or math.abs(obj.z) > BOUND - 2.0 then
        target = math.deg(math.atan(-obj.x, -obj.z))
    end
    if target ~= nil then
        local diff = ((target - heading + 180) % 360) - 180
        heading = heading + math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
    end
    local rad = math.rad(heading)
    local speed = fleeing and FLEE_SPEED or DRIFT
    obj.x = obj.x + math.sin(rad) * speed * dt
    obj.z = obj.z + math.cos(rad) * speed * dt
    obj.ry = heading
    obj.anim = "Walk"
    obj.x = math.max(-BOUND, math.min(BOUND, obj.x))
    obj.z = math.max(-BOUND, math.min(BOUND, obj.z))
    save.set("{prefix}heading", heading)
"#
    )
}

/// Patrouille d'artillerie (Créature 14, escargot-mortier) : fait la navette
/// très lentement entre son point d'apparition et un second point proche —
/// quasi statique, comme une pièce d'artillerie qu'on repositionne à peine.
/// Cohérent avec son obus en cloche (`AttackStyle::Lob`) : longue portée,
/// aucune mobilité.
fn creature_artillery_script(
    _arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    format!(
        r#"
    local SPEED = 0.35
    local LEG = 4.0

    local sx = save.get("{prefix}spawn_x")
    local sz = save.get("{prefix}spawn_z")
    if sx == nil then
        sx = obj.x; sz = obj.z
        save.set("{prefix}spawn_x", sx)
        save.set("{prefix}spawn_z", sz)
    end
    local going = (save.get("{prefix}going") or 1.0) > 0.5
    local tx = going and (sx + LEG) or sx
    local dx = tx - obj.x
    -- Audit gameplay : le cap claquait de ±180° d'un tick à l'autre à chaque
    -- bout de navette — pivot limité en vitesse (l'escargot se retourne
    -- pesamment, cohérent avec son tempérament d'artillerie).
    local TURN_RATE = 120.0
    local heading = save.get("{prefix}heading") or (dx >= 0 and 90.0 or -90.0)
    local target_ry = dx >= 0 and 90.0 or -90.0
    local diff = ((target_ry - heading + 180) % 360) - 180
    local turn = math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
    heading = heading + turn
    save.set("{prefix}heading", heading)
    obj.ry = heading
    if math.abs(dx) < 0.05 then
        save.set("{prefix}going", going and 0.0 or 1.0)
        obj.anim = "Idle"
    elseif math.abs(diff) > 30.0 then
        -- Encore en train de se retourner : pas d'avance en crabe.
        obj.anim = "Idle"
    else
        local step = math.min(SPEED * dt, math.abs(dx))
        obj.x = obj.x + (dx > 0 and step or -step)
        obj.anim = "Walk"
    end
    -- Retour au rail borné en vitesse : une bousculade (créature qui croise la
    -- navette) peut l'écarter de sa ligne — la re-coller d'un coup (`obj.z = sz`)
    -- était une téléportation d'autant de mètres que la poussée subie.
    local RAIL_SPEED = 0.6
    local dzr = sz - obj.z
    obj.z = obj.z + math.max(-RAIL_SPEED * dt, math.min(RAIL_SPEED * dt, dzr))
"#
    )
}

/// Zigzag erratique (Créature 15, oursin-étoile) : cap qui alterne
/// brusquement de biais toutes les ~1,2 s — une trajectoire imprévisible qui
/// rapproche souvent l'oursin du joueur sans le poursuivre. Cohérent avec sa
/// nova (`AttackStyle::Nova`) : c'est sa proximité erratique qui est le danger.
fn creature_zigzag_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SPEED = 1.5
    local FLIP_EVERY = 1.2
    local TURN = 65.0
    local BOUND = {bound}

    local heading = save.get("{prefix}heading") or 45.0
    local flip_at = save.get("{prefix}flip_at") or 0.0
    local side = save.get("{prefix}side") or 1.0
    if time >= flip_at then
        side = -side
        save.set("{prefix}side", side)
        save.set("{prefix}flip_at", time + FLIP_EVERY)
    end
    heading = heading + side * TURN * dt
    -- Rappel vers le centre en approche de bord, comme les autres patrouilles.
    if math.abs(obj.x) > BOUND - 2.0 or math.abs(obj.z) > BOUND - 2.0 then
        local to_center = math.deg(math.atan(-obj.x, -obj.z))
        local diff = ((to_center - heading + 180) % 360) - 180
        heading = heading + (diff > 0 and 1.0 or -1.0) * 120.0 * dt
    end
    local rad = math.rad(heading)
    obj.x = obj.x + math.sin(rad) * SPEED * dt
    obj.z = obj.z + math.cos(rad) * SPEED * dt
    obj.ry = heading
    obj.anim = "Walk"
    obj.x = math.max(-BOUND, math.min(BOUND, obj.x))
    obj.z = math.max(-BOUND, math.min(BOUND, obj.z))
    save.set("{prefix}heading", heading)
"#
    )
}

/// Patrouille aérienne (Créature 16, griffon) : grand cercle horizontal à
/// rayon fixe autour du point d'apparition, plus large et plus rapide que
/// l'orbite de la sentinelle (`creature_guard_script`) — cohérent avec son
/// éventail de bourrasques (`AttackStyle::Fan`, portée 8 m) : elle couvre du
/// terrain, pas un poste fixe.
///
/// **Audit gameplay (téléportations)** : la 1ʳᵉ version écrivait la position
/// **en absolu** sur le point paramétrique du cercle — deux « gros sauts »
/// visibles en jeu. Au premier tick, l'objet sautait instantanément du spawn
/// au cercle (RADIUS d'un coup) ; et quand un obstacle bloquait le corps
/// kinématique (`resolve_scripted_moves` ne fait que raboter le déplacement du
/// tick), `ang` continuait d'avancer pendant le blocage — au dégagement, la
/// créature **bondissait** de tout le retard accumulé. Désormais le point du
/// cercle est une **cible** vers laquelle on marche par pas plafonné (comme la
/// sentinelle), et `ang` n'avance que si la créature suit (retard < LAG) : une
/// cible qui attend au lieu d'un rendez-vous manqué à rattraper d'un bond.
fn creature_soar_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SPEED = 1.6
    local RADIUS = 6.0
    local LAG = 1.0
    local BOUND = {bound}

    local sx = save.get("{prefix}spawn_x")
    local sz = save.get("{prefix}spawn_z")
    if sx == nil then
        sx = obj.x; sz = obj.z
        save.set("{prefix}spawn_x", sx)
        save.set("{prefix}spawn_z", sz)
    end
    local ang = save.get("{prefix}ang") or 0.0
    -- Cible bornée à l'arène : un arc de cercle qui mordrait sur un mur reste
    -- atteignable (la cible glisse le long du mur au lieu d'être derrière).
    local function target_at(a)
        local px = math.max(-BOUND, math.min(BOUND, sx + math.cos(a) * RADIUS))
        local pz = math.max(-BOUND, math.min(BOUND, sz + math.sin(a) * RADIUS))
        return px, pz
    end
    local tx, tz = target_at(ang)
    local dx, dz = tx - obj.x, tz - obj.z
    local d = math.sqrt(dx * dx + dz * dz)
    if d < LAG then
        -- À jour : la cible avance, et on la re-mesure après coup — sinon une
        -- créature pile dessus resterait sous le seuil de marche une frame sur
        -- deux (flip-flop Walk/Idle, bégaiement d'animation).
        ang = ang + (SPEED / RADIUS) * dt
        save.set("{prefix}ang", ang)
        tx, tz = target_at(ang)
        dx, dz = tx - obj.x, tz - obj.z
        d = math.sqrt(dx * dx + dz * dz)
    end
    -- Cap borné en vitesse de rotation (même garde-fou que les autres
    -- comportements) : la direction vers une cible toute proche peut tourner
    -- très vite d'un tick à l'autre — le corps, lui, pivote au plus à TURN_RATE.
    local TURN_RATE = 300.0
    local hry = save.get("{prefix}ry") or obj.ry
    if d > 0.01 then
        local step = math.min(SPEED * 1.4 * dt, d)
        obj.x = obj.x + dx / d * step
        obj.z = obj.z + dz / d * step
        local want = math.deg(math.atan(dx, dz))
        local diff = ((want - hry + 180) % 360) - 180
        hry = hry + math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
        obj.anim = "Walk"
    else
        obj.anim = "Idle"
    end
    obj.ry = hry
    save.set("{prefix}ry", hry)
"#
    )
}

/// Lemniscate (Créature 17, kraken-mini) : dérive en huit couché autour de son
/// point d'apparition — ni fuit ni poursuit, juste une trajectoire hypnotique.
/// Cohérent avec sa nova resserrée (`AttackStyle::Nova`) : elle n'a besoin
/// d'aucune tactique d'approche, juste être là quand le joueur passe à portée.
/// **Audit gameplay (téléportations)** : même défaut et même correction que
/// `creature_soar_script` — la position était écrite en absolu sur la courbe
/// (dont l'aile sud mordait sur le mur de l'arène) ; le paramètre `t` avançait
/// pendant les blocages et la créature bondissait au dégagement. Désormais le
/// point de la courbe est une cible bornée, atteinte par pas plafonné, et `t`
/// n'avance que si la créature suit.
fn creature_lemniscate_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SPEED = 0.5
    local SCALE = 4.5
    local LAG = 1.0
    local BOUND = {bound}

    local sx = save.get("{prefix}spawn_x")
    local sz = save.get("{prefix}spawn_z")
    if sx == nil then
        sx = obj.x; sz = obj.z
        save.set("{prefix}spawn_x", sx)
        save.set("{prefix}spawn_z", sz)
    end
    local t = save.get("{prefix}t") or 0.0
    -- Courbe de Lissajous (huit couché) : x = sin(t), z = sin(t)*cos(t) —
    -- suivie comme une cible (bornée à l'arène), jamais écrite en absolu.
    local function target_at(u)
        local px = math.max(-BOUND, math.min(BOUND, sx + math.sin(u) * SCALE))
        local pz = math.max(-BOUND, math.min(BOUND, sz + math.sin(u) * math.cos(u) * SCALE))
        return px, pz
    end
    local tx, tz = target_at(t)
    local dx, dz = tx - obj.x, tz - obj.z
    local d = math.sqrt(dx * dx + dz * dz)
    if d < LAG then
        -- Cible re-mesurée après l'avance, cf. `creature_soar_script` (évite le
        -- flip-flop Walk/Idle d'une créature pile sur sa cible).
        t = t + SPEED * dt
        save.set("{prefix}t", t)
        tx, tz = target_at(t)
        dx, dz = tx - obj.x, tz - obj.z
        d = math.sqrt(dx * dx + dz * dz)
    end
    -- Vitesse de croisière ≈ dérivée max de la courbe (SPEED·SCALE·√2), plafonnée.
    -- Cap borné en vitesse de rotation : la tangente du huit tourne très vite
    -- aux pointes des lobes — cf. `creature_soar_script`, même garde-fou.
    local TURN_RATE = 300.0
    local hry = save.get("{prefix}ry") or obj.ry
    if d > 0.01 then
        local step = math.min(SPEED * SCALE * 1.5 * dt, d)
        obj.x = obj.x + dx / d * step
        obj.z = obj.z + dz / d * step
        local want = math.deg(math.atan(dx, dz))
        local diff = ((want - hry + 180) % 360) - 180
        hry = hry + math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
        obj.anim = "Walk"
    else
        obj.anim = "Idle"
    end
    obj.ry = hry
    save.set("{prefix}ry", hry)
"#
    )
}

/// Surgit puis plonge (Créature 18, ver des sables) : immobile (« sous le
/// sable ») pendant SUBMERGED, puis fonce en ligne droite pendant RUSH avant
/// de replonger. Cohérent avec sa rafale rapprochée (`AttackStyle::Burst`,
/// windup court) : la menace, c'est la charge, pas une poursuite soutenue.
fn creature_burrow_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SUBMERGED = 2.2
    local RUSH = 1.1
    local SPEED = 3.2
    local BOUND = {bound}

    local phase_end = save.get("{prefix}phase_end") or 0.0
    local rushing = (save.get("{prefix}rushing") or 0.0) > 0.5
    if time >= phase_end then
        rushing = not rushing
        save.set("{prefix}rushing", rushing and 1.0 or 0.0)
        phase_end = time + (rushing and RUSH or SUBMERGED)
        save.set("{prefix}phase_end", phase_end)
        if rushing then
            -- Nouvelle direction de charge à chaque surgissement.
            local heading = math.deg(math.atan(-obj.x, -obj.z)) + (math.sin(time * 3.1) * 90.0)
            save.set("{prefix}heading", heading)
        end
    end
    if rushing then
        local heading = save.get("{prefix}heading") or 0.0
        local rad = math.rad(heading)
        obj.x = obj.x + math.sin(rad) * SPEED * dt
        obj.z = obj.z + math.cos(rad) * SPEED * dt
        obj.ry = heading
        obj.anim = "Walk"
    else
        obj.anim = "Idle"
    end
    obj.x = math.max(-BOUND, math.min(BOUND, obj.x))
    obj.z = math.max(-BOUND, math.min(BOUND, obj.z))
"#
    )
}

/// Flotte et recule (Créature 19, lanterne-fantôme) : dérive vers le joueur
/// de très loin (curiosité), mais recule dès qu'il approche à moins de NEAR —
/// jamais au contact. Cohérent avec son follet à tête chercheuse
/// (`AttackStyle::Homing`, portée 9 m) : elle punit à distance, jamais de près.
/// **Audit gameplay (girouette)** : le sens avance/recul basculait sur un seuil
/// unique (`d < NEAR`), sans zone morte ni lissage — pile à la frontière, la
/// lanterne oscillait sur place en pivotant de 180° à chaque frame. Désormais
/// une **zone morte** ([NEAR, NEAR + 1]) où elle reste en vol stationnaire,
/// et une vitesse lissée exponentiellement (cf. `creature_kite_script`) : les
/// inversions deviennent des demi-tours progressifs.
fn creature_hover_script(
    arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    let bound = arena_half - 1.0;
    format!(
        r#"
    local SPEED = 0.7
    local NEAR = 4.0
    local DEAD = 1.0
    local SMOOTH = 3.0
    local BOUND = {bound}

    local svx = save.get("{prefix}svx") or 0.0
    local svz = save.get("{prefix}svz") or 0.0
    local wx, wz = 0.0, 0.0
    local players = find_tag("joueur")
    if #players > 0 then
        local p = players[1]
        local dx, dz = p.x - obj.x, p.z - obj.z
        local d = math.sqrt(dx * dx + dz * dz)
        if d > 0.05 then
            local nx, nz = dx / d, dz / d
            if d < NEAR then
                wx, wz = -nx, -nz
            elseif d > NEAR + DEAD then
                wx, wz = nx, nz
            end
            -- Entre NEAR et NEAR + DEAD : vol stationnaire (zone morte).
        end
    end
    local k = 1.0 - math.exp(-SMOOTH * dt)
    svx = svx + (wx * SPEED - svx) * k
    svz = svz + (wz * SPEED - svz) * k
    local sp = math.sqrt(svx * svx + svz * svz)
    obj.x = obj.x + svx * dt
    obj.z = obj.z + svz * dt
    -- Cap borné en vitesse de rotation, cf. `creature_kite_script` (même
    -- garde-fou contre le pivot de 180° à l'inversion de sens).
    local TURN_RATE = 240.0
    local hry = save.get("{prefix}ry") or obj.ry
    if sp > 0.1 then
        local want = math.deg(math.atan(svx, svz))
        local diff = ((want - hry + 180) % 360) - 180
        hry = hry + math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
    end
    obj.ry = hry
    obj.anim = sp > 0.1 and "Walk" or "Idle"
    obj.x = math.max(-BOUND, math.min(BOUND, obj.x))
    obj.z = math.max(-BOUND, math.min(BOUND, obj.z))
    save.set("{prefix}ry", hry)
    save.set("{prefix}svx", svx)
    save.set("{prefix}svz", svz)
"#
    )
}

/// Tourelle qui pivote (Créature 20, tortue-canon) : quasi immobile, tourne
/// juste sur elle-même pour s'orienter vers le joueur (ou lentement sinon).
/// Cohérent avec son obus rapide et rapproché (`AttackStyle::Lob`, portée
/// courte) : elle n'a pas besoin de bouger, juste de bien viser.
fn creature_turret_script(
    _arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    format!(
        r#"
    local TURN_RATE = 40.0

    local heading = save.get("{prefix}heading") or obj.ry
    local target = heading
    local players = find_tag("joueur")
    if #players > 0 then
        local p = players[1]
        local dx, dz = p.x - obj.x, p.z - obj.z
        if math.abs(dx) > 0.01 or math.abs(dz) > 0.01 then
            target = math.deg(math.atan(dx, dz))
        end
    else
        target = heading + 15.0 * dt
    end
    local diff = ((target - heading + 180) % 360) - 180
    local step = math.max(-TURN_RATE * dt, math.min(TURN_RATE * dt, diff))
    heading = heading + step
    obj.ry = heading
    obj.anim = "Idle"
    save.set("{prefix}heading", heading)
"#
    )
}

impl Scene {
    /// Démo « contrôleur » **sans script** (niveau 1) : joueur pilotable au joystick,
    /// saut, collisions, pièces à ramasser, lave à éviter.
    pub fn controller_demo() -> Self {
        Self::controller_level(1)
    }

    /// Niveau `level` (1-based) de la démo contrôleur. Les niveaux supérieurs sont plus
    /// grands/chargés (plus de pièces, lave plus large, bonus plus fréquents).
    pub fn controller_level(level: u32) -> Self {
        let lvl = level.max(1);
        let hard = (lvl - 1) as f32; // 0 au niveau 1, 1 au niveau 2, …

        // Sol statique (teinte qui varie par niveau pour les distinguer).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.30 + 0.12 * hard, 0.5 - 0.08 * hard, 0.42];

        // Joueur pilotable : Input Receiver + saut sur le bouton « Saut ».
        // Démarre au bord (pas sur la lave centrale).
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, -6.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Attaque au corps-à-corps : vainc les ennemis `attackable` à portée (cf.
        // `Scene::attack_at`), sur pression du bouton tactile « Attaque » ou de la
        // touche J (desktop, cf. `PlayerInput::attack`). Portée courte (0,7 m) : au-delà
        // de `attack_range`, ce qui compte c'est l'écart avec la portée de morsure de la
        // cible (son propre rayon) — un écart de 1,5 m rendait le combat sans risque
        // (audit gameplay : un bot qui approche puis attaque ne prenait jamais de dégâts).
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.6,
            attack_button: "Attaque".into(),
            attack_range: 0.7,
            ..Default::default()
        });

        // Effet visuel du coup : sphère blanche invisible par défaut, téléportée sur la
        // cible et affichée brièvement par `App` quand une attaque porte (cf.
        // `AppState::attack_flash`) — rend le coup lisible, pas juste sonore.
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        let mut objects = vec![sol, joueur, fx];

        // --- Murs de pourtour : enferment l'aire de jeu (le joueur ne tombe plus) ---
        // Le sol (plan unité × 16) couvre [-8, 8] ; on pose 4 murs statiques aux bords.
        let half = 7.5_f32;
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.45, 0.5, 0.62];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.6, -half),
            Vec3::new(16.0, 1.2, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.6, half),
            Vec3::new(16.0, 1.2, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.6, 0.0),
            Vec3::new(0.5, 1.2, 16.0),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.6, 0.0),
            Vec3::new(0.5, 1.2, 16.0),
        );

        // Mare de lave **au centre** (plus large aux niveaux supérieurs) : à contourner.
        // Note : le mesh Plane a une épaisseur visuelle nulle (y=0 pour tous les sommets),
        // donc l'échelle Y ne change rien au rendu — on s'en sert pour épaissir l'AABB de
        // collision verticalement (≈0.6 m autour du sol) afin que la zone mortelle détecte
        // fiablement un joueur qui marche dessus (capsule au repos ~y=0.5), tout en restant
        // franchissable en sautant par-dessus (le pic du saut dépasse cette plage).
        let lave_s = 3.0 + hard;
        let mut lave = demo_obj("Lave", MeshKind::Plane, Vec3::new(0.0, 0.02, 0.0));
        lave.transform = lave.transform.with_scale(Vec3::new(lave_s, 30.0, lave_s));
        lave.color = [0.95, 0.3, 0.1];
        lave.emissive = 0.7;
        lave.deadly = true;
        // Bouillonnement : la teinte pulse (deux fréquences superposées) sans toucher à
        // l'échelle Y (réservée à l'épaisseur de collision, cf. note ci-dessus).
        lave.script = "\
local b = 0.5 + 0.5 * math.sin(time * 2.2) + 0.25 * math.sin(time * 5.3)\n\
obj.r = 0.85 + 0.15 * b; obj.g = 0.22 + 0.18 * b; obj.b = 0.05 + 0.1 * b"
            .into();
        objects.push(lave);

        // Bulles de lave décoratives : jaillissent puis retombent en boucle, déphasées,
        // pour animer la surface (aucune collision/danger propre : la mare mère suffit).
        let bub_r = (lave_s * 0.5 - 0.4).max(0.3);
        for (n, (bx, bz, ph)) in [
            (0.5_f32, -0.3_f32, 0.0_f32),
            (-0.4, 0.4, 1.1),
            (0.1, 0.6, 2.3),
            (-0.5, -0.5, 3.6),
            (0.6, 0.1, 4.8),
        ]
        .into_iter()
        .enumerate()
        {
            let pos = Vec3::new(bx * bub_r, 0.05, bz * bub_r);
            let mut bubble = demo_obj(&format!("Bulle Lave {}", n + 1), MeshKind::Sphere, pos);
            bubble.color = [1.0, 0.5, 0.15];
            bubble.emissive = 1.0;
            bubble.script = format!(
                "local cyc = (time * 0.6 + {ph}) % 2.0\n\
                 local h = math.max(0.0, math.sin(cyc * math.pi))\n\
                 obj.y = 0.02 + h * 0.4\n\
                 obj.sx = 0.12 + h * 0.28; obj.sy = 0.12 + h * 0.28; obj.sz = 0.12 + h * 0.28"
            );
            objects.push(bubble);
        }

        // --- Pont surélevé traversant la lave (axe Z) : raccourci risqué mais direct.
        // Reste hors de portée verticale de la lave (marge ≈0.23 m) — sûr tant qu'on ne
        // tombe pas sur les côtés, ce qui ramène au niveau du sol au-dessus de la lave
        // (mort instantanée). Récompensé par une gemme suprême flottant en son centre.
        let bridge_half = lave_s * 0.5 + 0.8;
        let mut bridge = demo_obj("Pont", MeshKind::Cube, Vec3::new(0.0, 1.0, 0.0));
        bridge.transform = bridge
            .transform
            .with_scale(Vec3::new(0.9, 0.3, bridge_half * 2.0));
        bridge.physics = PhysicsKind::Static;
        bridge.color = [0.4, 0.36, 0.42];
        bridge.metallic = 0.25;
        bridge.roughness = 0.5;
        objects.push(bridge);

        let mut supreme = demo_obj("Gemme Suprême", MeshKind::Sphere, Vec3::new(0.0, 1.75, 0.0));
        supreme.transform = supreme.transform.with_scale(Vec3::splat(0.5));
        supreme.color = [0.85, 0.3, 0.95];
        supreme.emissive = 1.1;
        supreme.metallic = 0.5;
        supreme.tappable = true;
        supreme.tap_action = TapAction::Hide;
        supreme.respawn_delay = 7.0 - hard;
        objects.push(supreme);

        // Piliers-obstacles aux diagonales, surmontés d'une **étoile bonus** (en hauteur,
        // atteignable au saut ; réapparaît → score continu).
        for (n, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let base = Vec3::new(sx * 4.3, 0.0, sz * 4.3);
            let mut pil = demo_obj(
                &format!("Pilier {}", n + 1),
                MeshKind::Cube,
                base + Vec3::Y * 0.7,
            );
            pil.transform = pil.transform.with_scale(Vec3::new(0.8, 1.4, 0.8));
            pil.physics = PhysicsKind::Static;
            pil.color = [0.5, 0.52, 0.6];
            objects.push(pil);

            let mut star = demo_obj(
                &format!("Étoile {}", n + 1),
                MeshKind::Sphere,
                base + Vec3::Y * 1.9,
            );
            star.transform = star.transform.with_scale(Vec3::splat(0.4));
            star.color = [0.55, 0.85, 1.0];
            star.emissive = 0.8;
            star.tappable = true;
            star.tap_action = TapAction::Hide;
            star.respawn_delay = 4.0 - hard; // réapparition plus rapide au niveau 2
            objects.push(star);
        }

        // --- Pièces-objectif : anneaux générés automatiquement autour de la lave ---
        let rings: &[(u32, f32)] = if hard > 0.5 {
            &[(6, 3.8), (8, 6.4)]
        } else {
            &[(6, 3.4), (6, 6.2)]
        };
        let mut p = 0;
        for &(ring, radius) in rings {
            for k in 0..ring {
                // anneau extérieur décalé d'un demi-pas (disposition en quinconce).
                let off = if radius > 5.0 { 0.5 } else { 0.0 };
                let angle = (k as f32 + off) / ring as f32 * std::f32::consts::TAU;
                let pos = Vec3::new(angle.cos() * radius, 0.5, angle.sin() * radius);
                p += 1;
                let mut gem = demo_obj(&format!("Pièce {p}"), MeshKind::Sphere, pos);
                gem.transform = gem.transform.with_scale(Vec3::splat(0.45));
                gem.color = [1.0, 0.85, 0.2];
                gem.emissive = 0.5;
                gem.metallic = 0.6;
                gem.roughness = 0.25;
                gem.tappable = true;
                gem.tap_action = TapAction::Hide;
                objects.push(gem);
            }
        }

        // --- Escalier + plateforme surélevée côté ouest : défi de plateforme optionnel,
        // récompensé par des pièces bonus et un trophée (ne bloque pas la victoire).
        for i in 0..3u32 {
            let sy = 0.3 + i as f32 * 0.3;
            let sx = -7.0 + i as f32 * 0.65;
            let mut step = demo_obj(
                &format!("Marche {}", i + 1),
                MeshKind::Cube,
                Vec3::new(sx, sy * 0.5, 0.0),
            );
            step.transform = step.transform.with_scale(Vec3::new(0.75, sy, 2.2));
            step.physics = PhysicsKind::Static;
            step.color = [0.55, 0.5, 0.4];
            objects.push(step);
        }
        let mut podium = demo_obj("Plateforme", MeshKind::Cube, Vec3::new(-5.0, 0.95, 0.0));
        podium.transform = podium.transform.with_scale(Vec3::new(1.7, 0.3, 2.6));
        podium.physics = PhysicsKind::Static;
        podium.color = [0.52, 0.48, 0.58];
        podium.metallic = 0.35;
        podium.roughness = 0.35;
        objects.push(podium);

        // Deux pièces bonus flanquant le trophée, en hauteur sur la plateforme.
        for (n, dz) in [(1, -0.8), (2, 0.8)] {
            let mut bonus = demo_obj(
                &format!("Pièce Bonus {n}"),
                MeshKind::Sphere,
                Vec3::new(-5.0, 1.5, dz),
            );
            bonus.transform = bonus.transform.with_scale(Vec3::splat(0.4));
            bonus.color = [0.4, 0.9, 0.6];
            bonus.emissive = 0.7;
            bonus.tappable = true;
            bonus.tap_action = TapAction::Hide;
            bonus.respawn_delay = 6.0 - hard;
            objects.push(bonus);
        }
        // Trophée : bonus le plus précieux (score continu), au sommet de la plateforme.
        let mut trophy = demo_obj(
            "Étoile Trophée",
            MeshKind::Sphere,
            Vec3::new(-5.0, 2.1, 0.0),
        );
        trophy.transform = trophy.transform.with_scale(Vec3::splat(0.55));
        trophy.color = [1.0, 0.75, 0.25];
        trophy.emissive = 1.0;
        trophy.metallic = 0.4;
        trophy.tappable = true;
        trophy.tap_action = TapAction::Hide;
        trophy.respawn_delay = 5.0 - hard;
        objects.push(trophy);

        // --- Portique décoratif encadrant l'entrée côté sud (lisibilité + ambiance) ---
        for sx in [-1.6_f32, 1.6] {
            let mut post = demo_obj("Pilier Portique", MeshKind::Cube, Vec3::new(sx, 1.1, -5.6));
            post.transform = post.transform.with_scale(Vec3::new(0.5, 2.2, 0.5));
            post.physics = PhysicsKind::Static;
            post.color = [0.45, 0.4, 0.5];
            post.metallic = 0.5;
            post.roughness = 0.3;
            objects.push(post);
        }
        let mut lintel = demo_obj(
            "Linteau Portique",
            MeshKind::Cube,
            Vec3::new(0.0, 2.35, -5.6),
        );
        lintel.transform = lintel.transform.with_scale(Vec3::new(3.6, 0.4, 0.5));
        lintel.physics = PhysicsKind::Static;
        lintel.color = [0.45, 0.4, 0.5];
        lintel.metallic = 0.5;
        lintel.roughness = 0.3;
        objects.push(lintel);

        // --- Ennemis patrouilleurs : hazards mobiles (scriptés), infligent des **dégâts
        // progressifs** au contact (via `damage()`) plutôt qu'une mort instantanée comme
        // la lave — plus indulgent, encourage à esquiver/se replier plutôt qu'à figer la
        // partie au premier effleurement. Plus rapides et plus punitifs au niveau 2 (`hard`).
        // Pulsent en rouge (menace visuelle). Vaincus par l'attaque du joueur (à portée) :
        // disparaissent puis réapparaissent après un répit, plutôt que d'être éliminés
        // définitivement (le niveau reste tendu même après un bon coup).
        let enemy_speed = 1.0 + 0.4 * hard;
        let dmg_rate = 0.9 + 0.3 * hard;
        let mut enemy = |name: &str, pos: Vec3, script: String| {
            let mut e = demo_obj(name, MeshKind::Sphere, pos);
            e.transform = e.transform.with_scale(Vec3::new(0.7, 0.6, 0.7));
            e.color = [0.85, 0.08, 0.08];
            e.emissive = 0.5;
            e.trigger = true;
            e.combat = Some(Combat {
                attackable: true,
                ..Default::default()
            });
            e.respawn_delay = 8.0 - hard;
            e.script = script;
            objects.push(e);
        };
        // Sentinelle sud : va-et-vient devant l'entrée, le long du mur sud.
        enemy(
            "Ennemi Sentinelle",
            Vec3::new(0.0, 0.5, -7.0),
            format!(
                "local s = {enemy_speed}\n\
                 obj.x = math.sin(time * s) * 3.0\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );
        // Rôdeur est : va-et-vient le long du couloir est, entre le mur et les piliers.
        enemy(
            "Ennemi Rôdeur",
            Vec3::new(5.6, 0.5, 0.0),
            format!(
                "local s = {enemy_speed}\n\
                 obj.z = math.sin(time * s * 0.8) * 3.0\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );
        // Gardien du trésor : tourne en orbite près de la gemme suprême / du pont.
        enemy(
            "Ennemi Gardien",
            Vec3::new(2.2, 0.5, -2.2),
            format!(
                "local s = {enemy_speed}\n\
                 obj.x = 2.2 + math.cos(time * s * 0.9) * 1.1\n\
                 obj.z = -2.2 + math.sin(time * s * 0.9) * 1.1\n\
                 if obj.triggered then damage({dmg_rate} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = 0.7 + 0.3 * p; obj.g = 0.05; obj.b = 0.05"
            ),
        );

        // --- Torches aux 4 coins de l'arène (flamme émissive + halo de lumière chaude) ---
        let mut lights = vec![PointLight {
            // Lumière ponctuelle chaude au-dessus de l'arène (ambiance + lisibilité).
            position: [0.0, 6.0, 0.0],
            color: [1.0, 0.92, 0.78],
            intensity: 1.4,
            range: 16.0,
            ..PointLight::default()
        }];
        for (n, (cx, cz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let base = Vec3::new(cx * 6.9, 0.0, cz * 6.9);
            let mut torch = demo_obj(
                &format!("Torche {}", n + 1),
                MeshKind::Cube,
                base + Vec3::Y * 0.8,
            );
            torch.transform = torch.transform.with_scale(Vec3::new(0.3, 1.6, 0.3));
            torch.physics = PhysicsKind::Static;
            torch.color = [0.3, 0.28, 0.3];
            objects.push(torch);

            let mut flame = demo_obj(
                &format!("Flamme {}", n + 1),
                MeshKind::Sphere,
                base + Vec3::Y * 1.7,
            );
            flame.transform = flame.transform.with_scale(Vec3::splat(0.3));
            flame.color = [1.0, 0.55, 0.15];
            flame.emissive = 1.2;
            // Vacillement (déphasé par torche) : taille + teinte fluctuent, deux fréquences
            // superposées pour un scintillement moins mécanique qu'une simple sinusoïde.
            let phase = n as f32 * 1.7;
            flame.script = format!(
                "local f = 0.75 + 0.15 * math.sin(time * 9.0 + {phase}) \
                 + 0.10 * math.sin(time * 23.0 + {phase} * 2.0)\n\
                 obj.sx = 0.3 * f; obj.sy = 0.3 * f; obj.sz = 0.3 * f\n\
                 obj.r = 1.0; obj.g = 0.45 + 0.2 * f; obj.b = 0.1 + 0.15 * f"
            );
            objects.push(flame);

            lights.push(PointLight {
                position: (base + Vec3::Y * 1.7).into(),
                color: [1.0, 0.6, 0.25],
                intensity: 0.9,
                range: 6.0,
                ..PointLight::default()
            });
        }
        // Lueur rouge au ras de la lave : renforce le danger visuel de la zone mortelle.
        lights.push(PointLight {
            position: [0.0, 0.6, 0.0],
            color: [1.0, 0.35, 0.1],
            intensity: 1.1,
            range: 7.0,
            ..PointLight::default()
        });
        // Lueur violette autour de la gemme suprême, sur le pont : signale la récompense
        // la plus prestigieuse du niveau, visible de loin par contraste avec la lave.
        lights.push(PointLight {
            position: [0.0, 2.0, 0.0],
            color: [0.85, 0.4, 1.0],
            intensity: 0.8,
            range: 5.0,
            ..PointLight::default()
        });

        Scene {
            objects,
            camera_follow: true,
            point_lights: lights,
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            // Widgets HUD déclaratifs (Sprint 109) : score en bas-gauche et jauge de
            // vie en bas-droite, en plus des overlays historiques (barre de vie
            // haut-gauche, manche haut-centre…) — démontrent le système texte/jauge
            // dans un niveau réellement joué, sans remplacer les overlays déjà
            // éprouvés (vie, viseur…) ni leurs tests.
            hud_widgets: vec![
                HudWidget {
                    id: "score_label".into(),
                    anchor: HudAnchor::BottomLeft,
                    offset: [16.0, -16.0],
                    size: [0.0, 0.0],
                    kind: HudWidgetKind::Text {
                        content: "Score".into(),
                        binding: HudBinding::Score,
                    },
                },
                HudWidget {
                    id: "health_gauge".into(),
                    anchor: HudAnchor::BottomRight,
                    offset: [-16.0, -16.0],
                    size: [140.0, 14.0],
                    kind: HudWidgetKind::Gauge {
                        binding: HudBinding::Health,
                        max: 1.0,
                        color: [0.8, 0.15, 0.15],
                    },
                },
            ],
            ..Default::default()
        }
    }

    /// Démo « Tour d'ascension » : style de jeu très différent de la démo contrôleur
    /// (arène de combat) — pur platforming vertical, sans ennemi ni combat. Plateformes
    /// en spirale à gravir jusqu'au sommet ; une chute hors des plateformes est une mort
    /// instantanée (vide en contrebas), ce qui remplace la lave comme unique danger.
    pub fn tower_demo() -> Self {
        let mut objects = Vec::new();

        // Sol de départ (petit, juste pour l'atterrissage initial — pas d'arène close ici,
        // le style est vertical, pas horizontal).
        let mut sol = demo_obj("Socle", MeshKind::Cylinder, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(4.0, 0.6, 4.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.4, 0.5];
        objects.push(sol);

        // Joueur pilotable : mêmes contrôles que la démo contrôleur (joystick + saut),
        // mais ici la précision de saut est ce qui compte, pas le combat.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Tag lu par les scripts de comportement des créatures 12/13
        // (`find_tag("joueur")` — rôdeur qui maintient sa distance, méduse qui
        // fuit) : sans lui, elles retombent sur leur comportement sans cible.
        joueur.tag = "joueur".into();
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.7,
            ..Default::default()
        });
        objects.push(joueur);

        // Vide mortel loin en contrebas : toute chute hors des plateformes est une mort
        // instantanée (remplace la lave comme unique danger de ce style de niveau). Même
        // technique que la lave : l'échelle Y épaissit l'AABB de collision (le mesh Plane
        // a une épaisseur visuelle nulle, cf. note dans `controller_level`) pour détecter
        // fiablement le joueur en chute malgré le pas de simulation fixe.
        let mut vide = demo_obj("Vide", MeshKind::Plane, Vec3::new(0.0, -4.0, 0.0));
        vide.transform = vide.transform.with_scale(Vec3::new(80.0, 60.0, 80.0));
        vide.color = [0.05, 0.05, 0.12];
        vide.deadly = true;
        objects.push(vide);

        // --- Plateformes en spirale ascendante : 4 positions en rotation (avant/droite/
        // arrière/gauche), qui montent d'un cran à chaque tour. Chaque plateforme porte une
        // gemme-objectif (obligatoire pour gagner) légèrement au-dessus, au centre.
        const N: usize = 16;
        for i in 0..N {
            let angle_step = (i % 4) as f32;
            let (dx, dz) = match angle_step as u32 {
                0 => (0.0, -2.6),
                1 => (2.6, 0.0),
                2 => (0.0, 2.6),
                _ => (-2.6, 0.0),
            };
            let y = 1.4 + i as f32 * 1.75;
            let pos = Vec3::new(dx, y, dz);

            let mut plat = demo_obj(&format!("Plateforme {}", i + 1), MeshKind::Cylinder, pos);
            plat.transform = plat.transform.with_scale(Vec3::new(1.6, 0.35, 1.6));
            plat.physics = PhysicsKind::Static;
            // Dégradé froid (bleu nuit → cyan clair) à mesure qu'on grimpe : lisibilité de
            // la progression même sans HUD de score consulté.
            let t = i as f32 / (N - 1) as f32;
            plat.color = [0.25 + 0.15 * t, 0.4 + 0.35 * t, 0.55 + 0.35 * t];
            plat.metallic = 0.3;
            plat.roughness = 0.35;
            objects.push(plat);

            let mut gem = demo_obj(
                &format!("Gemme {}", i + 1),
                MeshKind::Sphere,
                pos + Vec3::Y * 0.85,
            );
            gem.transform = gem.transform.with_scale(Vec3::splat(0.4));
            gem.color = [0.6, 0.9, 1.0];
            gem.emissive = 0.7;
            gem.tappable = true;
            gem.tap_action = TapAction::Hide;
            objects.push(gem);
        }

        // Trophée décoratif au sommet, au-dessus de la dernière plateforme : bonus (score
        // continu, ne bloque pas la victoire — gagner = avoir gravi toute la tour).
        let top = Vec3::new(0.0, 1.4 + (N - 1) as f32 * 1.75, 0.0)
            + match ((N - 1) % 4) as u32 {
                0 => Vec3::new(0.0, 0.0, -2.6),
                1 => Vec3::new(2.6, 0.0, 0.0),
                2 => Vec3::new(0.0, 0.0, 2.6),
                _ => Vec3::new(-2.6, 0.0, 0.0),
            };
        let mut trophy = demo_obj("Étoile Sommet", MeshKind::Sphere, top + Vec3::Y * 1.6);
        trophy.transform = trophy.transform.with_scale(Vec3::splat(0.55));
        trophy.color = [1.0, 0.85, 0.3];
        trophy.emissive = 1.1;
        trophy.tappable = true;
        trophy.tap_action = TapAction::Hide;
        trophy.respawn_delay = 6.0;
        objects.push(trophy);

        // Étoiles décoratives (ciel nocturne) : petits points statiques loin en hauteur,
        // pure ambiance — contraste avec les torches chaudes de la démo contrôleur.
        for i in 0..24 {
            let a = i as f32 * 2.399963; // angle doré : répartition sans motif visible
            let r = 6.0 + (i % 5) as f32 * 3.0;
            let h = 4.0 + (i * 7 % 40) as f32;
            let mut star = demo_obj(
                &format!("Étoile Ciel {}", i + 1),
                MeshKind::Sphere,
                Vec3::new(a.cos() * r, h, a.sin() * r),
            );
            star.transform = star.transform.with_scale(Vec3::splat(0.12));
            star.color = [0.85, 0.9, 1.0];
            star.emissive = 1.0;
            objects.push(star);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 6.0, 0.0],
                    color: [0.75, 0.85, 1.0],
                    intensity: 1.2,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: top.into(),
                    color: [1.0, 0.9, 0.7],
                    intensity: 1.3,
                    range: 10.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Course infinie » (style Temple Run) : 3ᵉ style de jeu, distinct de l'arène
    /// de combat et de la tour de platforming — course automatique en avant, le joueur ne
    /// contrôle que le changement de voie (gauche/centre/droite) et le saut. Obstacles à
    /// esquiver (voie) ou à sauter, pièces à ramasser, ligne d'arrivée obligatoire.
    /// (Piste longue et procédurale plutôt que réellement infinie : le moteur n'a pas de
    /// génération/dé-spawn à la volée — cf. `Scene::temple_run_demo` pour le détail.)
    pub fn temple_run_demo() -> Self {
        const LANES: [f32; 3] = [-2.2, 0.0, 2.2];
        const TRACK_LEN: f32 = 190.0;

        let mut objects = Vec::new();

        // Sol unique sur toute la longueur de la piste.
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, TRACK_LEN * 0.5));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(8.0, 1.0, TRACK_LEN + 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.42, 0.38, 0.5];
        objects.push(sol);

        // Murs latéraux : bloquent (sans tuer) toute sortie des 3 voies.
        for sx in [-3.6_f32, 3.6] {
            let mut wall = demo_obj(
                "Mur Voie",
                MeshKind::Cube,
                Vec3::new(sx, 0.9, TRACK_LEN * 0.5),
            );
            wall.transform = wall
                .transform
                .with_scale(Vec3::new(0.4, 1.8, TRACK_LEN + 10.0));
            wall.physics = PhysicsKind::Static;
            wall.color = [0.3, 0.28, 0.4];
            objects.push(wall);
        }

        // Joueur : course automatique en +Z, le joystick/clavier X ne pilote que la voie.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, -3.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 7.0,
            auto_run_speed: 5.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });
        objects.push(joueur);

        // --- Génération procédurale de la piste : motifs répétés tous les 6 m, densité
        // croissante avec la distance (difficulté progressive, comme un vrai endless runner).
        let seg_len = 6.0_f32;
        let n_segments = (TRACK_LEN / seg_len) as u32;
        for seg in 0..n_segments {
            let z = 8.0 + seg as f32 * seg_len;
            // Les 2 premiers segments sont un couloir d'échauffement (aucun obstacle).
            if seg < 2 {
                continue;
            }
            match seg % 5 {
                0 => {
                    // Haie à sauter (barre pleine largeur, franchissable au saut : sa
                    // hauteur réelle de 1,1 m couvre naturellement l'AABB nécessaire pour
                    // détecter un joueur debout sans la traverser en l'air — contrairement
                    // à un mesh Plane plat, un Cube épais n'a pas besoin de l'astuce
                    // d'échelle Y utilisée pour la lave (cf. `controller_level`).
                    let mut haie = demo_obj("Haie", MeshKind::Cube, Vec3::new(0.0, 0.55, z));
                    haie.transform = haie.transform.with_scale(Vec3::new(7.0, 1.1, 0.6));
                    haie.color = [0.75, 0.35, 0.2];
                    haie.deadly = true;
                    objects.push(haie);
                }
                1 => {
                    // Barrage : 2 des 3 voies bloquées (hauteur non franchissable au saut),
                    // la voie ouverte tourne à chaque occurrence pour ne pas être mémorisable.
                    let open = (seg / 5) % 3;
                    for (lane, &lx) in LANES.iter().enumerate() {
                        if lane as u32 == open {
                            continue;
                        }
                        let mut bar = demo_obj("Barrage", MeshKind::Cube, Vec3::new(lx, 1.0, z));
                        bar.transform = bar.transform.with_scale(Vec3::new(1.8, 2.0, 0.6));
                        bar.color = [0.6, 0.2, 0.2];
                        bar.deadly = true;
                        objects.push(bar);
                    }
                }
                2 => {
                    // Arc de pièces sur les 3 voies : encourage à zigzaguer.
                    for &lx in &LANES {
                        let mut coin = demo_obj("Pièce", MeshKind::Sphere, Vec3::new(lx, 1.0, z));
                        coin.transform = coin.transform.with_scale(Vec3::splat(0.4));
                        coin.color = [1.0, 0.85, 0.2];
                        coin.emissive = 0.6;
                        coin.tappable = true;
                        coin.tap_action = TapAction::Hide;
                        // Bonus (score continu) : ne bloque pas la victoire, seule la
                        // ligne d'arrivée (plus bas) compte comme objectif obligatoire.
                        coin.respawn_delay = 999.0;
                        objects.push(coin);
                    }
                }
                3 => {
                    // Ligne de pièces dans une seule voie (récompense un choix de trajectoire).
                    let lane = (seg / 3) % 3;
                    let mut coin = demo_obj(
                        "Pièce",
                        MeshKind::Sphere,
                        Vec3::new(LANES[lane as usize], 1.0, z),
                    );
                    coin.transform = coin.transform.with_scale(Vec3::splat(0.4));
                    coin.color = [1.0, 0.85, 0.2];
                    coin.emissive = 0.6;
                    coin.tappable = true;
                    coin.tap_action = TapAction::Hide;
                    coin.respawn_delay = 999.0;
                    objects.push(coin);
                }
                _ => {} // couloir de respiration : pas d'obstacle
            }
        }

        // Ligne d'arrivée : seul objectif obligatoire (victoire = l'atteindre), un portique
        // lumineux bien visible + une étoile à ramasser.
        let finish_z = 8.0 + n_segments as f32 * seg_len + 4.0;
        for sx in [-3.2_f32, 3.2] {
            let mut post = demo_obj(
                "Pilier Arrivée",
                MeshKind::Cube,
                Vec3::new(sx, 1.4, finish_z),
            );
            post.transform = post.transform.with_scale(Vec3::new(0.5, 2.8, 0.5));
            post.physics = PhysicsKind::Static;
            post.color = [0.9, 0.75, 0.2];
            post.metallic = 0.5;
            objects.push(post);
        }
        let mut lintel = demo_obj(
            "Linteau Arrivée",
            MeshKind::Cube,
            Vec3::new(0.0, 3.0, finish_z),
        );
        lintel.transform = lintel.transform.with_scale(Vec3::new(6.9, 0.4, 0.5));
        lintel.physics = PhysicsKind::Static;
        lintel.color = [0.9, 0.75, 0.2];
        lintel.metallic = 0.5;
        objects.push(lintel);

        let mut finish = demo_obj(
            "Étoile Arrivée",
            MeshKind::Sphere,
            Vec3::new(0.0, 1.5, finish_z),
        );
        finish.transform = finish.transform.with_scale(Vec3::splat(0.6));
        finish.color = [1.0, 0.9, 0.3];
        finish.emissive = 1.2;
        finish.tappable = true;
        finish.tap_action = TapAction::Hide;
        // respawn_delay = 0 (défaut) ⇒ objectif obligatoire : seule pièce dont la victoire dépend.
        objects.push(finish);

        Scene {
            objects,
            camera_follow: true,
            // Éclairage réparti le long de la piste (190 m) : la lumière directionnelle
            // par défaut (`light`) couvre l'ambiance générale, ces points ponctuels
            // renforcent la lisibilité aux endroits clés (départ, milieu, arrivée).
            point_lights: [10.0, 70.0, 130.0, finish_z]
                .into_iter()
                .map(|z| PointLight {
                    position: [0.0, 8.0, z],
                    color: [1.0, 0.95, 0.85],
                    intensity: 1.2,
                    range: 26.0,
                    ..PointLight::default()
                })
                .collect(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Scène **exemple**, minimale et commentée : montre les 3 composants optionnels
    /// (`Controller`, `AudioSource`, `Combat`) chacun sur un seul objet, sans le décor
    /// dense d'un vrai niveau. Sert de référence rapide pour qui étend le moteur — pas
    /// une démo de gameplay comme les autres (arène/tour/course).
    pub fn components_demo() -> Self {
        // Sol minimal (juste assez pour marcher/sauter).
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(10.0, 1.0, 10.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.4, 0.45, 0.5];

        // --- Controller : rend un objet pilotable (joystick + saut + attaque). `None`
        // pour tous les autres objets de cette scène — un seul joueur en a besoin.
        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-2.5, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 1.5,
            ..Default::default()
        });

        // --- AudioSource : son associé à un objet (clip/autoplay/spatialisation). Le
        // clip est vide ici (aucun fichier son fourni avec l'exemple) — assigne-en un
        // via l'inspecteur (panneau Audio › Choisir un son…) pour l'entendre en Play.
        let mut boite = demo_obj("Boîte à musique", MeshKind::Cube, Vec3::new(0.0, 0.5, 2.0));
        boite.color = [0.6, 0.4, 0.8];
        boite.audio = Some(AudioSource {
            clip: String::new(),
            autoplay: true,
            spatial: true,
            ..Default::default()
        });

        // --- Combat : cible d'attaque (`attackable`) et ancre visuelle de l'effet
        // d'impact (`is_attack_fx`), rarement sur le même objet (ici, deux objets
        // séparés). Approche le joueur et appuie sur Attaque (ou touche J) pour tester.
        let mut cible = demo_obj(
            "Cible d'entraînement",
            MeshKind::Sphere,
            Vec3::new(2.5, 1.0, 0.0),
        );
        cible.color = [0.85, 0.15, 0.15];
        cible.emissive = 0.4;
        cible.combat = Some(Combat {
            attackable: true,
            ..Default::default()
        });
        cible.respawn_delay = 3.0;

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        Scene {
            objects: vec![sol, joueur, boite, cible, fx],
            camera_follow: true,
            point_lights: vec![PointLight {
                position: [0.0, 5.0, 0.0],
                color: [1.0, 0.95, 0.85],
                intensity: 1.2,
                range: 14.0,
                ..PointLight::default()
            }],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Vagues de zombies » : jeu **local contre l'ordinateur**, sans réseau, en
    /// **manches** (style Call of Duty Zombies) — 3 archétypes de monstres (`AiChaser`,
    /// poursuite active, pas de patrouille scriptée), de plus en plus nombreux et variés
    /// à chaque manche. Vaincre tous les monstres d'une manche révèle la suivante ; la
    /// dernière vaincue ⇒ victoire (`App` pilote la progression, cf. `AppState::wave`).
    pub fn zombies_demo() -> Self {
        let half = 10.0_f32;
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.22, 0.24, 0.28];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Portée courte (0,7 m, pas 1,5) : audit gameplay — un bot qui approche puis
        // attaque au cooldown ne prenait jamais un seul point de dégâts sur les 4 manches,
        // la portée dépassant bien trop largement le rayon de morsure des monstres.
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            attack_button: "Attaque".into(),
            attack_range: 0.7,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour.
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.3, 0.32, 0.38];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );

        // Piliers de couverture : obstacles pour casser une poursuite (les monstres ne
        // les contournent pas intelligemment, ils foncent tout droit vers le joueur).
        for (sx, sz) in [
            (-3.0_f32, 2.0),
            (3.0, -2.0),
            (0.0, 5.5),
            (-4.0, -5.0),
            (4.5, 4.5),
        ] {
            let mut pilier = demo_obj("Pilier", MeshKind::Cylinder, Vec3::new(sx, 0.9, sz));
            pilier.transform = pilier.transform.with_scale(Vec3::new(1.4, 1.8, 1.4));
            pilier.physics = PhysicsKind::Static;
            pilier.color = [0.4, 0.4, 0.45];
            objects.push(pilier);
        }

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        objects.push(fx);

        // --- 3 archétypes de monstres, de plus en plus présents/variés à chaque manche
        // (comme les vagues d'un mode zombies) : Rôdeur (basique), Coureur (rapide et
        // fragile), Brute (lente mais très punitive et plus difficile à esquiver).
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
        }
        const RODEUR: Kind = Kind {
            label: "Rôdeur",
            speed: 2.6,
            dmg: 0.8,
            scale: 0.7,
            color: [0.35, 0.55, 0.25],
        };
        const COUREUR: Kind = Kind {
            label: "Coureur",
            speed: 4.6,
            dmg: 0.5,
            scale: 0.55,
            color: [0.75, 0.8, 0.2],
        };
        const BRUTE: Kind = Kind {
            label: "Brute",
            speed: 1.8,
            dmg: 2.2,
            scale: 1.3,
            color: [0.45, 0.08, 0.25],
        };
        // (manche, archétypes de cette manche) — la difficulté monte : plus de monstres,
        // puis des archétypes plus dangereux introduits progressivement.
        let waves: &[(u32, &[&Kind])] = &[
            (1, &[&RODEUR, &RODEUR, &RODEUR]),
            (2, &[&RODEUR, &RODEUR, &RODEUR, &COUREUR, &COUREUR]),
            (
                3,
                &[
                    &RODEUR, &RODEUR, &COUREUR, &COUREUR, &COUREUR, &BRUTE, &BRUTE,
                ],
            ),
            (
                4,
                &[&RODEUR, &RODEUR, &COUREUR, &COUREUR, &BRUTE, &BRUTE, &BRUTE],
            ),
        ];
        let total: usize = waves.iter().map(|(_, ks)| ks.len()).sum();
        let mut spawned = 0usize;
        for &(wave, kinds) in waves {
            for (n, k) in kinds.iter().enumerate() {
                // Répartis en cercle sur tout le pourtour (indice global, pas par manche) :
                // les manches suivantes n'occupent pas les mêmes points que la précédente.
                let angle = spawned as f32 / total as f32 * std::f32::consts::TAU;
                let radius = half - 1.4;
                let pos = Vec3::new(
                    angle.cos() * radius,
                    k.scale.max(0.5) * 0.5,
                    angle.sin() * radius,
                );
                spawned += 1;

                let mut m = demo_obj(&format!("{} {}", k.label, n + 1), MeshKind::Sphere, pos);
                m.transform = m.transform.with_scale(Vec3::splat(k.scale));
                m.color = k.color;
                m.emissive = 0.5;
                m.trigger = true;
                m.ai_chaser = Some(AiChaser { speed: k.speed });
                m.combat = Some(Combat {
                    attackable: true,
                    wave,
                    ..Default::default()
                });
                // Pas de réapparition : un monstre vaincu reste mort pour la manche
                // (contrairement aux ennemis de l'arène de combat, qui reviennent).
                m.respawn_delay = 0.0;
                m.script = format!(
                    "if obj.triggered then damage({} * dt) end\n\
                     local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                     obj.r = {} + {} * p; obj.g = {}; obj.b = {}",
                    k.dmg,
                    k.color[0] * 0.7,
                    k.color[0] * 0.3,
                    k.color[1] * 0.6,
                    k.color[2] * 0.6,
                );
                objects.push(m);
            }
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 9.0, 0.0],
                    color: [0.75, 0.85, 1.0],
                    intensity: 1.3,
                    range: 24.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 3.0, 0.0],
                    color: [1.0, 0.5, 0.3],
                    intensity: 0.7,
                    range: 10.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démi-étendue (m) de la carte MMORPG : seule source de vérité de sa taille —
    /// sol, murs, bornes des scripts de créatures (`arena_half` passé aux
    /// générateurs) et gardes des tests (`simulation.rs`) en dérivent tous.
    /// 36.0 = carte 72×72 m (×3 l'arène d'origine de 24×24) pour loger les
    /// biomes : prairie centrale, forêt NE, lac et rivières à l'ouest, rizières
    /// SO, hameau et promontoire à l'est.
    pub(crate) const MMORPG_HALF: f32 = 36.0;

    /// Démo « MMORPG » : arène minimale dédiée au test multijoueur PC ↔ mobile —
    /// pas de monstres ni de manches (contrairement à `zombies_demo`), juste un
    /// joueur pilotable (joystick + saut) sur une
    /// carte simple avec quelques repères visuels statiques, pour voir
    /// clairement un joueur desktop et un joueur APK se déplacer l'un par
    /// rapport à l'autre (fantômes réseau, cf. `app::network_client`).
    pub fn mmorpg_demo() -> Self {
        let half = Self::MMORPG_HALF;
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        // Vert prairie (l'arène est habillée en coin de campagne, cf. le décor
        // nature plus bas) — l'ancien gris-vert sombre jurait avec les aplats.
        sol.color = [0.26, 0.38, 0.21];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        // Tag lu par les scripts de comportement des créatures 12/13/19
        // (`find_tag("joueur")` — rôdeur qui maintient sa distance, méduse qui
        // fuit, lanterne qui dérive vers lui) : sans lui, elles retombent sur
        // leur comportement sans cible (immobiles pour la lanterne).
        joueur.tag = "joueur".into();
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour (enferment l'aire de jeu, ne servent qu'à ne pas tomber).
        let mut wall = |name: &str, pos: Vec3, scale: Vec3| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.3, 0.32, 0.38];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, half),
            Vec3::new(2.0 * half, 1.8, 0.5),
        );
        wall(
            "Mur Est",
            Vec3::new(half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * half),
        );

        // Repères visuels statiques (juste pour situer les déplacements, sans danger)
        // aux quatre coins de la prairie centrale.
        for (n, (x, z)) in [
            (-15.0_f32, -15.0),
            (15.0, -15.0),
            (-15.0, 15.0),
            (15.0, 15.0),
        ]
        .into_iter()
        .enumerate()
        {
            let mut repere = demo_obj(
                &format!("Repère {}", n + 1),
                MeshKind::Cylinder,
                Vec3::new(x, 0.9, z),
            );
            repere.transform = repere.transform.with_scale(Vec3::new(1.0, 1.8, 1.0));
            repere.physics = PhysicsKind::Static;
            repere.color = [0.5, 0.45, 0.62];
            objects.push(repere);
        }

        // Zone de vent (Sprint 125, preuve d'implémentation visible dans une démo
        // jouée réellement plutôt que seulement en test unitaire) : plate-forme basse
        // et distinctement teintée (cyan) dans un coin de l'arène — traverser son AABB
        // pousse tout corps dynamique le long de `wind`, tant qu'il y reste. Pas de
        // collider (`PhysicsKind::None`) : une zone de vent ne doit rien bloquer,
        // seulement pousser.
        // Placée sur le « col venté » entre la prairie centrale et le promontoire
        // rocheux de l'est.
        let mut vent = demo_obj("Zone de vent", MeshKind::Cube, Vec3::new(14.0, 0.3, -4.0));
        vent.transform = vent.transform.with_scale(Vec3::new(8.0, 0.6, 8.0));
        vent.physics = PhysicsKind::None;
        vent.trigger = true;
        vent.wind = Some(Vec3::new(5.0, 0.0, 0.0));
        vent.color = [0.25, 0.75, 0.85];
        objects.push(vent);

        // Créatures qui errent (glb rigués/animés, cf. la doc de
        // `creature_wander_script`) : seule différence avec les repères ci-dessus,
        // des meshes importés skinnés plutôt que des primitives — chemins disque du
        // dépôt (pas `bundle://`) car cette démo tourne depuis les sources, jamais
        // depuis un export packagé. Table data-driven plutôt que dix blocs
        // copiés-collés (l'historique en comptait cinq avant les créatures 6-10) :
        // chaque entrée = (nom, fichier, spawn, bit de couche, préfixe de clés
        // `save`, attaque au contact éventuelle).
        //
        // Communs à toutes : échelle 0.35 (les meshes bruts font ~2-3 m de haut,
        // bbox locale non affectée par l'échelle Blender de l'objet, ignorée par
        // `import::load_gltf` qui ne lit que les sommets des primitives) ; corps
        // physique `Kinematic` (ne traversent ni joueur ni murs/objets fixes, cf.
        // `Physics::resolve_scripted_moves`) ; couche de collision dédiée (bit
        // propre) pour que leurs sondes raycast s'ignorent elles-mêmes tout en
        // voyant les autres ; spawns espacés, à bonne distance des murs/repères
        // (RAY_DIST 3,5 m devant chaque créature doit démarrer dégagé).
        //
        // Attaques : au contact (`bite`, cf. `creature_bite_script` — morsure de
        // la n°1, morsure rapide de la chauve-souris n°6, pincement lourd du crabe
        // n°7, chacun son tempérament et son `salt` de tirage) ; les attaques à
        // distance des n°3/8/9/10 sont natives, déclenchées par **nom d'objet**
        // (cf. `app::creature_attack::RANGED_CREATURE_ATTACKS`), rien à câbler ici.
        struct DemoCreature {
            name: &'static str,
            file: &'static str,
            spawn: Vec3,
            layer_bit: u32,
            prefix: &'static str,
            /// `Some((cooldown, chance, dégâts, salt))` : attaque au contact.
            bite: Option<(f32, f32, f32, f32)>,
            /// Cap initial (degrés) et déphasage du bruit de méandre — cf. la
            /// doc de `creature_wander_script` sur le bug qu'ils corrigent
            /// (créatures qui partaient toutes dans la même direction, en bloc
            /// contre le même mur). Passés à **tous** les générateurs pour un
            /// type de pointeur de fonction uniforme ; ceux qui n'en ont pas
            /// l'usage (sentinelle, rôdeur…) les ignorent (`_heading0`/`_phase`).
            heading0: f32,
            phase: f32,
            /// Générateur du script de comportement (patrouille à sondes,
            /// sentinelle, rôdeur, dérive, artillerie, zigzag…) — signature
            /// commune (demi-arène, préfixe `save`, masque de sonde, cap
            /// initial, déphasage), chaque générateur ignore ce dont il n'a
            /// pas besoin.
            script: fn(f32, &str, u32, f32, f32) -> String,
        }
        const MMORPG_CREATURES: &[DemoCreature] = &[
            DemoCreature {
                name: "Créature",
                file: "creature.glb",
                spawn: Vec3::new(0.0, 0.0, -4.0),
                layer_bit: 1,
                prefix: "creature_",
                heading0: 0.0,
                // `phase` décale le méandre du script d'errance (terme
                // `sin(time*0.35+phase)`, cf. `creature_wander_script`) : à
                // `phase = 0`, sur 30 s la dérive nette pousse systématiquement
                // vers le hameau (SE, façades planes abordées en tangente —
                // 3 rayons d'égale distance, quasi aucun signal d'évitement),
                // provoquant un arrêt prolongé. Inoffensif sur l'ancienne
                // arène 24 m (rien à heurter à cette distance), redevenu un
                // piège sur la carte 72 m. `-π/2` retarde et adoucit le
                // virage initial : la dérive longe la rizière sud, à
                // distance de tout décor solide sur les 30 s du test. Preuve :
                // `mmorpg_creature_never_gets_stuck_walking_into_a_wall`.
                phase: -std::f32::consts::FRAC_PI_2,
                bite: Some((2.2, 0.4, 0.12, 12.9898)),
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 2",
                file: "creature2.glb",
                spawn: Vec3::new(6.0, 0.0, 2.0),
                layer_bit: 2,
                prefix: "creature2_",
                heading0: 72.0,
                phase: 0.9,
                bite: None,
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 3",
                file: "creature3.glb",
                spawn: Vec3::new(-6.0, 0.0, 4.0),
                layer_bit: 3,
                prefix: "creature3_",
                heading0: 144.0,
                phase: 1.8,
                bite: None,
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 4",
                file: "creature4.glb",
                spawn: Vec3::new(-4.0, 0.0, -9.0),
                layer_bit: 4,
                prefix: "creature4_",
                heading0: 216.0,
                phase: 2.7,
                bite: None,
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 5",
                file: "creature5.glb",
                spawn: Vec3::new(7.0, 0.0, -7.0),
                layer_bit: 5,
                prefix: "creature5_",
                heading0: 288.0,
                phase: 3.6,
                bite: None,
                script: creature_wander_script,
            },
            // Chauve-souris : morsure rapide mais faible — harcèle plus qu'elle
            // ne punit.
            DemoCreature {
                name: "Créature 6",
                file: "creature6.glb",
                spawn: Vec3::new(20.0, 0.0, -20.0),
                layer_bit: 6,
                prefix: "creature6_",
                heading0: 24.0,
                phase: 4.5,
                bite: Some((1.6, 0.5, 0.08, 19.4142)),
                script: creature_wander_script,
            },
            // Crabe : pincement rare mais lourd — l'inverse de la chauve-souris.
            DemoCreature {
                name: "Créature 7",
                file: "creature7.glb",
                spawn: Vec3::new(-13.0, 0.0, 10.0),
                layer_bit: 7,
                prefix: "creature7_",
                heading0: 96.0,
                phase: 5.4,
                bite: Some((3.5, 0.6, 0.18, 27.1828)),
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 8",
                file: "creature8.glb",
                spawn: Vec3::new(24.0, 0.0, 18.0),
                layer_bit: 8,
                prefix: "creature8_",
                heading0: 168.0,
                phase: 6.3,
                bite: None,
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 9",
                file: "creature9.glb",
                spawn: Vec3::new(-4.0, 0.0, 29.0),
                layer_bit: 9,
                prefix: "creature9_",
                heading0: 240.0,
                phase: 7.2,
                bite: None,
                script: creature_wander_script,
            },
            DemoCreature {
                name: "Créature 10",
                file: "creature10.glb",
                spawn: Vec3::new(12.0, 0.0, -10.0),
                layer_bit: 10,
                prefix: "creature10_",
                heading0: 312.0,
                phase: 8.1,
                bite: None,
                script: creature_wander_script,
            },
            // 11-15 : la génération « qualité supérieure » — attaques à
            // distance toutes différentes (cf. `creature_attack::AttackStyle`)
            // et comportements dédiés, cohérents avec leur attaque.
            DemoCreature {
                name: "Créature 11",
                file: "creature11.glb",
                spawn: Vec3::new(10.0, 0.0, 16.0),
                layer_bit: 11,
                prefix: "creature11_",
                heading0: 48.0,
                phase: 9.0,
                bite: None,
                script: creature_guard_script,
            },
            DemoCreature {
                name: "Créature 12",
                file: "creature12.glb",
                spawn: Vec3::new(24.0, 0.0, -14.0),
                layer_bit: 12,
                prefix: "creature12_",
                heading0: 120.0,
                phase: 9.9,
                bite: None,
                script: creature_kite_script,
            },
            DemoCreature {
                name: "Créature 13",
                file: "creature13.glb",
                spawn: Vec3::new(-19.0, 0.0, 2.0),
                layer_bit: 13,
                prefix: "creature13_",
                heading0: 192.0,
                phase: 10.8,
                bite: None,
                script: creature_drift_script,
            },
            DemoCreature {
                name: "Créature 14",
                file: "creature14.glb",
                spawn: Vec3::new(-12.0, 0.0, 22.0),
                layer_bit: 14,
                prefix: "creature14_",
                heading0: 264.0,
                phase: 11.7,
                bite: None,
                script: creature_artillery_script,
            },
            DemoCreature {
                name: "Créature 15",
                file: "creature15.glb",
                spawn: Vec3::new(-20.5, 0.0, 15.5),
                layer_bit: 15,
                prefix: "creature15_",
                heading0: 336.0,
                phase: 12.6,
                bite: None,
                script: creature_zigzag_script,
            },
            // 16-20 : même palier de qualité que 11-15 — attaques et
            // comportements tout aussi variés, cf. `creature_attack.rs`.
            DemoCreature {
                name: "Créature 16",
                file: "creature16.glb",
                // Audit gameplay : l'ancien spawn (1.5, 7.5) mettait le cercle de
                // patrouille (R 3,5) à cheval sur la cabane et le mur nord —
                // blocages permanents. Est de l'arène, dégagé : rochers, arbres,
                // repères et murs tous à > 1,5 m du cercle, et hors de la zone
                // d'errance des créatures 1-5 (un premier essai au centre faisait
                // s'arrêter la n°1 à chaque passage du griffon devant ses sondes).
                spawn: Vec3::new(-2.0, 0.0, -18.0),
                layer_bit: 16,
                prefix: "creature16_",
                heading0: 12.0,
                phase: 13.5,
                bite: None,
                script: creature_soar_script,
            },
            DemoCreature {
                name: "Créature 17",
                file: "creature17.glb",
                // Audit gameplay : l'ancien spawn (-0.5, -10.5) faisait mordre
                // l'aile sud du huit (± 1,5 en z) sur le mur — remonté pour que
                // toute la courbe reste dans l'arène, loin des rochers 2/3.
                spawn: Vec3::new(-20.0, 0.0, -26.0),
                layer_bit: 17,
                prefix: "creature17_",
                heading0: 84.0,
                phase: 14.4,
                bite: None,
                script: creature_lemniscate_script,
            },
            DemoCreature {
                name: "Créature 18",
                file: "creature18.glb",
                spawn: Vec3::new(18.0, 0.0, 27.0),
                layer_bit: 18,
                prefix: "creature18_",
                heading0: 156.0,
                phase: 15.3,
                bite: None,
                script: creature_burrow_script,
            },
            DemoCreature {
                name: "Créature 19",
                file: "creature19.glb",
                spawn: Vec3::new(19.5, 0.0, 2.5),
                layer_bit: 19,
                prefix: "creature19_",
                heading0: 228.0,
                phase: 16.2,
                bite: None,
                script: creature_hover_script,
            },
            DemoCreature {
                name: "Créature 20",
                file: "creature20.glb",
                spawn: Vec3::new(26.0, 0.0, 13.0),
                layer_bit: 20,
                prefix: "creature20_",
                heading0: 300.0,
                phase: 17.1,
                bite: None,
                script: creature_turret_script,
            },
        ];

        let mut imported = Vec::new();
        for spec in MMORPG_CREATURES {
            let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), spec.file);
            match crate::scene::import::load_gltf(&path) {
                Ok((data, aabb_min, aabb_max)) => {
                    let mut mesh = ImportedMesh {
                        path: path.clone(),
                        data,
                        aabb_min,
                        aabb_max,
                        ..Default::default()
                    };
                    mesh.load_skinning();
                    let mesh_index = imported.len() as u32;
                    let mut creature =
                        demo_obj(spec.name, MeshKind::Imported(mesh_index), spec.spawn);
                    creature.transform = creature.transform.with_scale(Vec3::splat(0.35));
                    creature.animation = Some(AnimationState {
                        clip: "Idle".into(),
                        ..Default::default()
                    });
                    creature.physics = PhysicsKind::Kinematic;
                    creature.collision_layer = 1 << spec.layer_bit;
                    // Tuable (boule de feu/mêlée) et synchronisée en réseau — même
                    // pipeline générique que les autres monstres `Combat::attackable`
                    // (`fireball_impact`/`attack_at`, `AppState::network_snapshot`) :
                    // rien de spécifique aux créatures scriptées à ajouter côté mise à
                    // mort, cf. GAMEDESIGN_EN_LIGNE.md et ROADMAP (synchro réseau).
                    creature.combat = Some(Combat {
                        attackable: true,
                        ..Default::default()
                    });
                    let wander = (spec.script)(
                        half,
                        spec.prefix,
                        !(1_u32 << spec.layer_bit),
                        spec.heading0,
                        spec.phase,
                    );
                    creature.script = match spec.bite {
                        Some((cooldown, chance, damage, salt)) => {
                            // `trigger = true` active la détection de contact
                            // (`obj.triggered`) nécessaire à l'attaque, sans
                            // changer son collider (toujours solide).
                            creature.trigger = true;
                            // Persisté aussi nativement (`SceneObject::bite`), en plus
                            // du script Lua ci-dessous : le script pilote la version
                            // solo, ce champ permet à `app::health` de retrouver
                            // « quelles créatures mordent » côté réseau sans
                            // redécouvrir chaque nom en dur (cf. sa doc).
                            creature.bite = Some(crate::scene::BiteAttack {
                                cooldown,
                                chance,
                                damage,
                            });
                            format!(
                                "{wander}\n{}",
                                creature_bite_script(spec.prefix, cooldown, chance, damage, salt)
                            )
                        }
                        None => wander,
                    };
                    objects.push(creature);
                    imported.push(mesh);
                }
                Err(e) => log::error!("{} MMORPG ({path}) : {e}", spec.name),
            }
        }

        // --- Décor « nature » : la carte 72×72 devient un petit monde ------------
        // Biomes (nord = -Z) : prairie centrale autour du spawn joueur, forêt
        // dense au nord-est (deux clairières habitées), lac et rivières à
        // l'ouest (deux ponts), rizières en damier au sud-ouest, hameau sur la
        // route est-ouest et promontoire rocheux à l'est (tour de guet). La
        // carte reste PLATE : le sol est un `Plane` et les scripts des
        // créatures ne suivent aucun relief — le « promontoire » est un anneau
        // de rochers au sol, pas une élévation. Trois couches :
        //
        // 1) Aplats de terrain : primitives `Plane` sans collider, décalées de
        //    quelques centimètres en Y (eau < sable < chemins < route) pour
        //    éviter le z-fighting avec le sol et entre elles. L'eau n'a pas de
        //    collider : rivières et lac se traversent à gué, les ponts sont
        //    narratifs (et plus rapides que patauger entre deux berges).
        let eau = [0.18, 0.42, 0.65];
        let eau_sombre = [0.14, 0.34, 0.55];
        let terre = [0.42, 0.36, 0.26];
        let vert_riziere = [0.24, 0.44, 0.38];
        let mut aplat = |name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]| {
            let mut p = demo_obj(name, MeshKind::Plane, pos);
            p.transform = p.transform.with_scale(scale);
            p.color = color;
            objects.push(p);
        };
        aplat(
            "Rivière nord",
            Vec3::new(-26.0, 0.02, -21.0),
            Vec3::new(4.0, 1.0, 30.0),
            eau,
        );
        aplat(
            "Coude de rivière",
            Vec3::new(-22.0, 0.02, -6.0),
            Vec3::new(12.0, 1.0, 4.0),
            eau,
        );
        aplat(
            "Lac",
            Vec3::new(-19.0, 0.015, 4.0),
            Vec3::new(14.0, 1.0, 12.0),
            eau_sombre,
        );
        aplat(
            "Rivière sud",
            Vec3::new(-16.0, 0.02, 23.0),
            Vec3::new(4.0, 1.0, 26.0),
            eau,
        );
        aplat(
            "Berge du lac",
            Vec3::new(-19.0, 0.012, 12.0),
            Vec3::new(14.0, 1.0, 3.0),
            [0.72, 0.64, 0.44],
        );
        aplat(
            "Route principale",
            Vec3::new(0.0, 0.03, 14.0),
            Vec3::new(2.0 * half, 1.0, 2.2),
            terre,
        );
        aplat(
            "Chemin du hameau",
            Vec3::new(10.0, 0.028, 2.0),
            Vec3::new(2.0, 1.0, 24.0),
            terre,
        );
        aplat(
            "Chemin du pont nord",
            Vec3::new(-10.0, 0.028, -10.0),
            Vec3::new(32.0, 1.0, 1.6),
            terre,
        );
        for (i, (rx, rz)) in [
            (-8.0_f32, 26.0),
            (-1.0, 26.0),
            (-8.0, 32.0),
            (-1.0, 32.0),
            (6.0, 29.0),
        ]
        .into_iter()
        .enumerate()
        {
            aplat(
                &format!("Rizière {}", i + 1),
                Vec3::new(rx, 0.015, rz),
                Vec3::new(6.0, 1.0, 5.0),
                vert_riziere,
            );
        }

        // 2) Meshes glb générés par Blender headless (gen_nature_pack.py pour
        //    les statiques, gen_nature_animated.py pour les riggés). `solide` →
        //    corps statique avec collider `TriMesh` (silhouette exacte : les
        //    ponts se traversent à pied, on se faufile entre les troncs) ;
        //    sinon pur décor traversable (fleurs, riz, roseaux, panneaux…).
        //    `anim` → l'instance reçoit un `AnimationState` sur le clip nommé :
        //    le mesh reste partagé entre instances (chargé une seule fois),
        //    seul l'état d'animation est par objet — même mécanique que les
        //    créatures. Le TriMesh d'un solide animé est celui de la POSE DE
        //    REPOS (l'animation est purement visuelle) : les parties mobiles
        //    des moulins sont hors de portée du joueur (en hauteur/côté eau).
        //    Tout décor solide respecte ≥ 3,5 m (RAY_DIST des sondes) de
        //    dégagement autour des spawns de créatures — vérifié par le test
        //    `mmorpg_demo_contains_walkable_nature_decor`, y compris pour le
        //    décor semé procéduralement ci-dessous.
        struct DemoDecor {
            name: &'static str,
            file: &'static str,
            pos: (f32, f32),
            scale: f32,
            /// Cap (degrés autour de Y) : oriente le décor vers ce qu'il
            /// raconte (portes des cabanes vers la route, roue du moulin côté
            /// rivière…). Les glb sortent de Blender porte vers -Z.
            yaw_deg: f32,
            solide: bool,
            /// Clip joué en boucle (assets riggés de gen_nature_animated.py).
            anim: Option<&'static str>,
        }
        // Landmarks posés à la main : le hameau (centre-est, sur la route), le
        // promontoire (tour + anneau de rochers), les ponts, les moulins et la
        // vie des berges. Le reste (forêt, lisières, fleurs, riz) est semé
        // procéduralement plus bas.
        const NATURE_DECOR: &[DemoDecor] = &[
            DemoDecor {
                name: "Pont 1",
                file: "nature_bridge.glb",
                pos: (-16.0, 14.0),
                scale: 1.15,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Pont 2",
                file: "nature_bridge.glb",
                pos: (-26.0, -10.0),
                scale: 1.15,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            // Hameau : deux cabanes porte vers la route (au sud d'elles, d'où
            // le cap 180°), la hutte de l'autre côté de la route, le puits, la
            // charrette, le torii qui marque l'entrée du chemin.
            DemoDecor {
                name: "Cabane 1",
                file: "nature_cabin.glb",
                pos: (6.0, 9.0),
                scale: 1.0,
                yaw_deg: 180.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Cabane 2",
                file: "nature_cabin.glb",
                pos: (14.0, 9.0),
                scale: 1.0,
                yaw_deg: 180.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Hutte",
                file: "nature_hut.glb",
                pos: (10.0, 21.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Puits",
                file: "nature_well.glb",
                pos: (10.0, 7.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Charrette",
                file: "nature_cart.glb",
                pos: (7.5, 11.5),
                scale: 1.0,
                yaw_deg: 35.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Torii",
                file: "nature_torii.glb",
                pos: (10.0, 12.2),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Tas de bois",
                file: "nature_woodpile.glb",
                pos: (4.5, 10.8),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Bannière",
                file: "nature_banner.glb",
                pos: (13.0, 4.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            // Encadrent l'entrée du torii, de part et d'autre du chemin.
            DemoDecor {
                name: "Lanterne 1",
                file: "nature_lantern.glb",
                pos: (5.0, 13.5),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Lanterne 2",
                file: "nature_lantern.glb",
                pos: (15.0, 13.5),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            // Clôtures des potagers, de part et d'autre des cabanes.
            DemoDecor {
                name: "Clôture 1",
                file: "nature_fence.glb",
                pos: (4.0, 6.2),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture 2",
                file: "nature_fence.glb",
                pos: (16.0, 6.2),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture 3",
                file: "nature_fence.glb",
                pos: (2.2, 8.2),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture 4",
                file: "nature_fence.glb",
                pos: (18.5, 9.5),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: None,
            },
            // Promontoire : la tour de guet au centre d'un anneau de rochers,
            // tous à ≥ 4 m de la tortue-canon (26, 13) qui y niche.
            DemoDecor {
                name: "Tour de guet",
                file: "nature_tower.glb",
                pos: (27.0, 9.0),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher 1",
                file: "nature_rock.glb",
                pos: (31.2, 9.6),
                scale: 1.8,
                yaw_deg: 40.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher 2",
                file: "nature_rock.glb",
                pos: (29.6, 4.8),
                scale: 1.5,
                yaw_deg: 130.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher 3",
                file: "nature_rock.glb",
                pos: (24.0, 4.4),
                scale: 1.9,
                yaw_deg: 210.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher 4",
                file: "nature_rock.glb",
                pos: (22.4, 8.8),
                scale: 1.4,
                yaw_deg: 300.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher 5",
                file: "nature_rock.glb",
                pos: (30.4, 13.4),
                scale: 1.6,
                yaw_deg: 75.0,
                solide: true,
                anim: None,
            },
            // Moulins : la roue à aubes trempe dans la rivière sud (cap 180° —
            // la roue sort de Blender côté +X), les pales du moulin à vent
            // dominent les rizières.
            DemoDecor {
                name: "Moulin à eau",
                file: "nature_watermill.glb",
                pos: (-13.2, 17.0),
                scale: 1.0,
                yaw_deg: 180.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Moulin à vent",
                file: "nature_windmill.glb",
                pos: (10.0, 32.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            // Vie des berges et des champs (non solide sauf la barque).
            DemoDecor {
                name: "Feu de camp",
                file: "nature_campfire.glb",
                pos: (19.0, -6.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Épouvantail",
                file: "nature_scarecrow.glb",
                pos: (-8.0, 26.5),
                scale: 1.0,
                yaw_deg: 20.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Barque",
                file: "nature_boat.glb",
                pos: (-24.0, 11.8),
                scale: 1.0,
                yaw_deg: 20.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Panneau 1",
                file: "nature_signpost.glb",
                pos: (-12.8, 15.8),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Panneau 2",
                file: "nature_signpost.glb",
                pos: (11.6, 15.8),
                scale: 1.0,
                yaw_deg: 180.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Nénuphars 1",
                file: "nature_lily.glb",
                pos: (-21.0, 3.5),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Nénuphars 2",
                file: "nature_lily.glb",
                pos: (-16.5, 6.5),
                scale: 1.2,
                yaw_deg: 90.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Nénuphars 3",
                file: "nature_lily.glb",
                pos: (-23.5, 7.5),
                scale: 0.9,
                yaw_deg: 200.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Roseaux 1",
                file: "nature_reeds.glb",
                pos: (-12.4, 9.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Roseaux 2",
                file: "nature_reeds.glb",
                pos: (-25.2, 1.5),
                scale: 1.1,
                yaw_deg: 70.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Roseaux 3",
                file: "nature_reeds.glb",
                pos: (-13.6, 25.0),
                scale: 1.0,
                yaw_deg: 140.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Roseaux 4",
                file: "nature_reeds.glb",
                pos: (-18.7, 30.5),
                scale: 1.05,
                yaw_deg: 250.0,
                solide: false,
                anim: None,
            },
            // --- Faune ambiante (gen_menagerie_pack.py / gen_menagerie_pack2.py) : non
            //     solide sauf mention contraire (mouton/cerf, socle plein → sondes),
            //     `anim: Some("Idle")` sur toutes. Dispersée près de son biome (forêt,
            //     prairie, berges, hameau) — la non-solidité ne dispense pas du
            //     dégagement des spawns pour les 2 solides, cf. la doc de `DemoDecor`.
            DemoDecor {
                name: "Mouton",
                file: "fauna_sheep.glb",
                pos: (-3.0, 16.0),
                scale: 1.0,
                yaw_deg: 300.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Cerf",
                file: "fauna_deer.glb",
                pos: (4.5, -13.5),
                scale: 1.0,
                yaw_deg: 30.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Oiseau",
                file: "fauna_bird.glb",
                pos: (9.0, -12.0),
                scale: 1.0,
                yaw_deg: 80.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Papillon",
                file: "fauna_butterfly.glb",
                pos: (-2.0, 5.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Poisson",
                file: "fauna_fish.glb",
                pos: (-22.0, 5.0),
                scale: 1.0,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Luciole",
                file: "fauna_firefly.glb",
                pos: (-4.0, -2.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Lapin",
                file: "fauna_rabbit.glb",
                pos: (2.0, -6.0),
                scale: 1.0,
                yaw_deg: 60.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Grenouille",
                file: "fauna_frog.glb",
                pos: (-19.0, 5.5),
                scale: 1.0,
                yaw_deg: 170.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Poule",
                file: "fauna_chicken.glb",
                pos: (8.5, 12.0),
                scale: 1.0,
                yaw_deg: 200.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Écureuil",
                file: "fauna_squirrel.glb",
                pos: (14.0, -12.0),
                scale: 1.0,
                yaw_deg: 110.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Chouette",
                file: "fauna_owl.glb",
                pos: (12.0, -16.0),
                scale: 1.0,
                yaw_deg: 40.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Chauve-souris",
                file: "fauna_bat.glb",
                pos: (18.0, -12.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Escargot",
                file: "fauna_snail.glb",
                pos: (-14.5, 8.0),
                scale: 1.0,
                yaw_deg: 260.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Crabe",
                file: "fauna_crab.glb",
                pos: (-25.5, 10.5),
                scale: 1.0,
                yaw_deg: 15.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Canard",
                file: "fauna_duck.glb",
                pos: (-19.5, 8.5),
                scale: 1.0,
                yaw_deg: 220.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Hérisson",
                file: "fauna_hedgehog.glb",
                pos: (16.0, -18.0),
                scale: 1.0,
                yaw_deg: 330.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Abeille",
                file: "fauna_bee.glb",
                pos: (-3.5, -1.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Taupe",
                file: "fauna_mole.glb",
                pos: (0.0, -8.5),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            // --- Mécanismes de décor animés, 2 vagues (gen_menagerie_pack.py /
            //     gen_menagerie_pack2.py) : 3 clusters excentrés pour ne pas surcharger
            //     le hameau existant — steppe nord-centrale (caravane), champs à
            //     l'ouest des rizières (vie paysanne), rive est au sud du promontoire
            //     (quartier portuaire). Carillon/chaise/cerf-volant : hameau, non
            //     solides (trop fins pour les sondes, cf. gen_menagerie_pack.py).
            DemoDecor {
                name: "Girouette",
                file: "nature_weathervane.glb",
                pos: (-6.0, -16.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Pont-levis",
                file: "nature_drawbridge.glb",
                pos: (2.0, -20.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Catapulte",
                file: "nature_catapult.glb",
                pos: (-6.0, -24.0),
                scale: 1.0,
                yaw_deg: 30.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Clocheton",
                file: "nature_bell_tower.glb",
                pos: (3.0, -27.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Barrière de péage",
                file: "nature_toll_gate.glb",
                pos: (-2.0, -30.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Puits à manivelle",
                file: "nature_well_windlass.glb",
                pos: (-7.0, -30.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Marteau-pilon",
                file: "nature_forge_hammer.glb",
                pos: (-30.0, 22.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Métier à tisser",
                file: "nature_weaving_loom.glb",
                pos: (-24.0, 22.0),
                scale: 1.0,
                yaw_deg: 200.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Rouet",
                file: "nature_spinning_wheel.glb",
                pos: (-30.0, 27.0),
                scale: 1.0,
                yaw_deg: 250.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Soufflet",
                file: "nature_bellows.glb",
                pos: (-24.0, 27.0),
                scale: 1.0,
                yaw_deg: 150.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Pompe à eau",
                file: "nature_water_pump.glb",
                pos: (-18.0, 30.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Auvent de marché",
                file: "nature_market_awning.glb",
                pos: (-30.0, 32.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Moulin à prières",
                file: "nature_prayer_wheel.glb",
                pos: (-18.0, 24.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Bascule à balancier",
                file: "nature_seesaw.glb",
                pos: (-24.0, 32.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Manège",
                file: "nature_merry_go_round.glb",
                pos: (-33.0, 27.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Grue de quai",
                file: "nature_dock_crane.glb",
                pos: (30.0, 22.0),
                scale: 1.0,
                yaw_deg: 200.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Phare",
                file: "nature_lighthouse_lamp.glb",
                pos: (33.0, 27.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Horloge de tour",
                file: "nature_pendulum_clock.glb",
                pos: (30.0, 32.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Balançoire à corde",
                file: "nature_rope_swing.glb",
                pos: (16.0, -24.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Carillon à vent",
                file: "nature_wind_chime.glb",
                pos: (8.0, 20.5),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Chaise à bascule",
                file: "nature_rocking_chair.glb",
                pos: (11.5, 20.5),
                scale: 1.0,
                yaw_deg: 200.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Cerf-volant",
                file: "nature_kite.glb",
                pos: (0.0, 3.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
        ];

        // Chargeur commun aux landmarks et au scatter : un même fichier n'est
        // chargé qu'une fois, les instances partagent l'entrée `imported`.
        let mut anim_count = 0usize;
        let mut poser = |name: &str,
                         file: &'static str,
                         x: f32,
                         z: f32,
                         scale: f32,
                         yaw_deg: f32,
                         solide: bool,
                         anim: Option<&'static str>| {
            let mesh_index = match imported.iter().position(|m| m.path.ends_with(file)) {
                Some(i) => i as u32,
                None => {
                    let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
                    match crate::scene::import::load_gltf(&path) {
                        Ok((data, aabb_min, aabb_max)) => {
                            let mut mesh = ImportedMesh {
                                path,
                                data,
                                aabb_min,
                                aabb_max,
                                ..Default::default()
                            };
                            // Statiques : ne peuple que les tangentes du rendu.
                            // Riggés (moulins…) : charge squelette et clips.
                            mesh.load_skinning();
                            imported.push(mesh);
                            (imported.len() - 1) as u32
                        }
                        Err(e) => {
                            log::error!("{name} MMORPG ({file}) : {e}");
                            return;
                        }
                    }
                }
            };
            let mut deco = demo_obj(name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
            deco.transform = deco.transform.with_scale(Vec3::splat(scale));
            if yaw_deg != 0.0 {
                deco.transform.rotation = glam::Quat::from_rotation_y(yaw_deg.to_radians());
            }
            if solide {
                deco.physics = PhysicsKind::Static;
                deco.collider_shape = crate::runtime::physics::ColliderShape::TriMesh;
            }
            if let Some(clip) = anim {
                anim_count += 1;
                deco.animation = Some(AnimationState {
                    clip: clip.into(),
                    // Phases de départ décalées : deux instances du même clip ne
                    // pulsent jamais à l'unisson (l'œil le repère tout de suite).
                    time: anim_count as f32 * 0.37,
                    ..Default::default()
                });
            }
            objects.push(deco);
        };
        for spec in NATURE_DECOR {
            poser(
                spec.name,
                spec.file,
                spec.pos.0,
                spec.pos.1,
                spec.scale,
                spec.yaw_deg,
                spec.solide,
                spec.anim,
            );
        }

        // 3) Scatter procédural seedé : peuple forêt, lisières, prairie et
        //    rizières sans énumérer 100 entrées à la main. Graine LITTÉRALE →
        //    même carte à chaque chargement (et donc testable : densités et
        //    dégagements sont des invariants, pas des coups de dés).
        //    Rejection sampling : un candidat est rejeté s'il tombe dans une
        //    zone aménagée (eau, routes, rizières, hameau, promontoire, col
        //    venté, clairières), à < 4 m d'un spawn de créature (marge sur les
        //    3,5 m des sondes) ou, pour un solide, à < 2,5 m d'un autre solide
        //    (pas de troncs fusionnés). Budget solides borné (~75 avec les
        //    landmarks) : chaque TriMesh pèse sur la broad-phase des raycasts
        //    des sondes — la densité visuelle vient du végétal traversable.
        type Rect = (f32, f32, f32, f32); // (x0, z0, x1, z1)
        const EXCL_EAU_ROUTES: &[Rect] = &[
            (-28.0, -36.0, -24.0, -6.0), // rivière nord
            (-28.0, -8.0, -16.0, -4.0),  // coude
            (-26.0, -2.0, -12.0, 10.0),  // lac
            (-18.0, 10.0, -14.0, 36.0),  // rivière sud
            (-26.0, 10.5, -12.0, 13.5),  // berge de sable
            (-36.0, 12.3, 36.0, 15.7),   // route principale
            (8.8, -10.0, 11.2, 14.0),    // chemin du hameau
            (-26.0, -10.9, 6.0, -9.1),   // chemin du pont nord
        ];
        const EXCL_ZONES_AMENAGEES: &[Rect] = &[
            (-11.0, 23.5, -5.0, 28.5), // rizière 1
            (-4.0, 23.5, 2.0, 28.5),   // rizière 2
            (-11.0, 29.5, -5.0, 34.5), // rizière 3
            (-4.0, 29.5, 2.0, 34.5),   // rizière 4
            (3.0, 26.5, 9.0, 31.5),    // rizière 5
            (2.0, 4.0, 20.0, 22.0),    // hameau
            (20.0, 2.0, 34.0, 16.0),   // promontoire
            (9.0, -9.0, 19.0, 1.0),    // col venté (zone de vent lisible)
        ];
        // Clairières de la forêt : rayon 6 autour des spawns des créatures 6
        // (chauve-souris) et 12 (félin) — leurs territoires restent dégagés.
        const EXCL_CLAIRIERES: &[Rect] = &[(14.0, -26.0, 26.0, -14.0), (18.0, -20.0, 30.0, -8.0)];

        let spawns: Vec<(f32, f32)> = MMORPG_CREATURES
            .iter()
            .map(|c| (c.spawn.x, c.spawn.z))
            .collect();
        let mut solid_spots: Vec<(f32, f32)> = NATURE_DECOR
            .iter()
            .filter(|d| d.solide)
            .map(|d| d.pos)
            .collect();
        let mut rng = crate::runtime::rng::Rng::new(0x4E41_5455_5245_3732); // « NATURE72 »
        type Poser<'a> =
            dyn FnMut(&str, &'static str, f32, f32, f32, f32, bool, Option<&'static str>) + 'a;
        #[allow(clippy::too_many_arguments)]
        fn scatter(
            rng: &mut crate::runtime::rng::Rng,
            poser: &mut Poser<'_>,
            solid_spots: &mut Vec<(f32, f32)>,
            spawns: &[(f32, f32)],
            exclusions: &[&[Rect]],
            files: &[&'static str],
            prefix: &str,
            rect: Rect,
            n: usize,
            scale: (f32, f32),
            solide: bool,
        ) {
            let mut poses = 0usize;
            let mut essais = 0usize;
            while poses < n && essais < n * 40 {
                essais += 1;
                let x = rng.next_range(rect.0, rect.2);
                let z = rng.next_range(rect.1, rect.3);
                if exclusions
                    .iter()
                    .flat_map(|g| g.iter())
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                {
                    continue;
                }
                let d2 = |(sx, sz): (f32, f32)| (sx - x) * (sx - x) + (sz - z) * (sz - z);
                if spawns.iter().any(|&s| d2(s) < 16.0) {
                    continue; // < 4 m d'un spawn de créature
                }
                if solide && solid_spots.iter().any(|&s| d2(s) < 6.25) {
                    continue; // < 2,5 m d'un autre solide
                }
                poses += 1;
                let file = files[rng.next_below(files.len())];
                let s = rng.next_range(scale.0, scale.1);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    &format!("{prefix} {poses}"),
                    file,
                    x,
                    z,
                    s,
                    yaw,
                    solide,
                    None,
                );
                if solide {
                    solid_spots.push((x, z));
                }
            }
        }

        // Forêt dense du nord-est (évite les deux clairières) : feuillus mêlés
        // aux sapins, souches, sous-bois traversable.
        let foret: Rect = (8.0, -34.0, 34.0, -8.0);
        let excl_foret: &[&[Rect]] = &[EXCL_EAU_ROUTES, EXCL_ZONES_AMENAGEES, EXCL_CLAIRIERES];
        let excl_std: &[&[Rect]] = &[EXCL_EAU_ROUTES, EXCL_ZONES_AMENAGEES];
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_tree.glb", "nature_tree2.glb"],
            "Arbre",
            foret,
            22,
            (0.9, 1.3),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_pine.glb", "nature_pine2.glb"],
            "Sapin",
            foret,
            12,
            (0.9, 1.25),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_stump.glb"],
            "Souche",
            foret,
            5,
            (0.9, 1.2),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_bush.glb"],
            "Buisson",
            foret,
            8,
            (0.9, 1.4),
            false,
        );
        // Lisières : quelques arbres épars le long du mur ouest (au-delà de la
        // rivière nord) et au sud-est (la lande du ver des sables reste ouverte).
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_tree.glb", "nature_pine.glb"],
            "Arbre de lisière",
            (-35.0, -34.0, -29.0, -12.0),
            5,
            (0.9, 1.2),
            true,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_tree2.glb", "nature_pine2.glb"],
            "Arbre du sud",
            (22.0, 30.0, 34.0, 35.0),
            5,
            (0.9, 1.2),
            true,
        );
        // Prairie centrale : fleurs et buissons traversables uniquement — les
        // cinq errants et le joueur y circulent sans obstacle.
        let prairie: Rect = (-10.0, -12.0, 8.0, 8.0);
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_flowers.glb"],
            "Fleurs",
            prairie,
            12,
            (0.9, 1.4),
            false,
        );
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_bush.glb"],
            "Buisson fleuri",
            prairie,
            6,
            (0.9, 1.3),
            false,
        );
        // Plants de riz dans chaque bassin (traversables) : le scatter vise
        // l'intérieur du bassin, seules les zones eau/routes le repoussent.
        let excl_riz: &[&[Rect]] = &[EXCL_EAU_ROUTES];
        for (i, &(x0, z0, x1, z1)) in EXCL_ZONES_AMENAGEES[..5].iter().enumerate() {
            scatter(
                &mut rng,
                &mut poser,
                &mut solid_spots,
                &spawns,
                excl_riz,
                &["nature_rice.glb"],
                &format!("Riz {}", ["A", "B", "C", "D", "E"][i]),
                (x0 + 0.7, z0 + 0.7, x1 - 0.7, z1 - 0.7),
                3,
                (0.9, 1.3),
                false,
            );
        }

        // Objets d'inventaire (cf. `ItemPickup`) à trouver en explorant, posés
        // sur la narration de chaque biome : potions devant la Cabane 1 et aux
        // sorties des ponts, baies au bord des rizières, clé au pied de la tour
        // de guet, gemmes au promontoire et au fond de la forêt. Non solides
        // (`demo_obj` = `PhysicsKind::None`) : invisibles aux sondes raycast
        // des créatures, donc sans effet sur les patrouilles.
        struct DemoItem {
            name: &'static str,
            kind: ItemKind,
            count: u32,
            pos: (f32, f32, f32),
            /// > 0 ⇒ l'objet repousse (buisson à baies) ; 0 ⇒ trouvaille unique.
            respawn: f32,
        }
        const MMORPG_ITEMS: &[DemoItem] = &[
            // Devant la porte de la Cabane 1 (6, 9 — porte côté route).
            DemoItem {
                name: "Potion de soin",
                kind: ItemKind::Potion,
                count: 1,
                pos: (6.0, 0.35, 11.2),
                respawn: 0.0,
            },
            // Sortie est du Pont 1 (-16, 14).
            DemoItem {
                name: "Potion de soin 2",
                kind: ItemKind::Potion,
                count: 1,
                pos: (-12.6, 0.35, 14.0),
                respawn: 0.0,
            },
            // Sortie est du Pont 2 (-26, -10), sur le chemin de la forêt.
            DemoItem {
                name: "Potion de soin 3",
                kind: ItemKind::Potion,
                count: 1,
                pos: (-23.0, 0.35, -10.0),
                respawn: 0.0,
            },
            // Bord nord des rizières : 2 baies par cueillette, repousse en 20 s.
            DemoItem {
                name: "Buisson à baies",
                kind: ItemKind::Baie,
                count: 2,
                pos: (-4.0, 0.3, 22.5),
                respawn: 20.0,
            },
            // Au pied de la tour de guet (27, 9).
            DemoItem {
                name: "Clé du village",
                kind: ItemKind::Cle,
                count: 1,
                pos: (25.9, 0.3, 7.6),
                respawn: 0.0,
            },
            // Dans l'anneau de rochers du promontoire.
            DemoItem {
                name: "Gemme",
                kind: ItemKind::Gemme,
                count: 1,
                pos: (30.9, 0.3, 12.6),
                respawn: 0.0,
            },
            // Au fond de la forêt dense : la récompense de l'exploration.
            DemoItem {
                name: "Gemme de la forêt",
                kind: ItemKind::Gemme,
                count: 1,
                pos: (30.0, 0.3, -30.0),
                respawn: 0.0,
            },
        ];
        for spec in MMORPG_ITEMS {
            let mesh = if spec.kind == ItemKind::Potion {
                MeshKind::Capsule
            } else {
                MeshKind::Sphere
            };
            let mut item = demo_obj(
                spec.name,
                mesh,
                Vec3::new(spec.pos.0, spec.pos.1, spec.pos.2),
            );
            item.transform = item.transform.with_scale(Vec3::splat(0.35));
            item.color = spec.kind.color();
            item.emissive = 0.8;
            item.respawn_delay = spec.respawn;
            item.item_pickup = Some(ItemPickup {
                kind: spec.kind,
                count: spec.count,
            });
            objects.push(item);
        }

        Scene {
            objects,
            imported,
            camera_follow: true,
            point_lights: vec![
                // Éclairage général : hissé et élargi avec la carte 72×72 (l'ancien
                // range de 30 m laissait les biomes périphériques dans le noir).
                PointLight {
                    position: [0.0, 18.0, 0.0],
                    color: [0.9, 0.95, 1.0],
                    intensity: 1.4,
                    range: 90.0,
                    ..PointLight::default()
                },
                // Deux lampes chaudes au hameau (cf. les lanternes du décor) : la
                // zone habitée se repère de loin, même à contre-jour du soleil.
                PointLight {
                    position: [10.0, 3.0, 12.0],
                    color: [1.0, 0.75, 0.4],
                    intensity: 1.1,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [2.0, 3.0, 12.0],
                    color: [1.0, 0.75, 0.4],
                    intensity: 1.1,
                    range: 12.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            // Soleil orienté sud-est : les ombres portées de la forêt, de la
            // tour et du hameau donnent le relief qu'une carte plate n'a pas.
            light: Light {
                dir: [0.55, 1.0, -0.45],
                color: [1.0, 0.96, 0.88],
                ambient: 0.35,
            },
            // Ciel de journée chaude (cohérent avec la palette du pack nature,
            // cf. gen_nature_pack.py) + brume légère : donne la profondeur
            // atmosphérique sur 72 m, adoucit les murs d'enceinte au loin, et
            // masque le pop de détail — à coût GPU nul (déjà dans le shader).
            sky: Sky {
                horizon_color: [0.85, 0.78, 0.62],
                zenith_color: [0.30, 0.52, 0.78],
                fog_color: [0.78, 0.74, 0.62],
                fog_density: 0.012,
                ..Sky::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Donjon » façon roguelike : 3 salles reliées par des portes (une salle à la
    /// fois, comme un couloir de progression), chacune gardée par un monstre — réutilise
    /// le système de manches (`Combat::wave`) de `zombies_demo` : un monstre par manche,
    /// la salle suivante ne se révèle (et n'obtient de corps physique, cf.
    /// `Physics::build`) qu'une fois la précédente vidée. Particularité roguelike : à
    /// chaque chargement, 3 armes **distinctes** sont tirées au sort parmi les 5 profils
    /// connus (cf. `WEAPONS`) — une équipée au départ, les 2 autres cachées en butin
    /// dans les salles 1 et 2 (cf. `WeaponPickup`) à trouver en explorant avant d'arriver
    /// à la salle 3 (l'Ogre). Score +1 par monstre vaincu *et* par arme trouvée (cf.
    /// `AppState::advance_play`) : un vrai objectif d'exploration, pas juste un combat.
    pub fn roguelike_demo() -> Self {
        // Salles carrées de 9 m de côté, alignées le long de +Z, séparées par une porte
        // (mur avec une ouverture centrale de 3 m) plutôt que par un couloir séparé —
        // plus compact qu'un vrai couloir, mais tout aussi lisible comme 3 pièces
        // distinctes (ligne de vue coupée hors de l'ouverture).
        let half_x = 4.5_f32;
        let room_depth = 9.0_f32;
        let room_z = [-room_depth, 0.0, room_depth]; // centres des 3 salles
        let total_half_z = 1.5 * room_depth;

        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half_x, 1.0, 2.0 * total_half_z));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.18, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, room_z[0]));
        joueur.color = [0.95, 0.6, 0.25];

        // --- Tirage de 3 armes DISTINCTES parmi les 5 profils connus (`WEAPONS`) : une
        // pour l'équipement de départ, les 2 autres cachées en butin plus bas (cf.
        // `WeaponPickup`). Mélange de Fisher-Yates via `runtime::rng::Rng` (Sprint 131,
        // unifie ce qui était une copie locale du même xorshift64 maison que
        // `runtime::sfx`) : l'horloge système sert de graine.
        let mut rng = crate::runtime::rng::Rng::from_system_time();
        let mut order: [usize; WEAPONS.len()] = std::array::from_fn(|i| i);
        rng.shuffle(&mut order);
        let (starting_idx, found_idx) = (order[0], [order[1], order[2]]);
        let weapon = WEAPONS[starting_idx];
        log::info!(
            "Donjon : arme de départ « {} » (portée {:.1} m, recharge {:.2} s, préparation {:.2} s) — à trouver : {}, {}",
            weapon.label,
            weapon.range,
            weapon.cooldown,
            weapon.windup,
            WEAPONS[found_idx[0]].label,
            WEAPONS[found_idx[1]].label,
        );
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.0,
            attack_button: "Attaque".into(),
            attack_range: weapon.range,
            attack_cooldown: weapon.cooldown,
            attack_windup: weapon.windup,
            attack_mode: weapon.mode,
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];

        // Murs de pourtour de tout le donjon (une seule enveloppe extérieure).
        let wall = |name: &str, pos: Vec3, scale: Vec3, objects: &mut Vec<SceneObject>| {
            let mut w = demo_obj(name, MeshKind::Cube, pos);
            w.transform = w.transform.with_scale(scale);
            w.physics = PhysicsKind::Static;
            w.color = [0.32, 0.28, 0.35];
            objects.push(w);
        };
        wall(
            "Mur Nord",
            Vec3::new(0.0, 0.9, -total_half_z - 0.25),
            Vec3::new(2.0 * half_x + 0.5, 1.8, 0.5),
            &mut objects,
        );
        wall(
            "Mur Sud",
            Vec3::new(0.0, 0.9, total_half_z + 0.25),
            Vec3::new(2.0 * half_x + 0.5, 1.8, 0.5),
            &mut objects,
        );
        wall(
            "Mur Est",
            Vec3::new(half_x + 0.25, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * total_half_z + 0.5),
            &mut objects,
        );
        wall(
            "Mur Ouest",
            Vec3::new(-half_x - 0.25, 0.9, 0.0),
            Vec3::new(0.5, 1.8, 2.0 * total_half_z + 0.5),
            &mut objects,
        );

        // Portes entre les salles : mur transversal avec une ouverture centrale de 3 m
        // (deux segments latéraux), à mi-chemin entre chaque paire de salles.
        let door_gap_half = 1.5_f32;
        for (n, z) in [room_z[0], room_z[1]]
            .iter()
            .map(|z| z + room_depth * 0.5)
            .enumerate()
        {
            wall(
                &format!("Porte {} (gauche)", n + 1),
                Vec3::new(-(half_x + door_gap_half) * 0.5, 0.9, z),
                Vec3::new(half_x - door_gap_half, 1.8, 0.4),
                &mut objects,
            );
            wall(
                &format!("Porte {} (droite)", n + 1),
                Vec3::new((half_x + door_gap_half) * 0.5, 0.9, z),
                Vec3::new(half_x - door_gap_half, 1.8, 0.4),
                &mut objects,
            );
        }

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.95, 0.75];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;
        objects.push(fx);

        // --- Un monstre par salle (une manche chacun, cf. `Combat::wave`) : la salle 2
        // ne se révèle qu'une fois le monstre de la salle 1 vaincu, etc. — progression
        // « une salle à la fois » typique d'un roguelike, sans script de porte à part.
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
        }
        const GOBELIN: Kind = Kind {
            label: "Gobelin",
            speed: 3.2,
            dmg: 0.6,
            scale: 0.6,
            color: [0.35, 0.6, 0.3],
        };
        const SQUELETTE: Kind = Kind {
            label: "Squelette",
            speed: 2.4,
            dmg: 1.0,
            scale: 0.85,
            color: [0.75, 0.72, 0.65],
        };
        const OGRE: Kind = Kind {
            label: "Ogre",
            speed: 1.6,
            dmg: 2.4,
            scale: 1.4,
            color: [0.4, 0.15, 0.15],
        };
        // Décalage du monstre par rapport au centre de sa salle : loin du point d'entrée
        // du joueur — son spawn pour la salle 1 (sinon le Gobelin apparaissait pile sur
        // le joueur et mordait avant même qu'il ait pu bouger), la porte d'entrée pour
        // les salles 2 et 3 (sinon le monstre suivant mord dès le franchissement de la
        // porte, sans le moindre temps de réaction).
        const MONSTER_Z_OFFSET: [f32; 3] = [-3.0, 3.0, 3.0];
        for (wave, ((k, z), z_offset)) in [GOBELIN, SQUELETTE, OGRE]
            .into_iter()
            .zip(room_z)
            .zip(MONSTER_Z_OFFSET)
            .enumerate()
        {
            let wave = wave as u32 + 1;
            let mut m = demo_obj(
                k.label,
                MeshKind::Sphere,
                Vec3::new(0.0, k.scale.max(0.5) * 0.5, z + z_offset),
            );
            m.transform = m.transform.with_scale(Vec3::splat(k.scale));
            m.color = k.color;
            m.emissive = 0.5;
            m.trigger = true;
            m.ai_chaser = Some(AiChaser { speed: k.speed });
            m.combat = Some(Combat {
                attackable: true,
                wave,
                ..Default::default()
            });
            m.respawn_delay = 0.0;
            m.script = format!(
                "if obj.triggered then damage({} * dt) end\n\
                 local p = 0.5 + 0.5 * math.sin(time * 7.0)\n\
                 obj.r = {} + {} * p; obj.g = {}; obj.b = {}",
                k.dmg,
                k.color[0] * 0.7,
                k.color[0] * 0.3,
                k.color[1] * 0.6,
                k.color[2] * 0.6,
            );
            objects.push(m);
        }

        // Butins d'arme (cf. `WeaponPickup`) : un dans la salle 1, un dans la salle 2 —
        // la salle 3 (l'Ogre, le combat le plus dur) doit pouvoir être abordée avec la
        // meilleure arme déjà trouvée en explorant, pas en découvrir une nouvelle en
        // pleine bagarre. Coin de salle opposé au monstre (au centre), pour ne pas
        // forcer le joueur à passer devant le monstre juste pour le voir.
        for (n, &weapon_idx) in found_idx.iter().enumerate() {
            let w = WEAPONS[weapon_idx];
            let side = if n == 0 { 1.0 } else { -1.0 };
            let mut loot = demo_obj(
                &format!("Butin: {}", w.label),
                MeshKind::Cube,
                Vec3::new(side * 3.0, 0.4, room_z[n] + side * 3.0),
            );
            loot.transform = loot.transform.with_scale(Vec3::splat(0.4));
            loot.color = [1.0, 0.85, 0.2];
            loot.emissive = 1.2;
            loot.weapon_pickup = Some(WeaponPickup { weapon: weapon_idx });
            objects.push(loot);
        }

        // Objets d'inventaire (cf. `ItemPickup`) : de quoi remplir le sac en
        // explorant — une potion par salle 1/2 (soin de secours avant l'Ogre,
        // coin opposé au butin d'arme pour inciter à fouiller toute la salle),
        // un buisson à baies qui se régénère dans le couloir, et le trésor de
        // l'Ogre (clé + gemme) au fond de la salle 3.
        for (n, &z) in room_z.iter().take(2).enumerate() {
            let side = if n == 0 { -1.0 } else { 1.0 };
            let mut potion = demo_obj(
                "Potion de soin",
                MeshKind::Capsule,
                Vec3::new(side * 3.0, 0.35, z - side * 3.0),
            );
            potion.transform = potion.transform.with_scale(Vec3::splat(0.35));
            potion.color = ItemKind::Potion.color();
            potion.emissive = 0.8;
            potion.item_pickup = Some(ItemPickup {
                kind: ItemKind::Potion,
                count: 1,
            });
            objects.push(potion);
        }
        let mut baies = demo_obj(
            "Buisson à baies",
            MeshKind::Sphere,
            Vec3::new(1.5, 0.3, (room_z[0] + room_z[1]) * 0.5),
        );
        baies.transform = baies.transform.with_scale(Vec3::splat(0.5));
        baies.color = ItemKind::Baie.color();
        baies.emissive = 0.4;
        baies.respawn_delay = 20.0;
        baies.item_pickup = Some(ItemPickup {
            kind: ItemKind::Baie,
            count: 2,
        });
        objects.push(baies);
        for (name, kind, dx) in [
            ("Clé du donjon", ItemKind::Cle, -1.0),
            ("Gemme de l'Ogre", ItemKind::Gemme, 1.0),
        ] {
            let mut tresor = demo_obj(name, MeshKind::Cube, Vec3::new(dx, 0.3, room_z[2] - 3.5));
            tresor.transform = tresor.transform.with_scale(Vec3::splat(0.3));
            tresor.color = kind.color();
            tresor.emissive = 1.0;
            tresor.item_pickup = Some(ItemPickup { kind, count: 1 });
            objects.push(tresor);
        }

        Scene {
            objects,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 6.0, room_z[0]],
                    color: [0.4, 0.8, 0.5],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 6.0, room_z[1]],
                    color: [0.9, 0.85, 0.7],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 6.0, room_z[2]],
                    color: [0.9, 0.3, 0.3],
                    intensity: 1.0,
                    range: 12.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « Duel » façon Tekken/Smash Bros : arène compacte flottant au-dessus du
    /// vide, joueur contre un unique rival qui encaisse plusieurs coups (cf.
    /// `Combat::hp`) avant de tomber — un vrai combat, pas une mise à mort au premier
    /// coup. Deux façons de gagner, comme dans un vrai jeu de combat : l'achever à coups
    /// de poing (hp à 0, cf. `Scene::damage_attackable`), ou le faire sortir de l'arène
    /// d'un coup de recul (« ring out », cf. `AppState::stagger` — le vide sous la scène
    /// est une zone mortelle, cf. `deadly`, réutilisée pour l'IA comme pour le joueur).
    /// Réutilise le système de manches (`Combat::wave = 1`, un seul adversaire) plutôt
    /// qu'un mécanisme de victoire dédié : dès que le rival est invisible (achevé ou
    /// sorti de l'arène), `AppState::update_waves` déclenche la victoire tout seul.
    pub fn brawl_demo() -> Self {
        let half = 7.0_f32;

        let mut sol = demo_obj("Arène", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.18, 0.16, 0.22];
        sol.metallic = 0.5;
        sol.roughness = 0.3;

        // Le vide sous l'arène : aucun mur, aucun sol au-delà du bord — tomber suffit à
        // perdre (joueur) ou à être vaincu (rival, cf. la vérification de ring out dans
        // `AppState::advance_play`). Invisible : la chute elle-même (rien sous les
        // pieds) suffit à faire comprendre le danger, pas besoin d'un aplat coloré.
        let mut vide = demo_obj("Vide", MeshKind::Cube, Vec3::new(0.0, -8.0, 0.0));
        vide.transform = vide.transform.with_scale(Vec3::new(60.0, 10.0, 60.0));
        vide.deadly = true;
        vide.visible = false;

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(-4.0, 1.0, 0.0));
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            // Portée courte et préparation vive : des coups qui se rapprochent d'un jab
            // de jeu de combat, pas d'un missile à distance.
            attack_range: 1.3,
            attack_cooldown: 0.45,
            attack_windup: 0.15,
            ..Default::default()
        });

        // Ancre de l'effet visuel d'attaque (cf. `Combat::is_attack_fx`).
        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.9, 0.6];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        let mut rival = demo_obj("Rival", MeshKind::Capsule, Vec3::new(4.0, 1.0, 0.0));
        rival.transform = rival.transform.with_scale(Vec3::splat(1.05));
        rival.color = [0.55, 0.08, 0.12];
        rival.emissive = 0.35;
        rival.trigger = true;
        rival.ai_chaser = Some(AiChaser { speed: 2.8 });
        rival.combat = Some(Combat {
            attackable: true,
            // Une seule « manche » (cf. `Combat::wave`) : un adversaire unique, pas des
            // vagues — juste pour déclencher la victoire via `AppState::update_waves`
            // une fois qu'il est invisible (achevé ou sorti de l'arène), sans avoir à
            // écrire une condition de victoire dédiée à cette démo.
            wave: 1,
            // 3 coups pour l'achever : un vrai duel, pas une mise à mort au premier
            // coup (`Combat::hp` par défaut ailleurs). Reste vainquable par ring out
            // avant d'y arriver (cf. la vérification dans `AppState::advance_play`).
            hp: 3,
            ..Default::default()
        });
        rival.respawn_delay = 0.0;
        rival.script = "if obj.triggered then damage(0.9 * dt) end\n\
             local p = 0.5 + 0.5 * math.sin(time * 6.0)\n\
             obj.r = 0.55 + 0.35 * p; obj.g = 0.08; obj.b = 0.12"
            .into();

        Scene {
            objects: vec![sol, vide, joueur, fx, rival],
            camera_follow: true,
            // Angle plus bas et plus horizontal que les autres démos (pitch ~0,35 contre
            // ~0,62) : cadrage de profil façon jeu de combat plutôt qu'une vue plongeante
            // de action-aventure — le point de vue précis se règle facilement dans
            // l'éditeur (`Vue → Définir la caméra de jeu`) si besoin d'un angle différent.
            game_camera: Some(GameCamera {
                target: [0.0, 1.0, 0.0],
                yaw: 0.0,
                pitch: 0.35,
                distance: 9.0,
            }),
            point_lights: vec![
                // Lumière chaude du côté du joueur, froide du côté du rival — cadrage
                // « vs » à deux couleurs typique des jeux de combat.
                PointLight {
                    position: [-4.0, 4.0, 2.0],
                    color: [1.0, 0.65, 0.3],
                    intensity: 1.1,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [4.0, 4.0, -2.0],
                    color: [0.3, 0.55, 1.0],
                    intensity: 1.1,
                    range: 14.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Attaque".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Démo « gameplay complet » : joueur (joystick + gyroscope + saut + vibration),
    /// zone de danger qui retire de la vie (HUD), et cube tactile qui change de couleur.
    /// Montre toute l'API de script en une scène jouable.
    pub fn gameplay_demo() -> Self {
        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(16.0, 1.0, 16.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.35, 0.5, 0.4];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 0.5, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.script = "\
local s = 4.0
obj.x = obj.x + (input.jx + tilt.x) * s * dt
obj.z = obj.z - (input.jy + tilt.y) * s * dt
if input.btn.Saut then obj.y = 1.4; vibrate(40) else obj.y = 0.5 end"
            .into();

        let mut danger = demo_obj("Zone danger", MeshKind::Cube, Vec3::new(3.0, 0.5, 0.0));
        danger.color = [0.8, 0.2, 0.2];
        danger.emissive = 0.3;
        danger.trigger = true;
        danger.script = "\
if obj.triggered then set_health(0.25); vibrate(120) else set_health(1.0) end"
            .into();

        let mut bouton = demo_obj("Cube couleur", MeshKind::Cube, Vec3::new(-3.0, 0.5, 0.0));
        bouton.color = [0.3, 0.6, 0.9];
        bouton.tappable = true;
        bouton.script = "\
if obj.tapped then
  obj.r = (time * 0.7) % 1.0; obj.g = (time * 1.3) % 1.0; obj.b = (time * 1.9) % 1.0
end"
        .into();

        Scene {
            objects: vec![sol, joueur, danger, bouton],
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
        }
    }
    /// Scène **embarquée dans le binaire** (figée à la compilation depuis
    /// `assets/player_scene.json`, réécrite à chaque export). C'est le jeu que joue
    /// le mode Player d'un `.dmg`/`.apk`/`.ipa` exporté.
    pub fn embedded_player() -> Self {
        const JSON: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/player_scene.json"
        ));
        match serde_json::from_str::<Scene>(JSON) {
            Ok(mut s) => {
                s.reload_imported();
                s
            }
            Err(e) => {
                log::error!("Scène embarquée invalide ({e}) — retour à la démo.");
                Scene::demo()
            }
        }
    }

    /// Scène de démonstration : un sol, un cube, une sphère.
    pub fn demo() -> Self {
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls::default(),
            camera_follow: false,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, -1.0, 0.0))
                        .with_scale(Vec3::new(10.0, 1.0, 10.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Cube".into(),
                    transform: Transform::from_pos(Vec3::new(-1.2, -0.5, 0.0)),
                    mesh: MeshKind::Cube,
                    // exemple : tourne autour de Y à 60°/s en mode Play
                    script: "obj.ry = obj.ry + dt * 60.0".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Sphère".into(),
                    transform: Transform::from_pos(Vec3::new(1.2, 2.5, 0.0)),
                    mesh: MeshKind::Sphere,
                    script: String::new(),
                    // tombe et rebondit sur le sol en mode Play
                    physics: PhysicsKind::Dynamic,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [1.0, 1.0, 1.0],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
            ],
        }
    }

    /// Démo mobile « prête à jouer » : un sol, un personnage piloté au joystick
    /// (avec saut au bouton) et contrôles tactiles activés. Démontre toute la
    /// boucle joystick → script → rendu en mode Play.
    pub fn mobile_demo() -> Self {
        let player_script = "\
local speed = 4.0
obj.x = obj.x + input.jx * speed * dt
obj.z = obj.z - input.jy * speed * dt
if input.btn.Saut then obj.y = 1.4 else obj.y = 0.5 end";
        Scene {
            imported: Vec::new(),
            groups: Vec::new(),
            light: Light::default(),
            point_lights: Vec::new(),
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into()],
                ..Default::default()
            },
            camera_follow: true,
            game_camera: None,
            sky: Sky::default(),
            hud_layout: HudLayout::default(),
            hud_widgets: Vec::new(),
            version: Scene::CURRENT_VERSION,
            objects: vec![
                SceneObject {
                    name: "Sol".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.0, 0.0))
                        .with_scale(Vec3::new(14.0, 1.0, 14.0)),
                    mesh: MeshKind::Plane,
                    script: String::new(),
                    physics: PhysicsKind::Static,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.4, 0.5, 0.45],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Joueur".into(),
                    transform: Transform::from_pos(Vec3::new(0.0, 0.5, 0.0)),
                    mesh: MeshKind::Capsule,
                    script: player_script.into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.95, 0.6, 0.25],
                    texture: String::new(),
                    tappable: false,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
                SceneObject {
                    name: "Bouton couleur".into(),
                    transform: Transform::from_pos(Vec3::new(2.5, 0.5, -1.0)),
                    mesh: MeshKind::Cube,
                    // Tap → couleur aléatoire (changeante) via le temps.
                    script: "if obj.tapped then\n  obj.r = (time * 0.7) % 1.0\n  obj.g = (time * 1.3) % 1.0\n  obj.b = (time * 1.9) % 1.0\nend".into(),
                    physics: PhysicsKind::None,
                    collider_shape: crate::runtime::physics::ColliderShape::Auto,
                    group: String::new(),
                    color: [0.3, 0.6, 0.9],
                    texture: String::new(),
                    tappable: true,
                    metallic: 0.0,
                    roughness: 0.6,
                    emissive: 0.0,
                    trigger: false,
                    ..Default::default()
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preuve du décor nature de la démo MMORPG (Sprint en cours) : les glb du
    /// pack (`scripts/blender/gen_nature_pack.py`) se chargent réellement (un
    /// objet n'est poussé que si `load_gltf` réussit), le décor solide a bien un
    /// collider `TriMesh` statique, le végétal léger reste traversable, et les
    /// instances d'un même fichier partagent leur entrée `imported`.
    #[test]
    fn mmorpg_demo_contains_walkable_nature_decor() {
        let scene = Scene::mmorpg_demo();
        let by_name = |name: &str| {
            scene
                .objects
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {name} »"))
        };

        // Aplats de terrain : purement visuels, jamais de collider.
        for name in [
            "Rivière nord",
            "Lac",
            "Rivière sud",
            "Route principale",
            "Chemin du hameau",
            "Rizière 1",
        ] {
            assert_eq!(
                by_name(name).physics,
                PhysicsKind::None,
                "« {name} » est un aplat visuel, il ne doit rien bloquer"
            );
        }

        // Décor solide : statique + TriMesh (les ponts se traversent à pied sur
        // leur tablier, silhouette exacte pour cabanes/tour/rochers/moulins).
        for name in [
            "Pont 1",
            "Pont 2",
            "Cabane 1",
            "Hutte",
            "Tour de guet",
            "Rocher 1",
            "Puits",
            "Torii",
            "Moulin à eau",
            "Moulin à vent",
        ] {
            let obj = by_name(name);
            assert_eq!(obj.physics, PhysicsKind::Static, "« {name} » doit bloquer");
            assert_eq!(
                obj.collider_shape,
                crate::runtime::physics::ColliderShape::TriMesh,
                "« {name} » doit utiliser sa silhouette exacte, pas une boîte"
            );
        }

        // Végétal léger et décor de berge : traversable.
        for name in [
            "Fleurs 1",
            "Panneau 1",
            "Nénuphars 1",
            "Roseaux 1",
            "Épouvantail",
            "Feu de camp",
        ] {
            assert_eq!(
                by_name(name).physics,
                PhysicsKind::None,
                "« {name} » ne doit pas gêner les déplacements"
            );
        }

        // Instanciation : deux instances semées d'un même fichier partagent le
        // même mesh importé (aucun état par objet sur du décor statique,
        // inutile de recharger le fichier).
        let mesh_of = |name: &str| match by_name(name).mesh {
            MeshKind::Imported(i) => i,
            _ => panic!("« {name} » devrait être un mesh importé"),
        };
        assert_eq!(
            mesh_of("Pont 1"),
            mesh_of("Pont 2"),
            "les instances d'un même glb doivent partager leur entrée `imported`"
        );

        // Dégagement des spawns : les créatures démarrent avec RAY_DIST (3,5 m)
        // de sonde devant elles — aucun décor solide à moins de 3,5 m d'un spawn
        // (même règle que les repères, cf. le commentaire de MMORPG_CREATURES).
        let creatures: Vec<&SceneObject> = scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
            .collect();
        assert!(!creatures.is_empty(), "la démo doit garder ses créatures");
        for deco in scene
            .objects
            .iter()
            .filter(|o| o.physics == PhysicsKind::Static && matches!(o.mesh, MeshKind::Imported(_)))
        {
            for creature in &creatures {
                let delta = deco.transform.position - creature.transform.position;
                let d = (delta.x * delta.x + delta.z * delta.z).sqrt();
                assert!(
                    d >= 3.5,
                    "« {} » ({:?}) est à {d:.2} m du spawn de « {} » — il faut \
                     ≥ 3,5 m (RAY_DIST) de dégagement",
                    deco.name,
                    deco.transform.position,
                    creature.name
                );
            }
        }
    }

    /// Preuve de l'agrandissement ×3 (24×24 → 72×72) : le sol et les 4 murs
    /// suivent `Scene::MMORPG_HALF`, les deux ponts (aplats eau sans collider,
    /// glb solides) existent bel et bien.
    #[test]
    fn mmorpg_map_is_72m_with_biomes() {
        let scene = Scene::mmorpg_demo();
        let half = Scene::MMORPG_HALF;
        assert_eq!(half, 36.0, "carte 72×72 : ×3 l'arène d'origine de 24×24");

        let sol = scene
            .objects
            .iter()
            .find(|o| o.name == "Sol")
            .expect("la démo doit avoir un sol");
        assert!(
            (sol.transform.scale.x - 2.0 * half).abs() < 0.01
                && (sol.transform.scale.z - 2.0 * half).abs() < 0.01,
            "le sol doit couvrir 72×72 m (scale={:?})",
            sol.transform.scale
        );

        for name in ["Mur Nord", "Mur Sud", "Mur Est", "Mur Ouest"] {
            let mur = scene
                .objects
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("« {name} » doit exister"));
            let d = mur
                .transform
                .position
                .x
                .abs()
                .max(mur.transform.position.z.abs());
            assert!(
                (d - half).abs() < 0.01,
                "« {name} » doit border l'arène à ±{half} m (pos={:?})",
                mur.transform.position
            );
        }

        for (bridge, aplat) in [("Pont 1", "Rivière sud"), ("Pont 2", "Rivière nord")] {
            let obj = scene
                .objects
                .iter()
                .find(|o| o.name == bridge)
                .unwrap_or_else(|| panic!("« {bridge} » doit exister"));
            assert_eq!(
                obj.physics,
                PhysicsKind::Static,
                "« {bridge} » doit bloquer"
            );
            assert_eq!(
                obj.collider_shape,
                crate::runtime::physics::ColliderShape::TriMesh,
                "« {bridge} » doit avoir sa silhouette exacte"
            );
            assert_eq!(
                scene
                    .objects
                    .iter()
                    .find(|o| o.name == aplat)
                    .unwrap_or_else(|| panic!("« {aplat} » doit exister"))
                    .physics,
                PhysicsKind::None,
                "« {aplat} » (eau) ne doit rien bloquer, seuls les ponts le franchissent en dur"
            );
        }
    }

    /// Preuve que le scatter procédural (graine fixe) peuple vraiment la forêt
    /// du nord-est et le hameau/promontoire, plutôt que de tout rejeter en
    /// silence (rejection sampling contre les zones d'exclusion).
    #[test]
    fn mmorpg_forest_reaches_minimum_density() {
        let scene = Scene::mmorpg_demo();
        let in_forest = |o: &&SceneObject| {
            let p = o.transform.position;
            p.x >= 8.0 && p.x <= 34.0 && p.z >= -34.0 && p.z <= -8.0
        };
        let trees_and_pines = scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Arbre") || o.name.starts_with("Sapin"))
            .filter(in_forest)
            .count();
        assert!(
            trees_and_pines >= 30,
            "la forêt NE doit compter ≥ 30 arbres/sapins (trouvé {trees_and_pines})"
        );

        let batiments = ["Cabane 1", "Cabane 2", "Hutte"]
            .iter()
            .filter(|n| scene.objects.iter().any(|o| o.name == **n))
            .count();
        assert!(batiments >= 3, "le hameau doit avoir ≥ 3 bâtiments");

        let rochers = scene
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Rocher"))
            .count();
        assert!(
            rochers >= 5,
            "le promontoire doit avoir ≥ 5 rochers (trouvé {rochers})"
        );
    }

    /// Preuve anti-chevauchement : tout décor solide reste dans l'arène et à
    /// ≥ 2 m de tout autre solide (le scatter procédural pourrait sinon
    /// fusionner deux troncs visuellement).
    #[test]
    fn mmorpg_solid_decor_stays_inside_and_spaced() {
        let scene = Scene::mmorpg_demo();
        let half = Scene::MMORPG_HALF;
        let solids: Vec<&SceneObject> = scene
            .objects
            .iter()
            .filter(|o| o.physics == PhysicsKind::Static && matches!(o.mesh, MeshKind::Imported(_)))
            .collect();
        assert!(solids.len() >= 40, "le décor solide doit être substantiel");
        for s in &solids {
            let p = s.transform.position;
            assert!(
                p.x.abs() <= half && p.z.abs() <= half,
                "« {} » ({p:?}) doit rester dans l'arène",
                s.name
            );
        }
        for i in 0..solids.len() {
            for j in (i + 1)..solids.len() {
                let d = (solids[i].transform.position - solids[j].transform.position)
                    .length_squared()
                    .sqrt();
                assert!(
                    d >= 2.0,
                    "« {} » et « {} » sont à {d:.2} m l'un de l'autre (< 2 m, risque de fusion visuelle)",
                    solids[i].name,
                    solids[j].name
                );
            }
        }
    }

    /// Preuve du placement logique des 20 créatures : chaque spawn tombe dans
    /// le rectangle de son biome annoncé (forêt, lac, rizières, hameau,
    /// promontoire…), pas éparpillé au hasard sur la carte.
    #[test]
    fn mmorpg_creature_spawns_sit_in_their_biome() {
        let scene = Scene::mmorpg_demo();
        // (x0, z0, x1, z1), même convention que `Rect` dans le scatter procédural.
        #[allow(clippy::type_complexity)]
        let biomes: &[(&str, (f32, f32, f32, f32))] = &[
            ("Créature 6", (14.0, -34.0, 34.0, -8.0)),  // forêt NE
            ("Créature 7", (-33.0, -2.0, -5.0, 20.0)),  // lac / berges
            ("Créature 9", (-11.0, 23.0, 9.0, 35.0)),   // rizières
            ("Créature 11", (2.0, 4.0, 20.0, 22.0)),    // hameau
            ("Créature 12", (14.0, -34.0, 34.0, -8.0)), // forêt NE (2ᵉ clairière)
            ("Créature 13", (-33.0, -2.0, -5.0, 20.0)), // lac
            ("Créature 20", (20.0, 2.0, 34.0, 16.0)),   // promontoire
        ];
        for &(name, (x0, z0, x1, z1)) in biomes {
            let obj = scene
                .objects
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("« {name} » doit exister"));
            let p = obj.transform.position;
            assert!(
                p.x >= x0 && p.x <= x1 && p.z >= z0 && p.z <= z1,
                "« {name} » ({p:?}) doit être dans son biome {x0},{z0} → {x1},{z1}"
            );
        }
    }

    /// Preuve du décor animé (moulins, bannière, feu, épouvantail) : chaque
    /// instance porte un `AnimationState` sur un clip qui existe réellement
    /// dans le glb, avance (`speed > 0`), et les instances du même fichier ne
    /// sont pas synchronisées (phases de départ décalées). Les solides animés
    /// gardent un collider TriMesh (celui de la pose de repos).
    #[test]
    fn mmorpg_animated_decor_plays_looping_clips() {
        let scene = Scene::mmorpg_demo();
        for name in [
            "Moulin à eau",
            "Moulin à vent",
            "Bannière",
            "Feu de camp",
            "Épouvantail",
        ] {
            let obj = scene
                .objects
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("« {name} » doit exister"));
            let anim = obj
                .animation
                .as_ref()
                .unwrap_or_else(|| panic!("« {name} » doit avoir un AnimationState"));
            assert_eq!(anim.clip, "Idle", "« {name} » doit jouer son clip Idle");
            assert!(anim.speed > 0.0, "« {name} » doit être en mouvement");

            let mesh_index = match obj.mesh {
                MeshKind::Imported(i) => i,
                _ => panic!("« {name} » devrait être un mesh importé"),
            };
            let clips = &scene.imported[mesh_index as usize].clips;
            assert!(
                clips.iter().any(|c| c.name == "Idle"),
                "« {name} » : le glb doit contenir le clip Idle (clips={:?})",
                clips.iter().map(|c| &c.name).collect::<Vec<_>>()
            );
        }
        assert_eq!(
            scene
                .objects
                .iter()
                .find(|o| o.name == "Moulin à eau")
                .unwrap()
                .physics,
            PhysicsKind::Static,
            "un décor animé solide garde son collider TriMesh (pose de repos)"
        );
    }

    /// Preuve de l'ambiance visuelle (Étape 5 bis) : brume et ciel réglés
    /// explicitement (pas les valeurs plates par défaut), soleil orienté.
    #[test]
    fn mmorpg_demo_has_atmospheric_sky_and_fog() {
        let scene = Scene::mmorpg_demo();
        assert!(
            scene.sky.fog_density > 0.0,
            "la brume doit être activée pour la profondeur atmosphérique"
        );
        assert_ne!(
            scene.sky.horizon_color, scene.sky.zenith_color,
            "un vrai dégradé de ciel, pas un fond plat"
        );
        assert_ne!(
            scene.light.dir,
            Light::default().dir,
            "le soleil doit être orienté pour porter des ombres lisibles"
        );
    }

    /// Preuve de la demande gameplay « des objets à trouver dans la scène
    /// MMORPG » : la démo contient des `ItemPickup` d'au moins 3 sortes
    /// différentes, tous traversables (un objet à ramasser ne bloque ni le
    /// joueur ni les sondes des créatures) et dans l'arène ; le buisson à
    /// baies repousse (`respawn_delay > 0`), la clé et la gemme sont uniques.
    #[test]
    fn mmorpg_demo_contains_item_pickups_to_find() {
        let scene = Scene::mmorpg_demo();
        let items: Vec<&SceneObject> = scene
            .objects
            .iter()
            .filter(|o| o.item_pickup.is_some())
            .collect();
        let kinds: std::collections::HashSet<_> =
            items.iter().map(|o| o.item_pickup.unwrap().kind).collect();
        assert!(
            kinds.len() >= 3,
            "au moins 3 sortes d'objets à trouver (trouvé : {kinds:?})"
        );
        for o in &items {
            assert_eq!(
                o.physics,
                PhysicsKind::None,
                "« {} » : un objet à ramasser ne doit rien bloquer",
                o.name
            );
            let p = o.transform.position;
            assert!(
                p.x.abs() < Scene::MMORPG_HALF && p.z.abs() < Scene::MMORPG_HALF,
                "« {} » ({p:?}) doit être dans l'arène",
                o.name
            );
        }
        let by_name = |name: &str| {
            items
                .iter()
                .find(|o| o.name == name)
                .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {name} »"))
        };
        assert!(
            by_name("Buisson à baies").respawn_delay > 0.0,
            "le buisson à baies doit repousser"
        );
        for name in ["Clé du village", "Gemme"] {
            assert_eq!(
                by_name(name).respawn_delay,
                0.0,
                "« {name} » est une trouvaille unique, sans réapparition"
            );
        }
    }
}
