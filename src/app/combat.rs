//! Système de combat : attaque du joueur (préparation → missile homing → impact),
//! recul (knockback), mise à mort par ring-out, et système de manches (`Combat::wave`).
//!
//! Extrait de `app/mod.rs` pour isoler la surface de gameplay qu'un futur serveur de
//! jeu réseau (cf. SPRINT_MMORPG.md) devra piloter en autorité — sans toucher au reste
//! de la boucle (scripts, physique, caméra), qui reste dans `app/mod.rs`.

use glam::Vec3;

use super::AppState;
use crate::app::multiplayer::RoundObjective;
use crate::scene::AttackMode;

/// Durée (s) à survivre en mode `RoundObjective::Survie` (GDD §4, Sprint 6 de
/// `sprint10audit.md`) avant victoire — assez long pour sentir la pression
/// d'un supplément de vagues face à Vagues (mode fini), assez court pour
/// rester jouable en solo/duo sans contenu de scène dédié à ce mode.
const SURVIE_DURATION_SECS: f32 = 180.0;

/// Distance (m) sous laquelle le convoi (`RoundObjective::Escorte`, Sprint 7 de
/// `sprint10audit.md`) est considéré arrivé à destination — non nulle : une
/// trajectoire en ligne droite à vitesse constante ne tombe pas exactement sur le
/// point cible au pas fixe près (cf. `AppState::update_escorte`).
const CONVOY_ARRIVAL_DISTANCE: f32 = 1.0;

/// Missile en vol vers une cible verrouillée au tir (cf. `AppState::attack_projectile`).
/// Homing : vise la position **courante** de la cible chaque frame (une cible qui bouge
/// pendant le vol n'est pas esquivée), à vitesse constante `SPEED`.
pub(super) struct AttackProjectile {
    /// Indice de la cible dans `scene.objects`, verrouillé au tir.
    pub(super) target: usize,
    /// Position courante du missile (mise à jour chaque frame vers la cible).
    pub(super) pos: Vec3,
}

/// Vitesse du missile (m/s). Volontairement pas instantanée : le temps de vol laisse la
/// cible continuer d'approcher, donc mordre avant que l'impact ne soit résolu — une
/// garantie de risque qui reste partielle (cf. docs/audits/app-misc.md).
const ATTACK_PROJECTILE_SPEED: f32 = 10.0;

/// Vitesse horizontale (m/s) du recul (knockback) infligé à une cible touchée qui
/// survit au coup (cf. `Combat::hp`, `AppState::stagger`) — assez pour repousser un
/// adversaire vers le bord d'une arène façon Smash/Tekken (`Scene::brawl_demo`), pas
/// juste un tressaillement cosmétique.
const KNOCKBACK_SPEED: f32 = 9.0;

/// Durée (s) pendant laquelle le recul prime sur le pilotage IA (cf. `AppState::stagger`) :
/// sans cette fenêtre, le chasseur recalculerait sa vitesse de poursuite dès la frame
/// suivante et écraserait le recul avant qu'il n'ait le moindre effet visible.
const KNOCKBACK_DURATION: f32 = 0.35;

/// Préparation d'attaque en cours (cf. `Controller::attack_windup`) : verrouillée dès
/// l'appui, résolue une fois `remaining` écoulé — le joueur reste exposé pendant ce
/// temps (aucune protection spéciale : c'est le point).
pub(super) struct AttackCharge {
    /// Cible verrouillée (mode `AttackMode::Single`) ; `None` en mode `Zone` — rien à
    /// verrouiller à l'avance, la frappe touche tout ce qui est à portée au moment de
    /// la résolution, pas une cible unique choisie au moment du tir.
    pub(super) target: Option<usize>,
    /// Portée au moment de l'appui, ré-appliquée à la résolution en mode `Zone` (le
    /// mode `Single` n'en a pas besoin : `target` porte déjà l'information).
    pub(super) range: f32,
    pub(super) mode: AttackMode,
    pub(super) remaining: f32,
}

impl AppState {
    /// Indice de l'ancre visuelle d'attaque (`is_attack_fx`), s'il y en a une dans la scène.
    pub(super) fn attack_fx_index(&self) -> Option<usize> {
        self.scene
            .objects
            .iter()
            .position(|o| o.combat.as_ref().is_some_and(|c| c.is_attack_fx))
    }

