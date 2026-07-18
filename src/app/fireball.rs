//! Attaque à distance : un projectile part **devant** le tireur (le long de son
//! orientation, contrairement au missile homing de `combat.rs` qui verrouille une
//! cible), avance en ligne droite et frappe le premier obstacle physique ou
//! monstre `attackable` sur son chemin.
//!
//! **Multi-armes** : trois profils (cf. `RANGED_WEAPONS`) aux
//! compromis distincts — vitesse/recharge/dégâts/portée — sélectionnés au clavier
//! (1/2/3), au bouton tactile « Arme » (cycle, cf. `Controller::weapon_button`)
//! ou par `ClientMsg::Input::weapon` en ligne (borné côté serveur).
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

/// Profil d'arme à distance — l'équivalent projectile des `Weapon` de mêlée
/// (`scene::WEAPONS`) : le choix change le *style* (viser vite ? frapper fort ?
/// loin ?), chaque profil ayant un vrai coût en face de son avantage.
pub struct RangedWeapon {
    pub label: &'static str,
    /// Vitesse de vol (m/s).
    pub speed: f32,
    /// Temps de recharge (s) entre deux tirs — validé côté simulation
    /// (serveur pour les joueurs réseau) : le spam ne tire pas plus vite.
    pub cooldown: f32,
    /// Durée de vie (s) ⇒ portée max ≈ `speed × lifetime`.
    pub lifetime: f32,
    /// Rayon (m) du projectile : les AABB testés sont gonflés d'autant (un
    /// frôlement compte comme un impact — exiger le centre géométrique serait
    /// frustrant à viser), et c'est aussi sa taille affichée.
    pub radius: f32,
    /// Points de vie retirés par impact (cf. `Scene::damage_attackable_by`).
    pub damage: u32,
    /// Couleur du projectile (sphère émissive du pool d'affichage).
    pub color: [f32; 3],
}

/// Les armes à distance du jeu, indexées par `AppState::selected_weapon` /
/// `ClientMsg::Input::weapon`. L'ordre est un contrat réseau : le serveur et les
/// clients doivent partager la même table (même binaire ou même commit).
pub const RANGED_WEAPONS: &[RangedWeapon] = &[
    // Équilibrée : la boule de feu historique.
    RangedWeapon {
        label: "Boule de feu",
        speed: 12.0,
        cooldown: 0.9,
        lifetime: 1.5, // ≈ 18 m
        radius: 0.35,
        damage: 1,
        color: [1.0, 0.45, 0.1],
    },
    // Rapide : cadence et vitesse doublées, mais petite (plus dure à placer)
    // et portée plus courte — l'arme du duel rapproché nerveux.
    RangedWeapon {
        label: "Éclair",
        speed: 20.0,
        cooldown: 0.45,
        lifetime: 0.6, // ≈ 12 m
        radius: 0.22,
        damage: 1,
        color: [0.35, 0.75, 1.0],
    },
    // Lourde : un boulet lent à grosse recharge, mais 3 dégâts (le « chef » à
    // 3 PV tombe d'un coup) et un gros rayon qui pardonne la visée.
    RangedWeapon {
        label: "Boulet",
        speed: 8.0,
        cooldown: 1.8,
        lifetime: 2.0, // ≈ 16 m
        radius: 0.55,
        damage: 3,
        color: [0.45, 0.4, 0.5],
    },
];

/// Projectile en vol (cf. `AppState::fireballs`).
pub(super) struct Fireball {
    /// Indice de l'objet tireur dans `scene.objects` : jamais frappé par son
    /// propre projectile (il naît dans son AABB).
    pub(super) owner: usize,
    pub(super) pos: Vec3,
    /// Direction de vol (horizontale, normalisée), figée au tir.
    pub(super) dir: Vec3,
    /// Durée de vie restante (s) : écoulée sans impact, le projectile s'éteint —
    /// borne la portée sans avoir à tester la distance parcourue.
    pub(super) remaining: f32,
    /// Arme d'origine (indice dans `RANGED_WEAPONS`) : décide vitesse, dégâts,
    /// rayon et aspect pendant toute la vie du projectile — changer d'arme
    /// pendant qu'un tir vole ne modifie pas le tir déjà parti.
    pub(super) weapon: usize,
}

/// Distance (m) devant le tireur à laquelle le projectile apparaît : hors de son
/// propre AABB, pour ne pas exiger de cas particulier au premier pas de vol.
const SPAWN_AHEAD: f32 = 0.8;

