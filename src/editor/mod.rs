//! UI de l'éditeur basée sur egui : toolbar, hiérarchie, inspecteur.
//! Encapsule toute la plomberie egui-winit / egui-wgpu.

pub mod export;
pub mod readiness;

use egui::ViewportId;
use glam::{EulerRot, Quat};
use winit::window::Window;

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
    /// Commande en cours de saisie dans la Console (Sprint 82).
    console_input: String,
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
    /// « ⏭ » (Sprint 81) : avance d'exactement un pas fixe pendant la pause.
    pub step_frame: bool,
    /// Commande saisie dans la console (Sprint 82), à exécuter via
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
    /// Fenêtre Multijoueur : « Se connecter » demandé (adresse, pseudo).
    pub connect_to_server: Option<(String, String)>,
    /// Fenêtre Multijoueur : « Se déconnecter » demandé.
    pub disconnect_from_server: bool,
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
    /// « Quitter » : ferme l'application.
    pub quit: bool,
    pub play_audio: Option<String>,
    /// Réordonnancement de l'objet sélectionné : `Some(true)` = descendre, `Some(false)` = monter.
    pub move_in_list: Option<bool>,
    /// Réordonnancement par glisser-déposer dans la hiérarchie : `(index source, index cible)`.
    pub reorder: Option<(usize, usize)>,
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
        Editor {
            ctx,
            winit_state,
            renderer,
            export: export::ExportPanel::new(),
            hier_filter: String::new(),
            hier_new_group: String::new(),
            hier_rename: None,
            panels: Panels::default(),
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
        }
    }

    /// Réglages courants (clé API DeepSeek, config Firebase…) — lecture seule,
    /// pour les appelants (ex. `Renderer`) qui doivent construire une requête
    /// à partir de la config sans dupliquer l'état.
    pub fn settings(&self) -> &crate::app::settings::Settings {
        &self.settings
    }

    /// Mode Player : dessine **uniquement** les contrôles tactiles en surimpression
    /// (pas de panneaux d'éditeur) et met à jour l'état d'entrée lu par les scripts.
    /// Depuis le Sprint 65, ajoute aussi un petit overlay Multijoueur (adresse +
    /// pseudo + connecter/déconnecter) repliable, pour rejoindre un serveur
    /// RusteeGear depuis un APK — les actions demandées sont renvoyées dans un
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
        game_time: Option<f32>,
        score: u32,
        lost: bool,
        won: bool,
        wave: u32,
        restart: &mut bool,
        net_status: &str,
        net_connected: bool,
        weapon_label: &str,
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
            if let Some(h) = hud_health.or_else(|| mobile.health_bar.then_some(1.0)) {
                health_bar(ctx, area, h);
            }
            wave_hud(ctx, area, scene, wave);
            weapon_hud(ctx, area, weapon_label);
            if let Some((c, t)) = scene.collectibles() {
                collectibles_hud(ctx, area, c, t, game_time, score);
            }
            if lost {
                lose_banner(ctx, area);
            }
            // Fin de partie (gagné/perdu) : bouton « Rejouer » in-game (essentiel sur APK).
            if (won || lost) && restart_button(ctx, area, won) {
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
        // Multiplicateur du temps simulé (Sprint 81) — voir `AppState::time_scale`.
        time_scale: &mut f32,
        gizmo_mode: &mut GizmoMode,
        input_state: &mut crate::app::PlayerInput,
        device_preview: &mut bool,
        device_portrait: &mut bool,
        view_rect: &mut (f32, f32, f32, f32),
        hud_health: Option<f32>,
        damage_flash: f32,
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
                &mut actions,
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

/// Catégorie (libellé + icône) d'un type de mesh, pour le regroupement de la hiérarchie.
fn mesh_category(mesh: MeshKind) -> (&'static str, &'static str) {
    match mesh {
        MeshKind::Cube => ("Cubes", "🧊"),
        MeshKind::Sphere => ("Sphères", "⚪"),
        MeshKind::Plane => ("Plans", "▦"),
        MeshKind::Cylinder => ("Cylindres", "🛢"),
        MeshKind::Capsule => ("Capsules", "💊"),
        MeshKind::Terrain => ("Terrains", "⛰"),
        MeshKind::Imported(_) => ("Modèles", "📦"),
    }
}

/// Badges compacts d'un objet : physique / script / audio.
fn object_badges(obj: &crate::scene::SceneObject) -> String {
    let mut b = String::new();
    match obj.physics {
        PhysicsKind::Static => b.push_str(" 🧱"),
        PhysicsKind::Dynamic => b.push_str(" ⚙"),
        PhysicsKind::None => {}
    }
    if !obj.script.trim().is_empty() {
        b.push_str(" 📜");
    }
    if obj.audio.as_ref().is_some_and(|a| !a.clip.is_empty()) {
        b.push_str(" 🔊");
    }
    b
}

/// Hiérarchie ergonomique : recherche, **groupes définis par l'utilisateur** avec
/// glisser-déposer des objets, icônes et badges (physique/script/audio).
#[allow(clippy::too_many_arguments)] // panneau d'UI : états distincts à muter
fn hierarchy_panel(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
    selected_light: &mut Option<usize>,
    filter: &mut String,
    new_group: &mut String,
    rename: &mut Option<(usize, String)>,
    actions: &mut UiActions,
) {
    ui.horizontal(|ui| {
        ui.heading("Hiérarchie");
        ui.weak(format!("({})", scene.objects.len()));
    });
    ui.small("Glisser un objet sur un autre : réordonner · sur un groupe : ranger.");
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text("🔎 filtrer…")
            .desired_width(f32::INFINITY),
    );
    // Création d'un groupe.
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_group)
                .hint_text("nouveau groupe")
                .desired_width(130.0),
        );
        if ui.button("➕ Groupe").clicked() {
            let n = new_group.trim().to_string();
            if !n.is_empty() && !scene.groups.contains(&n) {
                scene.groups.push(n);
            }
            new_group.clear();
        }
    });
    ui.separator();

    let needle = filter.trim().to_lowercase();
    // Sections : groupes utilisateur (ordre conservé) puis « Sans groupe ».
    let mut sections: Vec<Option<String>> = scene.groups.iter().cloned().map(Some).collect();
    sections.push(None);

    // Mutations différées (appliquées après l'UI pour éviter les conflits d'emprunt).
    let mut moves: Vec<(usize, String)> = Vec::new();
    let mut delete_group: Option<String> = None;
    let mut commit_rename: Vec<(usize, String)> = Vec::new();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for sec in &sections {
                let gname = sec.clone().unwrap_or_default();
                let title = match sec {
                    Some(g) => format!("📁 {g}"),
                    None => "🗂 Sans groupe".to_string(),
                };
                let items: Vec<(usize, &crate::scene::SceneObject)> = scene
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| {
                        o.group == gname
                            && (needle.is_empty() || o.name.to_lowercase().contains(&needle))
                    })
                    .collect();

                // La zone de dépôt couvre tout le groupe : déposer un objet l'y assigne.
                let (_, payload) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::default(), |ui| {
                    ui.horizontal(|ui| {
                        egui::CollapsingHeader::new(format!("{title}  ({})", items.len()))
                            .default_open(true)
                            .id_salt(("grp", &gname))
                            .show(ui, |ui| {
                                for (i, obj) in &items {
                                    let i = *i;
                                    // Mode renommage inline pour cette ligne.
                                    if let Some((ri, buf)) =
                                        rename.as_mut().filter(|(ri, _)| *ri == i)
                                    {
                                        let r = ui.add(
                                            egui::TextEdit::singleline(buf)
                                                .desired_width(f32::INFINITY),
                                        );
                                        r.request_focus();
                                        // Valide à la perte de focus (Entrée ou clic ailleurs).
                                        if r.lost_focus() {
                                            commit_rename.push((*ri, buf.clone()));
                                        }
                                        continue;
                                    }
                                    let is_sel = selected.contains(&i);
                                    let label = format!(
                                        "{} {}{}",
                                        mesh_category(obj.mesh).1,
                                        obj.name,
                                        object_badges(obj)
                                    );
                                    let resp = ui
                                        .dnd_drag_source(egui::Id::new(("obj", i)), i, |ui| {
                                            let _ = ui.selectable_label(is_sel, label);
                                        })
                                        .response;
                                    // Dépôt d'un autre objet *sur* cette ligne → réordonner avant elle.
                                    if let Some(src) = resp.dnd_release_payload::<usize>()
                                        && *src != i
                                    {
                                        actions.reorder = Some((*src, i));
                                    }
                                    if resp.clicked() {
                                        let m = ui.input(|inp| inp.modifiers);
                                        if m.command || m.shift {
                                            // toggle dans l'ensemble
                                            if let Some(p) = selected.iter().position(|&x| x == i) {
                                                selected.remove(p);
                                                *selection = selected.last().copied();
                                            } else {
                                                selected.push(i);
                                                *selection = Some(i);
                                            }
                                        } else {
                                            *selection = Some(i);
                                            *selected = vec![i];
                                        }
                                    }
                                    if resp.double_clicked() {
                                        *rename = Some((i, obj.name.clone()));
                                    }
                                }
                                if items.is_empty() {
                                    ui.weak("  (déposer ici)");
                                }
                            });
                        // Bouton de suppression pour les groupes utilisateur.
                        if sec.is_some() && ui.small_button("🗑").clicked() {
                            delete_group = Some(gname.clone());
                        }
                    });
                });
                if let Some(idx) = payload {
                    moves.push((*idx, gname.clone()));
                }
            }
            if scene.objects.is_empty() {
                ui.weak("(scène vide)");
            }
        });

    // Application des mutations.
    for (idx, g) in moves {
        if let Some(o) = scene.objects.get_mut(idx) {
            o.group = g;
        }
    }
    if let Some(g) = delete_group {
        scene.groups.retain(|x| x != &g);
        for o in &mut scene.objects {
            if o.group == g {
                o.group.clear();
            }
        }
    }
    // Renommage validé.
    if let Some((idx, name)) = commit_rename.into_iter().next() {
        if let Some(o) = scene.objects.get_mut(idx) {
            o.name = name;
        }
        *rename = None;
    }

    // Section « Lumières » : liste les lumières ponctuelles comme des entités.
    if !scene.point_lights.is_empty() {
        ui.separator();
        egui::CollapsingHeader::new(format!("💡 Lumières ({})", scene.point_lights.len()))
            .default_open(true)
            .show(ui, |ui| {
                let mut remove = None;
                for i in 0..scene.point_lights.len() {
                    let spot = scene.point_lights[i].spot_angle > 0.0;
                    let label = if spot {
                        format!("🔦 Spot {i}")
                    } else {
                        format!("💡 Point {i}")
                    };
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(*selected_light == Some(i), label)
                            .clicked()
                        {
                            *selected_light = Some(i);
                            *selection = None;
                            selected.clear();
                        }
                        if ui.small_button("🗑").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    scene.point_lights.remove(i);
                    if *selected_light == Some(i) {
                        *selected_light = None;
                    }
                }
            });
    }
}

