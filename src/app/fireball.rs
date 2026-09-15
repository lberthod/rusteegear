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
use super::multiplayer::PlayerClass;
use crate::net::protocol::{DeathCauseKind, GameEvent, PlayerId};
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
    /// Un **autre joueur réseau** (14 septembre 2026, capacité 3 du kit
    /// 1-2-3-4, PvP) : uniquement dans les scènes `Scene::ability_bar` — cf.
    /// `resolve_pvp_ranged_hit`. Distinct de `Monster` : la vie individualisée
    /// (`network_health`) et le décompte des frags/morts diffèrent d'un
    /// monstre (`DeathCauseKind::Player`, pas de respawn de créature).
    Player(PlayerId),
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
        self.projectiles.fireball_cooldowns.retain(|_, cd| {
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
            if pressed && !self.projectiles.fireball_cooldowns.contains_key(&pi) {
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
            .network
            .network_players
            .iter()
            .filter_map(|(id, &index)| self.network.network_inputs.get(id).map(|inp| (index, inp)))
            .filter(|(index, inp)| {
                inp.fire
                    && !self.projectiles.fireball_cooldowns.contains_key(index)
                    && self.is_alive_at(*index)
            })
            .map(|(index, inp)| (index, inp.aim_yaw, clamp_weapon(inp.weapon)))
            .collect();
        for (index, yaw, weapon) in shooters {
            self.spawn_fireball(index, yaw, weapon);
        }

        // Vol + impacts. `mem::take` pour itérer sans bloquer l'emprunt de `self`
        // (les impacts mutent la scène) ; les survivants sont remis en place.
        let mut flying = std::mem::take(&mut self.projectiles.fireballs);
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
            let step = RANGED_WEAPONS[fb.weapon].speed * dt;
            match self.fireball_impact(&fb, step) {
                Some(Impact::Monster(i)) => {
                    self.resolve_fireball_hit(i, fb.pos, RANGED_WEAPONS[fb.weapon].damage, fb.owner)
                }
                Some(Impact::Player(id)) => {
                    self.resolve_pvp_ranged_hit(id, RANGED_WEAPONS[fb.weapon].damage, fb.owner)
                }
                Some(Impact::Obstacle) => {}
                None => survivors.push(fb),
            }
        }
        self.projectiles.fireballs = survivors;

        // Pool d'affichage : projectiles simulés ici (solo/serveur), ou reçus du
        // dernier `Snapshot` (client connecté — `self.fireballs` y reste vide).
        let shots: Vec<(Vec3, usize)> = if self.is_online_client() {
            self.projectiles.net_projectiles.clone()
        } else {
            self.projectiles
                .fireballs
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
        // Portée de précision de Givre (GDD §8.1, `PlayerClass::
        // ranged_lifetime_mult`) : +25 % de durée de vie, donc de portée
        // effective — appliquée ici, au seul point de départ d'un projectile,
        // jamais côté client. Un tireur non réseau (joueur local solo) n'a
        // pas de classe connue, reste à ×1,0 (même garde que
        // `resolve_fireball_hit` pour `ranged_damage_mult`).
        let lifetime_mult = self
            .network_player_id_at(owner)
            .and_then(|id| self.network_player_class(id))
            .map_or(1.0, |class| class.ranged_lifetime_mult());
        self.projectiles.fireballs.push(Fireball {
            owner,
            pos,
            dir,
            remaining: w.lifetime * lifetime_mult,
            weapon,
        });
        self.projectiles
            .fireball_cooldowns
            .insert(owner, w.cooldown);
    }

    /// Premier objet frappé par le projectile `fb` à sa position courante, s'il y
    /// en a un. Ignorés : le tireur lui-même, les objets masqués, tout objet
    /// pilotable (joueurs — pas de dégâts joueur-contre-joueur tant que la vie
    /// n'est pas individualisée, cf. `network_snapshot`), l'ancre FX d'attaque, et
    /// les objets ni `attackable` ni physiques (fantômes réseau, pool...).
    ///
    /// **Obstacles par rayon physique, pas par AABB** (14 septembre 2026 au
    /// soir, correctif « K/3 ne tire jamais dans la démo Rivière ») : l'ancien
    /// test « le point est dans l'AABB monde d'un objet solide » éteignait le
    /// projectile **dès son apparition** dans toute scène dont le décor est un
    /// seul grand maillage (le terrain « Vallée » couvre toute la carte, donc
    /// tout point de tir), solo comme en ligne — aucun tir n'était jamais
    /// visible. Les obstacles sont désormais détectés par `Physics::raycast`
    /// sur le segment parcouru ce tick (`step`, plus le rayon de l'arme),
    /// contre les colliders réels du décor (`OBSTACLE_MASK`, le même que la
    /// caméra et la ruée) ; les cibles (monstres `attackable`, joueurs PvP)
    /// gardent leur AABB gonflée (généreuse, cohérente avec les dégâts
    /// de zone). Sans physique construite (scène jamais jouée), repli sur
    /// l'ancien test AABB pour ne pas laisser un projectile traverser les murs.
    fn fireball_impact(&self, fb: &Fireball, step: f32) -> Option<Impact> {
        let has_physics = self.physics.is_some();
        for (i, o) in self.scene.objects.iter().enumerate() {
            if i == fb.owner || !o.visible {
                continue;
            }
            if o.controller.is_some() {
                // PvP (14 septembre 2026, capacité 3 du kit 1-2-3-4) :
                // seulement dans les scènes qui l'activent explicitement
                // (`Scene::ability_bar`), et seulement un **autre** joueur
                // réseau vivant, hors grâce d'apparition — sinon ce
                // `controller` reste ignoré comme avant (joueur local solo,
                // gabarit masqué, mondes coopératifs existants inchangés).
                if self.scene.ability_bar
                    && let Some(id) = self.network_player_id_at(i)
                    && self.network.network_health.get(&id).copied().unwrap_or(1.0) > 0.0
                    && !self
                        .network
                        .network_spawn_grace
                        .get(&id)
                        .is_some_and(|g| *g > 0.0)
                {
                    let inflate = Vec3::splat(RANGED_WEAPONS[fb.weapon].radius);
                    let (wmin, wmax) = self.scene.world_aabb(o);
                    if fb.pos.cmpge(wmin - inflate).all() && fb.pos.cmple(wmax + inflate).all() {
                        return Some(Impact::Player(id));
                    }
                }
                continue;
            }
            let attackable = o.combat.as_ref().is_some_and(|c| c.attackable);
            if o.combat.as_ref().is_some_and(|c| c.is_attack_fx) {
                continue;
            }
            let solid = o.physics != PhysicsKind::None;
            if !attackable && (!solid || has_physics) {
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
        // Décor : rayon sur le segment parcouru ce tick, cf. la doc ci-dessus.
        // Origine = position d'avant le vol de ce tick (le projectile naît déjà
        // `SPAWN_AHEAD` devant le tireur, hors de sa capsule).
        if let Some(phys) = self.physics.as_ref() {
            /// Décor fixe/murs, ni capteurs ni joueurs — même masque que
            /// `combat::update_dash` et la collision caméra.
            const OBSTACLE_MASK: u32 = 1;
            let radius = RANGED_WEAPONS[fb.weapon].radius;
            let origin = fb.pos - fb.dir * step;
            if phys
                .raycast(origin, fb.dir, step + radius, OBSTACLE_MASK)
                .is_some()
            {
                return Some(Impact::Obstacle);
            }
        }
        None
    }

    /// Applique `ranged_damage_mult` au dégât PvE d'un tir sur la cible
    /// `target` (GDD §8.1). Pour un multiplicateur ≤ 1,0 (Soutien ×0,70,
    /// Cendre ×0,80, Brasier ×0,60), comportement historique inchangé :
    /// `round().max(1.0)` — le minimum de 1 évite qu'un arrondi vers le bas
    /// ne rende un tir totalement inoffensif contre une cible à 1 PV, et cet
    /// équilibrage (DPS/TTK PvP) a déjà été validé sur ce calcul.
    ///
    /// Pour un multiplicateur > 1,0 (seul Givre aujourd'hui, ×1,50, ajouté le
    /// 15 septembre 2026), `round()` par coup AMPLIFIE le multiplicateur sur
    /// un petit dégât entier : Boule de feu/Éclair (`RANGED_WEAPONS`,
    /// dégât de base 1) donnent `round(1,0 × 1,5) = 2` à CHAQUE coup — un
    /// doublement, pas les +50 % prévus (Boulet, dégât de base 3, donne
    /// `round(4,5) = 5`, soit +67 %, même symptôme). Le calcul de
    /// design (DPS/TTK) a été validé sur `resolve_pvp_ranged_hit`, qui
    /// applique le multiplicateur à une vie **flottante** continue — jamais
    /// vérifié contre le chemin PvE, en dégât **entier**.
    ///
    /// On reporte donc la fraction non appliquée d'un coup sur le suivant,
    /// via `Combat::ranged_dmg_carry` (porté par la cible, comme `max_hp`) :
    /// sur une série de coups, le total appliqué converge vers
    /// `base × mult` plutôt que vers `round(base × mult)` répété. Exemple
    /// (Boule de feu, mult 1,50) : coups 1,2,3,4 → 1,2,1,2 dégâts (moyenne
    /// 1,5, exact) au lieu de 2,2,2,2 (moyenne 2,0, le bug). Cf.
    /// `sniper_pve_dps_matches_one_point_five_not_two_ratio` pour la
    /// non-régression contre le bestiaire réel de `riviere_demo`.
    fn apply_ranged_damage_mult_pve(&mut self, target: usize, damage: u32, class: PlayerClass) -> u32 {
        let mult = class.ranged_damage_mult();
        if mult <= 1.0 {
            return ((damage as f32) * mult).round().max(1.0) as u32;
        }
        let carry = self
            .scene
            .objects
            .get(target)
            .and_then(|o| o.combat.as_ref())
            .map_or(0.0, |c| c.ranged_dmg_carry);
        let total = (damage as f32) * mult + carry;
        let applied = total.floor().max(1.0);
        if let Some(c) = self
            .scene
            .objects
            .get_mut(target)
            .and_then(|o| o.combat.as_mut())
        {
            c.ranged_dmg_carry = total - applied;
        }
        applied as u32
    }

    /// Résout l'impact sur le monstre `i` : dégâts de l'arme, score, frag
    /// individualisé si le tireur est un joueur réseau (`owner`, brique de
    /// progression pour un futur MMORPG), son, flash, respawn, et évènement
    /// réseau `Defeated` si le coup l'achève (diffusé par le serveur headless,
    /// cf. `take_net_events` — les clients y réagissent une fois, son + flash,
    /// sans attendre le prochain `Snapshot`).
    fn resolve_fireball_hit(&mut self, i: usize, at: Vec3, damage: u32, owner: usize) {
        // Dégâts infligés modulés par la classe (GDD §8.1) : appliqué ici,
        // au point de résolution unique des tirs, jamais côté client — un
        // tireur non réseau (joueur local solo) n'a pas de classe connue,
        // `ranged_damage_mult` ne s'applique donc qu'aux joueurs réseau.
        // Cf. `apply_ranged_damage_mult_pve` pour pourquoi un multiplicateur
        // > 1,0 (Givre) ne peut pas réutiliser le simple `round().max(1.0)`
        // historique (Soutien/Cendre/Brasier, tous ≤ 1,0) sans amplifier le
        // dégât au-delà du ratio de design prévu.
        let shooter_id = self.network_player_id_at(owner);
        let damage = shooter_id
            .and_then(|id| self.network_player_class(id))
            .map_or(damage, |class| {
                self.apply_ranged_damage_mult_pve(i, damage, class)
            });
        // Contribution de dégâts (GDD §8.3, assists) : enregistrée avant de
        // savoir si ce coup achève la cible — un tir qui blesse sans tuer est
        // justement le cas qui doit ouvrir droit à un assist si un autre
        // joueur achève juste après (cf. `credit_assists_on_kill`).
        if let Some(id) = shooter_id {
            self.record_damage_contribution(i, id);
        }
        let defeated = self.scene.damage_attackable_by(i, damage);
        self.fx.attack_flash = 1.0;
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
            self.net_conn
                .pending_net_events
                .push(GameEvent::Defeated { index: i as u32 });
            let d = self.scene.objects[i].respawn_delay;
            if d > 0.0 {
                self.respawn_queue.push((i, self.time + d));
            }
        } else {
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
        }
    }

    /// Résout un tir PvP (14 septembre 2026, capacité 3 du kit 1-2-3-4) : dégâts
    /// sur `id` via `apply_network_damage` (bouclier de la cible pris en compte,
    /// cf. `health::BLOCK_DAMAGE_MULT`), frag crédité au tireur si le coup achève
    /// la cible. `damage` reste sur l'échelle PV de monstre (`RangedWeapon::
    /// damage`) : converti en fraction de `health::MAX_HEALTH` via
    /// `PVP_RANGED_DAMAGE_PER_HP` pour rester sur la même échelle que
    /// `network_health`, sans dupliquer une table par arme.
    ///
    /// `ranged_damage_mult` du tireur appliqué ici (15 septembre 2026, GDD
    /// §8.1, ajout de Givre) — jusqu'ici seul `resolve_fireball_hit` (PvE)
    /// l'appliquait : un bonus/malus de dégâts à distance restait invisible
    /// en PvP, asymétrie comparable à `melee_damage_mult`/`attack_cooldown_mult`
    /// dans `update_network_attacks`, dont ce correctif reprend exactement le
    /// pattern (multiplicateur appliqué directement au flottant, pas de
    /// passage par un `u32` arrondi côté PvP — pas d'arrondi parasite à
    /// introduire sur cette échelle déjà fractionnaire).
    fn resolve_pvp_ranged_hit(&mut self, id: PlayerId, damage: u32, owner: usize) {
        const PVP_RANGED_DAMAGE_PER_HP: f32 = 0.06;
        self.fx.attack_flash = 1.0;
        let shooter_id = self.network_player_id_at(owner);
        let ranged_mult = shooter_id
            .and_then(|sid| self.network_player_class(sid))
            .map_or(1.0, |class| class.ranged_damage_mult());
        let died = self.apply_network_damage(
            id,
            damage as f32 * PVP_RANGED_DAMAGE_PER_HP * ranged_mult,
            DeathCauseKind::Player,
            owner,
        );
        if died && let Some(shooter) = shooter_id {
            self.credit_kill(shooter);
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
        while self.projectiles.fireball_pool.len() < shots.len() {
            let index = self.scene.objects.len();
            self.scene.objects.push(crate::scene::SceneObject {
                name: format!("Projectile {}", self.projectiles.fireball_pool.len() + 1),
                mesh: crate::scene::MeshKind::Sphere,
                transform: crate::scene::Transform::from_pos(Vec3::ZERO),
                emissive: 2.0,
                physics: PhysicsKind::None,
                visible: false,
                ..Default::default()
            });
            self.projectiles.fireball_pool.push(index);
        }
        for (slot, &index) in self.projectiles.fireball_pool.iter().enumerate() {
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
        self.projectiles.fireballs.clear();
        self.projectiles.fireball_cooldowns.clear();
        self.projectiles.fireball_pool.clear();
        self.projectiles.net_projectiles.clear();
        self.net_conn.pending_net_events.clear();
    }

    /// Évènements de gameplay en attente de diffusion (monstre vaincu...), drainés
    /// par le serveur headless à chaque tick (`src/bin/server.rs`), qui les
    /// broadcast en `ServerMsg::Event`.
    pub fn take_net_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.net_conn.pending_net_events)
    }

    /// `true` si cette instance est un **client** connecté à un serveur (jamais le
    /// cas du serveur headless, qui n'a pas de `NetClient`) : la simulation locale
    /// des projectiles s'efface alors devant l'autorité du serveur. `pub(super)` :
    /// aussi utilisé par `simulation.rs`/`creature_attack.rs`/`health.rs` pour la
    /// même raison côté créatures scriptées (synchro réseau).
    pub(super) fn is_online_client(&self) -> bool {
        #[cfg(not(target_os = "ios"))]
        {
            self.net_conn.net_client.is_some()
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
            app.perf.last_frame =
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
            block: false,
            dash: false,
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

    /// Régression du 14 septembre 2026 au soir : dans la démo Rivière, le
    /// terrain « Vallée » (un seul grand maillage `Static`) contient tout point
    /// de tir dans son AABB monde — l'ancien test d'obstacle par AABB éteignait
    /// chaque boule de feu à sa naissance, K/3 ne montrait jamais rien.
    #[test]
    fn a_fireball_survives_over_the_riviere_terrain_mesh() {
        let mut app = AppState::new();
        app.load_riviere_demo();
        app.playing = true;
        // Physique construite, joueur posé au sol.
        advance(&mut app, 10, 0.05);
        app.input_state.fire = true;
        let mut ticks_with_projectile = 0;
        for k in 0..20 {
            advance(&mut app, 1, 0.05);
            if !app.network_snapshot(k).projectiles.is_empty() {
                ticks_with_projectile += 1;
            }
        }
        assert!(
            ticks_with_projectile >= 10,
            "la boule de feu doit voler au-dessus du terrain, pas s'éteindre dans son AABB \
             ({ticks_with_projectile} ticks avec projectile sur 20)"
        );
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
            app.projectiles.fireballs.len(),
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
            app.projectiles.fireballs.len(),
            1,
            "l'input réseau fire=true doit faire tirer l'objet de ce joueur"
        );
        assert_eq!(app.projectiles.fireballs[0].owner, index);

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

    /// GDD_MMORPG.md §8.1 (15 septembre 2026) : Givre, seule classe à
    /// dépasser ×1,0 en `ranged_damage_mult` (×1,50) — un Boulet tiré par
    /// Givre doit infliger strictement plus qu'un Boulet tiré par un Assaut
    /// sur la même cible à PV multiples. Même structure que
    /// `support_class_deals_less_ranged_damage_than_assault`.
    #[test]
    fn sniper_class_deals_more_ranged_damage_than_assault() {
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

        let mut sniper_app = app_with(scene_with_monster_ahead(false));
        sniper_app.hide_local_player_template();
        sniper_app.scene.objects[MONSTER].combat.as_mut().unwrap().hp = 10;
        let index = sniper_app
            .spawn_network_player(1, PlayerClass::Sniper)
            .unwrap();
        let shooter_pos = sniper_app.scene.objects[index].transform.position;
        sniper_app.scene.objects[MONSTER].transform.position =
            Vec3::new(shooter_pos.x, shooter_pos.y, shooter_pos.z - 6.0);
        sniper_app.set_network_input(1, boulet);
        advance(&mut sniper_app, 40, 0.05);
        let sniper_hp = sniper_app.scene.objects[MONSTER]
            .combat
            .as_ref()
            .unwrap()
            .hp;

        assert!(
            sniper_hp < assault_hp,
            "un Boulet de Givre doit laisser moins de PV à la cible qu'un Boulet d'Assaut : \
             {sniper_hp} >= {assault_hp}"
        );
    }

    /// Non-régression (15 septembre 2026) : bug d'arrondi corrigé dans
    /// `apply_ranged_damage_mult_pve` — un `round()` par coup sur le dégât
    /// PvE entier amplifiait le ×1,50 de Givre en ×2,0 pour une arme à 1
    /// dégât de base (Boule de feu/Éclair, `RANGED_WEAPONS`), car
    /// `round(1,0 × 1,5)` vaut 2 à CHAQUE coup, pas seulement en moyenne.
    /// Ce test tire la même arme (Boule de feu) le même nombre de fois avec
    /// un Assaut (mult ×1,0) et un Givre (mult ×1,50) sur un monstre à PV
    /// multiples (jamais achevé, `hp` volontairement très élevé) et vérifie
    /// que le ratio de dégât total réel reste proche de ×1,5 — pas ×2,0, le
    /// bug (cf. bestiaire réel de `riviere_demo::monster_roster`, PV 2 à 6 et
    /// boss à 60, tous à mêlée seule : rien n'y punit la fragilité de Givre,
    /// donc ce ratio de DPS doit rester fidèle au ×1,5 validé par le calcul
    /// de design, pas dériver vers ×2,0).
    #[test]
    fn sniper_pve_dps_matches_one_point_five_not_two_ratio() {
        let boule_de_feu: NetworkInput = NetworkInput {
            fire: true,
            weapon: 0, // « Boule de feu », 1 dégât de base (cf. RANGED_WEAPONS)
            ..net_input()
        };
        const HIGH_HP: u32 = 1_000_000;

        let mut assault_app = app_with(scene_with_monster_ahead(false));
        assault_app.hide_local_player_template();
        assault_app.scene.objects[MONSTER]
            .combat
            .as_mut()
            .unwrap()
            .hp = HIGH_HP;
        let index = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let shooter_pos = assault_app.scene.objects[index].transform.position;
        assault_app.scene.objects[MONSTER].transform.position =
            Vec3::new(shooter_pos.x, shooter_pos.y, shooter_pos.z - 6.0);
        assault_app.set_network_input(1, boule_de_feu);
        advance(&mut assault_app, 300, 0.05);
        let assault_damage = HIGH_HP
            - assault_app.scene.objects[MONSTER]
                .combat
                .as_ref()
                .unwrap()
                .hp;

        let mut sniper_app = app_with(scene_with_monster_ahead(false));
        sniper_app.hide_local_player_template();
        sniper_app.scene.objects[MONSTER].combat.as_mut().unwrap().hp = HIGH_HP;
        let index = sniper_app
            .spawn_network_player(1, PlayerClass::Sniper)
            .unwrap();
        let shooter_pos = sniper_app.scene.objects[index].transform.position;
        sniper_app.scene.objects[MONSTER].transform.position =
            Vec3::new(shooter_pos.x, shooter_pos.y, shooter_pos.z - 6.0);
        sniper_app.set_network_input(1, boule_de_feu);
        advance(&mut sniper_app, 300, 0.05);
        let sniper_damage = HIGH_HP
            - sniper_app.scene.objects[MONSTER]
                .combat
                .as_ref()
                .unwrap()
                .hp;

        assert!(
            assault_damage >= 5,
            "l'Assaut doit avoir tiré plusieurs fois pour que le ratio soit mesurable : \
             {assault_damage} dégât(s) total(-aux)"
        );
        let ratio = sniper_damage as f32 / assault_damage as f32;
        assert!(
            (1.4..=1.6).contains(&ratio),
            "le ratio de dégât PvE réel de Givre doit rester proche de ×1,5 (pas ×2,0, le bug \
             d'arrondi corrigé) : {sniper_damage}/{assault_damage} = {ratio}"
        );
    }

    /// Non-régression (15 septembre 2026, ajout de Givre) : `resolve_pvp_ranged_hit`
    /// n'appliquait jusqu'ici aucun `ranged_damage_mult` — un projectile tiré
    /// par Givre contre un autre joueur doit désormais infliger plus de
    /// dégâts qu'un même projectile tiré par un Assaut, exactement comme en
    /// PvE (`sniper_class_deals_more_ranged_damage_than_assault` ci-dessus).
    #[test]
    fn ranged_damage_mult_applies_to_pvp_hits_too() {
        let mut scene = crate::scene::Scene {
            ability_bar: true,
            ..Default::default()
        };
        scene.objects.push(crate::scene::SceneObject {
            name: "Sol".into(),
            mesh: crate::scene::MeshKind::Plane,
            transform: crate::scene::Transform::from_pos(Vec3::ZERO)
                .with_scale(Vec3::new(40.0, 1.0, 40.0)),
            physics: PhysicsKind::Static,
            ..Default::default()
        });
        scene.objects.push(crate::scene::SceneObject {
            name: "Joueur".into(),
            mesh: crate::scene::MeshKind::Capsule,
            transform: Transform::from_pos(Vec3::new(0.0, 1.0, 0.0)),
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });

        let fire_east: NetworkInput = NetworkInput {
            fire: true,
            aim_yaw: -std::f32::consts::FRAC_PI_2,
            ..net_input()
        };

        let mut assault_app = AppState::new();
        assault_app.scene = scene.clone();
        assault_app.playing = true;
        let attacker = assault_app
            .spawn_network_player(1, PlayerClass::Assault)
            .unwrap();
        let target = assault_app
            .spawn_network_player(2, PlayerClass::Assault)
            .unwrap();
        assault_app.scene.objects[target].transform.position =
            assault_app.scene.objects[attacker].transform.position + Vec3::new(6.0, 0.0, 0.0);
        assault_app.network.network_spawn_grace.insert(2, 0.0);
        assault_app.set_network_input(1, fire_east);
        advance(&mut assault_app, 40, 0.05);
        let assault_lost =
            crate::app::health::MAX_HEALTH - assault_app.network_player_health(2).unwrap();

        let mut sniper_app = AppState::new();
        sniper_app.scene = scene;
        sniper_app.playing = true;
        let attacker = sniper_app
            .spawn_network_player(1, PlayerClass::Sniper)
            .unwrap();
        let target = sniper_app
            .spawn_network_player(2, PlayerClass::Assault)
            .unwrap();
        sniper_app.scene.objects[target].transform.position =
            sniper_app.scene.objects[attacker].transform.position + Vec3::new(6.0, 0.0, 0.0);
        sniper_app.network.network_spawn_grace.insert(2, 0.0);
        sniper_app.set_network_input(1, fire_east);
        advance(&mut sniper_app, 40, 0.05);
        let sniper_lost =
            crate::app::health::MAX_HEALTH - sniper_app.network_player_health(2).unwrap();

        assert!(
            sniper_lost > assault_lost,
            "un tir PvP de Givre doit infliger plus de dégâts qu'un tir d'Assaut : \
             {sniper_lost} <= {assault_lost} — ranged_damage_mult doit s'appliquer en PvP"
        );
    }

    /// GDD §8.1 : « portée de précision » de Givre (+25 % de durée de vie,
    /// donc de portée) — un projectile tiré par Givre doit rester en vol plus
    /// de ticks qu'un même projectile tiré par un Assaut, la cible étant
    /// rendue inoffensive (`combat = None`) pour ne mesurer que la durée de
    /// vie naturelle, jamais un impact.
    #[test]
    fn sniper_class_fireballs_fly_further() {
        fn ticks_alive(class: PlayerClass) -> usize {
            let mut app = app_with(scene_with_monster_ahead(false));
            app.hide_local_player_template();
            // Neutralise le monstre : ni `attackable` ni solide, il ne peut
            // plus intercepter le projectile (cf. `fireball_impact`) — seule
            // la durée de vie doit décider de la fin du vol.
            app.scene.objects[MONSTER].combat = None;
            app.spawn_network_player(1, class).unwrap();
            app.set_network_input(
                1,
                NetworkInput {
                    fire: true,
                    ..net_input()
                },
            );
            // Une seule frame de tir : le reste du temps, la recharge (0,9 s)
            // dépasse largement la durée de vie du projectile (≤ 1,875 s),
            // aucun second tir ne part avant que le premier ne s'éteigne.
            advance(&mut app, 1, 0.02);
            app.set_network_input(
                1,
                NetworkInput {
                    fire: false,
                    ..net_input()
                },
            );
            let mut ticks = 0;
            while !app.projectiles.fireballs.is_empty() && ticks < 200 {
                advance(&mut app, 1, 0.05);
                ticks += 1;
            }
            ticks
        }

        let assault_ticks = ticks_alive(PlayerClass::Assault);
        let sniper_ticks = ticks_alive(PlayerClass::Sniper);
        assert!(
            sniper_ticks > assault_ticks,
            "le projectile de Givre doit voler plus longtemps que celui d'un Assaut : \
             {sniper_ticks} <= {assault_ticks}"
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
        app.network.network_health.insert(1, 0.0);
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
            app.projectiles.fireballs.len(),
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
            app.projectiles.fireballs.len(),
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
        assert_eq!(app.projectiles.fireballs.len(), 1);
        assert_eq!(
            app.projectiles.fireballs[0].weapon,
            RANGED_WEAPONS.len() - 1
        );
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
        // Historique : « cibles statiques, jamais d'ai_chaser » — la poursuite
        // permanente de l'époque était perçue comme un bug (cf.
        // docs/audits/app-network.md). Le chantier 4.1 (audit 2026-07-20)
        // réintroduit la chasse mais ENCADRÉE : créatures scriptées qui
        // patrouillent par défaut et ne chassent qu'à courte portée (9 m),
        // plafonnées à 2 par cible — c'est le « vrai danger mobile » que la
        // note d'origine appelait de ses vœux. Ce que ce test continue
        // d'interdire : un chasseur permanent non scripté (corps dynamique
        // sans patrouille), le comportement-bug d'origine.
        assert!(
            monsters.iter().all(|o| o.ai_chaser.is_none()
                || (o.physics == crate::runtime::physics::PhysicsKind::Kinematic
                    && !o.script.trim().is_empty())),
            "tout monstre chasseur de la carte multijoueur doit être une créature \
             scriptée (patrouille par défaut, chasse encadrée) — jamais un \
             chasseur permanent"
        );
    }

    #[test]
    fn the_visual_pool_follows_flying_fireballs_then_hides() {
        let mut app = app_with(scene_with_monster_ahead(false));
        app.input_state.fire = true;

        advance(&mut app, 2, 0.02);
        assert_eq!(
            app.projectiles.fireball_pool.len(),
            1,
            "une boule en vol = une sphère"
        );
        let sphere = app.projectiles.fireball_pool[0];
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
        assert_eq!(app.projectiles.fireball_pool.len(), 1);
    }
}
