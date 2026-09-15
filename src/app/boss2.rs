//! Second boss de fin de parcours de la démo Rivière & cascade (« Le
//! Roi-Champignon du Sous-bois », 15 septembre 2026) : sous-bois profond en
//! aval, à l'opposé géographique et thématique de « L'Aîné de la Cascade »
//! (`app::boss`, plateau rocheux amont). Trois phases pilotées par le même
//! ratio de PV (`Combat::hp/max_hp`, cf. sa doc) que le premier boss, mais
//! une mécanique délibérément distincte (consigne du design validé) : PAS de
//! jet télégraphié Single/Fan/Nova — une zone de dégâts au sol ancrée sur la
//! position du joueur (« Éclosion de spores »), combinée à l'invocation
//! progressive de trois Champiblobs (menace de mêlée gérée par l'IA
//! générique déjà existante, `AiChaser`/`BiteAttack` — aucun nouveau système
//! d'IA/de spawn à écrire ici).
//!
//! ## Pourquoi un second module, plutôt qu'une extension de `app::boss`
//!
//! Même doctrine que `app::boss` documente déjà pour ne pas généraliser
//! `creature_attack::RANGED_CREATURE_ATTACKS` (cf. sa doc) : deux bosses,
//! deux mécaniques radicalement différentes (jet volant vs zone au sol +
//! invocation), donnent un second champ `AppState::boss2` jumeau plutôt
//! qu'une table/collection générique — le gain DRY d'une abstraction commune
//! serait illusoire pour seulement 2 bosses prévus (pas N), dont la plus
//! grosse partie (dispatch d'attaque, tuning par phase) resterait de toute
//! façon spécifique à chacun.
//!
//! ## Réseau
//!
//! Même limitation assumée que `app::boss` (cf. sa doc, pas une régression
//! introduite ici) : les dégâts de la zone de spores ne visent que
//! `AppState::player_position()` (le joueur local), et la zone marquée au
//! sol elle-même (position, pulse de teinte du télégraphe) n'est ni
//! diffusée ni rejouée sur les clients connectés (`is_online_client`) —
//! seules position/visibilité/PV du boss et de ses Champiblobs (déjà
//! diffusées à tous comme n'importe quel monstre `attackable`, cf.
//! `AppState::network_snapshot`, `net::protocol::EntityDelta`) sont
//! autoritaires et visibles de tous. Révéler un Champiblob ne change donc
//! rien au protocole réseau : sa visibilité suit le même canal générique
//! qu'un monstre ordinaire qui réapparaît (`network_client::poll_network`
//! reconstruit déjà la physique du client dès que cette visibilité change,
//! cf. `net_visibility_dirty`) — pas de bump de `PROTOCOL_VERSION`.
//!
//! ## Champiblobs : pourquoi `update_boss2` reconstruit la physique
//!
//! Contrairement au reste du bestiaire (toujours visible à la construction
//! de la scène, cf. `scene::demos::riviere`), les trois Champiblobs sont
//! posés **masqués** dès le départ (butin de fin de parcours, pas un
//! monstre de plus dans la rotation). `Physics::build` ignore tout chasseur
//! masqué au moment de sa construction (cf. sa doc) : sans reconstruction,
//! un Champiblob révélé en cours de combat resterait sans corps rigide, donc
//! immobile et incapable de mordre. Même mécanisme que `AppState::
//! update_waves`/`update_survie` révélant la manche suivante — reconstruit
//! la physique après coup, hors client réseau connecté (dont l'équivalent
//! est déjà déclenché génériquement par `net_visibility_dirty`).

use glam::Vec3;

use super::creature_attack;
use super::AppState;
use crate::runtime::physics::{Physics, PhysicsKind};

/// Nom de l'objet de scène du boss (`Scene::riviere_demo`) — sert aussi de
/// préfixe unique à son script de morsure (cf. `creature_bite_script`).
pub const BOSS2_NAME: &str = "Le Roi-Champignon du Sous-bois";
/// Nom de l'objet de butin garanti, posé masqué dès la construction de la
/// scène (cf. la doc du module).
pub const BOSS2_LOOT_NAME: &str = "Sceptre du Roi Champignon";
/// Tag partagé des trois Champiblobs pré-placés (cf.
/// `scene::demos::riviere`) — pas un nom individuel par instance : ils sont
/// retrouvés ensemble par tag, dans l'ordre où ils apparaissent dans
/// `Scene::objects` (ordre de construction de la scène, stable d'un
/// chargement à l'autre).
pub const BOSS2_MINION_TAG: &str = "boss2_minion";

/// Décalage de phase du tirage déterministe (cf. `deterministic_roll`) —
/// choisi loin des `salt` de `RANGED_CREATURE_ATTACKS` (17..74) et de
/// `app::boss::BOSS_SALT` (97.13) pour qu'un même `time` ne fasse jamais
/// rouler les trois systèmes en lockstep.
const BOSS2_SALT: f32 = 173.29;
/// Pause de récupération (s) après la « grande éruption » de la phase
/// « Éruption des spores » : le boss reste figé (ne poursuit pas), une
/// fenêtre de riposte volontaire (cf. la doc du design validé) plutôt qu'un
/// DPS-check punitif — même valeur et même intention que `app::boss::
/// RECOVER_SECS`.
const RECOVER_SECS: f32 = 1.2;
/// Épaisseur (m) du disque de télégraphe au sol — juste assez visible sans
/// dépasser du terrain (posé légèrement au-dessus, cf. `sync_hazard_pool`).
const HAZARD_THICKNESS: f32 = 0.06;