/// Hauteur (m) au-dessus du centre du tireur : le projectile part du « buste »,
/// assez haut pour survoler le sol (plan mince à y=0) sur toute sa trajectoire.
const SPAWN_UP: f32 = 0.4;

/// Ce que le projectile a frappé ce pas-ci (cf. `fireball_impact`).
enum Impact {
    /// Un monstre `attackable` : blessé (`Scene::damage_attackable_by`, selon
    /// l'arme), le projectile s'éteint dans tous les cas (pas de perforation).
    Monster(usize),
    /// Un obstacle physique (mur, tour, décor `Static`/`Dynamic`) : le projectile
    /// s'éteint sans effet — c'est ce qui rend un mur utilisable comme abri.
    Obstacle,
}

/// Borne un indice d'arme reçu du réseau (ou d'un futur code de config) à la
/// table réelle — un client modifié qui envoie `weapon: 250` tire avec la
/// dernière arme connue, il ne panique pas le serveur.
pub fn clamp_weapon(weapon: u8) -> usize {
    (weapon as usize).min(RANGED_WEAPONS.len() - 1)
}

impl AppState {
    /// Fait vivre les projectiles pour cette frame : sélection d'arme (bouton
    /// tactile), tirs (joueur local en solo, joueurs réseau côté serveur), vol,
    /// impacts, et pool d'affichage. Appelée une fois par frame depuis
    /// `advance_play`, comme `update_attack` (dt réel).
    pub(super) fn update_fireballs(&mut self, dt: f32) {
        // Recharges : décomptées chaque frame, indépendamment du bouton (sinon
        // relâcher puis rappuyer contournerait le temporisateur) — mêmes raisons
        // que `attack_cooldown_remaining` (cf. `combat.rs`).
        self.fireball_cooldowns.retain(|_, cd| {
            *cd -= dt;
            *cd > 0.0
        });

        // Bouton tactile « Arme » : cycle sur le **front montant** uniquement —
        // l'overlay réécrit `buttons` à chaque frame (état maintenu), sans cette
        // détection le moindre appui ferait défiler toutes les armes en rafale.
        let weapon_down = self.input_state.weapon_cycle
            || self
                .player_object()
                .and_then(|o| o.controller.as_ref())
                .is_some_and(|c| {
                    !c.weapon_button.is_empty()
                        && self.input_state.buttons.contains(&c.weapon_button)
                });
        if weapon_down && !self.weapon_button_was_down {
            self.cycle_weapon();
        }
        self.weapon_button_was_down = weapon_down;

        // Tir du joueur local — en solo uniquement : connecté, le serveur est
        // autoritaire (l'input part via `network_input_msg`, les projectiles
        // reviennent par le `Snapshot`) ; simuler aussi localement ferait vivre
        // deux projectiles pour un seul tir (un vrai + un fantôme).
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
                let (yaw, _, _) = player.transform.rotation.to_euler(glam::EulerRot::YXZ);
                self.spawn_fireball(pi, yaw, self.selected_weapon);
            }
        }

        // Tirs des joueurs réseau (serveur autoritaire) : même recharge, par objet
        // tireur — un client modifié qui envoie `fire: true` à chaque tick ne tire
        // pas plus vite (cf. le même durcissement dans `update_network_attacks`).
        // La direction vient de l'`aim_yaw` reçu (l'orientation que ce joueur voit
        // à son écran), l'arme de son `weapon` (déjà borné par `sanitize`).
        let shooters: Vec<(usize, f32, usize)> = self
            .network_players
            .iter()
            .filter_map(|(id, &index)| self.network_inputs.get(id).map(|inp| (index, inp)))
            .filter(|(index, inp)| {
                inp.fire && !self.fireball_cooldowns.contains_key(index) && self.is_alive_at(*index)
            })
            .map(|(index, inp)| (index, inp.aim_yaw, clamp_weapon(inp.weapon)))
            .collect();
        for (index, yaw, weapon) in shooters {
            self.spawn_fireball(index, yaw, weapon);
        }

        // Vol + impacts. `mem::take` pour itérer sans bloquer l'emprunt de `self`
        // (les impacts mutent la scène) ; les survivants sont remis en place.
        let mut flying = std::mem::take(&mut self.fireballs);
        flying.retain_mut(|fb| {
            fb.remaining -= dt;
            if fb.remaining <= 0.0 {
                return false;
            }
            fb.pos += fb.dir * RANGED_WEAPONS[fb.weapon].speed * dt;
            true
        });
        let mut survivors = Vec::with_capacity(flying.len());
        for fb in flying {
            match self.fireball_impact(&fb) {
                Some(Impact::Monster(i)) => {
                    self.resolve_fireball_hit(i, fb.pos, RANGED_WEAPONS[fb.weapon].damage, fb.owner)
                }
                Some(Impact::Obstacle) => {}
                None => survivors.push(fb),
            }
        }
        self.fireballs = survivors;

        // Pool d'affichage : projectiles simulés ici (solo/serveur), ou reçus du
        // dernier `Snapshot` (client connecté — `self.fireballs` y reste vide).
        let shots: Vec<(Vec3, usize)> = if self.is_online_client() {
            self.net_projectiles.clone()
        } else {
            self.fireballs
                .iter()
                .map(|fb| (fb.pos, fb.weapon))
                .collect()
        };
        self.sync_fireball_pool(&shots);
    }

    /// Fait partir un projectile de l'arme `weapon` devant l'objet `owner`
    /// (orientation `yaw`) et arme sa recharge — celle de **cette** arme : passer
    /// sur une arme rapide n'écourte pas la recharge d'un tir lourd déjà parti
    /// (la recharge est par tireur, pas par arme).
    fn spawn_fireball(&mut self, owner: usize, yaw: f32, weapon: usize) {
        let Some(o) = self.scene.objects.get(owner) else {
            return;
        };
        let w = &RANGED_WEAPONS[weapon];
        // « Devant » = l'avant du personnage : -Z à yaw 0, la même convention que
        // la poussée tank W (cf. `network_move_axes` : vitesse monde
        // `(-sin yaw, 0, -cos yaw)`) — le projectile part là où le joueur regarde.
        let dir = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let pos = o.transform.position + dir * SPAWN_AHEAD + Vec3::Y * SPAWN_UP;
        self.fireballs.push(Fireball {
            owner,
            pos,
            dir,
            remaining: w.lifetime,
            weapon,
        });
        self.fireball_cooldowns.insert(owner, w.cooldown);
    }

    /// Premier objet frappé par le projectile `fb` à sa position courante, s'il y
    /// en a un. Ignorés : le tireur lui-même, les objets masqués, tout objet
    /// pilotable (joueurs — pas de dégâts joueur-contre-joueur tant que la vie
    /// n'est pas individualisée, cf. `network_snapshot`), l'ancre FX d'attaque, et
    /// les objets ni `attackable` ni physiques (fantômes réseau, pool...).
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
            let inflate = Vec3::splat(RANGED_WEAPONS[fb.weapon].radius);
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

    /// Résout l'impact sur le monstre `i` : dégâts de l'arme, score, frag
    /// individualisé si le tireur est un joueur réseau (`owner`, brique de
    /// progression pour un futur MMORPG), son, flash, respawn, et évènement
    /// réseau `Defeated` si le coup l'achève (diffusé par le serveur headless,
    /// cf. `take_net_events` — les clients y réagissent une fois, son + flash,
    /// sans attendre le prochain `Snapshot`).
    fn resolve_fireball_hit(&mut self, i: usize, at: Vec3, damage: u32, owner: usize) {
        // Dégâts infligés −30 % pour le Soutien (GDD §8.1) : appliqué ici,
        // au point de résolution unique des tirs, jamais côté client — un
        // tireur non réseau (joueur local solo) n'a pas de classe connue,
        // `ranged_damage_mult` ne s'applique donc qu'aux joueurs réseau.
        // Le minimum de 1 évite qu'un arrondi vers le bas ne rende un tir
        // Soutien totalement inoffensif contre une cible à 1 PV.
        let shooter_id = self.network_player_id_at(owner);
        let damage = shooter_id
            .and_then(|id| self.network_player_class(id))
            .map_or(damage, |class| {
                ((damage as f32) * class.ranged_damage_mult())
                    .round()
                    .max(1.0) as u32
            });
        // Contribution de dégâts (GDD §8.3, assists) : enregistrée avant de
        // savoir si ce coup achève la cible — un tir qui blesse sans tuer est
        // justement le cas qui doit ouvrir droit à un assist si un autre
        // joueur achève juste après (cf. `credit_assists_on_kill`).
        if let Some(id) = shooter_id {
            self.record_damage_contribution(i, id);
        }
        let defeated = self.scene.damage_attackable_by(i, damage);
        self.attack_flash = 1.0;
        if let Some(fx) = self.attack_fx_index()
            && let Some(o) = self.scene.objects.get_mut(fx)
        {
            o.transform.position = at;
            o.transform.scale = Vec3::splat(1.2);
            o.visible = true;
        }
        if defeated {
            self.add_score(1);
            if let Some(shooter_id) = shooter_id {
                self.credit_assists_on_kill(i, shooter_id);
                self.credit_kill(shooter_id);
            }
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

    /// Sélectionne directement une arme (clavier 1/2/3, cf. `lib.rs`) ; ignore un
    /// indice hors table (touche 4+ future, config corrompue...).
    pub fn select_weapon(&mut self, weapon: usize) {
        if weapon < RANGED_WEAPONS.len() {
            self.selected_weapon = weapon;
        }
    }

    /// Arme suivante (cycle) — bouton tactile « Arme » (cf. `update_fireballs`).
    pub fn cycle_weapon(&mut self) {
        self.selected_weapon = (self.selected_weapon + 1) % RANGED_WEAPONS.len();
    }

    /// Indice de l'arme à distance équipée (cf. `RANGED_WEAPONS`).
    pub fn selected_weapon(&self) -> usize {
        self.selected_weapon
    }

    /// Libellé de l'arme équipée, pour le HUD.
    pub fn selected_weapon_label(&self) -> &'static str {
        RANGED_WEAPONS[self.selected_weapon].label
    }

    /// Informations affichables de toutes les armes à distance (nom, couleur)
    /// — pour un futur inventaire (`editor/mod.rs`), qui n'a pas accès à
    /// `RangedWeapon` ni au module `fireball` (privé) : de simples tuples
    /// plutôt qu'exposer le type interne.
    pub fn ranged_weapon_display_info(&self) -> Vec<(&'static str, [f32; 3])> {
        RANGED_WEAPONS.iter().map(|w| (w.label, w.color)).collect()
    }

    /// Aligne le pool d'affichage (sphères émissives) sur `shots` (position +
    /// arme) : agrandit le pool à la demande, masque les sphères en trop, et
    /// applique couleur/taille de l'arme (une sphère du pool peut servir à un
    /// Éclair une frame et à un Boulet la suivante). Les objets du pool restent
    /// en place une fois créés (les retirer décalerait tous les indices de
    /// `scene.objects` — même contrainte que `despawn_network_player`).
    pub(super) fn sync_fireball_pool(&mut self, shots: &[(Vec3, usize)]) {
        while self.fireball_pool.len() < shots.len() {
            let index = self.scene.objects.len();
            self.scene.objects.push(crate::scene::SceneObject {
                name: format!("Projectile {}", self.fireball_pool.len() + 1),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::ZERO),
                emissive: 2.0,
                physics: PhysicsKind::None,
                visible: false,
                ..Default::default()
            });
            self.fireball_pool.push(index);
        }
        for (slot, &index) in self.fireball_pool.iter().enumerate() {
            if let Some(o) = self.scene.objects.get_mut(index) {
                match shots.get(slot) {
                    Some(&(p, weapon)) => {
                        let w = &RANGED_WEAPONS[weapon.min(RANGED_WEAPONS.len() - 1)];
                        o.transform.position = p;
                        o.transform.scale = Vec3::splat(w.radius * 2.0);
                        o.color = w.color;
                        o.visible = true;
                    }
                    None => o.visible = false,
                }
            }
        }
    }

    /// Oublie tous les projectiles, recharges et le pool d'affichage : à
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
    /// des projectiles s'efface alors devant l'autorité du serveur. `pub(super)` :
    /// aussi utilisé par `simulation.rs`/`creature_attack.rs`/`health.rs` pour la
    /// même raison côté créatures scriptées (synchro réseau).
    pub(super) fn is_online_client(&self) -> bool {
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
    use super::{RANGED_WEAPONS, clamp_weapon};
    use crate::app::multiplayer::{NetworkInput, PlayerClass};
    use crate::runtime::physics::PhysicsKind;
    use crate::scene::{Combat, Controller, MeshKind, Scene, SceneObject, Transform};

    /// Indice du monstre dans `scene_with_monster_ahead` (0 = sol, 1 = joueur).
    const MONSTER: usize = 2;

    /// Arène minimale : un joueur pilotable en (0, 1, 0) orienté vers -Z (yaw 0),
    /// un monstre `attackable` droit devant à -6 m, et un mur optionnel entre les
    /// deux — de quoi vérifier vol, impact, abri et recharge sans charger une démo.
    fn scene_with_monster_ahead(wall_between: bool) -> Scene {
        let mut scene = Scene::default();
        // Sol : sans lui, le joueur (corps dynamique) tombe dans le vide et les
        // tirs suivants partent bien sous les cibles — bug de scène de test
        // trouvé quand `the_standard_weapon_needs_three_hits_on_the_boss` n'a
        // compté qu'un seul impact sur trois attendus.
        scene.objects.push(SceneObject {
            name: "Sol".into(),
            mesh: MeshKind::Plane,
            transform: Transform::from_pos(Vec3::ZERO).with_scale(Vec3::new(40.0, 1.0, 40.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Capsule,
            transform: Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(Controller {
                input: true,
                fire_button: "Feu".into(),
                weapon_button: "Arme".into(),
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

    /// Input réseau neutre, à personnaliser par test.
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
    fn a_fireball_flies_forward_and_defeats_the_monster_ahead() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.input_state.fire = true;

        // 6 m à 12 m/s ≈ 0,5 s de vol : 2 s de simulation suffisent largement.
        advance(&mut app, 40, 0.05);

        assert!(
            !app.scene.objects[MONSTER].visible,
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
            app.scene.objects[MONSTER].visible,
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
            !app.scene.objects[MONSTER].visible,
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
            .spawn_network_player(1, PlayerClass::Assault)
            .expect("la scène de test a un gabarit pilotable");
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                ..net_input()
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

    /// Brique de progression pour un futur MMORPG (GAMEDESIGN_EN_LIGNE.md) : un
    /// monstre vaincu par la boule de feu d'un joueur réseau crédite **ce**
    /// joueur d'un frag, pas un score de salon partagé.
    #[test]
    fn a_network_players_fireball_kill_credits_their_kill_count() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        assert_eq!(app.network_player_kills(1), Some(0));
        // `spawn_network_player` décale le joueur réseau sur un cercle autour du
        // gabarit d'origine (cf. sa doc) : réaligne le monstre sur ce nouveau
        // point de tir plutôt que de recalculer un `aim_yaw`, pour rester au
        // plus près de `scene_with_monster_ahead` (monstre droit devant à -Z).
        let shooter_pos = app.scene.objects[index].transform.position;
        app.scene.objects[MONSTER].transform.position =
            Vec3::new(shooter_pos.x, shooter_pos.y, shooter_pos.z - 6.0);
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                ..net_input()
            },
        );

        advance(&mut app, 40, 0.05);

        assert!(
            !app.scene.objects[MONSTER].visible,
            "le monstre droit devant doit être vaincu par la boule de feu"
        );
        assert_eq!(
            app.network_player_kills(1),
            Some(1),
            "le tireur doit être crédité du frag"
        );
    }

    /// Sprint 4 (PHASE B, `sprint10audit.md`) : deux joueurs réseau tirent sur
    /// la même cible à plusieurs PV — celui qui la blesse sans l'achever doit
    /// recevoir un assist (pas un frag), celui qui l'achève reçoit le frag
    /// (pas d'assist pour son propre kill). Bout en bout via `fire: true`,
    /// pas un appel direct aux briques internes (`credit_assists_on_kill`),
    /// pour couvrir aussi le câblage dans `resolve_fireball_hit`.
    #[test]
    fn two_network_players_who_both_damage_a_creature_split_credit_between_kill_and_assist() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        app.scene.objects[MONSTER].combat.as_mut().unwrap().hp = 2;

        let p1 = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        let p2 = app.spawn_network_player(2, PlayerClass::Assault).unwrap();
        // Réaligne les deux tireurs sur l'origine du gabarit (`spawn_network_player`
        // les décale sur un cercle, cf. sa doc) pour rester au plus près de
        // `scene_with_monster_ahead` (monstre droit devant à -Z).
        app.scene.objects[p1].transform.position = Vec3::new(0.0, 1.0, 0.0);
        app.scene.objects[p2].transform.position = Vec3::new(0.0, 1.0, 0.0);

        // Joueur 1 tire seul : un coup (1 dégât sur 2 PV) doit blesser sans
        // achever. 0,6 s laisse le temps au premier tir d'atteindre la cible
        // (≈0,5 s de vol) sans laisser la recharge (0,9 s) permettre un second tir.
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                ..net_input()
            },
        );
        app.set_network_input(2, net_input());
        advance(&mut app, 12, 0.05);

        assert!(
            app.scene.objects[MONSTER].visible,
            "1 dégât sur 2 PV ne doit pas achever la cible"
        );
        assert_eq!(app.scene.objects[MONSTER].combat.as_ref().unwrap().hp, 1);

        // Joueur 1 arrête de tirer, joueur 2 achève la cible.
        app.set_network_input(1, net_input());
        app.set_network_input(
            2,
            NetworkInput {
                fire: true,
                ..net_input()
            },
        );
        advance(&mut app, 12, 0.05);

        assert!(
            !app.scene.objects[MONSTER].visible,
            "le second tireur doit achever la cible"
        );
        assert_eq!(
            app.network_player_kills(2),
            Some(1),
            "le tireur qui achève la cible reçoit le frag"
        );
        assert_eq!(
            app.network_player_kills(1),
            Some(0),
            "le premier tireur n'a pas achevé la cible, pas de frag"
        );
        assert_eq!(
            app.network_player_assists(1),
            Some(1),
            "le premier tireur a blessé une cible achevée par un autre joueur peu après : assist"
        );
        assert_eq!(
            app.network_player_assists(2),
            Some(0),
            "le tireur qui achève la cible ne se crédite pas d'un assist pour son propre kill"
        );
    }

    /// GDD §8.1 : « dégâts −30 % » pour le Soutien — un Boulet (3 dégâts de
    /// base) tiré par un Soutien doit infliger strictement moins qu'un
    /// Boulet tiré par un Assaut sur la même cible à PV multiples (sans quoi
    /// le monstre à 1 PV des autres tests ne pourrait jamais montrer la
    /// différence : les deux le vaincraient en un coup).
    #[test]
    fn support_class_deals_less_ranged_damage_than_assault() {
        let boulet: NetworkInput = NetworkInput {
            fire: true,
            weapon: 2, // « Boulet », 3 dégâts de base (cf. RANGED_WEAPONS)
            ..net_input()
        };

        let mut assault_app = app_with(scene_with_monster_ahead(false));
        assault_app.hide_local_player_template();
        assault_app.scene.objects[MONSTER]
            .combat
            .as_mut()
            .unwrap()
            .hp = 10;
        let index = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let shooter_pos = assault_app.scene.objects[index].transform.position;
        assault_app.scene.objects[MONSTER].transform.position =
            Vec3::new(shooter_pos.x, shooter_pos.y, shooter_pos.z - 6.0);
        assault_app.set_network_input(1, boulet);
        advance(&mut assault_app, 40, 0.05);
        let assault_hp = assault_app.scene.objects[MONSTER]
            .combat
            .as_ref()
            .unwrap()
            .hp;

        let mut support_app = app_with(scene_with_monster_ahead(false));
        support_app.hide_local_player_template();
        support_app.scene.objects[MONSTER]
            .combat
            .as_mut()
            .unwrap()
            .hp = 10;
        let index = support_app
            .spawn_network_player(1, PlayerClass::Support)
            .unwrap();
        let shooter_pos = support_app.scene.objects[index].transform.position;
        support_app.scene.objects[MONSTER].transform.position =
            Vec3::new(shooter_pos.x, shooter_pos.y, shooter_pos.z - 6.0);
        support_app.set_network_input(1, boulet);
        advance(&mut support_app, 40, 0.05);
        let support_hp = support_app.scene.objects[MONSTER]
            .combat
            .as_ref()
            .unwrap()
            .hp;

        assert!(
            support_hp > assault_hp,
            "un Boulet de Soutien doit laisser plus de PV à la cible qu'un Boulet d'Assaut : \
             {support_hp} <= {assault_hp}"
        );
    }

    /// GAMEDESIGN_EN_LIGNE.md §3.1 : un joueur réseau vaincu (0 PV)
    /// devient spectateur — son `fire: true` ne doit plus rien déclencher, même
    /// si son objet est encore techniquement présent dans `scene.objects`.
    #[test]
    fn a_defeated_network_player_cannot_fire() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.network_health.insert(1, 0.0);
        // Un vrai mort est masqué (cf. `health::update_network_health`) : sans
        // ça, la régénération passive (objet toujours visible, donc considéré
        // « vivant mais blessé ») ramènerait sa vie au-dessus de 0 dès la
        // frame suivante, invalidant le test.
        app.scene.objects[index].visible = false;
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                ..net_input()
            },
        );

        advance(&mut app, 5, 0.02);

        assert_eq!(
            app.fireballs.len(),
            0,
            "un joueur vaincu ne doit plus pouvoir tirer"
        );
    }

    /// La direction du tir réseau vient de l'`aim_yaw` envoyé
    /// par le client — l'orientation que ce joueur **voit à son écran** — pas de
    /// l'orientation serveur de l'objet (le bloc d'orientation de `sim_step`
    /// est réservé au joueur local, qui ne pivote jamais autrement côté serveur).
    #[test]
    fn a_network_fireball_flies_along_the_clients_aim_yaw() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        // Déplace le monstre en +X du joueur réseau : seul un tir orienté par
        // l'aim_yaw (-π/2 ⇒ direction (+1, 0, 0)) peut le toucher.
        let shooter = app.scene.objects[index].transform.position;
        app.scene.objects[MONSTER].transform.position = shooter + Vec3::new(6.0, 0.0, 0.0);
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                aim_yaw: -std::f32::consts::FRAC_PI_2,
                ..net_input()
            },
        );

        advance(&mut app, 40, 0.05);

        assert!(
            !app.scene.objects[MONSTER].visible,
            "le tir doit partir le long de l'aim_yaw du client (vers +X), pas de \
             l'orientation serveur jamais mise à jour"
        );
    }

    /// L'`aim_yaw` reçu oriente aussi l'**objet** du joueur
    /// réseau — c'est ce yaw que `network_snapshot` diffuse aux autres clients ;
    /// sans ça, les fantômes des autres joueurs ne pivotaient jamais.
    #[test]
    fn the_clients_aim_yaw_rotates_its_server_side_object() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        let index = app.spawn_network_player(1, PlayerClass::Assault).unwrap();
        app.set_network_input(
            1,
            NetworkInput {
                aim_yaw: 1.2,
                ..net_input()
            },
        );

        advance(&mut app, 10, 0.02);

        let yaw = app.scene.objects[index]
            .transform
            .rotation
            .to_euler(glam::EulerRot::YXZ)
            .0;
        assert!(
            (yaw - 1.2).abs() < 1e-3,
            "l'objet serveur du joueur réseau doit adopter l'aim_yaw reçu : {yaw}"
        );
        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .unwrap();
        assert!(
            (entity.yaw - 1.2).abs() < 1e-3,
            "le snapshot doit diffuser ce yaw aux autres clients : {}",
            entity.yaw
        );
    }

    #[test]
    fn a_spamming_network_client_cannot_outrun_the_server_cooldown() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                ..net_input()
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
    fn the_heavy_weapon_one_shots_the_three_hp_boss() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.scene.objects[MONSTER].combat.as_mut().unwrap().hp = 3;
        app.select_weapon(2); // Boulet : 3 dégâts
        app.input_state.fire = true;

        advance(&mut app, 40, 0.05);

        assert!(
            !app.scene.objects[MONSTER].visible,
            "le Boulet (3 dégâts) doit achever un monstre à 3 PV en un seul impact"
        );
    }

    #[test]
    fn the_standard_weapon_needs_three_hits_on_the_boss() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.scene.objects[MONSTER].combat.as_mut().unwrap().hp = 3;
        app.input_state.fire = true; // Boule de feu (1 dégât), tir en continu

        // Deux tirs maximum en 1,6 s (recharge 0,9 s) : le monstre doit tenir.
        advance(&mut app, 32, 0.05);
        assert!(
            app.scene.objects[MONSTER].visible,
            "après 2 impacts à 1 dégât, un monstre à 3 PV doit encore tenir debout"
        );
        // Le troisième tir finit le travail.
        advance(&mut app, 40, 0.05);
        assert!(!app.scene.objects[MONSTER].visible);
    }

    /// Un client modifié qui envoie un indice d'arme hors table ne doit ni
    /// paniquer le serveur ni inventer une arme : borné à la dernière connue.
    #[test]
    fn an_out_of_range_network_weapon_is_clamped_not_a_panic() {
        assert_eq!(clamp_weapon(250), RANGED_WEAPONS.len() - 1);
        let mut app = app_with(scene_with_monster_ahead(false));
        app.hide_local_player_template();
        app.spawn_network_player(1, PlayerClass::Assault);
        app.set_network_input(
            1,
            NetworkInput {
                fire: true,
                weapon: 250,
                ..net_input()
            },
        );
        advance(&mut app, 5, 0.02);
        assert_eq!(app.fireballs.len(), 1);
        assert_eq!(app.fireballs[0].weapon, RANGED_WEAPONS.len() - 1);
    }

    #[test]
    fn the_touch_weapon_button_cycles_once_per_press() {
        let mut app = app_with(scene_with_monster_ahead(false));
        assert_eq!(app.selected_weapon(), 0);

        // Bouton maintenu 5 frames : UN seul changement (front montant).
        app.input_state.buttons.insert("Arme".to_string());
        advance(&mut app, 5, 0.02);
        assert_eq!(
            app.selected_weapon(),
            1,
            "maintenir le bouton Arme ne doit cycler qu'une fois"
        );

        // Relâché puis rappuyé : un cran de plus, puis retour au début du cycle.
        app.input_state.buttons.clear();
        advance(&mut app, 2, 0.02);
        app.input_state.buttons.insert("Arme".to_string());
        advance(&mut app, 2, 0.02);
        assert_eq!(app.selected_weapon(), 2);
        app.input_state.buttons.clear();
        advance(&mut app, 2, 0.02);
        app.input_state.buttons.insert("Arme".to_string());
        advance(&mut app, 2, 0.02);
        assert_eq!(
            app.selected_weapon(),
            0,
            "le cycle doit boucler sur la table"
        );
    }

    /// Le bouton manette « Changer d'arme » (Sprint 110, `PlayerInput::
    /// weapon_cycle`) suit la même règle de front montant que le bouton tactile :
    /// maintenu, il ne cycle qu'une fois.
    #[test]
    fn the_gamepad_weapon_button_cycles_once_per_press() {
        let mut app = app_with(scene_with_monster_ahead(false));
        assert_eq!(app.selected_weapon(), 0);

        app.input_state.weapon_cycle = true;
        advance(&mut app, 5, 0.02);
        assert_eq!(
            app.selected_weapon(),
            1,
            "maintenir le bouton manette ne doit cycler qu'une fois"
        );

        app.input_state.weapon_cycle = false;
        advance(&mut app, 2, 0.02);
        app.input_state.weapon_cycle = true;
        advance(&mut app, 2, 0.02);
        assert_eq!(app.selected_weapon(), 2, "relâcher puis rappuyer recycle");
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
                .any(|e| matches!(e, crate::net::protocol::GameEvent::Defeated { index: 2 })),
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
    /// perdrait silencieusement monstres et boutons sans ce garde-fou.
    #[test]
    fn the_embedded_scene_ships_monsters_and_the_fire_button() {
        let scene = Scene::embedded_player();
        for button in ["Feu", "Arme", "Soin"] {
            assert!(
                scene.mobile.buttons.iter().any(|b| b == button),
                "l'overlay tactile (APK/aperçu desktop) doit proposer le bouton « {button} »"
            );
        }
        let player = scene
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .expect("la scène embarquée a un joueur pilotable");
        let ctrl = player.controller.as_ref().unwrap();
        assert_eq!(ctrl.fire_button, "Feu");
        assert_eq!(ctrl.weapon_button, "Arme");
        assert_eq!(ctrl.heal_button, "Soin");
        let monsters: Vec<_> = scene
            .objects
            .iter()
            .filter(|o| o.controller.is_none() && o.combat.as_ref().is_some_and(|c| c.attackable))
            .collect();
        assert!(
            monsters.len() >= 4,
            "la carte multijoueur doit placer des monstres à abattre à distance \
             (trouvés : {})",
            monsters.len()
        );
        // Revenu à des cibles statiques : `ai_chaser` (poursuite active) a été
        // retiré de la carte embarquée — la poursuite active reste perçue
        // comme un bug par un joueur solo qui ne s'attend pas à des monstres
        // mobiles (cf. docs/audits/app-network.md). Des cibles immobiles,
        // abattues à distance, ne peuvent structurellement plus produire ce
        // ressenti — la vie individualisée (§3.1) reste utile dès qu'un vrai
        // danger mobile sera réintroduit.
        assert!(
            monsters.iter().all(|o| o.ai_chaser.is_none()),
            "les monstres de la carte multijoueur doivent rester des cibles \
             statiques, pas des chasseurs mobiles (`ai_chaser`)"
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
        assert_eq!(
            app.scene.objects[sphere].color, RANGED_WEAPONS[0].color,
            "la sphère du pool doit porter la couleur de l'arme d'origine"
        );

        // Une fois la boule éteinte (impact ou fin de vie), la sphère se masque
        // mais reste en place (indices stables).
        app.input_state.fire = false;
        advance(&mut app, 60, 0.05);
        assert!(!app.scene.objects[sphere].visible);
        assert_eq!(app.fireball_pool.len(), 1);
    }
}
