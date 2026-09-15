//! Boss de fin de parcours de la démo Rivière & cascade (« L'Aîné de la
//! Cascade », 15 septembre 2026) : trois phases pilotées par le ratio de PV
//! (`Combat::hp / Combat::max_hp`, cf. sa doc) — vitesse de poursuite et
//! style/cadence de l'attaque à distance télégraphiée « Jet de la cascade » —
//! plus un loot garanti à la mort et une longue réapparition.
//!
//! ## Pourquoi un système séparé, plutôt qu'une extension de `creature_attack`
//!
//! `creature_attack::RANGED_CREATURE_ATTACKS` est une table **partagée**,
//! testée génériquement (`ranged_creatures_never_shoot_out_of_range`, `every_
//! ranged_attack_fires_a_visible_shot_with_its_own_color`...) : chaque entrée
//! y est résolue contre `Scene::mmorpg_demo()`, jamais `Scene::riviere_demo()`
//! — y ajouter le boss ferait paniquer ces tests (créature introuvable dans
//! la démo MMORPG). Le boss n'a par ailleurs qu'UNE seule créature dont
//! l'attaque doit changer de forme (Single → Fan → Nova) selon ses PV, un
//! besoin que la table (un style fixe par entrée) n'exprime pas nativement.
//! D'où un petit système dédié, qui reprend la même mécanique (créature
//! arrêtée pendant un `windup`, tirage déterministe par hachage de `time`,
//! pas de RNG non seedé — cf. `creature_attack::deterministic_roll`, réutilisé
//! tel quel) sans dupliquer la table ni ses tests.
//!
//! ## Réseau
//!
//! Position/visibilité/PV du boss (`Combat::hp/max_hp`, normalisé 0..1) sont
//! déjà diffusés à tous les clients comme n'importe quel monstre `attackable`
//! (cf. `AppState::network_snapshot`, `net::protocol::EntityDelta::health`) :
//! aucun changement de protocole n'est nécessaire pour la barre de vie HUD ni
//! pour la morsure au contact (`BiteAttack`/`creature_bite_script`, même
//! double résolution solo/réseau que le reste du bestiaire Rivière, cf.
//! `scene::demos::riviere`). **Limitation assumée** (partagée avec
//! `creature_attack.rs`, pas une régression introduite ici) : les dégâts de
//! l'attaque à distance ne visent que `AppState::player_position()` (le
//! joueur local), pas individuellement chaque joueur réseau — et le jet en
//! vol lui-même (position du projectile, pulse de teinte du télégraphe)
//! n'est ni diffusé ni rejoué sur les clients connectés (`is_online_client`),
//! qui ne voient donc pas le jet voler ni le boss rougir pendant sa visée —
//! seuls la position/l'issue (dégâts au joueur local, PV du boss) sont
//! autoritaires et visibles de tous. Un futur ajout de `EntityDelta` dédié
//! (position de projectile de boss, phase courante) lèverait cette
//! limitation si le playtest le juge nécessaire ; non fait ici pour ne pas
//! bumper `PROTOCOL_VERSION` sans besoin démontré.
//!
//! ## Loot garanti à la mort
//!
//! Aucun mécanisme de « loot à la mort » n'existait dans le moteur (butin
//! toujours posé au sol dès le chargement de la scène, cf. `riviere.rs`). Le
//! butin du boss (`BOSS_LOOT_NAME`) est donc posé dès la construction de
//! `Scene::riviere_demo`, `visible: false`, et sa visibilité est ensuite
//! purement **dérivée** de celle du boss à chaque pas fixe
//! (`update_boss_loot`, appelée par `update_boss`) : visible dès que le boss
//! ne l'est plus (vaincu), masqué dès que le boss l'est de nouveau
//! (réapparition). Un seul point d'accroche plutôt qu'une copie sur les 3
//! sites de résolution de dégâts existants (`attack_at`, `attack_zone_at`,
//! impact de projectile) — même idiome que `world_labels::
//! update_label_hit_flashes`, qui compare déjà l'état de vie d'un tick à
//! l'autre. Fonctionne à l'identique en solo et côté serveur : les deux
//! exécutent `sim_step_inner` (cf. `AppState::advance_play`,
//! `bin/server.rs`), et la visibilité du boss y est de toute façon
//! autoritaire (jamais lue depuis un `Snapshot` côté serveur).

use glam::{Quat, Vec3};

use super::creature_attack;
use super::AppState;
use crate::runtime::physics::PhysicsKind;