/// Les trois phases de combat, dérivées du seul ratio PV (recalculé chaque
/// tick depuis `Combat::hp/max_hp` — `Boss2State::last_phase` ne mémorise la
/// dernière valeur que pour détecter un **franchissement**, même rôle que
/// `app::boss::BossState::last_phase`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Boss2Phase {
    /// Plus de 50 % PV : poursuite standard, zones de spores isolées, aucun
    /// Champiblob encore révélé.
    Germination,
    /// 25-50 % PV : plus rapide, zones plus fréquentes, 2 Champiblobs révélés.
    Proliferation,
    /// < 25 % PV : vitesse maximale, grande éruption suivie d'une pause de
    /// récupération, les 3 Champiblobs révélés.
    Eruption,
}

impl Boss2Phase {
    /// Libellé affiché à côté de la barre de vie (cf. `editor::hud::boss_health_bar`).
    pub fn label(self) -> &'static str {
        match self {
            Boss2Phase::Germination => "Germination",
            Boss2Phase::Proliferation => "Prolifération",
            Boss2Phase::Eruption => "Éruption des spores",
        }
    }
}

/// Phase courante depuis le ratio PV (1.0 = pleine vie) — mêmes seuils que
/// `app::boss::phase_for_ratio` (50 %, 25 %), choix délibéré de cohérence
/// (cf. la doc du design validé) : le joueur retrouve les mêmes repères de
/// PV pour les deux boss.
pub fn phase_for_ratio2(ratio: f32) -> Boss2Phase {
    if ratio > 0.5 {
        Boss2Phase::Germination
    } else if ratio > 0.25 {
        Boss2Phase::Proliferation
    } else {
        Boss2Phase::Eruption
    }
}

/// Réglages d'une phase : vitesse de poursuite (`AiChaser::speed`, appliquée
/// directement — même convention d'authoring que `app::boss::PhaseTuning` et
/// le reste du roster Rivière) et profil de la zone de spores.
struct PhaseTuning2 {
    chase_speed: f32,
    /// Distance boss→joueur en-deçà de laquelle une zone peut être marquée
    /// (même rôle que `app::boss::PhaseTuning::range`) : le Roi-Champignon ne
    /// sème pas de spores à travers toute la vallée.
    range: f32,
    cooldown: f32,
    chance: f32,
    windup: f32,
    radius: f32,
    damage: f32,
    color: [f32; 3],
    /// Nombre cumulé de Champiblobs révélés une fois cette phase atteinte
    /// (0 en Germination, 2 en Prolifération, 3 en Éruption) — cf.
    /// `reveal_boss2_minions`.
    minions_target: usize,
}

fn tuning2(phase: Boss2Phase) -> PhaseTuning2 {
    match phase {
        Boss2Phase::Germination => PhaseTuning2 {
            chase_speed: 1.4,
            range: 10.0,
            cooldown: 6.0,
            chance: 0.5,
            windup: 1.3,
            radius: 1.6,
            damage: 0.14,
            color: [0.55, 0.85, 0.35],
            minions_target: 0,
        },
        Boss2Phase::Proliferation => PhaseTuning2 {
            chase_speed: 1.9,
            range: 10.0,
            cooldown: 4.0,
            chance: 0.6,
            windup: 0.9,
            radius: 1.9,
            damage: 0.12,
            color: [0.85, 0.6, 0.15],
            minions_target: 2,
        },
        Boss2Phase::Eruption => PhaseTuning2 {
            chase_speed: 2.3,
            range: 10.0,
            cooldown: 2.5,
            chance: 0.7,
            windup: 0.6,
            radius: 2.4,
            damage: 0.15,
            color: [0.85, 0.2, 0.55],
            minions_target: 3,
        },
    }
}

/// Fréquence (rad/s de `time`) du pulse de teinte du boss pendant la visée
/// d'une zone de spores (cf. `update_boss2`) — croît avec la phase, même
/// intention que `app::boss::telegraph_pulse_frequency` (l'attaque la plus
/// dangereuse doit avoir le télégraphe le plus alarmant).
fn telegraph_pulse_frequency2(phase: Boss2Phase) -> f32 {
    match phase {
        Boss2Phase::Germination => 7.0,
        Boss2Phase::Proliferation => 11.0,
        Boss2Phase::Eruption => 16.0,
    }
}

/// État de la zone de spores en préparation — même rôle que `app::boss::
/// BossRangedState`, adapté au vocabulaire « zone au sol » plutôt que « jet » :
/// pas de `dir`/vitesse (la zone ne vole pas), un `pending_center` en plus
/// (position du joueur capturée au déclenchement, cf. `update_boss2`).
#[derive(Default)]
struct HazardState {
    cooldown: f32,
    /// Échéance de la visée en cours, OU de la pause de récupération après
    /// la grande éruption (les deux réutilisent ce même champ, cf. la doc de
    /// `AppState::update_boss2`) — `None` : ni l'un ni l'autre.
    stopped_until: Option<f32>,
    /// Position du boss au moment où il s'arrête (visée ou récupération) —
    /// ré-ancrée à chaque tick par `refresh_boss2_frozen_anchor`.
    frozen_pos: Option<Vec3>,
    /// Centre de la zone marquée au sol (position du joueur au moment du
    /// déclenchement, PAS celle du boss) — `Some` seulement pendant la
    /// visée, remis à `None` dès la détonation.
    pending_center: Option<Vec3>,
    recovering: bool,
}

