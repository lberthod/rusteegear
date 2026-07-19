use super::{ImportedMesh, MeshKind};

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
pub(super) fn creature_bite_script(
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
pub(super) fn creature_guard_script(
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
pub(super) fn creature_kite_script(
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
pub(super) fn creature_drift_script(
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
pub(super) fn creature_artillery_script(
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
pub(super) fn creature_zigzag_script(
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
pub(super) fn creature_soar_script(
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
pub(super) fn creature_lemniscate_script(
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
pub(super) fn creature_burrow_script(
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
pub(super) fn creature_hover_script(
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
pub(super) fn creature_turret_script(
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
pub(super) fn import_single_model(
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
