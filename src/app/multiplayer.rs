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
    pub attack: bool,
    pub jump: bool,
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
        attack: input.attack,
        jump: input.jump,
    }
}

impl AppState {
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
    pub fn spawn_network_player(&mut self, id: PlayerId) -> Option<usize> {
        if let Some(&existing) = self.network_players.get(&id) {
            return Some(existing);
        }
        let mut template = self
            .scene
            .objects
            .iter()
            .find(|o| o.controller.as_ref().is_some_and(|c| c.input))?
            .clone();
        // Écarte chaque joueur du gabarit d'origine (et des précédents) : sans ça,
        // deux corps rigides spawnés au même point s'interpénètrent et la physique
        // les sépare par une violente impulsion à la première étape de simulation.
        let offset = (self.network_players.len() + 1) as f32 * 5.0;
        template.transform.position.x += offset;
        let index = self.scene.objects.len();
        self.scene.objects.push(template);
        self.network_players.insert(id, index);
        self.network_inputs.insert(id, NetworkInput::default());
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
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
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
            if !ready || !wants_attack {
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
    /// Limite connue (documentée, pas corrigée ici) : la santé par joueur n'est
    /// pas encore individualisée — `hud_health` reste un champ unique côté
    /// `AppState`, pensé pour un seul joueur local. `EntityDelta::health` est donc
    /// `None` pour l'instant ; individualiser la vie par joueur réseau est un
    /// prérequis pour un vrai combat joueur-contre-joueur (hors scope Sprint 55).
    pub fn network_snapshot(&self, tick: u32) -> Snapshot {
        let entities = self
            .network_players
            .values()
            .filter_map(|&index| self.scene.objects.get(index).map(|o| (index, o)))
            .map(|(index, o)| {
                let (yaw, _, _) = o.transform.rotation.to_euler(glam::EulerRot::YXZ);
                EntityDelta {
                    index: index as u32,
                    position: o.transform.position.to_array(),
                    yaw,
                    visible: o.visible,
                    health: None,
                }
            })
            .collect();
        Snapshot { tick, entities }
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
                attack: false,
                jump: false,
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
                attack: false,
                jump: false,
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
    fn sanitize_replaces_non_finite_axes_with_zero() {
        let dirty = NetworkInput {
            move_x: f32::NAN,
            move_y: f32::INFINITY,
            attack: true,
            jump: true,
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
            attack: false,
            jump: false,
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
                attack: false,
                jump: false,
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
                attack: true,
                jump: false,
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