/// Nom de l'objet de scène du boss (`Scene::riviere_demo`) — sert aussi de
/// préfixe unique à son script de morsure (cf. `creature_bite_script`).
pub const BOSS_NAME: &str = "L'Aîné de la Cascade";
/// Nom de l'objet de butin garanti, posé masqué dès la construction de la
/// scène (cf. la doc du module).
pub const BOSS_LOOT_NAME: &str = "Arc de l'Aînée";

/// Hauteur (m) au-dessus du sol à laquelle un jet part et vole (buste du
/// boss/du joueur) — même valeur que `creature_attack::SPAWN_UP`, dupliquée
/// (constante privée là-bas) plutôt que rendue publique pour un seul usage.
const SPAWN_UP: f32 = 0.5;
/// Décalage de phase du tirage déterministe (cf. `deterministic_roll`) —
/// choisi loin des `salt` de `RANGED_CREATURE_ATTACKS` (17..74) pour qu'un
/// même `time` ne fasse jamais rouler les deux systèmes en lockstep.
const BOSS_SALT: f32 = 97.13;
/// Pause de récupération (s) après la Nova de la phase « Dernier sursaut » :
/// le boss reste figé (ne poursuit pas), une fenêtre de riposte volontaire
/// (cf. la doc du design validé) plutôt qu'un DPS-check punitif.
const RECOVER_SECS: f32 = 1.2;

/// Les trois phases de combat, dérivées du seul ratio PV (recalculé chaque
/// tick depuis `Combat::hp/max_hp` — `BossState::last_phase` ne mémorise la
/// dernière valeur que pour détecter un **franchissement**, cf. sa doc, la
/// phase elle-même n'a pas besoin de cet historique).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BossPhase {
    /// > 50 % PV : poursuite standard, jet simple, longue visée.
    Veille,
    /// 25-50 % PV : plus rapide, éventail de 3 jets, visée plus courte.
    Courroux,
    /// < 25 % PV : vitesse maximale, onde à 360°, pause de récupération après.
    DernierSursaut,
}

impl BossPhase {
    /// Libellé affiché à côté de la barre de vie (cf. `editor::hud::boss_health_bar`).
    pub fn label(self) -> &'static str {
        match self {
            BossPhase::Veille => "Veille",
            BossPhase::Courroux => "Courroux",
            BossPhase::DernierSursaut => "Dernier sursaut",
        }
    }
}

/// Phase courante depuis le ratio PV (1.0 = pleine vie) — seuils du design
/// validé (50 %, 25 %).
pub fn phase_for_ratio(ratio: f32) -> BossPhase {
    if ratio > 0.5 {
        BossPhase::Veille
    } else if ratio > 0.25 {
        BossPhase::Courroux
    } else {
        BossPhase::DernierSursaut
    }
}

/// Réglages d'une phase : vitesse de poursuite (`AiChaser::speed`, appliquée
/// directement — même convention d'authoring que le reste du roster Rivière,
/// où `Archetype::hp_multiplier`/`speed_multiplier` ne sont **pas** appliqués
/// automatiquement, cf. `scene/mod.rs`) et profil de l'attaque à distance.
struct PhaseTuning {
    chase_speed: f32,
    range: f32,
    cooldown: f32,
    chance: f32,
    windup: f32,
    shot_speed: f32,
    lifetime: f32,
    radius: f32,
    damage: f32,
    color: [f32; 3],
}

fn tuning(phase: BossPhase) -> PhaseTuning {
    match phase {
        BossPhase::Veille => PhaseTuning {
            chase_speed: 1.6,
            range: 10.0,
            cooldown: 6.0,
            chance: 0.5,
            windup: 1.3,
            shot_speed: 8.0,
            lifetime: 1.6,
            radius: 0.9,
            damage: 0.16,
            color: [0.55, 0.85, 1.0],
        },
        BossPhase::Courroux => PhaseTuning {
            chase_speed: 2.0,
            range: 10.0,
            cooldown: 4.0,
            chance: 0.6,
            windup: 0.9,
            shot_speed: 9.5,
            lifetime: 1.3,
            radius: 0.8,
            damage: 0.12,
            color: [0.3, 0.65, 1.0],
        },
        BossPhase::DernierSursaut => PhaseTuning {
            chase_speed: 2.4,
            range: 10.0,
            cooldown: 3.0,
            chance: 0.7,
            windup: 0.6,
            shot_speed: 10.5,
            lifetime: 1.1,
            radius: 0.75,
            damage: 0.1,
            color: [0.85, 0.95, 1.0],
        },
    }
}

