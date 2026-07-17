//! UI de l'éditeur basée sur egui : toolbar, hiérarchie, inspecteur.
//! Encapsule toute la plomberie egui-winit / egui-wgpu.

pub mod export;
mod hierarchy;
mod hud;
mod menus;
mod readiness;
mod windows;

use egui::ViewportId;
use glam::{EulerRot, Quat};
use winit::window::Window;

use hierarchy::hierarchy_panel;
use hud::{
    HudImageCache, HudWidgetValues, RosterEntry, ally_down_banner, collectibles_hud, crosshair,
    damage_vignette, defeated_banner, health_bar, hud_preview_overlays, hud_widgets,
    item_inventory_panel, kills_hud, lose_banner, mobile_overlay, multiplayer_roster_panel,
    restart_button, scene_has_ranged_weapon, touch_feedback, wave_hud, weapon_hud,
    weapon_inventory_panel,
};
use menus::{menu_aide, menu_ajouter, menu_edition, menu_fichier, menu_outils};
use windows::{
    ai_scene_window, asset_browser_window, device_bezel, hud_preview_window,
    mobile_multiplayer_overlay, multiplayer_window, optimize_window, play_area_rect,
    scripts_window, settings_window, tool_windows,
};

use crate::app::GizmoMode;
use crate::runtime::physics::PhysicsKind;
use crate::scene::{MeshKind, Scene, Transform};

pub struct Editor {
    ctx: egui::Context,
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    export: export::ExportPanel,
    /// Texte de filtre de la hiérarchie (recherche par nom).
    hier_filter: String,
    /// Nom saisi pour créer un nouveau groupe.
    hier_new_group: String,
    /// Renommage inline en cours : (index objet, texte en édition).
    hier_rename: Option<(usize, String)>,
    /// État des fenêtres flottantes (Aide + Outils).
    panels: Panels,
    /// Réglages utilisateur (clé API DeepSeek…), persistés sur disque.
    settings: crate::app::settings::Settings,
    /// Consigne en langage naturel pour la génération de script par IA.
    ai_prompt: String,
    /// Consigne pour la génération de scène entière par IA.
    ai_scene_prompt: String,
    /// Mode de génération de scène : remplacer (true) ou ajouter (false).
    ai_scene_replace: bool,
    /// Historique des consignes IA récentes (scène), pour ré-exécution rapide.
    ai_history: Vec<String>,
    /// Adresse du serveur multijoueur saisie dans la fenêtre Multijoueur.
    mp_server_url: String,
    /// Pseudo saisi dans la fenêtre Multijoueur.
    mp_name: String,
    /// Email saisi dans la fenêtre Multijoueur (connexion Firebase).
    mp_email: String,
    /// Mot de passe saisi dans la fenêtre Multijoueur (connexion Firebase).
    mp_password: String,
    /// Code de salon de chat saisi dans la fenêtre Multijoueur.
    mp_lobby_code: String,
    /// Message en cours de saisie dans le chat de la fenêtre Multijoueur.
    mp_chat_input: String,
    /// Commande en cours de saisie dans la Console.
    console_input: String,
    /// Réglages du panneau « Aperçu HUD ».
    hud_preview: HudPreview,
    /// Texte saisi pour l'identifiant du prochain widget HUD créé (panneau
    /// « 🧩 Widgets HUD »).
    hud_widget_new_id: String,
    /// Textures des widgets HUD `Image`, mises en cache par chemin d'asset — cf.
    /// `hud::HudImageCache`.
    hud_image_cache: HudImageCache,
    /// Contenu du journal de crash (Sprint 113), lu **une fois** à la construction
    /// — `None` si `crash_log::read()` n'a rien trouvé. Mis à `None` par « Fermer »
    /// (`crash_log::clear` supprime aussi le fichier) : pas de re-lecture à chaque
    /// frame, le fichier ne change pas en cours de session (seul un panic l'écrit,
    /// et celui-ci termine le process avant que cette valeur ne compte à nouveau).
    crash_log_text: Option<String>,
}

/// Réglages du panneau « 👁 Aperçu HUD » : quels overlays de jeu (réticule,
/// inventaire, joueurs…) prévisualiser en mode Édition, sans passer par Play —
/// pour voir/positionner ces éléments sans lancer la simulation. État
/// purement éditeur (pas persisté dans la scène, contrairement à `Controller`
/// ou `MobileControls`) : c'est une bascule d'aperçu, pas une config de jeu.
/// Quand connecté en Play, le tableau des joueurs affiche déjà les vrais
/// joueurs — l'aperçu ici sert juste à voir la fenêtre en Édition, avec des
/// données d'exemple.
#[derive(Default)]
struct HudPreview {
    open: bool,
    crosshair: bool,
    weapon_inventory: bool,
    item_inventory: bool,
    weapon_hud: bool,
    kills: bool,
    roster: bool,
    /// 🖐 Repositionner : rend les overlays cochés glissables à la souris ;
    /// leur position est alors écrite dans `Scene::hud_layout`. Séparé de
    /// `open` pour ne pas activer le glisser par accident dès l'ouverture du
    /// panneau.
    reposition: bool,
}

/// Visibilité et état des fenêtres flottantes des menus « Aide » et « Outils ».
#[derive(Default)]
struct Panels {
    // Aide
    shortcuts: bool,
    diagnostic: bool,
    about: bool,
    // Outils
    console: bool,
    profiler: bool,
    /// Historique récent des FPS pour le graphe du profiler.
    fps_history: std::collections::VecDeque<f32>,
    readiness: bool,
    /// Résultats du dernier « APK Readiness Check » (vide tant qu'on n'a pas analysé).
    readiness_results: Vec<readiness::Check>,
    /// Fenêtre « Paramètres » (clé API…).
    settings: bool,
    /// Fenêtre « Générer une scène (IA) ».
    ai_scene: bool,
    /// Fenêtre « Optimisation mobile ».
    optimize: bool,
    /// Fenêtre « Gestionnaire d'assets ».
    assets: bool,
    /// Fenêtre « Gestionnaire de scripts Lua ».
    scripts: bool,
    /// Fenêtre « Multijoueur » (connexion à un serveur RusteeGear).
    multiplayer: bool,
    /// HUD de jeu masqué (bascule Select à la manette, cf.
    /// `Editor::toggle_play_hud`) : cache les widgets de Play (vague, arme,
    /// frags, roster) pour une capture propre — le feedback vital (vignette de
    /// dégâts, barre de vie) reste affiché, on masque l'information, jamais
    /// l'alerte.
    hud_hidden: bool,
    /// Fenêtre « 🧩 Widgets HUD » (édition de `Scene::hud_widgets`).
    hud_widgets_editor: bool,
    /// Fenêtre « 🩹 Journal de crash » (Sprint 113) — ouverte automatiquement au
    /// lancement si `crash_log::read()` a trouvé une trace, sinon accessible depuis
    /// le menu Aide. Écran **volontaire** : rien n'est envoyé nulle part depuis ici,
    /// juste consultation/copie/suppression locale (cf. doc de `crash_log`).
    crash_log: bool,
    /// Fenêtre « Nouveau projet » guidée (Sprint 113d) : choix d'un template
    /// (scène vide / démo contrôleur / niveau de combat) plutôt que de partir
    /// directement d'une scène nue.
    new_project_wizard: bool,
    /// Fenêtre « Ajouter un objet » simplifiée (Sprint 113d) : cartes avec icône
    /// pour les actions les plus courantes du menu Ajouter, en avant-plan plutôt
    /// que dans un sous-menu.
    add_object_cards: bool,
    /// Complément prefabs (validation + portée) : nom de scène/projet tapé par
    /// l'utilisateur, partagé entre le bouton « Créer un prefab » de l'Inspecteur et
    /// la section « 📁 Prefabs de cette scène » du navigateur d'assets — vide =
    /// portée générale. Ce moteur n'a pas de notion de « projet » séparée d'une
    /// scène : un seul champ sert aux deux usages.
    prefab_scope_name: String,
    /// Message à afficher dans le popup de confirmation après un clic sur
    /// « Créer un prefab » (`Ok(nom)` ou `Err(message)`) — posé par l'appelant
    /// (`gfx::renderer`) une fois `UiActions::save_as_prefab` traité, lu et effacé
    /// par le popup lui-même au clic sur OK.
    prefab_feedback: Option<Result<String, String>>,
    /// Suppression de prefab en attente de confirmation (portée, nom affiché) —
    /// posé par le bouton 🗑 de la liste, lu par le popup de confirmation, jamais
    /// appliqué directement au clic (destructif : toujours un aller-retour de
    /// validation, cf. demande utilisateur).
    prefab_pending_delete: Option<(crate::assets::PrefabScope, String)>,
}

/// Informations de diagnostic affichées dans le bandeau d'état (lecture seule).
pub struct StatusInfo<'a> {
    pub fps: f32,
    pub backend: &'a str,
    /// Une génération de script par IA est en cours.
    pub ai_busy: bool,
    /// La grille de référence est-elle affichée ?
    pub grid: bool,
    /// L'aimantation (snap) est-elle active ?
    pub snap: bool,
    /// Vue de debug active : Éclairé/Normales/Profondeur.
    pub debug_view: crate::app::DebugView,
    /// Durée (ms) de chaque passe GPU mesurée à la dernière frame profilée
    /// (Sprint 112, `Renderer::gpu_profiler_info`) — vide si le panneau Profiler
    /// n'a jamais été ouvert, ou si l'adaptateur ne supporte pas les timestamp
    /// queries (`Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`).
    pub gpu_pass_timings_ms: &'a [(&'static str, f32)],
    /// Estimation du nombre de draw calls de la dernière frame — cf. doc de
    /// `Renderer::last_frame_draw_calls`.
    pub gpu_draw_calls: u32,
}