/// État complet du second boss, dans `AppState` (cf. son champ `boss2`).
#[derive(Default)]
pub(super) struct Boss2State {
    hazard: HazardState,
    /// Pool d'un unique objet de scène (disque plat émissif) affichant la
    /// zone de spores en préparation — même principe que `app::boss::
    /// BossState::shot_pool`, réduit à un seul emplacement (une seule zone
    /// à la fois, contrairement au Fan/Nova du premier boss).
    hazard_pool: Vec<usize>,
    /// Dernière phase observée (`None` avant le premier `update_boss2`) —
    /// même rôle que `app::boss::BossState::last_phase` : détecte le
    /// **franchissement** d'un seuil de phase, pour y accrocher la
    /// révélation ponctuelle des Champiblobs (cf. `reveal_boss2_minions`).
    last_phase: Option<Boss2Phase>,
}

impl AppState {
    /// Vrai si l'objet `idx` est le second boss et qu'il est actuellement
    /// figé (visée d'une zone, ou pause de récupération après la grande
    /// éruption) — même rôle que `boss_is_frozen`, consulté par
    /// `sim_step_inner` pour ne pas laisser la chasse IA écraser le gel.
    pub(super) fn boss2_is_frozen(&self, idx: usize) -> bool {
        self.scene.objects.get(idx).is_some_and(|o| o.name == BOSS2_NAME)
            && self.boss2.hazard.stopped_until.is_some()
    }

    /// Ré-ancre la position gelée du second boss sur celle réellement
    /// atteinte ce tick (bousculade) — même rôle que
    /// `refresh_boss_frozen_anchor`, à appeler après `Physics::
    /// resolve_scripted_moves`/`step`.
    pub(super) fn refresh_boss2_frozen_anchor(&mut self) {
        if self.boss2.hazard.stopped_until.is_none() || self.boss2.hazard.frozen_pos.is_none() {
            return;
        }
        if let Some(obj) = self.scene.objects.iter().find(|o| o.name == BOSS2_NAME) {
            self.boss2.hazard.frozen_pos = Some(obj.transform.position);
        }
    }

