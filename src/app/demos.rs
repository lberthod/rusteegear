//! Chargeurs de démo/scène embarquée : chaque fonction remplace `scene` par une des
//! démos prédéfinies (`crate::scene::Scene::*_demo`) et réinitialise l'état de jeu
//! associé (vie, manches, sélection). Extrait de `app/mod.rs`.

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
    /// monstres/manches, dédiée au test multijoueur PC ↔ mobile.
    pub fn load_mmorpg_demo(&mut self) {
        self.push_undo();
        self.scene = Scene::mmorpg_demo();
        self.seed_mmorpg_repere_prefab_instances();
        self.imported_dirty = true;
        self.hud_health = None;
        self.damage_flash = 0.0;
        self.attack_flash = 0.0;
        self.wave = 0;
        self.is_leveled_demo = false;
        self.clear_selection();
    }

    /// Sprint 96, preuve d'implémentation : convertit les 4 « Repère N » de la démo
    /// MMORPG (4 objets indépendants, cf. `Scene::mmorpg_demo`) en véritables
    /// instances d'un même prefab `MmorpgRepere`, dans la portée **scène** `Mmorpg`
    /// (`assets_dir()/prefabs/scenes/Mmorpg/MmorpgRepere.json`) plutôt que générale —
    /// démonstration du deuxième niveau de portée (complément prefabs généraux/par
    /// scène). Éditer ce fichier à la main (ou changer un repère dans l'Inspecteur
    /// puis cliquer « 🧊 Créer un prefab » à nouveau, même nom de scène) puis
    /// « 🔄 Resynchroniser les instances » répercute le changement sur les 4 à la
    /// fois — sauf sur celle qu'on aurait explicitement surchargée. Sans effet si
    /// `assets_dir()` est indisponible (pas de `$HOME`) : la démo garde alors ses 4
    /// repères indépendants, comportement identique à avant ce complément.
    fn seed_mmorpg_repere_prefab_instances(&mut self) {
        let Some(template_idx) = self.scene.objects.iter().position(|o| o.name == "Repère 1")
        else {
            return;
        };
        let template = self.scene.objects[template_idx].clone();
        let scope = crate::assets::PrefabScope::Scene("Mmorpg".into());
        let Ok(asset_id) = crate::scene::Scene::save_prefab(&template, "MmorpgRepere", &scope)
        else {
            return;
        };
        for i in 1..=4 {
            let name = format!("Repère {i}");
            let Some(idx) = self.scene.objects.iter().position(|o| o.name == name) else {
                continue;
            };
            let pos = self.scene.objects[idx].transform.position;
            if let Some(instance) = crate::scene::Scene::instantiate_prefab(&asset_id, name, pos) {
                self.scene.objects[idx] = instance;
            }
        }
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

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use glam::Vec3;

    use super::*;

    #[test]
    fn only_controller_demo_is_marked_as_leveled() {
        // `is_leveled_demo` pilote si le bouton de fin de partie appelle `next_level()`
        // (bascule vers `controller_level`) ou `restart_game()` (relance la même scène).
        // Une régression ici ferait basculer une victoire en course infinie / tour /
        // manches de zombies vers l'arène de combat au lieu de relancer la bonne scène.
        let mut app = AppState::new();
        app.load_controller_demo();
        assert!(app.is_leveled_demo, "démo contrôleur : à niveaux");

        app.load_tower_demo();
        assert!(!app.is_leveled_demo, "tour : pas de niveau suivant");

        app.load_temple_run_demo();
        assert!(
            !app.is_leveled_demo,
            "course infinie : pas de niveau suivant"
        );

        app.load_zombies_demo();
        assert!(
            !app.is_leveled_demo,
            "zombies : pas de niveau suivant (manches)"
        );

        app.load_gameplay_demo();
        assert!(!app.is_leveled_demo);

        app.load_components_demo();
        assert!(!app.is_leveled_demo);

        app.load_mobile_demo();
        assert!(!app.is_leveled_demo);

        app.load_roguelike_demo();
        assert!(
            !app.is_leveled_demo,
            "donjon : pas de niveau suivant (manches)"
        );

        app.load_brawl_demo();
        assert!(
            !app.is_leveled_demo,
            "duel : pas de niveau suivant (manches)"
        );
    }

    #[test]
    fn roguelike_demo_clears_rooms_one_at_a_time_to_victory() {
        // Bout en bout sur la vraie scène (pas une scène synthétique) : la salle 2 ne
        // doit pas être révélée avant que la salle 1 soit vidée, et ainsi de suite
        // jusqu'à la victoire — même mécanique que `wave_system_reveals_next_wave_...`
        // mais sur `Scene::roguelike_demo`, portée d'attaque élargie et préparation
        // nulle pour isoler la logique de manches de la précision de visée et de l'arme
        // tirée au sort (cf. commentaire similaire dans
        // `wave_system_reveals_next_wave_then_wins_on_the_last`). Le joueur ne bouge
        // jamais dans ce test (aucune entrée de mouvement) : le missile doit donc
        // parcourir toute la longueur du donjon pour la salle 3 (~20 m) — budget de
        // boucle large pour laisser le temps au missile homing d'arriver.
        let mut app = AppState::new();
        app.load_roguelike_demo();
        for o in &mut app.scene.objects {
            if let Some(c) = &mut o.controller
                && c.input
            {
                c.attack_range = 50.0;
                c.attack_cooldown = 0.0;
                c.attack_windup = 0.0;
            }
        }
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        assert_eq!(app.wave, 1, "démarre à la salle 1");

        let monster_count_wave = |app: &AppState, w: u32| {
            app.scene
                .objects
                .iter()
                .filter(|o| o.visible && o.combat.as_ref().is_some_and(|c| c.wave == w))
                .count()
        };
        assert_eq!(
            monster_count_wave(&app, 1),
            1,
            "salle 1 : son monstre est visible"
        );
        assert_eq!(monster_count_wave(&app, 2), 0, "salle 2 : encore masquée");
        assert_eq!(monster_count_wave(&app, 3), 0, "salle 3 : encore masquée");

        app.input_state.attack = true;
        for wave in 1..=3u32 {
            for _ in 0..100 {
                app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
                app.advance_play();
                if app.wave > wave || app.has_won() {
                    break;
                }
            }
        }
        assert!(
            app.has_won(),
            "les 3 salles vidées doivent déclencher la victoire (wave={})",
            app.wave
        );
    }

    #[test]
    fn roguelike_demo_walking_onto_a_weapon_pickup_reequips_the_player() {
        // Le ramassage d'arme (donjon roguelike) est **natif** (pas un script Lua, qui ne
        // peut pas modifier `Controller`) : bout en bout via `advance_play`, pas
        // seulement au niveau `Scene::weapon_pickup_at` (déjà testé isolément côté scène).
        let mut app = AppState::new();
        app.load_roguelike_demo();
        let (loot_idx, loot_pos, expected) = app
            .scene
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| {
                o.weapon_pickup
                    .map(|wp| (i, o.transform.position, crate::scene::WEAPONS[wp.weapon]))
            })
            .expect("le donjon a au moins un butin d'arme");
        let pi = app
            .scene
            .objects
            .iter()
            .position(|o| o.controller.as_ref().is_some_and(|c| c.input))
            .unwrap();
        // Place le joueur exactement sur le butin (au lieu de simuler un déplacement) :
        // isole la résolution du ramassage de la logique de déplacement, déjà testée
        // ailleurs (`controller_demo_player_moves_with_joystick`).
        app.scene.objects[pi].transform.position = loot_pos;

        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();

        let ctrl = app.scene.objects[pi].controller.as_ref().unwrap();
        assert_eq!(
            (ctrl.attack_range, ctrl.attack_cooldown, ctrl.attack_windup),
            (expected.range, expected.cooldown, expected.windup),
            "le joueur doit être équipé du profil du butin ramassé"
        );
        assert!(
            !app.scene.objects[loot_idx].visible,
            "le butin ramassé doit disparaître"
        );
        assert_eq!(
            app.score(),
            1,
            "un butin ramassé doit compter au score, comme une pièce"
        );
    }

    #[test]
    fn brawl_demo_rival_survives_two_hits_then_falls_on_the_third() {
        // Le cœur du duel façon Tekken/Smash : le rival a plusieurs PV (cf.
        // `Combat::hp`), donc encaisse d'abord, ne meurt pas au premier coup. Portée
        // élargie et recharge/préparation nulles pour isoler la mécanique de PV de la
        // précision de visée et du timing (même convention que les tests de manches).
        let mut app = AppState::new();
        app.load_brawl_demo();
        let ri = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Rival")
            .unwrap();
        for o in &mut app.scene.objects {
            if let Some(c) = &mut o.controller
                && c.input
            {
                c.attack_range = 50.0;
                c.attack_cooldown = 0.0;
                c.attack_windup = 0.0;
            }
        }
        app.playing = true;
        app.input_state.attack = true;

        let mut hp_history = Vec::new();
        for _ in 0..1000 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if let Some(hp) = app.scene.objects[ri].combat.as_ref().map(|c| c.hp)
                && hp_history.last() != Some(&hp)
            {
                hp_history.push(hp);
            }
            if !app.scene.objects[ri].visible {
                break;
            }
        }
        assert_eq!(
            hp_history,
            vec![3, 2, 1, 0],
            "le rival doit encaisser 3 coups avant de tomber, pas mourir au premier"
        );
        assert!(
            !app.scene.objects[ri].visible,
            "invisible une fois achevé au 3e coup"
        );
        assert_eq!(
            app.score(),
            1,
            "le score ne doit compter que le coup qui achève, pas les coups intermédiaires"
        );
        assert!(
            app.has_won(),
            "achever l'unique rival doit déclencher la victoire (cf. Combat::wave = 1)"
        );
    }

    #[test]
    fn brawl_demo_non_lethal_hit_knocks_the_rival_away_from_the_player() {
        // Contrepoint « Smash » du coup qui achève : un coup qui blesse sans tuer doit
        // repousser la cible (cf. `AppState::stagger`/`KNOCKBACK_SPEED`), pas la laisser
        // reprendre aussitôt sa poursuite comme si de rien n'était — sinon aucun ring out
        // n'est jamais possible.
        let mut app = AppState::new();
        app.load_brawl_demo();
        let ri = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Rival")
            .unwrap();
        for o in &mut app.scene.objects {
            if let Some(c) = &mut o.controller
                && c.input
            {
                c.attack_range = 50.0;
                // Recharge énorme : un seul coup possible sur toute la durée du test,
                // pour observer le recul sans qu'un 2e coup n'interfère.
                c.attack_cooldown = 100.0;
                c.attack_windup = 0.0;
            }
        }
        app.playing = true;
        app.input_state.attack = true;

        let mut pos_at_impact = None;
        for _ in 0..200 {
            app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
            app.advance_play();
            if app.scene.objects[ri]
                .combat
                .as_ref()
                .is_some_and(|c| c.hp == 2)
            {
                pos_at_impact = Some(app.scene.objects[ri].transform.position);
                break;
            }
        }
        let pos_at_impact = pos_at_impact.expect("le 1er coup (non-létal) doit atterrir");
        let player_pos = app
            .player_position()
            .expect("le joueur ne bouge pas dans ce test (aucune entrée de mouvement)");
        let dist0 = (pos_at_impact - player_pos).length();

        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();
        let dist1 = (app.scene.objects[ri].transform.position - player_pos).length();
        assert!(
            dist1 > dist0,
            "le rival doit s'éloigner juste après un coup non-létal, pas continuer de \
             se rapprocher comme le ferait une poursuite ininterrompue (avant={dist0}, après={dist1})"
        );
    }

    #[test]
    fn falling_into_the_void_ring_outs_the_rival_and_counts_as_victory() {
        // Deuxième façon de gagner un duel façon Smash : sortir l'adversaire de l'arène,
        // pas seulement l'achever à coups de poing (cf. `Scene::brawl_demo`).
        let mut app = AppState::new();
        app.load_brawl_demo();
        let ri = app
            .scene
            .objects
            .iter()
            .position(|o| o.name == "Rival")
            .unwrap();
        // Téléporte le rival dans le vide sous l'arène (au lieu de simuler un vrai
        // recul jusqu'au bord) : isole la détection du ring out de la mécanique de
        // recul, déjà testée ailleurs (`brawl_demo_non_lethal_hit_knocks_the_rival_away_from_the_player`).
        app.scene.objects[ri].transform.position = Vec3::new(0.0, -8.0, 0.0);
        app.playing = true;
        app.last_frame = Instant::now() - std::time::Duration::from_secs_f32(0.05);
        app.advance_play();

        assert!(
            !app.scene.objects[ri].visible,
            "le rival doit être vaincu en tombant dans le vide"
        );
        assert!(
            app.has_won(),
            "un ring out doit compter comme une victoire (adversaire unique, wave=1)"
        );
    }

    #[test]
    fn mmorpg_demo_landmarks_are_prefab_instances_sharing_one_template() {
        // Sprint 96, preuve d'implémentation : les 4 repères ne sont plus 4 objets
        // indépendants mais 4 instances du même prefab (cf. `seed_mmorpg_repere_
        // prefab_instances`) — sauf si `assets_dir()` est indisponible (pas de $HOME),
        // auquel cas ce test n'a rien à vérifier.
        if crate::assets::assets_dir().is_none() {
            return;
        }
        // Verrou : ce test écrit dans le vrai `assets_dir()` (nom de prefab fixe,
        // `MmorpgRepere`), à sérialiser avec les autres tests dans le même cas (cf.
        // `REAL_ASSETS_DIR_TEST_LOCK` et `app::tests::a_spawned_enemy_via_lua_...`).
        let _guard = crate::assets::REAL_ASSETS_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut app = AppState::new();
        app.load_mmorpg_demo();
        let asset_ids: Vec<String> = (1..=4)
            .map(|i| {
                let name = format!("Repère {i}");
                let obj = app
                    .scene
                    .objects
                    .iter()
                    .find(|o| o.name == name)
                    .unwrap_or_else(|| panic!("{name} attendu dans la démo MMORPG"));
                obj.prefab
                    .as_ref()
                    .unwrap_or_else(|| panic!("{name} doit être une instance de prefab"))
                    .asset_id
                    .clone()
            })
            .collect();
        assert!(
            asset_ids.windows(2).all(|w| w[0] == w[1]),
            "les 4 repères doivent partager la même référence de prefab : {asset_ids:?}"
        );
        // Positions distinctes malgré le prefab partagé — `transform` est bien
        // surchargé par instance (cf. `Scene::instantiate_prefab`).
        let positions: std::collections::HashSet<_> = (1..=4)
            .map(|i| {
                let name = format!("Repère {i}");
                let p = app
                    .scene
                    .objects
                    .iter()
                    .find(|o| o.name == name)
                    .unwrap()
                    .transform
                    .position;
                (p.x.to_bits(), p.z.to_bits())
            })
            .collect();
        assert_eq!(
            positions.len(),
            4,
            "les 4 repères doivent garder des positions distinctes"
        );
    }
}