/// Menu « Fichier » : sauvegarde, ouverture, import, export.
fn menu_fichier(ui: &mut egui::Ui, export: &mut export::ExportPanel, actions: &mut UiActions) {
    ui.menu_button("Fichier", |ui| {
        if ui.button("✨  Nouveau projet").clicked() {
            actions.new_scene = true;
            ui.close();
        }
        if ui
            .button("🎮  Démo mobile (jouable)")
            .on_hover_text("Charge une scène : joystick + bouton Saut + personnage scripté")
            .clicked()
        {
            actions.load_demo = true;
            ui.close();
        }
        if ui
            .button("🎯  Démo gameplay (complète)")
            .on_hover_text("Joystick + gyroscope + saut + zone de danger + barre de vie + tap")
            .clicked()
        {
            actions.load_gameplay = true;
            ui.close();
        }
        if ui
            .button("🕹  Démo contrôleur (joystick + saut, sans script)")
            .on_hover_text(
                "Joueur pilotable au joystick, saut sur bouton, collisions avec le décor",
            )
            .clicked()
        {
            actions.load_controller = true;
            ui.close();
        }
        if ui
            .button("🗼  Démo Tour d'ascension (platforming)")
            .on_hover_text(
                "Style différent : grimpe la tour en spirale, aucune arme ni combat, éviter le vide",
            )
            .clicked()
        {
            actions.load_tower = true;
            ui.close();
        }
        if ui
            .button("🏃  Démo Course infinie (style Temple Run)")
            .on_hover_text(
                "Course automatique + changement de voie + saut : esquive les obstacles, ramasse les pièces",
            )
            .clicked()
        {
            actions.load_temple_run = true;
            ui.close();
        }
        if ui
            .button("🧩  Scène exemple (composants Controller/Audio/Combat)")
            .on_hover_text(
                "Référence minimale : un objet par composant optionnel, pas un niveau de jeu",
            )
            .clicked()
        {
            actions.load_components_demo = true;
            ui.close();
        }
        if ui
            .button("🧟  Démo Vagues de zombies (local, sans réseau)")
            .on_hover_text(
                "Manches de monstres (Rôdeur/Coureur/Brute) qui poursuivent le joueur, style Call of Zombies",
            )
            .clicked()
        {
            actions.load_ai_duel = true;
            ui.close();
        }
        if ui
            .button("🌐  Démo MMORPG (test multijoueur PC ↔ mobile)")
            .on_hover_text(
                "Arène minimale sans monstres/manches : joueur pilotable (joystick + saut), \
                 pensée pour voir un client desktop et un APK se déplacer l'un par rapport à l'autre",
            )
            .clicked()
        {
            actions.load_mmorpg = true;
            ui.close();
        }
        if ui
            .button("🗡  Démo Donjon (roguelike, 3 salles)")
            .on_hover_text(
                "3 salles à vider une à une (porte fermée jusqu'à la précédente vidée), arme de départ tirée au sort",
            )
            .clicked()
        {
            actions.load_roguelike = true;
            ui.close();
        }
        if ui
            .button("🥊  Démo Duel (façon Tekken/Smash Bros)")
            .on_hover_text(
                "Arène flottante, un rival à plusieurs coups avant de tomber, ring out possible (le vide sous l'arène est mortel)",
            )
            .clicked()
        {
            actions.load_brawl = true;
            ui.close();
        }
        if ui
            .button("✨  Générer une scène (IA)…")
            .on_hover_text("Crée une scène complète depuis une description (DeepSeek)")
            .clicked()
        {
            actions.open_ai_scene = true;
            ui.close();
        }
        ui.separator();
        if ui.button("💾  Enregistrer").clicked() {
            actions.save = true;
            ui.close();
        }
        if ui.button("💾  Enregistrer sous…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Scène JSON", &["json"])
                .set_file_name("scene.json")
                .save_file()
            {
                actions.save_path = Some(p.to_string_lossy().into_owned());
            }
            ui.close();
        }
        if ui.button("📂  Ouvrir…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Scène JSON", &["json"])
                .pick_file()
            {
                actions.load_path = Some(p.to_string_lossy().into_owned());
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            {
                actions.load = true;
            }
            ui.close();
        }
        ui.separator();
        if ui.button("📥  Importer glTF…").clicked() {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("glTF", &["glb", "gltf"])
                .pick_file()
            {
                actions.import = Some(p.to_string_lossy().into_owned());
            }
            ui.close();
        }
        ui.separator();
        if ui.button("📦  Build & Export…").clicked() {
            export.open = true;
            ui.close();
        }
        if ui
            .button("⚙  Paramètres projet…")
            .on_hover_text("Nom, package, version, build, orientation (panneau de build)")
            .clicked()
        {
            export.open = true;
            actions.open_settings = true;
            ui.close();
        }
        ui.separator();
        if ui.button("🚪  Quitter").clicked() {
            actions.quit = true;
            ui.close();
        }
    });
}

