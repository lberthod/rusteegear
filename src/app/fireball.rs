//! Attaque à distance « boule de feu » : un projectile part **devant** le tireur
//! (le long de son orientation, contrairement au missile homing de `combat.rs` qui
//! verrouille une cible), avance en ligne droite et frappe le premier obstacle
//! physique ou monstre `attackable` sur son chemin.
//!
//! Même simulation en solo (APK/macOS hors ligne, tireur = joueur local) et sur le
//! serveur autoritaire (tireurs = joueurs réseau, cf. `NetworkInput::fire`) — un
//! client **connecté** ne simule rien : il envoie `fire` au serveur et affiche les
//! projectiles du `Snapshot` (cf. `Snapshot::projectiles`), le serveur validant le
//! temps de recharge comme pour l'attaque au contact (`update_network_attacks`).

use glam::Vec3;

use super::AppState;
use crate::net::protocol::GameEvent;
use crate::runtime::physics::PhysicsKind;

/// Boule de feu en vol (cf. `AppState::fireballs`).
pub(super) struct Fireball {
    /// Indice de l'objet tireur dans `scene.objects` : jamais frappé par sa
    /// propre boule de feu (elle naît dans son AABB).
    pub(super) owner: usize,
    pub(super) pos: Vec3,
    /// Direction de vol (horizontale, normalisée), figée au tir.
    pub(super) dir: Vec3,
    /// Durée de vie restante (s) : écoulée sans impact, la boule s'éteint —
    /// borne la portée sans avoir à tester la distance parcourue.
    pub(super) remaining: f32,
}

/// Vitesse de vol (m/s) : nettement plus rapide que le missile homing
/// (`combat::ATTACK_PROJECTILE_SPEED`, 10 m/s) mais en ligne droite — l'adresse
/// de visée remplace le verrouillage automatique comme « coût » du tir.
const FIREBALL_SPEED: f32 = 12.0;

/// Temps de recharge (s) entre deux tirs d'un même tireur, validé côté
/// simulation (serveur pour les joueurs réseau) : maintenir ou spammer le
/// bouton ne tire pas plus vite. Cf. `AppState::fireball_cooldowns`.
pub(super) const FIREBALL_COOLDOWN: f32 = 0.9;

/// Durée de vie (s) ⇒ portée max ≈ `SPEED × LIFETIME` = 18 m : de quoi traverser
/// l'arène embarquée (murs à ±9 m) sans jamais voler indéfiniment.
const FIREBALL_LIFETIME: f32 = 1.5;

/// Rayon (m) de la boule : les AABB testés sont gonflés d'autant, pour qu'un
/// frôlement visuel compte comme un impact (le test exact point-dans-AABB
/// exigerait de toucher le centre géométrique — frustrant à viser).
const FIREBALL_RADIUS: f32 = 0.35;

/// Distance (m) devant le tireur à laquelle la boule apparaît : hors de son
/// propre AABB, pour ne pas exiger de cas particulier au premier pas de vol.
const SPAWN_AHEAD: f32 = 0.8;

/// Hauteur (m) au-dessus du centre du tireur : la boule part du « buste », assez
/// haut pour survoler le sol (plan mince à y=0) sur toute sa trajectoire.
const SPAWN_UP: f32 = 0.4;

/// Ce que la boule de feu a frappé ce pas-ci (cf. `fireball_impact`).
enum Impact {
    /// Un monstre `attackable` : blessé (`Scene::damage_attackable`), la boule
    /// s'éteint dans tous les cas (pas de perforation multi-cibles).
    Monster(usize),
    /// Un obstacle physique (mur, tour, décor `Static`/`Dynamic`) : la boule
    /// s'éteint sans effet — c'est ce qui rend un mur utilisable comme abri.
    Obstacle,
}