/// Fréquence (rad/s de `time`) du pulse de teinte du télégraphe pendant la
/// visée (cf. `update_boss`) — croît avec la phase : la Nova de « Dernier
/// sursaut » (windup le plus court, l'attaque la plus dangereuse, cf.
/// `tuning`) doit avoir le télégraphe le plus alarmant, pas le moins, alors
/// qu'une fréquence constante entre les trois phases (avant la relecture du
/// 15 septembre 2026) ne changeait que la durée du clignotement, jamais son
/// rythme.
fn telegraph_pulse_frequency(phase: BossPhase) -> f32 {
    match phase {
        BossPhase::Veille => 9.0,
        BossPhase::Courroux => 13.0,
        BossPhase::DernierSursaut => 18.0,
    }
}

/// État de l'attaque à distance du boss — même rôle que
/// `creature_attack::RangedState`, en plus léger (une seule créature, pas de
/// rafale) et avec `recovering` en plus pour distinguer « visée en cours »
/// de « pause de récupération après la Nova » (les deux réutilisent
/// `stopped_until`, cf. la doc de `AppState::update_boss`).
#[derive(Default)]
struct BossRangedState {
    cooldown: f32,
    stopped_until: Option<f32>,
    frozen_pos: Option<Vec3>,
    recovering: bool,
}

/// Un jet de la cascade en vol — même principe que
/// `creature_attack::CreatureShot`, sans tête chercheuse ni obus (le boss n'a
/// que des tirs droits, cf. `AppState::fire_boss_attack`).
struct BossShot {
    pos: Vec3,
    dir: Vec3,
    remaining: f32,
}

/// État complet du boss, dans `AppState` (cf. son champ `boss`).
#[derive(Default)]
pub(super) struct BossState {
    ranged: BossRangedState,
    shots: Vec<BossShot>,
    /// Pool d'objets de scène (sphères émissives) affichant les jets en vol —
    /// même principe que `creature_attack::creature_shot_pool`.
    shot_pool: Vec<usize>,
    /// Dernière phase observée (`None` avant le premier `update_boss`) — seul
    /// état non dérivable du ratio de PV (cf. la doc de `BossPhase`) : sert
    /// uniquement à détecter le **franchissement ponctuel** d'un seuil de
    /// phase (50 %/25 %) pour y accrocher un signal (`WaveStart`, cf.
    /// `update_boss`), impossible à exprimer à partir du seul ratio courant
    /// (qui ne dit rien de la phase précédente).
    last_phase: Option<BossPhase>,
    /// Indice en cache de `BOSS_NAME` dans `scene.objects`, validé en O(1)
    /// avant réutilisation (cf. `simulation::find_named_object_cached`) —
    /// audit perf du 15 septembre 2026 : élimine jusqu'à 3 scans linéaires
    /// complets par pas fixe (`update_boss`, `refresh_boss_frozen_anchor`,
    /// `update_boss_loot`) sans changer quel objet est trouvé.
    boss_idx_cache: Option<usize>,
    /// Même rôle que `boss_idx_cache`, pour `BOSS_LOOT_NAME`.
    loot_idx_cache: Option<usize>,
}

impl AppState {
    /// Vrai si l'objet `idx` est le boss et qu'il est actuellement figé (visée
    /// ou pause de récupération) — même rôle que `creature_is_aim_frozen`
    /// pour `RANGED_CREATURE_ATTACKS`, consulté par `sim_step_inner` pour ne
    /// pas laisser la chasse IA écraser le gel.
    pub(super) fn boss_is_frozen(&self, idx: usize) -> bool {
        self.scene.objects.get(idx).is_some_and(|o| o.name == BOSS_NAME)
            && self.boss.ranged.stopped_until.is_some()
    }

    /// Ré-ancre la position gelée du boss sur celle réellement atteinte ce
    /// tick (bousculade) — même rôle que `refresh_frozen_anchors`, à appeler
    /// après `Physics::resolve_scripted_moves`/`step`.
    pub(super) fn refresh_boss_frozen_anchor(&mut self) {
        if self.boss.ranged.stopped_until.is_none() || self.boss.ranged.frozen_pos.is_none() {
            return;
        }
        let idx = super::simulation::find_named_object_cached(
            &self.scene,
            self.boss.boss_idx_cache,
            BOSS_NAME,
        );
        self.boss.boss_idx_cache = idx;
        if let Some(obj) = idx.and_then(|i| self.scene.objects.get(i)) {
            self.boss.ranged.frozen_pos = Some(obj.transform.position);
        }
    }