/// Menu « Édition » : historique et opérations sur la sélection.
fn menu_edition(ui: &mut egui::Ui, selection: &Option<usize>, actions: &mut UiActions) {
    ui.menu_button("Édition", |ui| {
        if ui.button("↩  Annuler").clicked() {
            actions.undo = true;
            ui.close();
        }
        if ui.button("↪  Rétablir").clicked() {
            actions.redo = true;
            ui.close();
        }
        ui.separator();
        let has = selection.is_some();
        if ui
            .add_enabled(has, egui::Button::new("✂  Couper"))
            .clicked()
        {
            actions.cut = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("⧉  Copier"))
            .clicked()
        {
            actions.copy = true;
            ui.close();
        }
        if ui.button("📋  Coller").clicked() {
            actions.paste = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("⧉  Dupliquer"))
            .clicked()
        {
            actions.duplicate = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗑  Supprimer"))
            .clicked()
        {
            actions.delete = *selection;
            ui.close();
        }
        ui.separator();
        if ui.button("☑  Tout sélectionner").clicked() {
            actions.select_all = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗂  Grouper"))
            .clicked()
        {
            actions.group = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("🗂  Dégrouper"))
            .clicked()
        {
            actions.ungroup = true;
            ui.close();
        }
        ui.menu_button("📐  Aligner sur…", |ui| {
            ui.label("Aligne la sélection sur l'objet primaire");
            for (axis, label) in [(0, "Axe X"), (1, "Axe Y"), (2, "Axe Z")] {
                if ui.button(label).clicked() {
                    actions.align_axis = Some(axis);
                    ui.close();
                }
            }
        });
        ui.menu_button("📏  Distribuer sur…", |ui| {
            ui.label("Espace régulièrement la sélection (≥ 3 objets)");
            for (axis, label) in [(0, "Axe X"), (1, "Axe Y"), (2, "Axe Z")] {
                if ui.button(label).clicked() {
                    actions.distribute_axis = Some(axis);
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui
            .add_enabled(has, egui::Button::new("⬇  Aligner au sol"))
            .on_hover_text("Pose la base de l'objet sur le plan (y = 0)")
            .clicked()
        {
            actions.align_ground = true;
            ui.close();
        }
        if ui
            .add_enabled(has, egui::Button::new("↺  Réinitialiser transform"))
            .on_hover_text("Rotation et échelle par défaut (position conservée)")
            .clicked()
        {
            actions.reset_transform = true;
            ui.close();
        }
    });
}

/// Menu « Ajouter » façon Unity : objets 3D, lumières, caméras, physique, audio, UI mobile.
fn menu_ajouter(
    ui: &mut egui::Ui,
    scene: &mut Scene,
    selection: Option<usize>,
    actions: &mut UiActions,
) {
    use crate::scene::{MAX_POINT_LIGHTS, PointLight};
    ui.menu_button("Ajouter", |ui| {
        // --- Objet 3D ---
        ui.menu_button("🧱  Objet 3D", |ui| {
            for (kind, label) in [
                (MeshKind::Cube, "🧊  Cube"),
                (MeshKind::Sphere, "⚪  Sphère"),
                (MeshKind::Plane, "▦  Plan"),
                (MeshKind::Cylinder, "🛢  Cylindre"),
                (MeshKind::Capsule, "💊  Capsule"),
                (MeshKind::Terrain, "⛰  Terrain"),
            ] {
                if ui.button(label).clicked() {
                    actions.add = Some(kind);
                    ui.close();
                }
            }
        });

        // --- Lumières ---
        ui.menu_button("💡  Lumière", |ui| {
            let can_add = scene.point_lights.len() < MAX_POINT_LIGHTS;
            if ui
                .add_enabled(can_add, egui::Button::new("💡  Ponctuelle (point)"))
                .clicked()
            {
                scene.point_lights.push(PointLight::default());
                ui.close();
            }
            if ui
                .add_enabled(can_add, egui::Button::new("🔦  Spot (cône)"))
                .clicked()
            {
                scene.point_lights.push(PointLight {
                    spot_angle: 30.0,
                    ..PointLight::default()
                });
                ui.close();
            }
            ui.separator();
            if ui
                .button("☀  Directionnelle (réinitialiser)")
                .on_hover_text(
                    "Lumière directionnelle de la scène (une seule) — valeurs par défaut",
                )
                .clicked()
            {
                scene.light.dir = [0.5, 1.0, 0.3];
                scene.light.color = [1.0, 1.0, 1.0];
                ui.close();
            }
            if ui
                .button("🌙  Ambiante +0,1")
                .on_hover_text("Augmente la lumière ambiante de la scène")
                .clicked()
            {
                scene.light.ambient = (scene.light.ambient + 0.1).min(1.0);
                ui.close();
            }
        });

        // --- Caméras ---
        ui.menu_button("🎥  Caméra", |ui| {
            if ui
                .button("🎥  Principale (vue actuelle)")
                .on_hover_text("Fige le point de vue de jeu sur la caméra actuelle (Play)")
                .clicked()
            {
                actions.set_game_camera = true;
                ui.close();
            }
            if ui
                .selectable_label(scene.camera_follow, "🎯  Caméra de suivi (mobile)")
                .on_hover_text("En Play, la caméra suit l'objet scripté (joueur)")
                .clicked()
            {
                scene.camera_follow = !scene.camera_follow;
                ui.close();
            }
        });

        ui.separator();

        // --- Physique (s'applique à la sélection) ---
        let sel = selection.filter(|&i| i < scene.objects.len());
        ui.add_enabled_ui(sel.is_some(), |ui| {
            ui.menu_button("⚙  Physique (sélection)", |ui| {
                use crate::runtime::physics::PhysicsKind as P;
                if let Some(i) = sel {
                    if ui.button("🧱  Corps statique").clicked() {
                        scene.objects[i].physics = P::Static;
                        ui.close();
                    }
                    if ui.button("⚙  Rigidbody (dynamique)").clicked() {
                        scene.objects[i].physics = P::Dynamic;
                        ui.close();
                    }
                    if ui.button("🎯  Zone de déclenchement (trigger)").clicked() {
                        scene.objects[i].trigger = true;
                        ui.close();
                    }
                    if ui.button("✕  Aucune physique").clicked() {
                        scene.objects[i].physics = P::None;
                        ui.close();
                    }
                }
            });
        });

        // --- Audio (s'applique à la sélection) ---
        ui.add_enabled_ui(sel.is_some(), |ui| {
            if ui
                .button("🔊  Source audio (sélection)…")
                .on_hover_text("Choisit un son joué par l'objet sélectionné")
                .clicked()
            {
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                if let Some(i) = sel
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Audio", &["wav", "ogg", "flac", "mp3"])
                        .pick_file()
                {
                    scene.objects[i]
                        .audio
                        .get_or_insert_with(crate::scene::AudioSource::default)
                        .clip = p.to_string_lossy().into_owned();
                }
                ui.close();
            }
        });

        ui.separator();

        // --- UI mobile ---
        ui.menu_button("📱  UI mobile", |ui| {
            let m = &mut scene.mobile;
            if ui
                .selectable_label(m.joystick, "🕹  Joystick virtuel")
                .clicked()
            {
                m.joystick = !m.joystick;
                // Mutuellement exclusif avec la croix directionnelle : les deux se
                // dessinent dans le même coin de l'écran, jamais les deux à la fois.
                if m.joystick {
                    m.dpad = false;
                }
                ui.close();
            }
            if ui
                .selectable_label(m.dpad, "🎮  Pavé W/A/S/D (contrôles tank)")
                .on_hover_text(
                    "Mêmes contrôles que le clavier : W/S avance/recule le long de \
                     l'orientation du personnage, A/D le fait pivoter",
                )
                .clicked()
            {
                m.dpad = !m.dpad;
                if m.dpad {
                    m.joystick = false;
                }
                ui.close();
            }
            if ui.button("🔘  Bouton tactile").clicked() {
                let n = m.buttons.len() + 1;
                m.buttons.push(format!("B{n}"));
                ui.close();
            }
            if !m.buttons.is_empty() && ui.button("✕  Retirer le dernier bouton").clicked() {
                m.buttons.pop();
                ui.close();
            }
            if ui
                .selectable_label(m.touch_zone, "👆  Zone tactile (plein écran)")
                .on_hover_text("Un tap n'importe où expose input.btn.touch au script")
                .clicked()
            {
                m.touch_zone = !m.touch_zone;
                ui.close();
            }
            if ui
                .selectable_label(m.health_bar, "❤  Barre de vie (HUD)")
                .on_hover_text("Affiche une barre de vie pilotée par set_health() côté script")
                .clicked()
            {
                m.health_bar = !m.health_bar;
                ui.close();
            }
            if ui
                .selectable_label(m.safe_area, "🛡  Zone sûre (safe area)")
                .on_hover_text("Rentre les contrôles dans une marge sûre (encoche, bords arrondis)")
                .clicked()
            {
                m.safe_area = !m.safe_area;
                ui.close();
            }
        });
    });
}

/// Menu « Outils » : mode de manipulation du gizmo + diagnostics.
fn menu_outils(
    ui: &mut egui::Ui,
    gizmo_mode: &mut GizmoMode,
    export: &mut export::ExportPanel,
    panels: &mut Panels,
) {
    ui.menu_button("Outils", |ui| {
        ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔  Déplacer (W)");
        ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻  Tourner (E)");
        ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⤢  Redimensionner (R)");
        ui.separator();
        if ui.button("🖥  Console").clicked() {
            panels.console = true;
            ui.close();
        }
        if ui.button("📊  Profiler FPS").clicked() {
            panels.profiler = true;
            ui.close();
        }
        if ui.button("📜  Gestionnaire de scripts Lua").clicked() {
            panels.scripts = true;
            ui.close();
        }
        ui.separator();
        if ui.button("🤖  Build Android…").clicked() {
            export.open = true;
            ui.close();
        }
        if ui.button("📁  Gestionnaire d'assets").clicked() {
            panels.assets = true;
            ui.close();
        }
        if ui.button("🪶  Optimisation mobile").clicked() {
            panels.optimize = true;
            ui.close();
        }
        if ui.button("✔  Contrôle qualité APK").clicked() {
            panels.readiness = true;
            panels.readiness_results.clear(); // forcer une nouvelle analyse à l'ouverture
            ui.close();
        }
        if ui.button("🩺  Diagnostic système").clicked() {
            panels.diagnostic = true;
            ui.close();
        }
        if ui.button("🌐  Multijoueur").clicked() {
            panels.multiplayer = true;
            ui.close();
        }
        ui.separator();
        if ui.button("⚙  Paramètres").clicked() {
            panels.settings = true;
            ui.close();
        }
    });
}

/// Menu « Aide » : raccourcis, guide export, diagnostic, à propos.
fn menu_aide(ui: &mut egui::Ui, panels: &mut Panels) {
    ui.menu_button("Aide", |ui| {
        if ui.button("⌨  Raccourcis clavier").clicked() {
            panels.shortcuts = true;
            ui.close();
        }
        if ui.button("🩺  Diagnostic système").clicked() {
            panels.diagnostic = true;
            ui.close();
        }
        ui.separator();
        ui.hyperlink_to(
            "📖  Guide export APK",
            "https://github.com/lberthod/rusteegear",
        );
        ui.hyperlink_to("🐙  Dépôt GitHub", "https://github.com/lberthod/rusteegear");
        ui.separator();
        if ui.button("ℹ  À propos de RusteeGear").clicked() {
            panels.about = true;
            ui.close();
        }
    });
}

/// Fenêtres flottantes des menus « Aide » et « Outils ».
fn tool_windows(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    export: &export::ExportPanel,
    status: &StatusInfo,
    console_input: &mut String,
    actions: &mut UiActions,
) {
    // --- Console (logs en mémoire + commandes, Sprint 82) ---
    egui::Window::new("🖥  Console")
        .open(&mut panels.console)
        .default_size([460.0, 320.0])
        .show(ctx, |ui| {
            if ui.button("🧹  Effacer").clicked() {
                crate::log_buffer::clear();
            }
            ui.separator();
            // Champ de commande : `timescale <valeur>`, `pause`, `play`, `step`, `tp <x> <y> <z>`,
            // `net_stats` (cf. AppState::run_console_command — liste complète en survol).
            ui.horizontal(|ui| {
                let resp = ui
                    .add(
                        egui::TextEdit::singleline(console_input)
                            .hint_text("timescale 0.5 · pause · play · step · tp 0 1 0 · net_stats")
                            .desired_width(ui.available_width() - 70.0),
                    )
                    .on_hover_text(
                        "Commandes : timescale <v> · pause · play · stop · step · \
                         tp <x> <y> <z> · net_stats",
                    );
                let submit = ui.button("Exécuter").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if submit && !console_input.trim().is_empty() {
                    actions.console_command = Some(console_input.trim().to_string());
                    console_input.clear();
                    resp.request_focus();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in crate::log_buffer::snapshot() {
                        ui.monospace(line);
                    }
                });
        });

    // --- Profiler FPS (n'accumule l'historique que lorsque la fenêtre est ouverte) ---
    if !panels.profiler {
        panels.fps_history.clear();
    } else {
        panels.fps_history.push_back(status.fps);
        while panels.fps_history.len() > 120 {
            panels.fps_history.pop_front();
        }
    }
    let fps_hist = panels.fps_history.clone();
    egui::Window::new("📊  Profiler FPS")
        .open(&mut panels.profiler)
        .resizable(false)
        .show(ctx, |ui| {
            let avg = if fps_hist.is_empty() {
                0.0
            } else {
                fps_hist.iter().sum::<f32>() / fps_hist.len() as f32
            };
            let min = fps_hist.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = fps_hist.iter().cloned().fold(0.0_f32, f32::max);
            ui.label(format!("FPS actuel : {:.0}", status.fps));
            ui.label(format!(
                "min {:.0} · moy {:.0} · max {:.0}",
                if min.is_finite() { min } else { 0.0 },
                avg,
                max
            ));
            ui.label(format!("🧊 {} objets", scene.objects.len()));
            ui.separator();
            // Sparkline simple : barres verticales normalisées sur 60 FPS.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(240.0, 60.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
            let n = fps_hist.len().max(1);
            let bar_w = rect.width() / n as f32;
            for (i, &f) in fps_hist.iter().enumerate() {
                let h = (f / 60.0).clamp(0.0, 1.0) * rect.height();
                let x = rect.left() + i as f32 * bar_w;
                let color = if f >= 55.0 {
                    egui::Color32::from_rgb(80, 200, 120)
                } else if f >= 30.0 {
                    egui::Color32::from_rgb(220, 180, 60)
                } else {
                    egui::Color32::from_rgb(220, 90, 80)
                };
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x, rect.bottom() - h),
                        egui::pos2(x + bar_w.max(1.0), rect.bottom()),
                    ),
                    0.0,
                    color,
                );
            }
            // --- Profiler mémoire (estimation) ---
            ui.separator();
            ui.strong("Mémoire (estimation)");
            let (obj_b, mesh_b, n_tex) = scene.memory_estimate();
            let kb = |b: usize| format!("{:.1} Ko", b as f32 / 1024.0);
            ui.label(format!("Objets : {}", kb(obj_b)));
            ui.label(format!(
                "Meshes importés : {} ({} modèle(s))",
                kb(mesh_b),
                scene.imported.len()
            ));
            ui.label(format!("Textures : {n_tex} unique(s)"));
        });

    // --- Contrôle qualité APK (APK Readiness Check) ---
    let mut do_analyze = false;
    egui::Window::new("✔  Contrôle qualité APK")
        .open(&mut panels.readiness)
        .default_size([420.0, 380.0])
        .show(ctx, |ui| {
            if panels.readiness_results.is_empty() {
                panels.readiness_results = readiness::analyze(scene, export.config());
            }
            let (ok, warn, fail) = readiness::summary(&panels.readiness_results);
            ui.horizontal(|ui| {
                ui.label(format!("✅ {ok}"));
                ui.label(format!("⚠ {warn}"));
                ui.label(format!("❌ {fail}"));
                if ui.button("🔄  Ré-analyser").clicked() {
                    do_analyze = true;
                }
            });
            if fail == 0 {
                ui.colored_label(
                    egui::Color32::from_rgb(80, 200, 120),
                    "Prêt pour l'export Android 🎉",
                );
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 90, 80),
                    format!("{fail} blocage(s) à corriger avant l'export"),
                );
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for c in &panels.readiness_results {
                        ui.horizontal(|ui| {
                            ui.label(c.status.icon());
                            ui.label(&c.label);
                        });
                    }
                });
        });
    if do_analyze {
        panels.readiness_results = readiness::analyze(scene, export.config());
    }

    egui::Window::new("⌨  Raccourcis clavier")
        .open(&mut panels.shortcuts)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("shortcuts_grid")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for (k, v) in [
                        ("W", "Déplacer (translation)"),
                        ("E", "Tourner (rotation)"),
                        ("R", "Redimensionner (échelle)"),
                        ("F", "Recentrer la caméra sur la sélection"),
                        ("Cmd/Ctrl + Z", "Annuler"),
                        ("Cmd/Ctrl + Maj + Z", "Rétablir"),
                        ("Cmd/Ctrl + D", "Dupliquer la sélection"),
                        ("Suppr", "Supprimer la sélection"),
                    ] {
                        ui.strong(k);
                        ui.label(v);
                        ui.end_row();
                    }
                });
        });

    egui::Window::new("🩺  Diagnostic système")
        .open(&mut panels.diagnostic)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Environnement de build Android :");
            ui.separator();
            let check = |ui: &mut egui::Ui, label: &str, ok: bool| {
                ui.horizontal(|ui| {
                    ui.label(if ok { "✅" } else { "❌" });
                    ui.label(label);
                });
            };
            // Le binaire tourne forcément via la toolchain Rust.
            check(ui, "Rust / Cargo", true);
            let android_sdk = std::env::var("ANDROID_HOME")
                .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
                .is_ok();
            let android_ndk = std::env::var("ANDROID_NDK_HOME")
                .or_else(|_| std::env::var("NDK_HOME"))
                .is_ok();
            check(ui, "Android SDK (ANDROID_HOME)", android_sdk);
            check(ui, "Android NDK (ANDROID_NDK_HOME)", android_ndk);
            ui.separator();
            ui.label(format!(
                "🖥  Backend graphique : {}",
                if cfg!(target_os = "macos") {
                    "Metal"
                } else if cfg!(target_os = "windows") {
                    "DX12 / Vulkan"
                } else {
                    "Vulkan"
                }
            ));
        });

    egui::Window::new("ℹ  À propos de RusteeGear")
        .open(&mut panels.about)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("RusteeGear");
            ui.label("Éditeur 3D en Rust orienté export Android natif.");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.hyperlink_to(
                "github.com/lberthod/rusteegear",
                "https://github.com/lberthod/rusteegear",
            );
        });
}

