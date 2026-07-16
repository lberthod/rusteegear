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

use glam::Vec3;

use super::AppState;
use crate::net::protocol::{GameEvent, PlayerId};

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
        let monster_aabbs: Vec<(Vec3, Vec3)> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.ai_chaser.is_some() && o.visible)
            .map(|o| self.scene.world_aabb(o))
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
            let touched = monster_aabbs
                .iter()
                .any(|&aabb| aabbs_overlap(aabb, player_aabb));
            let was_alive = self.network_health.get(&id).copied().unwrap_or(MAX_HEALTH) > 0.0;
            let hp = self.network_health.entry(id).or_insert(MAX_HEALTH);
            if touched {
                *hp = (*hp - MONSTER_CONTACT_DPS * dt).max(0.0);
            } else {
                *hp = (*hp + REGEN_PER_S * dt).min(MAX_HEALTH);
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
                self.pending_net_events
                    .push(GameEvent::PlayerDown { player_id: id });
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
                let just_died = was_alive && *hp <= 0.0;
                if just_died {
                    if let Some(o) = self.scene.objects.get_mut(index) {
                        o.visible = false;
                    }
                    self.pending_net_events
                        .push(GameEvent::PlayerDown { player_id: id });
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Lose);
                } else {
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
            // Allié vivant, blessé (pas déjà à `MAX_HEALTH` : sans ce filtre, un
            // soigneur entouré d'alliés au max ne soignerait jamais personne
            // d'autre malgré un blessé un peu plus loin, mais à portée), le
            // plus proche à portée — jamais soi-même.
            let target_id = self
                .network_players
                .iter()
                .filter(|(id, _)| **id != healer_id)
                .filter_map(|(&id, &idx)| {
                    let hp = self.network_health.get(&id).copied().unwrap_or(0.0);
                    if hp <= 0.0 || hp >= MAX_HEALTH {
                        return None;
                    }
                    let pos = self.scene.objects.get(idx)?.transform.position;
                    let dist = pos.distance(healer_pos);
                    (dist <= HEAL_RANGE).then_some((id, dist))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id);
            if let Some(target_id) = target_id
                && let Some(hp) = self.network_health.get_mut(&target_id)
            {
                *hp = (*hp + HEAL_RATE_PER_S * dt).min(MAX_HEALTH);
            }
        }
    }

    /// Défaite de **salon** en multijoueur (GAMEDESIGN_EN_LIGNE.md §3.1) : vrai
    /// quand au moins un joueur réseau est connu et que TOUS sont vaincus.
    /// Remplace `is_lost()` (pensé pour un joueur local unique) côté serveur
    /// headless dès qu'un salon a des joueurs réseau ; en solo (aucun joueur
    /// réseau), retombe sur `is_lost()`, inchangé — aucune régression.
    pub fn is_room_lost(&self) -> bool {
        if self.network_players.is_empty() {
            self.is_lost()
        } else {
            self.network_players
                .keys()
                .all(|id| self.network_health.get(id).copied().unwrap_or(MAX_HEALTH) <= 0.0)
        }
    }

    /// Vie (0..1) du joueur réseau `id`, `None` s'il n'est pas connecté.
    pub fn network_player_health(&self, id: PlayerId) -> Option<f32> {
        self.network_health.get(&id).copied()
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
    use crate::app::multiplayer::NetworkInput;
    use crate::net::protocol::GameEvent;
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
                ai_chaser: Some(AiChaser { speed: 0.0 }),
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
        app.spawn_network_player(1);
        assert_eq!(app.network_player_health(1), Some(MAX_HEALTH));
    }

    #[test]
    fn contact_with_a_visible_chaser_drains_health_over_time() {
        let mut app = app_with(scene_with_optional_monster(true));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1).unwrap();
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
        app.spawn_network_player(1);
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
        let index = app.spawn_network_player(1).unwrap();
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
                .any(|e| matches!(e, GameEvent::PlayerDown { player_id: 1 })),
            "la mort d'un joueur réseau doit être diffusée : {events:?}"
        );
    }

    #[test]
    fn room_is_lost_only_once_every_network_player_is_defeated() {
        let mut app = app_with(scene_with_optional_monster(false));
        app.hide_local_player_template();
        app.spawn_network_player(1);
        app.spawn_network_player(2);

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
        let index = app.spawn_network_player(1).unwrap();
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
        let healer = app.spawn_network_player(1).unwrap();
        let wounded = app.spawn_network_player(2).unwrap();
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
        let healer = app.spawn_network_player(1).unwrap();
        let far = app.spawn_network_player(2).unwrap();
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
        let healer = app.spawn_network_player(1).unwrap();
        let healthy = app.spawn_network_player(2).unwrap();
        let wounded = app.spawn_network_player(3).unwrap();
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
}