/// Actions demandées par l'UI durant une frame, à traiter par l'appelant.
#[derive(Default)]
pub struct UiActions {
    pub save: bool,
    pub load: bool,
    /// « Enregistrer sous » : chemin JSON choisi.
    pub save_path: Option<String>,
    /// « Ouvrir » : chemin JSON choisi.
    pub load_path: Option<String>,
    pub import: Option<String>,
    pub add: Option<MeshKind>,
    pub delete: Option<usize>,
    pub duplicate: bool,
    pub undo: bool,
    pub redo: bool,
    /// « ⏭ » : avance d'exactement un pas fixe pendant la pause.
    pub step_frame: bool,
    /// Commande saisie dans la console, à exécuter via
    /// `AppState::run_console_command`.
    pub console_command: Option<String>,
    /// « Nouveau projet » : vide la scène.
    pub new_scene: bool,
    /// « Démo mobile » : charge une scène jouable (joystick + saut).
    pub load_demo: bool,
    /// « Démo gameplay » : scène complète (gyro/zone/vie/tap).
    pub load_gameplay: bool,
    /// « Démo contrôleur » : joueur pilotable au joystick + saut, sans script.
    pub load_controller: bool,
    /// « Démo Tour d'ascension » : platforming vertical, sans combat.
    pub load_tower: bool,
    /// « Démo Course infinie » (style Temple Run) : course auto + voies + obstacles.
    pub load_temple_run: bool,
    /// « Scène exemple » (composants optionnels) : référence minimale, pas un niveau.
    pub load_components_demo: bool,
    /// « Démo Vagues de zombies » : jeu local vs ordinateur, manches de monstres.
    pub load_ai_duel: bool,
    /// « Démo MMORPG » : arène minimale dédiée au test multijoueur PC ↔ mobile.
    pub load_mmorpg: bool,
    /// « Démo Donjon (roguelike) » : 3 salles à vider une à une, arme de départ aléatoire.
    pub load_roguelike: bool,
    /// « Démo Duel (Tekken/Smash) » : arène flottante, rival à plusieurs PV, ring out.
    pub load_brawl: bool,
    /// Bouton « Rejouer » de fin de partie (relance la partie en cours).
    pub restart: bool,
    /// Inventaire d'armes (cf. `weapon_inventory_panel`) : arme choisie par le
    /// joueur, à appliquer via `AppState::select_weapon`.
    pub select_weapon: Option<usize>,
    /// « Utiliser » cliqué dans le panneau 👜 Sac : sorte d'objet à consommer,
    /// à appliquer via `AppState::use_item`.
    pub use_item: Option<crate::scene::ItemKind>,
    /// Fenêtre Multijoueur : « Se connecter » demandé (adresse, pseudo).
    pub connect_to_server: Option<(String, String)>,
    /// Fenêtre Multijoueur : « Se déconnecter » demandé.
    pub disconnect_from_server: bool,
    /// Widgets HUD `Button` cliqués ce frame (leur champ `action`) — à transmettre à
    /// `AppState::push_hud_event` (délivrés aux scripts via `on_event("hud:<action>")`).
    pub hud_clicks: Vec<String>,
    /// Fenêtre Multijoueur : « Se connecter (compte) » Firebase demandé (email, mot de passe).
    pub firebase_sign_in: Option<(String, String)>,
    /// Fenêtre Multijoueur : « Créer un compte » Firebase demandé (email, mot de passe).
    pub firebase_sign_up: Option<(String, String)>,
    /// Fenêtre Multijoueur : « Envoyer » un message de chat (salon, pseudo, texte).
    pub send_chat_message: Option<(String, String, String)>,
    /// Fenêtre Multijoueur : « Rafraîchir » le chat demandé (salon).
    pub refresh_chat: Option<String>,
    /// Fenêtre Multijoueur : « Rafraîchir le classement » demandé.
    pub refresh_leaderboard: bool,
    /// « Aligner au sol » : pose la base de la sélection sur y = 0.
    pub align_ground: bool,
    /// « Réinitialiser transform » : remet rotation/échelle par défaut.
    pub reset_transform: bool,
    /// « Créer un prefab depuis la sélection » (Sprint 96, câblage UI) : sauvegarde
    /// l'objet sélectionné, dans la portée choisie (générale ou scène nommée).
    pub save_as_prefab: Option<crate::assets::PrefabScope>,
    /// Navigateur d'assets, section Prefabs : « Instancier » un prefab (référence
    /// stable `asset-id://<uuid>`).
    pub instantiate_prefab: Option<String>,
    /// « Resynchroniser les instances de prefab » : réapplique chaque template aux
    /// instances liées de la scène (`Scene::sync_prefab_instances`).
    pub sync_prefab_instances: bool,
    /// Suppression de prefab confirmée (portée, nom sans `.json`) — complément
    /// validation : posé seulement après confirmation dans le popup dédié.
    pub delete_prefab: Option<(crate::assets::PrefabScope, String)>,
    /// « Quitter » : ferme l'application.
    pub quit: bool,
    pub play_audio: Option<String>,
    /// Fenêtre Paramètres : volume musique/ambiance changé (Sprint 104).
    pub music_volume: Option<f32>,
    /// Fenêtre Paramètres : volume effets sonores changé (Sprint 104).
    pub sfx_volume: Option<f32>,
    /// Fenêtre Paramètres : langue du texte runtime changée (Sprint 130).
    pub locale: Option<crate::app::locale::Locale>,
    /// Réordonnancement de l'objet sélectionné : `Some(true)` = descendre, `Some(false)` = monter.
    pub move_in_list: Option<bool>,
    /// Réordonnancement par glisser-déposer dans la hiérarchie : `(index source, index cible)`.
    pub reorder: Option<(usize, usize)>,
    /// Clic sur un objet/une lumière dans la hiérarchie : recentrer la caméra
    /// dessus (même effet que la touche F), pour voir immédiatement ce qu'on
    /// vient de sélectionner sans chercher à l'écran.
    pub focus_selection: bool,
    /// Toolbar « Run Device » : ouvrir le panneau de build et installer sur l'appareil.
    pub run_device: bool,
    /// Menu « Paramètres projet » : ouvrir aussi la fenêtre Paramètres (clé IA…).
    pub open_settings: bool,
    /// Génération IA d'un script : `(index objet, requête DeepSeek)`.
    pub ai_generate: Option<(usize, crate::app::ai::AiRequest)>,
    /// Génération IA d'une scène : `(requête, remplacer ?)` (sinon ajout à la scène).
    pub ai_generate_scene: Option<(crate::app::ai::AiRequest, bool)>,
    /// Demande d'ouverture de la fenêtre « Générer une scène (IA) ».
    pub open_ai_scene: bool,
    /// Demande d'ouverture de la fenêtre « Nouveau projet » guidée (Sprint 113d).
    pub open_new_project_wizard: bool,
    /// Demande d'ouverture de la fenêtre « Ajouter un objet » simplifiée (Sprint 113d).
    pub open_add_object_cards: bool,
    /// Définir la caméra de jeu sur la vue actuelle.
    pub set_game_camera: bool,
    /// Retirer la caméra de jeu.
    pub clear_game_camera: bool,
    /// Optimisation mobile : réduire les textures au-delà de N px.
    pub optimize_textures: Option<u32>,
    /// Optimisation mobile : limiter le nombre de lumières ponctuelles.
    pub limit_lights: Option<usize>,
    /// Convertir les textures aux puissances de 2 (compression GPU mobile).
    pub convert_textures_pot: bool,
    /// Bake lighting : figer les lumières ponctuelles en émission statique.
    pub bake_lighting: bool,
    /// Mode performance Android : optimisations groupées (textures + lumières).
    pub perf_mode: bool,
    /// Préset qualité par plateforme (Sprint 126) — généralisation de `perf_mode`
    /// en plusieurs niveaux nommés (cf. `app::asset_ops::QualityPreset`).
    pub apply_quality_preset: Option<crate::app::asset_ops::QualityPreset>,
    /// Rassembler les assets externes dans le dossier projet (asset://).
    pub collect_assets: bool,
    /// Édition : couper / copier / coller / tout sélectionner / grouper / dégrouper.
    pub cut: bool,
    pub copy: bool,
    pub paste: bool,
    pub select_all: bool,
    pub group: bool,
    pub ungroup: bool,
    /// Aligner la sélection sur la primaire le long d'un axe (0=X, 1=Y, 2=Z).
    pub align_axis: Option<usize>,
    /// Distribuer la sélection à intervalles égaux le long d'un axe.
    pub distribute_axis: Option<usize>,
    /// Basculer l'affichage de la grille de référence.
    pub toggle_grid: bool,
    /// Basculer l'aimantation (snap) au déplacement.
    pub toggle_snap: bool,
    /// Nouvelle vue de debug demandée : Éclairé/Normales/Profondeur.
    pub set_debug_view: Option<crate::app::DebugView>,
}