/// Rectangle (points egui) de la zone de jeu : écran de téléphone centré dans la
/// région `central` si l'aperçu mobile est actif, sinon `central` en entier.
fn play_area_rect(central: egui::Rect, preview: bool, portrait: bool) -> egui::Rect {
    if !preview {
        return central;
    }
    let (x, y, w, h) = crate::app::device_rect(central.width(), central.height(), portrait);
    egui::Rect::from_min_size(central.min + egui::vec2(x, y), egui::vec2(w, h))
}

/// Dessine le cadre « téléphone » (biseau arrondi + encoche) autour de la zone de jeu.
fn device_bezel(ctx: &egui::Context, rect: egui::Rect) {
    use egui::{Color32, Stroke};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("device_bezel"),
    ));
    // Contour épais arrondi, façon châssis de smartphone.
    painter.rect_stroke(
        rect.expand(6.0),
        22.0,
        Stroke::new(10.0, Color32::from_rgb(20, 20, 24)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        16.0,
        Stroke::new(1.5, Color32::from_white_alpha(40)),
        egui::StrokeKind::Inside,
    );
    // Encoche centrale en haut.
    let notch = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 9.0),
        egui::vec2(rect.width().min(160.0) * 0.45, 14.0),
    );
    painter.rect_filled(notch, 7.0, Color32::from_rgb(20, 20, 24));
}

