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

    /// Initialise le système de manches (cf. `Combat::wave`) : révèle la manche 1,
    /// masque les suivantes. Sans effet si la scène n'a aucun monstre à manches
    /// (`self.wave` reste à 0) — appelée avant `Physics::build` (à l'entrée en Play et
    /// au redémarrage) pour que les monstres masqués n'aient pas de corps rigide créé
    /// inutilement (cf. le filtre `visible` dans `Physics::build`).
    pub(super) fn init_waves(&mut self) {
        let max = self.max_wave();
        self.wave = if max > 0 { 1 } else { 0 };
        if max == 0 {
            return;
        }
        for o in &mut self.scene.objects {
            if let Some(c) = &o.combat
                && c.wave > 0
            {
                o.visible = c.wave == self.wave;
            }
        }
        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::WaveStart);
    }

    /// Fait progresser le système de manches : la manche courante vidée (plus aucun
    /// monstre visible qui lui appartient) révèle la suivante, jusqu'à la dernière ⇒
    /// victoire. Reconstruit la physique après avoir révélé une manche (les nouveaux
    /// monstres visibles ont besoin d'un corps rigide, absent tant qu'ils étaient masqués).
    pub(super) fn update_waves(&mut self) {
        if self.wave == 0 {
            return;
        }
        let wave = self.wave;
        let remaining = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && o.combat.as_ref().is_some_and(|c| c.wave == wave))
            .count();
        if remaining > 0 {
            return;
        }
        let max = self.max_wave();
        if self.wave >= max {
            if self.win_time.is_none() {
                self.win_time = Some(self.time);
                crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Win);
            }
            return;
        }
        self.wave += 1;
        let next = self.wave;
        for o in &mut self.scene.objects {
            if let Some(c) = &o.combat
                && c.wave == next
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
        let wave = self.wave;
        let remaining = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible && o.combat.as_ref().is_some_and(|c| c.wave == wave))
            .count();
        if remaining > 0 {
            return;
        }
        let max = self.max_wave();
        self.wave = if self.wave >= max { 1 } else { self.wave + 1 };
        let next = self.wave;
        for o in &mut self.scene.objects {
            if let Some(c) = &o.combat
                && c.wave == next
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
        if self.attack_cooldown_remaining > 0.0 {
            self.attack_cooldown_remaining -= dt;
        }
        if self.attack_projectile.is_none()
            && self.attack_charge.is_none()
            && let Some(player) = self.player_object()
            && let Some(ctrl) = player.controller.clone()
        {
            let pressed = ((!ctrl.attack_button.is_empty()
                && self.input_state.buttons.contains(&ctrl.attack_button))
                || self.input_state.attack)
                && self.attack_cooldown_remaining <= 0.0;
            if pressed {
                let p = player.transform.position;
                let range = ctrl.attack_range;
                self.attack_cooldown_remaining = ctrl.attack_cooldown;
                let target = match ctrl.attack_mode {
                    AttackMode::Single => self.scene.nearest_attackable(p, range).map(Some),
                    AttackMode::Zone => self.scene.nearest_attackable(p, range).map(|_| None),
                };
                if let Some(target) = target {
                    self.attack_charge = Some(AttackCharge {
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
        if let Some(charge) = &mut self.attack_charge {
            charge.remaining -= dt;
            let cancel = charge
                .target
                .is_some_and(|t| !self.scene.objects.get(t).is_some_and(|o| o.visible));
            if cancel {
                self.attack_charge = None;
                if let Some(fx) = self.attack_fx_index()
                    && let Some(o) = self.scene.objects.get_mut(fx)
                {
                    o.visible = false;
                }
            } else if charge.remaining <= 0.0 {
                let (target, range, mode) = (charge.target, charge.range, charge.mode);
                self.attack_charge = None;
                if let Some(p) = self.player_position() {
                    match mode {
                        AttackMode::Single => {
                            let target = target.expect(
                                "mode Single verrouille toujours une cible avant de lancer une préparation",
                            );
                            self.attack_projectile = Some(AttackProjectile { target, pos: p });
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
                                self.attack_flash = 1.0;
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
        if let Some(proj) = self.attack_projectile.take() {
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
                    self.attack_flash = 1.0;
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
                                self.stagger.push((
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
                    self.attack_projectile = Some(AttackProjectile {
                        target: proj.target,
                        pos: new_pos,
                    });
                }
            }
            // Cible disparue en vol (respawn, autre mise à mort...) : le missile
            // s'évanouit silencieusement, `attack_projectile` reste `None`.
        }
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
