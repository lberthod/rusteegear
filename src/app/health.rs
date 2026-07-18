//! Vie individualisée par joueur réseau, dégâts de contact monstre, et soin
//! coopératif — GAMEDESIGN_EN_LIGNE.md §3.1/§3.6.
//!
//! Avant ce module, `hud_health` (un `Option<f32>` scalaire, `app/mod.rs`) était
//! la seule notion de vie de l'`AppState` — pensée pour un unique joueur local :
//! en multijoueur, la manche entière échouait ou survivait au sort d'un seul
//! objet « gabarit ». Ici, chaque joueur réseau a sa **propre** vie
//! (`AppState::network_health`), sa propre mort (spectateur — objet masqué,
//! entrées ignorées, cf. `sim_step`/`fireball`/`multiplayer::update_network_
//! attacks` — mais la manche continue pour les autres) et peut recevoir le
//! soin d'un allié. `hud_health` reste inchangé pour le joueur solo (aucune
//! régression : un seul joueur, ce champ suffit toujours).

use std::collections::VecDeque;

use glam::Vec3;

use super::AppState;
use crate::net::protocol::{DeathCause, DeathCauseKind, GameEvent, PlayerId};

/// Nombre de dernières sources de dégâts mémorisées par joueur réseau, pour le
/// diagnostic de mort (Sprint 2, `sprint10audit.md`, GDD §16.5) — juste assez
/// pour distinguer « un seul agresseur » d'« encerclé », pas un historique
/// complet (protocole/mémoire bornés, purge continue via `pop_front`).
const DEATH_CAUSE_WINDOW: usize = 5;

/// Résume la fenêtre de dégâts d'un joueur en cause de mort affichable : type
/// d'agresseur le plus récent, nombre d'agresseurs *distincts* de ce type dans
/// la fenêtre. `None` si la fenêtre est vide (mort sans dégât mémorisé, ex.
/// vie mise à 0 directement par un test/import).
fn compute_death_cause(buf: &VecDeque<(DeathCauseKind, usize)>) -> Option<DeathCause> {
    let last_kind = buf.back()?.0;
    let distinct = buf
        .iter()
        .filter(|(kind, _)| *kind == last_kind)
        .map(|(_, idx)| *idx)
        .collect::<std::collections::HashSet<_>>()
        .len();
    Some(DeathCause {
        kind: last_kind,
        distinct_attackers: distinct.min(u8::MAX as usize) as u8,
    })
}

/// Vie de départ (et maximum) d'un joueur réseau — pas encore différenciée par
/// rôle (cf. GAMEDESIGN_EN_LIGNE.md §3.5, délibérément hors scope ici : un
/// système de classes mérite sa propre UI de sélection, pas improvisée en
/// passant).
pub(super) const MAX_HEALTH: f32 = 1.0;

/// Dégâts par seconde infligés à un joueur réseau au contact (AABB) d'un
/// monstre `AiChaser` visible : ~6 s pour mourir de pleine vie à un seul
/// monstre — assez lent pour laisser une vraie fenêtre de réaction (fuir,
/// riposter, se faire soigner), pas une mort quasi instantanée au contact.
const MONSTER_CONTACT_DPS: f32 = 0.16;

/// Régénération passive (par seconde) hors de tout contact. Volontairement
/// plus lente que la régénération solo (`HEALTH_REGEN_PER_S = 0.25` dans
/// `advance_play`) : en coopératif, le soin d'un allié (cf.
/// `update_network_heal`) est censé rester le principal levier de
/// récupération active, pas la seule attente passive.
const REGEN_PER_S: f32 = 0.05;

/// Portée (m) du soin coopératif : un allié doit être à cette distance du
/// soigneur pour être ciblé.
const HEAL_RANGE: f32 = 2.5;

/// Débit de soin (par seconde) appliqué à l'allié le plus blessé à portée.
const HEAL_RATE_PER_S: f32 = 0.2;

/// Portée (m) du soin du Soutien (GDD §8.1 : « 0,5 PV/s, 4 m ») — le soin
/// universel (`HEAL_RANGE`/`HEAL_RATE_PER_S`) reste inchangé pour tous, le
/// Soutien le *multiplie*, il ne le remplace pas côté mécanique.
const SUPPORT_HEAL_RANGE: f32 = 4.0;

/// Débit de soin (par seconde) du Soutien — ×2,5 le débit universel.
const SUPPORT_HEAL_RATE_PER_S: f32 = 0.5;

/// Portée (m) de la réanimation (GDD §8.1) : non chiffrée explicitement au-delà
/// de « canal immobile » — réutilise `HEAL_RANGE`, cohérente avec le reste du
/// soin coopératif plutôt qu'une nouvelle valeur inventée sans playtest.
const REVIVE_RANGE: f32 = HEAL_RANGE;

/// Durée (s) du canal de réanimation (GDD §8.1 : « 10 s de canal immobile »).
const REVIVE_DURATION: f32 = 10.0;