    /// Plus haut numéro de manche présent dans la scène (0 = pas de système de manches).
    pub(super) fn max_wave(&self) -> u32 {
        self.scene
            .objects
            .iter()
            .filter_map(|o| o.combat.as_ref())
            .map(|c| c.wave)
            .max()
            .unwrap_or(0)
    }

    /// Nombre de manches consécutives révélées simultanément (Phase G, GDD §5.5/§18.5 :
    /// « la difficulté scale par le *nombre*, pas par les stats ») — dérivé de
    /// `network_player_count`, déjà tenu à jour côté serveur par
    /// `spawn_network_player`/`forget_network_player` (`bin/server.rs::Room`). 1 pour
    /// un salon solo/duo (comportement historique inchangé, une seule manche visible à
    /// la fois) ; davantage à mesure que le salon grossit, pour étaler la menace sur
    /// plusieurs fronts plutôt que de gonfler les PV d'un adversaire unique.
    pub(super) fn wave_window(&self) -> u32 {
        (self.network_player_count() as u32).max(1).div_ceil(2)
    }

    /// Initialise le système de manches (cf. `Combat::wave`) : révèle la manche 1 (et,
    /// selon `wave_window`, les suivantes si le salon compte assez de joueurs), masque
    /// le reste. Sans effet si la scène n'a aucun monstre à manches (`self.wave` reste
    /// à 0) — appelée avant `Physics::build` (à l'entrée en Play et au redémarrage)
    /// pour que les monstres masqués n'aient pas de corps rigide créé inutilement
    /// (cf. le filtre `visible` dans `Physics::build`).
    pub(super) fn init_waves(&mut self) {
        let max = self.max_wave();
        self.wave = if max > 0 { 1 } else { 0 };
        if max == 0 {
            return;
        }
        let last = (self.wave + self.wave_window() - 1).min(max);
        for o in &mut self.scene.objects {
            if let Some(c) = &o.combat
                && c.wave > 0
            {
                o.visible = c.wave >= self.wave && c.wave <= last;
            }
        }
        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::WaveStart);
    }

    /// Fait progresser le système de manches : la fenêtre de manches courante
    /// (`self.wave` à `self.wave + wave_window() - 1`, cf. `wave_window`) vidée — plus
    /// aucun monstre visible qui lui appartient — révèle la fenêtre suivante, jusqu'à
    /// la dernière manche ⇒ victoire. Reconstruit la physique après avoir révélé une
    /// manche (les nouveaux monstres visibles ont besoin d'un corps rigide, absent
    /// tant qu'ils étaient masqués).
    pub(super) fn update_waves(&mut self) {
        if self.wave == 0 {
            return;
        }
        let max = self.max_wave();
        let last = (self.wave + self.wave_window() - 1).min(max);
        let remaining = self
            .scene
            .objects
            .iter()
            .filter(|o| {
                o.visible
                    && o.combat
                        .as_ref()
                        .is_some_and(|c| c.wave >= self.wave && c.wave <= last)
            })
            .count();
        if remaining > 0 {
            return;
        }
        if last >= max {
            if self.win_time.is_none() {
                self.win_time = Some(self.time);
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Win);
            }
            return;
        }
        self.wave = last + 1;
        let next_last = (self.wave + self.wave_window() - 1).min(max);
        let next_first = self.wave;
        for o in &mut self.scene.objects {
            if let Some(c) = &o.combat
                && c.wave >= next_first
                && c.wave <= next_last
            {
                o.visible = true;
            }
        }
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::WaveStart);
    }

    /// Point d'entrée générique de la condition de victoire/défaite de manche
    /// (Phase C, `sprint10audit.md`) : branché sur `self.objective` plutôt que
    /// d'appeler `update_waves` en dur — c'est ce qui permet à `Room::restart`
    /// (`bin/server.rs`) de faire tourner des salons sur des modes différents
    /// sans dupliquer la boucle appelante (`advance_play`). Boss (Sprint 8) reste
    /// sur `update_waves` : le GDD le décrit comme « dernière vague : une créature
    /// unique » (§4), donc une scène Boss n'a qu'une manche contenant le boss — la
    /// victoire « dernière manche vidée » d'`update_waves` *est* déjà « boss vaincu »,
    /// sans logique dédiée à dupliquer. Escorte (Sprint 7) a sa propre fonction : ses
    /// conditions (arrivée à destination, convoi détruit) n'ont rien à voir avec des
    /// manches de monstres. `dt` uniquement nécessaire à `update_escorte` (déplacement
    /// du convoi), transmis à tous les bras pour un point d'entrée uniforme.
    pub(super) fn update_round(&mut self, dt: f32) {
        match self.objective {
            RoundObjective::Vagues | RoundObjective::Boss => self.update_waves(),
            RoundObjective::Survie => self.update_survie(),
            RoundObjective::Escorte => self.update_escorte(dt),
        }
    }

    /// Mode Escorte (GDD §4, Sprint 7) : fait avancer le convoi (`SceneObject::convoy`,
    /// premier objet qui en porte un — une seule scène Escorte n'en a qu'un) en ligne
    /// droite vers sa destination, à sa vitesse propre. Victoire dès que le convoi
    /// est assez proche de sa destination ; la défaite (convoi détruit par les
    /// créatures, cf. `Combat::attackable`) est détectée à côté par
    /// `AppState::is_room_lost` (comme pour les autres modes), pas ici — cette
    /// fonction n'a donc rien à faire une fois le convoi invisible (vaincu), sans
    /// quoi elle le ferait « avancer » depuis une position qui n'a plus de sens.
    /// Sans objet `convoy` dans la scène (mauvaise démo/scène chargée en mode
    /// Escorte), ne fait rien plutôt que de paniquer — même tolérance que
    /// `update_waves`/`update_survie` face à une scène sans manches (`self.wave == 0`).
    pub(super) fn update_escorte(&mut self, dt: f32) {
        if self.win_time.is_some() {
            return;
        }
        let Some(idx) = self.scene.objects.iter().position(|o| o.convoy.is_some()) else {
            return;
        };
        if !self.scene.objects[idx].visible {
            return;
        }
        let (destination, speed) = {
            let convoy = self.scene.objects[idx]
                .convoy
                .as_ref()
                .expect("idx pointe sur l'objet convoy trouvé ci-dessus");
            (convoy.destination, convoy.speed)
        };
        let pos = self.scene.objects[idx].transform.position;
        let to_dest = destination - pos;
        let dist = to_dest.length();
        if dist <= CONVOY_ARRIVAL_DISTANCE {
            self.win_time = Some(self.time);
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Win);
            return;
        }
        let step = (speed * dt).min(dist);
        let new_pos = pos + to_dest / dist * step;
        self.scene.objects[idx].transform.position = new_pos;
        if let Some(physics) = self.physics.as_mut() {
            physics.set_position(idx, new_pos);
        }
    }

    /// Mode Survie (GDD §4, Sprint 6) : victoire à `SURVIE_DURATION_SECS`
    /// écoulées (`self.time`, remis à 0 à l'entrée en Play, cf. `advance_play`)
    /// tant qu'au moins un joueur est vivant (la défaite reste détectée à côté,
    /// par `AppState::is_room_lost`, comme pour `Vagues`). Contrairement à
    /// `update_waves`, vider la dernière manche ne gagne pas la partie : elle
    /// boucle sur la manche 1 (monstres re-révélés) pour maintenir la pression
    /// jusqu'au chrono, plutôt que de laisser le salon vide de tout ennemi.
    pub(super) fn update_survie(&mut self) {
        if self.wave == 0 {
            return;
        }
        if self.win_time.is_some() {
            return;
        }
        if self.time >= SURVIE_DURATION_SECS {
            self.win_time = Some(self.time);
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Win);
            return;
        }
        let max = self.max_wave();
        let last = (self.wave + self.wave_window() - 1).min(max);
        let remaining = self
            .scene
            .objects
            .iter()
            .filter(|o| {
                o.visible
                    && o.combat
                        .as_ref()
                        .is_some_and(|c| c.wave >= self.wave && c.wave <= last)
            })
            .count();
        if remaining > 0 {
            return;
        }
        self.wave = if last >= max { 1 } else { last + 1 };
        let next_first = self.wave;
        let next_last = (self.wave + self.wave_window() - 1).min(max);
        for o in &mut self.scene.objects {
            if let Some(c) = &o.combat
                && c.wave >= next_first
                && c.wave <= next_last
            {
                o.visible = true;
            }
        }
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::WaveStart);
    }

    /// Attaque du joueur : bouton tactile nommé (controller.attack_button) ou touche
    /// clavier Attaque. Verrouille la cible `attackable` la plus proche à portée
    /// (mode `Single`) ou constate qu'il y en a au moins une à portée (mode `Zone`,
    /// cf. `Controller::attack_mode`) et lance une **préparation** (cf. `attack_charge`)
    /// avant de résoudre le coup — le joueur reste exposé pendant ce temps, sans
    /// protection spéciale (c'est le point : cf. `Controller::attack_windup`). Recharge
    /// requise : sans elle, maintenir le bouton déclencherait une préparation en rafale
    /// sans le moindre coût (cf. `Controller::attack_cooldown`). Décompte aussi la
    /// résolution de la préparation en cours et le vol homing du missile déjà tiré.
    /// Appelée une fois par frame depuis `advance_play` (dt réel, pas le pas fixe de
    /// simulation : le ressenti du timing d'attaque doit rester lié au framerate réel).
    pub(super) fn update_attack(&mut self, dt: f32) {
        // Temps de recharge de l'attaque : décompte à chaque frame, indépendamment
        // du bouton (sinon le relâcher puis le rappuyer contournerait le temporisateur).
        if self.attack.attack_cooldown_remaining > 0.0 {
            self.attack.attack_cooldown_remaining -= dt;
        }
        if self.attack.attack_projectile.is_none()
            && self.attack.attack_charge.is_none()
            && let Some(player) = self.player_object()
            && let Some(ctrl) = player.controller.clone()
        {
            let pressed = ((!ctrl.attack_button.is_empty()
                && self.input_state.buttons.contains(&ctrl.attack_button))
                || self.input_state.attack)
                && self.attack.attack_cooldown_remaining <= 0.0;
            if pressed {
                let p = player.transform.position;
                let range = ctrl.attack_range;
                self.attack.attack_cooldown_remaining = ctrl.attack_cooldown;
                let target = match ctrl.attack_mode {
                    AttackMode::Single => self.scene.nearest_attackable(p, range).map(Some),
                    AttackMode::Zone => self.scene.nearest_attackable(p, range).map(|_| None),
                };
                if let Some(target) = target {
                    self.attack.attack_charge = Some(AttackCharge {
                        target,
                        range,
                        mode: ctrl.attack_mode,
                        remaining: ctrl.attack_windup,
                    });
                    // Ancre visuelle : petit éclat au niveau du joueur pendant la
                    // préparation (télégraphe le coup à venir), avant même le tir.
                    if let Some(fx) = self.attack_fx_index()
                        && let Some(o) = self.scene.objects.get_mut(fx)
                    {
                        o.transform.position = p;
                        o.transform.scale = Vec3::splat(0.2);
                        o.visible = true;
                    }
                }
            }
        }
        // Préparation en cours : décompte, puis résout le coup une fois écoulée. En
        // mode `Single`, si la cible verrouillée disparaît entre-temps (respawn,
        // autre mise à mort...), la préparation s'annule silencieusement (pas de
        // missile à vide) ; en mode `Zone`, rien n'est verrouillé à l'avance, donc
        // rien à annuler — la frappe touche ce qui est à portée à la résolution,
        // quitte à ne rien toucher du tout.
        if let Some(charge) = &mut self.attack.attack_charge {
            charge.remaining -= dt;
            let cancel = charge
                .target
                .is_some_and(|t| !self.scene.objects.get(t).is_some_and(|o| o.visible));
            if cancel {
                self.attack.attack_charge = None;
                if let Some(fx) = self.attack_fx_index()
                    && let Some(o) = self.scene.objects.get_mut(fx)
                {
                    o.visible = false;
                }
            } else if charge.remaining <= 0.0 {
                let (target, range, mode) = (charge.target, charge.range, charge.mode);
                self.attack.attack_charge = None;
                if let Some(p) = self.player_position() {
                    match mode {
                        AttackMode::Single => {
                            let target = target.expect(
                                "mode Single verrouille toujours une cible avant de lancer une préparation",
                            );
                            self.attack.attack_projectile =
                                Some(AttackProjectile { target, pos: p });
                            if let Some(fx) = self.attack_fx_index()
                                && let Some(o) = self.scene.objects.get_mut(fx)
                            {
                                o.transform.position = p;
                                o.transform.scale = Vec3::splat(0.25);
                            }
                        }
                        AttackMode::Zone => {
                            let defeated = self.scene.attack_zone_at(p, range);
                            if !defeated.is_empty() {
                                self.add_score(defeated.len() as u32);
                                crate::runtime::sfx::play(
                                    &mut self.audio,
                                    crate::runtime::sfx::Sfx::Defeat,
                                );
                                self.fx.attack_flash = 1.0;
                                if let Some(fx) = self.attack_fx_index()
                                    && let Some(o) = self.scene.objects.get_mut(fx)
                                {
                                    o.transform.position = p;
                                    o.transform.scale = Vec3::splat(1.2);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Mise à jour du missile en vol : homing (vise la position courante de la
        // cible), avance à vitesse constante. À l'arrivée (ou si la cible a disparu
        // entre-temps — respawn, autre mise à mort...), résout l'impact.
        if let Some(proj) = self.attack.attack_projectile.take() {
            let alive = self
                .scene
                .objects
                .get(proj.target)
                .is_some_and(|o| o.visible);
            if alive {
                let target_pos = self.scene.objects[proj.target].transform.position;
                let to_target = target_pos - proj.pos;
                let step = ATTACK_PROJECTILE_SPEED * dt;
                if to_target.length() <= step.max(0.15) {
                    // Impact : résout le coup maintenant, pas au moment du tir. Une
                    // cible à plusieurs points de vie (cf. `Combat::hp`, le duel
                    // `Scene::brawl_demo`) peut survivre au coup — `damage_attackable`
                    // ne la masque que si ce coup l'achève.
                    let i = proj.target;
                    let defeated = self.scene.damage_attackable(i);
                    self.fx.attack_flash = 1.0;
                    if defeated {
                        self.add_score(1);
                        crate::runtime::sfx::play(
                            &mut self.audio,
                            crate::runtime::sfx::Sfx::Defeat,
                        );
                        let d = self.scene.objects[i].respawn_delay;
                        if d > 0.0 {
                            self.respawn_queue.push((i, self.time + d));
                        }
                    } else {
                        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
                        // Recul (knockback) : la cible survivante est repoussée loin
                        // du joueur — cf. `AppState::stagger`, qui empêche l'IA de
                        // reprendre la main sur sa vitesse tant que le recul dure.
                        if let Some(p) = self.player_position() {
                            let away = target_pos - p;
                            let dir = Vec3::new(away.x, 0.0, away.z);
                            if dir.length_squared() > 1e-6 {
                                self.attack.stagger.push((
                                    i,
                                    dir.normalize() * KNOCKBACK_SPEED,
                                    KNOCKBACK_DURATION,
                                ));
                            }
                        }
                    }
                    if let Some(fx) = self.attack_fx_index()
                        && let Some(o) = self.scene.objects.get_mut(fx)
                    {
                        o.transform.position = target_pos;
                        o.transform.scale = Vec3::splat(1.2);
                    }
                    // Le missile a atteint sa cible : rien à remettre dans `attack_projectile`.
                } else {
                    let new_pos = proj.pos + to_target.normalize() * step;
                    if let Some(fx) = self.attack_fx_index()
                        && let Some(o) = self.scene.objects.get_mut(fx)
                    {
                        o.transform.position = new_pos;
                    }
                    self.attack.attack_projectile = Some(AttackProjectile {
                        target: proj.target,
                        pos: new_pos,
                    });
                }
            }
            // Cible disparue en vol (respawn, autre mise à mort...) : le missile
            // s'évanouit silencieusement, `attack_projectile` reste `None`.
        }
    }

    /// Durée (s) de la roulade — glissement progressif, pas un bond instantané
    /// (14 septembre 2026, correction « saut périlleux sur place » : la version
    /// initiale téléportait la position d'un coup puis jouait la culbute figée
    /// sur ce point d'arrivée, ce qui se lisait comme un flip acrobatique plutôt
    /// qu'un glissage au sol). Correspond exactement à la durée du clip `Dash`
    /// dans `creature_ronde.glb`.
    pub(super) const ROLL_ANIM_SECONDS: f32 = 0.5;

    /// Ruée du joueur local (14 septembre 2026, capacité 4 du kit 1-2-3-4,
    /// `Scene::ability_bar`) : glissement de `multiplayer::DASH_DISTANCE` étalé
    /// sur `ROLL_ANIM_SECONDS` (pas un bond instantané, cf. la doc de la
    /// constante) dans la direction actuellement regardée par le joueur,
    /// temporisé par `multiplayer::DASH_COOLDOWN` comme l'attaque (décompté
    /// chaque frame, pas seulement au relâchement de la touche). Bornée par un
    /// rayon physique (même masque que la collision caméra) pour ne jamais
    /// traverser un mur — un obstacle plus proche que `DASH_DISTANCE`
    /// raccourcit le glissement au lieu de l'annuler.
    ///
    /// Deux phases distinguées par `self.attack.roll_anim_remaining` :
    /// 1. Glissement en cours (`> 0`) : avance `roll_velocity * dt` ce tick,
    ///    sans réévaluer l'appui — un nouvel appui pendant la roulade est
    ///    ignoré jusqu'à ce qu'elle se termine (attendu : on ne redirige pas
    ///    une roulade en plein vol).
    /// 2. Repos (`== 0`) : lit `input_state.dash`/le temporisateur comme
    ///    avant, et **arme** un nouveau glissement (vitesse + durée) au lieu
    ///    de téléporter la position.
    pub(super) fn update_dash(&mut self, dt: f32) {
        if self.attack.roll_anim_remaining > 0.0 {
            // `dt.min(remaining)` : le dernier tick du glissement peut être plus
            // court que `dt` (le minuteur s'épuise en cours de pas) — sans ce
            // plafond, ce tick avancerait de plus que la distance restante.
            let step = self.attack.roll_velocity * dt.min(self.attack.roll_anim_remaining);
            self.attack.roll_anim_remaining = (self.attack.roll_anim_remaining - dt).max(0.0);
            if let Some(index) = self.player_index()
                && let Some(o) = self.scene.objects.get_mut(index)
            {
                let new_pos = o.transform.position + step;
                o.transform.position = new_pos;
                if let Some(phys) = self.physics.as_mut() {
                    phys.set_position(index, new_pos);
                }
            }
            return;
        }
        if self.attack.dash_cooldown_remaining > 0.0 {
            self.attack.dash_cooldown_remaining -= dt;
        }
        if !self.input_state.dash || self.attack.dash_cooldown_remaining > 0.0 {
            return;
        }
        let Some(index) = self.player_index() else {
            return;
        };
        let Some(o) = self.scene.objects.get(index) else {
            return;
        };
        let pos = o.transform.position;
        let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
        let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        self.attack.dash_cooldown_remaining = super::multiplayer::DASH_COOLDOWN;
        let mut distance = super::multiplayer::DASH_DISTANCE;
        // Même masque que la collision caméra (`CAMERA_COLLISION_MASK`,
        // `app::simulation`) : le décor/les murs, pas les capteurs ni les
        // autres joueurs (une ruée qui buterait sur un adversaire serait
        // plus frustrante qu'utile).
        const DASH_RAYCAST_MASK: u32 = 1;
        // Décalage de l'origine du rayon (14 septembre 2026 — correctif « la
        // ruée ne déplace jamais le joueur ») : sans lui, le rayon part
        // **depuis l'intérieur** du collider capsule du joueur lui-même (sa
        // propre couche par défaut inclut le bit 0, comme `CAMERA_COLLISION_
        // MASK`) — `cast_ray(..., solid: true)` renvoie alors un impact
        // immédiat à distance ≈ 0 sur SON PROPRE corps, et la ruée se
        // résolvait en un bond de quelques millimètres, indiscernable d'un
        // no-op. Même idiome que `update_camera_collision` (`SKIP`).
        const DASH_RAYCAST_SKIP: f32 = 0.4;
        let ray_origin = pos + forward * DASH_RAYCAST_SKIP;
        if let Some(phys) = self.physics.as_ref()
            && let Some(hit) = phys.raycast(
                ray_origin,
                forward,
                (distance - DASH_RAYCAST_SKIP).max(0.0),
                DASH_RAYCAST_MASK,
            )
        {
            distance = DASH_RAYCAST_SKIP + hit.distance;
        }
        // Vitesse constante qui parcourt `distance` en `ROLL_ANIM_SECONDS` :
        // consommée tick par tick par la branche « glissement en cours »
        // ci-dessus, en phase avec la culbute du clip `Dash`.
        self.attack.roll_velocity = forward * (distance / Self::ROLL_ANIM_SECONDS);
        self.attack.roll_anim_remaining = Self::ROLL_ANIM_SECONDS;
        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Jump);
    }

    /// Mise à mort par « ring out » (arène façon Smash/Tekken, cf. `Scene::brawl_demo`) :
    /// un adversaire (IA poursuivante) qui tombe dans une zone mortelle (le vide sous
    /// l'arène) est vaincu, comme un coup réussi — réutilise `deadly_at` (déjà utilisé
    /// pour la défaite du joueur dans `advance_play`), pas un mécanisme séparé. Sans
    /// effet sur les autres démos : aucune n'a de zone mortelle à proximité de ses
    /// monstres poursuivants (arènes fermées par des murs).
    pub(super) fn check_ring_outs(&mut self) {
        for i in 0..self.scene.objects.len() {
            let o = &self.scene.objects[i];
            if !o.visible || o.ai_chaser.is_none() {
                continue;
            }
            if !o.combat.as_ref().is_some_and(|c| c.attackable) {
                continue;
            }
            if self.scene.deadly_at(o.transform.position) {
                self.scene.objects[i].visible = false;
                self.add_score(1);
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Defeat);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::multiplayer::PlayerClass;
    use crate::scene::{Combat, Controller, MeshKind, Scene, SceneObject, Transform};

    /// Gabarit pilotable (requis par `AppState::spawn_network_player`) + 2 manches de
    /// 2 monstres chacune, pour observer `wave_window` sans dépendre d'une démo réelle.
    fn scene_with_two_waves() -> Scene {
        let mut joueur = SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Cube,
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let monster = |wave: u32, name: &str| {
            let mut m = SceneObject {
                name: name.into(),
                mesh: MeshKind::Sphere,
                combat: Some(Combat {
                    attackable: true,
                    wave,
                    ..Default::default()
                }),
                ..Default::default()
            };
            m.color = [1.0; 3];
            m
        };
        Scene {
            objects: vec![
                joueur,
                monster(1, "Monstre 1a"),
                monster(1, "Monstre 1b"),
                monster(2, "Monstre 2a"),
                monster(2, "Monstre 2b"),
            ],
            ..Default::default()
        }
    }

    /// Phase G, Sprint 1 (GDD §5.5/§18.5) : « la difficulté scale par le nombre, pas
    /// par les stats » — un salon à 4 joueurs doit révéler plus d'ennemis simultanément
    /// qu'un salon à 1 joueur, à scène de manches identique.
    #[test]
    fn wave_window_scales_with_active_player_count() {
        let mut solo = AppState::new();
        solo.scene = scene_with_two_waves();
        solo.spawn_network_player(1, PlayerClass::Assault)
            .expect("gabarit pilotable présent");
        solo.init_waves();
        let solo_visible = solo
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && o.combat.is_some())
            .count();
        assert_eq!(
            solo_visible, 2,
            "salon à 1 joueur : seule la manche 1 (2 monstres) est révélée"
        );

        let mut squad = AppState::new();
        squad.scene = scene_with_two_waves();
        for id in 1..=4 {
            squad
                .spawn_network_player(id, PlayerClass::Assault)
                .expect("gabarit pilotable présent");
        }
        squad.init_waves();
        let squad_visible = squad
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && o.combat.is_some())
            .count();
        assert_eq!(
            squad_visible, 4,
            "salon à 4 joueurs : les 2 manches (4 monstres) sont révélées d'un coup"
        );

        assert!(
            squad_visible > solo_visible,
            "la difficulté doit scaler par le nombre de joueurs actifs"
        );
    }

    /// Preuve bout en bout de ce que la touche 1 (mêlée du kit 1-2-3-4, cf.
    /// `lib.rs::recompute_action_buttons`) doit produire : `input_state.attack`
    /// tenu doit vaincre un monstre à portée, comme la touche J historique.
    #[test]
    fn holding_attack_defeats_a_target_in_range() {
        let mut joueur = SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Cube,
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut monstre = SceneObject {
            name: "Monstre".into(),
            mesh: MeshKind::Sphere,
            transform: Transform::from_pos(Vec3::new(0.0, 0.0, 1.0)),
            combat: Some(Combat {
                attackable: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        monstre.color = [1.0; 3];
        let mut app = AppState::new();
        app.scene = Scene {
            objects: vec![joueur, monstre],
            ability_bar: true,
            ..Default::default()
        };
        app.playing = true;
        app.input_state.attack = true;

        // Préparation par défaut 0,25 s : 1 s de pas fixes laisse largement
        // le temps de résoudre le coup.
        for _ in 0..60 {
            app.update_attack(1.0 / 60.0);
        }

        assert!(
            !app.scene.objects[1].visible,
            "le monstre à portée doit être vaincu, input_state.attack tenu"
        );
    }

    /// Preuve bout en bout de ce que la touche 4 (ruée du kit 1-2-3-4) doit
    /// produire : `input_state.dash` tenu doit déplacer le joueur d'environ
    /// `multiplayer::DASH_DISTANCE` (aucun obstacle ici, rien à raboter).
    ///
    /// **Un monde physique réel est construit exprès** (14 septembre 2026,
    /// correctif « la ruée ne fait rien ») : un premier jet de ce test sans
    /// `Physics::build` ne testait pas du tout le chemin bogué — sans monde
    /// physique, `update_dash` saute le rayon entièrement et applique la
    /// distance pleine sans jamais la mesurer. Le vrai bug (rayon lancé
    /// depuis l'intérieur du propre collider du joueur, `cast_ray(solid:
    /// true)` le touchant à distance ≈ 0) n'apparaît qu'avec un collider
    /// joueur réel — d'où le `Controller`/`PhysicsKind::Kinematic`/
    /// `ColliderShape::Capsule` ci-dessous, calqués sur le gabarit réel de
    /// `scene::demos::riviere`.
    #[test]
    fn holding_dash_moves_the_player_forward() {
        let mut joueur = SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Capsule,
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            physics: crate::runtime::physics::PhysicsKind::Kinematic,
            collider_shape: crate::runtime::physics::ColliderShape::Capsule,
            ..Default::default()
        };
        joueur.color = [1.0; 3];
        let mut app = AppState::new();
        app.scene = Scene {
            objects: vec![joueur],
            ability_bar: true,
            ..Default::default()
        };
        app.physics = Some(crate::runtime::physics::Physics::build(&app.scene));
        app.playing = true;
        let start = app.scene.objects[0].transform.position;
        app.input_state.dash = true;

        // La ruée est désormais un glissement étalé sur `ROLL_ANIM_SECONDS`
        // (correctif « saut périlleux sur place » du 14 septembre 2026), pas
        // un bond instantané en un seul tick : on avance assez de pas pour
        // couvrir toute la durée du glissement avant de mesurer.
        for _ in 0..40 {
            app.update_dash(1.0 / 60.0);
        }
        let moved = app.scene.objects[0].transform.position.distance(start);
        assert!(
            moved > crate::app::multiplayer::DASH_DISTANCE - 0.5,
            "la ruée devrait déplacer le joueur d'environ {} m, mesuré : {moved:.2} m \
             (rayon lancé depuis son propre collider ?)",
            crate::app::multiplayer::DASH_DISTANCE
        );

        // Second bug du même correctif (14 septembre 2026) : sans
        // `Physics::set_position`, le `KinematicCharacterController` — qui a
        // SA PROPRE notion de la position du corps, indépendante de
        // `transform.position` — ramène le joueur pile où il était dès le
        // prochain pas simulé, annulant le bond aussi sûrement que le bug de
        // rayon ci-dessus (même symptôme observé en jeu : la ruée « ne fait
        // rien », vérifié via `window.__rusteegear_state` sur le site
        // déployé). Aucune entrée de déplacement ici : si la position
        // retombe près de `start`, c'est le contrôleur qui a gagné, pas un
        // mouvement volontaire du joueur.
        app.input_state.dash = false;
        for _ in 0..10 {
            app.sim_step(1.0 / 60.0);
        }
        let still_moved = app.scene.objects[0].transform.position.distance(start);
        assert!(
            still_moved > crate::app::multiplayer::DASH_DISTANCE - 0.5,
            "la position de la ruée doit tenir après plusieurs pas simulés, \
             mesuré : {still_moved:.2} m (le contrôleur cinématique a-t-il \
             ramené le joueur à sa position d'avant, faute de \
             Physics::set_position ?)"
        );
    }
}
