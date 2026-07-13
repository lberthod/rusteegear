//! Salons multijoueurs (SPRINT_MMORPG.md, Sprint 55) : associe chaque joueur
//! réseau (`PlayerId`) à un objet de la scène qu'il pilote, sur le même principe
//! que `combat.rs` (Sprint 50) — isoler la surface de gameplay qu'un transport
//! réseau doit piloter, sans mélanger cette logique au reste de `AppState`.
//!
//! `pub` (contrairement à `combat`, privé) : `src/bin/server.rs` — un binaire
//! séparé qui ne voit que l'API publique de la bibliothèque — doit pouvoir
//! appeler ces méthodes pour faire entrer/sortir des joueurs réseau.

use super::AppState;
use crate::net::protocol::{EntityDelta, PlayerId, Snapshot};

/// État de contrôle d'un joueur réseau pour le tick courant (cf.
/// `net::protocol::ClientMsg::Input`, dont les champs sont recopiés tels quels
/// ici plutôt que de dépendre directement du type réseau — garde `AppState`
/// utilisable même si le protocole évolue, cf. la même séparation pour
/// `PlayerInput`, qui sert au joueur local).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetworkInput {
    pub move_x: f32,
    pub move_y: f32,
    /// Orientation (yaw, radians) prédite/affichée par le client — appliquée à
    /// son objet côté serveur (cf. `sim_step`) et direction de ses tirs (cf.
    /// `ClientMsg::Input::aim_yaw` pour le pourquoi).
    pub aim_yaw: f32,
    pub attack: bool,
    pub jump: bool,
    /// Tir de l'arme à distance (cf. `app::fireball`) : recharge validée côté
    /// serveur par objet tireur (`AppState::fireball_cooldowns`), comme `attack`.
    pub fire: bool,
    /// Arme à distance sélectionnée (indice dans `fireball::RANGED_WEAPONS`,
    /// borné par `sanitize_network_input`).
    pub weapon: u8,
    /// Soin d'un allié proche (cf. `app::health`) : résolu et validé côté serveur.
    pub heal: bool,
}

/// Portée (m) de l'attaque réseau : un coup immédiat au contact, pas le
/// missile homing avec préparation du joueur local (`app::combat`, qui
/// dépend d'un unique `attack_charge`/`attack_projectile` par `AppState` —
/// en avoir un par joueur réseau est un vrai chantier de refonte, hors scope
/// ici). Volontairement courte : un coup à distance serait un simplement un
/// substitut dégradé du système existant, pas une intention de design.
const NETWORK_ATTACK_RANGE: f32 = 1.2;

/// Temps de recharge (s) entre deux attaques d'un même joueur réseau — sans
/// lui, un client qui maintient (ou spam) `attack: true` défairait tout ce
/// qui entre en portée sans le moindre risque, cf. le même raisonnement que
/// `Controller::attack_cooldown` pour le joueur local (`app::combat`).
const NETWORK_ATTACK_COOLDOWN: f32 = 0.4;