/// Fraction de PV max restaurée à la fin d'une réanimation (GDD §8.1 :
/// « retour à 30 % PV »).
const REVIVE_HEALTH_FRACTION: f32 = 0.3;

/// Chevauchement de deux AABB monde `(min, max)` — même test que
/// `Scene::world_aabb_intersects`, réimplémenté ici sur des paires `Vec3`
/// **possédées** plutôt que sur deux `&SceneObject` : évite d'emprunter deux
/// fois `scene.objects` en même temps que la boucle appelante a besoin d'un
/// emprunt mutable (marquer un joueur vaincu comme masqué).
fn aabbs_overlap(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}

/// Hachage déterministe de `time` en [0, 1) — même idiome que
/// `creature_attack::deterministic_roll`/le script Lua `creature_bite_script`
/// (pas partagé : trois occurrences courtes, chacune dans son propre module,
/// plutôt qu'une abstraction commune pour une seule ligne de calcul).
fn deterministic_roll(time: f32, salt: f32) -> f32 {
    let x = (time * salt).sin() * 43_758.547;
    x - x.floor()
}

impl AppState {
    /// Dégâts de contact monstre + régénération passive, pour chaque joueur
    /// réseau — appelée une fois par frame (dt réel) depuis `advance_play`,
    /// après `update_fireballs` (les dégâts à distance du tick sont déjà
    /// résolus, la vie de ce tick est donc à jour avant d'y ajouter le contact).
    pub(super) fn update_network_health(&mut self, dt: f32) {
        if self.network_players.is_empty() {
            return;
        }
        // AABB des monstres `AiChaser` visibles ce tick — même notion de
        // « danger » que le pilotage IA (cf. `sim_step`), calculée une fois et
        // réutilisée pour chaque joueur plutôt que reparcourue par joueur.
        // Indice d'objet conservé (pas seulement l'AABB) pour le diagnostic de
        // mort (Sprint 2) : distinguer un seul monstre au contact de plusieurs.
        let monster_aabbs: Vec<(usize, (Vec3, Vec3))> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.ai_chaser.is_some() && o.visible)
            .map(|(i, o)| (i, self.scene.world_aabb(o)))
            .collect();

