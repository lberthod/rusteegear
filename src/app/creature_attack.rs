//! Attaques à distance des créatures PNJ (pistolet à eau, crachat de feu,
//! étincelle, spore) — généralisation du module mono-créature `water_attack.rs`
//! né avec la Créature 3 : une table de configs (`RANGED_CREATURE_ATTACKS`),
//! chaque créature repérée par **nom d'objet** dans la scène, un état par
//! config (`AppState::creature_ranged`), un pool d'affichage partagé.
//!
//! Contrairement à `fireball.rs` (armes du joueur : le projectile part
//! instantanément sur pression du bouton), ces attaques font **s'arrêter** la
//! créature quelques instants avant de tirer (`windup`) — un vrai temps de
//! visée repérable en jeu, pas un tir en marchant — et ne se déclenchent
//! qu'occasionnellement (cooldown + tirage au sort), même esprit que
//! `scene::demos::creature_bite_script` (attaques au contact des n°1/6/7) mais
//! gérées ici **nativement** plutôt qu'en Lua : `fireball.rs` n'expose aucune
//! primitive de tir aux scripts, et en ajouter une (avec la parité
//! `scripting.rs`/`scripting_web.rs` que ce projet exige, cf. leur doc) aurait
//! été disproportionné pour des PNJ.
//!
//! Tirage déterministe (hachage de `time`, même idiome que le bruit de méandre
//! de `creature_wander_script`) plutôt qu'un RNG non seedé — cf. la doc de
//! `runtime::rng` sur le choix du projet en faveur d'un RNG reproductible :
//! un comportement testable exactement plutôt que probabiliste. Le `salt` par
//! config évite que deux créatures à portée au même tick roulent en lockstep.

use glam::{Quat, Vec3};

use super::AppState;
use crate::runtime::physics::PhysicsKind;

/// Profil d'attaque à distance d'une créature PNJ — l'équivalent côté PNJ des
/// `RangedWeapon` du joueur (`fireball::RANGED_WEAPONS`), avec en plus le
/// comportement (portée de déclenchement, cooldown, chance, temps de visée).
pub(super) struct RangedAttackConfig {
    /// Nom d'objet de scène visé (ex. « Créature 3 ») : la créature n'a rien à
    /// câbler dans son script, la correspondance se fait par nom — cohérent
    /// avec les scènes/exports où les créatures sont identifiées ainsi.
    pub(super) creature: &'static str,
    /// Portée (m) au-delà de laquelle la créature ne tente pas de tirer.
    pub(super) range: f32,
    /// Temps minimal (s) entre deux tentatives, réussies ou non.
    pub(super) cooldown: f32,
    /// Probabilité qu'une tentative à portée se concrétise réellement.
    pub(super) chance: f32,
    /// Temps (s) où la créature reste arrêtée, visée figée, avant le tir.
    pub(super) windup: f32,
    /// Vitesse de vol (m/s).
    pub(super) speed: f32,
    /// Durée de vie (s) ⇒ portée de vol max ≈ `speed × lifetime`.
    pub(super) lifetime: f32,
    /// Rayon (m) de détection d'impact autour du joueur, et taille affichée.
    pub(super) radius: f32,
    /// Vie retirée par impact.
    pub(super) damage: f32,
    /// Couleur du projectile (sphère émissive du pool).
    pub(super) color: [f32; 3],
    /// Décalage de phase du tirage déterministe (cf. la doc du module).
    pub(super) salt: f32,
    /// Forme du tir (cf. `AttackStyle`) — c'est elle qui différencie vraiment
    /// les attaques entre elles, au-delà des chiffres.
    pub(super) style: AttackStyle,
}

/// Forme d'une attaque à distance — ce qui se passe quand la visée aboutit.
/// Chaque variante donne une **mécanique** différente à esquiver, pas juste
/// des stats différentes : c'est la demande gameplay des créatures 11-15
/// (« que des attaques à distance différentes »).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum AttackStyle {
    /// Un projectile droit vers le joueur (les créatures 3/8/9/10 historiques).
    Single,
    /// `count` projectiles simultanés en éventail horizontal de `spread_deg`
    /// degrés, centré sur le joueur — s'esquive latéralement, mais pas au
    /// dernier moment.
    Fan { count: u32, spread_deg: f32 },
    /// `count` projectiles successifs espacés d'`interval` s, chacun re-visé
    /// sur la position courante du joueur — la créature reste figée jusqu'au
    /// dernier tir (cf. `update_creature_ranged_attacks`), il faut *rester* en
    /// mouvement pour tous les éviter.
    Burst { count: u32, interval: f32 },
    /// Projectile lent qui vire vers le joueur (`turn_rate` rad/s) : il ne
    /// s'esquive pas, il se sème (vitesse faible mais trajectoire têtue).
    Homing { turn_rate: f32 },
    /// Tir en cloche : part à l'horizontale vers le joueur avec une vitesse
    /// verticale calculée pour retomber sur lui (`gravity` m/s²) — l'ombre au
    /// sol est le seul préavis, on esquive en ne restant pas où on était.
    Lob { gravity: f32 },
    /// Anneau de `count` projectiles dans toutes les directions horizontales —
    /// aucune visée : c'est une zone de déni autour de la créature, on s'en
    /// éloigne pendant qu'elle « se gonfle » (windup).
    Nova { count: u32 },
}