impl AppState {
    /// Fait vivre les boules de feu pour cette frame : tirs (joueur local en solo,
    /// joueurs réseau côté serveur), vol, impacts, et pool d'affichage. Appelée une
    /// fois par frame depuis `advance_play`, comme `update_attack` (dt réel).
    pub(super) fn update_fireballs(&mut self, dt: f32) {
        // Recharges : décomptées chaque frame, indépendamment du bouton (sinon
        // relâcher puis rappuyer contournerait le temporisateur) — mêmes raisons
        // que `attack_cooldown_remaining` (cf. `combat.rs`).
        self.fireball_cooldowns.retain(|_, cd| {
            *cd -= dt;
            *cd > 0.0
        });

        // Tir du joueur local — en solo uniquement : connecté, le serveur est
        // autoritaire (l'input part via `network_input_msg`, les projectiles
        // reviennent par le `Snapshot`) ; simuler aussi localement ferait vivre
        // deux boules pour un seul tir (une vraie + une fantôme).
        if !self.is_online_client()
            && let Some(pi) = self.player_index()
            && let Some(player) = self.scene.objects.get(pi)
            && player.visible
            && let Some(ctrl) = player.controller.clone()
        {
            let pressed = self.input_state.fire
                || (!ctrl.fire_button.is_empty()
                    && self.input_state.buttons.contains(&ctrl.fire_button));
            if pressed && !self.fireball_cooldowns.contains_key(&pi) {
                self.spawn_fireball(pi);
            }
        }

        // Tirs des joueurs réseau (serveur autoritaire) : même recharge, par objet
        // tireur — un client modifié qui envoie `fire: true` à chaque tick ne tire
        // pas plus vite (cf. le même durcissement dans `update_network_attacks`).
        let shooters: Vec<usize> = self
            .network_players
            .iter()
            .filter(|(id, _)| self.network_inputs.get(id).is_some_and(|i| i.fire))
            .map(|(_, &index)| index)
            .filter(|index| !self.fireball_cooldowns.contains_key(index))
            .collect();
        for index in shooters {
            self.spawn_fireball(index);
        }

        // Vol + impacts. `mem::take` pour itérer sans bloquer l'emprunt de `self`
        // (les impacts mutent la scène) ; les survivantes sont remises en place.
        let mut flying = std::mem::take(&mut self.fireballs);
        flying.retain_mut(|fb| {
            fb.remaining -= dt;
            if fb.remaining <= 0.0 {
                return false;
            }
            fb.pos += fb.dir * FIREBALL_SPEED * dt;
            true
        });
        let mut survivors = Vec::with_capacity(flying.len());
        for fb in flying {
            match self.fireball_impact(&fb) {
                Some(Impact::Monster(i)) => self.resolve_fireball_hit(i, fb.pos),
                Some(Impact::Obstacle) => {}
                None => survivors.push(fb),
            }
        }
        self.fireballs = survivors;

        // Pool d'affichage : positions simulées ici (solo/serveur), ou reçues du
        // dernier `Snapshot` (client connecté — `self.fireballs` y reste vide).
        let positions: Vec<Vec3> = if self.is_online_client() {
            self.net_projectiles.clone()
        } else {
            self.fireballs.iter().map(|fb| fb.pos).collect()
        };
        self.sync_fireball_pool(&positions);
    }

    /// Fait partir une boule de feu devant l'objet `owner` et arme sa recharge.
    fn spawn_fireball(&mut self, owner: usize) {
        let Some(o) = self.scene.objects.get(owner) else {
            return;
        };
        let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
        // « Devant » = l'avant du personnage : -Z à yaw 0, la même convention que
        // la poussée tank W (cf. `network_move_axes` : vitesse monde
        // `(-sin yaw, 0, -cos yaw)`) — la boule part là où le joueur regarde.
        let dir = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let pos = o.transform.position + dir * SPAWN_AHEAD + Vec3::Y * SPAWN_UP;
        self.fireballs.push(Fireball {
            owner,
            pos,
            dir,
            remaining: FIREBALL_LIFETIME,
        });
        self.fireball_cooldowns.insert(owner, FIREBALL_COOLDOWN);
    }