/// Fenêtre « Paramètres » : clé API DeepSeek (persistée à chaque modification).
fn settings_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    settings: &mut crate::app::settings::Settings,
) {
    let mut open = panels.settings;
    egui::Window::new("⚙  Paramètres")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("IA — génération de scripts");
            ui.label("Clé API DeepSeek");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut settings.deepseek_api_key)
                    .password(true)
                    .hint_text("sk-…")
                    .desired_width(280.0),
            );
            if resp.lost_focus() || resp.changed() {
                settings.save();
            }
            ui.label(if settings.deepseek_api_key.trim().is_empty() {
                "❌ Aucune clé : génération IA désactivée"
            } else {
                "✅ Clé enregistrée"
            });
            ui.add_space(6.0);
            ui.label("Modèle");
            ui.horizontal(|ui| {
                for m in ["deepseek-chat", "deepseek-reasoner"] {
                    if ui
                        .selectable_label(settings.deepseek_model == m, m)
                        .clicked()
                    {
                        settings.deepseek_model = m.to_string();
                        settings.save();
                    }
                }
            });
            let resp_m = ui.add(
                egui::TextEdit::singleline(&mut settings.deepseek_model)
                    .hint_text("id du modèle (ex. deepseek-chat)")
                    .desired_width(280.0),
            );
            if resp_m.lost_focus() || resp_m.changed() {
                settings.save();
            }
            ui.small(
                "« deepseek-chat » pointe vers la dernière version. Saisis un id précis au besoin.",
            );
            ui.add_space(6.0);
            ui.label("Température (0 = précis, 1 = créatif)");
            if ui
                .add(egui::Slider::new(
                    &mut settings.deepseek_temperature,
                    0.0..=1.0,
                ))
                .drag_stopped()
            {
                settings.save();
            }
            ui.add_space(6.0);
            ui.hyperlink_to(
                "Obtenir une clé DeepSeek",
                "https://platform.deepseek.com/api_keys",
            );

            ui.add_space(12.0);
            ui.separator();
            ui.heading("Multijoueur — comptes (Firebase)");
            ui.label("Clé API Web Firebase");
            let resp_fb_key = ui.add(
                egui::TextEdit::singleline(&mut settings.firebase_api_key)
                    .password(true)
                    .hint_text("AIza…")
                    .desired_width(280.0),
            );
            if resp_fb_key.lost_focus() || resp_fb_key.changed() {
                settings.save();
            }
            ui.label("URL Realtime Database");
            let resp_fb_url = ui.add(
                egui::TextEdit::singleline(&mut settings.firebase_database_url)
                    .hint_text("https://xxx-default-rtdb.firebaseio.com")
                    .desired_width(280.0),
            );
            if resp_fb_url.lost_focus() || resp_fb_url.changed() {
                settings.save();
            }
            ui.label(
                if settings.firebase_api_key.trim().is_empty()
                    || settings.firebase_database_url.trim().is_empty()
                {
                    "❌ Configuration incomplète : comptes multijoueur désactivés"
                } else {
                    "✅ Configuration enregistrée"
                },
            );
            ui.small(
                "Clé publique par conception côté Firebase — la sécurité vient des règles \
                 de la Realtime Database, pas du secret de cette clé (cf. SPRINT_MMORPG.md).",
            );
        });
    panels.settings = open;
}

/// Overlay Multijoueur minimal pour le mode Player (mobile/APK, Sprint 65) :
/// adresse + pseudo + connecter/déconnecter, replié par défaut pour ne pas
/// gêner le joystick. Pas de compte Firebase/chat/classement ici — hors scope
/// de ce premier test (cf. `multiplayer_window`, l'équivalent complet côté
/// éditeur desktop).
fn mobile_multiplayer_overlay(
    ctx: &egui::Context,
    server_url: &mut String,
    name: &mut String,
    net_status: &str,
    net_connected: bool,
    actions: &mut UiActions,
) {
    egui::Window::new("🌐")
        .id(egui::Id::new("mobile_multiplayer"))
        .collapsible(true)
        .default_open(false)
        .resizable(false)
        // Décalage vertical généreux (pas seulement 8 px) : en plein écran immersif
        // (NativeActivity Android), la zone de rendu passe sous la barre de statut
        // système — un petit décalage laissait l'icône 🌐 cachée dessous, invisible
        // et donc impossible à toucher (constaté en testant sur un vrai téléphone).
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 56.0))
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.label("Adresse du serveur");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(server_url).hint_text("ws://192.168.1.x:7777"),
            );
            ui.label("Pseudo");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(name).hint_text("Joueur"),
            );
            ui.add_space(4.0);
            if net_connected {
                if ui.button("🔌 Se déconnecter").clicked() {
                    actions.disconnect_from_server = true;
                }
            } else {
                let can_connect = !server_url.trim().is_empty() && !name.trim().is_empty();
                if ui
                    .add_enabled(can_connect, egui::Button::new("▶ Se connecter"))
                    .clicked()
                {
                    actions.connect_to_server = Some((server_url.clone(), name.clone()));
                }
            }
            ui.add_space(4.0);
            ui.small(if net_status.is_empty() {
                "Non connecté"
            } else {
                net_status
            });
        });
}

/// Fenêtre « Multijoueur » : adresse du serveur + pseudo, connexion/déconnexion
/// (SPRINT_MMORPG.md). Le joueur local reste piloté comme en solo ; les autres
/// joueurs connectés apparaissent comme des objets fantômes une fois reçus par
/// `Snapshot` (cf. `app::network_client`).
#[allow(clippy::too_many_arguments)]
fn multiplayer_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    server_url: &mut String,
    name: &mut String,
    email: &mut String,
    password: &mut String,
    lobby_code: &mut String,
    chat_input: &mut String,
    settings: &crate::app::settings::Settings,
    net_status: &str,
    net_connected: bool,
    chat_messages: &[crate::app::network_client::ChatLine],
    has_firebase_account: bool,
    leaderboard: &[crate::app::network_client::LeaderboardLine],
    actions: &mut UiActions,
) {
    let mut open = panels.multiplayer;
    egui::Window::new("🌐  Multijoueur")
        .open(&mut open)
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.label("Adresse du serveur");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(server_url).hint_text("ws://127.0.0.1:7777"),
            );
            ui.label("Pseudo");
            ui.add_enabled(
                !net_connected,
                egui::TextEdit::singleline(name).hint_text("Joueur"),
            );
            ui.add_space(6.0);
            if net_connected {
                if ui.button("🔌  Se déconnecter").clicked() {
                    actions.disconnect_from_server = true;
                }
            } else {
                let can_connect = !server_url.trim().is_empty() && !name.trim().is_empty();
                if ui
                    .add_enabled(can_connect, egui::Button::new("▶  Se connecter"))
                    .clicked()
                {
                    actions.connect_to_server = Some((server_url.clone(), name.clone()));
                }
                if !can_connect {
                    ui.small("Adresse et pseudo requis.");
                }
            }
            ui.add_space(6.0);
            ui.label(if net_status.is_empty() {
                "Non connecté"
            } else {
                net_status
            });
            ui.add_space(6.0);
            ui.small(
                "Lance d'abord un serveur (`cargo run --bin server`), puis connecte-toi \
                 depuis chaque instance de l'éditeur/du player avec la même adresse.",
            );

            ui.add_space(12.0);
            ui.separator();
            ui.heading("Compte (optionnel)");
            let firebase_configured = !settings.firebase_api_key.trim().is_empty()
                && !settings.firebase_database_url.trim().is_empty();
            if !firebase_configured {
                ui.small(
                    "Configure d'abord une clé API et une URL Database dans \
                     ⚙ Paramètres pour activer les comptes (progression persistante).",
                );
            } else {
                ui.label("Email");
                ui.add(egui::TextEdit::singleline(email).hint_text("toi@example.com"));
                ui.label("Mot de passe");
                ui.add(egui::TextEdit::singleline(password).password(true));
                let can_auth = !email.trim().is_empty() && !password.trim().is_empty();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_auth, egui::Button::new("Se connecter (compte)"))
                        .clicked()
                    {
                        actions.firebase_sign_in = Some((email.clone(), password.clone()));
                    }
                    if ui
                        .add_enabled(can_auth, egui::Button::new("Créer un compte"))
                        .clicked()
                    {
                        actions.firebase_sign_up = Some((email.clone(), password.clone()));
                    }
                });
                ui.small(
                    "Se connecter avant de rejoindre un salon relie ta progression \
                     (XP, classement) à ce compte, cf. SPRINT_MMORPG.md.",
                );
            }

            if firebase_configured {
                ui.add_space(12.0);
                ui.separator();
                ui.heading("Chat");
                ui.label("Salon");
                ui.add(egui::TextEdit::singleline(lobby_code).hint_text("default"));
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        if chat_messages.is_empty() {
                            ui.small("Aucun message pour l'instant.");
                        }
                        for line in chat_messages {
                            ui.label(format!("{} : {}", line.sender, line.text));
                        }
                    });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(chat_input)
                            .hint_text("Message…")
                            .desired_width(180.0),
                    );
                    let can_send = has_firebase_account
                        && !chat_input.trim().is_empty()
                        && !lobby_code.trim().is_empty();
                    if ui
                        .add_enabled(can_send, egui::Button::new("Envoyer"))
                        .clicked()
                    {
                        actions.send_chat_message =
                            Some((lobby_code.clone(), name.clone(), chat_input.clone()));
                        chat_input.clear();
                    }
                });
                if !has_firebase_account {
                    ui.small("Connecte-toi d'abord à un compte pour envoyer des messages.");
                }
                if ui.button("🔄  Rafraîchir").clicked() && !lobby_code.trim().is_empty() {
                    actions.refresh_chat = Some(lobby_code.clone());
                }

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Classement");
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        if leaderboard.is_empty() {
                            ui.small("Aucun score pour l'instant.");
                        }
                        for (rank, entry) in leaderboard.iter().enumerate() {
                            ui.label(format!("{}. {} — {}", rank + 1, entry.name, entry.score));
                        }
                    });
                if ui.button("🔄  Rafraîchir le classement").clicked() {
                    actions.refresh_leaderboard = true;
                }
            }
        });
    panels.multiplayer = open;
}

