//! Inventaire du joueur (le « sac ») : objets trouvés en scène (`ItemPickup` —
//! potions, baies, clés, gemmes…), ramassés au contact comme les pièces et les
//! butins d'arme (cf. `sim_step`), empilés par sorte, et pour les consommables
//! utilisables depuis le panneau HUD 👜 (cf. `editor::hud::item_inventory_panel`).
//!
//! Troisième famille de ramassage, distincte des deux existantes : les pièces
//! (`collect_at`) comptent dans le score et l'objectif de victoire, les butins
//! d'arme (`weapon_pickup_at`) équipent immédiatement — un objet d'inventaire,
//! lui, se **garde** pour plus tard, d'où un état dédié (`AppState::inventory`)
//! plutôt qu'un effet instantané.

use crate::scene::ItemKind;

use super::AppState;

/// Rayon (m) de ramassage autour du joueur — même ordre que les pièces (0,7)
/// et les armes (0,9) : marcher sur l'objet suffit, pas besoin de viser.
const PICKUP_RADIUS: f32 = 0.8;

impl AppState {
    /// Ramassage des objets d'inventaire au contact du joueur — appelée chaque
    /// pas fixe depuis `sim_step`, à côté des pièces et des butins d'arme. Les
    /// objets à `respawn_delay > 0` réapparaissent (même file que les pièces
    /// bonus : un buisson à baies se régénère, une clé unique non).
    pub(super) fn update_item_pickups(&mut self) {
        let Some(p) = self.player_position() else {
            return;
        };
        let now = self.time;
        let hit = self.scene.item_pickups_at(p, PICKUP_RADIUS);
        if hit.is_empty() {
            return;
        }
        crate::runtime::sfx::play(&mut self.audio, crate::runtime::sfx::Sfx::Pickup);
        for (i, item) in hit {
            self.add_item(item.kind, item.count);
            log::info!(
                "Objet trouvé : {} ×{} (sac : {} au total)",
                item.kind.label(),
                item.count,
                self.item_count(item.kind)
            );
            let d = self.scene.objects[i].respawn_delay;
            if d > 0.0 {
                self.respawn_queue.push((i, now + d));
            }
        }
    }

    /// Ajoute `n` exemplaires d'une sorte au sac (empilés sur la ligne existante ;
    /// une sorte jamais vue s'ajoute en fin — l'ordre du sac est l'ordre de
    /// première découverte, stable pour le HUD, contrairement à une table de
    /// hachage qui ferait sauter les lignes d'une frame à l'autre).
    pub(crate) fn add_item(&mut self, kind: ItemKind, n: u32) {
        if let Some(entry) = self.inventory.iter_mut().find(|(k, _)| *k == kind) {
            entry.1 += n;
        } else {
            self.inventory.push((kind, n));
        }
    }

    /// Nombre d'exemplaires d'une sorte dans le sac.
    pub(crate) fn item_count(&self, kind: ItemKind) -> u32 {
        self.inventory
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(0, |&(_, n)| n)
    }

    /// Contenu du sac pour le HUD (cf. `editor::hud::item_inventory_panel`).
    pub fn inventory_items(&self) -> &[(ItemKind, u32)] {
        &self.inventory
    }