    /// Fait vivre le boss pour ce pas fixe : dérive la visibilité du butin
    /// garanti de celle du boss (cf. la doc du module), calcule sa phase et
    /// applique sa vitesse de poursuite et son télégraphe visuel (pulse de
    /// teinte pendant la visée, cf. `Scene::boss_demo` pour le même patron),
    /// puis — hors client réseau connecté (cf. `is_online_client`, même garde
    /// que `update_creature_ranged_attacks`) — fait vivre son attaque à
    /// distance (visée, tir, vol, impact sur le joueur local).
    pub(super) fn update_boss(&mut self, dt: f32, time: f32) {
        self.update_boss_loot();
        let boss_idx = super::simulation::find_named_object_cached(
            &self.scene,
            self.boss.boss_idx_cache,
            BOSS_NAME,
        );
        self.boss.boss_idx_cache = boss_idx;
        let Some(boss_idx) = boss_idx else {
            return;
        };
        if !self.scene.objects[boss_idx].visible {
            // Vaincu (ou pas encore réapparu) : oublie tout état de visée/tir
            // en cours et vide le pool d'affichage — même politique que
            // `clear_creature_shots`.
            self.boss.ranged = BossRangedState::default();
            self.boss.shots.clear();
            self.sync_boss_shot_pool([1.0, 1.0, 1.0]);
            return;
        }

        let ratio = self.scene.objects[boss_idx]
            .combat
            .as_ref()
            .map(|c| {
                if c.max_hp > 0 {
                    c.hp as f32 / c.max_hp as f32
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let phase = phase_for_ratio(ratio);
        let tune = tuning(phase);

        // Signal ponctuel au franchissement d'un seuil de phase (relecture du
        // 15 septembre 2026) : jusqu'ici, seul le petit libellé texte de la
        // barre HUD (`BossPhase::label`) changeait — rien ne marquait le
        // moment précis où le boss entre en Courroux/Dernier sursaut. Réutilise
        // `Sfx::WaveStart` (déjà le signal « montée en tension » du jeu, cf.
        // sa doc) plutôt qu'un nouveau son dédié, et un flash de dégât bref
        // (`fx.damage_flash`, déjà lu par le rendu) plutôt qu'un nouveau canal
        // d'effet visuel. Ignore le premier appel (`last_phase == None`) : ce
        // n'est pas une transition, juste l'observation initiale.
        if let Some(last) = self.boss.last_phase
            && last != phase
        {
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::WaveStart);
            self.fx.damage_flash = 1.0;
        }
        self.boss.last_phase = Some(phase);

        if let Some(ai) = self.scene.objects[boss_idx].ai_chaser.as_mut() {
            ai.speed = tune.chase_speed;
        }

        // Télégraphe visuel : pulse de teinte pendant la visée (même patron
        // que `Scene::boss_demo`, cf. sa doc) — purement cosmétique, non
        // répliqué aux clients réseau connectés (cf. la doc du module).
        // Fréquence du pulse accélérée phase après phase (relecture du 15
        // septembre 2026) : à fréquence constante, la Nova de la phase
        // « Dernier sursaut » — la plus dangereuse (onde à 360°, windup le
        // plus court, cf. `tuning`) — n'avait pourtant pas un télégraphe
        // visuellement plus marqué que le simple jet de la phase « Veille »,
        // seule la durée de la visée raccourcissait. Le rythme du clignotement
        // doit lui aussi s'accélérer pour rester lisible malgré la fenêtre de
        // réaction plus courte.
        let pulse_freq = telegraph_pulse_frequency(phase);
        let aiming = self.boss.ranged.stopped_until.is_some();
        self.scene.objects[boss_idx].color = if aiming {
            let p = (time * pulse_freq).sin() * 0.5 + 0.5;
            [1.0, 1.0 - 0.45 * p, 1.0 - 0.35 * p]
        } else {
            [1.0, 1.0, 1.0]
        };

        if self.is_online_client() {
            return;
        }
        let Some(player_pos) = self.player_position() else {
            return;
        };

        if let Some(deadline) = self.boss.ranged.stopped_until {
            // Arrêté (visée ou récupération) : gèle la position (annule le
            // déplacement de chasse de ce tick) et force `Idle`.
            if let Some(frozen) = self.boss.ranged.frozen_pos {
                self.scene.objects[boss_idx].transform.position = frozen;
            }
            if let Some(anim) = self.scene.objects[boss_idx].animation.as_mut() {
                anim.set_clip("Idle");
            }
            if time >= deadline {
                if self.boss.ranged.recovering {
                    self.boss.ranged.recovering = false;
                    self.boss.ranged.stopped_until = None;
                    self.boss.ranged.frozen_pos = None;
                } else {
                    let origin =
                        self.scene.objects[boss_idx].transform.position + Vec3::Y * SPAWN_UP;
                    self.fire_boss_attack(phase, origin, player_pos);
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Jump);
                    if phase == BossPhase::DernierSursaut {
                        // Pause de récupération : reste figé, ne tire pas
                        // encore — cf. `RECOVER_SECS`.
                        self.boss.ranged.stopped_until = Some(time + RECOVER_SECS);
                        self.boss.ranged.recovering = true;
                    } else {
                        self.boss.ranged.stopped_until = None;
                        self.boss.ranged.frozen_pos = None;
                    }
                }
            }
        } else {
            self.boss.ranged.cooldown = (self.boss.ranged.cooldown - dt).max(0.0);
            if self.boss.ranged.cooldown <= 0.0 {
                let boss_pos = self.scene.objects[boss_idx].transform.position;
                if player_pos.distance(boss_pos) <= tune.range {
                    // Tentative consommée qu'elle réussisse ou non — même
                    // garde que `update_creature_ranged_attacks`.
                    self.boss.ranged.cooldown = tune.cooldown;
                    if creature_attack::deterministic_roll(time, BOSS_SALT) < tune.chance {
                        self.boss.ranged.frozen_pos = Some(boss_pos);
                        self.boss.ranged.stopped_until = Some(time + tune.windup);
                    }
                }
            }
        }

        // Vol + impacts. `tune` (phase courante) régit aussi les jets déjà en
        // vol : simplification assumée — un changement de phase en plein vol
        // d'un jet (fenêtre de ≤ ~1,6 s) lui applique la vitesse/rayon/dégâts
        // de la nouvelle phase plutôt que celle qui l'a tiré, sans effet
        // perceptible en jeu.
        let mut shots = std::mem::take(&mut self.boss.shots);
        shots.retain_mut(|s| {
            s.remaining -= dt;
            if s.remaining <= 0.0 {
                return false;
            }
            s.pos += s.dir * tune.shot_speed * dt;
            true
        });
        shots.retain(|s| {
            let hit = s.pos.distance(player_pos + Vec3::Y * SPAWN_UP) <= tune.radius;
            if hit && self.play_grace <= 0.0 {
                self.hud_health = self.hud_health.map(|h| (h - tune.damage).max(0.0));
                self.fx.damage_flash = 1.0;
                self.fx.camera_shake = 1.0;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
            }
            !hit
        });
        self.boss.shots = shots;
        self.sync_boss_shot_pool(tune.color);
    }

    /// Fait partir un jet (Single), un éventail de 3 (Fan) ou une onde de 8
    /// (Nova) selon la phase — même dispatch que `creature_attack::
    /// fire_creature_attack`, réduit aux trois formes utiles ici.
    fn fire_boss_attack(&mut self, phase: BossPhase, origin: Vec3, player: Vec3) {
        let lifetime = tuning(phase).lifetime;
        let target = player + Vec3::Y * SPAWN_UP;
        let flat = (player - origin).with_y(0.0);
        let flat_dir = flat.try_normalize().unwrap_or(Vec3::NEG_Z);
        let dirs: Vec<Vec3> = match phase {
            BossPhase::Veille => (target - origin).try_normalize().into_iter().collect(),
            BossPhase::Courroux => {
                let spread = 45f32.to_radians();
                (0..3u32)
                    .map(|k| {
                        let t = k as f32 / 2.0 - 0.5;
                        Quat::from_rotation_y(t * spread) * flat_dir
                    })
                    .collect()
            }
            BossPhase::DernierSursaut => (0..8u32)
                .map(|k| {
                    let angle = std::f32::consts::TAU * k as f32 / 8.0;
                    Quat::from_rotation_y(angle) * flat_dir
                })
                .collect(),
        };
        for dir in dirs {
            self.boss.shots.push(BossShot {
                pos: origin,
                dir,
                remaining: lifetime,
            });
        }
    }

    /// Aligne le pool d'affichage des jets en vol — même principe que
    /// `creature_attack::sync_creature_shot_pool`, en plus simple (pas de
    /// config par tir, une seule couleur pour toute la volée courante).
    fn sync_boss_shot_pool(&mut self, color: [f32; 3]) {
        let snapshot: Vec<(Vec3, Vec3)> = self.boss.shots.iter().map(|s| (s.pos, s.dir)).collect();
        while self.boss.shot_pool.len() < snapshot.len() {
            let index = self.scene.objects.len();
            self.scene.objects.push(crate::scene::SceneObject {
                name: format!("Jet de la cascade {}", self.boss.shot_pool.len() + 1),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::ZERO),
                emissive: 1.6,
                physics: PhysicsKind::None,
                visible: false,
                ..Default::default()
            });
            self.boss.shot_pool.push(index);
        }
        for (slot, &index) in self.boss.shot_pool.iter().enumerate() {
            if let Some(o) = self.scene.objects.get_mut(index) {
                match snapshot.get(slot) {
                    Some(&(pos, dir)) => {
                        o.transform.position = pos;
                        o.transform.scale = Vec3::new(0.35, 0.35, 0.65);
                        o.transform.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
                        o.color = color;
                        o.visible = true;
                    }
                    None => o.visible = false,
                }
            }
        }
    }