/// Fenêtre « Générer une scène (IA) » : consigne → scène (remplacer ou ajouter) via DeepSeek.
#[allow(clippy::too_many_arguments)]
fn ai_scene_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    settings: &crate::app::settings::Settings,
    prompt: &mut String,
    replace: &mut bool,
    history: &mut Vec<String>,
    status: &StatusInfo,
    actions: &mut UiActions,
) {
    let mut open = panels.ai_scene;
    egui::Window::new("✨  Générer une scène (IA)")
        .open(&mut open)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.label("Décris la scène à générer :");
            ui.add(
                egui::TextEdit::multiline(prompt)
                    .desired_rows(3)
                    .desired_width(340.0)
                    .hint_text(
                        "ex : « un sol, un personnage capsule piloté au joystick, 3 cubes \
                         tactiles colorés et une caméra qui suit »",
                    ),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(replace, true, "Remplacer");
                ui.selectable_value(replace, false, "Ajouter à la scène");
            });
            let has_key = !settings.deepseek_api_key.trim().is_empty();
            let can = has_key && !status.ai_busy && !prompt.trim().is_empty();
            ui.horizontal(|ui| {
                let label = if *replace {
                    "✨ Générer (remplace)"
                } else {
                    "✨ Générer (ajoute)"
                };
                if ui.add_enabled(can, egui::Button::new(label)).clicked() {
                    let p = prompt.trim().to_string();
                    // Historique : consigne en tête, sans doublon, max 8.
                    history.retain(|h| h != &p);
                    history.insert(0, p.clone());
                    history.truncate(8);
                    actions.ai_generate_scene = Some((
                        crate::app::ai::AiRequest {
                            api_key: settings.deepseek_api_key.clone(),
                            model: settings.deepseek_model.clone(),
                            temperature: settings.deepseek_temperature,
                            prompt: p,
                        },
                        *replace,
                    ));
                }
                if status.ai_busy {
                    ui.spinner();
                    ui.label("génération…");
                } else if !has_key {
                    ui.label("clé API requise (⚙ Paramètres)");
                }
            });
            if !history.is_empty() {
                ui.separator();
                ui.label("Consignes récentes :");
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for h in history.iter() {
                            let short: String = h.chars().take(60).collect();
                            if ui.selectable_label(false, short).clicked() {
                                *prompt = h.clone();
                            }
                        }
                    });
            }
        });
    panels.ai_scene = open;
}

/// Fenêtre « Optimisation mobile » : actions concrètes pour alléger la scène.
fn optimize_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    actions: &mut UiActions,
) {
    let mut open = panels.optimize;
    egui::Window::new("🪶  Optimisation mobile")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let n_tex = scene
                .objects
                .iter()
                .filter(|o| !o.texture.is_empty())
                .count();
            ui.label(format!(
                "{n_tex} objet(s) texturé(s), {} lumière(s) ponctuelle(s)",
                scene.point_lights.len()
            ));
            ui.separator();
            ui.label("Réduire les textures (côté le plus long) :");
            ui.horizontal(|ui| {
                for max in [1024u32, 2048, 4096] {
                    if ui.button(format!("≤ {max} px")).clicked() {
                        actions.optimize_textures = Some(max);
                    }
                }
            });
            ui.small("Écrit des copies …_optN.png et met à jour les objets (annulable).");
            if ui
                .button("🧱 Convertir en puissances de 2")
                .on_hover_text("Redimensionne les textures en POT (mip-mapping/compression GPU)")
                .clicked()
            {
                actions.convert_textures_pot = true;
            }
            ui.separator();
            if scene.point_lights.len() > 4 && ui.button("Limiter à 4 lumières").clicked() {
                actions.limit_lights = Some(4);
            }
            if !scene.point_lights.is_empty()
                && ui
                    .button("💡 Bake lighting (figer les lumières)")
                    .on_hover_text(
                        "Fige les lumières ponctuelles en émission statique puis les supprime",
                    )
                    .clicked()
            {
                actions.bake_lighting = true;
            }
            ui.separator();
            // Évolutions de rendu non encore implémentées : grisées et explicitées.
            ui.add_enabled(
                false,
                egui::Button::new("🔻 Fusionner les meshes statiques"),
            )
            .on_hover_text("À venir : fusion des géométries statiques (réduction des draw calls)");
            ui.add_enabled(
                false,
                egui::Button::new("📉 Activer LOD / occlusion culling"),
            )
            .on_hover_text(
                "À venir : niveaux de détail et culling d'occlusion (sous-systèmes de rendu)",
            );
            ui.separator();
            if ui
                .button("⚡ Mode performance Android")
                .on_hover_text("Réduit les textures à ≤ 1024 px et limite à 4 lumières en une fois")
                .clicked()
            {
                actions.perf_mode = true;
            }
            ui.small("💡 Astuce : utilise « Contrôle qualité APK » pour vérifier les gains.");
        });
    panels.optimize = open;
}

/// Fenêtre « Gestionnaire de scripts Lua » : liste les objets scriptés, donne un
/// aperçu et permet de sélectionner l'objet (édition dans l'inspecteur).
fn scripts_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &Scene,
    selection: &mut Option<usize>,
    selected: &mut Vec<usize>,
) {
    let mut open = panels.scripts;
    egui::Window::new("📜  Gestionnaire de scripts Lua")
        .open(&mut open)
        .default_size([420.0, 320.0])
        .show(ctx, |ui| {
            let scripted: Vec<usize> = scene
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.script.trim().is_empty())
                .map(|(i, _)| i)
                .collect();
            let total_lines: usize = scripted
                .iter()
                .map(|&i| scene.objects[i].script.lines().count())
                .sum();
            ui.label(format!(
                "{} script(s), {} ligne(s) au total",
                scripted.len(),
                total_lines
            ));
            ui.separator();
            if scripted.is_empty() {
                ui.weak("Aucun script. Sélectionne un objet et écris du Lua dans l'inspecteur.");
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for i in scripted {
                        let obj = &scene.objects[i];
                        let lines = obj.script.lines().count();
                        let header = format!("📜 {} ({lines} l.)", obj.name);
                        let is_sel = *selection == Some(i);
                        if ui.selectable_label(is_sel, header).clicked() {
                            *selection = Some(i);
                            *selected = vec![i];
                        }
                        // Aperçu : première ligne non vide du script.
                        if let Some(first) = obj.script.lines().find(|l| !l.trim().is_empty()) {
                            ui.indent(("preview", i), |ui| {
                                ui.weak(egui::RichText::new(first.trim()).monospace().small());
                            });
                        }
                    }
                });
        });
    panels.scripts = open;
}

