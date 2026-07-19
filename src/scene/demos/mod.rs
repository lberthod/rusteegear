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

mod creature_scripts;
use creature_scripts::*;

mod controller;

mod tower;

mod temple_run;

mod components;

mod zombies;

mod roguelike;

mod brawl;

mod boss;

mod escorte;

mod misc;

mod mmorpg;

mod hameau_gdd;

impl Scene {
    /// Démi-étendue (m) de la carte MMORPG : seule source de vérité de sa taille —
    /// sol, murs, bornes des scripts de créatures (`arena_half` passé aux
    /// générateurs) et gardes des tests (`simulation.rs`) en dérivent tous.
    /// 36.0 = carte 72×72 m (×3 l'arène d'origine de 24×24) pour loger les
    /// biomes : prairie centrale, forêt NE, lac et rivières à l'ouest, rizières
    /// SO, hameau et promontoire à l'est.
    pub(crate) const MMORPG_HALF: f32 = 36.0;
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