    /// Oublie les jets en vol, l'état de visée et le pool d'affichage — mêmes
    /// sites d'appel que `clear_creature_shots` (le pool vit dans
    /// `scene.objects`, ses indices deviennent obsolètes après une
    /// restauration en bloc de la scène, ex. « Rejouer »/sortie de Play).
    pub(super) fn clear_boss_shots(&mut self) {
        self.boss.ranged = BossRangedState::default();
        self.boss.shots.clear();
        self.boss.shot_pool.clear();
    }

    /// Dérive la visibilité du butin garanti (`BOSS_LOOT_NAME`) de celle du
    /// boss — cf. la doc du module. Purement fonctionnel de l'état déjà
    /// autoritaire de la scène (visibilité du boss) : appelé sans garde
    /// réseau, il produit le même résultat en solo, sur le serveur, et sur un
    /// client connecté (qui reçoit déjà la visibilité du boss via
    /// `Snapshot`/`EntityDelta::visible`).
    fn update_boss_loot(&mut self) {
        let boss_idx = super::simulation::find_named_object_cached(
            &self.scene,
            self.boss.boss_idx_cache,
            BOSS_NAME,
        );
        self.boss.boss_idx_cache = boss_idx;
        let Some(boss_visible) = boss_idx.and_then(|i| self.scene.objects.get(i)).map(|o| o.visible)
        else {
            return;
        };
        let loot_idx = super::simulation::find_named_object_cached(
            &self.scene,
            self.boss.loot_idx_cache,
            BOSS_LOOT_NAME,
        );
        self.boss.loot_idx_cache = loot_idx;
        if let Some(loot) = loot_idx.and_then(|i| self.scene.objects.get_mut(i)) {
            let want_visible = !boss_visible;
            if loot.visible != want_visible {
                loot.visible = want_visible;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::physics::Physics;

    #[test]
    fn phase_thresholds_match_the_validated_design() {
        assert_eq!(phase_for_ratio(1.0), BossPhase::Veille);
        assert_eq!(phase_for_ratio(0.51), BossPhase::Veille);
        assert_eq!(phase_for_ratio(0.5), BossPhase::Courroux);
        assert_eq!(phase_for_ratio(0.26), BossPhase::Courroux);
        assert_eq!(phase_for_ratio(0.25), BossPhase::DernierSursaut);
        assert_eq!(phase_for_ratio(0.0), BossPhase::DernierSursaut);
    }

    /// La Nova de « Dernier sursaut » (attaque la plus dangereuse, windup le
    /// plus court) doit avoir le télégraphe visuellement le plus marqué : la
    /// fréquence du pulse de teinte doit strictement croître phase après
    /// phase, pas rester constante (cf. la doc de `telegraph_pulse_frequency`).
    #[test]
    fn telegraph_pulse_accelerates_from_phase_to_phase() {
        let veille = telegraph_pulse_frequency(BossPhase::Veille);
        let courroux = telegraph_pulse_frequency(BossPhase::Courroux);
        let dernier = telegraph_pulse_frequency(BossPhase::DernierSursaut);
        assert!(courroux > veille, "Courroux doit pulser plus vite que Veille");
        assert!(
            dernier > courroux,
            "Dernier sursaut doit pulser plus vite que Courroux : l'attaque la plus \
             dangereuse doit avoir le télégraphe le plus alarmant"
        );
    }

    fn boss_index(app: &AppState) -> usize {
        app.scene
            .objects
            .iter()
            .position(|o| o.name == BOSS_NAME)
            .expect("la démo Rivière doit contenir le boss")
    }

    fn loot_index(app: &AppState) -> usize {
        app.scene
            .objects
            .iter()
            .position(|o| o.name == BOSS_LOOT_NAME)
            .expect("la démo Rivière doit contenir le butin du boss")
    }

    /// Non-régression de spawn : le boss apparaît visible, encaisse un très
    /// grand nombre de PV, garde son propre tag/groupe (pas confondu avec le
    /// roster ordinaire) et n'est éligible qu'à une très longue réapparition.
    #[test]
    fn boss_spawns_with_massive_hp_and_a_long_respawn_delay() {
        let app_scene = crate::scene::Scene::riviere_demo();
        let idx = app_scene
            .objects
            .iter()
            .position(|o| o.name == BOSS_NAME)
            .expect("boss absent de la démo Rivière");
        let boss = &app_scene.objects[idx];
        assert!(boss.visible, "le boss doit apparaître visible dès le départ");
        assert_eq!(boss.tag, "boss", "le boss ne doit pas partager le tag \"monstre\"");
        assert_eq!(boss.group, "Boss");
        let combat = boss.combat.as_ref().expect("le boss doit être attaquable");
        assert!(combat.attackable);
        assert_eq!(combat.hp, 60, "PV d'authoring inattendus pour un boss");
        assert_eq!(combat.wave, 0, "pas de système de manches dans Rivière");
        assert_eq!(
            boss.ai_chaser.as_ref().map(|c| c.archetype),
            Some(crate::scene::Archetype::Colosse)
        );
        assert!(boss.bite.is_some(), "le boss doit aussi mordre au contact");
        assert!(!boss.script.is_empty(), "script de morsure solo manquant");
        assert_eq!(
            boss.respawn_delay, 600.0,
            "le boss doit réapparaître très rarement (10 minutes), pas comme un monstre ordinaire"
        );

        let loot = app_scene
            .objects
            .iter()
            .find(|o| o.name == BOSS_LOOT_NAME)
            .expect("butin du boss absent de la démo Rivière");
        assert!(!loot.visible, "le butin garanti doit être masqué tant que le boss est vivant");
        assert!(loot.weapon_pickup.is_some());
    }

    /// Preuve du seuil de phase appliqué **en jeu** (pas seulement
    /// `phase_for_ratio` en isolation) : blesser le boss jusqu'à franchir les
    /// seuils 50 %/25 % change bien sa vitesse de poursuite, pilotée par
    /// `update_boss` à chaque pas fixe.
    #[test]
    fn boss_chase_speed_increases_as_hp_drops_through_phase_thresholds() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss_index(&app);

        app.update_boss(1.0 / 60.0, 0.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning(BossPhase::Veille).chase_speed,
            "pleine vie : phase Veille"
        );

        // 51 % PV restant : encore Phase 1.
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 51;
        app.update_boss(1.0 / 60.0, 1.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning(BossPhase::Veille).chase_speed
        );

        // 50 % PV : Phase 2 (Courroux).
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 50;
        app.update_boss(1.0 / 60.0, 2.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning(BossPhase::Courroux).chase_speed
        );

        // 20 % PV : Phase 3 (Dernier sursaut).
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 20;
        app.update_boss(1.0 / 60.0, 3.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning(BossPhase::DernierSursaut).chase_speed
        );
    }

    /// Preuve de l'attaque à distance télégraphiée, une phase à la fois :
    /// visée forcée (comme `creature_attack::force_fire`, sans dépendre du
    /// tirage probabiliste), le nombre de jets partis au déclenchement doit
    /// correspondre à la forme de la phase (1 = Single, 3 = Fan, 8 = Nova).
    #[test]
    fn boss_ranged_attack_shape_matches_the_current_phase() {
        let cases = [
            (100u32, 100u32, 1usize), // pleine vie : Veille, Single
            (100u32, 40u32, 3usize),  // 40 % : Courroux, Fan
            (100u32, 10u32, 8usize),  // 10 % : Dernier sursaut, Nova
        ];
        for (max_hp, hp, expected_shots) in cases {
            let mut app = AppState::new();
            app.scene = crate::scene::Scene::riviere_demo();
            let idx = boss_index(&app);
            let pi = app
                .scene
                .objects
                .iter()
                .position(|o| o.controller.is_some())
                .expect("joueur pilotable absent");
            app.hud_health = Some(1.0);
            app.scene.objects[idx].combat.as_mut().unwrap().max_hp = max_hp;
            app.scene.objects[idx].combat.as_mut().unwrap().hp = hp;
            let near = app.scene.objects[idx].transform.position + Vec3::new(5.0, 0.0, 0.0);
            app.scene.objects[pi].transform.position = near;
            app.physics = Some(Physics::build(&app.scene));
            app.physics.as_mut().unwrap().set_position(pi, near);

            let frozen = app.scene.objects[idx].transform.position;
            app.boss.ranged.frozen_pos = Some(frozen);
            app.boss.ranged.stopped_until = Some(app.time + 1.0 / 60.0);

            app.sim_step(1.0 / 60.0); // franchit l'échéance : la volée part
            assert_eq!(
                app.boss.shots.len(),
                expected_shots,
                "hp={hp}/{max_hp} : volée inattendue"
            );
        }
    }

    /// Preuve de la pause de récupération après la Nova de la phase 3 : le
    /// boss reste figé (`boss_is_frozen`) juste après avoir tiré, il ne
    /// tire pas une deuxième Nova immédiatement.
    #[test]
    fn boss_pauses_to_recover_after_its_phase_three_nova() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss_index(&app);
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.is_some())
            .unwrap();
        app.hud_health = Some(1.0);
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 10;
        let near = app.scene.objects[idx].transform.position + Vec3::new(5.0, 0.0, 0.0);
        app.scene.objects[pi].transform.position = near;
        app.physics = Some(Physics::build(&app.scene));
        app.physics.as_mut().unwrap().set_position(pi, near);

        let frozen = app.scene.objects[idx].transform.position;
        app.boss.ranged.frozen_pos = Some(frozen);
        app.boss.ranged.stopped_until = Some(app.time + 1.0 / 60.0);
        app.sim_step(1.0 / 60.0);

        assert_eq!(app.boss.shots.len(), 8, "la Nova doit être partie");
        assert!(
            app.boss_is_frozen(idx),
            "le boss doit rester figé en pause de récupération juste après sa Nova"
        );
        assert!(app.boss.ranged.recovering);
    }