impl Editor {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();
        let winit_state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            device,
            color_format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );
        // Ouvre automatiquement le journal de crash s'il y en a un à consulter —
        // l'utilisateur n'a pas à savoir qu'un menu Aide existe pour le trouver.
        let crash_log_text = crate::crash_log::read();
        let panels = Panels {
            crash_log: crash_log_text.is_some(),
            ..Default::default()
        };

        Editor {
            ctx,
            winit_state,
            renderer,
            export: export::ExportPanel::new(),
            hier_filter: String::new(),
            hier_new_group: String::new(),
            hier_rename: None,
            panels,
            settings: crate::app::settings::Settings::load(),
            ai_prompt: String::new(),
            ai_scene_prompt: String::new(),
            ai_scene_replace: true,
            ai_history: Vec::new(),
            mp_server_url: crate::app::network_client::DEFAULT_SERVER_URL.to_string(),
            mp_name: String::new(),
            mp_email: String::new(),
            mp_password: String::new(),
            mp_lobby_code: "default".to_string(),
            mp_chat_input: String::new(),
            console_input: String::new(),
            hud_preview: HudPreview::default(),
            hud_widget_new_id: String::new(),
            hud_image_cache: HudImageCache::default(),
            crash_log_text,
        }
    }

    /// Réglages courants (clé API DeepSeek, config Firebase…) — lecture seule,
    /// pour les appelants (ex. `Renderer`) qui doivent construire une requête
    /// à partir de la config sans dupliquer l'état.
    pub fn settings(&self) -> &crate::app::settings::Settings {
        &self.settings
    }

    /// Ouvre/ferme la fenêtre Multijoueur — bouton Start de la manette
    /// (`App::recompute_action_buttons`), même fenêtre que le menu Outils.
    pub fn toggle_multiplayer_window(&mut self) {
        self.panels.multiplayer = !self.panels.multiplayer;
    }

    /// Masque/affiche les widgets HUD de Play — bouton Select de la manette.
    /// Cf. `Panels::hud_hidden` : l'alerte vitale (vignette de dégâts, barre
    /// de vie) n'est jamais masquée.
    pub fn toggle_play_hud(&mut self) {
        self.panels.hud_hidden = !self.panels.hud_hidden;
    }

    /// Panneau « 📊 Profiler FPS » ouvert ? Lu par `Renderer::render` (Sprint 112)
    /// pour ne payer le coût des timestamp queries GPU que quand ce panneau est
    /// visible — même logique que `fps_history`, qui ne s'accumule aussi que là.
    pub fn profiler_open(&self) -> bool {
        self.panels.profiler
    }

    /// Pose le résultat de `AppState::save_selected_as_prefab` (succès ou échec), lu
    /// la frame suivante par le popup de validation (`windows::prefab_feedback_popup`)
    /// — posé par l'appelant (`gfx::renderer`) juste après avoir traité
    /// `UiActions::save_as_prefab`.
    pub fn set_prefab_feedback(&mut self, result: Result<String, String>) {
        self.panels.prefab_feedback = Some(result);
    }

    /// Mode Player : dessine **uniquement** les contrôles tactiles en surimpression
    /// (pas de panneaux d'éditeur) et met à jour l'état d'entrée lu par les scripts.
    /// Ajoute aussi un petit overlay Multijoueur (adresse + pseudo +
    /// connecter/déconnecter) repliable, pour rejoindre un serveur RusteeGear
    /// depuis un APK — les actions demandées sont renvoyées dans un
    /// `UiActions` (même convention que `run`) pour être traitées par l'appelant.
    #[allow(clippy::too_many_arguments)] // états distincts à passer à l'overlay
    pub fn run_player_overlay(
        &mut self,
        window: &Window,
        scene: &Scene,
        input_state: &mut crate::app::PlayerInput,
        device_preview: bool,
        device_portrait: bool,
        hud_health: Option<f32>,
        damage_flash: f32,
        ally_down_flash: f32,
        game_time: Option<f32>,
        score: u32,
        lost: bool,
        won: bool,
        wave: u32,
        restart: &mut bool,
        net_status: &str,
        net_connected: bool,
        weapon_label: &str,
        defeated: bool,
        kills: u32,
        weapon_inventory: &[(&str, [f32; 3])],
        selected_weapon: usize,
        item_inventory: &[(crate::scene::ItemKind, u32)],
        roster: &[RosterEntry],
        locale: crate::app::locale::Locale,
    ) -> (egui::FullOutput, UiActions) {
        let raw_input = self.winit_state.take_egui_input(window);
        let mobile = &scene.mobile;
        let mut actions = UiActions::default();
        let mp_server_url = &mut self.mp_server_url;
        let mp_name = &mut self.mp_name;
        let output = self.ctx.run_ui(raw_input, |ui| {
            let ctx = ui.ctx();
            let area = play_area_rect(ctx.content_rect(), device_preview, device_portrait);
            if device_preview {
                device_bezel(ctx, area);
                touch_feedback(ctx, area);
            }
            if damage_flash > 0.0 {
                damage_vignette(ctx, area, damage_flash);
            }
            if ally_down_flash > 0.0 {
                ally_down_banner(ctx, area, ally_down_flash, locale);
            }
            if let Some(h) = hud_health.or_else(|| mobile.health_bar.then_some(1.0)) {
                health_bar(ctx, area, h);
            }
            // Décalages persistés dans la scène (Scene::hud_layout) : pas de
            // glisser possible ici (`draggable: false`), l'overlay mobile autonome n'a
            // pas de panneau 👁 Aperçu HUD — copies locales, `scene` n'est pas `&mut`.
            let mut layout = scene.hud_layout;
            wave_hud(ctx, area, scene, wave, locale);
            weapon_hud(
                ctx,
                area,
                weapon_label,
                &mut layout.weapon_hud,
                false,
                locale,
            );
            // Frags (GAMEDESIGN_EN_LIGNE.md, brique de progression MMORPG) : toujours
            // affiché en Play, contrairement au score de `collectibles_hud` juste en
            // dessous, qui ne s'affiche que si la scène a des collectibles (la carte
            // multijoueur n'en a pas — cf. docs/audits/editor.md pour l'absence de
            // score en ligne que ce HUD dédié corrige).
            kills_hud(ctx, area, kills, &mut layout.kills, false, locale);
            multiplayer_roster_panel(ctx, area, roster, &mut layout.roster, false, locale);
            if scene_has_ranged_weapon(scene) {
                crosshair(ctx, area, &mut layout.crosshair, false);
                weapon_inventory_panel(
                    ctx,
                    area,
                    weapon_inventory,
                    selected_weapon,
                    &mut layout.weapon_inventory,
                    false,
                    &mut actions,
                    locale,
                );
            }
            item_inventory_panel(
                ctx,
                area,
                item_inventory,
                &mut layout.item_inventory,
                false,
                &mut actions,
            );
            if let Some((c, t)) = scene.collectibles() {
                collectibles_hud(ctx, area, c, t, game_time, score, locale);
            }
            if lost {
                lose_banner(ctx, area, locale);
            } else if defeated {
                defeated_banner(ctx, area, locale);
            }
            // Fin de partie (gagné/perdu) : bouton « Rejouer » in-game (essentiel sur APK).
            if (won || lost) && restart_button(ctx, area, won, locale) {
                *restart = true;
            }
            if mobile.any() {
                mobile_overlay(ctx, area, mobile, input_state);
            } else {
                input_state.joy = (0.0, 0.0);
                input_state.touch_thrust = 0.0;
                input_state.touch_turn = 0.0;
                input_state.buttons.clear();
            }
            mobile_multiplayer_overlay(
                ctx,
                mp_server_url,
                mp_name,
                net_status,
                net_connected,
                &mut actions,
            );
            let values = HudWidgetValues {
                health: hud_health.unwrap_or(1.0),
                score,
                kills,
                wave,
            };
            actions.hud_clicks = hud_widgets(ctx, area, scene, &values, &mut self.hud_image_cache);
        });
        self.winit_state
            .handle_platform_output(window, output.platform_output.clone());
        (output, actions)
    }

    /// Transmet l'événement à egui. Retourne `true` si egui l'a consommé.
    pub fn on_window_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.winit_state.on_window_event(window, event).consumed
    }

    /// Construit l'UI (mutant la scène et la sélection) et renvoie la sortie egui à peindre.
    #[allow(clippy::too_many_arguments)] // états distincts à muter
    pub fn run(
        &mut self,
        window: &Window,
        scene: &mut Scene,
        selection: &mut Option<usize>,
        selected: &mut Vec<usize>,
        selected_light: &mut Option<usize>,
        playing: &mut bool,
        paused: &mut bool,
        // Multiplicateur du temps simulé — voir `AppState::time_scale`.
        time_scale: &mut f32,
        gizmo_mode: &mut GizmoMode,
        input_state: &mut crate::app::PlayerInput,
        device_preview: &mut bool,
        device_portrait: &mut bool,
        view_rect: &mut (f32, f32, f32, f32),
        hud_health: Option<f32>,
        damage_flash: f32,
        ally_down_flash: f32,
        game_time: Option<f32>,
        score: u32,
        lost: bool,
        won: bool,
        wave: u32,
        status: StatusInfo,
        net_status: &str,
        net_connected: bool,
        chat_messages: &[crate::app::network_client::ChatLine],
        has_firebase_account: bool,
        leaderboard: &[crate::app::network_client::LeaderboardLine],
        weapon_label: &str,
        defeated: bool,
        kills: u32,
        weapon_inventory: &[(&str, [f32; 3])],
        selected_weapon: usize,
        item_inventory: &[(crate::scene::ItemKind, u32)],
        roster: &[RosterEntry],
        locale: crate::app::locale::Locale,
    ) -> (egui::FullOutput, UiActions) {
        let raw_input = self.winit_state.take_egui_input(window);
        let mut actions = UiActions::default();

        let export = &mut self.export;
        let hier_filter = &mut self.hier_filter;
        let hier_new_group = &mut self.hier_new_group;
        let hier_rename = &mut self.hier_rename;
        let panels = &mut self.panels;
        let settings = &mut self.settings;
        let ai_prompt = &mut self.ai_prompt;
        let ai_scene_prompt = &mut self.ai_scene_prompt;
        let ai_scene_replace = &mut self.ai_scene_replace;
        let ai_history = &mut self.ai_history;
        let mp_server_url = &mut self.mp_server_url;
        let mp_name = &mut self.mp_name;
        let mp_email = &mut self.mp_email;
        let mp_password = &mut self.mp_password;
        let mp_lobby_code = &mut self.mp_lobby_code;
        let mp_chat_input = &mut self.mp_chat_input;
        let console_input = &mut self.console_input;
        let hud_preview = &mut self.hud_preview;
        let hud_image_cache = &mut self.hud_image_cache;
        let hud_widget_new_id = &mut self.hud_widget_new_id;
        let crash_log_text = &mut self.crash_log_text;
        let output = self.ctx.run_ui(raw_input, |ui| {
            build_ui(
                ui,
                scene,
                selection,
                selected,
                selected_light,
                playing,
                paused,
                time_scale,
                gizmo_mode,
                input_state,
                device_preview,
                device_portrait,
                view_rect,
                hud_health,
                damage_flash,
                ally_down_flash,
                game_time,
                score,
                lost,
                won,
                wave,
                &status,
                export,
                hier_filter,
                hier_new_group,
                hier_rename,
                panels,
                settings,
                ai_prompt,
                ai_scene_prompt,
                ai_scene_replace,
                ai_history,
                mp_server_url,
                mp_name,
                mp_email,
                mp_password,
                mp_lobby_code,
                mp_chat_input,
                console_input,
                net_status,
                net_connected,
                chat_messages,
                has_firebase_account,
                leaderboard,
                weapon_label,
                defeated,
                kills,
                weapon_inventory,
                selected_weapon,
                item_inventory,
                roster,
                hud_preview,
                hud_image_cache,
                hud_widget_new_id,
                crash_log_text,
                &mut actions,
                locale,
            );
        });

        self.winit_state
            .handle_platform_output(window, output.platform_output.clone());
        (output, actions)
    }

    /// Peint l'UI egui dans `view`. Renvoie d'éventuels command buffers à soumettre avant l'encodeur.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size_in_pixels: [u32; 2],
        output: egui::FullOutput,
    ) -> Vec<wgpu::CommandBuffer> {
        let ppp = output.pixels_per_point;
        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }
        let primitives = self.ctx.tessellate(output.shapes, ppp);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels,
            pixels_per_point: ppp,
        };
        let cmds = self
            .renderer
            .update_buffers(device, queue, encoder, &primitives, &screen);

        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &primitives, &screen);
        }

        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        cmds
    }
}

