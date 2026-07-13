//! Chargeurs de démo/scène embarquée : chaque fonction remplace `scene` par une des
//! démos prédéfinies (`crate::scene::Scene::*_demo`) et réinitialise l'état de jeu
//! associé (vie, manches, sélection). Extrait de `app/mod.rs` (Sprint 103a).

use super::AppState;
use crate::scene::Scene;

impl AppState {
    /// Charge la scène embarquée (jeu exporté) à la place de la démo : appelé en mode Player.
    pub fn use_embedded_scene(&mut self) {
        self.scene = Scene::embedded_player();
        self.selection = None;
    }

    /// Charge la démo mobile prête à jouer (avec historique pour annuler).
    pub fn load_mobile_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::mobile_demo();
        self.imported_dirty = true;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo gameplay complète (joystick/gyro/saut/zone/vie/tap).
    pub fn load_gameplay_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::gameplay_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « contrôleur » : joueur pilotable au joystick + saut, sans script.
    pub fn load_controller_demo(&mut self) {
        self.level = 1;
        self.push_undo();
        self.scene = Scene::controller_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = true;
        self.clear_selection();
    }

    /// Charge la démo « Tour d'ascension » (cf. `Scene::tower_demo`) : style de jeu
    /// différent de la démo contrôleur — platforming vertical pur, sans combat.
    pub fn load_tower_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::tower_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Course infinie » (cf. `Scene::temple_run_demo`) : 3ᵉ style de jeu
    /// — course automatique, changement de voie, obstacles à esquiver/sauter.
    pub fn load_temple_run_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::temple_run_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Vagues de zombies » (cf. `Scene::zombies_demo`) : jeu local
    /// contre l'ordinateur, sans réseau — manches de monstres poursuivant le joueur.
    pub fn load_zombies_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::zombies_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « MMORPG » (cf. `Scene::mmorpg_demo`) : arène minimale sans
    /// monstres/manches, dédiée au test multijoueur PC ↔ mobile (Sprint 65).
    pub fn load_mmorpg_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::mmorpg_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Donjon » façon roguelike (cf. `Scene::roguelike_demo`) : 3 salles
    /// à vider une à une (portes fermées jusqu'à la manche suivante), arme de départ
    /// tirée au sort parmi 3 profils à chaque chargement.
    pub fn load_roguelike_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::roguelike_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la démo « Duel » façon Tekken/Smash Bros (cf. `Scene::brawl_demo`) : arène
    /// flottante, un seul rival à plusieurs points de vie, à achever ou à sortir de
    /// l'arène (ring out).
    pub fn load_brawl_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::brawl_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Charge la scène **exemple** des composants optionnels (cf. `Scene::components_demo`) :
    /// Controller/AudioSource/Combat, un seul chacun, pour référence rapide (pas un niveau).
    pub fn load_components_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::components_demo();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }
}