        let ids: Vec<PlayerId> = self.network_players.keys().copied().collect();
        for id in ids {
            let Some(&index) = self.network_players.get(&id) else {
                continue;
            };
            let Some((visible, player_aabb)) = self
                .scene
                .objects
                .get(index)
                .map(|o| (o.visible, self.scene.world_aabb(o)))
            else {
                continue;
            };
            if !visible {
                // Déjà vaincu (spectateur) : vie figée à 0, rien à faire.
                continue;
            }
            let touching: Vec<usize> = monster_aabbs
                .iter()
                .filter(|&&(_, aabb)| aabbs_overlap(aabb, player_aabb))
                .map(|&(idx, _)| idx)
                .collect();
            let touched = !touching.is_empty();
            let max_hp = self.max_health_for(id);
            let was_alive = self.network_health.get(&id).copied().unwrap_or(max_hp) > 0.0;
            let hp = self.network_health.entry(id).or_insert(max_hp);
            if touched {
                *hp = (*hp - MONSTER_CONTACT_DPS * dt).max(0.0);
                let buf = self.recent_damage.entry(id).or_default();
                for idx in touching {
                    buf.push_back((DeathCauseKind::Monster, idx));
                    while buf.len() > DEATH_CAUSE_WINDOW {
                        buf.pop_front();
                    }
                }
            } else {
                *hp = (*hp + REGEN_PER_S * dt).min(max_hp);
            }
            let just_died = was_alive && *hp <= 0.0;
            if just_died {
                // Spectateur pour le reste de la manche : pas de réapparition
                // dans ce sprint (décision assumée, cf. GAMEDESIGN_EN_LIGNE.md
                // §3.1 — garder le scope contenu ; une réanimation serait une
                // extension naturelle mais distincte).
                if let Some(o) = self.scene.objects.get_mut(index) {
                    o.visible = false;
                }
                let cause = self
                    .recent_damage
                    .remove(&id)
                    .and_then(|buf| compute_death_cause(&buf));
                self.pending_net_events.push(GameEvent::PlayerDown {
                    player_id: id,
                    cause,
                });
                self.player_down_count += 1;
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
            }
        }
    }

    /// Dégâts de contact des créatures scriptées mordeuses (`SceneObject::bite`,
    /// ex. Créature 1/6/7 — cf. sa doc), pour chaque joueur réseau — pendant de
    /// `update_network_health` (monstres `AiChaser`) mais **générique à toute
    /// créature future** posant ce champ, pas câblée sur un nom précis. En solo,
    /// la version Lua (`creature_bite_script`, `damage()`/`hud_health`) reste seule
    /// responsable — inchangée, cette fonction ne s'exécute jamais dans ce cas.
    /// Appelée depuis `advance_play`, au même endroit que `update_network_health`
    /// (avant `update_network_heal` : le contact de ce tick doit être à jour avant
    /// que le soin ne s'applique).
    pub(super) fn update_creature_bite(&mut self, dt: f32) {
        if self.network_players.is_empty() {
            return;
        }
        // Créatures mordeuses visibles ce tick, avec leur AABB — calculé une fois,
        // réutilisé pour chaque joueur (même idiome que `monster_aabbs` ci-dessus).
        let biters: Vec<(usize, crate::scene::BiteAttack, (Vec3, Vec3))> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.visible)
            .filter_map(|(i, o)| o.bite.map(|b| (i, b, self.scene.world_aabb(o))))
            .collect();
        if biters.is_empty() {
            return;
        }
        for cd in self.bite_cooldowns.values_mut() {
            *cd = (*cd - dt).max(0.0);
        }

        let ids: Vec<PlayerId> = self.network_players.keys().copied().collect();
        for id in ids {
            let Some(&index) = self.network_players.get(&id) else {
                continue;
            };
            let Some((visible, player_aabb)) = self
                .scene
                .objects
                .get(index)
                .map(|o| (o.visible, self.scene.world_aabb(o)))
            else {
                continue;
            };
            if !visible {
                continue;
            }
            for &(creature_idx, bite, creature_aabb) in &biters {
                if !aabbs_overlap(creature_aabb, player_aabb) {
                    continue;
                }
                let cd = self.bite_cooldowns.entry((creature_idx, id)).or_insert(0.0);
                if *cd > 0.0 {
                    continue;
                }
                // Tentative consommée qu'elle réussisse ou non — même garde-fou
                // que `creature_bite_script`/`creature_attack` (sans ça, un
                // cooldown à 0 au contact permanent referait le tirage chaque
                // tick, rendant `chance` illusoire).
                *cd = bite.cooldown;
                // Salt dérivé de la paire (créature, joueur) : deux morsures au
                // même tick ne roulent pas en lockstep, comme `creature_attack`.
                let salt = 11.0 + creature_idx as f32 * 7.0 + id as f32 * 3.0;
                if deterministic_roll(self.time, salt) >= bite.chance {
                    continue;
                }
                let was_alive = self.network_health.get(&id).copied().unwrap_or(MAX_HEALTH) > 0.0;
                let hp = self.network_health.entry(id).or_insert(MAX_HEALTH);
                *hp = (*hp - bite.damage).max(0.0);
                let buf = self.recent_damage.entry(id).or_default();
                buf.push_back((DeathCauseKind::Creature, creature_idx));
                while buf.len() > DEATH_CAUSE_WINDOW {
                    buf.pop_front();
                }
                let just_died = was_alive && *hp <= 0.0;
                if just_died {
                    if let Some(o) = self.scene.objects.get_mut(index) {
                        o.visible = false;
                    }
                    let cause = self
                        .recent_damage
                        .remove(&id)
                        .and_then(|buf| compute_death_cause(&buf));
                    self.pending_net_events.push(GameEvent::PlayerDown {
                        player_id: id,
                        cause,
                    });
                    self.player_down_count += 1;
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
                } else {
                    if Some(id) == self.net_player_id {
                        self.camera_shake = 1.0;
                    }
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
                }
            }
        }
    }

    /// Résout le soin coopératif pour ce tick (GAMEDESIGN_EN_LIGNE.md §3.6) :
    /// chaque joueur réseau vivant dont l'`Input` demande `heal` soigne
    /// l'allié vivant le plus proche **et blessé** à portée (`HEAL_RANGE`), au
    /// débit `HEAL_RATE_PER_S`. Continue (pas de recharge discrète) : le débit
    /// lui-même borne l'efficacité, pas un temporisateur.
    pub(super) fn update_network_heal(&mut self, dt: f32) {
        if self.network_players.len() < 2 {
            // Un soin a besoin d'un soigneur ET d'un allié : rien à faire seul
            // (le cas de loin le plus courant hors session de test/démo).
            return;
        }
        let healers: Vec<(PlayerId, usize)> = self
            .network_players
            .iter()
            .filter(|(id, _)| self.network_inputs.get(id).is_some_and(|i| i.heal))
            .filter(|(id, _)| self.network_health.get(id).copied().unwrap_or(0.0) > 0.0)
            .map(|(&id, &idx)| (id, idx))
            .collect();
        for (healer_id, healer_idx) in healers {
            let Some(healer_pos) = self
                .scene
                .objects
                .get(healer_idx)
                .map(|o| o.transform.position)
            else {
                continue;
            };
            // Soutien (GDD §8.1) : soin ×2,5 — 0,5 PV/s à 4 m, contre 0,2 PV/s
            // à 2,5 m pour les autres classes (soin universel, jamais retiré).
            let is_support = self
                .network_classes
                .get(&healer_id)
                .is_some_and(|c| matches!(c, crate::app::multiplayer::PlayerClass::Support));
            let (heal_range, heal_rate) = if is_support {
                (SUPPORT_HEAL_RANGE, SUPPORT_HEAL_RATE_PER_S)
            } else {
                (HEAL_RANGE, HEAL_RATE_PER_S)
            };
            // Allié vivant, blessé (pas déjà à son plafond : sans ce filtre, un
            // soigneur entouré d'alliés au max ne soignerait jamais personne
            // d'autre malgré un blessé un peu plus loin, mais à portée), le
            // plus proche à portée — jamais soi-même.
            let target_id = self
                .network_players
                .iter()
                .filter(|(id, _)| **id != healer_id)
                .filter_map(|(&id, &idx)| {
                    let hp = self.network_health.get(&id).copied().unwrap_or(0.0);
                    if hp <= 0.0 || hp >= self.max_health_for(id) {
                        return None;
                    }
                    let pos = self.scene.objects.get(idx)?.transform.position;
                    let dist = pos.distance(healer_pos);
                    (dist <= heal_range).then_some((id, dist))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id);
            if let Some(target_id) = target_id {
                let max_hp = self.max_health_for(target_id);
                if let Some(hp) = self.network_health.get_mut(&target_id) {
                    *hp = (*hp + heal_rate * dt).min(max_hp);
                }
            }
        }
    }

    /// Résout la réanimation pour ce tick (GDD §8.1 : exclusivité du Soutien,
    /// « seul à réanimer »). Un Soutien vivant dont l'`Input` demande `heal`
    /// et qui reste à portée (`REVIVE_RANGE`) d'un allié spectateur (0 PV)
    /// canalise dessus ; `REVIVE_DURATION` d'affilée le ramène à
    /// `REVIVE_HEALTH_FRACTION` de ses PV max. **Canal, pas jauge** : relâcher
    /// `heal`, sortir de portée ou changer de cible remet le compteur à zéro
    /// — c'est la décision dramatique du GDD (§5.3 : « 10 s de canal immobile
    /// au milieu d'une horde »), pas un droit acquis qu'on peut fractionner
    /// sans risque.
    pub(super) fn update_network_revive(&mut self, dt: f32) {
        if self.network_players.len() < 2 {
            return;
        }
        let healers: Vec<(PlayerId, usize)> = self
            .network_players
            .iter()
            .filter(|(id, _)| self.network_classes.get(id).is_some_and(|c| c.can_revive()))
            .filter(|(id, _)| self.network_inputs.get(id).is_some_and(|i| i.heal))
            .filter(|(id, _)| self.network_health.get(id).copied().unwrap_or(0.0) > 0.0)
            .map(|(&id, &idx)| (id, idx))
            .collect();

        // Oublie le canal de tout Soutien qui ne remplit plus les conditions
        // de base ci-dessus (heal relâché, déconnecté...) — sans ce nettoyage,
        // un canal interrompu resterait accumulé en attente d'une reprise
        // plutôt que d'exiger un canal continu comme le décrit le GDD.
        let healer_ids: std::collections::HashSet<PlayerId> =
            healers.iter().map(|(id, _)| *id).collect();
        self.network_revive.retain(|id, _| healer_ids.contains(id));

        for (healer_id, healer_idx) in healers {
            let Some(healer_pos) = self
                .scene
                .objects
                .get(healer_idx)
                .map(|o| o.transform.position)
            else {
                continue;
            };
            // Allié spectateur (0 PV, jamais un simple blessé — cf.
            // `update_network_heal` pour ce cas) le plus proche à portée.
            let target_id = self
                .network_players
                .iter()
                .filter(|(id, _)| **id != healer_id)
                .filter_map(|(&id, &idx)| {
                    let hp = self.network_health.get(&id).copied().unwrap_or(1.0);
                    if hp > 0.0 {
                        return None;
                    }
                    let pos = self.scene.objects.get(idx)?.transform.position;
                    let dist = pos.distance(healer_pos);
                    (dist <= REVIVE_RANGE).then_some((id, dist))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id);

            let Some(target_id) = target_id else {
                self.network_revive.remove(&healer_id);
                continue;
            };

            let entry = self
                .network_revive
                .entry(healer_id)
                .or_insert((target_id, 0.0));
            if entry.0 != target_id {
                // Cible changée : le canal recommence, aucun progrès reporté.
                *entry = (target_id, 0.0);
            }
            entry.1 += dt;
            if entry.1 >= REVIVE_DURATION {
                let max_hp = self.max_health_for(target_id);
                if let Some(hp) = self.network_health.get_mut(&target_id) {
                    *hp = max_hp * REVIVE_HEALTH_FRACTION;
                }
                if let Some(&idx) = self.network_players.get(&target_id)
                    && let Some(o) = self.scene.objects.get_mut(idx)
                {
                    o.visible = true;
                }
                self.network_revive.remove(&healer_id);
                self.revives_completed += 1;
            }
        }
    }

    /// Défaite de **salon** en multijoueur (GAMEDESIGN_EN_LIGNE.md §3.1) : vrai
    /// quand au moins un joueur réseau est connu et que TOUS sont vaincus, **ou**
    /// (mode Escorte, Sprint 7 de `sprint10audit.md`) quand le convoi est détruit —
    /// ce second cas prime sur l'état des joueurs : un convoi anéanti perd la manche
    /// même si des joueurs sont encore en vie (GDD §4, contrairement aux autres
    /// modes où seule la mort de tous les joueurs compte). Remplace `is_lost()`
    /// (pensé pour un joueur local unique) côté serveur headless dès qu'un salon a
    /// des joueurs réseau ; en solo (aucun joueur réseau) et hors mode Escorte,
    /// retombe sur `is_lost()`, inchangé — aucune régression.
    pub fn is_room_lost(&self) -> bool {
        if self.objective == crate::app::multiplayer::RoundObjective::Escorte
            && self.is_convoy_destroyed()
        {
            return true;
        }
        if self.network_players.is_empty() {
            self.is_lost()
        } else {
            self.network_players
                .keys()
                .all(|id| self.network_health.get(id).copied().unwrap_or(MAX_HEALTH) <= 0.0)
        }
    }

    /// Convoi détruit (mode Escorte, Sprint 7) : présent dans la scène mais rendu
    /// invisible par `Scene::damage_attackable` une fois ses PV à 0 — `false` si la
    /// scène n'a pas d'objet `convoy` (mauvais mode/scène) plutôt que de compter ça
    /// comme une défaite immédiate.
    fn is_convoy_destroyed(&self) -> bool {
        self.scene
            .objects
            .iter()
            .find(|o| o.convoy.is_some())
            .is_some_and(|o| !o.visible)
    }

    /// Vie (0..1) du joueur réseau `id`, `None` s'il n'est pas connecté.
    pub fn network_player_health(&self, id: PlayerId) -> Option<f32> {
        self.network_health.get(&id).copied()
    }

    /// PV max du joueur réseau `id` (base `MAX_HEALTH` modulée par sa classe,
    /// GDD §3.2 — ex. Éclaireur ×0,70) : `spawn_network_player` la calcule une
    /// fois pour toutes au spawn. `MAX_HEALTH` en repli si `id` est inconnu
    /// (jamais censé arriver après un spawn, mais un repli sûr plutôt qu'un
    /// panic pour un id qui ne serait plus connecté).
    pub(super) fn max_health_for(&self, id: PlayerId) -> f32 {
        self.network_max_health
            .get(&id)
            .copied()
            .unwrap_or(MAX_HEALTH)
    }

    /// `true` si l'objet `index` n'est **pas** un joueur réseau vaincu — `true`
    /// par défaut pour tout objet qui n'est pas un joueur réseau (joueur local,
    /// monstre, décor...), qui n'a pas de notion de vie individualisée ici.
    /// Sert à exclure un joueur à 0 PV des tirs/attaques (cf. `fireball.rs`,
    /// `multiplayer::update_network_attacks`).
    pub(super) fn is_alive_at(&self, index: usize) -> bool {
        self.network_players
            .iter()
            .find(|&(_, &idx)| idx == index)
            .is_none_or(|(id, _)| self.network_health.get(id).copied().unwrap_or(MAX_HEALTH) > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::super::AppState;
    use super::MAX_HEALTH;
    use crate::app::multiplayer::{NetworkInput, PlayerClass};
    use crate::net::protocol::{DeathCauseKind, GameEvent};
    use crate::scene::{AiChaser, Combat, Controller, MeshKind, Scene, SceneObject, Transform};

    /// Scène minimale : un sol (les joueurs doivent tenir debout, sans quoi ils
    /// tombent dans le vide et les tests de contact/portée deviennent instables
    /// — même bug de scène de test corrigé une fois pour `fireball.rs`), un
    /// gabarit joueur pilotable, et éventuellement un monstre `AiChaser` au
    /// centre (contact garanti pour un joueur qui y reste).
    fn scene_with_optional_monster(with_monster: bool) -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: MeshKind::Plane,
            transform: Transform::from_pos(Vec3::ZERO).with_scale(Vec3::new(40.0, 1.0, 40.0)),
            physics: crate::runtime::physics::PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Capsule,
            transform: Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        if with_monster {
            scene.objects.push(SceneObject {
                name: "Monstre".into(),
                mesh: MeshKind::Cube,
                transform: Transform::from_pos(Vec3::new(0.0, 1.0, 0.0))
                    .with_scale(Vec3::splat(1.0)),
                ai_chaser: Some(AiChaser {
                    speed: 0.0,
                    ..Default::default()
                }),
                combat: Some(Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        scene
    }

    fn app_with(scene: Scene) -> AppState {
        let mut app = AppState::new();
        app.scene = scene;
        app.playing = true;
        app
    }

    fn advance(app: &mut AppState, frames: usize, frame_dt: f32) {
        for _ in 0..frames {
            app.last_frame =
                std::time::Instant::now() - std::time::Duration::from_secs_f32(frame_dt);
            app.advance_play();
        }
    }

    fn net_input() -> NetworkInput {
        NetworkInput {
            move_x: 0.0,
            move_y: 0.0,
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
        }
    }

    #[test]
    fn joining_starts_at_full_health() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        app.spawn_network_player(1, PlayerClass::Assault);
        assert_eq!(app.network_player_health(1), Some(MAX_HEALTH));
    }

    #[test]
    fn contact_with_a_visible_chaser_drains_health_over_time() {
        let mut app = app_with(scene_with_optional_monster(true));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        // Replace le joueur pile sur le monstre (contact garanti). Appelle
        // `update_network_health` **directement**, pas `advance_play`/la
        // physique : deux corps rigides placés au même point se repousseraient
        // l'un l'autre par une violente impulsion dès le premier pas physique
        // (même piège documenté pour `spawn_network_player`, cf. `SPAWN_
        // RADIUS`) — sans rapport avec ce qu'on teste ici (la détection de
        // contact et le décompte de vie), qui n'a besoin d'aucune simulation.
        let monster_pos = app.scene.objects[2].transform.position;
        app.scene.objects[index].transform.position = monster_pos;

        for _ in 0..30 {
            app.update_network_health(0.05); // 1,5 s de contact continu
        }

        let hp = app.network_player_health(1).unwrap();
        assert!(
            hp < MAX_HEALTH,
            "un contact soutenu avec un monstre visible doit user la vie : {hp}"
        );
    }

    #[test]
    fn health_regenerates_passively_out_of_contact() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.network_health.insert(1, 0.3);

        advance(&mut app, 40, 0.05); // 2 s hors de tout contact

        let hp = app.network_player_health(1).unwrap();
        assert!(
            hp > 0.3,
            "sans contact, la vie doit régénérer passivement : {hp}"
        );
    }

    #[test]
    fn dying_hides_the_player_and_queues_a_player_down_event() {
        let mut app = app_with(scene_with_optional_monster(true));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let monster_pos = app.scene.objects[2].transform.position;
        app.scene.objects[index].transform.position = monster_pos;

        // ~7 s de contact continu : de quoi épuiser toute la vie (MONSTER_CONTACT_DPS
        // ≈ 0,16/s ⇒ ~6,25 s pour vider 1.0), avec marge. Appel direct (pas
        // `advance_play`) — même raison que le test précédent : la physique
        // séparerait les deux corps placés au même point, sans rapport avec
        // ce qu'on vérifie ici.
        for _ in 0..160 {
            app.update_network_health(0.05);
        }

        assert_eq!(app.network_player_health(1), Some(0.0));
        assert!(
            !app.scene.objects[index].visible,
            "un joueur à 0 PV doit devenir spectateur (objet masqué)"
        );
        let events = app.take_net_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::PlayerDown { player_id: 1, .. })),
            "la mort d'un joueur réseau doit être diffusée : {events:?}"
        );
    }

    /// Sprint 2 (`sprint10audit.md`) : la mort par contact monstre doit porter
    /// une cause exploitable côté client (diagnostic de mort, GDD §16.5), pas
    /// juste l'identifiant de la victime.
    #[test]
    fn death_by_monster_contact_carries_a_death_cause() {
        let mut app = app_with(scene_with_optional_monster(true));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let monster_pos = app.scene.objects[2].transform.position;
        app.scene.objects[index].transform.position = monster_pos;

        for _ in 0..160 {
            app.update_network_health(0.05);
        }

        let events = app.take_net_events();
        let cause = events.iter().find_map(|e| match e {
            GameEvent::PlayerDown {
                player_id: 1,
                cause,
            } => Some(*cause),
            _ => None,
        });
        assert_eq!(
            cause,
            Some(Some(crate::net::protocol::DeathCause {
                kind: DeathCauseKind::Monster,
                distinct_attackers: 1,
            })),
            "un seul monstre au contact doit produire une cause Monster/1 : {events:?}"
        );
    }

    /// Symétrique du test ci-dessus pour le cas « Encerclé » (GDD §16.5,
    /// exemple cité littéralement : « Encerclé — 2 Traqueuses ») : deux
    /// monstres au contact **au même instant** doivent produire
    /// `distinct_attackers: 2`, pas 1 — sans ce test, une régression qui
    /// écraserait la fenêtre à un seul agresseur (ex. `push_back` remplacé par
    /// une simple affectation) serait passée inaperçue.
    #[test]
    fn death_by_two_simultaneous_monsters_reports_two_distinct_attackers() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let player_pos = app.scene.objects[index].transform.position;
        for i in 0..2 {
            app.scene.objects.push(SceneObject {
                name: format!("Monstre {i}"),
                mesh: MeshKind::Cube,
                transform: Transform::from_pos(player_pos).with_scale(Vec3::splat(1.0)),
                ai_chaser: Some(AiChaser {
                    speed: 0.0,
                    ..Default::default()
                }),
                combat: Some(Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            });
        }

        for _ in 0..160 {
            app.update_network_health(0.05);
        }

        let events = app.take_net_events();
        let cause = events.iter().find_map(|e| match e {
            GameEvent::PlayerDown {
                player_id: 1,
                cause,
            } => Some(*cause),
            _ => None,
        });
        assert_eq!(
            cause,
            Some(Some(crate::net::protocol::DeathCause {
                kind: DeathCauseKind::Monster,
                distinct_attackers: 2,
            })),
            "deux monstres au contact simultané doivent produire une cause \
             Monster/2 (« Encerclé ») : {events:?}"
        );
    }

    #[test]
    fn room_is_lost_only_once_every_network_player_is_defeated() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.spawn_network_player(2, PlayerClass::Assault);

        assert!(!app.is_room_lost(), "deux joueurs en vie : le salon tient");

        app.network_health.insert(1, 0.0);
        assert!(
            !app.is_room_lost(),
            "un seul joueur vaincu sur deux : le salon continue"
        );

        app.network_health.insert(2, 0.0);
        assert!(
            app.is_room_lost(),
            "tous les joueurs réseau vaincus : le salon est perdu"
        );
    }

    #[test]
    fn a_defeated_players_movement_and_attack_inputs_are_ignored() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.network_health.insert(1, 0.0);
        // Un vrai mort est masqué (cf. `update_network_health`) : sans ça, la
        // régénération passive (objet visible ⇒ « vivant mais blessé », pas de
        // monstre à proximité dans cette scène) ramènerait sa vie au-dessus de
        // 0 dès la frame suivante, invalidant le test.
        app.scene.objects[index].visible = false;
        let start = app.scene.objects[index].transform.position;
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 1.0,
                move_y: 0.0,
                ..net_input()
            },
        );

        advance(&mut app, 30, 0.05);

        let end = app.scene.objects[index].transform.position;
        let horiz = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
        assert!(
            horiz < 0.5,
            "un joueur vaincu ne doit plus pouvoir se déplacer : {start:?} -> {end:?}"
        );
    }

    #[test]
    fn healing_transfers_health_to_the_nearest_wounded_ally_in_range() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let healer = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let wounded = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        // Rapproche l'allié blessé, à portée de soin.
        let healer_pos = app.scene.objects[healer].transform.position;
        app.scene.objects[wounded].transform.position = healer_pos + Vec3::new(1.0, 0.0, 0.0);
        app.network_health.insert(2, 0.4);
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );

        advance(&mut app, 20, 0.05); // 1 s de soin continu

        let hp = app.network_player_health(2).unwrap();
        assert!(
            hp > 0.4,
            "le soin doit augmenter la vie de l'allié blessé à portée : {hp}"
        );
    }

    #[test]
    fn healing_ignores_allies_out_of_range() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let healer = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let far = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        let healer_pos = app.scene.objects[healer].transform.position;
        app.scene.objects[far].transform.position = healer_pos + Vec3::new(50.0, 0.0, 0.0);
        app.network_health.insert(2, 0.4);
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );

        // Appel direct à `update_network_heal` (pas `advance_play`, qui
        // enchaînerait aussi `update_network_health` — la régénération passive
        // ferait alors dériver la vie indépendamment du soin, brouillant ce
        // qu'on isole ici : la portée du soin, rien d'autre).
        for _ in 0..20 {
            app.update_network_heal(0.05);
        }

        assert_eq!(
            app.network_player_health(2),
            Some(0.4),
            "un allié hors de portée ne doit pas être soigné"
        );
    }

    #[test]
    fn a_healer_never_heals_a_fully_healthy_ally_over_a_farther_wounded_one() {
        // Non testé plus finement ici (portée/priorité déjà couvertes ci-dessus) :
        // vérifie juste qu'un allié déjà au max n'est pas la cible retenue quand
        // un autre, blessé, est aussi à portée.
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let healer = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let healthy = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        let wounded = app.spawn_network_player(3, PlayerClass::Assault).unwrap();
        let healer_pos = app.scene.objects[healer].transform.position;
        app.scene.objects[healthy].transform.position = healer_pos + Vec3::new(0.5, 0.0, 0.0);
        app.scene.objects[wounded].transform.position = healer_pos + Vec3::new(-0.5, 0.0, 0.0);
        app.network_health.insert(3, 0.3);
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );

        advance(&mut app, 20, 0.05);

        assert_eq!(app.network_player_health(2), Some(MAX_HEALTH));
        assert!(app.network_player_health(3).unwrap() > 0.3);
    }

    /// GDD §8.1 : le Soutien soigne ×2,5 le débit universel — un Soutien et
    /// un Assaut soignant le même allié blessé le même temps ne rendent pas
    /// la même vie.
    #[test]
    fn support_class_heals_faster_than_the_universal_rate() {
        let mut assault_app = app_with(scene_with_optional_monster(false));
        assault_app.hide_local_player_template();
        let healer = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let wounded = assault_app
            .spawn_network_player(2, PlayerClass::Assault)
            .unwrap();
        let healer_pos = assault_app.scene.objects[healer].transform.position;
        assault_app.scene.objects[wounded].transform.position =
            healer_pos + Vec3::new(1.0, 0.0, 0.0);
        assault_app.network_health.insert(2, 0.1);
        assault_app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );
        for _ in 0..20 {
            assault_app.update_network_heal(0.05);
        }
        let assault_healed = assault_app.network_player_health(2).unwrap();

        let mut support_app = app_with(scene_with_optional_monster(false));
        support_app.hide_local_player_template();
        let healer = support_app
            .spawn_network_player(1, PlayerClass::Support)
            .unwrap();
        let wounded = support_app
            .spawn_network_player(2, PlayerClass::Assault)
            .unwrap();
        let healer_pos = support_app.scene.objects[healer].transform.position;
        support_app.scene.objects[wounded].transform.position =
            healer_pos + Vec3::new(1.0, 0.0, 0.0);
        support_app.network_health.insert(2, 0.1);
        support_app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );
        for _ in 0..20 {
            support_app.update_network_heal(0.05);
        }
        let support_healed = support_app.network_player_health(2).unwrap();

        assert!(
            support_healed > assault_healed,
            "le Soutien doit soigner plus vite : {support_healed} <= {assault_healed}"
        );
    }

    /// GDD §8.1 : « seul à réanimer » — un canal continu de 10 s ramène un
    /// spectateur à 30 % PV, exclusivité du Soutien.
    #[test]
    fn a_support_channeling_ten_seconds_revives_a_downed_ally_to_30_percent() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let healer = app.spawn_network_player(1, PlayerClass::Support).unwrap();
        let downed = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        let healer_pos = app.scene.objects[healer].transform.position;
        app.scene.objects[downed].transform.position = healer_pos + Vec3::new(1.0, 0.0, 0.0);
        app.network_health.insert(2, 0.0);
        app.scene.objects[downed].visible = false;
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );

        for _ in 0..199 {
            app.update_network_revive(0.05);
        }
        assert_eq!(
            app.network_player_health(2),
            Some(0.0),
            "à moins de 10 s de canal, la cible doit rester vaincue"
        );
        assert!(
            !app.scene.objects[downed].visible,
            "toujours spectateur avant la fin du canal"
        );

        app.update_network_revive(0.05); // franchit les 10 s (199×0,05 + 0,05 = 10.0)

        assert_eq!(
            app.network_player_health(2),
            Some(MAX_HEALTH * 0.3),
            "la réanimation doit rendre exactement 30 % des PV max"
        );
        assert!(
            app.scene.objects[downed].visible,
            "l'allié réanimé doit redevenir visible"
        );
    }

    /// Une classe autre que Soutien ne peut jamais réanimer, même en
    /// maintenant `heal` à portée d'un allié vaincu — c'est l'exclusivité
    /// même de la classe (GDD §8.1).
    #[test]
    fn a_non_support_player_never_revives_a_downed_ally() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let healer = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let downed = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        let healer_pos = app.scene.objects[healer].transform.position;
        app.scene.objects[downed].transform.position = healer_pos + Vec3::new(1.0, 0.0, 0.0);
        app.network_health.insert(2, 0.0);
        app.scene.objects[downed].visible = false;
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );

        for _ in 0..250 {
            app.update_network_revive(0.05);
        }

        assert_eq!(
            app.network_player_health(2),
            Some(0.0),
            "un Assaut/Éclaireur ne réanime jamais, quelle que soit la durée du canal"
        );
    }

    /// GDD §5.3 : « 10 s de canal immobile » — un canal interrompu (heal
    /// relâché à mi-chemin) ne conserve pas son progrès ; le reprendre repart
    /// de zéro, jamais de là où il s'était arrêté.
    #[test]
    fn an_interrupted_revive_channel_loses_its_progress() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        let healer = app.spawn_network_player(1, PlayerClass::Support).unwrap();
        let downed = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        let healer_pos = app.scene.objects[healer].transform.position;
        app.scene.objects[downed].transform.position = healer_pos + Vec3::new(1.0, 0.0, 0.0);
        app.network_health.insert(2, 0.0);
        app.scene.objects[downed].visible = false;
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );
        for _ in 0..100 {
            app.update_network_revive(0.05); // 5 s de canal, à mi-chemin
        }

        // Relâche `heal` un tick : le canal doit être abandonné.
        app.set_network_input(1, net_input());
        app.update_network_revive(0.05);

        // Reprend le canal : encore 9,9 s ne doivent PAS suffire (il faudrait
        // les 10 s pleines si le progrès avait été conservé).
        app.set_network_input(
            1,
            NetworkInput {
                heal: true,
                ..net_input()
            },
        );
        for _ in 0..198 {
            app.update_network_revive(0.05);
        }

        assert_eq!(
            app.network_player_health(2),
            Some(0.0),
            "un canal interrompu doit repartir de zéro, pas reprendre où il s'était arrêté"
        );
    }
}