    /// Premier objet frappé par la boule `fb` à sa position courante, s'il y en a
    /// un. Ignorés : le tireur lui-même, les objets masqués, tout objet pilotable
    /// (joueurs — pas de dégâts joueur-contre-joueur tant que la vie n'est pas
    /// individualisée, cf. `network_snapshot`), l'ancre FX d'attaque, et les
    /// objets ni `attackable` ni physiques (fantômes réseau, pool de boules...).
    fn fireball_impact(&self, fb: &Fireball) -> Option<Impact> {
        for (i, o) in self.scene.objects.iter().enumerate() {
            if i == fb.owner || !o.visible || o.controller.is_some() {
                continue;
            }
            let attackable = o.combat.as_ref().is_some_and(|c| c.attackable);
            if o.combat.as_ref().is_some_and(|c| c.is_attack_fx) {
                continue;
            }
            let solid = o.physics != PhysicsKind::None;
            if !attackable && !solid {
                continue;
            }
            let (wmin, wmax) = self.scene.world_aabb(o);
            let inflate = Vec3::splat(FIREBALL_RADIUS);
            let hit = fb.pos.cmpge(wmin - inflate).all() && fb.pos.cmple(wmax + inflate).all();
            if !hit {
                continue;
            }
            return Some(if attackable {
                Impact::Monster(i)
            } else {
                Impact::Obstacle
            });
        }
        None
    }