/// Les attaques à distance des créatures du jeu. Ordre = indice dans
/// `AppState::creature_ranged` (état par config).
pub(super) const RANGED_CREATURE_ATTACKS: &[RangedAttackConfig] = &[
    // Créature 3 (bipède à pousse-feuille) : pistolet à eau — le profil
    // historique du module, équilibré.
    RangedAttackConfig {
        creature: "Créature 3",
        range: 7.0,
        cooldown: 4.0,
        chance: 0.5,
        windup: 0.5,
        speed: 9.0,
        lifetime: 1.4,
        radius: 0.75,
        damage: 0.15,
        color: [0.25, 0.65, 0.95],
        salt: 17.31,
        style: AttackStyle::Single,
    },
    // Créature 8 (salamandre) : crachat de feu — plus rare, plus rapide, plus
    // douloureux.
    RangedAttackConfig {
        creature: "Créature 8",
        range: 6.0,
        cooldown: 5.0,
        chance: 0.5,
        windup: 0.4,
        speed: 11.0,
        lifetime: 1.2,
        radius: 0.7,
        damage: 0.2,
        color: [1.0, 0.45, 0.1],
        salt: 23.17,
        style: AttackStyle::Single,
    },
    // Créature 9 (souris électrique) : étincelle — nerveuse, fréquente, petite
    // et faible ; harcèle plus qu'elle ne punit.
    RangedAttackConfig {
        creature: "Créature 9",
        range: 5.0,
        cooldown: 3.0,
        chance: 0.6,
        windup: 0.3,
        speed: 16.0,
        lifetime: 0.8,
        radius: 0.5,
        damage: 0.1,
        color: [1.0, 0.9, 0.2],
        salt: 29.53,
        style: AttackStyle::Single,
    },
    // Créature 10 (limace-champignon) : spore — lente, longue visée, rare,
    // mais la plus punitive si on la laisse arriver.
    RangedAttackConfig {
        creature: "Créature 10",
        range: 6.0,
        cooldown: 6.0,
        chance: 0.5,
        windup: 0.7,
        speed: 6.0,
        lifetime: 1.8,
        radius: 0.9,
        damage: 0.25,
        color: [0.5, 0.85, 0.3],
        salt: 31.77,
        style: AttackStyle::Single,
    },
    // Créature 11 (golem de cristal) : éventail de 3 éclats — une sentinelle
    // qui couvre un arc, pas un point.
    RangedAttackConfig {
        creature: "Créature 11",
        range: 7.0,
        cooldown: 5.0,
        chance: 0.55,
        windup: 0.6,
        speed: 10.0,
        lifetime: 1.2,
        radius: 0.6,
        damage: 0.12,
        color: [0.45, 0.85, 1.0],
        salt: 37.19,
        style: AttackStyle::Fan {
            count: 3,
            spread_deg: 50.0,
        },
    },
    // Créature 12 (félin d'ombre) : rafale de 3 tirs re-visés — punit
    // l'immobilité, chaque tir est faible mais la rafale complète fait mal.
    RangedAttackConfig {
        creature: "Créature 12",
        range: 6.5,
        cooldown: 4.5,
        chance: 0.6,
        windup: 0.35,
        speed: 13.0,
        lifetime: 1.0,
        radius: 0.5,
        damage: 0.08,
        color: [0.6, 0.3, 0.9],
        salt: 41.41,
        style: AttackStyle::Burst {
            count: 3,
            interval: 0.22,
        },
    },
    // Créature 13 (méduse) : orbe à tête chercheuse — lente mais têtue, on la
    // sème en courant, on ne l'esquive pas d'un pas de côté.
    RangedAttackConfig {
        creature: "Créature 13",
        range: 8.0,
        cooldown: 6.0,
        chance: 0.5,
        windup: 0.8,
        speed: 4.5,
        lifetime: 3.0,
        radius: 0.6,
        damage: 0.18,
        color: [0.95, 0.6, 0.9],
        salt: 43.77,
        style: AttackStyle::Homing { turn_rate: 1.6 },
    },
    // Créature 14 (escargot-mortier) : obus en cloche — longue portée, long
    // préavis, gros dégâts là où on *était*.
    RangedAttackConfig {
        creature: "Créature 14",
        range: 9.0,
        cooldown: 7.0,
        chance: 0.6,
        windup: 0.9,
        speed: 6.0,
        lifetime: 3.0,
        radius: 0.8,
        damage: 0.22,
        color: [0.8, 0.65, 0.3],
        salt: 47.23,
        style: AttackStyle::Lob { gravity: 9.81 },
    },
    // Créature 15 (oursin-étoile) : nova de 8 pointes — courte portée, zone de
    // déni tout autour, on s'écarte pendant le gonflement.
    RangedAttackConfig {
        creature: "Créature 15",
        range: 4.0,
        cooldown: 6.0,
        chance: 0.6,
        windup: 0.6,
        speed: 8.0,
        lifetime: 0.9,
        radius: 0.5,
        damage: 0.1,
        color: [1.0, 0.8, 0.3],
        salt: 53.11,
        style: AttackStyle::Nova { count: 8 },
    },
    // Créature 16 (griffon-vent) : bourrasque large en éventail, 5 rafales —
    // couvre un arc encore plus large que le golem (11), depuis les airs.
    RangedAttackConfig {
        creature: "Créature 16",
        range: 8.0,
        cooldown: 5.5,
        chance: 0.55,
        windup: 0.5,
        speed: 12.0,
        lifetime: 1.1,
        radius: 0.55,
        damage: 0.1,
        color: [0.85, 0.92, 0.98],
        salt: 59.37,
        style: AttackStyle::Fan {
            count: 5,
            spread_deg: 70.0,
        },
    },
    // Créature 17 (kraken-mini) : nova resserrée d'encre, 6 jets — courte
    // portée, moins de projectiles que l'oursin mais plus rapprochés.
    RangedAttackConfig {
        creature: "Créature 17",
        range: 3.5,
        cooldown: 5.5,
        chance: 0.6,
        windup: 0.5,
        speed: 7.0,
        lifetime: 0.7,
        radius: 0.45,
        damage: 0.09,
        color: [0.75, 0.35, 0.85],
        salt: 61.83,
        style: AttackStyle::Nova { count: 6 },
    },
    // Créature 18 (ver des sables) : rafale de 4 piques très rapprochées —
    // le « rush » de surface s'accompagne d'une salve courte et dense.
    RangedAttackConfig {
        creature: "Créature 18",
        range: 5.5,
        cooldown: 5.0,
        chance: 0.6,
        windup: 0.3,
        speed: 14.0,
        lifetime: 0.7,
        radius: 0.45,
        damage: 0.07,
        color: [0.8, 0.65, 0.35],
        salt: 67.21,
        style: AttackStyle::Burst {
            count: 4,
            interval: 0.15,
        },
    },
    // Créature 19 (lanterne-fantôme) : follet à tête chercheuse, très lent à
    // virer mais increvable — plus sournois que l'orbe de la méduse (13).
    RangedAttackConfig {
        creature: "Créature 19",
        range: 9.0,
        cooldown: 7.0,
        chance: 0.5,
        windup: 0.9,
        speed: 3.5,
        lifetime: 4.0,
        radius: 0.55,
        damage: 0.16,
        color: [0.55, 0.9, 0.85],
        salt: 71.53,
        style: AttackStyle::Homing { turn_rate: 1.0 },
    },
    // Créature 20 (tortue-canon) : obus en cloche courte et vive — portée
    // plus courte et arc plus bas que l'escargot (14), tourelle qui riposte vite.
    RangedAttackConfig {
        creature: "Créature 20",
        range: 6.0,
        cooldown: 5.5,
        chance: 0.6,
        windup: 0.5,
        speed: 9.0,
        lifetime: 1.5,
        radius: 0.65,
        damage: 0.14,
        color: [0.35, 0.55, 0.25],
        salt: 73.89,
        style: AttackStyle::Lob { gravity: 12.0 },
    },
];