    /// Fait vivre le second boss pour ce pas fixe : dérive la visibilité du
    /// butin garanti de celle du boss, calcule sa phase et applique sa
    /// vitesse de poursuite et son télégraphe visuel, révèle ses Champiblobs
    /// au franchissement d'un seuil de phase, puis — hors client réseau
    /// connecté (cf. `is_online_client`, même garde que `update_boss`) —
    /// fait vivre sa zone de spores (visée, marquage au sol, détonation).
    pub(super) fn update_boss2(&mut self, dt: f32, time: f32) {
        self.update_boss2_loot();
        let Some(boss_idx) = self.scene.objects.iter().position(|o| o.name == BOSS2_NAME) else {
            return;
        };
        if !self.scene.objects[boss_idx].visible {
            // Vaincu (ou pas encore réapparu) : oublie tout état de visée en
            // cours, vide le pool d'affichage, et re-masque toute couvée
            // révélée pendant le combat précédent — même politique que
            // `clear_boss_shots`, étendue aux Champiblobs (cf. la doc du
            // module).
            self.boss2.hazard = HazardState::default();
            self.sync_hazard_pool(None);
            self.hide_all_boss2_minions();
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
        let phase = phase_for_ratio2(ratio);
        let tune = tuning2(phase);

        // Signal ponctuel au franchissement d'un seuil de phase — même
        // idiome que `app::boss::update_boss` (cf. sa doc), plus la
        // révélation de couvée propre à ce boss. Ignore le premier appel
        // (`last_phase == None`) : ce n'est pas une transition, juste
        // l'observation initiale.
        if let Some(last) = self.boss2.last_phase
            && last != phase
        {
            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::WaveStart);
            self.fx.damage_flash = 1.0;
            self.reveal_boss2_minions(tune.minions_target);
        }
        self.boss2.last_phase = Some(phase);

        if let Some(ai) = self.scene.objects[boss_idx].ai_chaser.as_mut() {
            ai.speed = tune.chase_speed;
        }

        // Télégraphe visuel : pulse de teinte du boss pendant la visée d'une
        // zone (en plus du disque au sol lui-même, cf. `sync_hazard_pool`
        // plus bas) — purement cosmétique, non répliqué aux clients réseau
        // connectés (cf. la doc du module). PAS pendant la pause de
        // récupération (`recovering`) : le boss est alors simplement figé,
        // sans menace imminente à signaler.
        let pulse_freq = telegraph_pulse_frequency2(phase);
        let aiming = self.boss2.hazard.stopped_until.is_some() && !self.boss2.hazard.recovering;
        self.scene.objects[boss_idx].color = if aiming {
            let p = (time * pulse_freq).sin() * 0.5 + 0.5;
            [1.0 - 0.4 * p, 1.0, 1.0 - 0.6 * p]
        } else {
            [1.0, 1.0, 1.0]
        };

        if self.is_online_client() {
            return;
        }
        let Some(player_pos) = self.player_position() else {
            return;
        };

        if let Some(deadline) = self.boss2.hazard.stopped_until {
            // Arrêté (visée ou récupération) : gèle la position (annule le
            // déplacement de chasse de ce tick) et force `Idle`.
            if let Some(frozen) = self.boss2.hazard.frozen_pos {
                self.scene.objects[boss_idx].transform.position = frozen;
            }
            if let Some(anim) = self.scene.objects[boss_idx].animation.as_mut() {
                anim.set_clip("Idle");
            }
            if time >= deadline {
                if self.boss2.hazard.recovering {
                    self.boss2.hazard.recovering = false;
                    self.boss2.hazard.stopped_until = None;
                    self.boss2.hazard.frozen_pos = None;
                } else {
                    // Détonation : dégâts si le joueur est encore dans le
                    // rayon marqué (distance au sol, Y ignoré — la zone
                    // n'est pas un jet qui vise le buste, cf. la doc du
                    // module) — contre-jeu « sortir du disque », pas
                    // « esquiver une trajectoire ».
                    if let Some(center) = self.boss2.hazard.pending_center {
                        let hit = player_pos.with_y(0.0).distance(center.with_y(0.0)) <= tune.radius;
                        if hit && self.play_grace <= 0.0 {
                            self.hud_health = self.hud_health.map(|h| (h - tune.damage).max(0.0));
                            self.fx.damage_flash = 1.0;
                            self.fx.camera_shake = 1.0;
                            crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Hit);
                        }
                    }
                    crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Jump);
                    self.boss2.hazard.pending_center = None;
                    if phase == Boss2Phase::Eruption {
                        // Grande éruption : pause de récupération, cf.
                        // `RECOVER_SECS`.
                        self.boss2.hazard.stopped_until = Some(time + RECOVER_SECS);
                        self.boss2.hazard.recovering = true;
                    } else {
                        self.boss2.hazard.stopped_until = None;
                        self.boss2.hazard.frozen_pos = None;
                    }
                }
            }
        } else {
            self.boss2.hazard.cooldown = (self.boss2.hazard.cooldown - dt).max(0.0);
            if self.boss2.hazard.cooldown <= 0.0 {
                let boss_pos = self.scene.objects[boss_idx].transform.position;
                if player_pos.distance(boss_pos) <= tune.range {
                    // Tentative consommée qu'elle réussisse ou non — même
                    // garde que `update_creature_ranged_attacks`/`update_boss`.
                    self.boss2.hazard.cooldown = tune.cooldown;
                    if creature_attack::deterministic_roll(time, BOSS2_SALT) < tune.chance {
                        self.boss2.hazard.frozen_pos = Some(boss_pos);
                        self.boss2.hazard.pending_center = Some(player_pos);
                        self.boss2.hazard.stopped_until = Some(time + tune.windup);
                    }
                }
            }
        }

        // Disque de télégraphe : visible tant qu'une zone est en
        // préparation (`pending_center`), masqué le reste du temps (pas
        // encore marquée, ou déjà détonée ce tick) — même idiome que
        // `sync_boss_shot_pool`.
        let pending = self
            .boss2
            .hazard
            .pending_center
            .map(|c| (c, tune.radius, tune.color));
        self.sync_hazard_pool(pending);
    }

    /// Aligne le pool d'affichage de la zone de spores (au plus un disque à
    /// la fois) — même principe que `sync_boss_shot_pool`, en plus simple
    /// (pas de déplacement par tick, la zone reste fixe au sol).
    fn sync_hazard_pool(&mut self, pending: Option<(Vec3, f32, [f32; 3])>) {
        if self.boss2.hazard_pool.is_empty() {
            let index = self.scene.objects.len();
            self.scene.objects.push(crate::scene::SceneObject {
                name: "Éclosion de spores".into(),
                mesh: crate::scene::MeshKind::Cylinder,
                transform: crate::scene::Transform::from_pos(Vec3::ZERO),
                emissive: 1.3,
                physics: PhysicsKind::None,
                visible: false,
                ..Default::default()
            });
            self.boss2.hazard_pool.push(index);
        }
        let slot = self.boss2.hazard_pool[0];
        if let Some(o) = self.scene.objects.get_mut(slot) {
            match pending {
                Some((center, radius, color)) => {
                    // Légèrement au-dessus du sol (évite le z-fighting avec
                    // le terrain) — même marge que le butin posé au sol
                    // dans `riviere.rs` (`boss_y + 0.1`).
                    o.transform.position = center + Vec3::Y * 0.05;
                    o.transform.scale = Vec3::new(radius * 2.0, HAZARD_THICKNESS, radius * 2.0);
                    o.color = color;
                    o.visible = true;
                }
                None => o.visible = false,
            }
        }
    }

    /// Oublie la zone en cours, l'état de visée et le pool d'affichage —
    /// mêmes sites d'appel que `clear_boss_shots` (le pool vit dans
    /// `scene.objects`, ses indices deviennent obsolètes après une
    /// restauration en bloc de la scène, ex. « Rejouer »/sortie de Play).
    pub(super) fn clear_boss2_hazards(&mut self) {
        self.boss2.hazard = HazardState::default();
        self.boss2.hazard_pool.clear();
    }

    /// Dérive la visibilité du butin garanti (`BOSS2_LOOT_NAME`) de celle du
    /// boss — même patron que `app::boss::update_boss_loot` (cf. sa doc).
    fn update_boss2_loot(&mut self) {
        let Some(boss_visible) = self
            .scene
            .objects
            .iter()
            .find(|o| o.name == BOSS2_NAME)
            .map(|o| o.visible)
        else {
            return;
        };
        if let Some(loot) = self.scene.objects.iter_mut().find(|o| o.name == BOSS2_LOOT_NAME) {
            let want_visible = !boss_visible;
            if loot.visible != want_visible {
                loot.visible = want_visible;
            }
        }
    }

    /// Révèle les Champiblobs pré-placés jusqu'au `target`-ième inclus, dans
    /// l'ordre où ils apparaissent dans `Scene::objects` (stable depuis la
    /// construction de la scène) — sans effet au-delà du nombre déjà
    /// visible (ne masque jamais un Champiblob déjà engagé, ne fait
    /// qu'ajouter). Reconstruit la physique si au moins un Champiblob vient
    /// d'être révélé (cf. la doc du module) — hors client réseau connecté,
    /// où l'équivalent est déjà déclenché génériquement par
    /// `net_visibility_dirty` dès que le serveur diffuse cette visibilité.
    ///
    /// Restaure aussi `Combat::hp` à `Combat::max_hp` pour chaque Champiblob
    /// **nouvellement** révélé (repli implicite sur la valeur d'autorat déjà
    /// présente dans `hp` si `max_hp` vaut encore 0, càd s'il n'a jamais été
    /// touché — cf. la doc de `Combat::max_hp`) : contrairement au reste du
    /// bestiaire, les Champiblobs ont `respawn_delay = 0` (jamais posé
    /// explicitement dans `scene::demos::riviere`), donc ne passent JAMAIS
    /// par `AppState::process_respawns`, seul autre code à restaurer `hp`.
    /// Sans cette restauration ici, un Champiblob tué pendant une phase où
    /// il était déjà révélé, puis re-« révélé » (idempotent, cf. plus haut)
    /// au seuil de phase suivant ou à une réapparition ultérieure du boss,
    /// redeviendrait visible avec `hp == 0` : un « cadavre » à l'apparence
    /// normale mais qui disparaît au moindre contact et ne revient jamais à
    /// pleine vie de toute la session — contraire à l'intention du design
    /// (la couvée doit revenir au complet avec le boss).
    fn reveal_boss2_minions(&mut self, target: usize) {
        if self.is_online_client() {
            return;
        }
        let minion_indices: Vec<usize> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.tag == BOSS2_MINION_TAG)
            .map(|(i, _)| i)
            .collect();
        let mut revealed_any = false;
        for &idx in minion_indices.iter().take(target) {
            let obj = &mut self.scene.objects[idx];
            if !obj.visible {
                obj.visible = true;
                revealed_any = true;
                if let Some(c) = obj.combat.as_mut()
                    && c.max_hp > 0
                {
                    c.hp = c.max_hp;
                    c.ranged_dmg_carry = 0.0;
                }
            }
        }
        if revealed_any {
            self.physics = Some(Physics::build(&self.scene));
        }
    }

    /// Masque toute la couvée (défaite du boss, ou pas encore réapparu) —
    /// idempotent, appelé sans condition depuis le début de `update_boss2`
    /// tant que le boss est invisible (cf. sa doc).
    fn hide_all_boss2_minions(&mut self) {
        for o in self.scene.objects.iter_mut().filter(|o| o.tag == BOSS2_MINION_TAG) {
            o.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_thresholds_match_the_validated_design() {
        assert_eq!(phase_for_ratio2(1.0), Boss2Phase::Germination);
        assert_eq!(phase_for_ratio2(0.51), Boss2Phase::Germination);
        assert_eq!(phase_for_ratio2(0.5), Boss2Phase::Proliferation);
        assert_eq!(phase_for_ratio2(0.26), Boss2Phase::Proliferation);
        assert_eq!(phase_for_ratio2(0.25), Boss2Phase::Eruption);
        assert_eq!(phase_for_ratio2(0.0), Boss2Phase::Eruption);
    }

    /// Même garde-fou que `app::boss::telegraph_pulse_accelerates_from_
    /// phase_to_phase` : l'Éruption des spores (attaque la plus dangereuse,
    /// windup le plus court) doit avoir le télégraphe le plus marqué.
    #[test]
    fn telegraph_pulse_accelerates_from_phase_to_phase() {
        let germination = telegraph_pulse_frequency2(Boss2Phase::Germination);
        let proliferation = telegraph_pulse_frequency2(Boss2Phase::Proliferation);
        let eruption = telegraph_pulse_frequency2(Boss2Phase::Eruption);
        assert!(proliferation > germination);
        assert!(eruption > proliferation);
    }

    fn boss2_index(app: &AppState) -> usize {
        app.scene
            .objects
            .iter()
            .position(|o| o.name == BOSS2_NAME)
            .expect("la démo Rivière doit contenir le second boss")
    }

    fn loot2_index(app: &AppState) -> usize {
        app.scene
            .objects
            .iter()
            .position(|o| o.name == BOSS2_LOOT_NAME)
            .expect("la démo Rivière doit contenir le butin du second boss")
    }

    fn minion_indices(app: &AppState) -> Vec<usize> {
        app.scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.tag == BOSS2_MINION_TAG)
            .map(|(i, _)| i)
            .collect()
    }

    /// Non-régression de spawn : le second boss apparaît visible, encaisse
    /// ses PV d'authoring, garde son propre tag/groupe (pas confondu avec le
    /// roster ordinaire ni avec le premier boss), n'est éligible qu'à une
    /// très longue réapparition, et sa couvée de trois Champiblobs est
    /// posée mais masquée tant qu'aucun combat n'a commencé.
    #[test]
    fn boss2_spawns_with_its_authored_hp_and_a_long_respawn_delay() {
        let scene = crate::scene::Scene::riviere_demo();
        let idx = scene
            .objects
            .iter()
            .position(|o| o.name == BOSS2_NAME)
            .expect("second boss absent de la démo Rivière");
        let boss = &scene.objects[idx];
        assert!(boss.visible, "le second boss doit apparaître visible dès le départ");
        assert_eq!(boss.tag, "boss2");
        assert_eq!(boss.group, "Boss2");
        let combat = boss.combat.as_ref().expect("le second boss doit être attaquable");
        assert!(combat.attackable);
        assert_eq!(combat.hp, 70, "PV d'authoring inattendus pour le second boss");
        assert_eq!(combat.wave, 0);
        assert_eq!(
            boss.ai_chaser.as_ref().map(|c| c.archetype),
            Some(crate::scene::Archetype::Colosse)
        );
        assert!(boss.bite.is_some());
        assert!(!boss.script.is_empty());
        assert_eq!(boss.respawn_delay, 600.0);

        let loot = scene
            .objects
            .iter()
            .find(|o| o.name == BOSS2_LOOT_NAME)
            .expect("butin du second boss absent de la démo Rivière");
        assert!(!loot.visible, "le butin garanti doit être masqué tant que le boss est vivant");
        assert!(loot.weapon_pickup.is_some());

        let minions: Vec<&crate::scene::SceneObject> =
            scene.objects.iter().filter(|o| o.tag == BOSS2_MINION_TAG).collect();
        assert_eq!(minions.len(), 3, "3 Champiblobs attendus");
        for m in &minions {
            assert!(!m.visible, "{} ne doit pas être visible avant le combat", m.name);
            assert!(m.ai_chaser.is_some());
            assert!(m.bite.is_some());
            assert!(m.combat.as_ref().is_some_and(|c| c.attackable));
        }
    }

    /// Preuve du seuil de phase appliqué **en jeu** : blesser le boss
    /// jusqu'à franchir les seuils 50 %/25 % change sa vitesse de poursuite,
    /// pilotée par `update_boss2` à chaque pas fixe — même patron que
    /// `app::boss::boss_chase_speed_increases_as_hp_drops_through_phase_
    /// thresholds`.
    #[test]
    fn boss2_chase_speed_increases_as_hp_drops_through_phase_thresholds() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);

        app.update_boss2(1.0 / 60.0, 0.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning2(Boss2Phase::Germination).chase_speed
        );

        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 50;
        app.update_boss2(1.0 / 60.0, 1.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning2(Boss2Phase::Proliferation).chase_speed
        );

        app.scene.objects[idx].combat.as_mut().unwrap().hp = 20;
        app.update_boss2(1.0 / 60.0, 2.0);
        assert_eq!(
            app.scene.objects[idx].ai_chaser.as_ref().unwrap().speed,
            tuning2(Boss2Phase::Eruption).chase_speed
        );
    }

    /// Preuve de l'invocation progressive : franchir les seuils de phase
    /// révèle bien 2 puis 3 Champiblobs (aucun en Germination), jamais plus
    /// que `target`, et jamais moins qu'avant (une couvée déjà révélée ne
    /// se re-masque pas en cours de combat).
    #[test]
    fn boss2_reveals_its_brood_progressively_as_phases_advance() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);
        app.physics = Some(Physics::build(&app.scene));

        app.update_boss2(1.0 / 60.0, 0.0);
        let revealed = |app: &AppState| {
            minion_indices(app)
                .iter()
                .filter(|&&i| app.scene.objects[i].visible)
                .count()
        };
        assert_eq!(revealed(&app), 0, "aucun Champiblob en Germination");

        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 50;
        app.update_boss2(1.0 / 60.0, 1.0);
        assert_eq!(revealed(&app), 2, "2 Champiblobs attendus en Prolifération");

        app.scene.objects[idx].combat.as_mut().unwrap().hp = 20;
        app.update_boss2(1.0 / 60.0, 2.0);
        assert_eq!(revealed(&app), 3, "les 3 Champiblobs attendus en Éruption");
    }

    /// Preuve de la zone de spores télégraphiée : le joueur resté dans le
    /// rayon marqué au moment de la détonation encaisse des dégâts ; un
    /// joueur qui s'est déplacé hors du rayon entre le marquage et la
    /// détonation n'en subit aucun — le contre-jeu voulu (« sortir du
    /// disque »), pas une esquive de trajectoire.
    #[test]
    fn boss2_marks_a_ground_hazard_and_only_damages_a_player_still_inside_it() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.is_some())
            .expect("joueur pilotable absent");
        app.hud_health = Some(1.0);
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 100;
        let radius = tuning2(Boss2Phase::Germination).radius;
        let center = app.scene.objects[idx].transform.position + Vec3::new(5.0, 0.0, 0.0);

        // Marque la zone directement (sans dépendre du tirage probabiliste,
        // même approche que les tests de `app::boss`) puis fait détoner au
        // pas suivant.
        app.scene.objects[pi].transform.position = center;
        app.physics = Some(Physics::build(&app.scene));
        app.physics.as_mut().unwrap().set_position(pi, center);
        app.boss2.hazard.frozen_pos = Some(app.scene.objects[idx].transform.position);
        app.boss2.hazard.pending_center = Some(center);
        app.boss2.hazard.stopped_until = Some(app.time + 1.0 / 60.0);
        app.sim_step(1.0 / 60.0);
        assert_eq!(app.hud_health, Some(1.0 - tuning2(Boss2Phase::Germination).damage));

        // Rejoue le même scénario, mais le joueur s'échappe avant la
        // détonation : aucun dégât.
        app.hud_health = Some(1.0);
        let outside = center + Vec3::new(radius + 3.0, 0.0, 0.0);
        app.scene.objects[pi].transform.position = outside;
        app.physics.as_mut().unwrap().set_position(pi, outside);
        app.boss2.hazard.frozen_pos = Some(app.scene.objects[idx].transform.position);
        app.boss2.hazard.pending_center = Some(center);
        app.boss2.hazard.stopped_until = Some(app.time + 1.0 / 60.0);
        app.sim_step(1.0 / 60.0);
        assert_eq!(app.hud_health, Some(1.0), "joueur sorti à temps : aucun dégât");
    }

    /// Preuve de la pause de récupération après la grande éruption de la
    /// phase « Éruption des spores » : le boss reste figé
    /// (`boss2_is_frozen`) juste après avoir détoné, il ne marque pas
    /// immédiatement une nouvelle zone.
    #[test]
    fn boss2_pauses_to_recover_after_its_eruption_detonation() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.is_some())
            .unwrap();
        app.hud_health = Some(1.0);
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 10;
        let center = app.scene.objects[idx].transform.position + Vec3::new(5.0, 0.0, 0.0);
        app.scene.objects[pi].transform.position = center;
        app.physics = Some(Physics::build(&app.scene));
        app.physics.as_mut().unwrap().set_position(pi, center);

        app.boss2.hazard.frozen_pos = Some(app.scene.objects[idx].transform.position);
        app.boss2.hazard.pending_center = Some(center);
        app.boss2.hazard.stopped_until = Some(app.time + 1.0 / 60.0);
        app.sim_step(1.0 / 60.0);

        assert!(
            app.boss2_is_frozen(idx),
            "le boss doit rester figé en pause de récupération juste après sa grande éruption"
        );
        assert!(app.boss2.hazard.recovering);
    }

    /// Non-régression (même patron que `app::boss::crossing_a_phase_
    /// threshold_triggers_a_one_shot_flash`) : franchir un seuil de phase
    /// déclenche un flash ponctuel, un appel qui n'en franchit aucun non.
    #[test]
    fn crossing_a_phase_threshold_triggers_a_one_shot_flash() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);

        app.update_boss2(1.0 / 60.0, 0.0);
        assert_eq!(app.fx.damage_flash, 0.0);

        app.fx.damage_flash = 0.0;
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 51;
        app.update_boss2(1.0 / 60.0, 1.0);
        assert_eq!(app.fx.damage_flash, 0.0, "reste en Germination : pas de transition");

        app.scene.objects[idx].combat.as_mut().unwrap().hp = 50;
        app.update_boss2(1.0 / 60.0, 2.0);
        assert_eq!(app.fx.damage_flash, 1.0);
    }

    /// Non-régression du bug des sbires « zombies » : un Champiblob tué
    /// pendant une phase où il était déjà révélé, puis re-révélé au
    /// franchissement du seuil de phase suivant, doit revenir à PLEINE VIE
    /// (`Combat::hp == Combat::max_hp`, `max_hp` > 0) et pleinement
    /// fonctionnel (`attackable`), pas seulement visible avec `hp == 0` —
    /// sinon il réapparaîtrait comme un cadavre qui disparaît au moindre
    /// contact et ne revient jamais à pleine vie de toute la session (cf. la
    /// doc de `reveal_boss2_minions`). Les Champiblobs ont `respawn_delay ==
    /// 0.0` (jamais fixé explicitement dans `scene::demos::riviere`) : ils
    /// ne passent donc jamais par `AppState::process_respawns`, seul autre
    /// code à restaurer `hp` — `reveal_boss2_minions` est le seul chemin qui
    /// peut le faire pour eux.
    #[test]
    fn boss2_reveals_a_previously_slain_minion_at_full_hp_not_as_an_invisible_husk() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);
        app.physics = Some(Physics::build(&app.scene));

        // Franchit le seuil de Prolifération : révèle 2 Champiblobs.
        app.update_boss2(1.0 / 60.0, 0.0);
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 50;
        app.update_boss2(1.0 / 60.0, 1.0);
        let minions = minion_indices(&app);
        let revealed: Vec<usize> = minions
            .iter()
            .copied()
            .filter(|&i| app.scene.objects[i].visible)
            .collect();
        assert_eq!(revealed.len(), 2, "2 Champiblobs attendus en Prolifération");
        let victim = revealed[0];
        assert_eq!(app.scene.objects[victim].respawn_delay, 0.0, "les Champiblobs ne réapparaissent jamais tout seuls");

        // Le joueur achève ce Champiblob (respawn_delay == 0 : pas de mise en
        // file dans `respawn_queue`, `AppState::process_respawns` ne le
        // touchera donc jamais). `damage_attackable_by` renvoie `true` sur le
        // coup qui achève la cible (hp tombé à 0) — on frappe donc jusqu'à ce
        // qu'il renvoie `true`.
        let authored_hp = app.scene.objects[victim].combat.as_ref().unwrap().hp;
        while !app.scene.damage_attackable_by(victim, 1) {}
        assert_eq!(app.scene.objects[victim].combat.as_ref().unwrap().hp, 0);
        assert!(!app.scene.objects[victim].visible, "un Champiblob vaincu doit disparaître");
        assert_eq!(
            app.scene.objects[victim].combat.as_ref().unwrap().max_hp,
            authored_hp,
            "max_hp doit avoir été capturé au premier coup"
        );

        // Franchit le seuil suivant (Éruption) : re-« révèle » (idempotent
        // pour les 2 déjà engagés) la couvée au complet, y compris le
        // Champiblob vaincu ci-dessus.
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 20;
        app.update_boss2(1.0 / 60.0, 2.0);

        assert!(
            app.scene.objects[victim].visible,
            "le Champiblob vaincu doit redevenir visible à la révélation suivante"
        );
        let victim_combat = app.scene.objects[victim].combat.as_ref().unwrap();
        assert_eq!(
            victim_combat.hp, victim_combat.max_hp,
            "il doit revenir à PLEINE vie, pas rester à 0 PV (bug des sbires zombies)"
        );
        assert!(victim_combat.hp > 0, "max_hp doit être positif : pas un cadavre indestructible-en-apparence mais déjà mort");
        assert!(victim_combat.attackable, "il doit rester une cible valide");

        // Pleinement fonctionnel, pas seulement visible : un coup ne doit
        // PAS le faire disparaître immédiatement comme un cadavre à 0 PV le
        // ferait (`damage_attackable_by` ne renvoie `true`, et ne masque la
        // cible, que sur le coup qui l'achève).
        assert!(
            !app.scene.damage_attackable_by(victim, 1),
            "un seul coup ne doit pas suffire à re-vaincre un Champiblob revenu à pleine vie"
        );
        assert!(
            app.scene.objects[victim].visible,
            "il doit rester visible après un seul coup, preuve qu'il n'était pas déjà à 0 PV"
        );
    }

    /// Preuve du loot garanti et du re-masquage de la couvée : masqué tant
    /// que le boss est vivant ou seulement blessé, révélé au pas fixe qui
    /// suit son dernier coup, masqué de nouveau à sa réapparition — et
    /// aucun Champiblob ne doit rester visible après cette réapparition
    /// (même patron que `app::boss::boss_defeat_reveals_its_guaranteed_
    /// loot_and_respawn_hides_it_again`, étendu à la couvée).
    #[test]
    fn boss2_defeat_reveals_its_guaranteed_loot_and_respawn_hides_everything_again() {
        let mut app = AppState::new();
        app.scene = crate::scene::Scene::riviere_demo();
        let idx = boss2_index(&app);
        let lidx = loot2_index(&app);
        app.physics = Some(Physics::build(&app.scene));
        assert!(!app.scene.objects[lidx].visible);

        // Fait avancer le combat jusqu'à révéler toute la couvée : le tout
        // premier appel n'observe que la phase initiale (pas un
        // franchissement, cf. la doc de `Boss2State::last_phase`), il faut
        // donc un second appel après avoir fait chuter les PV pour
        // effectivement croiser un seuil.
        app.scene.objects[idx].combat.as_mut().unwrap().max_hp = 100;
        app.update_boss2(1.0 / 60.0, 0.0);
        app.scene.objects[idx].combat.as_mut().unwrap().hp = 10;
        app.update_boss2(1.0 / 60.0, 1.0 / 60.0);
        let minions = minion_indices(&app);
        assert!(minions.iter().all(|&i| app.scene.objects[i].visible));

        // Coup fatal.
        assert!(app.scene.damage_attackable_by(idx, 10));
        assert!(!app.scene.objects[idx].visible);
        app.update_boss2(1.0 / 60.0, 1.0);
        assert!(
            app.scene.objects[lidx].visible,
            "le butin garanti doit apparaître à la mort du second boss"
        );
        assert!(
            minions.iter().all(|&i| !app.scene.objects[i].visible),
            "la couvée doit disparaître avec le boss vaincu"
        );

        // Réapparition simulée : ni le butin ni la couvée ne doivent rester au sol.
        app.scene.objects[idx].visible = true;
        let max_hp = app.scene.objects[idx].combat.as_ref().unwrap().max_hp;
        app.scene.objects[idx].combat.as_mut().unwrap().hp = max_hp;
        app.update_boss2(1.0 / 60.0, 2.0);
        assert!(!app.scene.objects[lidx].visible);
        assert!(minions.iter().all(|&i| !app.scene.objects[i].visible));
    }
}