#[allow(clippy::too_many_arguments)] // panneau d'UI : chaque paramètre est un état distinct à muter
fn build_ui(
    root: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
    selected_light: &mut Option<usize>,
    playing: &mut bool,
    paused: &mut bool,
    time_scale: &mut f32,
    gizmo_mode: &mut GizmoMode,
    input_state: &mut crate::app::PlayerInput,
    device_preview: &mut bool,
    device_portrait: &mut bool,
    view_rect: &mut (f32, f32, f32, f32),
    hud_health: Option<f32>,
    damage_flash: f32,
    ally_down_flash: f32,
    game_time: Option<f32>,
    score: u32,
    lost: bool,
    won: bool,
    wave: u32,
    status: &StatusInfo,
    export: &mut export::ExportPanel,
    hier_filter: &mut String,
    hier_new_group: &mut String,
    hier_rename: &mut Option<(usize, String)>,
    panels: &mut Panels,
    settings: &mut crate::app::settings::Settings,
    ai_prompt: &mut String,
    ai_scene_prompt: &mut String,
    ai_scene_replace: &mut bool,
    ai_history: &mut Vec<String>,
    mp_server_url: &mut String,
    mp_name: &mut String,
    mp_email: &mut String,
    mp_password: &mut String,
    mp_lobby_code: &mut String,
    mp_chat_input: &mut String,
    console_input: &mut String,
    net_status: &str,
    net_connected: bool,
    chat_messages: &[crate::app::network_client::ChatLine],
    has_firebase_account: bool,
    leaderboard: &[crate::app::network_client::LeaderboardLine],
    weapon_label: &str,
    defeated: bool,
    kills: u32,
    weapon_inventory: &[(&str, [f32; 3])],
    selected_weapon: usize,
    item_inventory: &[(crate::scene::ItemKind, u32)],
    roster: &[RosterEntry],
    hud_preview: &mut HudPreview,
    hud_image_cache: &mut HudImageCache,
    hud_widget_new_id: &mut String,
    crash_log_text: &mut Option<String>,
    actions: &mut UiActions,
    locale: crate::app::locale::Locale,
) {
    // Fenêtre « Paramètres » (clé API DeepSeek…).
    settings_window(root.ctx(), panels, settings, actions);
    // Fenêtre « 👁 Aperçu HUD » : prévisualiser les overlays de jeu en Édition.
    hud_preview_window(root.ctx(), hud_preview);
    // Fenêtre « 🧩 Widgets HUD » : ajouter/éditer les widgets déclaratifs de la scène.
    windows::hud_widgets_window(root.ctx(), panels, scene, hud_widget_new_id);
    // Fenêtre « 🩹 Journal de crash » (Sprint 113) : écran volontaire de consultation.
    windows::crash_log_window(root.ctx(), panels, crash_log_text);
    // Fenêtre « Multijoueur » (connexion à un serveur RusteeGear).
    multiplayer_window(
        root.ctx(),
        panels,
        mp_server_url,
        mp_name,
        mp_email,
        mp_password,
        mp_lobby_code,
        mp_chat_input,
        settings,
        net_status,
        net_connected,
        chat_messages,
        has_firebase_account,
        leaderboard,
        actions,
    );
    // Fenêtre « Générer une scène (IA) ».
    ai_scene_window(
        root.ctx(),
        panels,
        settings,
        ai_scene_prompt,
        ai_scene_replace,
        ai_history,
        status,
        actions,
    );
    // Fenêtre « Nouveau projet » guidée (Sprint 113d).
    windows::new_project_wizard_window(root.ctx(), panels, actions);
    // Fenêtre « Ajouter un objet » simplifiée (Sprint 113d).
    windows::add_object_cards_window(root.ctx(), panels, scene, actions);
    optimize_window(root.ctx(), panels, scene, actions);
    asset_browser_window(root.ctx(), panels, scene, *selection, actions);
    scripts_window(root.ctx(), panels, scene, selection, selected);
    // Complément prefabs : popup de validation après création + confirmation avant
    // suppression (demande utilisateur — un aller-retour explicite pour les deux,
    // pas une action silencieuse).
    windows::prefab_feedback_popup(root.ctx(), panels);
    windows::prefab_delete_confirm_popup(root.ctx(), panels, actions);

    // Fenêtre flottante « Build & Export ».
    export.ui(root.ctx(), scene);
    // Fenêtres des menus « Aide » et « Outils » (raccourcis, diagnostic, console, profiler, qualité APK).
    tool_windows(
        root.ctx(),
        panels,
        scene,
        export,
        status,
        console_input,
        actions,
    );

    // Bandeau d'état (bas) : FPS, nombre d'objets, mode, backend GPU.
    egui::Panel::bottom("status_bar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("⏱ {:.0} FPS", status.fps));
            ui.separator();
            ui.label(format!("🧊 {} objets", scene.objects.len()));
            ui.separator();
            ui.label(match (*playing, *paused) {
                (true, true) => "⏸ Pause",
                (true, false) => "▶ Play",
                _ => "✎ Edit",
            });
            ui.separator();
            ui.label(format!("GPU : {}", status.backend));
        });
    });

    // --- Barre de menus (style application de bureau) ---
    egui::Panel::top("menubar").show_inside(root, |ui| {
        ui.horizontal(|ui| {
            menu_fichier(ui, export, actions);
            menu_edition(ui, selection, actions);
            menu_ajouter(ui, scene, *selection, actions);
            menu_outils(ui, gizmo_mode, export, panels);
            menu_aide(ui, panels);
        });
    });
    // Ouverture différée de la fenêtre « Générer une scène (IA) » (demandée par le menu).
    if actions.open_ai_scene {
        panels.ai_scene = true;
    }
    // Ouverture différée des fenêtres du Sprint 113d (assistants pour non-développeur).
    if actions.open_new_project_wizard {
        panels.new_project_wizard = true;
    }
    if actions.open_add_object_cards {
        panels.add_object_cards = true;
    }
    // « Run Device » (toolbar) : build Android + installation sur le téléphone branché.
    if actions.run_device {
        export.run_on_device(scene);
    }
    // « Paramètres projet » (menu Fichier) : ouvre aussi la fenêtre Paramètres.
    if actions.open_settings {
        panels.settings = true;
    }

    // --- Barre d'outils rapide (passe à la ligne si la fenêtre est étroite) ---
    egui::Panel::top("toolbar").show_inside(root, |ui| {
        ui.horizontal_wrapped(|ui| {
            // Play / Pause / Stop distincts (style lecteur).
            if !*playing {
                if ui.button("▶ Play").clicked() {
                    *playing = true;
                    *paused = false;
                }
            } else {
                let pause_label = if *paused {
                    "▶ Reprendre"
                } else {
                    "⏸ Pause"
                };
                if ui.button(pause_label).clicked() {
                    *paused = !*paused;
                }
                if ui.button("⏹ Stop").clicked() {
                    *playing = false;
                    *paused = false;
                }
                // Pas unique : n'a de sens qu'en pause.
                if ui
                    .add_enabled(*paused, egui::Button::new("⏭"))
                    .on_hover_text("Avancer d'un pas fixe")
                    .clicked()
                {
                    actions.step_frame = true;
                }
            }
            ui.separator();
            // Time scale : ralenti/accélère la simulation pour déboguer la
            // physique et le réseau. Préréglages + valeur affichée plutôt qu'un slider :
            // les valeurs qui comptent en pratique sont peu nombreuses (figé/ralenti/normal/rapide).
            ui.label("⏱");
            for (label, value) in [("¼×", 0.25), ("½×", 0.5), ("1×", 1.0), ("2×", 2.0)] {
                if ui.selectable_label(*time_scale == value, label).clicked() {
                    *time_scale = value;
                }
            }
            ui.separator();
            // Outils en icônes seules (les noms sont au survol et dans le menu
            // Outils), dans l'ordre des raccourcis Q W E R T Y. Icônes choisies
            // dans la couverture réelle des polices d'egui — 🖐 et ⤢ n'y sont
            // pas et rendaient des carrés (tofu).
            ui.selectable_value(gizmo_mode, GizmoMode::Pan, "✋")
                .on_hover_text(
                    "Main (Q) : glisser = déplacer la vue — aussi : clic milieu ou Maj+glisser",
                );
            ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔")
                .on_hover_text("Déplacer l'objet (W)");
            ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻")
                .on_hover_text("Tourner l'objet (E)");
            ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⛶")
                .on_hover_text("Redimensionner l'objet (R)");
            ui.selectable_value(gizmo_mode, GizmoMode::Orbit, "🔄")
                .on_hover_text(
                    "Orbite libre (T) : glisser = tourner la vue (horizontal et vertical)",
                );
            ui.selectable_value(gizmo_mode, GizmoMode::Zoom, "🔍")
                .on_hover_text("Loupe (Y) : glisser haut/bas = zoom avant/arrière");
            if ui
                .add_enabled(selection.is_some(), egui::Button::new("⌖"))
                .on_hover_text("Cadrer la sélection (F)")
                .clicked()
            {
                actions.focus_selection = true;
            }
            ui.separator();
            if ui.button("↩").on_hover_text("Annuler (Cmd+Z)").clicked() {
                actions.undo = true;
            }
            if ui
                .button("↪")
                .on_hover_text("Rétablir (Cmd+Maj+Z)")
                .clicked()
            {
                actions.redo = true;
            }
            ui.separator();
            if ui.button("💾").on_hover_text("Enregistrer").clicked() {
                actions.save = true;
            }
            ui.separator();
            let has_sel = selection.is_some();
            if ui
                .add_enabled(has_sel, egui::Button::new("⧉"))
                .on_hover_text("Dupliquer (Cmd+D)")
                .clicked()
            {
                actions.duplicate = true;
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("🗑"))
                .on_hover_text("Supprimer")
                .clicked()
            {
                actions.delete = *selection;
            }
            ui.separator();
            // Aperçu mobile : cadre téléphone + orientation.
            if ui
                .selectable_label(*device_preview, "📱 Aperçu mobile")
                .on_hover_text("Affiche la scène dans un écran de téléphone (mode jeu tactile)")
                .clicked()
            {
                *device_preview = !*device_preview;
                if *device_preview {
                    // On passe en « mode jeu » : pas d'objet sélectionné/gizmo.
                    *selection = None;
                    selected.clear();
                }
            }
            if *device_preview {
                let label = if *device_portrait {
                    "⟳ Portrait"
                } else {
                    "⟳ Paysage"
                };
                if ui
                    .button(label)
                    .on_hover_text("Basculer l'orientation")
                    .clicked()
                {
                    *device_portrait = !*device_portrait;
                }
            }
            // Aperçu HUD : voir réticule/inventaire/joueurs en Édition,
            // sans passer par Play (cf. `HudPreview`).
            if ui
                .selectable_label(hud_preview.open, "👁 Aperçu HUD")
                .on_hover_text(
                    "Prévisualise les overlays de jeu (réticule, inventaire, joueurs…) \
                     en Édition, sans lancer Play",
                )
                .clicked()
            {
                hud_preview.open = !hud_preview.open;
            }
            // Widgets HUD déclaratifs (`Scene::hud_widgets`) : texte/image/jauge/
            // bouton ancrés dans la scène — cf. Sprint 109.
            if ui
                .selectable_label(panels.hud_widgets_editor, "🧩 Widgets HUD")
                .on_hover_text(
                    "Ajouter/éditer des widgets HUD déclaratifs (texte, image, jauge, bouton)",
                )
                .clicked()
            {
                panels.hud_widgets_editor = !panels.hud_widgets_editor;
            }
            if ui
                .selectable_label(scene.camera_follow, "🎥 Suivi")
                .on_hover_text("En Play, la caméra suit le joueur (objet scripté)")
                .clicked()
            {
                scene.camera_follow = !scene.camera_follow;
            }
            if ui
                .selectable_label(status.grid, "▦ Grille")
                .on_hover_text("Grille de référence au sol (édition)")
                .clicked()
            {
                actions.toggle_grid = true;
            }
            if ui
                .selectable_label(status.snap, "🧲 Snap")
                .on_hover_text(
                    "Aimanter position (pas 0.5) et rotation (pas 15°) au gizmo — \
                     tenir Ctrl pendant un glissé inverse ponctuellement ce réglage",
                )
                .clicked()
            {
                actions.toggle_snap = true;
            }
            // Vue de debug : remplace l'éclairage par les normales ou la
            // profondeur, pour voir directement ce que le pipeline calcule.
            ui.separator();
            ui.label("👁");
            for view in crate::app::DebugView::ALL {
                if ui
                    .selectable_label(status.debug_view == view, view.label())
                    .clicked()
                {
                    actions.set_debug_view = Some(view);
                }
            }
            // Build APK + Run Device : différenciateurs du moteur (passent à la ligne si étroit).
            ui.separator();
            if ui
                .selectable_label(export.open, "🤖 Build APK")
                .on_hover_text("Build & Export (.dmg / .apk / .ipa)")
                .clicked()
            {
                export.open = !export.open;
            }
            if ui
                .button("📲 Run Device")
                .on_hover_text(
                    "Build Android + installation/lancement sur le téléphone branché (adb)",
                )
                .clicked()
            {
                actions.run_device = true;
            }
        });
    });

    // Mode « focus jeu » : en aperçu mobile, on masque les panneaux latéraux pour
    // laisser toute la place au téléphone (la toolbar reste pour quitter l'aperçu).
    let show_panels = !*device_preview;

    if show_panels {
        egui::Panel::left("hierarchy")
            .default_size(200.0)
            .show_inside(root, |ui| {
                hierarchy_panel(
                    ui,
                    scene,
                    selection,
                    selected,
                    selected_light,
                    hier_filter,
                    hier_new_group,
                    hier_rename,
                    actions,
                );
            });
    }

    if show_panels {
        egui::Panel::right("inspector")
            .default_size(240.0)
            .show_inside(root, |ui| {
                ui.heading("Inspecteur");
                ui.separator();
                // Le contenu (scripts, matériau, audio, composants…) dépasse vite la
                // hauteur de la fenêtre : tout sauf le titre défile verticalement.
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                ui.collapsing("🔆 Éclairage (scène)", |ui| {
                    let l = &mut scene.light;
                    ui.label("Direction");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut l.dir[0]).speed(0.02).prefix("x "));
                        ui.add(egui::DragValue::new(&mut l.dir[1]).speed(0.02).prefix("y "));
                        ui.add(egui::DragValue::new(&mut l.dir[2]).speed(0.02).prefix("z "));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Couleur");
                        ui.color_edit_button_rgb(&mut l.color);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Ambiante");
                        ui.add(egui::Slider::new(&mut l.ambient, 0.0..=1.0));
                    });
                });
                ui.collapsing("🌫 Ciel & brouillard", |ui| {
                    let sky = &mut scene.sky;
                    ui.horizontal(|ui| {
                        ui.label("Ciel — horizon");
                        ui.color_edit_button_rgb(&mut sky.horizon_color);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Ciel — zénith");
                        ui.color_edit_button_rgb(&mut sky.zenith_color);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Brouillard");
                        ui.color_edit_button_rgb(&mut sky.fog_color);
                    });
                    ui.add(
                        egui::Slider::new(&mut sky.fog_density, 0.0..=1.0)
                            .text("densité du brouillard"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sky.bloom_intensity, 0.0..=3.0).text("bloom"),
                    );
                    ui.weak(
                        "Halo autour des zones dont la radiance dépasse 1.0 (émissifs, \
                         spéculaire fort) ; coupé automatiquement en qualité Basse.",
                    );
                });
                if !scene.point_lights.is_empty() {
                    ui.collapsing(
                        format!("💡 Lumières ponctuelles ({})", scene.point_lights.len()),
                        |ui| {
                            let mut remove = None;
                            for (idx, pl) in scene.point_lights.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("#{idx}"));
                                    ui.color_edit_button_rgb(&mut pl.color);
                                    if ui.small_button("🗑").clicked() {
                                        remove = Some(idx);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::DragValue::new(&mut pl.position[0])
                                            .speed(0.05)
                                            .prefix("x "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut pl.position[1])
                                            .speed(0.05)
                                            .prefix("y "),
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut pl.position[2])
                                            .speed(0.05)
                                            .prefix("z "),
                                    );
                                });
                                ui.add(
                                    egui::Slider::new(&mut pl.intensity, 0.0..=5.0)
                                        .text("intensité"),
                                );
                                ui.add(egui::Slider::new(&mut pl.range, 0.5..=30.0).text("portée"));
                                ui.add(
                                    egui::Slider::new(&mut pl.spot_angle, 0.0..=89.0)
                                        .text("cône (0 = point)"),
                                );
                                if pl.spot_angle > 0.0 {
                                    ui.horizontal(|ui| {
                                        ui.label("dir");
                                        ui.add(
                                            egui::DragValue::new(&mut pl.spot_dir[0])
                                                .speed(0.02)
                                                .prefix("x "),
                                        );
                                        ui.add(
                                            egui::DragValue::new(&mut pl.spot_dir[1])
                                                .speed(0.02)
                                                .prefix("y "),
                                        );
                                        ui.add(
                                            egui::DragValue::new(&mut pl.spot_dir[2])
                                                .speed(0.02)
                                                .prefix("z "),
                                        );
                                    });
                                }
                                ui.separator();
                            }
                            if let Some(i) = remove {
                                scene.point_lights.remove(i);
                            }
                        },
                    );
                }
                ui.horizontal(|ui| {
                    if scene.game_camera.is_some() {
                        ui.label("🎥 Caméra de jeu définie");
                        if ui.small_button("✕").on_hover_text("Retirer").clicked() {
                            actions.clear_game_camera = true;
                        }
                        if ui
                            .small_button("⟳")
                            .on_hover_text("Recadrer sur la vue")
                            .clicked()
                        {
                            actions.set_game_camera = true;
                        }
                    } else if ui
                        .button("🎥 Définir la caméra de jeu")
                        .on_hover_text("Fige la vue actuelle comme point de vue de Play")
                        .clicked()
                    {
                        actions.set_game_camera = true;
                    }
                });
                ui.separator();
                match *selection {
                    Some(i) if i < scene.objects.len() => {
                        // Boutons tactiles définis (pour mapper le saut), copiés avant l'emprunt mut.
                        let mobile_buttons = scene.mobile.buttons.clone();
                        // Activer le joystick après l'emprunt mut de l'objet (cf. plus bas).
                        let mut need_joystick = false;
                        let obj = &mut scene.objects[i];
                        ui.horizontal(|ui| {
                            ui.label("Nom");
                            ui.text_edit_singleline(&mut obj.name);
                            if ui.small_button("▲").on_hover_text("Monter").clicked() {
                                actions.move_in_list = Some(false);
                            }
                            if ui.small_button("▼").on_hover_text("Descendre").clicked() {
                                actions.move_in_list = Some(true);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Couleur (teinte)");
                            ui.color_edit_button_rgb(&mut obj.color);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Texture");
                            if ui.button("Choisir…").clicked() {
                                #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("Image", &["png", "jpg", "jpeg"])
                                    .pick_file()
                                {
                                    obj.texture = p.to_string_lossy().into_owned();
                                }
                            }
                            if !obj.texture.is_empty() && ui.button("✕").clicked() {
                                obj.texture.clear();
                            }
                            let t = if obj.texture.is_empty() {
                                "(aucune)".to_string()
                            } else {
                                std::path::Path::new(&obj.texture)
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            };
                            ui.label(t);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Portée du prefab");
                            ui.add(
                                egui::TextEdit::singleline(&mut panels.prefab_scope_name)
                                    .hint_text("Général (vide) ou nom de scène/projet")
                                    .desired_width(160.0),
                            )
                            .on_hover_text(
                                "Vide = prefab général, visible depuis toute scène. Un nom \
                                 (ex. « Mmorpg ») range le prefab à part, propre à cette \
                                 scène/ce projet — même champ que la section « Prefabs de \
                                 cette scène » du navigateur d'assets.",
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .button("🧊 Créer un prefab depuis la sélection")
                                .on_hover_text(
                                    "Enregistre cet objet comme prefab réutilisable, dans la \
                                     portée ci-dessus — cf. navigateur d'assets.",
                                )
                                .clicked()
                            {
                                let scope = if panels.prefab_scope_name.trim().is_empty() {
                                    crate::assets::PrefabScope::General
                                } else {
                                    crate::assets::PrefabScope::Scene(
                                        panels.prefab_scope_name.trim().to_string(),
                                    )
                                };
                                actions.save_as_prefab = Some(scope);
                            }
                            if obj.prefab.is_some()
                                && ui
                                    .button("🔄 Resynchroniser les instances")
                                    .on_hover_text(
                                        "Réapplique le prefab source à toutes ses \
                                         instances de la scène (sauf leurs surcharges).",
                                    )
                                    .clicked()
                            {
                                actions.sync_prefab_instances = true;
                            }
                        });
                        ui.separator();
                        transform_editor(ui, &mut obj.transform);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Physique");
                            ui.selectable_value(&mut obj.physics, PhysicsKind::None, "Aucune");
                            ui.selectable_value(&mut obj.physics, PhysicsKind::Static, "Statique");
                            ui.selectable_value(
                                &mut obj.physics,
                                PhysicsKind::Dynamic,
                                "Dynamique",
                            );
                            ui.selectable_value(
                                &mut obj.physics,
                                PhysicsKind::Kinematic,
                                "Cinématique",
                            )
                            .on_hover_text(
                                "Objet déplacé par script qui collisionne avec le monde : \
                                 il glisse contre les murs, objets fixes et joueurs au \
                                 lieu de les traverser.",
                            );
                        });
                        if obj.physics != PhysicsKind::None {
                            ui.horizontal(|ui| {
                                use crate::runtime::physics::ColliderShape as Cs;
                                ui.label("Collider").on_hover_text(
                                    "Forme invisible utilisée pour les collisions physiques \
                                     (rebonds, blocages) — indépendante du mesh visible affiché.",
                                );
                                ui.selectable_value(&mut obj.collider_shape, Cs::Auto, "Auto")
                                    .on_hover_text(
                                        "Devine la forme la plus proche du mesh (Box pour un \
                                         cube, Sphère pour... une sphère, etc.) — le bon choix \
                                         par défaut, à ne changer qu'en cas de comportement \
                                         physique inattendu.",
                                    );
                                ui.selectable_value(&mut obj.collider_shape, Cs::Box, "Box");
                                ui.selectable_value(&mut obj.collider_shape, Cs::Sphere, "Sphère");
                                ui.selectable_value(&mut obj.collider_shape, Cs::Capsule, "Capsule");
                                // TriMesh/ConvexHull : n'ont de sens que pour un modèle importé
                                // — leur géométrie vient des vertices du glTF, rien de tel
                                // n'existe pour une primitive (Cube/Sphère/...).
                                if matches!(obj.mesh, crate::scene::MeshKind::Imported(_)) {
                                    ui.selectable_value(
                                        &mut obj.collider_shape,
                                        Cs::ConvexHull,
                                        "Enveloppe convexe",
                                    )
                                    .on_hover_text(
                                        "Fidèle à la forme importée, utilisable en dynamique.",
                                    );
                                    ui.selectable_value(&mut obj.collider_shape, Cs::TriMesh, "Silhouette exacte")
                                        .on_hover_text(
                                            "Un triangle par triangle du mesh — décor statique \
                                             uniquement (repli automatique sur Enveloppe convexe \
                                             si l'objet est dynamique).",
                                        );
                                }
                            });
                            ui.checkbox(&mut obj.ccd, "CCD (anti-tunneling)").on_hover_text(
                                "Détection de collision continue — pour un objet rapide et \
                                 fin (missile) qui pourrait sinon traverser un mur mince sans \
                                 jamais entrer en collision. Coûteux : à réserver aux objets \
                                 qui en ont réellement besoin.",
                            );
                            ui.horizontal(|ui| {
                                ui.label("Couches (bits)");
                                ui.add(egui::DragValue::new(&mut obj.collision_layer).hexadecimal(
                                    8, false, true,
                                ))
                                .on_hover_text("Couche(s) que cet objet occupe.");
                                ui.label("Masque");
                                ui.add(egui::DragValue::new(&mut obj.collision_mask).hexadecimal(
                                    8, false, true,
                                ))
                                .on_hover_text(
                                    "Couches avec lesquelles cet objet entre en collision \
                                     (toutes par défaut — 0xFFFFFFFF).",
                                );
                            });
                        }
                        ui.checkbox(&mut obj.tappable, "👆 Tactile (cliquable)")
                            .on_hover_text(
                                "En Play, un tap dessus expose obj.tapped au script (ex. couleur)",
                            );
                        if obj.tappable {
                            ui.horizontal(|ui| {
                                ui.label("Action au tap");
                                use crate::scene::TapAction as Ta;
                                egui::ComboBox::from_id_salt(("tap_action", i))
                                    .selected_text(obj.tap_action.label())
                                    .show_ui(ui, |ui| {
                                        for a in Ta::ALL {
                                            ui.selectable_value(&mut obj.tap_action, a, a.label());
                                        }
                                    });
                            })
                            .response
                            .on_hover_text("Comportement sans script quand on tape l'objet");
                        }
                        // Objet d'inventaire (cf. `ItemPickup`) : la case pose/retire le
                        // composant, la sorte et la quantité ne s'affichent que posé —
                        // même esprit que « Action au tap » juste au-dessus.
                        let mut is_item = obj.item_pickup.is_some();
                        ui.checkbox(&mut is_item, "🧺 Objet à ramasser")
                            .on_hover_text(
                                "En Play, marcher dessus l'ajoute au sac du joueur (panneau 👜)",
                            );
                        if is_item && obj.item_pickup.is_none() {
                            obj.item_pickup = Some(crate::scene::ItemPickup {
                                kind: crate::scene::ItemKind::Potion,
                                count: 1,
                            });
                        } else if !is_item {
                            obj.item_pickup = None;
                        }
                        if let Some(item) = &mut obj.item_pickup {
                            ui.horizontal(|ui| {
                                ui.label("Sorte");
                                egui::ComboBox::from_id_salt(("item_kind", i))
                                    .selected_text(item.kind.label())
                                    .show_ui(ui, |ui| {
                                        for k in crate::scene::ItemKind::ALL {
                                            ui.selectable_value(&mut item.kind, k, k.label());
                                        }
                                    });
                                ui.label("×");
                                ui.add(egui::DragValue::new(&mut item.count).range(1..=99))
                                    .on_hover_text("Quantité ajoutée au sac par ramassage");
                            });
                        }
                        ui.checkbox(&mut obj.deadly, "💀 Zone mortelle")
                            .on_hover_text(
                                "En Play, la partie est perdue si le joueur entre dans son AABB",
                            );
                        ui.checkbox(&mut obj.trigger, "🎯 Zone de déclenchement")
                            .on_hover_text(
                                "En Play, expose obj.triggered au script quand le joueur entre dans sa zone",
                            );
                        ui.separator();
                        ui.collapsing("Matériau", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Métallique");
                                ui.add(egui::Slider::new(&mut obj.metallic, 0.0..=1.0))
                                    .on_hover_text(
                                        "0 = plastique/bois (reflets diffus), 1 = métal pur \
                                         (reflets nets, pas de couleur diffuse). La plupart des \
                                         objets du quotidien sont proches de 0.",
                                    );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Rugosité");
                                ui.add(egui::Slider::new(&mut obj.roughness, 0.04..=1.0))
                                    .on_hover_text(
                                        "Bas = surface lisse et brillante (reflet net, comme du \
                                         verre poli), haut = surface mate mais diffuse la lumière \
                                         (reflet étalé, comme du plâtre).",
                                    );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Émission");
                                ui.add(egui::Slider::new(&mut obj.emissive, 0.0..=3.0))
                                    .on_hover_text(
                                        "0 = l'objet ne brille pas par lui-même (couleur normale, \
                                         éclairée par les lumières de la scène). Au-dessus de 0, \
                                         l'objet émet sa propre lumière (néon, écran, lave).",
                                    );
                            });
                        });
                        ui.separator();
                        ui.collapsing("Audio", |ui| {
                            use crate::scene::AudioSource;
                            let clip = obj
                                .audio
                                .as_ref()
                                .map(|a| a.clip.clone())
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                if ui.button("Choisir un son…").clicked() {
                                    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                                    if let Some(p) = rfd::FileDialog::new()
                                        .add_filter("Audio", &["wav", "ogg", "flac", "mp3"])
                                        .pick_file()
                                    {
                                        obj.audio.get_or_insert_with(AudioSource::default).clip =
                                            p.to_string_lossy().into_owned();
                                    }
                                }
                                if !clip.is_empty() && ui.button("▶ Tester").clicked() {
                                    actions.play_audio = Some(clip.to_string());
                                }
                            });
                            let label = if clip.is_empty() {
                                "(aucun)".to_string()
                            } else {
                                std::path::Path::new(&clip)
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            };
                            ui.label(label);
                            // Autoplay/spatialisation n'ont de sens que si un clip est choisi :
                            // évite de créer un `AudioSource` (donc un badge 🔊) juste en les cochant.
                            if let Some(a) = &mut obj.audio
                                && !a.clip.is_empty()
                            {
                                ui.checkbox(&mut a.autoplay, "Jouer au lancement (Play)");
                                ui.checkbox(&mut a.spatial, "🔊 Spatialisé (volume selon distance)")
                                    .on_hover_text(
                                        "Le volume au lancement décroît avec la distance à la caméra",
                                    );
                            }
                        });
                        ui.separator();
                        ui.collapsing("🧩 Composants mobiles (Android)", |ui| {
                            use crate::scene::Controller;
                            ui.weak("Touch Area : voir « Tactile » ci-dessus.");
                            // `controller` est optionnel (composant) : les checkboxes le créent
                            // à la volée au premier cochage, et le suppriment si plus rien n'y
                            // reste actif (la grande majorité des objets n'en ont pas besoin).
                            let mut has_input =
                                obj.controller.as_ref().is_some_and(|c| c.input);
                            let mut has_gyro = obj.controller.as_ref().is_some_and(|c| c.gyro);
                            if ui
                                .checkbox(&mut has_input, "🕹 Input Receiver (joystick)")
                                .on_hover_text(
                                    "L'objet se déplace avec le joystick en Play (plan X/Z)",
                                )
                                .changed()
                            {
                                obj.controller.get_or_insert_with(Controller::default).input =
                                    has_input;
                                if has_input {
                                    // Active le joystick d'office : sans lui, rien à piloter en jeu.
                                    need_joystick = true;
                                }
                            }
                            if ui
                                .checkbox(&mut has_gyro, "📐 Gyroscope Controller (tilt)")
                                .on_hover_text("L'objet se déplace selon l'inclinaison de l'appareil")
                                .changed()
                            {
                                obj.controller.get_or_insert_with(Controller::default).gyro =
                                    has_gyro;
                            }
                            if !has_input && !has_gyro {
                                obj.controller = None;
                            }
                            if let Some(ctrl) = &mut obj.controller {
                                ui.horizontal(|ui| {
                                    ui.label("Vitesse");
                                    ui.add(egui::Slider::new(&mut ctrl.move_speed, 0.5..=10.0));
                                });
                                ui.weak("Pilotable : devient un corps dynamique (collisions + gravité).");
                                // Saut : choix du bouton tactile déclencheur.
                                ui.horizontal(|ui| {
                                    ui.label("🦘 Saut ← bouton");
                                    let sel = if ctrl.jump_button.is_empty() {
                                        "(aucun)".to_string()
                                    } else {
                                        ctrl.jump_button.clone()
                                    };
                                    egui::ComboBox::from_id_salt(("jump_btn", i))
                                        .selected_text(sel)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut ctrl.jump_button,
                                                String::new(),
                                                "(aucun)",
                                            );
                                            for b in &mobile_buttons {
                                                ui.selectable_value(
                                                    &mut ctrl.jump_button,
                                                    b.clone(),
                                                    b,
                                                );
                                            }
                                        });
                                });
                                if mobile_buttons.is_empty() {
                                    ui.weak("Ajoute un bouton via Ajouter › UI mobile pour le saut.");
                                }
                                if !ctrl.jump_button.is_empty() {
                                    ui.horizontal(|ui| {
                                        ui.label("Hauteur");
                                        ui.add(
                                            egui::Slider::new(&mut ctrl.jump_height, 0.3..=5.0)
                                                .suffix(" m"),
                                        );
                                    });
                                }
                                // Feu/Arme/Soin (fireball.rs) : mêmes combos bouton tactile que Saut.
                                let button_combo =
                                    |ui: &mut egui::Ui, label: &str, salt: &str, field: &mut String| {
                                        ui.horizontal(|ui| {
                                            ui.label(label);
                                            let sel = if field.is_empty() {
                                                "(aucun)".to_string()
                                            } else {
                                                field.clone()
                                            };
                                            egui::ComboBox::from_id_salt((salt, i))
                                                .selected_text(sel)
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(
                                                        field,
                                                        String::new(),
                                                        "(aucun)",
                                                    );
                                                    for b in &mobile_buttons {
                                                        ui.selectable_value(field, b.clone(), b);
                                                    }
                                                });
                                        });
                                    };
                                button_combo(ui, "🔥 Feu ← bouton", "fire_btn", &mut ctrl.fire_button);
                                button_combo(
                                    ui,
                                    "🎒 Arme (cycle) ← bouton",
                                    "weapon_btn",
                                    &mut ctrl.weapon_button,
                                );
                                button_combo(ui, "❤ Soin ← bouton", "heal_btn", &mut ctrl.heal_button);
                            }
                            ui.horizontal(|ui| {
                                ui.label("📳 Vibration au tap");
                                let mut on = obj.vibrate_on_tap > 0;
                                if ui.checkbox(&mut on, "").changed() {
                                    obj.vibrate_on_tap = if on { 80 } else { 0 };
                                }
                                if obj.vibrate_on_tap > 0 {
                                    ui.add(
                                        egui::Slider::new(&mut obj.vibrate_on_tap, 20..=400)
                                            .suffix(" ms"),
                                    );
                                }
                            });
                        });
                        ui.separator();
                        ui.collapsing("Script (Lua)", |ui| {
                            ui.label(
                                "Variables : obj.x/y/z, obj.rx/ry/rz (°), obj.sx/sy/sz, \
                                 obj.r/g/b, obj.tapped, obj.triggered, dt, time, input.jx/jy, input.btn.<nom>, tilt.x/y, vibrate(ms), set_health(0..1)",
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut obj.script)
                                    .code_editor()
                                    .desired_rows(4)
                                    .hint_text("ex : obj.ry = obj.ry + dt * 90"),
                            );
                            // --- Génération par IA (DeepSeek) ---
                            ui.separator();
                            ui.label("✨ Générer par IA");
                            ui.add(
                                egui::TextEdit::multiline(ai_prompt)
                                    .desired_rows(2)
                                    .hint_text(
                                        "Décris le comportement, ex : « tourne lentement et \
                                         grossit quand on le touche »",
                                    ),
                            );
                            let has_key = !settings.deepseek_api_key.trim().is_empty();
                            let can_gen =
                                has_key && !status.ai_busy && !ai_prompt.trim().is_empty();
                            let can_opt =
                                has_key && !status.ai_busy && !obj.script.trim().is_empty();
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(can_gen, egui::Button::new("✨ Générer"))
                                    .on_hover_text("Crée un script à partir de la consigne")
                                    .clicked()
                                {
                                    actions.ai_generate = Some((
                                        i,
                                        crate::app::ai::AiRequest {
                                            api_key: settings.deepseek_api_key.clone(),
                                            model: settings.deepseek_model.clone(),
                                            temperature: settings.deepseek_temperature,
                                            prompt: ai_prompt.clone(),
                                        },
                                    ));
                                }
                                if ui
                                    .add_enabled(can_opt, egui::Button::new("🔧 Optimiser"))
                                    .on_hover_text("Améliore/corrige le script actuel")
                                    .clicked()
                                {
                                    let extra = ai_prompt.trim();
                                    let consigne = if extra.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" Tiens compte de : {extra}.")
                                    };
                                    let prompt = format!(
                                        "Améliore et optimise ce script Lua (corrige les bugs, \
                                         simplifie, garde le même comportement).{consigne}\n\n\
                                         Script actuel :\n{}",
                                        obj.script
                                    );
                                    actions.ai_generate = Some((
                                        i,
                                        crate::app::ai::AiRequest {
                                            api_key: settings.deepseek_api_key.clone(),
                                            model: settings.deepseek_model.clone(),
                                            temperature: settings.deepseek_temperature,
                                            prompt,
                                        },
                                    ));
                                }
                                if status.ai_busy {
                                    ui.spinner();
                                    ui.label("IA…");
                                } else if !has_key {
                                    ui.label("clé API requise (⚙ Paramètres)");
                                }
                            });
                        });
                        ui.separator();
                        if ui.button("🗑 Supprimer").clicked() {
                            actions.delete = Some(i);
                        }
                        // L'emprunt mut de `obj` est terminé : on peut toucher scene.mobile.
                        if need_joystick {
                            scene.mobile.joystick = true;
                        }
                    }
                    _ => {
                        ui.label("Aucun objet sélectionné.");
                    }
                }
                    });
            });
    }

    // Région centrale 3D (ce qui reste après les panneaux) : base de l'aperçu mobile.
    let central = root.available_rect_before_wrap();
    let ppp = root.ctx().pixels_per_point();
    *view_rect = (
        central.left() * ppp,
        central.top() * ppp,
        central.width() * ppp,
        central.height() * ppp,
    );

    // Cadre « téléphone » + contrôles tactiles, confinés à la zone de jeu.
    let play_rect = play_area_rect(central, *device_preview, *device_portrait);
    if *device_preview {
        device_bezel(root.ctx(), play_rect);
        touch_feedback(root.ctx(), play_rect);
    }
    if *playing && damage_flash > 0.0 {
        damage_vignette(root.ctx(), play_rect, damage_flash);
    }
    if *playing && ally_down_flash > 0.0 {
        ally_down_banner(root.ctx(), play_rect, ally_down_flash, locale);
    }
    if let Some(h) = hud_health.or_else(|| scene.mobile.health_bar.then_some(1.0)) {
        health_bar(root.ctx(), play_rect, h);
    }
    if *playing && !panels.hud_hidden {
        // Décalages persistés (Scene::hud_layout) : pas de glisser pendant une
        // partie en cours (`draggable: false`) — le repositionnement se fait via
        // 👁 Aperçu HUD › 🖐 Repositionner, en Édition, ci-dessous. Le bloc
        // entier se masque d'un Select à la manette (`Panels::hud_hidden`) —
        // la vignette de dégâts et la barre de vie, au-dessus, jamais.
        wave_hud(root.ctx(), play_rect, scene, wave, locale);
        weapon_hud(
            root.ctx(),
            play_rect,
            weapon_label,
            &mut scene.hud_layout.weapon_hud,
            false,
            locale,
        );
        kills_hud(
            root.ctx(),
            play_rect,
            kills,
            &mut scene.hud_layout.kills,
            false,
            locale,
        );
        multiplayer_roster_panel(
            root.ctx(),
            play_rect,
            roster,
            &mut scene.hud_layout.roster,
            false,
            locale,
        );
        if scene_has_ranged_weapon(scene) {
            crosshair(
                root.ctx(),
                play_rect,
                &mut scene.hud_layout.crosshair,
                false,
            );
            weapon_inventory_panel(
                root.ctx(),
                play_rect,
                weapon_inventory,
                selected_weapon,
                &mut scene.hud_layout.weapon_inventory,
                false,
                actions,
                locale,
            );
        }
        item_inventory_panel(
            root.ctx(),
            play_rect,
            item_inventory,
            &mut scene.hud_layout.item_inventory,
            false,
            actions,
        );
    } else if hud_preview.open {
        hud_preview_overlays(
            root.ctx(),
            play_rect,
            hud_preview,
            &mut scene.hud_layout,
            weapon_label,
            weapon_inventory,
            selected_weapon,
            actions,
            locale,
        );
    }
    if *playing && let Some((c, t)) = scene.collectibles() {
        collectibles_hud(root.ctx(), play_rect, c, t, game_time, score, locale);
    }
    if *playing && lost {
        lose_banner(root.ctx(), play_rect, locale);
    } else if *playing && defeated {
        defeated_banner(root.ctx(), play_rect, locale);
    }
    // Fin de partie : bouton « Rejouer » (preview éditeur, comme sur APK).
    if *playing && (won || lost) && restart_button(root.ctx(), play_rect, won, locale) {
        actions.restart = true;
    }
    if *playing && scene.mobile.any() {
        mobile_overlay(root.ctx(), play_rect, &scene.mobile, input_state);
    } else {
        input_state.joy = (0.0, 0.0);
        input_state.buttons.clear();
    }
    // Widgets HUD déclaratifs (`Scene::hud_widgets`) : visibles en Play comme en
    // Édition (via 👁 Aperçu HUD) — même logique que `hud_preview_overlays`
    // ci-dessus pour les overlays historiques.
    if *playing || hud_preview.open {
        let values = HudWidgetValues {
            health: hud_health.unwrap_or(1.0),
            score,
            kills,
            wave,
        };
        actions.hud_clicks = hud_widgets(root.ctx(), play_rect, scene, &values, hud_image_cache);
    }

    // Les actions (add/delete/duplicate/undo/redo) sont appliquées par AppState
    // après cette frame, afin de passer par l'historique.
}

