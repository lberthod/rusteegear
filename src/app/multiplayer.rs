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

impl AppState {
    /// Fait entrer un nouveau joueur réseau dans la partie : clone le premier
    /// objet « gabarit » pilotable trouvé dans la scène (le même gabarit que le
    /// joueur local utiliserait, cf. `player_index`) et l'ajoute comme un objet
    /// indépendant, avec sa propre entrée d'input. Reconstruit la physique
    /// (nouvel objet ⇒ nouveau corps rigide). `None` si la scène ne contient
    /// aucun gabarit pilotable (aucune manche/démo compatible chargée).
    pub fn spawn_network_player(&mut self, id: PlayerId) -> Option<usize> {
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
        self.physics = Some(crate::runtime::physics::Physics::build(&self.scene));
    }

    /// Enregistre l'input reçu d'un joueur réseau pour le tick courant : remplace
    /// le précédent (le client renvoie son état complet à chaque message, pas un
    /// delta, cf. `ClientMsg::Input`). Sans effet si `id` n'est pas (ou plus)
    /// connecté (message reçu après une déconnexion, par exemple).
    pub fn set_network_input(&mut self, id: PlayerId, input: NetworkInput) {
        if let Some(slot) = self.network_inputs.get_mut(&id) {
            *slot = input;
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
}
