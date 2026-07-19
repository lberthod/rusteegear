//! Scènes de démo prêtes à jouer (`Scene::mobile_demo`, `Scene::zombies_demo`,
//! niveaux du contrôleur, etc.) et la scène embarquée du player exporté.
//! Extrait de `scene/mod.rs`.

use glam::Vec3;

use super::{
    AiChaser, AnimationState, Archetype, AudioSource, Combat, Controller, Convoy, GameCamera,
    HudAnchor, HudBinding, HudLayout, HudWidget, HudWidgetKind, ImportedMesh, ItemKind, ItemPickup,
    Light, MeshKind, MobileControls, PointLight, Scene, SceneObject, Sky, TapAction, Transform,
    WEAPONS, WeaponPickup, demo_obj,
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
pub(crate) fn creature_wander_script(
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
    _arena_half: f32,
    prefix: &str,
    _ray_mask: u32,
    _heading0: f32,
    _phase: f32,
) -> String {
    format!(
        r#"
    local DRIFT = 0.6
    local FLEE_SPEED = 1.6
    local FLEE = 3.0
    -- Rayon de dérive autour du point d'apparition (spawn), pas de l'arène :
    -- la méduse dérive dans SON lac, un petit plan d'eau borné par des murs
    -- invisibles à ~6-7 m du spawn (`Créature 13`, cf. `mmorpg_demo`). L'ancien
    -- rappel relatif à l'origine du monde (`atan(-obj.x, -obj.z)`) visait le
    -- centre de l'ARÈNE, pas celui du lac : dès que le lac a été muré, la
    -- méduse dérivait tout droit dans un mur et s'y frottait en boucle (jamais
    -- assez proche du centre de l'arène pour déclencher le rappel). `BOUND`
    -- reste un rayon local, pas une position absolue.
    local BOUND = 5.0

    local sx = save.get("{prefix}spawn_x")
    local sz = save.get("{prefix}spawn_z")
    if sx == nil then
        sx = obj.x; sz = obj.z
        save.set("{prefix}spawn_x", sx)
        save.set("{prefix}spawn_z", sz)
    end

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
    -- Rappel doux vers le spawn en approche du bord de la zone de dérive
    -- (fuir dans un mur n'a pas de sens).
    local ox, oz = obj.x - sx, obj.z - sz
    if math.abs(ox) > BOUND - 1.5 or math.abs(oz) > BOUND - 1.5 then
        target = math.deg(math.atan(-ox, -oz))
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
    obj.x = math.max(sx - BOUND, math.min(sx + BOUND, obj.x))
    obj.z = math.max(sz - BOUND, math.min(sz + BOUND, obj.z))
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

/// Charge un unique glTF d'`assets/models/` dans `imported` et renvoie le
/// `MeshKind::Imported` qui le référence — même pipeline que la grande table
/// `MMORPG_CREATURES`/`NATURE_DECOR`/`MONSTER_DECOR` (cf. leurs boucles) mais
/// pour une démo à une poignée d'objets qui n'a pas besoin d'une table
/// data-driven. `mesh.load_skinning()` est inconditionnel (comme dans ces
/// boucles) : sans effet sur un asset statique (peuple juste ses tangentes de
/// rendu), charge squelette/clips sur un asset riggé — le même appel convient
/// aux deux cas. Repli sur `fallback` (mesh primitif) si le fichier est
/// introuvable/invalide, plutôt que de faire planter la démo entière pour un
/// asset manquant.
fn import_single_model(
    imported: &mut Vec<ImportedMesh>,
    file: &str,
    fallback: MeshKind,
) -> MeshKind {
    let path = format!("{}/assets/models/{file}", env!("CARGO_MANIFEST_DIR"));
    match crate::scene::import::load_gltf(&path) {
        Ok((data, aabb_min, aabb_max)) => {
            let mut mesh = ImportedMesh {
                path,
                data,
                aabb_min,
                aabb_max,
                ..Default::default()
            };
            mesh.load_skinning();
            let mesh_index = imported.len() as u32;
            imported.push(mesh);
            MeshKind::Imported(mesh_index)
        }
        Err(e) => {
            log::error!("import_single_model({file}) : {e}");
            fallback
        }
    }
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
    /// **manches** (style Call of Duty Zombies) — 3 profils de monstres (`AiChaser`,
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

        // --- 3 profils de monstres, de plus en plus présents/variés à chaque manche
        // (comme les vagues d'un mode zombies) : Rôdeur (basique), Coureur (rapide et
        // fragile), Brute (lente mais très punitive et plus difficile à esquiver).
        // Chacun porte aussi un `archetype` (grammaire GDD §5.4, cf. `Archetype`) —
        // à ne pas confondre : `Kind` est un profil d'auteur local à cette démo
        // (stats/couleur/dégâts), `Archetype` est la famille de chasse partagée par
        // tout `AiChaser` du moteur.
        struct Kind {
            label: &'static str,
            speed: f32,
            dmg: f32,
            scale: f32,
            color: [f32; 3],
            archetype: Archetype,
            /// PV de base, avant `Archetype::hp_multiplier` (GDD_MMORPG.md §5.4).
            hp: u32,
        }
        const RODEUR: Kind = Kind {
            label: "Rôdeur",
            speed: 2.6,
            dmg: 0.8,
            scale: 0.7,
            color: [0.35, 0.55, 0.25],
            archetype: Archetype::Traqueuse,
            hp: 2,
        };
        const COUREUR: Kind = Kind {
            label: "Coureur",
            speed: 4.6,
            dmg: 0.5,
            scale: 0.55,
            color: [0.75, 0.8, 0.2],
            archetype: Archetype::Meute,
            hp: 2,
        };
        const BRUTE: Kind = Kind {
            label: "Brute",
            speed: 1.8,
            dmg: 2.2,
            scale: 1.3,
            color: [0.45, 0.08, 0.25],
            archetype: Archetype::Colosse,
            hp: 2,
        };
        // (manche, profils de cette manche) — la difficulté monte : plus de monstres,
        // puis des profils plus dangereux introduits progressivement.
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
                m.ai_chaser = Some(AiChaser {
                    speed: k.speed,
                    archetype: k.archetype,
                });
                m.combat = Some(Combat {
                    attackable: true,
                    wave,
                    // PV différenciés par archétype (GDD_MMORPG.md §5.4), cf.
                    // `Archetype::hp_multiplier`.
                    hp: ((k.hp as f32) * k.archetype.hp_multiplier())
                        .round()
                        .max(1.0) as u32,
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
        // Sprint 24 (Phase K, `sprintreflecion.md`) : `MeshKind::Terrain` remplace
        // l'ancien `MeshKind::Plane` plat — relief réel (collines) sur la marge
        // ouest de la carte, nul (y≈0, tolérance sub-centimétrique) partout où le
        // hameau/la forêt/l'eau/les rizières/la route sont déjà placés à la main
        // ci-dessous (cf. `gfx::mesh::mmorpg_terrain_local_height` pour le détail
        // du découpage en zones). Échelle X/Z = 2×`half` pour couvrir toute la
        // carte 72×72 m ; échelle Y = 1.0 (découplée) car la fonction de hauteur
        // renvoie déjà des mètres directement, pas un facteur à re-multiplier.
        let mut sol = demo_obj("Sol", MeshKind::Terrain, Vec3::new(0.0, 0.0, 0.0));
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
            // Gris-vert mousseux (cohérent avec `nature_rock`/`nature_moss_boulder`,
            // cf. leurs `baseColorFactor` ~0.44/0.44/0.42 et ~0.30/0.48/0.20) — l'ancien
            // mauve/lavande ([0.5, 0.45, 0.62]) jurait avec la palette naturelle,
            // d'autant plus visible que le Repère du coin (-15, 15) tombe juste sur
            // la rive ouest du lac (capture en jeu : « rocher rose » signalé là).
            repere.color = [0.42, 0.43, 0.36];
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
            /// Manche d'apparition (`Combat::wave`, GDD_MMORPG.md §5.5 — la
            /// dent de scie de « La Horde ») : la vague 1 est l'échauffement
            /// (errantes inoffensives), les suivantes montent en intensité.
            /// Règles d'authoring verrouillées par le test
            /// `mmorpg_demo_waves_follow_the_gdd_authoring_rules` : budget de
            /// PV strictement croissant par vague, au moins un chef à 3 PV
            /// dès la vague 2, dernière vague ≥ 4/3 de l'avant-dernière.
            wave: u32,
            /// PV (`Combat::hp`) : 1 pour la troupe, 3 pour un « chef » — la
            /// cible qui justifie le Boulet (GDD §5.5 : « un chef à 3 PV
            /// tombe d'un coup », sans elle l'arme lourde est objectivement
            /// inférieure).
            hp: u32,
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
                wave: 1,
                hp: 1,
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
                wave: 1,
                hp: 1,
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
                wave: 1,
                hp: 1,
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
                wave: 1,
                hp: 1,
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
                wave: 1,
                hp: 1,
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
                wave: 2,
                hp: 1,
            },
            // Crabe : pincement rare mais lourd — l'inverse de la chauve-souris.
            DemoCreature {
                name: "Créature 7",
                file: "creature7.glb",
                // Berge est du lac, à l'écart du nouveau mur d'eau (le crabe
                // spawnait auparavant quasi sur le bord du Lac, l:[-26,-12]
                // z:[-2,10], désormais bordé de murs invisibles).
                spawn: Vec3::new(-9.0, 0.0, 14.0),
                layer_bit: 7,
                prefix: "creature7_",
                heading0: 96.0,
                phase: 5.4,
                bite: Some((3.5, 0.6, 0.18, 27.1828)),
                script: creature_wander_script,
                wave: 2,
                hp: 3,
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
                wave: 3,
                hp: 1,
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
                wave: 3,
                hp: 1,
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
                wave: 3,
                hp: 1,
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
                wave: 3,
                hp: 3,
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
                wave: 3,
                hp: 1,
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
                wave: 3,
                hp: 1,
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
                wave: 4,
                hp: 3,
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
                wave: 4,
                hp: 1,
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
                wave: 4,
                hp: 3,
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
                wave: 4,
                hp: 1,
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
                wave: 4,
                hp: 3,
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
                wave: 4,
                hp: 1,
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
                wave: 3,
                hp: 3,
            },
            DemoCreature {
                name: "Créature 21",
                file: "creature21.glb",
                spawn: Vec3::new(4.0, 0.0, 26.0),
                layer_bit: 21,
                prefix: "creature21_",
                heading0: 12.0,
                phase: 18.0,
                bite: None,
                script: creature_wander_script,
                wave: 2,
                hp: 1,
            },
            DemoCreature {
                name: "Créature 22",
                file: "creature22.glb",
                spawn: Vec3::new(24.0, 0.0, -4.0),
                layer_bit: 22,
                prefix: "creature22_",
                heading0: 84.0,
                phase: 18.9,
                bite: None,
                script: creature_wander_script,
                wave: 2,
                hp: 1,
            },
            DemoCreature {
                name: "Créature 23",
                file: "creature23.glb",
                spawn: Vec3::new(-2.0, 0.0, 22.0),
                layer_bit: 23,
                prefix: "creature23_",
                heading0: 156.0,
                phase: 19.8,
                bite: None,
                script: creature_wander_script,
                wave: 2,
                hp: 1,
            },
            DemoCreature {
                name: "Créature 24",
                file: "creature24.glb",
                spawn: Vec3::new(-14.0, 0.0, -22.0),
                layer_bit: 24,
                prefix: "creature24_",
                heading0: 228.0,
                phase: 20.7,
                bite: None,
                // Pas `creature_burrow_script` : la charge garde un cap constant
                // pendant 1,1 s, et un corps scripté dont le TriMesh s'est
                // incrusté dans le sol (chute du 1er tick non arrêtée,
                // trimesh-vs-trimesh) peut se coincer sur un cap « malchanceux »
                // (normales de contact des triangles) — gel complet observé sur
                // ce mesh. Le méandre de wander varie son cap à chaque frame et
                // se décoince naturellement.
                script: creature_wander_script,
                wave: 2,
                hp: 1,
            },
            DemoCreature {
                name: "Créature 25",
                file: "creature25.glb",
                spawn: Vec3::new(14.0, 0.0, 26.0),
                layer_bit: 25,
                prefix: "creature25_",
                heading0: 300.0,
                phase: 21.6,
                bite: None,
                script: creature_zigzag_script,
                wave: 4,
                hp: 1,
            },
            DemoCreature {
                name: "Créature 26",
                file: "creature26.glb",
                spawn: Vec3::new(28.0, 0.0, 26.0),
                layer_bit: 26,
                prefix: "creature26_",
                heading0: 12.0,
                phase: 22.5,
                bite: None,
                script: creature_lemniscate_script,
                wave: 4,
                hp: 3,
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
                    // `wave`/`hp` : la dent de scie de « La Horde » (GDD §5.5),
                    // cf. les docs de `DemoCreature::wave`/`hp`.
                    creature.combat = Some(Combat {
                        attackable: true,
                        wave: spec.wave,
                        hp: spec.hp,
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

        // --- Faune ambiante (créatures 27-61, packs générés Blender headless) ----
        // Purement décorative : même pipeline d'import + `creature_wander_script`
        // que `MMORPG_CREATURES` ci-dessus (elles errent, évitent les murs, jouent
        // Idle/Walk), mais SANS `Combat` ni morsure — ni tuable, ni dangereuse.
        // Préfixe de nom « Errant » (ni « Créature », réservé à
        // `MMORPG_CREATURES`/ses outils de synchro, ni « Faune », déjà pris par
        // `hameau_gdd_demo()` — cf. son bloc « Faune ambiante »
        // `gen_menagerie_pack*.py` et ses noms `Faune {n} {cluster}-{poses}` de
        // `faune_scatter`, présents dans la scène déjà embarquée : un préfixe
        // partagé aurait fait retirer ce décor-là par erreur, constaté à
        // l'exécution de l'outil de synchro avant ce correctif).
        //
        // Table data-driven (fichier, spawn) plutôt que 35 blocs de champs répétés
        // (même choix que `MMORPG_CREATURES`) : cap initial et déphasage du
        // méandre dérivés de l'index pour que deux instances ne partent jamais
        // dans le même sens (cf. la doc de `creature_wander_script` sur ce bug).
        // Couche de collision : 5 bits partagés en rotation (27..31, pas un bit
        // par créature comme `MMORPG_CREATURES` — u32 ne peut décaler que jusqu'à
        // 31) ; deux errantes qui partagent un bit s'ignorent mutuellement au
        // raycast (se traversent), acceptable pour du décor sans aucun garde-fou
        // dessus, cf. le commentaire de doc sur `ray_mask`.
        const MMORPG_AMBIENT_FAUNA_SPAWNS: &[(&str, f32, f32)] = &[
            // Forêt nord-est (x resserré sur 9..20 pour rester à l'écart du
            // second hameau fortifié de `VILLAGE_PROPS`, x 23..35).
            ("creature27.glb", 10.0, -10.0),
            ("creature28.glb", 14.0, -14.0),
            ("creature29.glb", 18.0, -18.0),
            ("creature30.glb", 10.0, -18.0),
            ("creature31.glb", 14.0, -24.0),
            ("creature32.glb", 18.0, -28.0),
            ("creature33.glb", 10.0, -28.0),
            ("creature34.glb", 14.0, -32.0),
            ("creature35.glb", 18.0, -12.0),
            ("creature36.glb", 10.0, -32.0),
            // Prairie centrale.
            ("creature37.glb", -6.0, -8.0),
            ("creature38.glb", 0.0, -6.0),
            ("creature39.glb", 4.0, -2.0),
            ("creature40.glb", -4.0, 2.0),
            ("creature41.glb", 2.0, 6.0),
            ("creature42.glb", -8.0, 4.0),
            // Rives du lac et des rivières (x = -30/-31, à l'ouest des plans
            // d'eau et de leurs berges).
            ("creature43.glb", -31.0, -30.0),
            ("creature44.glb", -31.0, -20.0),
            ("creature45.glb", -30.0, 0.0),
            ("creature46.glb", -31.0, 20.0),
            ("creature47.glb", -30.0, 30.0),
            ("creature48.glb", -22.0, 16.0),
            // Rizières en damier (sud-ouest).
            ("creature49.glb", -8.0, 25.0),
            ("creature50.glb", -2.0, 26.0),
            ("creature51.glb", 4.0, 28.0),
            ("creature52.glb", -6.0, 32.0),
            ("creature53.glb", 2.0, 33.0),
            // Promontoire rocheux (est).
            ("creature54.glb", 22.0, 4.0),
            ("creature55.glb", 26.0, 6.0),
            ("creature56.glb", 30.0, 8.0),
            ("creature57.glb", 24.0, 12.0),
            ("creature58.glb", 32.0, 14.0),
            // Lisières diverses (complètent la répartition par biome).
            ("creature59.glb", 25.0, -5.0),
            ("creature60.glb", 15.0, -5.0),
            ("creature61.glb", 20.0, -30.0),
            // Renard (creature62, jusqu'ici généré mais jamais spawné — cf.
            // `docs/rapport_qualite_creatures_vs_hyper3d.md`), lisière ouest
            // de la prairie centrale, à l'écart du lac (`EXCL_EAU_ROUTES`) et
            // de la halte « Ouest souche/fleurs ».
            ("creature62.glb", -9.5, -0.5),
            // Pack savane africaine (creature63-67, `gen_creature_pack63_67.py`),
            // couloir libre entre rizières (x -11..9, z 23.5..34.5) et
            // promontoire rocheux (x 20..34, z 2..16) — chaque position
            // vérifiée à ≥4 m de tout décor/créature/halte déjà posé et de
            // ses 4 voisines du pack (`EXCL_EAU_ROUTES`/`EXCL_ZONES_AMENAGEES`
            // exclus par construction).
            ("creature63.glb", 9.0, 26.0),
            ("creature64.glb", 16.5, 33.0),
            ("creature65.glb", 22.0, 25.0),
            ("creature66.glb", 22.0, 29.0),
            ("creature67.glb", 22.0, 33.0),
            // Pack mammifères ronds au corps organique (creature68-72,
            // `gen_creature_pack68_72_organic.py`, Metaball + Automatic
            // Weights — cf. `proto_creature62_fox_organic.py`) : hippopotame,
            // capybara, loutre de mer, koala, marmotte. Dispersés en lisière
            // ouest/sud-ouest, à ≥4,5 m de tout décor/créature/halte déjà
            // posé (`EXCL_EAU_ROUTES`/`EXCL_ZONES_AMENAGEES` exclus par
            // construction).
            ("creature68.glb", -18.0, -12.0),
            ("creature69.glb", -23.5, -33.0),
            ("creature70.glb", -13.5, -13.0),
            ("creature71.glb", -12.5, 34.0),
            ("creature72.glb", -21.5, -3.5),
        ];
        for (i, &(file, x, z)) in MMORPG_AMBIENT_FAUNA_SPAWNS.iter().enumerate() {
            let n = i + 27;
            let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
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
                    let name = format!("Errant {n}");
                    let prefix = format!("faune{n}_");
                    let mut fauna =
                        demo_obj(&name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
                    // Gabarit 0.28..0.45 plutôt qu'une échelle 0.35 fixe pour les 35
                    // instances (constaté sur une capture en jeu : à taille identique
                    // et à hauteur d'œil, elles se fondent en points flous indistincts).
                    // Suite du ratio doré (même famille de trucs que `heading0`/`phase`
                    // ci-dessous) : distribution à faible discrépance, deux voisines
                    // n'ont jamais un gabarit presque identique.
                    let gabarit = 0.28 + 0.17 * (i as f32 * 0.618_034).fract();
                    fauna.transform = fauna.transform.with_scale(Vec3::splat(gabarit));
                    fauna.animation = Some(AnimationState {
                        clip: "Idle".into(),
                        ..Default::default()
                    });
                    fauna.physics = PhysicsKind::Kinematic;
                    let layer_bit = 27 + (i as u32 % 5);
                    fauna.collision_layer = 1 << layer_bit;
                    let heading0 = (i as f32 * 47.0).rem_euclid(360.0);
                    let phase = i as f32 * 0.833;
                    fauna.script = creature_wander_script(
                        half,
                        &prefix,
                        !(1_u32 << layer_bit),
                        heading0,
                        phase,
                    );
                    objects.push(fauna);
                    imported.push(mesh);
                }
                Err(e) => log::error!("Errant {n} MMORPG ({path}) : {e}"),
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
        // 1) Aplats de terrain : primitives `Plane`, décalées de quelques
        //    centimètres en Y (eau < sable < chemins < route) pour éviter le
        //    z-fighting avec le sol et entre elles. L'eau reste un `Plane`
        //    (pas de heightmap dans ce moteur), mais désormais bordée de murs
        //    invisibles (étape 2 ci-dessous) : rivières et lac ne se
        //    traversent plus à gué, seuls les deux ponts passent.
        let eau = [0.18, 0.42, 0.65];
        let eau_sombre = [0.14, 0.34, 0.55];
        let terre = [0.42, 0.36, 0.26];
        let vert_riziere = [0.24, 0.44, 0.38];
        // Surface plus lisse/brillante que le sol : sans reflet d'environnement
        // réel (le moteur n'a ni alpha ni uniforme de temps, cf. la doc de
        // `mmorpg_demo`), un `roughness` bas donne un reflet spéculaire net qui
        // suffit à distinguer l'eau d'un simple aplat de peinture bleue.
        let mut aplat_eau = |name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]| {
            let mut p = demo_obj(name, MeshKind::Plane, pos);
            p.transform = p.transform.with_scale(scale);
            p.color = color;
            p.roughness = 0.08;
            p.metallic = 0.15;
            objects.push(p);
        };
        aplat_eau(
            "Rivière nord",
            Vec3::new(-26.0, 0.02, -21.0),
            Vec3::new(4.0, 1.0, 30.0),
            eau,
        );
        aplat_eau(
            "Coude de rivière",
            Vec3::new(-22.0, 0.02, -6.0),
            Vec3::new(12.0, 1.0, 4.0),
            eau,
        );
        aplat_eau(
            "Lac",
            Vec3::new(-19.0, 0.015, 4.0),
            Vec3::new(14.0, 1.0, 12.0),
            eau_sombre,
        );
        aplat_eau(
            "Rivière sud",
            Vec3::new(-16.0, 0.02, 23.0),
            Vec3::new(4.0, 1.0, 26.0),
            eau,
        );
        let mut aplat = |name: &str, pos: Vec3, scale: Vec3, color: [f32; 3]| {
            let mut p = demo_obj(name, MeshKind::Plane, pos);
            p.transform = p.transform.with_scale(scale);
            p.color = color;
            objects.push(p);
        };
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
        // Trois liaisons de voirie qui recousent les biomes entre eux (les
        // biomes existaient mais on n'y « allait » que hors piste) : un chemin
        // route → rizières dans l'interstice entre les damiers 1 et 2, le
        // prolongement du chemin du pont nord vers le promontoire (le chemin
        // s'arrêtait à x=6, la tour de guet restait hors réseau), et une place
        // de terre battue autour du puits du hameau. Y : la place (0.031)
        // au-dessus du chemin du hameau (0.028) qu'elle chevauche ; les deux
        // chemins sous la route (0.03), même logique anti z-fighting que les
        // aplats existants.
        aplat(
            "Chemin des rizières",
            Vec3::new(-4.5, 0.026, 20.0),
            Vec3::new(1.8, 1.0, 12.0),
            terre,
        );
        aplat(
            "Sentier du promontoire",
            Vec3::new(20.0, 0.027, -10.0),
            Vec3::new(28.0, 1.0, 1.6),
            terre,
        );
        aplat(
            "Place du hameau",
            Vec3::new(10.0, 0.031, 7.0),
            Vec3::new(7.0, 1.0, 6.0),
            terre,
        );

        // 2) Murs d'eau invisibles : bordent les 4 plans d'eau ci-dessus pour
        //    les rendre réellement infranchissables (collider `Static` seul,
        //    `visible = false` — l'eau garde son aspect, elle bloque juste le
        //    passage comme une vraie rivière). Seules ouvertures : les deux
        //    ponts existants (`Pont 1` sur la rivière sud, `Pont 2` sur la
        //    rivière nord), qui redeviennent ainsi les seuls passages — le
        //    commentaire historique « les ponts sont narratifs » ne l'est
        //    plus.
        //
        //    Les 4 rects d'eau (rivière nord, coude, lac, rivière sud) se
        //    chevauchent/se touchent de façon irrégulière (le coude et le lac
        //    ont même un interstice de 2 m entre eux) : les murer à la main
        //    côté par côté s'est révélé source d'erreurs de continuité (essayé
        //    puis abandonné — un bord mal raccordé laisse une brèche). Plus
        //    fiable : rasteriser l'UNION des 4 rects sur une grille et poser
        //    un segment de mur à chaque frontière eau→terre — la topologie du
        //    contour est alors dérivée automatiquement, pas raisonnée à la
        //    main. `GRID` = 3 m (assez fin pour suivre les angles à ~1,5 m
        //    près, largement sous la marge de 3,5 m des sondes créatures).
        //    `GRID` = 1 m (pas 3 m comme au premier jet) : au grain plus
        //    large, l'ouverture minimale au droit d'un pont devait couvrir 2
        //    cellules pour ne pas rogner le tablier (~1,84 m de large), ce qui
        //    ouvrait ~6 m de berge — largement plus que le pont, le joueur
        //    pouvait entrer dans l'eau en longeant la rive à côté du tablier
        //    sans jamais poser le pied dessus. À 1 m, l'ouverture se resserre
        //    à ~3 m (tablier + ~0,6 m de marge de chaque côté), et le mur
        //    repousse immédiatement quiconque s'écarte du pont.
        {
            const GRID: f32 = 1.0;
            let water_rects: [(f32, f32, f32, f32); 4] = [
                (-28.0, -36.0, -24.0, -6.0), // rivière nord
                (-28.0, -8.0, -16.0, -4.0),  // coude
                (-26.0, -2.0, -12.0, 10.0),  // lac
                (-18.0, 10.0, -14.0, 36.0),  // rivière sud
            ];
            // Rectangles où aucun mur ne doit être posé : juste assez larges
            // pour couvrir le tablier du pont (largeur réelle ~1,84 m,
            // `gen_bridge()` × échelle 1.15) sans plus — un gap trop large
            // laisserait le joueur entrer dans l'eau à côté du pont sans
            // jamais l'emprunter.
            let bridge_gaps: [(f32, f32, f32, f32); 2] = [
                (-29.0, -11.5, -23.0, -8.5), // Pont 2 (rivière nord, z≈-10)
                (-19.0, 12.5, -13.0, 15.5),  // Pont 1 (rivière sud, z≈14)
            ];
            let is_water = |x: f32, z: f32| {
                water_rects
                    .iter()
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
            };
            let in_gap = |x: f32, z: f32| {
                bridge_gaps
                    .iter()
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
            };
            // Bornes de balayage : englobent les 4 rects avec une marge d'une
            // cellule pour détecter la frontière extérieure.
            let (mut gx0, mut gz0, mut gx1, mut gz1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for &(x0, z0, x1, z1) in &water_rects {
                gx0 = gx0.min(x0);
                gz0 = gz0.min(z0);
                gx1 = gx1.max(x1);
                gz1 = gz1.max(z1);
            }
            let mut mur_n = 0u32;
            let mut cx = gx0 - GRID;
            while cx <= gx1 + GRID {
                let mut cz = gz0 - GRID;
                while cz <= gz1 + GRID {
                    if is_water(cx, cz) {
                        // Frontière est/ouest (mur vertical le long de X constant).
                        for &(nx, wall_x) in
                            &[(cx + GRID, cx + GRID / 2.0), (cx - GRID, cx - GRID / 2.0)]
                        {
                            if !is_water(nx, cz) && !in_gap(wall_x, cz) {
                                mur_n += 1;
                                let mut w = demo_obj(
                                    &format!("Mur d'eau {mur_n}"),
                                    MeshKind::Cube,
                                    Vec3::new(wall_x, 0.9, cz),
                                );
                                w.transform = w.transform.with_scale(Vec3::new(0.4, 1.8, GRID));
                                w.physics = PhysicsKind::Static;
                                w.visible = false;
                                objects.push(w);
                            }
                        }
                        // Frontière nord/sud (mur horizontal le long de Z constant).
                        for &(nz, wall_z) in
                            &[(cz + GRID, cz + GRID / 2.0), (cz - GRID, cz - GRID / 2.0)]
                        {
                            if !is_water(cx, nz) && !in_gap(cx, wall_z) {
                                mur_n += 1;
                                let mut w = demo_obj(
                                    &format!("Mur d'eau {mur_n}"),
                                    MeshKind::Cube,
                                    Vec3::new(cx, 0.9, wall_z),
                                );
                                w.transform = w.transform.with_scale(Vec3::new(GRID, 1.8, 0.4));
                                w.physics = PhysicsKind::Static;
                                w.visible = false;
                                objects.push(w);
                            }
                        }
                    }
                    cz += GRID;
                }
                cx += GRID;
            }
        }

        // 1 bis) Sprint 26 (Phase K, `sprintreflecion.md`) : petit bassin
        //    intégré à un contrefort du relief existant (`gfx::mesh::
        //    mmorpg_terrain_local_height`, zone `MMORPG_MOUND_X_LOCAL`/
        //    `MMORPG_MOUND_Z_LOCAL`) — PAS un retrofit du lac historique
        //    ci-dessus (`water_rects`), qui reste posé sur un sol resté plat
        //    à dessein (des centaines de placements en dépendent, cf. la doc
        //    de `mmorpg_terrain_local_height`). Zone (x∈[-34,-30],
        //    z∈[3,5;8,5]) vérifiée numériquement libre de tout décor/spawn
        //    placé à la main à ≥3 m près (`NATURE_DECOR`/`VILLAGE_PROPS`/
        //    `MONSTER_DECOR`/`MMORPG_HALTES`/`MMORPG_CREATURES`/
        //    `MMORPG_AMBIENT_FAUNA_SPAWNS`/`EXCL_*`), y compris du semis
        //    procédural le plus proche (« Arbre du sud », x∈[22,34] z∈[30,35],
        //    seule autre poche libre trouvée côté sud-est de la carte — trop
        //    loin pour interférer ici) : juste à l'est de la bande de
        //    collines existante, dont le relief retombe déjà à 0 dès
        //    x=-34,5. Rive nord du bassin (z≈7, contre le contrefort) suit
        //    donc une pente réelle du terrain ; rive sud (z≈8,5, côté champ
        //    ouvert) reste sur du plat, comme n'importe quelle berge. Murs
        //    invisibles sur les 4 côtés — même principe que les « Mur d'eau »
        //    ci-dessus, mais un seul rectangle isolé (pas de pont à
        //    ménager) : pas besoin de l'algorithme de rastérisation par
        //    union, 4 plaques statiques suffisent.
        {
            const SE_LAKE: (f32, f32, f32, f32) = (-33.5, 7.0, -31.0, 8.5); // (x0,z0,x1,z1)
            let (x0, z0, x1, z1) = SE_LAKE;
            let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
            let mut p = demo_obj(
                "Bassin du contrefort",
                MeshKind::Plane,
                Vec3::new(cx, 0.02, cz),
            );
            p.transform = p.transform.with_scale(Vec3::new(x1 - x0, 1.0, z1 - z0));
            p.color = eau_sombre;
            p.roughness = 0.08;
            p.metallic = 0.15;
            objects.push(p);

            let wall_h = 1.8;
            let thick = 0.4;
            let mut bassin_wall = |name: &str, pos: Vec3, scale: Vec3| {
                let mut w = demo_obj(name, MeshKind::Cube, pos);
                w.transform = w.transform.with_scale(scale);
                w.physics = PhysicsKind::Static;
                w.visible = false;
                objects.push(w);
            };
            bassin_wall(
                "Mur bassin Nord",
                Vec3::new(cx, wall_h * 0.5, z0),
                Vec3::new(x1 - x0 + thick, wall_h, thick),
            );
            bassin_wall(
                "Mur bassin Sud",
                Vec3::new(cx, wall_h * 0.5, z1),
                Vec3::new(x1 - x0 + thick, wall_h, thick),
            );
            bassin_wall(
                "Mur bassin Ouest",
                Vec3::new(x0, wall_h * 0.5, cz),
                Vec3::new(thick, wall_h, z1 - z0 + thick),
            );
            bassin_wall(
                "Mur bassin Est",
                Vec3::new(x1, wall_h * 0.5, cz),
                Vec3::new(thick, wall_h, z1 - z0 + thick),
            );
        }

        // Tunnel/surplomb (Sprint 26) : passage praticable sous un arceau
        // statique — géométrie non-heightmap à dessein, un heightmap XZ→Y ne
        // représente pas un surplomb (cf. l'objectif du sprint dans
        // `sprintreflecion.md`) — posé sur du sol resté PLAT (hors de toute
        // zone de relief : x∈[-34.2,-31.0] est à l'ouest de
        // `MMORPG_MOUND_X_LOCAL`, qui commence à x=-30,2, donc `mound_h` y
        // est nul ; la bande de collines historique est, elle, nulle dès
        // x=-34,5). Même poche libre que le bassin ci-dessus mais décalée en
        // Z (couloir x∈[-34.4,-30.8] z∈[-8.8,-2.8], vérifié libre
        // séparément) : deux piliers encadrant un passage de 2 m de large,
        // surmontés d'un toit — tous statiques, le sol sous le passage garde
        // son collider dalle plate habituel (aucun trou de collision).
        {
            let pillar_y = 1.1;
            let pillar_h = 2.2;
            let z_center = -5.75;
            let z_len = 5.1;
            let mut arche = |name: &str, pos: Vec3, scale: Vec3| {
                let mut c = demo_obj(name, MeshKind::Cube, pos);
                c.transform = c.transform.with_scale(scale);
                c.physics = PhysicsKind::Static;
                c.color = [0.5, 0.48, 0.44];
                objects.push(c);
            };
            arche(
                "Pilier tunnel Ouest",
                Vec3::new(-33.9, pillar_y, z_center),
                Vec3::new(0.6, pillar_h, z_len),
            );
            arche(
                "Pilier tunnel Est",
                Vec3::new(-31.3, pillar_y, z_center),
                Vec3::new(0.6, pillar_h, z_len),
            );
            arche(
                "Toit tunnel",
                Vec3::new(-32.6, pillar_y + pillar_h * 0.5 + 0.2, z_center),
                Vec3::new(3.2, 0.4, z_len),
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
            // Habillage des trois liaisons de voirie ajoutées aux aplats
            // (chemin des rizières, sentier du promontoire, place du hameau) :
            // pierres au carrefour route/chemin, pierre levée qui balise le
            // sentier est, panneaux aux embranchements, lanternes aux étapes.
            // Tout est posé en bord de voie, jamais sur la bande roulante.
            // IMPORTANT : dans NATURE_DECOR (pas MONSTER_DECOR) — seuls
            // NATURE_DECOR et VILLAGE_PROPS alimentent `solid_spots`, la
            // liste que le scatter procédural évite à ≥ 2,5 m.
            DemoDecor {
                name: "Pierre du carrefour 1",
                file: "nature_rock.glb",
                pos: (14.5, 17.6),
                scale: 0.45,
                yaw_deg: 25.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Pierre du carrefour 2",
                file: "nature_rock.glb",
                pos: (16.8, 19.2),
                scale: 0.3,
                yaw_deg: 130.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Pierre levée du sentier",
                file: "nature_rock.glb",
                // Bord sud du sentier, hors du rectangle de la forêt NE.
                pos: (30.0, -7.6),
                scale: 0.7,
                yaw_deg: 30.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Panneau des rizières",
                file: "nature_signpost.glb",
                pos: (-6.2, 15.8),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Panneau du promontoire",
                file: "nature_signpost.glb",
                pos: (7.0, -11.8),
                scale: 1.0,
                yaw_deg: 270.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Lanterne du carrefour",
                file: "nature_lantern.glb",
                pos: (8.4, 15.7),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Lanterne du sentier est",
                file: "nature_lantern.glb",
                pos: (24.0, -8.4),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            // --- Pack « pierre & mystique » (gen_stone_pack.py) : landmarks
            //     minéraux et sacrés qui étoffent les zones encore vides —
            //     landes de l'est, ruines du sud, clairières. Tous solides
            //     (TriMesh silhouette : on se faufile entre les pierres du
            //     cromlech ou sous l'arche), tous à ≥ 4 m des spawns. ---
            DemoDecor {
                name: "Menhir",
                file: "nature_menhir.glb",
                pos: (32.0, -2.0),
                scale: 1.0,
                yaw_deg: 30.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Cairn du sentier",
                file: "nature_cairn.glb",
                pos: (27.0, -8.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Cromlech",
                file: "nature_stone_circle.glb",
                pos: (33.0, 18.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            // Ruines du sud : l'arche et sa colonne racontent le domaine
            // disparu, entre le clocheton et la balançoire.
            DemoDecor {
                name: "Arche en ruine",
                file: "nature_ruin_arch.glb",
                pos: (7.0, -26.0),
                scale: 1.0,
                yaw_deg: 20.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Colonne brisée",
                file: "nature_ruin_column.glb",
                pos: (10.0, -28.0),
                scale: 1.0,
                yaw_deg: 70.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Cristaux",
                file: "nature_crystal_cluster.glb",
                pos: (18.0, -28.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            // Bord du chemin des rizières, tourné vers les marcheurs (est).
            DemoDecor {
                name: "Autel du chemin",
                file: "nature_shrine.glb",
                pos: (-7.5, 20.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Lanterne de pierre",
                file: "nature_stone_lantern.glb",
                pos: (17.8, 13.2),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Ruche",
                file: "nature_beehive.glb",
                pos: (19.5, 21.5),
                scale: 1.0,
                yaw_deg: 210.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Totem",
                file: "nature_totem.glb",
                pos: (-24.0, -18.0),
                scale: 1.0,
                yaw_deg: 150.0,
                solide: true,
                anim: None,
            },
            // Kiosque du hameau : l'entrée (face -Z du glb) regarde le chemin
            // au sud, l'intérieur reste traversable entre les poteaux.
            DemoDecor {
                name: "Kiosque",
                file: "nature_gazebo.glb",
                pos: (3.5, 19.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
        ];

        // Hameau fortifié : assets « maison », générés procéduralement par
        // scripts/blender/gen_hamlet_*.py (cf. sprintcration3delement.md et
        // la mémoire projet `charte-graphique-assets-maison` pour le détail
        // du pipeline). Remplace depuis le Sprint 7 les assets tiers
        // village_*.glb (Medieval Village Pack, Quaternius/CC0, retraités par
        // scripts/blender/import_village_pack.py) — mêmes noms de fonction/
        // silhouette, aucune géométrie tierce réutilisée. Posé dans le coin
        // sud-est de la carte (x∈[22,36], z∈[-36,-20]), à l'écart du hameau
        // existant et à ≥ 4 m du spawn le plus proche (créature 5, (20,-20))
        // — la règle des 3,5 m (RAY_DIST) ne s'applique qu'au décor solide,
        // testée plus bas.
        const VILLAGE_PROPS: &[DemoDecor] = &[
            // --- Bâtiments (grille 3×4, colonnes x=25/29/33, lignes
            //     z=-23/-27/-30.5/-34) ---
            DemoDecor {
                name: "Caserne du hameau",
                file: "hamlet_barracks.glb",
                pos: (25.0, -23.0),
                scale: 1.0,
                yaw_deg: 200.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Tour du hameau",
                file: "hamlet_bell_tower.glb",
                pos: (29.0, -23.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Auberge",
                file: "hamlet_inn.glb",
                pos: (33.0, -23.0),
                scale: 1.0,
                yaw_deg: 160.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Forge",
                file: "hamlet_blacksmith.glb",
                pos: (25.0, -27.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Puits du hameau",
                file: "hamlet_well.glb",
                pos: (29.0, -27.0),
                scale: 2.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Scierie",
                file: "hamlet_sawmill.glb",
                pos: (33.0, -27.0),
                scale: 1.0,
                yaw_deg: 270.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Maison du hameau 1",
                file: "hamlet_house_a.glb",
                pos: (25.0, -30.5),
                scale: 1.0,
                yaw_deg: 190.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Gazebo du hameau",
                file: "hamlet_gazebo.glb",
                pos: (29.0, -30.5),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Moulin du hameau",
                file: "hamlet_mill.glb",
                pos: (33.0, -30.5),
                scale: 1.0,
                yaw_deg: 250.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Maison du hameau 2",
                file: "hamlet_house_b.glb",
                pos: (25.0, -34.0),
                scale: 1.0,
                yaw_deg: 180.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Écurie",
                file: "hamlet_stable.glb",
                pos: (29.0, -34.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Maison du hameau 3",
                file: "hamlet_house_c.glb",
                pos: (33.0, -34.0),
                scale: 1.0,
                yaw_deg: 220.0,
                solide: true,
                anim: None,
            },
            // --- Étals et clôtures (solides) ---
            DemoDecor {
                name: "Étal du marché 1",
                file: "hamlet_market_stand_a.glb",
                pos: (27.0, -24.5),
                scale: 2.2,
                yaw_deg: 45.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Étal du marché 2",
                file: "hamlet_market_stand_b.glb",
                pos: (31.0, -24.5),
                scale: 2.2,
                yaw_deg: 315.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture du hameau 1",
                file: "hamlet_fence.glb",
                pos: (26.0, -19.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture du hameau 2",
                file: "hamlet_fence.glb",
                pos: (29.0, -19.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture du hameau 3",
                file: "hamlet_fence.glb",
                pos: (32.0, -19.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture du hameau 4",
                file: "hamlet_fence.glb",
                pos: (27.0, -35.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Clôture du hameau 5",
                file: "hamlet_fence.glb",
                pos: (31.0, -35.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            // --- Petit décor solide (tonneaux, caisses, bancs, rochers) ---
            DemoDecor {
                name: "Tonneau du hameau 1",
                file: "hamlet_barrel.glb",
                pos: (23.0, -24.0),
                scale: 3.0,
                yaw_deg: 15.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Tonneau du hameau 2",
                file: "hamlet_barrel.glb",
                pos: (34.7, -25.0),
                scale: 3.0,
                yaw_deg: 80.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Tonneau du hameau 3",
                file: "hamlet_barrel.glb",
                pos: (27.5, -21.5),
                scale: 3.0,
                yaw_deg: 200.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Caisse du hameau 1",
                file: "hamlet_crate.glb",
                pos: (23.0, -28.0),
                scale: 3.0,
                yaw_deg: 25.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Caisse du hameau 2",
                file: "hamlet_crate.glb",
                pos: (34.7, -29.0),
                scale: 3.0,
                yaw_deg: 140.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Caisse du hameau 3",
                file: "hamlet_crate.glb",
                pos: (30.5, -21.5),
                scale: 3.0,
                yaw_deg: 300.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Banc du hameau 1",
                file: "hamlet_bench_a.glb",
                pos: (27.0, -28.75),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Banc du hameau 2",
                file: "hamlet_bench_a.glb",
                pos: (31.0, -28.75),
                scale: 3.0,
                yaw_deg: 180.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Banc du hameau 3",
                file: "hamlet_bench_b.glb",
                pos: (27.0, -32.25),
                scale: 3.0,
                yaw_deg: 90.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Banc du hameau 4",
                file: "hamlet_bench_b.glb",
                pos: (31.0, -32.25),
                scale: 3.0,
                yaw_deg: 270.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher du hameau 1",
                file: "hamlet_rocks.glb",
                pos: (23.0, -32.0),
                scale: 2.5,
                yaw_deg: 10.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher du hameau 2",
                file: "hamlet_rocks.glb",
                pos: (34.7, -32.5),
                scale: 2.2,
                yaw_deg: 80.0,
                solide: true,
                anim: None,
            },
            DemoDecor {
                name: "Rocher du hameau 3",
                file: "hamlet_rocks.glb",
                pos: (24.0, -20.5),
                scale: 2.0,
                yaw_deg: 140.0,
                solide: true,
                anim: None,
            },
            // --- Décor traversable (sacs, foin, paquets, portes/fenêtres
            //     posées, fumée…) : aucune contrainte de dégagement. ---
            DemoDecor {
                name: "Sac du hameau 1",
                file: "hamlet_bag.glb",
                pos: (28.0, -24.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Sac du hameau 2",
                file: "hamlet_bag.glb",
                pos: (32.0, -31.0),
                scale: 3.0,
                yaw_deg: 90.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Sac ouvert du hameau",
                file: "hamlet_bag_open.glb",
                pos: (26.0, -24.5),
                scale: 3.0,
                yaw_deg: 45.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Sacs du hameau",
                file: "hamlet_bags.glb",
                pos: (34.0, -31.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Foin du hameau 1",
                file: "hamlet_hay.glb",
                pos: (24.0, -22.0),
                scale: 3.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Foin du hameau 2",
                file: "hamlet_hay.glb",
                pos: (34.0, -24.0),
                scale: 3.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Foin du hameau 3",
                file: "hamlet_hay.glb",
                pos: (28.0, -32.0),
                scale: 3.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Paquet du hameau 1",
                file: "hamlet_package_a.glb",
                pos: (30.0, -24.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Paquet du hameau 2",
                file: "hamlet_package_a.glb",
                pos: (24.0, -31.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Paquet du hameau 3",
                file: "hamlet_package_b.glb",
                pos: (32.0, -28.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Paquet du hameau 4",
                file: "hamlet_package_b.glb",
                pos: (26.0, -29.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Chaudron de la forge",
                file: "hamlet_cauldron.glb",
                pos: (24.5, -27.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Feu du hameau",
                file: "hamlet_bonfire.glb",
                pos: (28.0, -29.0),
                scale: 3.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fumée de la forge",
                file: "hamlet_smoke.glb",
                pos: (24.5, -27.5),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fumée de la scierie",
                file: "hamlet_smoke.glb",
                pos: (33.0, -27.3),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Lame de la scierie",
                file: "hamlet_sawmill_saw.glb",
                pos: (33.0, -27.6),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Porte ronde du hameau",
                file: "hamlet_door_round.glb",
                pos: (27.0, -22.5),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Porte droite du hameau",
                file: "hamlet_door_straight.glb",
                pos: (31.0, -23.5),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fenêtre ronde 1",
                file: "hamlet_round_window.glb",
                pos: (24.3, -23.5),
                scale: 2.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fenêtre ronde 2",
                file: "hamlet_round_window.glb",
                pos: (33.7, -31.0),
                scale: 2.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fenêtre du hameau 1",
                file: "hamlet_window_a.glb",
                pos: (25.5, -26.0),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fenêtre du hameau 2",
                file: "hamlet_window_a.glb",
                pos: (32.5, -31.5),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fenêtre du hameau 3",
                file: "hamlet_window_b.glb",
                pos: (28.5, -31.0),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Fenêtre du hameau 4",
                file: "hamlet_window_b.glb",
                pos: (30.5, -27.0),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Cloche du hameau",
                file: "hamlet_bell.glb",
                pos: (29.0, -22.3),
                scale: 2.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Marches du hameau 1",
                file: "hamlet_stairs.glb",
                pos: (29.0, -21.7),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Marches du hameau 2",
                file: "hamlet_stairs.glb",
                pos: (33.0, -29.3),
                scale: 2.5,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Chemin du hameau 1",
                file: "hamlet_path_straight.glb",
                pos: (29.0, -21.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Chemin du hameau 2",
                file: "hamlet_path_straight.glb",
                pos: (29.0, -24.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Chemin du hameau 3",
                file: "hamlet_path_straight.glb",
                pos: (29.0, -27.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
            DemoDecor {
                name: "Chemin du hameau 4",
                file: "hamlet_path_straight.glb",
                pos: (29.0, -30.0),
                scale: 3.0,
                yaw_deg: 0.0,
                solide: false,
                anim: None,
            },
        ];

        // Ménagerie de monstres (Ultimate Monsters Bundle, Quaternius/CC0,
        // retraité par scripts/blender/import_monster_pack.py — armature/
        // skin retirés à l'export, ce ne sont que des silhouettes figées en
        // pose de repos). Non solides à dessein : ce pack est riggé dans le
        // fichier source, et `MAX_SKINNED_INSTANCES` (src/gfx/renderer.rs,
        // partagé avec les créatures MMORPG et le décor nature animé, déjà
        // ~66/96 utilisés) aurait explosé si ces 45 assets gardaient leur
        // squelette — cf. le script pour le détail. Posés en grille dans la
        // bande nord jusque-là vide (x∈[-16,16], z∈[-34,-20]), à l'écart de
        // toute zone déjà aménagée.
        const MONSTER_DECOR: &[DemoDecor] = &[
            DemoDecor {
                name: "Monstre Alien",
                file: "monster_alien.glb",
                pos: (-16.0, -34.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Alien 2",
                file: "monster_alien_b.glb",
                pos: (-12.0, -34.0),
                scale: 0.9,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Alpaking",
                file: "monster_alpaking.glb",
                pos: (-8.0, -34.0),
                scale: 0.9,
                yaw_deg: 90.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Alpaking évolué",
                file: "monster_alpaking_evolved.glb",
                pos: (-4.0, -34.0),
                scale: 0.9,
                yaw_deg: 135.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Armabee",
                file: "monster_armabee.glb",
                pos: (0.0, -34.0),
                scale: 0.9,
                yaw_deg: 180.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Armabee évolué",
                file: "monster_armabee_evolved.glb",
                pos: (4.0, -34.0),
                scale: 0.9,
                yaw_deg: 225.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Birb",
                file: "monster_birb.glb",
                pos: (8.0, -34.0),
                scale: 1.0,
                yaw_deg: 270.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Démon bleu",
                file: "monster_blue_demon.glb",
                pos: (12.0, -34.0),
                scale: 0.9,
                yaw_deg: 315.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Lapin monstre",
                file: "monster_bunny.glb",
                pos: (16.0, -34.0),
                scale: 0.9,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Cactoro",
                file: "monster_cactoro.glb",
                pos: (-16.0, -30.5),
                scale: 0.9,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Cactoro 2",
                file: "monster_cactoro_b.glb",
                pos: (-12.0, -30.5),
                scale: 0.9,
                yaw_deg: 90.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Chat monstre",
                file: "monster_cat.glb",
                pos: (-8.0, -30.5),
                scale: 1.3,
                yaw_deg: 135.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Poule monstre",
                file: "monster_chicken.glb",
                pos: (-4.0, -30.5),
                scale: 1.3,
                yaw_deg: 180.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Démon",
                file: "monster_demon.glb",
                pos: (0.0, -30.5),
                scale: 0.75,
                yaw_deg: 225.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Démon 2",
                file: "monster_demon_b.glb",
                pos: (4.0, -30.5),
                scale: 0.9,
                yaw_deg: 270.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Dino",
                file: "monster_dino.glb",
                pos: (8.0, -30.5),
                scale: 0.9,
                yaw_deg: 315.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Dragon",
                file: "monster_dragon.glb",
                pos: (12.0, -30.5),
                scale: 0.9,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Dragon évolué",
                file: "monster_dragon_evolved.glb",
                pos: (16.0, -30.5),
                scale: 0.75,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Poisson monstre",
                file: "monster_fish.glb",
                pos: (-16.0, -27.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Poisson monstre 2",
                file: "monster_fish_b.glb",
                pos: (-12.0, -27.0),
                scale: 0.9,
                yaw_deg: 135.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Grenouille monstre",
                file: "monster_frog.glb",
                pos: (-8.0, -27.0),
                scale: 0.9,
                yaw_deg: 180.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Fantôme",
                file: "monster_ghost.glb",
                pos: (-4.0, -27.0),
                scale: 0.75,
                yaw_deg: 225.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Crâne fantôme",
                file: "monster_ghost_skull.glb",
                pos: (0.0, -27.0),
                scale: 0.75,
                yaw_deg: 270.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Glub",
                file: "monster_glub.glb",
                pos: (4.0, -27.0),
                scale: 0.9,
                yaw_deg: 315.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Glub évolué",
                file: "monster_glub_evolved.glb",
                pos: (8.0, -27.0),
                scale: 0.9,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Goleling",
                file: "monster_goleling.glb",
                pos: (12.0, -27.0),
                scale: 0.9,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Goleling évolué",
                file: "monster_goleling_evolved.glb",
                pos: (16.0, -27.0),
                scale: 0.9,
                yaw_deg: 90.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Blob vert",
                file: "monster_green_blob.glb",
                pos: (-16.0, -23.5),
                scale: 1.3,
                yaw_deg: 135.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Blob épineux",
                file: "monster_green_spiky_blob.glb",
                pos: (-12.0, -23.5),
                scale: 0.9,
                yaw_deg: 180.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Hywirl",
                file: "monster_hywirl.glb",
                pos: (-8.0, -23.5),
                scale: 0.75,
                yaw_deg: 225.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Monkroose",
                file: "monster_monkroose.glb",
                pos: (-4.0, -23.5),
                scale: 0.9,
                yaw_deg: 270.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Mushnub",
                file: "monster_mushnub.glb",
                pos: (0.0, -23.5),
                scale: 1.0,
                yaw_deg: 315.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Mushnub évolué",
                file: "monster_mushnub_evolved.glb",
                pos: (4.0, -23.5),
                scale: 0.9,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Roi champignon",
                file: "monster_mushroom_king.glb",
                pos: (8.0, -23.5),
                scale: 0.9,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Ninja",
                file: "monster_ninja.glb",
                pos: (12.0, -23.5),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Ninja 2",
                file: "monster_ninja_b.glb",
                pos: (16.0, -23.5),
                scale: 0.9,
                yaw_deg: 135.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Orc",
                file: "monster_orc.glb",
                pos: (-16.0, -20.0),
                scale: 0.9,
                yaw_deg: 180.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Orc ennemi",
                file: "monster_orc_enemy.glb",
                pos: (-12.0, -20.0),
                scale: 1.0,
                yaw_deg: 225.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Pigeon monstre",
                file: "monster_pigeon.glb",
                pos: (-8.0, -20.0),
                scale: 1.3,
                yaw_deg: 270.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Blob rose",
                file: "monster_pink_blob.glb",
                pos: (-4.0, -20.0),
                scale: 1.3,
                yaw_deg: 315.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Squidle",
                file: "monster_squidle.glb",
                pos: (0.0, -20.0),
                scale: 0.75,
                yaw_deg: 0.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Tribal",
                file: "monster_tribal.glb",
                pos: (4.0, -20.0),
                scale: 0.75,
                yaw_deg: 45.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Sorcier",
                file: "monster_wizard.glb",
                pos: (8.0, -20.0),
                scale: 1.0,
                yaw_deg: 90.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Yeti",
                file: "monster_yeti.glb",
                pos: (12.0, -20.0),
                scale: 1.0,
                yaw_deg: 135.0,
                solide: false,
                anim: Some("Idle"),
            },
            DemoDecor {
                name: "Monstre Yeti 2",
                file: "monster_yeti_b.glb",
                pos: (16.0, -20.0),
                scale: 0.9,
                yaw_deg: 180.0,
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
        for spec in NATURE_DECOR
            .iter()
            .chain(VILLAGE_PROPS.iter())
            .chain(MONSTER_DECOR.iter())
        {
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
            // Chemin du pont nord + son prolongement « sentier du
            // promontoire » (même bande z, x poussé de 6 à 34).
            (-26.0, -10.9, 34.0, -9.1),
            (-5.6, 14.0, -3.4, 26.0), // chemin des rizières
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
            .chain(VILLAGE_PROPS.iter())
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

        // Variante « en bosquets » de `scatter` : au lieu d'un tirage uniforme
        // dans tout le rectangle (visuellement un peu quadrillé malgré le
        // RNG), tire d'abord `n_clusters` centres, puis disperse
        // `per_cluster.0..per_cluster.1` instances autour de chacun dans un
        // disque de rayon `cluster_radius` — tirage en AIRE uniforme
        // (`r = radius * sqrt(u)`, pas `r = radius * u` qui sur-représenterait
        // le centre) pour un semis de bosquet crédible, façon sous-bois réel.
        // Mêmes règles de rejet que `scatter` (exclusions, spawns, solides).
        #[allow(clippy::too_many_arguments)]
        fn scatter_clustered(
            rng: &mut crate::runtime::rng::Rng,
            poser: &mut Poser<'_>,
            solid_spots: &mut Vec<(f32, f32)>,
            spawns: &[(f32, f32)],
            exclusions: &[&[Rect]],
            files: &[&'static str],
            prefix: &str,
            rect: Rect,
            n_clusters: usize,
            per_cluster: (usize, usize),
            cluster_radius: f32,
            scale: (f32, f32),
            solide: bool,
        ) {
            let mut poses = 0usize;
            for c in 0..n_clusters {
                // Centre du bosquet : rejeté s'il tombe dans une exclusion (le
                // bosquet entier resterait alors coincé contre une zone
                // aménagée) — pas de contrainte spawn/solide ici, seules les
                // instances individuelles comptent pour ces règles.
                let mut center = None;
                for _ in 0..20 {
                    let cx = rng.next_range(rect.0, rect.2);
                    let cz = rng.next_range(rect.1, rect.3);
                    if !exclusions
                        .iter()
                        .flat_map(|g| g.iter())
                        .any(|&(x0, z0, x1, z1)| cx >= x0 && cx <= x1 && cz >= z0 && cz <= z1)
                    {
                        center = Some((cx, cz));
                        break;
                    }
                }
                let Some((cx, cz)) = center else { continue };
                let n = per_cluster.0
                    + if per_cluster.1 > per_cluster.0 {
                        rng.next_below(per_cluster.1 - per_cluster.0 + 1)
                    } else {
                        0
                    };
                let mut placed_in_cluster = 0usize;
                let mut essais = 0usize;
                while placed_in_cluster < n && essais < n * 40 {
                    essais += 1;
                    let r = cluster_radius * rng.next_range(0.0, 1.0).sqrt();
                    let a = rng.next_range(0.0, std::f32::consts::TAU);
                    let x = cx + r * a.cos();
                    let z = cz + r * a.sin();
                    if x < rect.0 || x > rect.2 || z < rect.1 || z > rect.3 {
                        continue;
                    }
                    if exclusions
                        .iter()
                        .flat_map(|g| g.iter())
                        .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                    {
                        continue;
                    }
                    let d2 = |(sx, sz): (f32, f32)| (sx - x) * (sx - x) + (sz - z) * (sz - z);
                    if spawns.iter().any(|&s| d2(s) < 16.0) {
                        continue;
                    }
                    if solide && solid_spots.iter().any(|&s| d2(s) < 6.25) {
                        continue;
                    }
                    placed_in_cluster += 1;
                    poses += 1;
                    let file = files[rng.next_below(files.len())];
                    let s = rng.next_range(scale.0, scale.1);
                    let yaw = rng.next_range(0.0, 360.0);
                    poser(
                        &format!("{prefix} {c}-{poses}"),
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
        }

        // Forêt dense du nord-est (évite les deux clairières) : feuillus mêlés
        // aux sapins, souches, sous-bois traversable.
        let foret: Rect = (8.0, -34.0, 34.0, -8.0);
        let excl_foret: &[&[Rect]] = &[EXCL_EAU_ROUTES, EXCL_ZONES_AMENAGEES, EXCL_CLAIRIERES];
        let excl_std: &[&[Rect]] = &[EXCL_EAU_ROUTES, EXCL_ZONES_AMENAGEES];

        // Quatre petites « Halte » à mi-distance (10-20 m du spawn, un point
        // par biome principal) : le regard n'avait aucun échelon entre le vide
        // proche et le mur lointain du biome. Chaque halte = un solide (arbre/
        // rocher/souche) + un compagnon non solide tout proche (la contrainte
        // de 2 m ne s'applique qu'aux solides entre eux, cf.
        // `mmorpg_solid_decor_stays_inside_and_spaced`) ; positions choisies à
        // ≥ 4 m de tout spawn de créature et hors zones aménagées. Posées AVANT
        // tout le scatter procédural ci-dessous (leurs positions rejoignent
        // `solid_spots` immédiatement) pour que forêt/prairie/lisières les
        // évitent d'elles-mêmes plutôt que de risquer une fusion visuelle
        // découverte après coup par `mmorpg_solid_decor_stays_inside_and_spaced`.
        struct Halte {
            name: &'static str,
            file: &'static str,
            pos: (f32, f32),
            scale: f32,
            yaw_deg: f32,
            solide: bool,
        }
        const MMORPG_HALTES: &[Halte] = &[
            // Vers la forêt nord-est.
            Halte {
                name: "Halte NE rocher",
                file: "nature_rock.glb",
                pos: (7.5, -11.5),
                scale: 0.8,
                yaw_deg: 35.0,
                solide: true,
            },
            Halte {
                name: "Halte NE fougère",
                file: "nature_fern.glb",
                pos: (8.3, -10.5),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
            // Vers le lac et les rivières (ouest).
            Halte {
                name: "Halte Ouest souche",
                file: "nature_stump.glb",
                pos: (-11.5, -6.0),
                scale: 1.0,
                yaw_deg: 0.0,
                solide: true,
            },
            Halte {
                name: "Halte Ouest fleurs",
                file: "nature_daisies.glb",
                pos: (-10.3, -5.3),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
            // Vers les rizières (sud-ouest).
            Halte {
                name: "Halte Sud-Ouest arbre",
                file: "nature_willow.glb",
                pos: (-6.0, 12.0),
                scale: 0.9,
                yaw_deg: 200.0,
                solide: true,
            },
            Halte {
                name: "Halte Sud-Ouest fleurs",
                file: "nature_lavender.glb",
                pos: (-4.8, 11.3),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
            // Vers le hameau et le promontoire (est).
            Halte {
                name: "Halte Est rocher",
                file: "nature_rock.glb",
                // Pas (13, 4) : à 1,4 m de « Bannière » (landmark posé à la
                // main du hameau, cf. `NATURE_DECOR`) — constaté par
                // `mmorpg_solid_decor_stays_inside_and_spaced`. Décalé plus au
                // nord, dans la bande étroite (z 1..4) qui échappe à la fois
                // au col venté (z ≤ 1) et au hameau (z ≥ 4).
                pos: (15.0, 2.5),
                scale: 0.85,
                yaw_deg: 150.0,
                solide: true,
            },
            Halte {
                name: "Halte Est fleurs",
                file: "nature_sunflowers.glb",
                pos: (15.8, 1.7),
                scale: 1.1,
                yaw_deg: 0.0,
                solide: false,
            },
        ];
        for h in MMORPG_HALTES {
            poser(
                h.name, h.file, h.pos.0, h.pos.1, h.scale, h.yaw_deg, h.solide, None,
            );
            if h.solide {
                solid_spots.push(h.pos);
            }
        }

        // Variété de la lisière forêt/prairie (bord sud-ouest, x 9..15,5, z
        // -17,5..-12 — le segment de forêt le plus proche du regard du
        // joueur depuis la prairie) : le remplissage `Arbre`/`Sapin`
        // ci-dessous y répète surtout arbres/arbres2/sapins/sapins2, un mur
        // de silhouettes similaires constaté sur une capture en jeu.
        // Positions FIXES (pas de tirage RNG, contrairement à `scatter`) :
        // cette portion de `foret` est déjà saturée à ~69 % par le
        // remplissage suivant (cf. son propre commentaire) — un tirage
        // aléatoire y échoue près de 100 % du temps (constaté : 8 demandés,
        // 0 placés). Réservées dans `solid_spots` avant ledit remplissage,
        // qui les évite de lui-même. Même préfixe « Arbre exotique » que le
        // bosquet A) plus bas : aucun nouveau préfixe à ajouter à l'outil de
        // synchro. Espacées ≥ 2,5 m entre elles et du Halte NE tout proche,
        // ≥ 4 m des spawns des créatures dont le territoire touche ce coin
        // (RAY_DIST 3,5 m + marge, cf. `mmorpg_demo_contains_walkable_nature_decor`).
        for (name, file, x, z) in [
            (
                "Arbre exotique bouleau de lisière",
                "nature_birch.glb",
                9.0,
                -15.0,
            ),
            (
                "Arbre exotique chêne de lisière",
                "nature_oak.glb",
                13.5,
                -15.0,
            ),
            (
                "Arbre exotique érable de lisière",
                "nature_maple_autumn.glb",
                11.0,
                -17.5,
            ),
            (
                "Arbre exotique ginkgo de lisière",
                "nature_ginkgo.glb",
                15.5,
                -12.0,
            ),
        ] {
            poser(name, file, x, z, 1.0, 0.0, true, None);
            solid_spots.push((x, z));
        }
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_tree.glb", "nature_tree2.glb"],
            "Arbre",
            foret,
            // 22 → 28 : le hameau fortifié (Medieval Village Pack) mord sur ce
            // rectangle et son décor solide fait échouer plus de tirages
            // (rejection sampling à ≥ 2,5 m d'un autre solide) — on compense
            // pour garder ≥ 30 arbres/sapins (cf. test de densité).
            // 28 → 31 : les spawns des créatures 21-26 (éléphanteau + pack
            // savane & terreurs) décalent le flux RNG du scatter — marge pour
            // garder l'invariant ≥ 30 sans rejouer cette compensation à chaque
            // nouveau spawn.
            // 31 → 40 : les 4 arbres de lisière + le rocher du Halte NE
            // ci-dessus réservent désormais des places dans `foret` avant ce
            // tirage (rejection sampling à ≥ 2,5 m), qui en trouve donc moins
            // — reconstaté par comptage direct (25 arbres pour n=31) ; 40
            // restaure une marge confortable au-dessus du minimum testé.
            40,
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
            // 15 → 20 : même compensation que ci-dessus pour `Arbre` (places
            // réservées par la lisière/Halte), constaté par comptage direct.
            20,
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
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_bush.glb"],
            "Buisson",
            foret,
            4,
            (2, 4),
            2.5,
            (0.9, 1.4),
            false,
        );
        // Couverture d'herbe/fougères du sous-bois : non solide, coût nul sur
        // le budget physique (aucun collider), rendu batché — la densité
        // vient de là plutôt que de multiplier les solides.
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &["nature_grass_tuft.glb", "nature_fern.glb"],
            "Sous-bois",
            foret,
            18,
            (4, 8),
            1.8,
            (0.85, 1.3),
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
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_bush.glb"],
            "Buisson fleuri",
            prairie,
            3,
            (2, 3),
            2.2,
            (0.9, 1.3),
            false,
        );
        // Herbe basse de la prairie centrale : même logique de bosquets que
        // le sous-bois, teinte plus claire (grass_tuft seul, pas de fougère
        // sombre de forêt).
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &["nature_grass_tuft.glb"],
            "Herbe",
            prairie,
            10,
            (6, 10),
            2.0,
            (0.85, 1.3),
            false,
        );

        // --- Élargissement de la prairie centrale ---------------------------
        // Le rect `prairie` ci-dessus (-10,-12)-(8,8) concentre tout le semis
        // près du spawn ; entre lui et les quatre « Repère » (±15, ±15) qui
        // bordent la prairie restait un large anneau d'herbe nue (constaté sur
        // une capture en jeu : grand aplat vert vide). Comble cet anneau sans
        // toucher aux biomes voisins (forêt/hameau/lac/rizières/promontoire),
        // ni au rect déjà dense, ni aux abords du spawn joueur et des Repère
        // (rien ne doit gêner la vue au tout premier coup d'œil).
        const PRAIRIE_DEJA_SEMEE: &[Rect] = &[(-10.0, -12.0, 8.0, 8.0)];
        // Même bornes que le rect `foret` : la prairie élargie ne doit jamais
        // mordre sur la forêt (son propre semis gère sa densité).
        const EXCL_FORET_ZONE: &[Rect] = &[(8.0, -34.0, 34.0, -8.0)];
        // Dégagement (6×6 m) autour de chacun des quatre Repère.
        const EXCL_REPERES: &[Rect] = &[
            (-18.0, -18.0, -12.0, -12.0),
            (12.0, -18.0, 18.0, -12.0),
            (-18.0, 12.0, -12.0, 18.0),
            (12.0, 12.0, 18.0, 18.0),
        ];
        // Dégagement (8×8 m) autour du spawn du joueur (0, 0).
        const EXCL_SPAWN_JOUEUR: &[Rect] = &[(-4.0, -4.0, 4.0, 4.0)];
        let excl_prairie_large: &[&[Rect]] = &[
            EXCL_EAU_ROUTES,
            EXCL_ZONES_AMENAGEES,
            EXCL_FORET_ZONE,
            PRAIRIE_DEJA_SEMEE,
            EXCL_REPERES,
            EXCL_SPAWN_JOUEUR,
        ];
        let prairie_large: Rect = (-16.0, -16.0, 16.0, 16.0);

        // Touffes/fougères éparses (non solides) : le gros de la densité
        // visuelle, sans peser sur la broad-phase des raycasts.
        scatter_clustered(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &["nature_grass_tuft.glb", "nature_fern.glb"],
            "Prairie centrale herbe",
            prairie_large,
            8,
            (3, 6),
            2.2,
            (0.85, 1.25),
            false,
        );
        // Fleurs des prés (non solides), variété différente de celles déjà
        // semées dans le rect dense pour ne pas juste répéter le motif.
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &[
                "nature_daisies.glb",
                "nature_irises.glb",
                "nature_lavender.glb",
            ],
            "Prairie centrale fleur",
            prairie_large,
            10,
            (0.85, 1.3),
            false,
        );
        // Petits rochers isolés (solides, échelle réduite pour rester
        // discrets — pas l'anneau du promontoire).
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &["nature_rock.glb"],
            "Prairie centrale rocher",
            prairie_large,
            4,
            (0.5, 0.75),
            true,
        );
        // Un ou deux arbres isolés (solides) : jamais un bosquet, juste de
        // quoi casser la platitude du grand aplat vert.
        scatter(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_prairie_large,
            &["nature_oak.glb", "nature_tree.glb"],
            "Prairie centrale arbre isolé",
            prairie_large,
            2,
            (0.8, 1.0),
            true,
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

        // Roseaux/nénuphars systématiques le long des 4 berges (en plus des
        // quelques instances posées à la main dans `NATURE_DECOR`, gardées
        // telles quelles) : un point tiré sur le PÉRIMÈTRE de chaque rect
        // d'eau (`EXCL_EAU_ROUTES[..4]`, les 4 vrais plans d'eau — pas la
        // berge de sable ni les routes), décalé perpendiculairement côté
        // terre pour les roseaux, côté eau pour les nénuphars (flottent, non
        // solides dans les deux cas). Bien plus systématique que les 7
        // instances isolées d'origine.
        for &(x0, z0, x1, z1) in &EXCL_EAU_ROUTES[..4] {
            for i in 0..7 {
                let side = rng.next_below(4);
                let along = rng.next_range(0.0, 1.0);
                let (bx, bz, nx, nz): (f32, f32, f32, f32) = match side {
                    0 => (x0 + along * (x1 - x0), z0, 0.0, -1.0), // nord
                    1 => (x0 + along * (x1 - x0), z1, 0.0, 1.0),  // sud
                    2 => (x0, z0 + along * (z1 - z0), -1.0, 0.0), // ouest
                    _ => (x1, z0 + along * (z1 - z0), 1.0, 0.0),  // est
                };
                let is_reed = i % 2 == 0;
                let offset = rng.next_range(0.3, 0.8);
                let (px, pz) = if is_reed {
                    (bx + nx * offset, bz + nz * offset)
                } else {
                    (bx - nx * offset, bz - nz * offset)
                };
                if spawns.iter().any(|&(sx, sz)| {
                    let dx = sx - px;
                    let dz = sz - pz;
                    dx * dx + dz * dz < 16.0
                }) {
                    continue;
                }
                let file = if is_reed {
                    "nature_reeds.glb"
                } else {
                    "nature_lily.glb"
                };
                let scale = rng.next_range(0.85, 1.15);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    &format!(
                        "Berge {} {i}",
                        if is_reed { "Roseaux" } else { "Nénuphars" }
                    ),
                    file,
                    px,
                    pz,
                    scale,
                    yaw,
                    false,
                    None,
                );
            }
        }

        // --- Flore complémentaire + objets décoratifs (packs générés Blender
        //     headless, gen_creature_pack*.py / gen_nature_pack*.py) ------------
        // Contrairement à `scatter`/`scatter_clustered` (tirage aléatoire avec
        // remise dans une liste de fichiers), chaque fichier ci-dessous doit
        // apparaître AU MOINS une fois dans la scène (sinon un asset généré
        // resterait inutilisé) : `scatter_each` place chaque fichier de la liste
        // exactement une fois, avec le même rejet de zones aménagées / spawns /
        // chevauchement de solides que `scatter`, mais sans tirage avec remise.
        #[allow(clippy::too_many_arguments)]
        fn scatter_each(
            rng: &mut crate::runtime::rng::Rng,
            poser: &mut Poser<'_>,
            solid_spots: &mut Vec<(f32, f32)>,
            spawns: &[(f32, f32)],
            exclusions: &[&[Rect]],
            files: &[&'static str],
            prefix: &str,
            rect: Rect,
            scale: (f32, f32),
            solide: bool,
        ) {
            for &file in files {
                let mut placed = false;
                for _ in 0..4000 {
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
                        continue;
                    }
                    if solide && solid_spots.iter().any(|&s| d2(s) < 6.25) {
                        continue;
                    }
                    let s = rng.next_range(scale.0, scale.1);
                    let yaw = rng.next_range(0.0, 360.0);
                    let short = file
                        .trim_start_matches("nature_")
                        .trim_start_matches("item_")
                        .trim_end_matches(".glb");
                    poser(
                        &format!("{prefix} {short}"),
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
                    placed = true;
                    break;
                }
                if !placed {
                    log::error!("« {prefix} » : impossible de placer {file} sans chevauchement");
                }
            }
        }

        // A) Arbres exotiques (packs faune d'Asie / fantastique / marin, décor
        //    végétal) : forêt nord-est, solides comme les arbres existants.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &[
                "nature_apple_tree.glb",
                "nature_bamboo.glb",
                "nature_birch.glb",
                "nature_cherry_blossom.glb",
                "nature_cypress.glb",
                "nature_dead_tree.glb",
                "nature_ginkgo.glb",
                "nature_hazel.glb",
                "nature_magnolia.glb",
                "nature_maple_autumn.glb",
                "nature_oak.glb",
                "nature_olive.glb",
                "nature_palm.glb",
                "nature_pine_parasol.glb",
                "nature_plum.glb",
                "nature_poplar.glb",
                "nature_sequoia.glb",
                "nature_tree_windswept.glb",
            ],
            "Arbre exotique",
            // Pas `foret` (déjà saturé à ~69 % de sa surface utile par les
            // arbres/sapins/souches du scatter existant, au-delà du seuil de
            // remplissage aléatoire (« jamming ») pour un rejet à ≥ 2,5 m —
            // 300 tirages par fichier n'y trouvaient plus jamais de place,
            // constaté par comptage direct). Bosquet complémentaire au
            // sud-est de la forêt, hors rizières (x max 9) et hors hameau/
            // promontoire (`EXCL_ZONES_AMENAGEES`), quasi vierge de décor
            // solide.
            (9.0, 20.0, 35.5, 35.5),
            (0.85, 1.2),
            true,
        );
        // B) Mobilier villageois (fontaine, meule, puits à poulie, balançoire,
        //    topiaire, tonnelle glycine…) : posé dans le hameau lui-même (zone
        //    aménagée non exclue ici), à ≥ 2,5 m de tout autre solide déjà posé
        //    à la main dans `NATURE_DECOR`/`VILLAGE_PROPS`.
        let excl_hameau_only: &[&[Rect]] = &[EXCL_EAU_ROUTES];
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "nature_fountain.glb",
                "nature_well_pulley.glb",
                "nature_grindstone.glb",
                "nature_hay_roller.glb",
                "nature_potters_wheel.glb",
                "nature_wheelbarrow.glb",
                "nature_swing_bench.glb",
                "nature_topiary.glb",
                "nature_vine_trellis.glb",
                "nature_wisteria_arch.glb",
                "nature_sundial.glb",
                "nature_birdhouse.glb",
            ],
            "Mobilier du hameau",
            (1.0, 2.0, 21.0, 23.0),
            (0.9, 1.1),
            true,
        );
        // C) Rochers moussus de bord de lac (moss_boulder/mossy_log) : rive
        //    ouest du lac. Pas le rect exact de la « Berge du lac » (aplat
        //    sable) : ce rect est LUI-MÊME une entrée d'`EXCL_EAU_ROUTES`, donc
        //    100 % des tirages y étaient rejetés (constaté : 0/2 placés).
        //    Juste à l'ouest, hors de tout aplat eau/route/berge.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &["nature_moss_boulder.glb", "nature_mossy_log.glb"],
            "Rocher moussu",
            (-30.0, -4.0, -26.0, 8.0),
            (0.9, 1.2),
            true,
        );
        // D) Sous-bois exotique (non solide : champignons, houx, ronces,
        //    girouettes/oriflammes de prière plantées au sol).
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_foret,
            &[
                "nature_giant_mushroom.glb",
                "nature_mushrooms.glb",
                "nature_holly.glb",
                "nature_bramble.glb",
                "nature_mast_flag.glb",
                "nature_prayer_flags.glb",
            ],
            "Sous-bois exotique",
            foret,
            (0.85, 1.2),
            false,
        );
        // E) Fleurs des prés (non solides) : prairie centrale.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_std,
            &[
                "nature_daisies.glb",
                "nature_irises.glb",
                "nature_lavender.glb",
                "nature_sunflowers.glb",
                "nature_sunflowers_sway.glb",
                "nature_thistle.glb",
                "nature_windsock.glb",
            ],
            "Fleur des prés",
            prairie,
            (0.85, 1.3),
            false,
        );
        // F) Cultures complémentaires (non solides) : bande des rizières,
        //    exclusions restreintes à l'eau/aux routes (comme le riz existant)
        //    pour pouvoir tomber DANS les bassins.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_riz,
            &[
                "nature_cabbages.glb",
                "nature_carrots.glb",
                "nature_corn.glb",
                "nature_pumpkins.glb",
                "nature_tomatoes.glb",
                "nature_wheat.glb",
                "nature_wheat_sway.glb",
            ],
            "Culture",
            (-11.0, 23.0, 9.0, 35.0),
            (0.85, 1.25),
            false,
        );
        // G) Rives du lac et des rivières (non solides) : roseaux/nénuphars
        //    existants complétés par saules, bambou et barque flottante.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "nature_cattails.glb",
                "nature_reeds_sway.glb",
                "nature_willow.glb",
                "nature_willow_sway.glb",
                "nature_boat_bob.glb",
                "nature_bamboo_sway.glb",
            ],
            "Rive du lac",
            (-30.0, -14.0, -10.0, 16.0),
            (0.85, 1.2),
            false,
        );
        // H) Petit décor du hameau (non solide) : lanterne suspendue, buisson à
        //    baies.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &["nature_lantern_hanging.glb", "nature_berry_bush.glb"],
            "Décor du hameau",
            (1.0, 2.0, 21.0, 23.0),
            (0.9, 1.1),
            false,
        );

        // I) Objets décoratifs (`item_*`, packs générés) : PURE décor visuel,
        //    aucune mécanique de ramassage (pas d'`ItemPickup`, à ne pas
        //    confondre avec `MMORPG_ITEMS` plus bas) — regroupés en petites
        //    scènes cohérentes posées au sol près du hameau, non solides,
        //    petite échelle.
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_axe.glb",
                "item_bow.glb",
                "item_hammer.glb",
                "item_shield.glb",
                "item_sword.glb",
                "item_ball.glb",
                "item_bomb.glb",
            ],
            "Établi d'armes",
            (13.0, 5.0, 16.0, 7.0),
            (0.3, 0.4),
            false,
        );
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_apple.glb",
                "item_bread.glb",
                "item_carrot.glb",
                "item_cheese.glb",
                "item_egg.glb",
                "item_fish.glb",
                "item_meat.glb",
                "item_mushroom.glb",
                "item_berry.glb",
            ],
            // Pas « Étal du marché » : déjà pris par `VILLAGE_PROPS` (« Étal du
            // marché 1/2 », `hamlet_market_stand_*`) — un préfixe partagé
            // ferait retirer/réinjecter ces deux landmarks par erreur dans
            // l'outil de synchro du décor ambiant (cf. `AMBIENT_DECOR_PREFIXES`
            // dans `scene::mod`).
            "Étal des vivres",
            (16.5, 7.5, 19.5, 9.5),
            (0.3, 0.4),
            false,
        );
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_coin.glb",
                "item_gem.glb",
                "item_crown.glb",
                "item_ring.glb",
                "item_star.glb",
            ],
            "Coin trésor",
            (6.5, 5.0, 9.0, 6.5),
            (0.3, 0.4),
            false,
        );
        scatter_each(
            &mut rng,
            &mut poser,
            &mut solid_spots,
            &spawns,
            excl_hameau_only,
            &[
                "item_potion.glb",
                "item_mana.glb",
                "item_scroll.glb",
                "item_book.glb",
                "item_feather.glb",
                "item_heart.glb",
                "item_key.glb",
                "item_lantern.glb",
                "item_pouch.glb",
            ],
            "Table d'apothicaire",
            (3.0, 5.5, 6.0, 6.5),
            (0.3, 0.4),
            false,
        );

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

        // Les Braises (GDD §2.1) sont la fiction du jeu : « c'est [le feu
        // communal] qui attire les hordes ». La charte (§10.1 « au centre,
        // les braises ; au loin, le danger », §10.2 orange = système
        // feu/joueur) exige que ce soit le point chaud/saturé le plus
        // lisible de la carte — jusqu'ici posé comme n'importe quel décor
        // inerte (pas d'émissif). Marquage a posteriori (pas de champ
        // couleur/émissif sur `DemoDecor`, partagé par ~150 entrées neutres)
        // plutôt qu'une extension de la table pour deux objets seulement.
        for name in ["Feu du hameau", "Feu de camp"] {
            if let Some(feu) = objects.iter_mut().find(|o| o.name == name) {
                feu.emissive = 1.2;
            }
        }

        // Convoi (GDD §4, mode Escorte) : jusqu'ici absent de la scène réseau
        // réelle (`mmorpg_demo`, embarquée via `player_scene.json`) — seule
        // `Scene::escorte_demo()` (solo) en avait un. Conséquence mécanique
        // vérifiée (Phase L, `sprintreflecion.md`) : un salon réseau qui
        // choisit `RoundObjective::Escorte` ne se terminait jamais
        // (`AppState::update_escorte`/`is_convoy_destroyed` retournent tôt
        // sans rien faire quand aucun objet `convoy` n'existe, cf.
        // `src/app/combat.rs`/`src/app/health.rs`). Même modèle
        // (`nature_cart.glb`) et mêmes composants (`Combat`/`Convoy`) que
        // `escorte_demo`, positionné sur la route principale (bande z
        // 12.3–15.7, exclue du scatter procédural ci-dessus — cf.
        // `EXCL_EAU_ROUTES`), entre le col venté et le hameau : x -18 → -2,
        // à l'écart des collines de l'Ouest (x < -27) et des bâtiments du
        // hameau (x > 2).
        {
            let convoi_mesh = import_single_model(&mut imported, "nature_cart.glb", MeshKind::Cube);
            let mut convoi = demo_obj(
                "Convoi — chariot de braises",
                convoi_mesh,
                Vec3::new(-18.0, 0.0, 14.0),
            );
            convoi.transform.rotation = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
            convoi.emissive = 0.3;
            convoi.combat = Some(Combat {
                attackable: true,
                hp: 8,
                ..Default::default()
            });
            convoi.convoy = Some(Convoy {
                destination: Vec3::new(-2.0, 0.0, 14.0),
                speed: 1.2,
            });
            objects.push(convoi);
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
                // Les Braises (GDD §2.1/§10.1) : le feu communal du hameau
                // (forge/scierie, x≈28/z≈-29) est hors de portée des deux
                // lampes ci-dessus — il n'avait aucune source de lumière
                // propre alors qu'il est la fiction centrale du jeu.
                PointLight {
                    position: [28.0, 1.2, -29.0],
                    color: [1.0, 0.55, 0.15],
                    intensity: 1.6,
                    range: 14.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [19.0, 1.0, -6.0],
                    color: [1.0, 0.6, 0.2],
                    intensity: 0.9,
                    range: 8.0,
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

    /// Démo « Hameau fortifié » (GDD §7 « le hameau est du gameplay », §7.3
    /// « la vie du hameau », §5.4 archétypes de créatures, §10 direction
    /// artistique) — prototypée visuellement dans Blender avant intégration
    /// ici, patron identique à `mmorpg_demo` (tables de données + closures/fn
    /// locales de pose, pas de JSON écrit à la main) mais géométrie
    /// entièrement différente : fort carré 48×48 (remparts, 4 portes + 2
    /// brèches diagonales, chemin de ronde), place centrale, anneau de 16
    /// spawns joueur, 4 îlots bâtis, artisanat, marché, lanternes/bannières,
    /// rivière/lac hors les murs, forêt en anneau avec couloirs dégagés dans
    /// l'axe des 6 lisières de spawn de vagues, faune variée.
    ///
    /// Créatures : reprises telles quelles de `mmorpg_demo()` (mêmes
    /// composants — mesh/physics/script/trigger/collision_layer — c'est ce
    /// que compare le garde-fou `the_embedded_scene_creatures_match_the_demo`,
    /// **par nom**, pas par position), spawns conservés à l'identique : aucune
    /// vague/créature existante ne disparaît silencieusement (cf. la consigne
    /// d'intégration). Comme le nouveau fort occupe globalement le même ordre
    /// de grandeur que l'ancienne arène (le rayon de la forêt va jusqu'à 70 m,
    /// l'ancienne carte faisait 72 m de côté), les spawns d'origine restent
    /// dans une zone plausible (forêt/berge) plutôt que dans un mur neuf.
    ///
    /// Écarts assumés par rapport au prototype Blender (documentés dans le
    /// rapport d'intégration, pas de garde-fou automatisé ne les couvre) :
    /// - Les « marqueurs, pas des meshes » (lisières de vague, anneau de spawn
    ///   joueur) sont de minuscules cylindres non solides : le moteur n'a pas
    ///   de type « Empty » distinct d'un mesh (cf. `MeshKind`), c'est
    ///   l'équivalent le plus proche.
    /// - Chaque cour est fermée par exactement 3 panneaux `hamlet_fence.glb`
    ///   à l'échelle native (~3 m), un par côté bâti — pas une rangée de
    ///   panneaux jointifs : au sens strict ça laisse un jour entre panneau et
    ///   coin de cour plutôt qu'un mur continu, mais respecte la spec « 3
    ///   pans, une ouverture côté place ».
    /// - Pas de collision d'eau dédiée (pas de « Mur d'eau » invisible comme
    ///   dans `mmorpg_demo`) : la rivière/le lac ne sont que des aplats
    ///   visuels non solides. Aucun garde-fou de cette nouvelle démo n'exige
    ///   un blocage de baignade (contrairement à `mmorpg_demo`) ; à ajouter
    ///   si un jour cette carte a ses propres tests d'étanchéité.
    pub fn hameau_gdd_demo() -> Self {
        const HALF: f32 = 24.0; // fort 48×48, centré à l'origine

        fn at(radius: f32, az_deg: f32) -> (f32, f32) {
            // Convention du fichier : -Z = Nord, +X = Est (cf. « Mur Nord » de
            // `mmorpg_demo`, posé à z = -half). az_deg = 0 ⇒ Nord, sens horaire.
            let r = az_deg.to_radians();
            (radius * r.sin(), -radius * r.cos())
        }

        fn in_corridor(az_deg: f32) -> bool {
            // Couloirs dégagés (±13°) dans l'axe des 6 lisières de spawn de
            // vagues (4 portes cardinales + 2 brèches diagonales) : l'arrivée
            // d'une vague doit rester visible depuis le fort, pas masquée par
            // un mur d'arbres semé juste devant.
            const AZIMUTHS: [f32; 6] = [0.0, 45.0, 90.0, 180.0, 225.0, 270.0];
            AZIMUTHS.iter().any(|&a| {
                let d = (az_deg - a + 180.0).rem_euclid(360.0) - 180.0;
                d.abs() < 13.0
            })
        }

        #[allow(clippy::too_many_arguments)]
        fn poser(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            name: &str,
            file: &'static str,
            x: f32,
            z: f32,
            scale: f32,
            yaw_deg: f32,
            solide: bool,
        ) {
            let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
            let mesh_index = match imported.iter().position(|m| m.path == path) {
                Some(i) => i as u32,
                None => match crate::scene::import::load_gltf(&path) {
                    Ok((data, aabb_min, aabb_max)) => {
                        let mut mesh = ImportedMesh {
                            path,
                            data,
                            aabb_min,
                            aabb_max,
                            ..Default::default()
                        };
                        mesh.load_skinning();
                        imported.push(mesh);
                        (imported.len() - 1) as u32
                    }
                    Err(e) => {
                        log::error!("{name} ({file}) : {e}");
                        return;
                    }
                },
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
            objects.push(deco);
        }

        fn marker(objects: &mut Vec<SceneObject>, name: &str, x: f32, z: f32, color: [f32; 3]) {
            // Cf. la doc de fonction : substitut d'« Empty » (le moteur n'a
            // que des meshes) — petit disque non solide, ne bloque ni ne
            // gêne rien, juste un repère visuel/de conception.
            let mut m = demo_obj(name, MeshKind::Cylinder, Vec3::new(x, 0.05, z));
            m.transform = m.transform.with_scale(Vec3::new(0.4, 0.1, 0.4));
            m.physics = PhysicsKind::None;
            m.color = color;
            objects.push(m);
        }

        #[allow(clippy::too_many_arguments)]
        fn box_seg(
            objects: &mut Vec<SceneObject>,
            name: &str,
            x0: f32,
            x1: f32,
            z0: f32,
            z1: f32,
            y: f32,
            height: f32,
            min_thick: f32,
            color: [f32; 3],
        ) {
            let cx = (x0 + x1) * 0.5;
            let cz = (z0 + z1) * 0.5;
            let sx = (x1 - x0).abs().max(min_thick);
            let sz = (z1 - z0).abs().max(min_thick);
            let mut w = demo_obj(name, MeshKind::Cube, Vec3::new(cx, y, cz));
            w.transform = w.transform.with_scale(Vec3::new(sx, height, sz));
            w.physics = PhysicsKind::Static;
            w.color = color;
            objects.push(w);
        }

        #[allow(clippy::too_many_arguments)]
        fn aplat(
            objects: &mut Vec<SceneObject>,
            name: &str,
            cx: f32,
            cz: f32,
            sx: f32,
            sz: f32,
            y: f32,
            color: [f32; 3],
        ) {
            let mut o = demo_obj(name, MeshKind::Plane, Vec3::new(cx, y, cz));
            o.transform = o.transform.with_scale(Vec3::new(sx, 1.0, sz));
            o.physics = PhysicsKind::None;
            o.color = color;
            objects.push(o);
        }

        #[allow(clippy::too_many_arguments)]
        fn foret_scatter(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            rng: &mut crate::runtime::rng::Rng,
            r_in: f32,
            r_out: f32,
            excl: &[(f32, f32, f32, f32)],
            n: usize,
        ) {
            const FILES: [&str; 4] = [
                "nature_tree.glb",
                "nature_tree2.glb",
                "nature_pine.glb",
                "nature_pine2.glb",
            ];
            let mut poses = 0usize;
            let mut essais = 0usize;
            while poses < n && essais < n * 30 {
                essais += 1;
                let x = rng.next_range(-r_out, r_out);
                let z = rng.next_range(-r_out, r_out);
                let r = (x * x + z * z).sqrt();
                if r < r_in || r > r_out {
                    continue;
                }
                let az = x.atan2(-z).to_degrees().rem_euclid(360.0);
                if in_corridor(az) {
                    continue;
                }
                if excl
                    .iter()
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                {
                    continue;
                }
                poses += 1;
                let bush = rng.next_range(0.0, 1.0) < 0.15;
                let file = if bush {
                    "nature_bush.glb"
                } else {
                    FILES[rng.next_below(FILES.len())]
                };
                let scale = rng.next_range(0.9, 1.4);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    objects,
                    imported,
                    &format!(
                        "{} {poses}",
                        if bush {
                            "Buisson de forêt"
                        } else {
                            "Arbre de forêt"
                        }
                    ),
                    file,
                    x,
                    z,
                    scale,
                    yaw,
                    true,
                );
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn faune_scatter(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            rng: &mut crate::runtime::rng::Rng,
            r_in: f32,
            r_out: f32,
            excl: &[(f32, f32, f32, f32)],
            file: &'static str,
            prefix: &str,
            n: usize,
        ) {
            let mut poses = 0usize;
            let mut essais = 0usize;
            while poses < n && essais < n * 40 {
                essais += 1;
                let x = rng.next_range(-r_out, r_out);
                let z = rng.next_range(-r_out, r_out);
                let r = (x * x + z * z).sqrt();
                if r < r_in || r > r_out {
                    continue;
                }
                if excl
                    .iter()
                    .any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
                {
                    continue;
                }
                poses += 1;
                let scale = rng.next_range(0.85, 1.2);
                let yaw = rng.next_range(0.0, 360.0);
                poser(
                    objects,
                    imported,
                    &format!("{prefix} {poses}"),
                    file,
                    x,
                    z,
                    scale,
                    yaw,
                    false,
                );
            }
        }

        let mut sol = demo_obj("Sol", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol.transform.with_scale(Vec3::new(90.0, 1.0, 90.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.30, 0.40, 0.22];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 0.0));
        joueur.color = [0.95, 0.6, 0.25];
        joueur.tag = "joueur".into();
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.5,
            fire_button: "Feu".into(),
            weapon_button: "Arme".into(),
            heal_button: "Soin".into(),
            ..Default::default()
        });

        let mut objects = vec![sol, joueur];
        let mut imported: Vec<ImportedMesh> = Vec::new();

        // --- Créatures : reprises de `mmorpg_demo()`, cf. la doc de fonction.
        let base = Scene::mmorpg_demo();
        for c in base
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Créature"))
        {
            let mut c = c.clone();
            if let MeshKind::Imported(old_idx) = c.mesh {
                let path = base.imported[old_idx as usize].path.clone();
                let new_idx = match imported.iter().position(|m| m.path == path) {
                    Some(i) => i,
                    None => match crate::scene::import::load_gltf(&path) {
                        Ok((data, aabb_min, aabb_max)) => {
                            let mut mesh = ImportedMesh {
                                path,
                                data,
                                aabb_min,
                                aabb_max,
                                ..Default::default()
                            };
                            mesh.load_skinning();
                            imported.push(mesh);
                            imported.len() - 1
                        }
                        Err(e) => {
                            log::error!("Créature « {} » : {e}", c.name);
                            continue;
                        }
                    },
                };
                c.mesh = MeshKind::Imported(new_idx as u32);
            }
            objects.push(c);
        }

        // --- Remparts : 4 pans, porte principale (brèche 5 m) au milieu de
        // chacun, 2 brèches diagonales secondaires (coins Nord-Est/Sud-Ouest,
        // en ne construisant pas les 5 derniers mètres des deux pans qui s'y
        // rejoignent). Habillés avec le pack siège (creation3DBlendersuite.md
        // / docs/integration_siege_scene.md) plutôt que des `box_seg` plats — même
        // périmètre exact (HALF/GATE_HALF/TRIM inchangés, donc `in_corridor`
        // et les couloirs de vague restent valides), juste le rendu change.
        const GATE_HALF: f32 = 2.5;
        const TRIM: f32 = 5.0;
        const MODULE_LEN: f32 = 4.0; // largeur de siege_wall_segment.glb

        #[allow(clippy::too_many_arguments)]
        fn poser_scaled(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            name: &str,
            file: &'static str,
            x: f32,
            z: f32,
            scale: Vec3,
            yaw_deg: f32,
            solide: bool,
        ) {
            let path = format!("{}/assets/models/{}", env!("CARGO_MANIFEST_DIR"), file);
            let mesh_index = match imported.iter().position(|m| m.path == path) {
                Some(i) => i as u32,
                None => match crate::scene::import::load_gltf(&path) {
                    Ok((data, aabb_min, aabb_max)) => {
                        let mut mesh = ImportedMesh {
                            path,
                            data,
                            aabb_min,
                            aabb_max,
                            ..Default::default()
                        };
                        mesh.load_skinning();
                        imported.push(mesh);
                        (imported.len() - 1) as u32
                    }
                    Err(e) => {
                        log::error!("{name} ({file}) : {e}");
                        return;
                    }
                },
            };
            let mut deco = demo_obj(name, MeshKind::Imported(mesh_index), Vec3::new(x, 0.0, z));
            deco.transform = deco.transform.with_scale(scale);
            if yaw_deg != 0.0 {
                deco.transform.rotation = glam::Quat::from_rotation_y(yaw_deg.to_radians());
            }
            if solide {
                deco.physics = PhysicsKind::Static;
                deco.collider_shape = crate::runtime::physics::ColliderShape::TriMesh;
            }
            objects.push(deco);
        }

        // Répète siege_wall_segment.glb sur la longueur exacte d'un pan (mise
        // à l'échelle X non uniforme si la longueur ne divise pas rond par
        // MODULE_LEN — appliquée en espace local avant la rotation de yaw,
        // cf. convention `poser`/hamlet_common : échelle avant rotation).
        #[allow(clippy::too_many_arguments)]
        fn wall_run(
            objects: &mut Vec<SceneObject>,
            imported: &mut Vec<ImportedMesh>,
            label: &str,
            x0: f32,
            x1: f32,
            z0: f32,
            z1: f32,
            yaw_deg: f32,
        ) {
            let dx = x1 - x0;
            let dz = z1 - z0;
            let total = (dx * dx + dz * dz).sqrt();
            if total < 0.5 {
                return;
            }
            let n = (total / MODULE_LEN).round().max(1.0) as usize;
            let seg_len = total / n as f32;
            let x_scale = seg_len / MODULE_LEN;
            for i in 0..n {
                let t = (i as f32 + 0.5) / n as f32;
                let x = x0 + dx * t;
                let z = z0 + dz * t;
                poser_scaled(
                    objects,
                    imported,
                    &format!("{label} {}", i + 1),
                    "siege_wall_segment.glb",
                    x,
                    z,
                    Vec3::new(x_scale, 1.0, 1.0),
                    yaw_deg,
                    true,
                );
            }
        }

        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Nord Ouest",
            -HALF,
            -GATE_HALF,
            -HALF,
            -HALF,
            0.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Nord Est",
            GATE_HALF,
            HALF - TRIM,
            -HALF,
            -HALF,
            0.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Est Nord",
            HALF,
            HALF,
            -HALF + TRIM,
            -GATE_HALF,
            90.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Est Sud",
            HALF,
            HALF,
            GATE_HALF,
            HALF,
            90.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Sud Ouest",
            -HALF + TRIM,
            -GATE_HALF,
            HALF,
            HALF,
            0.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Sud Est",
            GATE_HALF,
            HALF,
            HALF,
            HALF,
            0.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Ouest Nord",
            -HALF,
            -HALF,
            -HALF,
            -GATE_HALF,
            90.0,
        );
        wall_run(
            &mut objects,
            &mut imported,
            "Rempart Ouest Sud",
            -HALF,
            -HALF,
            GATE_HALF,
            HALF - TRIM,
            90.0,
        );

        // Tours d'angle aux 2 coins pleins (Nord-Ouest/Sud-Est) ; les coins
        // Nord-Est/Sud-Ouest restent des brèches ouvertes (cf. TRIM ci-dessus).
        poser(
            &mut objects,
            &mut imported,
            "Tour Nord-Ouest",
            "siege_tower.glb",
            -HALF,
            -HALF,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tour Sud-Est",
            "siege_tower.glb",
            HALF,
            HALF,
            1.0,
            0.0,
            true,
        );

        // Portes : GATE_W (siege_gate_*) = 5.0 = 2×GATE_HALF, aucune
        // re-échelle nécessaire. Nord/Sud fermées, Est/Ouest embrasées — les
        // deux variantes doivent être présentes dans la carte (l'état réel
        // « embrasé » au signal de vague reste un chantier de gameplay
        // séparé, cf. docs/integration_siege_scene.md « hors scope »).
        poser(
            &mut objects,
            &mut imported,
            "Porte Nord",
            "siege_gate_closed.glb",
            0.0,
            -HALF,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Porte Sud",
            "siege_gate_closed.glb",
            0.0,
            HALF,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Porte Est",
            "siege_gate_burning.glb",
            HALF,
            0.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Porte Ouest",
            "siege_gate_burning.glb",
            -HALF,
            0.0,
            1.0,
            90.0,
            true,
        );

        // --- Chemin de ronde (hauteur ~2,2 m), longe l'intérieur des 4 murs,
        // mêmes brèches diagonales que les remparts (pas de coupure au droit
        // des portes : les défenseurs peuvent longer au-dessus de l'entrée).
        const RAMPART_R: f32 = HALF - 1.0;
        const RAMPART_COLOR: [f32; 3] = [0.4, 0.38, 0.4];
        box_seg(
            &mut objects,
            "Chemin de ronde Nord",
            -RAMPART_R,
            RAMPART_R - TRIM,
            -RAMPART_R,
            -RAMPART_R,
            2.2,
            0.3,
            1.5,
            RAMPART_COLOR,
        );
        box_seg(
            &mut objects,
            "Chemin de ronde Est",
            RAMPART_R,
            RAMPART_R,
            -RAMPART_R + TRIM,
            RAMPART_R,
            2.2,
            0.3,
            1.5,
            RAMPART_COLOR,
        );
        box_seg(
            &mut objects,
            "Chemin de ronde Sud",
            -RAMPART_R + TRIM,
            RAMPART_R,
            RAMPART_R,
            RAMPART_R,
            2.2,
            0.3,
            1.5,
            RAMPART_COLOR,
        );
        box_seg(
            &mut objects,
            "Chemin de ronde Ouest",
            -RAMPART_R,
            -RAMPART_R,
            -RAMPART_R,
            RAMPART_R - TRIM,
            2.2,
            0.3,
            1.5,
            RAMPART_COLOR,
        );
        poser(
            &mut objects,
            &mut imported,
            "Marches du rempart Nord-Ouest",
            "siege_rampart_stairs.glb",
            -HALF + 2.0,
            -HALF + 4.0,
            1.1,
            45.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Marches du rempart Sud-Est",
            "siege_rampart_stairs.glb",
            HALF - 2.0,
            HALF - 4.0,
            1.1,
            225.0,
            true,
        );

        // --- Dressing complémentaire des remparts/portes (pack siège) :
        // bastions à mi-pan, module de créneau en filler sur les tours,
        // chemin de ronde décoratif + torches sur le parapet, poterne à la
        // brèche Sud-Ouest, herse/caisse/pieux/bannière à la porte Nord,
        // boulets en tas près de la porte Est, chariot de braises sur le
        // chemin entre la porte Nord et la place.
        poser(
            &mut objects,
            &mut imported,
            "Bastion Nord",
            "siege_bastion.glb",
            0.0,
            -HALF,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bastion Sud",
            "siege_bastion.glb",
            0.0,
            HALF,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Module de créneau Nord-Ouest",
            "siege_crenel_module.glb",
            -HALF + 1.0,
            -HALF - 0.3,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Module de créneau Sud-Est",
            "siege_crenel_module.glb",
            HALF - 1.0,
            HALF + 0.3,
            1.0,
            180.0,
            false,
        );
        for (i, (x, z, yaw)) in [
            (-HALF + 6.0_f32, -HALF + 1.0_f32, 0.0_f32),
            (HALF - 6.0, -HALF + 1.0, 0.0),
            (HALF - 1.0, -HALF + 6.0, 90.0),
            (HALF - 1.0, HALF - 6.0, 90.0),
            (HALF - 6.0, HALF - 1.0, 180.0),
            (-HALF + 6.0, HALF - 1.0, 180.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                &format!("Torche de rempart {}", i + 1),
                "siege_rampart_torch.glb",
                x,
                z,
                1.0,
                yaw,
                false,
            );
        }
        poser(
            &mut objects,
            &mut imported,
            "Chemin de ronde décoratif Nord",
            "siege_rampart_walk.glb",
            -6.0,
            -HALF + 1.1,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Poterne Sud-Ouest",
            "siege_postern.glb",
            -HALF + 2.0,
            HALF - 2.0,
            1.0,
            135.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Herse de la porte Nord",
            "siege_portcullis.glb",
            0.0,
            -HALF + 0.4,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse de réserve de la porte Nord",
            "siege_reserve_crate.glb",
            1.6,
            -HALF + 1.5,
            1.0,
            10.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Rangée de pieux de la porte Nord",
            "siege_stake_row.glb",
            -1.5,
            -HALF + 2.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bannière de vague Nord",
            "siege_wave_banner.glb",
            -3.0,
            -HALF + 0.6,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bannière de vague Sud",
            "siege_wave_banner.glb",
            3.0,
            HALF - 0.6,
            1.0,
            180.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Corne d'alerte Nord",
            "siege_alert_horn.glb",
            -2.2,
            -HALF + 0.5,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Corne d'alerte Sud",
            "siege_alert_horn.glb",
            2.2,
            HALF - 0.5,
            1.0,
            180.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Panneau directionnel Est",
            "siege_rampart_signpost.glb",
            HALF - 3.0,
            -3.0,
            1.0,
            90.0,
            true,
        );
        for (i, (dx, dz)) in [(0.0_f32, 0.0_f32), (0.3, 0.25), (-0.25, 0.2)]
            .into_iter()
            .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                &format!("Boulet {}", i + 1),
                "siege_cannonball.glb",
                HALF - 2.0 + dx,
                -2.0 + dz,
                1.0,
                0.0,
                true,
            );
        }
        poser(
            &mut objects,
            &mut imported,
            "Chariot de braises",
            "siege_ember_cart.glb",
            0.0,
            -12.0,
            1.0,
            0.0,
            true,
        );

        // --- Place centrale : brasero communal (pack siège, remplace le feu
        // de camp générique — pièce signature de la place, cf.
        // creation3DBlendersuite.md), chaudron, gazebo, beffroi + girouette,
        // lucioles en cercle, dressing de mode (bannière/fanion/trophées).
        poser(
            &mut objects,
            &mut imported,
            "Feu communal",
            "siege_communal_brazier.glb",
            0.0,
            0.0,
            1.2,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bannière de mode de la place",
            "siege_mode_banner.glb",
            -2.0,
            4.5,
            1.0,
            20.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Fanion de la place",
            "siege_team_pennant.glb",
            2.0,
            4.5,
            1.0,
            -20.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Trophée de fin de manche",
            "siege_round_trophy.glb",
            0.0,
            3.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Portail de fin",
            "siege_end_portal.glb",
            0.0,
            6.0,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Chaudron de la place",
            "hamlet_cauldron.glb",
            1.3,
            0.8,
            1.0,
            20.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Gazebo de la place",
            "hamlet_gazebo.glb",
            4.5,
            -3.0,
            1.1,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Beffroi",
            "hamlet_bell_tower.glb",
            -4.5,
            -3.0,
            1.1,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Girouette du beffroi",
            "nature_weathervane.glb",
            -4.5,
            -3.0,
            1.0,
            0.0,
            false,
        );
        for i in 0..6 {
            let az = i as f32 * 60.0;
            let (x, z) = at(2.2, az);
            poser(
                &mut objects,
                &mut imported,
                &format!("Luciole de la place {}", i + 1),
                "fauna_firefly.glb",
                x,
                z,
                1.0,
                az,
                false,
            );
        }

        // --- Anneau de 16 spawns joueur (repères, pas des meshes — cf. la
        // doc de fonction) + 6 lisières de spawn de vagues, une par porte/
        // brèche, à 27 m du centre.
        for i in 0..16 {
            let az = i as f32 * 22.5;
            let (x, z) = at(6.5, az);
            marker(
                &mut objects,
                &format!("Point de spawn joueur {}", i + 1),
                x,
                z,
                [0.3, 0.8, 0.4],
            );
        }
        const WAVE_EDGES: [(&str, f32); 6] = [
            ("Nord", 0.0),
            ("Nord-Est", 45.0),
            ("Est", 90.0),
            ("Sud", 180.0),
            ("Sud-Ouest", 225.0),
            ("Ouest", 270.0),
        ];
        for (label, az) in WAVE_EDGES {
            let (x, z) = at(27.0, az);
            marker(
                &mut objects,
                &format!("Lisière de vague {label}"),
                x,
                z,
                [0.85, 0.25, 0.2],
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("Marqueur de zone {label}"),
                "siege_ground_marker.glb",
                x,
                z,
                1.0,
                az,
                false,
            );
        }

        // --- 4 îlots bâtis aux diagonales (maison + cour clôturée) : la
        // faune paisible (moutons/poules) vit dans deux des quatre cours, la
        // 4ᵉ héberge l'épouvantail + les parterres de fleurs.
        struct Islet {
            label: &'static str,
            house_file: &'static str,
            az: f32,
            fauna: Option<(&'static str, &'static str)>,
            extra: Option<&'static str>,
        }
        const ISLETS: &[Islet] = &[
            Islet {
                label: "Nord-Est",
                house_file: "hamlet_house_a.glb",
                az: 45.0,
                fauna: Some(("Mouton", "fauna_sheep.glb")),
                extra: None,
            },
            Islet {
                label: "Sud-Est",
                house_file: "hamlet_inn.glb",
                az: 135.0,
                fauna: Some(("Poule", "fauna_chicken.glb")),
                extra: None,
            },
            Islet {
                label: "Sud-Ouest",
                house_file: "hamlet_house_b.glb",
                az: 225.0,
                fauna: None,
                extra: None,
            },
            Islet {
                label: "Nord-Ouest",
                house_file: "hamlet_house_c.glb",
                az: 315.0,
                fauna: None,
                extra: Some("nature_scarecrow.glb"),
            },
        ];
        for isl in ISLETS {
            let (hx, hz) = at(13.0, isl.az);
            poser(
                &mut objects,
                &mut imported,
                &format!("Maison {}", isl.label),
                isl.house_file,
                hx,
                hz,
                1.0,
                isl.az + 180.0,
                true,
            );
            let (yx, yz) = at(17.5, isl.az);
            let yh = 3.0;
            let sides = [
                ("Nord", 0.0_f32, -1.0_f32),
                ("Sud", 0.0, 1.0),
                ("Est", 1.0, 0.0),
                ("Ouest", -1.0, 0.0),
            ];
            let mut best = 0usize;
            let mut best_d = f32::MAX;
            for (i, &(_, nx, nz)) in sides.iter().enumerate() {
                let mx = yx + nx * yh;
                let mz = yz + nz * yh;
                let d = mx * mx + mz * mz;
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            for (i, &(slabel, nx, nz)) in sides.iter().enumerate() {
                if i == best {
                    continue; // ouverture côté place
                }
                let mx = yx + nx * yh;
                let mz = yz + nz * yh;
                let yaw = if nz != 0.0 { 0.0 } else { 90.0 };
                poser(
                    &mut objects,
                    &mut imported,
                    &format!("Clôture {} {slabel}", isl.label),
                    "hamlet_fence.glb",
                    mx,
                    mz,
                    3.0,
                    yaw,
                    true,
                );
            }
            if let Some((name, file)) = isl.fauna {
                poser(
                    &mut objects,
                    &mut imported,
                    &format!("{name} {}", isl.label),
                    file,
                    yx - 1.0,
                    yz - 1.0,
                    0.9,
                    0.0,
                    false,
                );
                poser(
                    &mut objects,
                    &mut imported,
                    &format!("{name} {} 2", isl.label),
                    file,
                    yx + 1.0,
                    yz + 1.0,
                    0.9,
                    90.0,
                    false,
                );
            }
            if let Some(file) = isl.extra {
                poser(
                    &mut objects,
                    &mut imported,
                    "Épouvantail",
                    file,
                    yx,
                    yz,
                    1.0,
                    0.0,
                    true,
                );
                poser(
                    &mut objects,
                    &mut imported,
                    "Parterre de fleurs",
                    "nature_flowers.glb",
                    yx + 1.5,
                    yz,
                    1.0,
                    0.0,
                    false,
                );
            }
        }

        // --- Bâtiments d'artisanat, entre les îlots et les murs (flancs des
        // 4 portes cardinales).
        poser(
            &mut objects,
            &mut imported,
            "Forge",
            "hamlet_blacksmith.glb",
            8.0,
            -19.0,
            1.1,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Soufflet de la forge",
            "nature_bellows.glb",
            9.4,
            -18.0,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Marteau-pilon",
            "nature_forge_hammer.glb",
            6.6,
            -18.0,
            1.0,
            180.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Métier à tisser",
            "nature_weaving_loom.glb",
            -8.0,
            -19.0,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Écurie",
            "hamlet_stable.glb",
            19.0,
            -8.0,
            1.1,
            270.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Foin de l'écurie",
            "hamlet_hay.glb",
            19.0,
            -5.8,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Rouet",
            "nature_spinning_wheel.glb",
            19.0,
            8.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Scierie",
            "hamlet_sawmill.glb",
            8.0,
            19.0,
            1.1,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Lame de la scierie",
            "hamlet_sawmill_saw.glb",
            9.4,
            19.6,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Pompe à eau",
            "nature_water_pump.glb",
            -8.0,
            19.0,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Puits",
            "hamlet_well.glb",
            -19.0,
            -8.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Treuil du puits",
            "nature_well_windlass.glb",
            -19.0,
            -6.0,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Moulin",
            "hamlet_mill.glb",
            -19.0,
            8.0,
            1.1,
            90.0,
            true,
        );

        // --- Mobilier de place/marché.
        poser(
            &mut objects,
            &mut imported,
            "Étal du marché 1",
            "hamlet_market_stand_a.glb",
            6.0,
            3.5,
            1.0,
            200.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Étal du marché 2",
            "hamlet_market_stand_b.glb",
            -6.0,
            3.5,
            1.0,
            340.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Banc du marché 1",
            "hamlet_bench_a.glb",
            3.0,
            6.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Banc du marché 2",
            "hamlet_bench_b.glb",
            -3.0,
            6.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du marché 1",
            "hamlet_barrel.glb",
            6.5,
            -0.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du marché 2",
            "hamlet_barrel.glb",
            6.9,
            1.0,
            1.0,
            30.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du marché 3",
            "hamlet_barrel.glb",
            -6.5,
            -0.5,
            1.0,
            60.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du marché 1",
            "hamlet_crate.glb",
            7.5,
            0.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du marché 2",
            "hamlet_crate.glb",
            -7.5,
            0.5,
            1.0,
            45.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du marché 3",
            "hamlet_crate.glb",
            -7.2,
            -1.5,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sac du marché",
            "hamlet_bag.glb",
            5.0,
            5.0,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sac ouvert du marché",
            "hamlet_bag_open.glb",
            5.5,
            5.4,
            1.0,
            20.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sacs du marché",
            "hamlet_bags.glb",
            -5.0,
            5.0,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Paquet du marché 1",
            "hamlet_package_a.glb",
            -5.5,
            -5.5,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Paquet du marché 2",
            "hamlet_package_b.glb",
            5.5,
            -5.5,
            1.0,
            30.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Chaise du marché 1",
            "hamlet_chair.glb",
            2.0,
            6.0,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Chaise du marché 2",
            "hamlet_chair.glb",
            -2.0,
            6.0,
            1.0,
            180.0,
            false,
        );

        // --- Lanternes (2/porte) + bannière (1/porte) aux 4 portes
        // principales : télégraphe visuel de l'arrivée des vagues (GDD §10).
        const GATES: [(&str, f32, f32, f32); 4] = [
            ("Nord", 0.0, -HALF, 0.0),
            ("Sud", 0.0, HALF, 180.0),
            ("Est", HALF, 0.0, 90.0),
            ("Ouest", -HALF, 0.0, 270.0),
        ];
        for (label, gx, gz, yaw) in GATES {
            let (dx, dz) = if gz.abs() > gx.abs() {
                (1.0, 0.0)
            } else {
                (0.0, 1.0)
            };
            poser(
                &mut objects,
                &mut imported,
                &format!("Lanterne {label} 1"),
                "nature_lantern.glb",
                gx - dx * 3.0,
                gz - dz * 3.0,
                1.0,
                0.0,
                false,
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("Lanterne {label} 2"),
                "nature_lantern.glb",
                gx + dx * 3.0,
                gz + dz * 3.0,
                1.0,
                0.0,
                false,
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("Bannière {label}"),
                "nature_banner.glb",
                gx,
                gz,
                1.0,
                yaw,
                false,
            );
        }

        // --- Hors les murs : rivière (deux bras, ouest et sud) rejoignant un
        // lac au coin sud-ouest, pont, moulin à eau, berges, petite rizière.
        const EAU: [f32; 3] = [0.18, 0.42, 0.65];
        const EAU_LAC: [f32; 3] = [0.14, 0.34, 0.55];
        const SABLE: [f32; 3] = [0.72, 0.64, 0.44];
        aplat(
            &mut objects,
            "Rivière ouest",
            -31.5,
            0.0,
            5.0,
            58.0,
            0.02,
            EAU,
        );
        aplat(&mut objects, "Rivière sud", 0.0, 31.5, 58.0, 5.0, 0.02, EAU);
        aplat(
            &mut objects,
            "Berge du lac",
            -42.5,
            42.5,
            29.0,
            29.0,
            0.012,
            SABLE,
        );
        aplat(&mut objects, "Lac", -42.0, 42.0, 24.0, 24.0, 0.015, EAU_LAC);
        aplat(
            &mut objects,
            "Rizière du sud",
            -42.0,
            60.0,
            8.0,
            6.0,
            0.03,
            [0.55, 0.6, 0.25],
        );
        poser(
            &mut objects,
            &mut imported,
            "Pont de la rivière ouest",
            "nature_bridge.glb",
            -31.5,
            0.0,
            1.15,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Moulin à eau",
            "nature_watermill.glb",
            -36.5,
            8.0,
            1.1,
            90.0,
            true,
        );
        for (i, (name, x, z)) in [
            ("Roseaux 1", -29.5, -15.0),
            ("Roseaux 2", -29.5, 15.0),
            ("Roseaux 3", 0.0, 30.5),
            ("Roseaux 4", -20.0, 30.5),
            ("Roseaux 5", -46.0, 30.0),
            ("Roseaux 6", -38.0, 46.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_reeds.glb",
                x,
                z,
                1.0,
                i as f32 * 40.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Nénuphars 1", -38.0, 38.0),
            ("Nénuphars 2", -46.0, 46.0),
            ("Nénuphars 3", -44.0, 36.0),
            ("Nénuphars 4", -40.0, 44.0),
            ("Nénuphars 5", -44.0, 40.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_lily.glb",
                x,
                z,
                1.0,
                i as f32 * 50.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Rocher de berge 1", -33.0, -20.0),
            ("Rocher de berge 2", -33.0, 20.0),
            ("Rocher de berge 3", -25.0, 33.0),
            ("Rocher de berge 4", -46.0, 42.0),
            ("Rocher de berge 5", 10.0, 32.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_rock.glb",
                x,
                z,
                1.0,
                i as f32 * 70.0,
                false,
            );
        }
        for i in 0..4 {
            poser(
                &mut objects,
                &mut imported,
                &format!("Riz du sud {}", i + 1),
                "nature_rice.glb",
                -45.0 + i as f32 * 3.0,
                60.0,
                1.0,
                0.0,
                false,
            );
        }

        // --- Habillage organique `shore_*` (Phase E, traduit de la
        // composition Blender `docs/blender/composition_eau.blend`, Phase B) :
        // berges/rochers lissés autour des 3 rects d'eau réels
        // (Rivière ouest/sud, Lac) — cf. `mapsJeuReflexionAnalyse.md` §2 bis.2
        // point 3. Décor purement cosmétique, non solide (comme les Roseaux/
        // Nénuphars ci-dessus), pour ne pas gêner les sondes IA (§4.2 règle 1
        // du doc de réflexion).
        const SHORE: &[(&str, f32, f32, f32, f32)] = &[
            ("shore_bank_moss", -22.15, 27.54, 0.757, 207.8),
            ("shore_bank_moss", 7.74, 27.71, 1.005, 170.3),
            ("shore_bank_moss", 11.59, 27.88, 0.812, 252.0),
            ("shore_bank_moss", -0.83, 28.15, 1.068, 64.4),
            ("shore_bank_moss", 3.08, 28.48, 0.793, 70.2),
            ("shore_bank_moss", -17.10, 28.53, 0.867, 117.5),
            ("shore_beached_algae", -33.48, 31.39, 0.695, 43.1),
            ("shore_beached_fish", -33.71, 32.97, 0.768, 269.1),
            ("shore_drift_line", 12.32, 30.29, 0.574, 270.9),
            ("shore_drift_line", 6.98, 30.31, 0.502, 228.4),
            ("shore_driftwood", -26.58, 30.81, 0.606, 335.6),
            ("shore_driftwood", 20.75, 31.89, 0.610, 304.9),
            ("shore_driftwood", 16.83, 32.01, 0.615, 270.1),
            ("shore_gentle_bank", -35.33, -19.33, 0.957, 117.5),
            ("shore_gentle_bank", -34.65, -10.40, 0.914, 327.7),
            ("shore_gentle_bank", -28.20, 2.89, 0.700, 356.4),
            ("shore_gentle_bank", -34.65, 12.65, 1.049, 141.9),
            ("shore_gentle_bank", -28.56, 13.02, 0.832, 52.7),
            ("shore_gentle_bank", -35.38, 20.69, 0.816, 89.8),
            ("shore_gentle_bank", -4.67, 27.40, 0.901, 128.1),
            ("shore_gentle_bank", 20.75, 27.53, 1.073, 40.5),
            ("shore_gentle_bank", -9.03, 28.23, 0.954, 134.6),
            ("shore_gentle_bank", -27.50, 28.31, 0.906, 220.7),
            ("shore_gentle_bank", 24.81, 28.34, 0.909, 292.2),
            ("shore_gentle_bank", -50.00, 28.80, 0.994, 334.1),
            ("shore_gentle_bank", -28.44, 33.00, 1.130, 160.3),
            ("shore_gentle_bank", 17.19, 34.64, 0.887, 302.8),
            ("shore_gentle_bank", 10.49, 34.75, 0.765, 42.5),
            ("shore_gentle_bank", -13.24, 34.82, 0.785, 90.6),
            ("shore_gentle_bank", -4.92, 34.91, 0.729, 144.2),
            ("shore_gentle_bank", -28.51, 42.00, 0.981, 149.0),
            ("shore_natural_basin", -34.00, 28.45, 0.912, 311.2),
            ("shore_natural_basin", -50.00, 55.84, 0.823, 356.6),
            ("shore_nest", -32.50, 32.00, 1.000, 39.6),
            ("shore_pebble_group", 21.87, 34.56, 0.894, 320.2),
            ("shore_pebble_group", 2.16, 34.86, 0.850, 304.8),
            ("shore_pebble_group", -9.55, 34.96, 0.927, 189.8),
            ("shore_pebble_group", -26.98, 35.05, 0.967, 124.4),
            ("shore_rooted_bank", -35.59, -15.27, 0.998, 302.5),
            ("shore_rooted_bank", -34.63, 2.51, 0.796, 263.5),
            ("shore_rooted_bank", -34.69, 16.57, 0.880, 216.5),
            ("shore_rooted_bank", -55.40, 33.00, 1.094, 84.0),
            ("shore_rooted_bank", -55.27, 42.00, 0.865, 327.2),
            ("shore_rooted_bank", -55.51, 51.00, 0.912, 339.7),
            ("shore_rooted_bank", -28.14, 51.00, 1.058, 218.7),
            ("shore_rooted_bank", -34.00, 55.11, 0.885, 115.8),
            ("shore_shell_cluster", -33.65, 32.02, 0.658, 73.0),
            ("shore_smooth_rock", -33.63, -23.27, 0.457, 21.4),
            ("shore_smooth_rock", -27.64, -22.96, 0.885, 235.4),
            ("shore_smooth_rock", -33.23, -15.61, 0.405, 24.1),
            ("shore_smooth_rock", -28.18, -9.15, 0.760, 203.8),
            ("shore_smooth_rock", -31.02, 13.18, 0.422, 139.0),
            ("shore_smooth_rock", -27.77, 16.44, 0.816, 48.9),
            ("shore_smooth_rock", -33.49, 20.06, 0.477, 184.4),
            ("shore_smooth_rock", -0.49, 34.44, 0.939, 192.8),
            ("shore_smooth_rock", -22.77, 34.59, 0.875, 305.9),
            ("shore_smooth_rock", -17.93, 34.71, 0.986, 281.2),
            ("shore_smooth_rock", 25.23, 34.94, 0.803, 50.3),
            ("shore_smooth_rock", 8.12, 35.35, 0.942, 39.9),
            ("shore_steep_bank", -28.41, -27.12, 0.889, 184.3),
            ("shore_steep_bank", -35.44, -27.00, 1.023, 308.5),
            ("shore_steep_bank", -34.57, -23.31, 1.062, 193.9),
            ("shore_steep_bank", -27.96, -20.12, 0.899, 179.9),
            ("shore_steep_bank", -28.09, -15.75, 0.869, 10.7),
            ("shore_steep_bank", -27.89, -7.22, 0.839, 3.9),
            ("shore_steep_bank", -34.80, -6.23, 1.015, 347.1),
            ("shore_steep_bank", -27.91, 21.31, 0.845, 139.8),
            ("shore_steep_bank", -27.67, 25.13, 0.738, 351.8),
            ("shore_steep_bank", -34.71, 25.67, 0.959, 296.5),
            ("shore_steep_bank", -13.32, 27.80, 0.767, 66.6),
            ("shore_steep_bank", 16.32, 28.19, 1.074, 264.6),
            ("shore_water_ripple", -33.00, 34.00, 0.803, 98.2),
            ("shore_water_ripple", -33.00, 50.00, 0.922, 188.9),
            ("shore_water_ripple", -50.00, 51.00, 0.874, 141.9),
        ];
        for (i, (base, x, z, scale, yaw)) in SHORE.iter().enumerate() {
            let file: &'static str = match *base {
                "shore_bank_moss" => "shore_bank_moss.glb",
                "shore_beached_algae" => "shore_beached_algae.glb",
                "shore_beached_fish" => "shore_beached_fish.glb",
                "shore_drift_line" => "shore_drift_line.glb",
                "shore_driftwood" => "shore_driftwood.glb",
                "shore_gentle_bank" => "shore_gentle_bank.glb",
                "shore_natural_basin" => "shore_natural_basin.glb",
                "shore_nest" => "shore_nest.glb",
                "shore_pebble_group" => "shore_pebble_group.glb",
                "shore_rooted_bank" => "shore_rooted_bank.glb",
                "shore_shell_cluster" => "shore_shell_cluster.glb",
                "shore_smooth_rock" => "shore_smooth_rock.glb",
                "shore_steep_bank" => "shore_steep_bank.glb",
                "shore_water_ripple" => "shore_water_ripple.glb",
                other => unreachable!("asset shore_* non mappé : {other}"),
            };
            poser(
                &mut objects,
                &mut imported,
                &format!("{base} {}", i + 1),
                file,
                *x,
                *z,
                *scale,
                *yaw,
                false,
            );
        }

        // --- Habillage organique `grotto_*` (Phase F, traduit de la
        // composition Blender `docs/blender/composition_grotto.blend`,
        // Phase C) : entrée de grotte sur la marge ouest de
        // `hameau_gdd_demo()`, juste au-delà de la Rivière ouest (bord est à
        // x=-34). **Composition dégradée assumée** : `hameau_gdd_demo()` n'a
        // aucun relief/heightmap (`Sol` = `MeshKind::Plane`), contrairement à
        // `mmorpg_demo()` — la Phase C a vérifié l'absence de terrain à
        // intégrer avant de composer (décision actée dans
        // `sprintjeurefelxion.md` §4, sculpter un nouveau relief reste hors
        // scope de ce sprint). Formations structurelles (mur du fond, arche,
        // passage bas, colonnes, poutres, stalagmites larges, bloc effondré)
        // solides en TriMesh — suit le maillage réel, ne bloque donc pas le
        // passage sous l'arche/le passage bas ; le reste est cosmétique.
        const GROTTO: &[(&str, f32, f32, f32, f32, f32)] = &[
            // (base, x, z, hauteur, scale, yaw_deg)
            ("grotto_back_wall", -45.0, 0.0, 0.0, 1.0, 90.0),
            ("grotto_bones", -37.0, -4.0, 0.0, 0.9, 45.0),
            ("grotto_bumpy_floor", -38.5, 0.0, -0.02, 1.0, 0.0),
            ("grotto_collapsed_block", -40.0, 6.5, 0.0, 1.0, 15.0),
            ("grotto_column", -38.0, -2.8, 0.0, 1.0, 0.0),
            ("grotto_column", -38.0, 2.8, 0.0, 1.0, 0.0),
            ("grotto_crystal", -44.0, -1.0, 0.0, 0.6, 120.0),
            ("grotto_crystal", -41.0, 4.5, 0.0, 0.8, 30.0),
            ("grotto_entrance_arch", -37.5, 0.0, 0.0, 1.1, 90.0),
            ("grotto_glow_mushroom_cluster", -39.5, -4.5, 0.0, 1.0, 0.0),
            ("grotto_glow_mushroom_small", -38.5, 4.0, 0.0, 0.8, 0.0),
            ("grotto_hanging_drop", -37.6, 0.6, 2.0, 1.0, 0.0),
            ("grotto_hanging_root", -44.7, -1.8, 1.7, 1.0, 25.0),
            ("grotto_low_passage", -39.5, 0.0, 0.0, 1.0, 90.0),
            ("grotto_mold_veil", -44.8, 1.8, 1.2, 1.0, 60.0),
            ("grotto_rubble", -36.5, 2.5, 0.0, 1.0, 0.0),
            ("grotto_stalactite_large", -44.6, 1.2, 1.9, 0.9, 40.0),
            ("grotto_stalactite_large", -37.8, -1.0, 2.1, 1.0, 0.0),
            ("grotto_stalactite_small", -44.8, -1.5, 1.8, 0.7, 70.0),
            ("grotto_stalactite_small", -38.2, 1.0, 1.9, 0.8, 10.0),
            ("grotto_stalagmite_large", -44.5, -3.0, 0.0, 0.95, 55.0),
            ("grotto_stalagmite_large", -42.0, 3.5, 0.0, 1.0, 0.0),
            ("grotto_stalagmite_small", -43.5, 2.5, 0.0, 0.6, 90.0),
            ("grotto_stalagmite_small", -39.0, -3.5, 0.0, 0.7, 20.0),
            ("grotto_support_beam", -41.5, -1.5, 0.0, 0.9, 15.0),
            ("grotto_support_beam", -41.5, 1.8, 0.0, 0.9, 340.0),
            ("grotto_underground_puddle", -42.0, 0.5, 0.01, 1.0, 0.0),
        ];
        for (i, (base, x, z, height, scale, yaw)) in GROTTO.iter().enumerate() {
            let file: &'static str = match *base {
                "grotto_back_wall" => "grotto_back_wall.glb",
                "grotto_bones" => "grotto_bones.glb",
                "grotto_bumpy_floor" => "grotto_bumpy_floor.glb",
                "grotto_collapsed_block" => "grotto_collapsed_block.glb",
                "grotto_column" => "grotto_column.glb",
                "grotto_crystal" => "grotto_crystal.glb",
                "grotto_entrance_arch" => "grotto_entrance_arch.glb",
                "grotto_glow_mushroom_cluster" => "grotto_glow_mushroom_cluster.glb",
                "grotto_glow_mushroom_small" => "grotto_glow_mushroom_small.glb",
                "grotto_hanging_drop" => "grotto_hanging_drop.glb",
                "grotto_hanging_root" => "grotto_hanging_root.glb",
                "grotto_low_passage" => "grotto_low_passage.glb",
                "grotto_mold_veil" => "grotto_mold_veil.glb",
                "grotto_rubble" => "grotto_rubble.glb",
                "grotto_stalactite_large" => "grotto_stalactite_large.glb",
                "grotto_stalactite_small" => "grotto_stalactite_small.glb",
                "grotto_stalagmite_large" => "grotto_stalagmite_large.glb",
                "grotto_stalagmite_small" => "grotto_stalagmite_small.glb",
                "grotto_support_beam" => "grotto_support_beam.glb",
                "grotto_underground_puddle" => "grotto_underground_puddle.glb",
                other => unreachable!("asset grotto_* non mappé : {other}"),
            };
            let solide = matches!(
                *base,
                "grotto_back_wall"
                    | "grotto_entrance_arch"
                    | "grotto_low_passage"
                    | "grotto_column"
                    | "grotto_support_beam"
                    | "grotto_stalagmite_large"
                    | "grotto_collapsed_block"
            );
            poser(
                &mut objects,
                &mut imported,
                &format!("{base} {}", i + 1),
                file,
                *x,
                *z,
                *scale,
                *yaw,
                solide,
            );
            if *height != 0.0
                && let Some(o) = objects.last_mut()
            {
                o.transform.position.y = *height;
            }
        }

        // --- Poste de guet, sur l'axe d'approche Nord (léger décalage pour
        // ne pas boucher la lisière de vague), avec 2 lanternes.
        poser(
            &mut objects,
            &mut imported,
            "Poste de guet",
            "nature_tower.glb",
            7.0,
            -49.5,
            1.3,
            8.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Lanterne du poste de guet 1",
            "nature_lantern.glb",
            4.0,
            -49.5,
            1.0,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Lanterne du poste de guet 2",
            "nature_lantern.glb",
            10.0,
            -49.5,
            1.0,
            0.0,
            false,
        );

        // --- Cabane de garde-forestier, en clairière, entourée de bois de
        // chauffage et de réserves.
        poser(
            &mut objects,
            &mut imported,
            "Cabane du garde-forestier",
            "nature_cabin.glb",
            39.0,
            -22.5,
            1.2,
            250.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bois du garde-forestier 1",
            "nature_woodpile.glb",
            36.0,
            -20.0,
            1.0,
            20.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Bois du garde-forestier 2",
            "nature_woodpile.glb",
            42.0,
            -25.0,
            1.0,
            80.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tonneau du garde-forestier",
            "hamlet_barrel.glb",
            37.0,
            -25.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du garde-forestier 1",
            "hamlet_crate.glb",
            41.5,
            -19.5,
            1.0,
            30.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du garde-forestier 2",
            "hamlet_crate.glb",
            35.5,
            -24.0,
            1.0,
            60.0,
            true,
        );

        // --- Camp de chasseurs : second foyer, foin et provisions.
        poser(
            &mut objects,
            &mut imported,
            "Foyer du camp de chasseurs",
            "nature_campfire.glb",
            17.0,
            47.0,
            1.1,
            0.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Foin du camp de chasseurs 1",
            "hamlet_hay.glb",
            14.0,
            49.5,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Foin du camp de chasseurs 2",
            "hamlet_hay.glb",
            20.0,
            44.5,
            1.0,
            90.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du camp de chasseurs 1",
            "hamlet_crate.glb",
            19.5,
            50.0,
            1.0,
            15.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Caisse du camp de chasseurs 2",
            "hamlet_crate.glb",
            15.5,
            44.0,
            1.0,
            75.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Sac du camp de chasseurs",
            "hamlet_bag.glb",
            18.0,
            43.0,
            1.0,
            0.0,
            false,
        );

        // --- Second point d'eau (mare), au Nord-Est, à l'opposé du lac —
        // berge, roseaux, nénuphars, rochers, + ponton de pêche.
        const MARE: [f32; 3] = [0.20, 0.44, 0.66];
        const MARE_SABLE: [f32; 3] = [0.70, 0.62, 0.42];
        aplat(
            &mut objects,
            "Berge de la mare",
            44.0,
            -46.0,
            18.0,
            18.0,
            0.012,
            MARE_SABLE,
        );
        aplat(&mut objects, "Mare", 44.0, -46.0, 14.0, 14.0, 0.015, MARE);
        poser(
            &mut objects,
            &mut imported,
            "Ponton de pêche",
            "nature_boat.glb",
            44.0,
            -37.5,
            1.1,
            180.0,
            false,
        );
        poser(
            &mut objects,
            &mut imported,
            "Panneau du ponton",
            "nature_signpost.glb",
            41.0,
            -38.0,
            1.0,
            160.0,
            false,
        );
        for (i, (name, x, z)) in [
            ("Rocher de la mare 1", 44.0, -55.0),
            ("Rocher de la mare 2", 53.0, -46.0),
            ("Rocher de la mare 3", 35.0, -46.0),
            ("Rocher de la mare 4", 44.0, -37.0),
            ("Rocher de la mare 5", 49.0, -52.0),
            ("Rocher de la mare 6", 38.0, -40.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_rock.glb",
                x,
                z,
                1.0,
                i as f32 * 55.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Roseaux de la mare 1", 39.0, -51.0),
            ("Roseaux de la mare 2", 49.0, -51.0),
            ("Roseaux de la mare 3", 39.0, -41.5),
            ("Roseaux de la mare 4", 49.0, -41.5),
            ("Roseaux de la mare 5", 44.0, -53.5),
            ("Roseaux de la mare 6", 44.0, -38.5),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_reeds.glb",
                x,
                z,
                1.0,
                i as f32 * 45.0,
                false,
            );
        }
        for (i, (name, x, z)) in [
            ("Nénuphar de la mare 1", 41.0, -46.0),
            ("Nénuphar de la mare 2", 47.0, -46.0),
            ("Nénuphar de la mare 3", 44.0, -49.0),
            ("Nénuphar de la mare 4", 44.0, -43.0),
            ("Nénuphar de la mare 5", 42.0, -49.0),
        ]
        .into_iter()
        .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                name,
                "nature_lily.glb",
                x,
                z,
                1.0,
                i as f32 * 60.0,
                false,
            );
        }

        // --- Prairies fleuries : 3 clairières plus denses que le fleurissage
        // existant des îlots, réparties hors des couloirs de vague.
        const PRAIRIES: &[(&str, f32, f32)] = &[
            ("Est", 51.7, 18.8),
            ("Sud-Ouest", -17.8, 48.9),
            ("Ouest", -45.1, -16.4),
            ("Nord", 8.0, -60.0),
        ];
        for (label, cx, cz) in PRAIRIES {
            for i in 0..9 {
                let ang = i as f32 * 40.0;
                let ring = if i % 2 == 0 { 1.8 } else { 3.2 };
                let (dx, dz) = at(ring, ang);
                poser(
                    &mut objects,
                    &mut imported,
                    &format!("Fleurs de la prairie {label} {}", i + 1),
                    "nature_flowers.glb",
                    cx + dx,
                    cz + dz,
                    1.1,
                    ang,
                    false,
                );
            }
        }

        // --- Bosquet/verger : petite clairière plus dense, façon lieu de
        // repos, avec quelques rochers.
        const VERGER_CX: f32 = 28.3;
        const VERGER_CZ: f32 = 28.3;
        const VERGER_FILES: [&str; 4] = [
            "nature_tree.glb",
            "nature_tree2.glb",
            "nature_pine.glb",
            "nature_pine2.glb",
        ];
        {
            let mut rng_verger = crate::runtime::rng::Rng::new(0x5645_5247_4552_3238); // « VERGER28 »
            for i in 0..26 {
                let ang = rng_verger.next_range(0.0, 360.0);
                let r = rng_verger.next_range(1.5, 6.5);
                let (dx, dz) = at(r, ang);
                let file = VERGER_FILES[rng_verger.next_below(VERGER_FILES.len())];
                let scale = rng_verger.next_range(0.85, 1.2);
                poser(
                    &mut objects,
                    &mut imported,
                    &format!("Arbre du verger {}", i + 1),
                    file,
                    VERGER_CX + dx,
                    VERGER_CZ + dz,
                    scale,
                    ang,
                    true,
                );
            }
            for (i, (dx, dz)) in [
                (3.0_f32, 0.0_f32),
                (-3.0, 1.5),
                (0.0, -3.0),
                (4.5, -2.5),
                (-4.5, -1.0),
            ]
            .into_iter()
            .enumerate()
            {
                poser(
                    &mut objects,
                    &mut imported,
                    &format!("Rocher du verger {}", i + 1),
                    "nature_rock.glb",
                    VERGER_CX + dx,
                    VERGER_CZ + dz,
                    1.0,
                    i as f32 * 90.0,
                    false,
                );
            }
        }

        // --- Forêt en anneau (27 → 70 m), couloirs dégagés dans l'axe des 6
        // lisières de spawn, eau/rizière exclues + faune variée.
        let excl_eau: [(f32, f32, f32, f32); 15] = [
            (-34.0, -29.0, -29.0, 29.0),  // rivière ouest
            (-29.0, 29.0, 29.0, 34.0),    // rivière sud
            (-58.0, 27.0, -27.0, 58.0),   // lac + berge
            (-46.0, 57.0, -38.0, 63.0),   // rizière du sud
            (33.0, -58.0, 55.0, -35.0),   // mare du Nord-Est + ponton
            (0.0, -56.0, 14.0, -43.0),    // poste de guet
            (33.0, -28.0, 45.0, -17.0),   // cabane du garde-forestier
            (11.0, 41.0, 23.0, 53.0),     // camp de chasseurs
            (48.0, 15.0, 55.0, 22.0),     // prairie fleurie Est
            (-21.0, 45.0, -14.0, 52.0),   // prairie fleurie Sud-Ouest
            (-49.0, -20.0, -42.0, -13.0), // prairie fleurie Ouest
            (3.0, -64.0, 13.0, -56.0),    // prairie fleurie Nord
            (21.0, 21.0, 36.0, 36.0),     // bosquet/verger
            (39.0, 36.0, 49.0, 46.0),     // clairière de l'Aînée (autel + cage du chef) — Phase D
            (-47.0, -9.0, -36.0, 9.0),    // entrée de grotte, marge ouest — Phase F
        ];
        let mut rng = crate::runtime::rng::Rng::new(0x4841_4D45_4155_3438); // « HAMEAU48 »
        foret_scatter(
            &mut objects,
            &mut imported,
            &mut rng,
            27.0,
            70.0,
            &excl_eau,
            195,
        );

        const FOREST_FAUNA: &[&str] = &[
            "fauna_deer.glb",
            "fauna_rabbit.glb",
            "fauna_squirrel.glb",
            "fauna_fox.glb",
            "fauna_boar.glb",
            "fauna_hedgehog.glb",
            "fauna_goat.glb",
            "fauna_raccoon.glb",
            "fauna_mole.glb",
        ];
        const AIR_FAUNA: &[&str] = &[
            "fauna_bird.glb",
            "fauna_crow.glb",
            "fauna_jay.glb",
            "fauna_bat.glb",
            "fauna_butterfly.glb",
            "fauna_dragonfly.glb",
            "fauna_bee.glb",
            "fauna_ladybug.glb",
        ];
        for (i, &file) in FOREST_FAUNA.iter().chain(AIR_FAUNA.iter()).enumerate() {
            faune_scatter(
                &mut objects,
                &mut imported,
                &mut rng,
                28.0,
                65.0,
                &excl_eau,
                file,
                &format!("Faune {}", i + 1),
                7,
            );
        }
        for (i, (x, z)) in [(0.0_f32, -23.0_f32), (7.0, -49.5), (39.0, -22.5)]
            .into_iter()
            .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                &format!("Chouette {}", i + 1),
                "fauna_owl.glb",
                x,
                z,
                0.8,
                0.0,
                false,
            );
        }
        // --- Faune aquatique : lac historique (Sud-Ouest) + nouvelle mare
        // (Nord-Est), comptes doublés/triplés par rapport à la version
        // d'origine (1 par espèce).
        for (name, file, x, z) in [
            ("Canard 1", "fauna_duck.glb", -31.5, 10.0),
            ("Canard 2", "fauna_duck.glb", 47.0, -44.0),
            ("Canard 3", "fauna_duck.glb", -44.0, 44.0),
            ("Oie 1", "fauna_goose.glb", -31.5, -10.0),
            ("Oie 2", "fauna_goose.glb", 41.0, -48.0),
            ("Oie 3", "fauna_goose.glb", -40.0, 38.0),
            ("Grenouille 1", "fauna_frog.glb", -30.0, 20.0),
            ("Grenouille 2", "fauna_frog.glb", 40.0, -42.0),
            ("Grenouille 3", "fauna_frog.glb", -36.0, 34.0),
            ("Grenouille 4", "fauna_frog.glb", 48.0, -49.0),
            ("Poisson 1", "fauna_fish.glb", -42.0, 42.0),
            ("Poisson 2", "fauna_fish.glb", 44.0, -46.0),
            ("Poisson 3", "fauna_fish.glb", -45.0, 39.0),
            ("Poisson 4", "fauna_fish.glb", 42.0, -49.0),
            ("Héron 1", "fauna_heron.glb", -34.0, 28.0),
            ("Héron 2", "fauna_heron.glb", 37.0, -37.0),
            ("Héron 3", "fauna_heron.glb", -48.0, 48.0),
            ("Tortue 1", "fauna_turtle.glb", -46.0, 46.0),
            ("Tortue 2", "fauna_turtle.glb", 50.0, -46.0),
            ("Crabe 1", "fauna_crab.glb", -30.0, 31.0),
            ("Crabe 2", "fauna_crab.glb", 43.0, -55.0),
            ("Crabe 3", "fauna_crab.glb", -25.0, 34.0),
            ("Escargot 1", "fauna_snail.glb", -29.5, 22.0),
            ("Escargot 2", "fauna_snail.glb", 36.0, -47.0),
            ("Escargot 3", "fauna_snail.glb", -47.0, 41.0),
        ] {
            poser(
                &mut objects,
                &mut imported,
                name,
                file,
                x,
                z,
                0.9,
                0.0,
                false,
            );
        }
        // --- Lucioles supplémentaires près du camp de chasseurs (ambiance
        // nocturne, en plus du cercle de 6 sur la place).
        for (i, (x, z)) in [(20.0_f32, 49.0_f32), (14.0, 45.0), (18.0, 51.0)]
            .into_iter()
            .enumerate()
        {
            poser(
                &mut objects,
                &mut imported,
                &format!("Luciole du camp de chasseurs {}", i + 1),
                "fauna_firefly.glb",
                x,
                z,
                1.0,
                i as f32 * 40.0,
                false,
            );
        }

        // --- Lande environnante (pack siège, creation3DBlendersuite.md) :
        // décor dispersé dans l'anneau extérieur, en évitant les zones déjà
        // nommées (îlots r=13-20, camp de chasseurs ~(14-20,45-51), mare aux
        // nénuphars ~(41-49,-43 à -49), prairies, verger (28.3,28.3)) — au
        // meilleur effort, pas un compactage garanti sans chevauchement.
        const LANDE: &[(&str, f32, f32, f32)] = &[
            ("Rocher de lande", -38.0, 5.0, 0.0),
            ("Rocher de lande 2", 36.0, -20.0, 40.0),
            ("Arbre mort tourmenté", -34.0, -25.0, 20.0),
            ("Ossements épars", -30.0, 12.0, 60.0),
            ("Menhir de lande", 38.0, 8.0, 0.0),
            ("Broussaille épineuse", -40.0, -8.0, 0.0),
            ("Broussaille épineuse 2", 30.0, -32.0, 90.0),
            ("Mare stagnante", -36.0, 24.0, 0.0),
            ("Ravine de terrain", -25.0, -36.0, 30.0),
            ("Poteau de bannière en ruine", 34.0, 30.0, 0.0),
            ("Cairn de guerre", -20.0, 38.0, 0.0),
            ("Touffe de brume basse", -32.0, -18.0, 0.0),
            ("Touffe de brume basse 2", 20.0, -40.0, 0.0),
        ];
        for (name, x, z, yaw) in LANDE {
            let file = match *name {
                "Rocher de lande" | "Rocher de lande 2" => "siege_moor_rock.glb",
                "Arbre mort tourmenté" => "siege_dead_tree.glb",
                "Ossements épars" => "siege_scattered_bones.glb",
                "Menhir de lande" => "siege_menhir.glb",
                "Broussaille épineuse" | "Broussaille épineuse 2" => "siege_thorny_scrub.glb",
                "Mare stagnante" => "siege_stagnant_pond.glb",
                "Ravine de terrain" => "siege_ravine.glb",
                "Poteau de bannière en ruine" => "siege_ruined_banner_post.glb",
                "Cairn de guerre" => "siege_war_cairn.glb",
                _ => "siege_low_mist.glb",
            };
            poser(
                &mut objects,
                &mut imported,
                name,
                file,
                *x,
                *z,
                1.0,
                *yaw,
                file != "siege_low_mist.glb",
            );
        }

        // --- Aînée de la lande (boss, GDD §4) : autel de mise en scène + cage
        // du chef, dans une clairière dégagée de la lande (loin du camp de
        // chasseurs/mare/prairies/verger). Tas de trophées près du camp de
        // chasseurs (mode Survie).
        poser(
            &mut objects,
            &mut imported,
            "Autel de l'Aînée",
            "siege_elder_altar.glb",
            42.4,
            42.4,
            1.0,
            0.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Cage du chef",
            "siege_chief_cage.glb",
            45.4,
            40.4,
            1.0,
            -30.0,
            true,
        );
        poser(
            &mut objects,
            &mut imported,
            "Tas de trophées du camp",
            "siege_trophy_pile.glb",
            22.0,
            44.0,
            1.0,
            0.0,
            false,
        );

        // Les Braises (GDD §2.1) sont la fiction du jeu : « c'est [le feu
        // communal] qui attire les hordes ». La charte (§10.1 « au centre,
        // les braises ; au loin, le danger », §10.2 orange = système
        // feu/joueur) exige que ce soit le point chaud/saturé le plus
        // lisible de la carte — jusqu'ici posé comme n'importe quel décor
        // inerte (pas d'émissif), cf. docs/SPRINT3D_AUDIT_GAMEDESIGN.md §1.1.
        if let Some(feu) = objects.iter_mut().find(|o| o.name == "Feu communal") {
            feu.emissive = 1.2;
        }

        Scene {
            objects,
            imported,
            camera_follow: true,
            point_lights: vec![
                PointLight {
                    position: [0.0, 20.0, 0.0],
                    color: [0.9, 0.95, 1.0],
                    intensity: 1.4,
                    range: 100.0,
                    ..PointLight::default()
                },
                PointLight {
                    position: [0.0, 4.0, 0.0],
                    color: [1.0, 0.75, 0.4],
                    intensity: 1.2,
                    range: 14.0,
                    ..PointLight::default()
                },
            ],
            mobile: MobileControls {
                joystick: true,
                buttons: vec!["Saut".into(), "Feu".into(), "Arme".into(), "Soin".into()],
                ..Default::default()
            },
            light: Light {
                dir: [0.55, 1.0, -0.45],
                color: [1.0, 0.96, 0.88],
                ambient: 0.35,
            },
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
            archetype: Archetype,
            /// PV de base, avant `Archetype::hp_multiplier` (GDD_MMORPG.md §5.4).
            hp: u32,
        }
        const GOBELIN: Kind = Kind {
            label: "Gobelin",
            speed: 3.2,
            dmg: 0.6,
            scale: 0.6,
            color: [0.35, 0.6, 0.3],
            archetype: Archetype::Meute,
            hp: 2,
        };
        const SQUELETTE: Kind = Kind {
            label: "Squelette",
            speed: 2.4,
            dmg: 1.0,
            scale: 0.85,
            color: [0.75, 0.72, 0.65],
            archetype: Archetype::Furtive,
            hp: 2,
        };
        const OGRE: Kind = Kind {
            label: "Ogre",
            speed: 1.6,
            dmg: 2.4,
            scale: 1.4,
            color: [0.4, 0.15, 0.15],
            archetype: Archetype::Colosse,
            hp: 2,
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
            m.ai_chaser = Some(AiChaser {
                speed: k.speed,
                archetype: k.archetype,
            });
            m.combat = Some(Combat {
                attackable: true,
                wave,
                // PV différenciés par archétype (GDD_MMORPG.md §5.4), cf.
                // `Archetype::hp_multiplier`.
                hp: ((k.hp as f32) * k.archetype.hp_multiplier())
                    .round()
                    .max(1.0) as u32,
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
        rival.ai_chaser = Some(AiChaser {
            speed: 2.8,
            ..Default::default()
        });
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

    /// Démo « Boss » (Phase C, Sprint 8 de `sprint10audit.md`, `RoundObjective::Boss`) :
    /// arène fermée, un unique adversaire à PV massifs, lent, contact doublé (GDD §4 :
    /// « dernière vague : une créature unique, PV massifs, lente, contact doublé » —
    /// archétype `Colosse`, cf. `GDD_MMORPG.md:368` « c'est aussi le boss »). Une seule
    /// manche (`Combat::wave: 1`) contenant le boss : `AppState::update_round` gagne la
    /// partie dès qu'elle est vidée (comportement `Vagues`, cf. sa doc), donc « mort du
    /// boss » et « dernière manche vidée » sont ici la même condition — pas de logique
    /// de victoire dédiée à écrire pour ce sprint, juste ce contenu.
    pub fn boss_demo() -> Self {
        let half = 10.0_f32;
        let mut imported: Vec<ImportedMesh> = Vec::new();

        let mut sol = demo_obj("Arène", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.14, 0.12, 0.16];
        sol.roughness = 0.7;

        let mut mur = demo_obj("Mur d'arène", MeshKind::Cube, Vec3::new(0.0, 2.0, -half));
        mur.transform = mur.transform.with_scale(Vec3::new(2.0 * half, 4.0, 0.6));
        mur.physics = PhysicsKind::Static;
        mur.color = [0.2, 0.18, 0.24];

        let mut joueur = demo_obj("Joueur", MeshKind::Capsule, Vec3::new(0.0, 1.0, 6.0));
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 4.5,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            attack_range: 1.6,
            attack_cooldown: 0.45,
            attack_windup: 0.15,
            ..Default::default()
        });

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.9, 0.6];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        // Modèle réel plutôt qu'un primitif (le GDD §4 nomme explicitement
        // l'archétype Colosse « yéti, dragon, roi-champignon, alpaking » —
        // GDD_MMORPG.md:368) : `monster_dragon_evolved.glb`, silhouette figée
        // sans squelette (comme tout `MONSTER_DECOR`, cf. sa doc — armature
        // retirée à l'export), suffisant pour un adversaire massif qui charge
        // plus qu'il n'anime. Repli sur une capsule si l'asset est introuvable.
        let boss_mesh = import_single_model(
            &mut imported,
            "monster_dragon_evolved.glb",
            MeshKind::Capsule,
        );
        let mut boss = demo_obj(
            "Boss — L'Aînée de la lande",
            boss_mesh,
            Vec3::new(0.0, 1.4, -4.0),
        );
        boss.transform = boss.transform.with_scale(Vec3::splat(2.2));
        boss.emissive = 0.3;
        boss.trigger = true;
        boss.ai_chaser = Some(AiChaser {
            // Lente (GDD §4) : l'archétype Colosse ralentit déjà la poursuite une fois
            // engagée (`Archetype::speed_multiplier`), une vitesse de base modeste la
            // garde lente même avant application du multiplicateur.
            speed: 1.8,
            archetype: Archetype::Colosse,
        });
        boss.combat = Some(Combat {
            attackable: true,
            wave: 1,
            // PV massifs (GDD §4) : très au-dessus du rival du Duel (`hp: 3`).
            hp: 15,
            ..Default::default()
        });
        boss.respawn_delay = 0.0;
        // Contact doublé (GDD §4) : deux fois le dégât de contact du rival du Duel
        // (`Scene::brawl_demo`, 0.9) — pattern d'attaque distinct par son intensité,
        // pas par un nouveau système. Pulse de teinte rouge (télégraphe la menace)
        // en plus de la couleur propre du modèle, pas à sa place (`color` reste
        // blanc = inchangée au repos, cf. `demo_obj`) — un tint fixe agressif
        // écraserait la texture du modèle importé en continu, pas seulement au pic.
        boss.script = "if obj.triggered then damage(1.8 * dt) end\n\
             local p = 0.5 + 0.5 * math.sin(time * 3.0)\n\
             obj.r = 1.0; obj.g = 1.0 - 0.5 * p; obj.b = 1.0 - 0.5 * p"
            .into();

        Scene {
            objects: vec![sol, mur, joueur, fx, boss],
            imported,
            camera_follow: true,
            game_camera: Some(GameCamera {
                target: [0.0, 1.5, 0.0],
                yaw: 0.0,
                pitch: 0.45,
                distance: 11.0,
            }),
            point_lights: vec![PointLight {
                position: [0.0, 6.0, -2.0],
                color: [0.7, 0.3, 0.9],
                intensity: 1.3,
                range: 20.0,
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

    /// Démo « Escorte » (Phase C, Sprint 7 de `sprint10audit.md`, `RoundObjective::Escorte`) :
    /// un convoi lent traverse un couloir d'une porte à l'autre (GDD §4) pendant que des
    /// créatures le prennent pour cible en priorité (cf. `AppState::update_escorte` et le
    /// ciblage prioritaire dans `AppState::advance_play`). Victoire à l'arrivée, défaite
    /// si le convoi est détruit avant (`AppState::is_room_lost`).
    pub fn escorte_demo() -> Self {
        let longueur = 40.0_f32;
        let mut imported: Vec<ImportedMesh> = Vec::new();

        let mut sol = demo_obj("Couloir", MeshKind::Plane, Vec3::new(0.0, 0.0, 0.0));
        sol.transform = sol
            .transform
            .with_scale(Vec3::new(8.0, 1.0, longueur + 8.0));
        sol.physics = PhysicsKind::Static;
        sol.color = [0.2, 0.18, 0.14];

        let mut joueur = demo_obj(
            "Joueur",
            MeshKind::Capsule,
            Vec3::new(-2.0, 1.0, -longueur / 2.0 + 2.0),
        );
        joueur.color = [0.9, 0.75, 0.3];
        joueur.controller = Some(Controller {
            input: true,
            move_speed: 5.0,
            jump_button: "Saut".into(),
            jump_height: 1.2,
            attack_button: "Attaque".into(),
            attack_range: 2.0,
            attack_cooldown: 0.4,
            attack_windup: 0.12,
            ..Default::default()
        });

        let mut fx = demo_obj("FX Attaque", MeshKind::Sphere, Vec3::ZERO);
        fx.color = [1.0, 0.9, 0.6];
        fx.emissive = 1.6;
        fx.combat = Some(Combat {
            is_attack_fx: true,
            ..Default::default()
        });
        fx.visible = false;

        // Modèle réel plutôt qu'un cube (« chariot lent », GDD §4) : même
        // asset que la démo « Charrette » du hameau (`nature_cart.glb`, décor
        // déjà utilisé ailleurs à l'échelle 1.0 — cf. `NATURE_DECOR`), repli
        // sur un cube si l'asset est introuvable.
        let convoi_mesh = import_single_model(&mut imported, "nature_cart.glb", MeshKind::Cube);
        let mut convoi = demo_obj(
            "Convoi — chariot de braises",
            convoi_mesh,
            Vec3::new(0.0, 0.0, -longueur / 2.0),
        );
        convoi.transform.rotation = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        convoi.emissive = 0.3;
        convoi.combat = Some(Combat {
            attackable: true,
            hp: 8,
            ..Default::default()
        });
        convoi.convoy = Some(Convoy {
            destination: Vec3::new(0.0, 0.0, longueur / 2.0),
            speed: 1.2,
        });

        // Créature réelle plutôt qu'une capsule teintée : silhouette figée
        // (comme `monster_dragon_evolved.glb` du boss, cf. sa doc) mais
        // suffisante pour un chasseur qui fonce en ligne droite (`AiChaser`),
        // sans animation de marche à proprement parler.
        let chasseresse_mesh =
            import_single_model(&mut imported, "monster_alien.glb", MeshKind::Capsule);
        let mut chasseresse = demo_obj("Chasseresse", chasseresse_mesh, Vec3::new(3.0, 0.0, 0.0));
        chasseresse.trigger = true;
        chasseresse.ai_chaser = Some(AiChaser {
            speed: 3.0,
            archetype: Archetype::Traqueuse,
        });
        chasseresse.combat = Some(Combat {
            attackable: true,
            hp: 2,
            ..Default::default()
        });
        chasseresse.respawn_delay = 0.0;
        chasseresse.script = "if obj.triggered then damage(0.8 * dt) end".into();

        Scene {
            objects: vec![sol, joueur, fx, convoi, chasseresse],
            imported,
            camera_follow: true,
            game_camera: Some(GameCamera {
                target: [0.0, 1.5, 0.0],
                yaw: 0.0,
                pitch: 0.5,
                distance: 10.0,
            }),
            point_lights: vec![PointLight {
                position: [0.0, 6.0, -longueur / 2.0],
                color: [1.0, 0.6, 0.3],
                intensity: 1.2,
                range: 24.0,
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
#[path = "tests.rs"]
mod tests;