fn transform_editor(ui: &mut egui::Ui, t: &mut Transform) {
    ui.label("Position");
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut t.position.x)
                .speed(0.05)
                .prefix("x "),
        );
        ui.add(
            egui::DragValue::new(&mut t.position.y)
                .speed(0.05)
                .prefix("y "),
        );
        ui.add(
            egui::DragValue::new(&mut t.position.z)
                .speed(0.05)
                .prefix("z "),
        );
    });

    // rotation éditée en degrés via les angles d'Euler — canonicalisés : sans ça,
    // un yaw au-delà de ±90° s'affiche (±180, 180−y, ±180) et éditer un seul champ
    // recompose la rotation avec les flips (cf. `scene::canonical_euler_xyz`).
    let (mut rx, mut ry, mut rz) = crate::scene::canonical_euler_xyz(t.rotation);
    rx = rx.to_degrees();
    ry = ry.to_degrees();
    rz = rz.to_degrees();
    ui.label("Rotation (°)");
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .add(egui::DragValue::new(&mut rx).speed(1.0).prefix("x "))
            .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut ry).speed(1.0).prefix("y "))
            .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut rz).speed(1.0).prefix("z "))
            .changed();
    });
    if changed {
        t.rotation = Quat::from_euler(
            EulerRot::XYZ,
            rx.to_radians(),
            ry.to_radians(),
            rz.to_radians(),
        );
    }

    ui.label("Échelle");
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut t.scale.x)
                .speed(0.05)
                .prefix("x "),
        );
        ui.add(
            egui::DragValue::new(&mut t.scale.y)
                .speed(0.05)
                .prefix("y "),
        );
        ui.add(
            egui::DragValue::new(&mut t.scale.z)
                .speed(0.05)
                .prefix("z "),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use hud::roster_display_order;

    /// Le tableau des joueurs (👥) classe par frags décroissants, frags
    /// inconnus (avant premier snapshot) comptés 0, et conserve l'ordre
    /// d'origine à égalité (tri stable ⇒ soi-même reste devant).
    #[test]
    fn roster_trie_par_frags_decroissants() {
        let roster: Vec<RosterEntry> = vec![
            ("Vous".into(), Some(1.0), Some(2), true),
            ("Alice".into(), Some(0.5), Some(5), false),
            ("Bob".into(), None, None, false),
            ("Chloé".into(), Some(0.2), Some(2), false),
        ];
        let ordered: Vec<&str> = roster_display_order(&roster)
            .iter()
            .map(|(name, _, _, _)| name.as_str())
            .collect();
        assert_eq!(ordered, ["Alice", "Vous", "Chloé", "Bob"]);
    }
}