    /// Utilise (consomme) un exemplaire depuis le panneau HUD : un consommable
    /// (`ItemKind::heal() > 0`) soigne le joueur solo (`hud_health`) et sa ligne
    /// décroît (retirée à zéro). Sans effet — l'objet est conservé — si la sorte
    /// n'est pas consommable, si le sac n'en a pas, ou si la scène n'a pas de
    /// barre de vie (`hud_health == None` : rien à soigner, gaspiller l'objet
    /// serait une punition gratuite).
    pub fn use_item(&mut self, kind: ItemKind) {
        let heal = kind.heal();
        if heal <= 0.0 || self.item_count(kind) == 0 {
            return;
        }
        let Some(h) = self.hud_health else {
            return;
        };
        self.hud_health = Some((h + heal).min(1.0));
        if let Some(pos) = self.inventory.iter().position(|(k, _)| *k == kind) {
            self.inventory[pos].1 -= 1;
            if self.inventory[pos].1 == 0 {
                self.inventory.remove(pos);
            }
        }
        log::info!("{} utilisée : +{:.0} % de vie", kind.label(), heal * 100.0);
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::super::AppState;
    use crate::scene::{ItemKind, ItemPickup, MeshKind, Scene, SceneObject, Transform};

    /// Scène minimale : un joueur pilotable en 0 et un objet d'inventaire à
    /// `pos`, du genre et de la quantité donnés.
    fn scene_with_item(kind: ItemKind, count: u32, pos: Vec3) -> Scene {
        let mut scene = Scene::default();
        let mut player = SceneObject {
            name: "Joueur".into(),
            mesh: MeshKind::Capsule,
            ..Default::default()
        };
        player.controller = Some(crate::scene::Controller {
            input: true,
            ..Default::default()
        });
        scene.objects.push(player);
        let mut item = SceneObject {
            name: kind.label().into(),
            transform: Transform::from_pos(pos),
            ..Default::default()
        };
        item.item_pickup = Some(ItemPickup { kind, count });
        scene.objects.push(item);
        scene
    }

    /// Preuve de la demande gameplay « trouver des objets, les ramasser, ils
    /// vont dans un inventaire » : marcher sur une potion la masque en scène et
    /// l'ajoute au sac ; un objet hors de portée n'est pas ramassé.
    #[test]
    fn walking_over_an_item_puts_it_in_the_inventory() {
        let mut app = AppState::new();
        app.scene = scene_with_item(ItemKind::Potion, 1, Vec3::ZERO);
        app.update_item_pickups();
        assert_eq!(app.item_count(ItemKind::Potion), 1);
        assert!(!app.scene.objects[1].visible, "l'objet ramassé est masqué");

        let mut far = AppState::new();
        far.scene = scene_with_item(ItemKind::Potion, 1, Vec3::new(10.0, 0.0, 0.0));
        far.update_item_pickups();
        assert_eq!(far.item_count(ItemKind::Potion), 0);
        assert!(far.scene.objects[1].visible);
    }

    /// Les sortes s'empilent (2 ramassages de baies = une ligne ×2), et l'ordre
    /// du sac est l'ordre de première découverte (stable pour le HUD).
    #[test]
    fn items_stack_by_kind_in_discovery_order() {
        let mut app = AppState::new();
        app.add_item(ItemKind::Gemme, 1);
        app.add_item(ItemKind::Baie, 3);
        app.add_item(ItemKind::Gemme, 2);
        assert_eq!(
            app.inventory_items(),
            &[(ItemKind::Gemme, 3), (ItemKind::Baie, 3)]
        );
    }

    /// Utiliser une potion soigne (`hud_health`) et décrémente le sac ; la ligne
    /// disparaît à zéro. Une clé (non consommable) reste intacte, et sans barre
    /// de vie l'objet n'est pas gaspillé.
    #[test]
    fn using_a_potion_heals_and_consumes_it() {
        let mut app = AppState::new();
        app.hud_health = Some(0.5);
        app.add_item(ItemKind::Potion, 1);
        app.add_item(ItemKind::Cle, 1);

        app.use_item(ItemKind::Potion);
        assert_eq!(app.hud_health, Some(0.85));
        assert_eq!(app.item_count(ItemKind::Potion), 0);
        assert_eq!(app.inventory_items().len(), 1, "ligne à zéro retirée");

        app.use_item(ItemKind::Cle);
        assert_eq!(
            app.item_count(ItemKind::Cle),
            1,
            "une clé ne se consomme pas"
        );

        app.hud_health = None;
        app.add_item(ItemKind::Baie, 1);
        app.use_item(ItemKind::Baie);
        assert_eq!(
            app.item_count(ItemKind::Baie),
            1,
            "sans barre de vie, le consommable est conservé"
        );
    }

    /// Un objet à `respawn_delay > 0` (buisson à baies) réapparaît après le
    /// délai via la même file que les pièces bonus, et peut être ramassé à
    /// nouveau ; le sac cumule.
    #[test]
    fn respawning_item_can_be_picked_up_again() {
        let mut app = AppState::new();
        app.scene = scene_with_item(ItemKind::Baie, 2, Vec3::ZERO);
        app.scene.objects[1].respawn_delay = 1.0;

        app.update_item_pickups();
        assert_eq!(app.item_count(ItemKind::Baie), 2);
        assert_eq!(app.respawn_queue, vec![(1, 1.0)]);

        // Le délai écoulé, la file (traitée dans `sim_step`) le rend visible.
        app.scene.objects[1].visible = true;
        app.update_item_pickups();
        assert_eq!(app.item_count(ItemKind::Baie), 4);
    }

    /// L'inventaire repart vide à chaque nouvelle partie : la transition
    /// Édition→Play (`advance_play`) purge le sac, comme le score.
    #[test]
    fn inventory_resets_when_play_starts() {
        let mut app = AppState::new();
        app.add_item(ItemKind::Gemme, 5);
        app.playing = true;
        app.advance_play();
        assert!(app.inventory_items().is_empty());
    }
}