/// Hauteur (m) au-dessus du sol à laquelle un projectile part et vole (buste
/// de la créature/du joueur, pas le sol).
const SPAWN_UP: f32 = 0.5;

/// État d'une config de `RANGED_CREATURE_ATTACKS` (même indice).
#[derive(Default)]
pub(super) struct RangedState {
    /// Cooldown restant (s) avant la prochaine tentative.
    cooldown: f32,
    /// `Some(échéance)` (en `self.time`) tant que la créature est arrêtée pour
    /// viser : position gelée, animation forcée sur `Idle`.
    pub(super) stopped_until: Option<f32>,
    /// Position gelée pendant la visée, capturée au moment où elle commence.
    pub(super) frozen_pos: Option<Vec3>,
    /// Tirs restants d'une rafale en cours (`AttackStyle::Burst`) : tant que
    /// ce compteur est positif, la créature reste figée et `stopped_until`
    /// porte l'échéance du prochain tir, pas celle de la visée initiale.
    burst_left: u32,
}

/// Un vecteur d'états alignés sur `RANGED_CREATURE_ATTACKS` (l'initialisation
/// de `AppState::creature_ranged`).
pub(super) fn default_states() -> Vec<RangedState> {
    RANGED_CREATURE_ATTACKS
        .iter()
        .map(|_| RangedState::default())
        .collect()
}

/// Projectile de créature en vol (cf. `AppState::creature_shots`).
pub(super) struct CreatureShot {
    pub(super) pos: Vec3,
    pub(super) dir: Vec3,
    remaining: f32,
    /// Indice dans `RANGED_CREATURE_ATTACKS` : décide vitesse, dégâts, rayon
    /// et couleur pendant toute la vie du projectile.
    pub(super) cfg: usize,
    /// Vitesse verticale (m/s) — seuls les obus `AttackStyle::Lob` en ont une
    /// (intégrée avec leur gravité à chaque pas) ; 0 pour tous les autres, qui
    /// volent le long de `dir` sans jamais en dévier verticalement.
    vvel: f32,
}

/// Hachage déterministe de `time` en [0, 1) — cf. la doc du module.
fn deterministic_roll(time: f32, salt: f32) -> f32 {
    let x = (time * salt).sin() * 43_758.547;
    x - x.floor()
}

impl AppState {
    /// Fait vivre les attaques à distance des créatures pour ce pas **fixe** :
    /// pour chaque config, décide si sa créature s'arrête pour tirer, gèle sa
    /// position/animation pendant qu'elle vise, fait partir le projectile à la
    /// fin de la visée ; puis avance tous les projectiles en vol et résout
    /// leurs impacts sur le joueur. Appelée depuis `AppState::sim_step`,
    /// **après** la boucle des scripts (dont le déplacement de patrouille est
    /// annulé ce tick-ci pour une créature arrêtée) et **avant**
    /// `Physics::resolve_scripted_moves`/`step` — la position gelée doit être
    /// celle que la physique et le rendu voient ce tick.
    /// Ré-ancre la position gelée de chaque créature en pleine visée sur la
    /// position **réellement atteinte** ce tick — à appeler après
    /// `Physics::resolve_scripted_moves`. Sans ça, une créature bousculée
    /// pendant sa visée (dépénétration : une autre lui marche dessus) gardait
    /// son ancrage d'origine et y était **catapultée** dès que le passage se
    /// libérait — un « gros saut » observé en jeu (cf. la preuve
    /// `mmorpg_creatures_never_teleport_nor_snap_turn`). Geler veut dire « ne
    /// marche pas pendant la visée », pas « élastique vers le point de capture ».
    pub(super) fn refresh_frozen_anchors(&mut self) {
        for (ci, cfg) in RANGED_CREATURE_ATTACKS.iter().enumerate() {
            let state = &mut self.creature_ranged[ci];
            if state.stopped_until.is_none() || state.frozen_pos.is_none() {
                continue;
            }
            if let Some(obj) = self.scene.objects.iter().find(|o| o.name == cfg.creature) {
                state.frozen_pos = Some(obj.transform.position);
            }
        }
    }