/// Fenêtre « Gestionnaire d'assets » : liste les assets du projet + embarqués,
/// permet de rassembler les fichiers externes et d'assigner une texture à la sélection.
fn asset_browser_window(
    ctx: &egui::Context,
    panels: &mut Panels,
    scene: &mut Scene,
    selection: Option<usize>,
    actions: &mut UiActions,
) {
    let mut open = panels.assets;
    egui::Window::new("📁  Gestionnaire d'assets")
        .open(&mut open)
        .default_size([360.0, 320.0])
        .show(ctx, |ui| {
            if ui
                .button("📦 Rassembler les assets du projet")
                .on_hover_text(
                    "Copie les fichiers externes dans ~/.motor3derust/assets et utilise asset://",
                )
                .clicked()
            {
                actions.collect_assets = true;
            }
            ui.separator();
            let assets = crate::assets::list_assets();
            if assets.is_empty() {
                ui.label("Aucun asset. Importe une texture/un modèle, puis « Rassembler ».");
            } else {
                let sel_obj = selection.filter(|&i| i < scene.objects.len());
                ui.label(match sel_obj {
                    Some(_) => "Clique un asset image pour l'appliquer à l'objet sélectionné :",
                    None => "Sélectionne un objet pour assigner une texture.",
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for a in assets {
                            let is_img =
                                a.ends_with(".png") || a.ends_with(".jpg") || a.ends_with(".jpeg");
                            let resp = ui.selectable_label(false, &a);
                            if resp.clicked()
                                && is_img
                                && let Some(i) = sel_obj
                            {
                                scene.objects[i].texture = a.clone();
                            }
                        }
                    });
            }
        });
    panels.assets = open;
}

/// Flash rouge plein écran quand la vie baisse (contact ennemi) : retour immédiat, même
/// sans regarder la barre de vie. `intensity` (1 = pic du coup) décroît vers 0 côté App.
fn damage_vignette(ctx: &egui::Context, area: egui::Rect, intensity: f32) {
    use egui::Color32;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_damage_flash"),
    ));
    let alpha = (70.0 * intensity.clamp(0.0, 1.0)) as u8;
    painter.rect_filled(
        area,
        0.0,
        Color32::from_rgba_unmultiplied(220, 20, 20, alpha),
    );
}

/// Indicateur de manche (haut-centre), pour les scènes à système de manches (cf.
/// `Combat::wave`/`AppState::wave`) — style « Vague N/M » (Call of Zombies). N'affiche
/// rien si `wave == 0` (pas de système de manches dans la scène courante).
/// HUD de l'arme à distance équipée (bas-centre, entre le pavé tank et les
/// boutons tactiles) : libellé + rappel des raccourcis. Texte ASCII/latin
/// uniquement — pas d'emoji, absents de la fonte egui embarquée sur Android
/// (cf. le pavé W/A/S/D : carrés vides constatés sur APK réel, 2026-07-13).
fn weapon_hud(ctx: &egui::Context, area: egui::Rect, label: &str) {
    use egui::{Align2, Color32, FontId};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_weapon"),
    ));
    painter.text(
        egui::pos2(area.center().x, area.bottom() - 34.0),
        Align2::CENTER_CENTER,
        format!("Arme : {label}"),
        FontId::proportional(16.0),
        Color32::from_rgb(255, 170, 80),
    );
    painter.text(
        egui::pos2(area.center().x, area.bottom() - 14.0),
        Align2::CENTER_CENTER,
        "K ou « Feu » : tirer — 1/2/3 ou « Arme » : changer",
        FontId::proportional(11.0),
        Color32::from_white_alpha(150),
    );
}

fn wave_hud(ctx: &egui::Context, area: egui::Rect, scene: &Scene, wave: u32) {
    if wave == 0 {
        return;
    }
    let max_wave = scene
        .objects
        .iter()
        .filter_map(|o| o.combat.as_ref())
        .map(|c| c.wave)
        .max()
        .unwrap_or(0);
    if max_wave == 0 {
        return;
    }
    let remaining = scene
        .objects
        .iter()
        .filter(|o| o.visible && o.combat.as_ref().is_some_and(|c| c.wave == wave))
        .count();
    use egui::{Align2, Color32, FontId};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_wave"),
    ));
    painter.text(
        egui::pos2(area.center().x, area.top() + 22.0),
        Align2::CENTER_CENTER,
        format!("🧟 Vague {wave} / {max_wave}"),
        FontId::proportional(22.0),
        Color32::from_rgb(230, 120, 90),
    );
    painter.text(
        egui::pos2(area.center().x, area.top() + 44.0),
        Align2::CENTER_CENTER,
        format!("{remaining} restant(s)"),
        FontId::proportional(14.0),
        Color32::from_white_alpha(200),
    );
}

/// Barre de vie du HUD (haut de la zone de jeu), pilotée par `set_health` côté script.
fn health_bar(ctx: &egui::Context, area: egui::Rect, h: f32) {
    use egui::{Color32, Stroke};
    let h = h.clamp(0.0, 1.0);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_health"),
    ));
    let w = (area.width() * 0.4).min(220.0);
    let bg = egui::Rect::from_min_size(
        egui::pos2(area.left() + 20.0, area.top() + 16.0),
        egui::vec2(w, 16.0),
    );
    painter.rect_filled(bg, 4.0, Color32::from_black_alpha(140));
    let fill = egui::Rect::from_min_size(bg.min, egui::vec2(w * h, 16.0));
    let col = Color32::from_rgb(((1.0 - h) * 220.0) as u8 + 30, (h * 200.0) as u8 + 30, 50);
    painter.rect_filled(fill, 4.0, col);
    painter.rect_stroke(
        bg,
        4.0,
        Stroke::new(1.5, Color32::from_white_alpha(120)),
        egui::StrokeKind::Inside,
    );
}

/// HUD des collectibles (haut-droite) : « ⭐ ramassés / total », et bannière « Gagné ! »
/// quand tout est ramassé.
fn collectibles_hud(
    ctx: &egui::Context,
    area: egui::Rect,
    collected: usize,
    total: usize,
    time: Option<f32>,
    score: u32,
) {
    use egui::{Align2, Color32, FontId};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_collectibles"),
    ));
    let pos = egui::pos2(area.right() - 20.0, area.top() + 18.0);
    painter.text(
        pos,
        Align2::RIGHT_CENTER,
        format!("⭐ {collected} / {total}"),
        FontId::proportional(20.0),
        Color32::from_rgb(255, 220, 90),
    );
    painter.text(
        egui::pos2(area.right() - 20.0, area.top() + 42.0),
        Align2::RIGHT_CENTER,
        format!("🏆 {score}"),
        FontId::proportional(16.0),
        Color32::from_rgb(150, 220, 255),
    );
    if let Some(t) = time {
        painter.text(
            egui::pos2(area.right() - 20.0, area.top() + 64.0),
            Align2::RIGHT_CENTER,
            format!("⏱ {t:.1}s"),
            FontId::proportional(16.0),
            Color32::from_white_alpha(200),
        );
    }
    if collected == total && total > 0 {
        let msg = match time {
            Some(t) => format!("🎉 Gagné en {t:.1}s !"),
            None => "🎉 Gagné !".to_string(),
        };
        painter.text(
            area.center(),
            Align2::CENTER_CENTER,
            msg,
            FontId::proportional(40.0),
            Color32::from_rgb(120, 230, 140),
        );
    }
}

/// Bannière de défaite « 💀 Perdu ! » au centre de la zone de jeu.
fn lose_banner(ctx: &egui::Context, area: egui::Rect) {
    use egui::{Align2, Color32, FontId};
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hud_lose"),
    ));
    painter.text(
        area.center(),
        Align2::CENTER_CENTER,
        "💀 Perdu !",
        FontId::proportional(44.0),
        Color32::from_rgb(230, 90, 80),
    );
}

/// Bouton tactile « 🔄 Rejouer » centré sous la bannière de fin de partie.
/// Renvoie `true` s'il est cliqué (pour relancer la partie, y compris sur APK).
fn restart_button(ctx: &egui::Context, area: egui::Rect, won: bool) -> bool {
    let mut clicked = false;
    let label = if won {
        "➡ Niveau suivant"
    } else {
        "🔄 Rejouer"
    };
    egui::Area::new("restart_btn".into())
        .fixed_pos(egui::pos2(area.center().x - 85.0, area.center().y + 40.0))
        .show(ctx, |ui| {
            let btn = egui::Button::new(egui::RichText::new(label).size(20.0));
            if ui.add_sized([170.0, 46.0], btn).clicked() {
                clicked = true;
            }
        });
    clicked
}

/// Anneau de retour visuel à l'endroit touché (simulation tactile), dans `area`.
fn touch_feedback(ctx: &egui::Context, area: egui::Rect) {
    use egui::{Color32, Stroke};
    let down = ctx.input(|i| i.pointer.primary_down());
    if !down {
        return;
    }
    let Some(p) = ctx.pointer_interact_pos() else {
        return;
    };
    if !area.contains(p) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("touch_feedback"),
    ));
    painter.circle_stroke(p, 24.0, Stroke::new(3.0, Color32::from_white_alpha(150)));
    painter.circle_filled(p, 7.0, Color32::from_white_alpha(90));
}