    /// Résout l'impact sur le monstre `i` : dégât, score, son, flash, respawn, et
    /// évènement réseau `Defeated` si le coup l'achève (diffusé par le serveur
    /// headless, cf. `take_net_events` — les clients y réagissent une fois, son +
    /// flash, sans attendre le prochain `Snapshot`).
    fn resolve_fireball_hit(&mut self, i: usize, at: Vec3) {
        let defeated = self.scene.damage_attackable(i);
        self.attack_flash = 1.0;
        if let Some(fx) = self.attack_fx_index()
            && let Some(o) = self.scene.objects.get_mut(fx)
        {
            o.transform.position = at;
            o.transform.scale = Vec3::splat(1.2);
            o.visible = true;
        }
        if defeated {
            self.score += 1;
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Defeat);
            self.pending_net_events
                .push(GameEvent::Defeated { index: i as u32 });
            let d = self.scene.objects[i].respawn_delay;
            if d > 0.0 {
                self.respawn_queue.push((i, self.time + d));
            }
        } else {
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
        }
    }

    /// Aligne le pool d'affichage (sphères émissives) sur `positions` : agrandit le
    /// pool à la demande, masque les sphères en trop. Les objets du pool restent en
    /// place une fois créés (les retirer décalerait tous les indices de
    /// `scene.objects` — même contrainte que `despawn_network_player`).
    pub(super) fn sync_fireball_pool(&mut self, positions: &[Vec3]) {
        while self.fireball_pool.len() < positions.len() {
            let index = self.scene.objects.len();
            self.scene.objects.push(crate::scene::SceneObject {
                name: format!("Boule de feu {}", self.fireball_pool.len() + 1),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                    .with_scale(Vec3::splat(FIREBALL_RADIUS * 2.0)),
                color: [1.0, 0.45, 0.1],
                emissive: 2.0,
                physics: PhysicsKind::None,
                visible: false,
                ..Default::default()
            });
            self.fireball_pool.push(index);
        }
        for (slot, &index) in self.fireball_pool.iter().enumerate() {
            if let Some(o) = self.scene.objects.get_mut(index) {
                match positions.get(slot) {
                    Some(&p) => {
                        o.transform.position = p;
                        o.visible = true;
                    }
                    None => o.visible = false,
                }
            }
        }
    }

    /// Oublie toutes les boules de feu, recharges et le pool d'affichage : à
    /// appeler chaque fois que `scene.objects` est restauré en bloc (mêmes sites
    /// que `clear_network_players`) — le pool vit dans `scene.objects`, ses
    /// indices deviennent obsolètes après restauration.
    pub(super) fn clear_fireballs(&mut self) {
        self.fireballs.clear();
        self.fireball_cooldowns.clear();
        self.fireball_pool.clear();
        self.net_projectiles.clear();
        self.pending_net_events.clear();
    }

    /// Évènements de gameplay en attente de diffusion (monstre vaincu...), drainés
    /// par le serveur headless à chaque tick (`src/bin/server.rs`), qui les
    /// broadcast en `ServerMsg::Event`.
    pub fn take_net_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.pending_net_events)
    }

    /// `true` si cette instance est un **client** connecté à un serveur (jamais le
    /// cas du serveur headless, qui n'a pas de `NetClient`) : la simulation locale
    /// des boules de feu s'efface alors devant l'autorité du serveur.
    fn is_online_client(&self) -> bool {
        #[cfg(not(target_os = "ios"))]
        {
            self.net_client.is_some()
        }
        #[cfg(target_os = "ios")]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::super::AppState;
    use crate::app::multiplayer::NetworkInput;
    use crate::runtime::physics::PhysicsKind;
    use crate::scene::{Combat, Controller, MeshKind, Scene, SceneObject, Transform};

    /// Arène minimale : un joueur pilotable en (0, 1, 0) orienté vers -Z (yaw 0),
    /// un monstre `attackable` droit devant à -6 m, et un mur optionnel entre les
    /// deux — de quoi vérifier vol, impact, abri et recharge sans charger une démo.
    fn scene_with_monster_ahead(wall_between: bool) -> Scene {
        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Capsule,
            transform: Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(Controller {
                input: true,
                fire_button: "Feu".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Monstre".into(),
            mesh: MeshKind::Cube,
            transform: Transform::from_pos(Vec3::new(0.0, 1.0, -6.0)).with_scale(Vec3::splat(1.2)),
            combat: Some(Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        if wall_between {
            scene.objects.push(SceneObject {
                name: "Mur".into(),
                mesh: MeshKind::Cube,
                transform: Transform::from_pos(Vec3::new(0.0, 1.0, -3.0))
                    .with_scale(Vec3::new(4.0, 2.0, 0.4)),
                physics: PhysicsKind::Static,
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

    #[test]
    fn a_fireball_flies_forward_and_defeats_the_monster_ahead() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.input_state.fire = true;

        // 6 m à 12 m/s ≈ 0,5 s de vol : 2 s de simulation suffisent largement.
        advance(&mut app, 40, 0.05);

        assert!(
            !app.scene.objects[1].visible,
            "le monstre droit devant doit être vaincu par la boule de feu"
        );
        assert_eq!(app.score(), 1, "un monstre vaincu = +1 au score");
    }

    #[test]
    fn a_wall_shields_the_monster_behind_it() {
        let mut app = app_with(scene_with_monster_ahead(true));
        app.input_state.fire = true;

        advance(&mut app, 40, 0.05);

        assert!(
            app.scene.objects[1].visible,
            "la boule de feu doit s'éteindre sur le mur, jamais atteindre le monstre abrité"
        );
        assert_eq!(app.score(), 0);
    }

    #[test]
    fn the_touch_fire_button_fires_like_the_keyboard() {
        let mut app = app_with(scene_with_monster_ahead(false));
        // Bouton tactile nommé (cf. `Controller::fire_button`), pas le clavier :
        // le chemin APK/aperçu mobile doit tirer exactement comme la touche K.
        app.input_state.buttons.insert("Feu".to_string());

        advance(&mut app, 40, 0.05);

        assert!(
            !app.scene.objects[1].visible,
            "le bouton tactile « Feu » doit tirer comme la touche clavier"
        );
    }

    #[test]
    fn holding_fire_respects_the_cooldown() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.input_state.fire = true;

        // 10 frames de 20 ms = 0,2 s, bien sous la recharge (0,9 s) : une seule
        // boule doit être partie malgré le bouton maintenu.
        advance(&mut app, 10, 0.02);

        assert_eq!(
            app.fireballs.len(),
            1,
            "maintenir le bouton ne doit tirer qu'une boule par temps de recharge"
        );
    }

    #[test]
    fn a_network_players_fire_input_spawns_a_server_side_fireball() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        let index = app
            .spawn_network_player(1)
            .expect("la scène de test a un gabarit pilotable");
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 0.0,
                move_y: 0.0,
                attack: false,
                jump: false,
                fire: true,
            },
        );

        advance(&mut app, 5, 0.02);

        assert_eq!(
            app.fireballs.len(),
            1,
            "l'input réseau fire=true doit faire tirer l'objet de ce joueur"
        );
        assert_eq!(app.fireballs[0].owner, index);

        // Et le snapshot diffusé doit exposer le projectile aux clients.
        let snap = app.network_snapshot(1);
        assert_eq!(snap.projectiles.len(), 1);
    }

    #[test]
    fn a_spamming_network_client_cannot_outrun_the_server_cooldown() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        app.spawn_network_player(1);
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 0.0,
                move_y: 0.0,
                attack: false,
                jump: false,
                fire: true,
            },
        );

        // 0,3 s de spam, bien sous la recharge : une seule boule en vol.
        advance(&mut app, 15, 0.02);
        assert_eq!(
            app.fireballs.len(),
            1,
            "le serveur doit imposer sa recharge, quel que soit le spam du client"
        );
    }

    #[test]
    fn defeating_a_monster_queues_a_network_event() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.input_state.fire = true;

        advance(&mut app, 40, 0.05);

        let events = app.take_net_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, crate::net::protocol::GameEvent::Defeated { index: 1 })),
            "un monstre vaincu doit produire un évènement Defeated à diffuser : {events:?}"
        );
        assert!(
            app.take_net_events().is_empty(),
            "take_net_events doit drainer la file (pas de re-diffusion infinie)"
        );
    }

    /// Verrouille le contenu multijoueur de la scène embarquée (le jeu réellement
    /// exporté, jouée par le serveur ET les clients — cf. `src/bin/server.rs`) :
    /// un ré-export depuis l'éditeur réécrit `assets/player_scene.json`, et
    /// perdrait silencieusement monstres et bouton « Feu » sans ce garde-fou.
    #[test]
    fn the_embedded_scene_ships_monsters_and_the_fire_button() {
        let scene = Scene::embedded_player();
        assert!(
            scene.mobile.buttons.iter().any(|b| b == "Feu"),
            "l'overlay tactile (APK/aperçu desktop) doit proposer le bouton « Feu »"
        );
        let player = scene
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la scène embarquée a un joueur pilotable");
        assert_eq!(
            player.controller.as_ref().unwrap().fire_button,
            "Feu",
            "le contrôleur du joueur doit relier le bouton « Feu » au tir"
        );
        let monsters = scene
            .objects
            .iter()
            .filter(|o| o.controller.is_none() && o.combat.as_ref().is_some_and(|c| c.attackable))
            .count();
        assert!(
            monsters >= 4,
            "la carte multijoueur doit placer des monstres à abattre à distance \
             (trouvés : {monsters})"
        );
    }

    #[test]
    fn the_visual_pool_follows_flying_fireballs_then_hides() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.input_state.fire = true;

        advance(&mut app, 2, 0.02);
        assert_eq!(app.fireball_pool.len(), 1, "une boule en vol = une sphère");
        let sphere = app.fireball_pool[0];
        assert!(app.scene.objects[sphere].visible);

        // Une fois la boule éteinte (impact ou fin de vie), la sphère se masque
        // mais reste en place (indices stables).
        app.input_state.fire = false;
        advance(&mut app, 60, 0.05);
        assert!(!app.scene.objects[sphere].visible);
        assert_eq!(app.fireball_pool.len(), 1);
    }
}