    /// Non-régression (relecture du 15 septembre 2026) : franchir un seuil de
    /// phase (ici 50 %, Veille → Courroux) doit déclencher un flash bref
    /// (`fx.damage_flash`), pas seulement changer le libellé texte de la
    /// barre HUD — cf. la doc de `BossState::last_phase`. Un appel qui ne
    /// franchit aucun seuil (reste en Veille) ne doit rien déclencher.
    #[test]
    fn crossing_a_phase_threshold_triggers_a_one_shot_flash() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss_index(&app);

        // Premier appel (pleine vie) : observe seulement la phase initiale,
        // ne doit rien déclencher (pas de phase précédente à comparer).
        app.update_boss(1.0 / 60.0, 0.0);
        assert_eq!(app.fx.damage_flash, 0.0, "premier appel : pas de transition");

        // Toujours en Veille (51 % PV) : pas de franchissement.
        app.fx.damage_flash = 0.0;
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 51;
        app.update_boss(1.0 / 60.0, 1.0);
        assert_eq!(app.fx.damage_flash, 0.0, "reste en Veille : pas de transition");

        // Franchit le seuil des 50 % : Courroux, doit flasher.
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 50;
        app.update_boss(1.0 / 60.0, 2.0);
        assert_eq!(
            app.fx.damage_flash, 1.0,
            "le franchissement du seuil de 50% doit déclencher un flash"
        );
    }

    /// Preuve du loot garanti (cf. la doc du module) : masqué tant que le
    /// boss est vivant ou seulement blessé, révélé au pas fixe qui suit son
    /// dernier coup, masqué de nouveau à sa réapparition (pas de doublon au
    /// sol). La minuterie réelle de `respawn_delay` (600 s) est déjà couverte
    /// génériquement par `a_respawning_enemy_comes_back_with_its_original_hp`
    /// (`app::simulation_tests`) — ici, la réapparition est simulée
    /// directement (visibilité + PV restaurés) pour isoler la dérivation du
    /// loot de cette minuterie.
    #[test]
    fn boss_defeat_reveals_its_guaranteed_loot_and_respawn_hides_it_again() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss_index(&app);
        let lidx = loot_index(&app);
        assert!(!app.scene.objects[lidx].visible);

        // Blessé mais pas achevé : le butin reste masqué.
        assert!(!app.scene.damage_attackable_by(idx, 59));
        app.update_boss(1.0 / 60.0, 0.0);
        assert!(!app.scene.objects[lidx].visible);

        // Coup fatal.
        assert!(app.scene.damage_attackable_by(idx, 1));
        assert!(!app.scene.objects[idx].visible);
        app.update_boss(1.0 / 60.0, 1.0);
        assert!(
            app.scene.objects[lidx].visible,
            "le butin garanti doit apparaître à la mort du boss"
        );

        // Réapparition simulée : le butin ne doit pas rester au sol.
        app.scene.objects[idx].visible = true;
        let max_hp = app.scene.objects[idx].combat.as_ref().unwrap().max_hp;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = max_hp;
        app.update_boss(1.0 / 60.0, 2.0);
        assert!(
            !app.scene.objects[lidx].visible,
            "le butin ne doit pas rester visible après la réapparition du boss"
        );
    }
}