/// Nettoie un `NetworkInput` reçu du réseau avant de le mémoriser (cf.
/// `AppState::set_network_input`) : rejette `NaN`/infini (remplacés par 0, le
/// neutre pour un axe de déplacement) et borne les axes à `[-1, 1]` — la même
/// borne que le joueur local (`inp.joy.0.clamp(-1.0, 1.0)`), appliquée ici à
/// la source plutôt que de faire confiance à `sim_step` pour la répéter.
fn sanitize_network_input(input: NetworkInput) -> NetworkInput {
    let clean = |v: f32| {
        if v.is_finite() {
            v.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    };
    NetworkInput {
        move_x: clean(input.move_x),
        move_y: clean(input.move_y),
        // Yaw : seul `NaN`/infini est dangereux (il se propagerait au quaternion
        // de l'objet puis au snapshot de tout le monde) — pas de clamp [-1, 1]
        // ici, un angle vit naturellement au-delà (sin/cos se moquent du reste).
        aim_yaw: if input.aim_yaw.is_finite() {
            input.aim_yaw
        } else {
            0.0
        },
        attack: input.attack,
        jump: input.jump,
        fire: input.fire,
        // Borné à la table réelle dès la réception : le reste du code peut
        // indexer `RANGED_WEAPONS` sans re-vérifier.
        weapon: super::fireball::clamp_weapon(input.weapon) as u8,
        heal: input.heal,
    }
}

impl AppState {
    /// Masque le gabarit joueur local avant même qu'un joueur ne rejoigne — à
    /// appeler par un serveur headless juste après avoir chargé la scène, et
    /// **avant** de démarrer le Play (`self.playing = true`).
    ///
    /// Sans cet appel, le gabarit reste « le joueur » du point de vue de
    /// `player_index` (c'est le premier objet pilotable trouvé) pendant toute
    /// l'attente du premier joueur réseau : personne ne le pilote, les
    /// monstres le poursuivent quand même et sa santé s'épuise sans qu'il ne
    /// bouge jamais — la manche se terminait en défaite après quelques
    /// secondes, **avant même qu'un joueur n'ait eu le temps de se
    /// connecter** (bug trouvé en testant réellement deux applications l'une
    /// contre l'autre, cf. AUDIT_MMORPG.md). `spawn_network_player` masque
    /// aussi ce gabarit dès le premier joueur, donc cet appel n'est utile que
    /// pour la fenêtre *avant* ce premier join.
    pub fn hide_local_player_template(&mut self) {
        if let Some(o) = self
            .scene
            .objects
            .iter_mut()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))
        {
            o.visible = false;
        }
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
    }

    /// Fait entrer un nouveau joueur réseau dans la partie : clone le premier
    /// objet « gabarit » pilotable trouvé dans la scène (le même gabarit que le
    /// joueur local utiliserait, cf. `player_index`) et l'ajoute comme un objet
    /// indépendant, avec sa propre entrée d'input. Reconstruit la physique
    /// (nouvel objet ⇒ nouveau corps rigide). `None` si la scène ne contient
    /// aucun gabarit pilotable (aucune manche/démo compatible chargée).
    ///
    /// **Idempotent (bug corrigé à l'audit du 2026-07-07)** : si `id` a déjà un
    /// objet (un second `ClientMsg::Join` du même client — rejeu réseau, bug
    /// client, ou trame forgée par un client modifié : rien dans le protocole
    /// n'empêchait un client d'envoyer `Join` une seconde fois après le
    /// premier, cf. `net::server_loop::handle_connection`, qui ne borne que la
    /// *première* trame à être un `Join`), renvoie l'objet existant plutôt que
    /// d'en spawner un second — sinon l'ancien objet devient un fantôme : plus
    /// jamais référencé par `network_players` (donc invisible du `Snapshot`
    /// réseau), mais toujours simulé par la physique indéfiniment, et chaque
    /// spawn en trop reconstruit toute la physique de la scène (coût qui
    /// grandit avec le nombre d'objets).
    ///
    /// **Recyclage des emplacements orphelins (audit en conditions réelles,
    /// 2026-07-13)** : `despawn_network_player` ne retire jamais l'objet du
    /// `Vec` (juste `visible = false`, cf. sa doc — retirer décalerait les
    /// indices des joueurs encore connectés). Sans recyclage, un salon qui
    /// voit beaucoup de va-et-vient (reconnexions, tests, joueurs qui
    /// abandonnent) accumule un clone du gabarit par `Join` **pour toujours**
    /// — chacun avec son propre corps physique dynamique, simulé à chaque pas
    /// même invisible (`controller.input` rend l'objet « controllable »
    /// indépendamment de sa visibilité, contrairement à `ai_chaser`). Sur un
    /// salon de longue durée (le salon partagé par défaut, potentiellement
    /// des heures), ça grossit sans borne et alourdit chaque `Physics::build`
    /// (reconstruit à chaque join/leave) — un vrai risque de à-coups perçus
    /// comme des blocages de mouvement en jeu. On réutilise donc en priorité
    /// un clone déjà présent (`controller.input`, hors gabarit d'origine) qui
    /// n'appartient plus à aucun joueur connu, avant d'en pousser un nouveau —
    /// borne la taille de la scène au pic de joueurs *simultanés* jamais
    /// atteint, pas au nombre cumulé de connexions depuis le démarrage.
    pub fn spawn_network_player(&mut self, id: PlayerId) -> Option<usize> {
        if let Some(&existing) = self.network_players.get(&id) {
            return Some(existing);
        }
        let template_index = self
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))?;
        let reusable_index = self.scene.objects.iter().enumerate().position(|(i, o)| {
            i != template_index
                && o.controller.as_ref().is_some_and(|c| c.input)
                && !self.network_players.values().any(|&v| v == i)
        });
        let mut template = self.scene.objects[template_index].clone();
        // Écarte chaque joueur du gabarit d'origine (et des précédents) : sans ça,
        // deux corps rigides spawnés au même point s'interpénètrent et la physique
        // les sépare par une violente impulsion à la première étape de simulation.
        // Cercle serré autour du gabarit (pas une ligne qui s'éloigne de +5 m par
        // joueur, cf. l'audit du 2026-07-12) : tous les joueurs démarrent proches
        // les uns des autres, dans le même coin de la carte, pour pouvoir se voir
        // et marcher les uns vers les autres dès la connexion plutôt que de devoir
        // traverser toute l'arène.
        const SPAWN_RADIUS: f32 = 3.0;
        let n = self.network_players.len() as f32;
        let angle = n * std::f32::consts::TAU / 8.0;
        template.transform.position.x += angle.cos() * SPAWN_RADIUS;
        template.transform.position.z += angle.sin() * SPAWN_RADIUS;
        // Toujours visible, même si le gabarit d'origine est déjà masqué (cf. plus
        // bas, et `hide_local_player_template` appelé avant le premier join) : sans
        // ce reset, chaque joueur réseau hérite du `visible=false` du gabarit et
        // `network_snapshot` diffuse cette invisibilité telle quelle — les clients
        // ne voient alors jamais aucun fantôme, quelle que soit la justesse du reste
        // du pipeline réseau (bug constaté en conditions réelles, 2026-07-12 : deux
        // vrais clients connectés, positions bien reçues, `visible` toujours faux).
        template.visible = true;
        let index = match reusable_index {
            Some(i) => {
                self.scene.objects[i] = template;
                i
            }
            None => {
                let i = self.scene.objects.len();
                self.scene.objects.push(template);
                i
            }
        };
        self.network_players.insert(id, index);
        self.network_inputs.insert(id, NetworkInput::default());
        // Vie individualisée (GAMEDESIGN_EN_LIGNE.md §3.1) : chaque joueur
        // réseau démarre à pleine vie, indépendamment des autres — cf.
        // `app::health`.
        self.network_health
            .insert(id, crate::app::health::MAX_HEALTH);
        // Masque le gabarit d'origine : personne ne le pilote (ni un joueur
        // réseau — chacun a son propre clone — ni un joueur local, le serveur
        // étant headless). Sans ça, `player_index` continuerait de le désigner
        // comme « le joueur » (c'est le premier objet pilotable trouvé) : les
        // monstres poursuivraient un mannequin inerte au lieu des vrais joueurs
        // réseau, et sa santé qui s'épuise sans qu'il ne bouge jamais
        // terminerait la manche en défaite en quelques secondes, avant même
        // qu'un joueur ait eu le temps de rejoindre (cf. AUDIT_MMORPG.md).
        self.scene.objects[template_index].visible = false;
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
        Some(index)
    }

    /// Retire un joueur réseau (déconnexion volontaire ou timeout, cf. Sprint 60) :
    /// masque son objet (comme un ennemi vaincu, cf. `Combat::attackable`) plutôt
    /// que de le retirer du `Vec` — un retrait décalerait les indices de tous les
    /// joueurs suivants dans `scene.objects`, cassant leur mapping `network_players`.
    pub fn despawn_network_player(&mut self, id: PlayerId) {
        if let Some(index) = self.network_players.remove(&id)
            && let Some(o) = self.scene.objects.get_mut(index)
        {
            o.visible = false;
        }
        self.network_inputs.remove(&id);
        self.network_attack_cooldowns.remove(&id);
        self.network_health.remove(&id);
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
    }

    /// Oublie tous les joueurs réseau (audit du 2026-07-07, cf. AUDIT_MMORPG.md
    /// §4.2) : à appeler chaque fois que `scene.objects` est remis à un état
    /// antérieur en bloc (`restart_game`, transition Play→Edit dans
    /// `advance_play`), qui ne connaît pas les objets ajoutés en cours de partie
    /// par `spawn_network_player`. Sans cet appel, `network_players` continuerait
    /// de pointer vers des indices obsolètes (un autre objet, ou plus rien) après
    /// la restauration — un joueur réseau piloterait alors le mauvais objet.
    /// Ne notifie pas les clients (pas de `PlayerLeft` envoyé) : c'est un simple
    /// nettoyage de la table de correspondance côté `AppState`, pas une
    /// déconnexion — le serveur réseau (`src/bin/server.rs`) reste responsable de
    /// décider s'il faut aussi fermer les connexions ou re-spawner les joueurs
    /// dans la scène restaurée.
    pub fn clear_network_players(&mut self) {
        self.network_players.clear();
        self.network_inputs.clear();
        self.network_attack_cooldowns.clear();
        self.network_health.clear();
    }

    /// Enregistre l'input reçu d'un joueur réseau pour le tick courant : remplace
    /// le précédent (le client renvoie son état complet à chaque message, pas un
    /// delta, cf. `ClientMsg::Input`). Sans effet si `id` n'est pas (ou plus)
    /// connecté (message reçu après une déconnexion, par exemple).
    ///
    /// **Durcissement (Sprint 60)** : les valeurs brutes reçues du réseau ne
    /// sont **jamais** dignes de confiance (cf. `sanitize_network_input`) — un
    /// client modifié pourrait envoyer `NaN`/`Infinity` (bytes bincode arbitraires,
    /// pas nécessairement produits par un client légitime passant par des sliders
    /// egui bornés) ; sans nettoyage ici, un `NaN` se propagerait dans la
    /// position physique de l'objet (`f32::clamp` ne filtre pas `NaN`, cf. la
    /// sémantique de `f32::clamp` : les comparaisons avec `NaN` sont toujours
    /// fausses) et corromprait la simulation pour tout le monde, pas seulement
    /// ce joueur.
    pub fn set_network_input(&mut self, id: PlayerId, input: NetworkInput) {
        if let Some(slot) = self.network_inputs.get_mut(&id) {
            *slot = sanitize_network_input(input);
        }
    }

    /// Indice de l'objet piloté par ce joueur réseau, s'il est connecté.
    pub fn network_player_object(&self, id: PlayerId) -> Option<usize> {
        self.network_players.get(&id).copied()
    }

    /// Nombre de joueurs réseau actuellement en jeu (hors joueur local).
    pub fn network_player_count(&self) -> usize {
        self.network_players.len()
    }

    /// Résout les attaques des joueurs réseau pour ce tick (Sprint 60) : décompte
    /// les temps de recharge, puis pour chaque joueur dont l'`Input` demande une
    /// attaque et dont le temps de recharge est écoulé, frappe immédiatement à
    /// portée (`NETWORK_ATTACK_RANGE`) depuis sa position — validation **serveur**
    /// du temps de recharge (`NETWORK_ATTACK_COOLDOWN`), pas seulement affichée
    /// côté client : un client modifié qui renvoie `attack: true` à chaque tick
    /// ne peut pas frapper plus vite que le temps de recharge imposé ici.
    ///
    /// **Vaincu = pas d'attaque** (GAMEDESIGN_EN_LIGNE.md §3.1) : un joueur à
    /// 0 PV est spectateur, son `Input` continue d'arriver (le client ne sait
    /// pas qu'il doit arrêter d'envoyer) mais le serveur l'ignore.
    pub fn update_network_attacks(&mut self, dt: f32) {
        for cd in self.network_attack_cooldowns.values_mut() {
            *cd -= dt;
        }
        let ids: Vec<PlayerId> = self.network_players.keys().copied().collect();
        for id in ids {
            let ready = self
                .network_attack_cooldowns
                .get(&id)
                .is_none_or(|cd| *cd <= 0.0);
            let wants_attack = self.network_inputs.get(&id).is_some_and(|i| i.attack);
            let alive = self.network_health.get(&id).copied().unwrap_or(1.0) > 0.0;
            if !ready || !wants_attack || !alive {
                continue;
            }
            let Some(index) = self.network_players.get(&id).copied() else {
                continue;
            };
            let Some(pos) = self.scene.objects.get(index).map(|o| o.transform.position) else {
                continue;
            };
            self.scene.attack_at(pos, NETWORK_ATTACK_RANGE);
            self.network_attack_cooldowns
                .insert(id, NETWORK_ATTACK_COOLDOWN);
        }
    }

    /// Construit un `Snapshot` de tous les joueurs réseau, pour diffusion via
    /// `ServerMsg::Snapshot` (cf. `net::server_loop::NetServer::broadcast`).
    ///
    /// **Vie individualisée (GAMEDESIGN_EN_LIGNE.md §3.1)** : `health` porte
    /// désormais la vie propre de chaque joueur (`app::health`), plus le champ
    /// scalaire unique d'avant — chaque client voit la vie de chacun, pas
    /// seulement la sienne (cf. `network_client::RemotePlayer::health`).
    pub fn network_snapshot(&self, tick: u32) -> Snapshot {
        let mut entities: Vec<EntityDelta> = self
            .network_players
            .iter()
            .filter_map(|(&id, &index)| self.scene.objects.get(index).map(|o| (id, index, o)))
            .map(|(id, index, o)| {
                let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
                EntityDelta {
                    index: index as u32,
                    player_id: Some(id),
                    position: o.transform.position.to_array(),
                    yaw,
                    visible: o.visible,
                    health: self.network_health.get(&id).copied(),
                    anim_clip: o
                        .animation
                        .as_ref()
                        .map(|a| a.clip.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        // Monstres (`Combat::attackable`, hors joueurs et ancre FX) : diffusés
        // avec `player_id: None` — le cas prévu de longue date par
        // `EntityDelta::player_id`, activé avec l'attaque à distance : sans
        // cette diffusion, chaque client simulerait/afficherait ses propres
        // monstres, et un monstre tué par la boule de feu d'un joueur resterait
        // debout sur l'écran des autres. Diffusés même masqués (mort = le
        // `visible: false` doit atteindre tous les écrans), et **par valeur de
        // scène partagée** : les indices coïncident car serveur et clients
        // chargent la même scène embarquée (cf. `src/bin/server.rs`).
        for (index, o) in self.scene.objects.iter().enumerate() {
            let is_monster = o.controller.is_none()
                && o.combat
                    .as_ref()
                    .is_some_and(|c| c.attackable && !c.is_attack_fx);
            if !is_monster {
                continue;
            }
            let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
            entities.push(EntityDelta {
                index: index as u32,
                player_id: None,
                position: o.transform.position.to_array(),
                yaw,
                visible: o.visible,
                health: None,
                anim_clip: o
                    .animation
                    .as_ref()
                    .map(|a| a.clip.clone())
                    .unwrap_or_default(),
            });
        }
        Snapshot {
            tick,
            entities,
            projectiles: self
                .fireballs
                .iter()
                .map(|fb| crate::net::protocol::ProjectileState {
                    position: fb.pos.to_array(),
                    weapon: fb.weapon as u8,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    fn app_with_zombies_demo() -> AppState {
        let mut app = AppState::new();
        app.load_zombies_demo();
        app
    }

    /// Régression trouvée en testant réellement un serveur headless
    /// (cf. AUDIT_MMORPG.md) : `hide_local_player_template` masque le gabarit
    /// avant le premier join, pour qu'aucun objet ne soit désigné « le
    /// joueur ». Sans exclure explicitement les monstres (`ai_chaser`/
    /// `combat.attackable`, qui portent aussi un script) du repli « premier
    /// objet scripté visible » de `player_index`, un monstre était désigné
    /// « le joueur » dès que les monstres devenaient visibles (manche 1) — les
    /// monstres se déclenchaient alors entre eux et vidaient `hud_health`
    /// (partagé) en quelques secondes, sans le moindre joueur connecté.
    #[test]
    fn waiting_for_the_first_player_never_drains_health_via_monster_scripts() {
        let mut app = app_with_zombies_demo();
        app.hide_local_player_template();
        app.playing = true;
        for _ in 0..80 {
            app.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        assert_eq!(
            app.hud_health, None,
            "sans joueur connecté, aucun script de monstre ne doit pouvoir affecter la vie"
        );
        assert!(
            !app.is_lost(),
            "la manche ne doit jamais se perdre toute seule en attendant"
        );
    }

    #[test]
    fn spawning_a_network_player_adds_a_new_controllable_object() {
        let mut app = app_with_zombies_demo();
        let before = app.scene.objects.len();

        let index = app
            .spawn_network_player(1)
            .expect("la démo zombies a un gabarit pilotable");

        assert_eq!(app.scene.objects.len(), before + 1);
        assert_eq!(app.network_player_object(1), Some(index));
        assert_eq!(app.network_player_count(), 1);
        assert!(app.scene.objects[index].controller.is_some());
    }

    #[test]
    fn two_network_players_get_independent_objects_and_ids() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1).unwrap();
        let b = app.spawn_network_player(2).unwrap();
        assert_ne!(a, b, "chaque joueur doit avoir son propre objet");
        assert_eq!(app.network_player_count(), 2);
    }

    /// Régression (trouvée à l'audit du 2026-07-07) : rien dans le protocole
    /// n'empêche un client d'envoyer un second `Join` (rejeu, bug client, trame
    /// forgée — cf. `net::server_loop::handle_connection`, qui ne borne que la
    /// *première* trame à être un `Join`). Avant le correctif, ce second appel
    /// spawnait un objet fantôme supplémentaire sans jamais nettoyer le
    /// premier : plus référencé par `network_players` (invisible du `Snapshot`
    /// réseau), mais simulé indéfiniment par la physique.
    #[test]
    fn spawning_twice_for_the_same_player_reuses_the_existing_object() {
        let mut app = app_with_zombies_demo();
        let before = app.scene.objects.len();

        let first = app.spawn_network_player(1).unwrap();
        let second = app.spawn_network_player(1).unwrap();

        assert_eq!(
            first, second,
            "un second Join du même joueur doit renvoyer le même objet, pas en créer un autre"
        );
        assert_eq!(
            app.scene.objects.len(),
            before + 1,
            "un seul objet doit avoir été ajouté à la scène, pas un fantôme par Join répété"
        );
        assert_eq!(app.network_player_count(), 1);
    }

    #[test]
    fn despawning_hides_the_object_and_forgets_the_player() {
        let mut app = app_with_zombies_demo();
        let index = app.spawn_network_player(1).unwrap();

        app.despawn_network_player(1);

        assert_eq!(app.network_player_object(1), None);
        assert_eq!(app.network_player_count(), 0);
        assert!(
            !app.scene.objects[index].visible,
            "l'objet doit rester en place (indices stables) mais devenir invisible"
        );
    }

    /// Audit en conditions réelles (2026-07-13) : sans recyclage, chaque
    /// `Join` pousse un nouveau clone dans `scene.objects`, pour toujours —
    /// un salon de longue durée (beaucoup de va-et-vient) grossit sans borne
    /// et alourdit chaque `Physics::build` (reconstruit à chaque join/leave),
    /// perçu en jeu comme des à-coups/blocages de mouvement. Vérifie qu'un
    /// nouveau joueur qui rejoint APRÈS le départ d'un précédent réutilise
    /// son emplacement au lieu d'en créer un troisième.
    #[test]
    fn a_new_player_reuses_a_slot_left_by_a_departed_one_instead_of_growing_the_scene() {
        let mut app = app_with_zombies_demo();
        let before = app.scene.objects.len();

        let first = app.spawn_network_player(1).unwrap();
        assert_eq!(app.scene.objects.len(), before + 1);
        app.despawn_network_player(1);

        let second = app.spawn_network_player(2).unwrap();

        assert_eq!(
            second, first,
            "le second joueur doit récupérer l'emplacement laissé par le premier"
        );
        assert_eq!(
            app.scene.objects.len(),
            before + 1,
            "la scène ne doit pas grossir tant qu'il ne dépasse jamais 1 joueur simultané"
        );
        assert!(
            app.scene.objects[second].visible,
            "l'emplacement recyclé doit redevenir visible pour son nouveau joueur"
        );
    }

    /// Complète le test précédent : avec deux joueurs simultanés, un 3e qui
    /// rejoint pendant que les deux premiers sont encore là doit bien obtenir
    /// un nouvel objet (rien à recycler) — le recyclage ne doit jamais faire
    /// partager un objet à deux joueurs connectés en même temps.
    #[test]
    fn simultaneous_players_never_share_a_recycled_slot() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1).unwrap();
        let b = app.spawn_network_player(2).unwrap();
        assert_ne!(a, b);

        app.despawn_network_player(1);
        // Pendant que 2 est toujours connecté, 3 rejoint : doit récupérer
        // l'emplacement de 1 (le seul orphelin), jamais celui de 2.
        let c = app.spawn_network_player(3).unwrap();

        assert_eq!(
            c, a,
            "le 3e joueur doit recycler l'emplacement du 1er, parti"
        );
        assert_ne!(c, b, "jamais l'emplacement d'un joueur encore connecté");
        assert_eq!(app.network_player_count(), 2);
    }

    /// Régression AUDIT_MMORPG.md §4.2 : `restart_game` remet `scene.objects` à
    /// l'état d'avant Play (qui ne connaît pas les joueurs réseau, spawnés en
    /// cours de partie) — `network_players` doit être oublié en même temps,
    /// sinon il continuerait de pointer vers des indices obsolètes.
    #[test]
    fn restart_game_forgets_network_players() {
        let mut app = app_with_zombies_demo();
        app.play_snapshot = app.scene.objects.clone();
        app.spawn_network_player(1);
        assert_eq!(app.network_player_count(), 1);

        app.restart_game();

        assert_eq!(
            app.network_player_count(),
            0,
            "un restart doit oublier les joueurs réseau, pas garder des indices obsolètes"
        );
        assert_eq!(app.network_player_object(1), None);
    }

    #[test]
    fn setting_input_for_an_unknown_player_is_a_no_op() {
        let mut app = app_with_zombies_demo();
        // Ne doit pas paniquer même si `id` n'a jamais rejoint (message tardif
        // après une déconnexion, par exemple).
        app.set_network_input(
            999,
            NetworkInput {
                move_x: 1.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            },
        );
        assert_eq!(app.network_player_count(), 0);
    }

    #[test]
    fn network_input_moves_the_players_own_object_independently() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1).unwrap();
        let b = app.spawn_network_player(2).unwrap();
        let start_a = app.scene.objects[a].transform.position;
        let start_b = app.scene.objects[b].transform.position;

        // Le joueur 1 avance en +X, le joueur 2 reste immobile (input par défaut).
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 1.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            },
        );
        app.playing = true;
        for _ in 0..30 {
            app.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }

        let end_a = app.scene.objects[a].transform.position;
        let end_b = app.scene.objects[b].transform.position;
        // Distance horizontale (X/Z) seulement : les deux capsules tombent et se
        // stabilisent sous la gravité (léger mouvement vertical + un peu de glissement
        // à l'atterrissage), sans rapport avec l'input — seul le déplacement
        // horizontal du joueur 1 doit refléter son `NetworkInput`.
        let horiz = |a: Vec3, b: Vec3| ((b.x - a.x).powi(2) + (b.z - a.z).powi(2)).sqrt();
        let moved_a = horiz(start_a, end_a);
        let moved_b = horiz(start_b, end_b);
        assert!(
            moved_a > 1.0,
            "le joueur 1 doit s'être nettement déplacé horizontalement : {start_a:?} -> {end_a:?}"
        );
        assert!(
            moved_b < moved_a * 0.5,
            "le joueur 2 (input par défaut) ne doit pas suivre le déplacement du joueur 1 : \
             joueur 1 = {moved_a:.2} m, joueur 2 = {moved_b:.2} m"
        );
    }

    #[test]
    fn network_snapshot_reports_every_connected_player() {
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1).unwrap();
        let b = app.spawn_network_player(2).unwrap();

        let snap = app.network_snapshot(7);
        assert_eq!(snap.tick, 7);
        let indices: Vec<u32> = snap.entities.iter().map(|e| e.index).collect();
        assert!(indices.contains(&(a as u32)));
        assert!(indices.contains(&(b as u32)));
    }

    #[test]
    fn network_snapshot_reports_the_player_animation_clip() {
        // Sprint 88 : le clip joué par un joueur réseau (s'il a un
        // `AnimationState`) doit atterrir dans son `EntityDelta`, pour que les
        // autres écrans jouent la même animation sur son fantôme.
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1).unwrap();
        app.scene.objects[a].animation = Some(crate::scene::AnimationState {
            clip: "run".into(),
            time: 0.0,
            speed: 1.0,
            prev_clip: String::new(),
            prev_time: 0.0,
            blend: 1.0,
        });

        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert_eq!(entity.anim_clip, "run");
    }

    #[test]
    fn network_snapshot_leaves_anim_clip_empty_without_animation_state() {
        // Un joueur sans `AnimationState` (mesh non skinné) ne doit pas planter
        // ni inventer un clip — champ vide, comme documenté.
        let mut app = app_with_zombies_demo();
        app.spawn_network_player(1).unwrap();
        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert!(entity.anim_clip.is_empty());
    }

    #[test]
    fn spawned_players_stay_visible_even_after_hiding_the_local_template() {
        // Reproduit l'ordre réel du serveur headless (`src/bin/server.rs`) :
        // `hide_local_player_template` masque le gabarit *avant* le premier
        // join. Sans le correctif (`template.visible = true` dans
        // `spawn_network_player`), chaque joueur réseau héritait de ce
        // `visible=false` — invisible pour toujours dans le `Snapshot`, malgré
        // des positions correctement transmises (bug constaté en conditions
        // réelles le 2026-07-12 : deux vrais clients connectés, jamais l'un
        // visible pour l'autre).
        let mut app = app_with_zombies_demo();
        app.hide_local_player_template();
        let index = app.spawn_network_player(1).unwrap();
        assert!(
            app.scene.objects[index].visible,
            "un joueur réseau tout juste spawné doit être visible, \
             même si le gabarit d'origine a été masqué avant son arrivée"
        );
        let snap = app.network_snapshot(1);
        let entity = snap
            .entities
            .iter()
            .find(|e| e.player_id == Some(1))
            .expect("le joueur 1 doit figurer dans le snapshot");
        assert!(
            entity.visible,
            "le snapshot diffusé doit refléter visible=true"
        );
    }

    #[test]
    fn sanitize_replaces_non_finite_axes_with_zero() {
        let dirty = NetworkInput {
            move_x: f32::NAN,
            move_y: f32::INFINITY,
            aim_yaw: 0.0,
            attack: true,
            jump: true,
            fire: false,
            weapon: 0,
            heal: false,
        };
        let clean = sanitize_network_input(dirty);
        assert_eq!(clean.move_x, 0.0);
        assert_eq!(clean.move_y, 0.0);
        // Les booléens ne sont pas concernés par le nettoyage numérique.
        assert!(clean.attack);
        assert!(clean.jump);
    }

    #[test]
    fn sanitize_clamps_axes_to_unit_range() {
        let dirty = NetworkInput {
            move_x: 50.0,
            move_y: -50.0,
            aim_yaw: 0.0,
            attack: false,
            jump: false,
            fire: false,
            weapon: 0,
            heal: false,
        };
        let clean = sanitize_network_input(dirty);
        assert_eq!(clean.move_x, 1.0);
        assert_eq!(clean.move_y, -1.0);
    }

    #[test]
    fn a_nan_input_from_the_network_never_corrupts_the_players_position() {
        // Un client modifié pourrait envoyer des octets produisant un NaN (pas
        // nécessairement un client légitime passant par des sliders bornés) :
        // vérifie que la position reste finie après plusieurs pas de simulation.
        let mut app = app_with_zombies_demo();
        let a = app.spawn_network_player(1).unwrap();
        app.set_network_input(
            1,
            NetworkInput {
                move_x: f32::NAN,
                move_y: f32::NAN,
                aim_yaw: 0.0,
                attack: false,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            },
        );
        app.playing = true;
        for _ in 0..10 {
            app.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let pos = app.scene.objects[a].transform.position;
        assert!(
            pos.is_finite(),
            "un NaN reçu du réseau ne doit jamais corrompre la position : {pos:?}"
        );
    }

    /// Construit une scène minimale : un joueur pilotable au centre, et deux
    /// cibles `attackable` à portée immédiate (cf. `NETWORK_ATTACK_RANGE`) —
    /// de quoi vérifier que le temps de recharge serveur limite bien le nombre
    /// de cibles vaincues par unité de temps, indépendamment de ce qu'un client
    /// prétend envoyer.
    fn scene_with_player_and_two_targets_in_range() -> crate::scene::Scene {
        use crate::scene::{Combat, Controller, MeshKind, Scene, SceneObject, Transform};

        let mut scene = Scene::default();
        scene.objects.push(SceneObject {
            name: "Joueur".into(),
            transform: Transform::from_pos(Vec3::ZERO),
            controller: Some(Controller {
                input: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        for i in 0..2 {
            scene.objects.push(SceneObject {
                name: format!("Cible {i}"),
                mesh: MeshKind::Cube,
                transform: Transform::from_pos(Vec3::new(0.3, 0.0, 0.0)),
                combat: Some(Combat {
                    attackable: true,
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        scene
    }

    #[test]
    fn server_rejects_attack_before_cooldown_elapsed() {
        let mut app = AppState::new();
        app.scene = scene_with_player_and_two_targets_in_range();
        let player_index = app.spawn_network_player(1).unwrap();
        // Les deux cibles sont à portée du point d'apparition du joueur réseau
        // (décalé de +5 en X par `spawn_network_player`) : replace-les au même
        // décalage pour rester dans `NETWORK_ATTACK_RANGE`.
        let player_pos = app.scene.objects[player_index].transform.position;
        for o in app.scene.objects.iter_mut() {
            if o.combat.is_some() {
                o.transform.position = player_pos;
            }
        }
        app.playing = true;
        app.set_network_input(
            1,
            NetworkInput {
                move_x: 0.0,
                move_y: 0.0,
                aim_yaw: 0.0,
                attack: true,
                jump: false,
                fire: false,
                weapon: 0,
                heal: false,
            },
        );

        // Plusieurs ticks rapprochés (bien en-deçà de NETWORK_ATTACK_COOLDOWN) :
        // un client qui spamme `attack: true` ne doit vaincre qu'UNE cible.
        for _ in 0..5 {
            app.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.02);
            app.advance_play();
        }
        let defeated_early: usize = app
            .scene
            .objects
            .iter()
            .filter(|o| o.combat.is_some() && !o.visible)
            .count();
        assert_eq!(
            defeated_early, 1,
            "le temps de recharge doit limiter à une seule cible vaincue malgré le spam"
        );

        // Après le temps de recharge, une nouvelle attaque doit passer.
        for _ in 0..15 {
            app.last_frame = std::time::Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
        }
        let defeated_later: usize = app
            .scene
            .objects
            .iter()
            .filter(|o| o.combat.is_some() && !o.visible)
            .count();
        assert_eq!(
            defeated_later, 2,
            "une fois le temps de recharge écoulé, la seconde cible doit tomber aussi"
        );
    }
}