    pub(super) fn update_creature_ranged_attacks(&mut self, dt: f32, time: f32) {
        if self.is_online_client() {
            // Autorité serveur (même pattern que `fireball::update_fireballs`) :
            // aucune simulation ni dégât local, uniquement l'affichage des
            // projectiles reçus par `Snapshot` — sinon le déplacement/visée
            // tournerait deux fois (un vrai côté serveur, un fantôme ici).
            let shots: Vec<(Vec3, Vec3, usize)> = self.net_creature_shots.clone();
            self.sync_creature_shot_pool(&shots);
            return;
        }
        let player_pos = self.player_position();

        for (ci, cfg) in RANGED_CREATURE_ATTACKS.iter().enumerate() {
            let Some(creature_idx) = self
                .scene
                .objects
                .iter()
                .position(|o| o.name == cfg.creature)
            else {
                continue;
            };
            if !self.scene.objects[creature_idx].visible {
                continue;
            }

            if let Some(deadline) = self.creature_ranged[ci].stopped_until {
                // Arrêtée : gèle la position (annule le déplacement que le
                // script de patrouille a quand même calculé ce tick) et force
                // `Idle`.
                if let Some(frozen) = self.creature_ranged[ci].frozen_pos {
                    self.scene.objects[creature_idx].transform.position = frozen;
                }
                if let Some(anim) = self.scene.objects[creature_idx].animation.as_mut() {
                    anim.set_clip("Idle");
                }
                if time >= deadline {
                    // Visée terminée (ou tir suivant d'une rafale) : la ou les
                    // munitions partent selon le style, visées sur la position
                    // **actuelle** du joueur — les projectiles simples restent
                    // figés sur leur trajectoire (pas des têtes chercheuses,
                    // sauf `Homing` qui vire *en vol*).
                    if let Some(p) = player_pos {
                        let origin = self.scene.objects[creature_idx].transform.position
                            + Vec3::Y * SPAWN_UP;
                        self.fire_creature_attack(ci, origin, p);
                        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Jump);
                    }
                    // Rafale : reste figée jusqu'au tir suivant ; sinon libérée.
                    if let AttackStyle::Burst { count, interval } = cfg.style {
                        let state = &mut self.creature_ranged[ci];
                        if state.burst_left == 0 {
                            // Premier tir de la rafale (la visée initiale vient
                            // d'aboutir) : arme les suivants.
                            state.burst_left = count.saturating_sub(1);
                        } else {
                            state.burst_left -= 1;
                        }
                        if state.burst_left > 0 {
                            state.stopped_until = Some(time + interval);
                            continue;
                        }
                    }
                    self.creature_ranged[ci].stopped_until = None;
                    self.creature_ranged[ci].frozen_pos = None;
                }
            } else {
                self.creature_ranged[ci].cooldown =
                    (self.creature_ranged[ci].cooldown - dt).max(0.0);
                if self.creature_ranged[ci].cooldown <= 0.0 {
                    let creature_pos = self.scene.objects[creature_idx].transform.position;
                    let in_range =
                        player_pos.is_some_and(|p| p.distance(creature_pos) <= cfg.range);
                    if in_range {
                        // Tentative consommée qu'elle réussisse ou non : sans
                        // ça, rester à portée avec un cooldown à 0 referait le
                        // tirage chaque tick (60/s), rendant `chance` illusoire
                        // — même garde-fou que `creature_bite_script`.
                        self.creature_ranged[ci].cooldown = cfg.cooldown;
                        if deterministic_roll(time, cfg.salt) < cfg.chance {
                            self.creature_ranged[ci].frozen_pos = Some(creature_pos);
                            self.creature_ranged[ci].stopped_until = Some(time + cfg.windup);
                        }
                    }
                }
            }
        }

        // Vol + impacts, toutes configs confondues.
        let mut shots = std::mem::take(&mut self.creature_shots);
        shots.retain_mut(|s| {
            s.remaining -= dt;
            if s.remaining <= 0.0 {
                return false;
            }
            let cfg = &RANGED_CREATURE_ATTACKS[s.cfg];
            match cfg.style {
                // Obus : trajectoire balistique — horizontale via `dir`,
                // verticale intégrée séparément (cf. `CreatureShot::vvel`).
                AttackStyle::Lob { gravity } => {
                    s.pos += s.dir * cfg.speed * dt + Vec3::Y * (s.vvel * dt);
                    s.vvel -= gravity * dt;
                    // Écrasé au sol sans avoir touché : le tir meurt là (pas de
                    // roulage le long du sol jusqu'au joueur).
                    if s.pos.y <= 0.0 {
                        return false;
                    }
                }
                // Tête chercheuse : vire vers le joueur, au plus `turn_rate`
                // rad/s — un virage borné, pas un aimant instantané.
                AttackStyle::Homing { turn_rate } => {
                    if let Some(p) = player_pos
                        && let Some(target) = (p + Vec3::Y * SPAWN_UP - s.pos).try_normalize()
                    {
                        let angle = s.dir.angle_between(target);
                        let max = turn_rate * dt;
                        s.dir = if angle <= max {
                            target
                        } else if let Some(axis) = s.dir.cross(target).try_normalize() {
                            Quat::from_axis_angle(axis, max) * s.dir
                        } else {
                            s.dir
                        };
                    }
                    s.pos += s.dir * cfg.speed * dt;
                }
                _ => s.pos += s.dir * cfg.speed * dt,
            }
            true
        });
        shots.retain(|s| {
            let Some(p) = player_pos else { return true };
            let cfg = &RANGED_CREATURE_ATTACKS[s.cfg];
            let hit = s.pos.distance(p + Vec3::Y * SPAWN_UP) <= cfg.radius;
            if hit {
                self.hud_health = self.hud_health.map(|h| (h - cfg.damage).max(0.0));
                self.damage_flash = 1.0;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
            }
            !hit
        });
        self.creature_shots = shots;