/// Dessine les contrôles tactiles (joystick virtuel + boutons) à l'intérieur de
/// `area` et met à jour l'état d'entrée lu par les scripts Lua.
fn mobile_overlay(
    ctx: &egui::Context,
    area: egui::Rect,
    cfg: &crate::scene::MobileControls,
    input: &mut crate::app::PlayerInput,
) {
    use egui::{Color32, Sense, Stroke, Vec2};

    input.joy = (0.0, 0.0);
    input.touch_thrust = 0.0;
    input.touch_turn = 0.0;
    input.buttons.clear();

    // Screen Safe Area : rentre les contrôles dans une marge sûre (encoche/bords).
    let area = if cfg.safe_area {
        let inset = (area.width().min(area.height()) * 0.06).min(28.0);
        area.shrink(inset)
    } else {
        area
    };

    let margin = 32.0;

    // --- Zone tactile plein écran : un tap n'importe où expose input.btn.touch ---
    if cfg.touch_zone {
        let down = ctx.input(|i| i.pointer.primary_down());
        if let Some(p) = ctx.pointer_interact_pos()
            && down
            && area.contains(p)
        {
            input.buttons.insert("touch".to_string());
        }
    }

    // --- Pavé « tank » W/A/S/D (bas-gauche), à la place du joystick si activé :
    // mêmes contrôles que le clavier desktop — W/S avance/recule le long de
    // l'orientation *actuelle* du personnage, A/D le fait pivoter (demandé le
    // 2026-07-13 : « les touches WASD disponibles sur APK et macOS »). L'ancienne
    // croix directionnelle écrivait `input.joy` (déplacement caméra-relatif),
    // un simple doublon discret du joystick — le pavé tank apporte, lui, le
    // second schéma de contrôle du jeu au tactile.
    if cfg.dpad {
        let btn = 56.0;
        let gap = 6.0;
        let size = Vec2::splat(btn * 3.0 + gap * 2.0);
        let pos = egui::pos2(area.left() + margin, area.bottom() - margin - size.y);
        egui::Area::new("mobile_dpad".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let cell = |col: f32, row: f32| {
                    egui::Rect::from_min_size(
                        rect.min + Vec2::new(col * (btn + gap), row * (btn + gap)),
                        Vec2::splat(btn),
                    )
                };
                // Lettres ASCII plutôt que ▲▼◀▶ : les triangles haut/bas manquaient
                // de la fonte embarquée sur Android — carrés vides constatés sur
                // l'APK réel (capture d'écran utilisateur, 2026-07-13).
                let up = ui.put(cell(1.0, 0.0), egui::Button::new("W").corner_radius(10.0));
                let left = ui.put(cell(0.0, 1.0), egui::Button::new("A").corner_radius(10.0));
                let right = ui.put(cell(2.0, 1.0), egui::Button::new("D").corner_radius(10.0));
                let down = ui.put(cell(1.0, 2.0), egui::Button::new("S").corner_radius(10.0));

                let mut thrust = 0.0f32;
                let mut turn = 0.0f32;
                if up.is_pointer_button_down_on() {
                    thrust += 1.0;
                }
                if down.is_pointer_button_down_on() {
                    thrust -= 1.0;
                }
                // Mêmes signes que le clavier (cf. `lib.rs` : `key_turn =
                // axis_from_held(a, d)`) : A = -1, D = +1.
                if left.is_pointer_button_down_on() {
                    turn -= 1.0;
                }
                if right.is_pointer_button_down_on() {
                    turn += 1.0;
                }
                // Canaux tactiles dédiés (cf. `PlayerInput::thrust`/`turn`) :
                // réécrits chaque frame (0 au relâchement), cumulés avec le
                // clavier sans jamais écraser son état, tenu par événements.
                input.touch_thrust = thrust;
                input.touch_turn = turn;
            });
    } else if cfg.joystick {
        let radius = 55.0;
        let pos = egui::pos2(area.left() + margin, area.bottom() - margin - radius * 2.0);
        egui::Area::new("mobile_joystick".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0), Sense::drag());
                let center = rect.center();
                let painter = ui.painter();
                painter.circle_filled(center, radius, Color32::from_black_alpha(110));
                painter.circle_stroke(
                    center,
                    radius,
                    Stroke::new(2.0, Color32::from_white_alpha(120)),
                );
                let mut knob = center;
                if let Some(p) = resp.interact_pointer_pos() {
                    let mut off = p - center;
                    if off.length() > radius {
                        off = off.normalized() * radius;
                    }
                    knob = center + off;
                    input.joy = (off.x / radius, -off.y / radius); // y inversé : haut = +1
                }
                painter.circle_filled(knob, 22.0, Color32::from_white_alpha(200));
            });
    }

    // --- Boutons (bas-droite de la zone de jeu) ---
    if !cfg.buttons.is_empty() {
        let btn = 64.0;
        let spacing = 8.0;
        let width = cfg.buttons.len() as f32 * (btn + spacing);
        let pos = egui::pos2(area.right() - margin - width, area.bottom() - margin - btn);
        egui::Area::new("mobile_buttons".into())
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for name in &cfg.buttons {
                        let resp =
                            ui.add_sized([btn, btn], egui::Button::new(name).corner_radius(32.0));
                        // Bouton « maintenu » : actif tant que le pointeur est enfoncé dessus.
                        if resp.is_pointer_button_down_on() {
                            input.buttons.insert(name.clone());
                        }
                    }
                });
            });
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
    actions: &mut UiActions,
) {
    // Fenêtre « Paramètres » (clé API DeepSeek…).
    settings_window(root.ctx(), panels, settings);
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
    optimize_window(root.ctx(), panels, scene, actions);
    asset_browser_window(root.ctx(), panels, scene, *selection, actions);
    scripts_window(root.ctx(), panels, scene, selection, selected);

    // Fenêtre flottante « Build & Export » (Sprint 19).
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
                // Pas unique (Sprint 81) : n'a de sens qu'en pause.
                if ui
                    .add_enabled(*paused, egui::Button::new("⏭"))
                    .on_hover_text("Avancer d'un pas fixe")
                    .clicked()
                {
                    actions.step_frame = true;
                }
            }
            ui.separator();
            // Time scale (Sprint 81) : ralenti/accélère la simulation pour déboguer la
            // physique et le réseau. Préréglages + valeur affichée plutôt qu'un slider :
            // les valeurs qui comptent en pratique sont peu nombreuses (figé/ralenti/normal/rapide).
            ui.label("⏱");
            for (label, value) in [("¼×", 0.25), ("½×", 0.5), ("1×", 1.0), ("2×", 2.0)] {
                if ui.selectable_label(*time_scale == value, label).clicked() {
                    *time_scale = value;
                }
            }
            ui.separator();
            ui.selectable_value(gizmo_mode, GizmoMode::Translate, "↔ Déplacer");
            ui.selectable_value(gizmo_mode, GizmoMode::Rotate, "↻ Tourner");
            ui.selectable_value(gizmo_mode, GizmoMode::Scale, "⤢ Redim.");
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
                .on_hover_text("Aimanter les déplacements à la grille (pas 0.5)")
                .clicked()
            {
                actions.toggle_snap = true;
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
                                #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
                        });
                        if obj.physics != PhysicsKind::None {
                            ui.horizontal(|ui| {
                                use crate::runtime::physics::ColliderShape as Cs;
                                ui.label("Collider");
                                ui.selectable_value(&mut obj.collider_shape, Cs::Auto, "Auto");
                                ui.selectable_value(&mut obj.collider_shape, Cs::Box, "Box");
                                ui.selectable_value(&mut obj.collider_shape, Cs::Sphere, "Sphère");
                                ui.selectable_value(&mut obj.collider_shape, Cs::Capsule, "Capsule");
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
                                ui.add(egui::Slider::new(&mut obj.metallic, 0.0..=1.0));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Rugosité");
                                ui.add(egui::Slider::new(&mut obj.roughness, 0.04..=1.0));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Émission");
                                ui.add(egui::Slider::new(&mut obj.emissive, 0.0..=3.0));
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
                                    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    if let Some(h) = hud_health.or_else(|| scene.mobile.health_bar.then_some(1.0)) {
        health_bar(root.ctx(), play_rect, h);
    }
    if *playing {
        wave_hud(root.ctx(), play_rect, scene, wave);
        weapon_hud(root.ctx(), play_rect, weapon_label);
    }
    if *playing && let Some((c, t)) = scene.collectibles() {
        collectibles_hud(root.ctx(), play_rect, c, t, game_time, score);
    }
    if *playing && lost {
        lose_banner(root.ctx(), play_rect);
    }
    // Fin de partie : bouton « Rejouer » (preview éditeur, comme sur APK).
    if *playing && (won || lost) && restart_button(root.ctx(), play_rect, won) {
        actions.restart = true;
    }
    if *playing && scene.mobile.any() {
        mobile_overlay(root.ctx(), play_rect, &scene.mobile, input_state);
    } else {
        input_state.joy = (0.0, 0.0);
        input_state.buttons.clear();
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

    // rotation éditée en degrés via les angles d'Euler
    let (mut rx, mut ry, mut rz) = t.rotation.to_euler(EulerRot::XYZ);
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