        // Pool d'affichage : projectiles simulés ici (solo/serveur) — le chemin
        // client connecté est déjà retourné plus haut avant d'atteindre ce point.
        let shots: Vec<(Vec3, Vec3, usize)> = self
            .creature_shots
            .iter()
            .map(|s| (s.pos, s.dir, s.cfg))
            .collect();
        self.sync_creature_shot_pool(&shots);
    }

    /// Fait partir la ou les munitions de la config `ci` depuis `origin`
    /// (buste de la créature) vers le joueur en `player` — le dispatch par
    /// `AttackStyle` : un tir droit (Single/Homing/Burst — la répétition de la
    /// rafale est gérée par l'appelant via `RangedState::burst_left`), un
    /// éventail (Fan), un anneau complet (Nova), ou un obus balistique (Lob).
    fn fire_creature_attack(&mut self, ci: usize, origin: Vec3, player: Vec3) {
        let cfg = &RANGED_CREATURE_ATTACKS[ci];
        let target = player + Vec3::Y * SPAWN_UP;
        let mut push = |dir: Vec3, vvel: f32| {
            self.creature_shots.push(CreatureShot {
                pos: origin,
                dir,
                remaining: cfg.lifetime,
                cfg: ci,
                vvel,
            });
        };
        // Direction horizontale vers le joueur — base des styles qui visent au
        // sol (Fan/Nova/Lob) ; `Vec3::NEG_Z` en secours si le joueur est pile
        // sur la créature (direction indéfinie).
        let flat = (player - origin).with_y(0.0);
        let flat_dir = flat.try_normalize().unwrap_or(Vec3::NEG_Z);
        match cfg.style {
            AttackStyle::Single | AttackStyle::Burst { .. } | AttackStyle::Homing { .. } => {
                if let Some(dir) = (target - origin).try_normalize() {
                    push(dir, 0.0);
                }
            }
            AttackStyle::Fan { count, spread_deg } => {
                let spread = spread_deg.to_radians();
                for k in 0..count {
                    // Éventail centré sur le joueur : offsets répartis dans
                    // [-spread/2, +spread/2] (un seul tir part droit).
                    let t = if count > 1 {
                        k as f32 / (count - 1) as f32 - 0.5
                    } else {
                        0.0
                    };
                    push(Quat::from_rotation_y(t * spread) * flat_dir, 0.0);
                }
            }
            AttackStyle::Nova { count } => {
                for k in 0..count {
                    let angle = std::f32::consts::TAU * k as f32 / count as f32;
                    push(Quat::from_rotation_y(angle) * flat_dir, 0.0);
                }
            }
            AttackStyle::Lob { gravity } => {
                // Temps de vol horizontal jusqu'au joueur à vitesse fixe, puis
                // vitesse verticale pour retomber au même niveau à l'arrivée
                // (cloche symétrique : `vvel = g·T/2`). Borne basse sur T :
                // un joueur collé à la créature donnerait un obus sans cloche.
                let t_flight = (flat.length() / cfg.speed).max(0.3);
                push(flat_dir, 0.5 * gravity * t_flight);
            }
        }
    }

    /// Aligne le pool d'affichage (une sphère étirée par projectile en vol,
    /// orientée dans le sens du vol pour lire comme un jet plutôt qu'une simple
    /// bille, colorée selon l'attaque d'origine) — même principe que
    /// `fireball::sync_fireball_pool`. Accepte une liste externe de (position,
    /// direction, config) plutôt que de toujours lire `self.creature_shots` :
    /// côté client connecté, l'appelant passe `net_creature_shots` (reçus du
    /// `Snapshot`) à la place, `self.creature_shots` y restant vide.
    fn sync_creature_shot_pool(&mut self, shots: &[(Vec3, Vec3, usize)]) {
        while self.creature_shot_pool.len() < shots.len() {
            let index = self.scene.objects.len();
            self.scene.objects.push(crate::scene::SceneObject {
                name: format!("Tir de créature {}", self.creature_shot_pool.len() + 1),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::ZERO),
                emissive: 1.6,
                physics: PhysicsKind::None,
                visible: false,
                ..Default::default()
            });
            self.creature_shot_pool.push(index);
        }
        for (slot, &index) in self.creature_shot_pool.iter().enumerate() {
            if let Some(o) = self.scene.objects.get_mut(index) {
                match shots.get(slot) {
                    Some(&(pos, dir, cfg_idx)) => {
                        let cfg = &RANGED_CREATURE_ATTACKS
                            [cfg_idx.min(RANGED_CREATURE_ATTACKS.len() - 1)];
                        o.transform.position = pos;
                        o.transform.scale =
                            Vec3::new(cfg.radius * 0.5, cfg.radius * 0.5, cfg.radius);
                        o.transform.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
                        o.color = cfg.color;
                        o.visible = true;
                    }
                    None => o.visible = false,
                }
            }
        }
    }

    /// Oublie les projectiles en vol, les états d'arrêt/visée et le pool
    /// d'affichage — mêmes sites d'appel que `clear_fireballs` (le pool vit
    /// dans `scene.objects`, ses indices deviennent obsolètes après une
    /// restauration en bloc, ex. retour Edit <-> Play).
    pub(super) fn clear_creature_shots(&mut self) {
        self.creature_shots.clear();
        self.creature_shot_pool.clear();
        for state in &mut self.creature_ranged {
            *state = RangedState::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_index(creature: &str) -> usize {
        RANGED_CREATURE_ATTACKS
            .iter()
            .position(|c| c.creature == creature)
            .expect("créature absente de RANGED_CREATURE_ATTACKS")
    }

    fn indices(app: &crate::app::AppState, creature: &str) -> (usize, usize) {
        let creature_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == creature)
            .unwrap_or_else(|| panic!("la démo MMORPG doit contenir « {creature} »"));
        let player_idx = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Joueur")
            .expect("la démo MMORPG doit contenir un « Joueur »");
        (creature_idx, player_idx)
    }

    /// Preuve de la demande gameplay « la Créature 3 tire de l'eau et le fait
    /// parfois » : à portée continue du joueur pendant 30 s, elle doit tirer au
    /// moins une fois, mais pas à chaque frame — même propriété que
    /// `creature_1_bites_the_player_sometimes_not_on_every_contact_tick`
    /// (`app::simulation::tests`), pour la même raison (cooldown + tirage, pas
    /// un flux continu). Les dégâts observés mêlent les jets de la n°3 et les
    /// éventuelles autres attaques de créatures croisées : on ne borne donc que
    /// le nombre de tirs de la n°3 réellement partis (`creature_shots`).
    #[test]
    fn creature_3_shoots_water_sometimes_when_in_range() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let (creature_idx, player_idx) = indices(&app, "Créature 3");
        let ci = cfg_index("Créature 3");

        let start = app.scene.objects[creature_idx].transform.position;
        app.scene.objects[player_idx].transform.position = start;
        app.hud_health = Some(1.0);
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        app.physics
            .as_mut()
            .unwrap()
            .set_position(player_idx, start);

        let dt = 1.0 / 60.0;
        let mut aims = 0u32;
        let mut was_aiming = false;
        for _ in 0..(30 * 60) {
            app.sim_step(dt);
            let aiming = app.creature_ranged[ci].stopped_until.is_some();
            if aiming && !was_aiming {
                aims += 1;
            }
            was_aiming = aiming;

            // Portée permanente : replace le joueur sur la créature (qui
            // continue de patrouiller entre deux visées) avant le pas suivant.
            let pos = app.scene.objects[creature_idx].transform.position;
            app.physics.as_mut().unwrap().set_position(player_idx, pos);
            app.scene.objects[player_idx].transform.position = pos;
        }

        assert!(
            aims > 0,
            "30 s à portée continue auraient dû déclencher au moins une visée"
        );
        assert!(
            aims < 10,
            "{aims} visées en 30 s pour un cooldown de 4 s — l'attaque semble se \
             déclencher en continu plutôt que « parfois »"
        );
    }

    /// Contre-épreuve de portée : hors de portée, une créature à attaque à
    /// distance ne doit jamais viser ni tirer — déterministe (aucun hasard
    /// toléré, contrairement au test précédent).
    #[test]
    fn ranged_creatures_never_shoot_out_of_range() {
        // Une config à la fois, chacune dans son propre `AppState` : avec 20
        // créatures scattées sur l'arène, aucun coin partagé n'est plus assez
        // loin de **toutes** les portées à la fois (une créature 16-20 finit
        // toujours par retomber dans le rayon d'une autre) — tester chaque
        // config isolément est plus simple et reste tout aussi précis : le
        // joueur est placé à `range + marge` de sa seule créature concernée,
        // repoussé à chaque tick (elle comme lui) contre la patrouille/dérive.
        for (ci, cfg) in RANGED_CREATURE_ATTACKS.iter().enumerate() {
            let mut app = AppState::new();
            app.scene = crate::scene::Scene::mmorpg_demo();
            let (creature_idx, player_idx) = indices(&app, cfg.creature);
            app.hud_health = Some(1.0);
            app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

            let spawn = app.scene.objects[creature_idx].transform.position;
            let far = spawn + Vec3::new(cfg.range + 5.0, 0.0, 0.0);
            app.scene.objects[player_idx].transform.position = far;
            app.physics.as_mut().unwrap().set_position(player_idx, far);
            // Neutralise les 19 autres créatures pour ce tour : le point
            // « loin » de la créature testée peut tomber par hasard à portée
            // d'une voisine (arène dense) — seule la config `ci` nous intéresse.
            for (i, state) in app.creature_ranged.iter_mut().enumerate() {
                if i != ci {
                    state.cooldown = 1000.0;
                }
            }

            let dt = 1.0 / 60.0;
            for step in 0..(3 * 60) {
                app.sim_step(dt);
                assert!(
                    app.creature_shots.is_empty(),
                    "{} — step {step} : hors de portée, aucun projectile ne devrait partir",
                    cfg.creature
                );
                assert!(
                    app.creature_ranged[ci].stopped_until.is_none(),
                    "{} — step {step} : hors de portée, ne devrait jamais viser",
                    cfg.creature
                );
                app.physics
                    .as_mut()
                    .unwrap()
                    .set_position(creature_idx, spawn);
                app.scene.objects[creature_idx].transform.position = spawn;
                app.physics.as_mut().unwrap().set_position(player_idx, far);
                app.scene.objects[player_idx].transform.position = far;
            }
        }
    }

    /// Preuve du « elle s'arrête » : pendant la fenêtre de visée, la position
    /// de la créature reste figée malgré son script de patrouille qui continue
    /// de tourner, et son animation est forcée sur `Idle`. Déclenché
    /// directement (sans dépendre du tirage probabiliste) pour un test exact.
    #[test]
    fn a_ranged_creature_freezes_in_place_while_aiming() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let (creature_idx, _) = indices(&app, "Créature 3");
        let ci = cfg_index("Créature 3");
        app.hud_health = Some(1.0);
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

        let frozen_at = app.scene.objects[creature_idx].transform.position;
        app.creature_ranged[ci].frozen_pos = Some(frozen_at);
        app.creature_ranged[ci].stopped_until = Some(app.time + 1.0);

        let dt = 1.0 / 60.0;
        for step in 0..30 {
            app.sim_step(dt);
            let pos = app.scene.objects[creature_idx].transform.position;
            // Horizontal seulement : un micro-ajustement vertical (snap au sol
            // du contrôleur cinématique) est normal et hors sujet — ce qui
            // compte, c'est qu'elle ne *marche* plus.
            let horiz = (pos - frozen_at).with_y(0.0).length();
            assert!(
                horiz < 0.05,
                "step {step} : la créature a marché pendant qu'elle vise \
                 (position={pos:?}, attendu≈{frozen_at:?})"
            );
            let clip = app.scene.objects[creature_idx]
                .animation
                .as_ref()
                .map(|a| a.clip.as_str());
            assert_eq!(
                clip,
                Some("Idle"),
                "step {step} : l'animation doit rester `Idle` pendant la visée"
            );
        }
    }

    /// Preuve du visuel, pour **chaque** attaque de la table : un projectile
    /// déclenché (visée forcée, pour ne pas dépendre du tirage) produit un
    /// objet visible du pool, à la couleur de son attaque — l'eau reste bleue,
    /// le feu orange, l'étincelle jaune, la spore verte, même si plusieurs
    /// créatures tirent dans la même partie.
    #[test]
    fn every_ranged_attack_fires_a_visible_shot_with_its_own_color() {
        for (ci, cfg) in RANGED_CREATURE_ATTACKS.iter().enumerate() {
            let mut app = AppState::new();
            app.scene = crate::scene::Scene::mmorpg_demo();
            let (creature_idx, player_idx) = indices(&app, cfg.creature);
            app.hud_health = Some(1.0);
            app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));

            // Joueur loin (hors du rayon d'impact) : le projectile doit voler
            // plusieurs ticks, laissant le temps de l'observer en vol.
            app.scene.objects[player_idx].transform.position =
                app.scene.objects[creature_idx].transform.position + Vec3::new(5.0, 0.0, 0.0);

            let frozen_at = app.scene.objects[creature_idx].transform.position;
            app.creature_ranged[ci].frozen_pos = Some(frozen_at);
            app.creature_ranged[ci].stopped_until = Some(app.time + 1.0 / 60.0);

            let dt = 1.0 / 60.0;
            app.sim_step(dt); // franchit l'échéance : la volée part
            app.sim_step(dt); // un pas de vol

            // Nombre de munitions de la **première volée**, selon le style —
            // c'est le test qui verrouille « des attaques différentes » : un
            // éventail en tire 3 d'un coup, une nova 8, une rafale une seule
            // (les suivantes sont testées par `burst_fires_shots_sequentially`).
            let expected = match cfg.style {
                AttackStyle::Fan { count, .. } | AttackStyle::Nova { count } => count as usize,
                _ => 1,
            };
            assert_eq!(
                app.creature_shots.len(),
                expected,
                "{} : volée initiale de {expected} projectile(s) attendue",
                cfg.creature
            );
            let sphere_idx = app.creature_shot_pool[0];
            let sphere = &app.scene.objects[sphere_idx];
            assert!(
                sphere.visible,
                "{} : la sphère du projectile doit être visible en vol",
                cfg.creature
            );
            assert_eq!(
                sphere.color, cfg.color,
                "{} : le projectile doit porter la couleur de son attaque",
                cfg.creature
            );
        }
    }

    /// Force la visée d'une config vers un joueur posé à `offset` de la
    /// créature, puis avance d'un tick : la première volée vient de partir.
    fn force_fire(cfg_name: &str, offset: Vec3) -> crate::app::AppState {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::mmorpg_demo();
        let (creature_idx, player_idx) = indices(&app, cfg_name);
        let ci = cfg_index(cfg_name);
        app.hud_health = Some(1.0);
        // Le joueur est placé **avant** de construire la physique : construite
        // d'abord, elle mémorisait la position de spawn et y ramenait le joueur
        // au premier `sim_step` — le test croyait viser un joueur à `offset` de
        // la créature alors qu'il était reparti à l'autre bout de l'arène.
        app.scene.objects[player_idx].transform.position =
            app.scene.objects[creature_idx].transform.position + offset;
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        let frozen_at = app.scene.objects[creature_idx].transform.position;
        app.creature_ranged[ci].frozen_pos = Some(frozen_at);
        app.creature_ranged[ci].stopped_until = Some(app.time + 1.0 / 60.0);
        app.sim_step(1.0 / 60.0);
        // Isole la config testée : le joueur posé près de la créature visée
        // est souvent à portée d'une voisine (l'arène est dense depuis les
        // créatures 11-15) — bloque toute nouvelle tentative (cooldown) ET
        // annule les visées que les voisines ont pu commencer pendant le tick
        // déjà simulé (leur tir partirait sinon en plein comptage).
        for (i, state) in app.creature_ranged.iter_mut().enumerate() {
            state.cooldown = 1000.0;
            if i != ci {
                state.stopped_until = None;
                state.frozen_pos = None;
                state.burst_left = 0;
            }
        }
        app
    }

    /// Preuve de la rafale (Créature 12) : les 3 tirs partent **échelonnés**
    /// (1 par échéance d'`interval`), pas d'un bloc, et la créature reste
    /// figée entre eux (c'est ce qui rend la rafale lisible : elle s'arrête,
    /// enchaîne ses tirs, puis repart).
    #[test]
    fn burst_fires_shots_sequentially_while_staying_frozen() {
        let ci = cfg_index("Créature 12");
        let AttackStyle::Burst { count, interval } = RANGED_CREATURE_ATTACKS[ci].style else {
            panic!("la Créature 12 doit tirer en rafale");
        };
        let mut app = force_fire("Créature 12", Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(app.creature_shots.len(), 1, "premier tir de la rafale");
        assert!(
            app.creature_ranged[ci].stopped_until.is_some(),
            "la créature doit rester figée entre deux tirs de rafale"
        );
        // Le joueur s'écarte de la ligne de tir : resté à 5 m, le 1er tir le
        // toucherait avant l'envol du 3e, et le comptage « 3 en vol » serait
        // invérifiable.
        let (_, player_idx) = indices(&app, "Créature 12");
        let dodge = app.scene.objects[player_idx].transform.position + Vec3::new(0.0, 0.0, 20.0);
        app.scene.objects[player_idx].transform.position = dodge;
        app.physics
            .as_mut()
            .unwrap()
            .set_position(player_idx, dodge);
        // Avance jusqu'à épuisement de la rafale : chaque échéance ajoute un tir.
        let dt = 1.0 / 60.0;
        let steps = ((interval * (count - 1) as f32 + 0.15) / dt) as u32;
        let mut max_seen = 1;
        for _ in 0..steps {
            app.sim_step(dt);
            max_seen = max_seen.max(app.creature_shots.len());
        }
        assert_eq!(
            max_seen, count as usize,
            "la rafale complète doit avoir mis {count} tirs en vol simultanément"
        );
        assert!(
            app.creature_ranged[ci].stopped_until.is_none(),
            "rafale finie : la créature doit être libérée"
        );
    }

    /// Preuve de la tête chercheuse (Créature 13) : l'orbe vire vers un joueur
    /// qui s'est déplacé après le tir — son cap final pointe nettement plus
    /// vers la nouvelle position qu'au départ.
    #[test]
    fn homing_shot_curves_toward_a_moved_player() {
        let mut app = force_fire("Créature 13", Vec3::new(6.0, 0.0, 0.0));
        assert_eq!(app.creature_shots.len(), 1);
        let (_, player_idx) = indices(&app, "Créature 13");
        // Le joueur se décale perpendiculairement à la trajectoire initiale.
        let dodge = app.scene.objects[player_idx].transform.position + Vec3::new(0.0, 0.0, 4.0);
        app.scene.objects[player_idx].transform.position = dodge;
        app.physics
            .as_mut()
            .unwrap()
            .set_position(player_idx, dodge);

        let dir_before = app.creature_shots[0].dir;
        let dt = 1.0 / 60.0;
        for _ in 0..30 {
            app.sim_step(dt);
            if app.creature_shots.is_empty() {
                break; // il l'a déjà rattrapé — c'est le comportement voulu
            }
            // Le joueur reste sur place (repoussé à chaque tick, il patrouille
            // sinon au gré du contrôleur réseau/joystick absent — non, il est
            // statique ; ce repositionnement neutralise seulement la physique).
            app.scene.objects[player_idx].transform.position = dodge;
            app.physics
                .as_mut()
                .unwrap()
                .set_position(player_idx, dodge);
        }
        if let Some(shot) = app.creature_shots.first() {
            let to_player = (dodge + Vec3::Y * 0.5 - shot.pos).normalize();
            assert!(
                shot.dir.dot(to_player) > dir_before.dot(to_player) + 0.1,
                "l'orbe doit avoir viré vers la nouvelle position du joueur \
                 (avant {:?}, après {:?})",
                dir_before,
                shot.dir
            );
        }
    }

    /// Preuve de l'obus en cloche (Créature 14) : le tir **monte** d'abord
    /// (cloche), puis retombe et meurt au sol s'il n'a rien touché — jamais une
    /// ligne droite à hauteur de buste comme les autres styles.
    #[test]
    fn lob_shot_arcs_up_then_falls_to_the_ground() {
        let mut app = force_fire("Créature 14", Vec3::new(7.0, 0.0, 0.0));
        assert_eq!(app.creature_shots.len(), 1);
        let (_, player_idx) = indices(&app, "Créature 14");
        // Le joueur s'écarte : l'obus doit retomber dans le vide et s'éteindre.
        let away = app.scene.objects[player_idx].transform.position + Vec3::new(0.0, 0.0, 5.0);
        app.scene.objects[player_idx].transform.position = away;
        app.physics.as_mut().unwrap().set_position(player_idx, away);

        let start_y = app.creature_shots[0].pos.y;
        let mut peak = start_y;
        let dt = 1.0 / 60.0;
        for _ in 0..(4 * 60) {
            app.sim_step(dt);
            match app.creature_shots.first() {
                Some(shot) => peak = peak.max(shot.pos.y),
                None => break,
            }
            app.scene.objects[player_idx].transform.position = away;
            app.physics.as_mut().unwrap().set_position(player_idx, away);
        }
        assert!(
            peak > start_y + 0.5,
            "l'obus doit monter en cloche (départ {start_y:.2}, sommet {peak:.2})"
        );
        assert!(
            app.creature_shots.is_empty(),
            "l'obus esquivé doit s'être écrasé au sol et éteint"
        );
    }
}
