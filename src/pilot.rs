//! Pont de pilotage externe (« pilot ») : un petit serveur TCP localhost qui
//! permet de piloter l'application vivante depuis l'extérieur — console
//! développeur, éval Lua, lecture des logs, capture d'écran, inspection de la
//! scène, injection d'entrées joueur. Pensé pour les audits automatisés
//! (agent/CI) : le binaire GUI n'est pas pilotable par un outil de contrôle
//! d'écran (fenêtre winit/wgpu sans arbre d'accessibilité), ce pont est le
//! canal sémantique qui le remplace.
//!
//! Sécurité : **jamais actif par défaut** (l'éval Lua est de l'exécution de code
//! arbitraire). Activation explicite par `--pilot[=PORT]` ou `RUSTEEGEAR_PILOT`,
//! écoute sur `127.0.0.1` uniquement.
//!
//! Protocole : une requête JSON par ligne, une réponse JSON par ligne.
//! Requête : `{"cmd": "console"|"lua"|"logs"|"scene"|"state"|"input"|"screenshot"
//! |"player"|"camera"|"object"|"scene_cmd"|"options"|"net", ...}`.
//! Réponse : `{"ok": true, "result": ...}` ou `{"ok": false, "error": "..."}`.
//! Client de référence : le binaire `pilot` (`src/bin/pilot.rs`) ; référence
//! complète des verbes et exemples : `docs/PILOT.md`.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::time::Duration;

use crate::app::AppState;
use crate::gfx::renderer::Renderer;

/// Port d'écoute par défaut (0 = port éphémère choisi par l'OS, utilisé par les tests).
pub const DEFAULT_PORT: u16 = 4517;

/// Délai maximum d'attente d'une réponse du thread principal par une connexion.
/// Large : le thread principal draine à chaque tour de boucle (60 ms au repos),
/// mais un chargement de scène ou une capture peuvent prendre plus longtemps.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Nombre maximum de requêtes traitées par appel à [`PilotServer::poll`] : un
/// client qui inonde le pont ne doit pas monopoliser le thread principal au
/// détriment du rendu/de la simulation — l'excédent attend simplement le tour
/// de boucle suivant (le [`REPLY_TIMEOUT`] côté connexion couvre largement).
const MAX_REQUESTS_PER_POLL: usize = 32;

/// Réveille la boucle d'événements quand une requête arrive : au repos, winit
/// dort jusqu'à 60 ms entre deux tours (`ControlFlow::wait_duration`, cf.
/// `about_to_wait`) — sans coup de coude, chaque commande pilot paierait cette
/// latence. Branché sur un `EventLoopProxy` par `run()` ; `None` dans les tests,
/// qui pompent `poll` en continu.
pub type Waker = Box<dyn Fn() + Send + Sync>;

/// Une requête reçue sur une connexion, en attente de traitement sur le thread
/// principal (seul détenteur de `AppState`/`Renderer`).
struct PilotRequest {
    line: String,
    reply: SyncSender<String>,
}

/// Serveur de pilotage : accepte les connexions sur un thread dédié, mais tout
/// le traitement a lieu sur le thread appelant de [`PilotServer::poll`] — aucun
/// état applicatif n'est partagé entre threads, seules les lignes de texte
/// transitent par le canal.
pub struct PilotServer {
    rx: Receiver<PilotRequest>,
    /// Adresse réellement liée (utile avec le port 0 des tests).
    pub local_addr: std::net::SocketAddr,
}

impl PilotServer {
    /// Lie `127.0.0.1:port` et démarre le thread accepteur. `Err` si le port est
    /// occupé (message explicite : deux instances `--pilot` simultanées).
    /// `waker` : appelé à chaque requête reçue pour réveiller la boucle
    /// d'événements (cf. [`Waker`]) — `None` si personne ne dort (tests).
    pub fn start(port: u16, waker: Option<Waker>) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("pilot : liaison 127.0.0.1:{port} impossible ({e})"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| format!("pilot : adresse locale illisible ({e})"))?;
        let (tx, rx) = std::sync::mpsc::channel::<PilotRequest>();
        let waker = waker.map(std::sync::Arc::new);
        // Tick de réveil à ~30 Hz tant que le pont est actif : masquée ou
        // occultée, l'application est mise en App Nap par macOS (plus de
        // redraws, boucle d'événements endormie entre deux requêtes) — la
        // simulation, les chargements asynchrones et le réseau gelaient alors
        // que le pont répondait (audit du 19 juillet 2026). Chaque coup de
        // coude force un tour de `about_to_wait`, dont le garde-fou « rendu
        // muet > 100 ms » fait avancer le jeu. Coût : ~30 réveils/s, uniquement
        // en mode pilotage (opt-in) — négligeable pour un outil de dev.
        if let Some(tick_waker) = waker.clone() {
            let _ = std::thread::Builder::new()
                .name("pilot-tick".into())
                .spawn(move || {
                    loop {
                        tick_waker();
                        std::thread::sleep(Duration::from_millis(33));
                    }
                });
        }
        std::thread::Builder::new()
            .name("pilot-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let tx = tx.clone();
                    let waker = waker.clone();
                    let _ = std::thread::Builder::new()
                        .name("pilot-conn".into())
                        .spawn(move || serve_connection(stream, tx, waker));
                }
            })
            .map_err(|e| format!("pilot : démarrage du thread accepteur impossible ({e})"))?;
        Ok(Self { rx, local_addr })
    }

    /// Draine et traite toutes les requêtes en attente — à appeler à chaque tour
    /// de boucle depuis le thread qui possède `AppState` (et le `Renderer` s'il
    /// existe : `None` en headless, la commande `screenshot` répond alors une
    /// erreur explicite au lieu d'échouer silencieusement).
    pub fn poll(&self, app: &mut AppState, mut renderer: Option<&mut Renderer>) {
        for _ in 0..MAX_REQUESTS_PER_POLL {
            let Ok(req) = self.rx.try_recv() else { break };
            let response = handle_request(&req.line, app, renderer.as_deref_mut());
            // Connexion déjà fermée côté client : réponse perdue, sans gravité.
            let _ = req.reply.send(response);
        }
    }
}

/// Boucle d'une connexion : lit ligne à ligne, fait traiter chaque requête par le
/// thread principal via le canal, renvoie la réponse. Une erreur d'E/S ou la fin
/// du flux termine la connexion (le thread meurt proprement).
fn serve_connection(
    stream: TcpStream,
    tx: Sender<PilotRequest>,
    waker: Option<std::sync::Arc<Waker>>,
) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        if tx
            .send(PilotRequest {
                line,
                reply: reply_tx,
            })
            .is_err()
        {
            // Le récepteur (l'application) n'existe plus : plus rien à servir.
            break;
        }
        // Requête déposée : coup de coude à la boucle d'événements pour qu'elle
        // la traite tout de suite au lieu d'attendre son prochain réveil.
        if let Some(wake) = waker.as_deref() {
            wake();
        }
        let response = reply_rx.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|_| {
            serde_json::json!({
                "ok": false,
                "error": "délai dépassé : le thread principal n'a pas traité la requête \
                          (application bloquée ou fermée ?)"
            })
            .to_string()
        });
        if writeln!(writer, "{response}").is_err() {
            break;
        }
    }
}

/// Traite une ligne de requête et fabrique la ligne de réponse — toujours du JSON
/// valide, jamais de panique sur une entrée malformée (même contrat que
/// `run_console_command`).
fn handle_request(line: &str, app: &mut AppState, renderer: Option<&mut Renderer>) -> String {
    match dispatch(line, app, renderer) {
        Ok(result) => serde_json::json!({"ok": true, "result": result}).to_string(),
        Err(error) => serde_json::json!({"ok": false, "error": error}).to_string(),
    }
}

fn dispatch(
    line: &str,
    app: &mut AppState,
    renderer: Option<&mut Renderer>,
) -> Result<serde_json::Value, String> {
    let req: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("requête JSON invalide : {e}"))?;
    let cmd = req
        .get("cmd")
        .and_then(|c| c.as_str())
        .ok_or("champ `cmd` manquant (chaîne attendue)")?;
    let str_field = |name: &str| -> Result<&str, String> {
        req.get(name)
            .and_then(|v| v.as_str())
            .ok_or(format!("commande `{cmd}` : champ `{name}` manquant"))
    };
    match cmd {
        // Même surface que la fenêtre Console de l'éditeur.
        "console" => Ok(serde_json::json!(
            app.run_console_command(str_field("arg")?)
        )),
        "lua" => app
            .eval_lua(str_field("src")?)
            .map(serde_json::Value::String),
        "logs" => Ok(serde_json::json!(crate::log_buffer::snapshot())),
        "scene" => Ok(scene_dump(app)),
        "state" => Ok(state_dump(app)),
        "input" => {
            let f = |name: &str| req.get(name).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let b = |name: &str| req.get(name).and_then(|v| v.as_bool()).unwrap_or(false);
            // Assignation absolue (les champs absents retombent à neutre) : chaque
            // requête décrit l'état complet des entrées, comme une manette — pas
            // d'accumulation d'appuis fantômes entre deux requêtes.
            let inp = &mut app.input_state;
            inp.key_turn = f("turn").clamp(-1.0, 1.0);
            inp.key_thrust = f("thrust").clamp(-1.0, 1.0);
            inp.key_move = (f("mx").clamp(-1.0, 1.0), f("my").clamp(-1.0, 1.0));
            inp.jump = b("jump");
            inp.attack = b("attack");
            inp.fire = b("fire");
            inp.heal = b("heal");
            // `hold_ms` : tient ces entrées pendant l'équivalent de `hold_ms`
            // millisecondes de pas fixes **simulés immédiatement**
            // (`advance_steps`), puis relâche tout. Temps simulé plutôt que
            // temps réel : déterministe, instantané, et insensible à l'App Nap
            // (fenêtre masquée, le temps réel du process est étranglé par
            // macOS — c'est le geste `move` du client CLI).
            if let Some(hold_ms) = req.get("hold_ms").and_then(|v| v.as_u64()) {
                if !app.playing {
                    return Err("input hold_ms : uniquement en Play (console play d'abord)".into());
                }
                let steps = ((hold_ms.min(30_000) as f32 / 1000.0) * 60.0).round() as u32;
                let before = app.player_position();
                app.advance_steps(steps.max(1));
                app.input_state = Default::default();
                let pos = app.player_position();
                return Ok(serde_json::json!({
                    "steps": steps.max(1),
                    "physics": app.physics_ready(),
                    "player_pos_before": before.map(|p| [p.x, p.y, p.z]),
                    "player_pos": pos.map(|p| [p.x, p.y, p.z]),
                }));
            }
            Ok(serde_json::json!(format!(
                "entrées posées : turn={:.2} thrust={:.2} jump={} attack={} fire={} heal={}",
                inp.key_turn, inp.key_thrust, inp.jump, inp.attack, inp.fire, inp.heal
            )))
        }
        "screenshot" => {
            let Some(renderer) = renderer else {
                return Err("capture impossible : pas de renderer (exécution headless)".into());
            };
            let path = str_field("path")?;
            let u = |name: &str, default: u32| {
                req.get(name)
                    .and_then(|v| v.as_u64())
                    .map(|v| (v as u32).clamp(16, 4096))
                    .unwrap_or(default)
            };
            let (width, height) = (u("width", 800), u("height", 600));
            renderer.screenshot_png(app, width, height, std::path::Path::new(path))?;
            Ok(serde_json::json!(format!("{path} ({width}×{height})")))
        }
        "player" => Ok(player_dump(app)),
        // Pose de caméra : écrit les champs fournis, renvoie toujours la pose
        // résultante. `follow` pilote `scene.camera_follow` — indispensable pour
        // cadrer librement pendant Play (le suivi réécrit la caméra chaque frame).
        "camera" => {
            if let Some(t) = req.get("target").and_then(|v| v.as_array()) {
                let xyz: Vec<f32> = t
                    .iter()
                    .filter_map(|v| v.as_f64())
                    .map(|v| v as f32)
                    .collect();
                if xyz.len() != 3 {
                    return Err("camera : `target` doit être [x, y, z]".into());
                }
                app.camera.target = glam::Vec3::new(xyz[0], xyz[1], xyz[2]);
            }
            if let Some(v) = req.get("yaw").and_then(|v| v.as_f64()) {
                app.camera.yaw = (v as f32).to_radians();
            }
            if let Some(v) = req.get("pitch").and_then(|v| v.as_f64()) {
                app.camera.pitch = (v as f32).to_radians();
            }
            if let Some(v) = req.get("distance").and_then(|v| v.as_f64()) {
                app.camera.distance = (v as f32).max(0.1);
            }
            if let Some(v) = req.get("follow").and_then(|v| v.as_bool()) {
                app.scene.camera_follow = v;
            }
            if req.get("frame").and_then(|v| v.as_bool()).unwrap_or(false) {
                app.frame_selected();
            }
            Ok(camera_dump(app))
        }
        "object" => object_cmd(&req, app),
        "scene_cmd" => match str_field("op")? {
            "save" => match req.get("path").and_then(|v| v.as_str()) {
                Some(path) => {
                    app.save_to(path);
                    Ok(serde_json::json!(format!("scène sauvée : {path}")))
                }
                None => {
                    app.save();
                    Ok(serde_json::json!("scène sauvée (chemin par défaut)"))
                }
            },
            "load" => {
                let path = str_field("path")?;
                // Synchrone (pas `load_from`, dont le thread de fond peut être
                // étranglé plusieurs secondes par l'App Nap fenêtre masquée) :
                // un pilote a besoin d'une réponse « chargée » fiable, tout de
                // suite — ~15 ms pour la scène du hameau, négligeable.
                let count = app.load_from_blocking(path)?;
                Ok(serde_json::json!(format!(
                    "scène chargée : {path} ({count} objets)"
                )))
            }
            "new" => {
                app.new_scene();
                Ok(serde_json::json!("scène vide créée"))
            }
            other => Err(format!(
                "scene_cmd : op inconnue « {other} » — save, load, new"
            )),
        },
        // Options de jeu : chaque champ présent est appliqué. Les toggles UI
        // (`hud`, `map`, `settings_overlay`, `multiplayer_window`) passent par
        // l'éditeur egui, donc exigent un renderer (fenêtre réelle).
        "options" => {
            let mut applied: Vec<String> = Vec::new();
            if let Some(v) = req.get("music").and_then(|v| v.as_f64()) {
                app.set_music_volume((v as f32).clamp(0.0, 1.0));
                applied.push(format!("music={v:.2}"));
            }
            if let Some(v) = req.get("sfx").and_then(|v| v.as_f64()) {
                app.set_sfx_volume((v as f32).clamp(0.0, 1.0));
                applied.push(format!("sfx={v:.2}"));
            }
            if let Some(v) = req.get("timescale").and_then(|v| v.as_f64()) {
                app.time_scale = (v as f32).clamp(0.0, 8.0);
                applied.push(format!("timescale={:.2}", app.time_scale));
            }
            if let Some(v) = req.get("reduce_shake").and_then(|v| v.as_bool()) {
                app.set_reduce_shake(v);
                applied.push(format!("reduce_shake={v}"));
            }
            let mut renderer = renderer;
            for toggle in ["hud", "map", "settings_overlay", "multiplayer_window"] {
                if req.get(toggle).and_then(|v| v.as_bool()).unwrap_or(false) {
                    let Some(r) = renderer.as_deref_mut() else {
                        return Err(format!(
                            "options : le toggle `{toggle}` exige un renderer (fenêtre réelle, \
                             indisponible en headless)"
                        ));
                    };
                    match toggle {
                        "hud" => r.toggle_play_hud(),
                        "map" => r.toggle_player_map(),
                        "settings_overlay" => r.toggle_player_settings(),
                        _ => r.toggle_multiplayer_window(),
                    }
                    applied.push(format!("{toggle} basculé"));
                }
            }
            if applied.is_empty() {
                return Err("options : aucun champ reconnu — music, sfx, timescale, \
                            reduce_shake, hud, map, settings_overlay, multiplayer_window"
                    .into());
            }
            Ok(serde_json::json!(applied.join(", ")))
        }
        "net" => match str_field("op")? {
            "connect" => {
                let url = str_field("url")?;
                let name = req.get("name").and_then(|v| v.as_str()).unwrap_or("Pilot");
                let class = match req
                    .get("class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("assaut")
                {
                    "assaut" | "assault" => crate::app::multiplayer::PlayerClass::Assault,
                    "eclaireur" | "scout" => crate::app::multiplayer::PlayerClass::Scout,
                    "soutien" | "support" => crate::app::multiplayer::PlayerClass::Support,
                    other => return Err(format!("net : classe inconnue « {other} »")),
                };
                let objective = match req
                    .get("objective")
                    .and_then(|v| v.as_str())
                    .unwrap_or("vagues")
                {
                    "vagues" => crate::app::multiplayer::RoundObjective::Vagues,
                    "survie" => crate::app::multiplayer::RoundObjective::Survie,
                    "escorte" => crate::app::multiplayer::RoundObjective::Escorte,
                    "boss" => crate::app::multiplayer::RoundObjective::Boss,
                    other => return Err(format!("net : objectif inconnu « {other} »")),
                };
                let room = req.get("room").and_then(|v| v.as_str()).unwrap_or("");
                app.connect_to_server_as(url, name, class, room, objective);
                Ok(serde_json::json!(format!(
                    "connexion à {url} lancée ({name})"
                )))
            }
            "disconnect" => {
                app.disconnect_from_server();
                Ok(serde_json::json!("déconnecté"))
            }
            other => Err(format!(
                "net : op inconnue « {other} » — connect, disconnect"
            )),
        },
        other => Err(format!(
            "commande inconnue : « {other} » — console, lua, logs, scene, state, input, \
             screenshot, player, camera, object, scene_cmd, options, net"
        )),
    }
}

/// Sous-commandes `object` : créer, lire, modifier, dupliquer, supprimer,
/// blesser un objet — les mêmes gestes que l'Inspecteur/menu Ajouter de
/// l'éditeur, avec `push_undo` avant toute mutation (annulable au même titre).
fn object_cmd(req: &serde_json::Value, app: &mut AppState) -> Result<serde_json::Value, String> {
    let op = req
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or("object : champ `op` manquant — add, import, get, set, delete, duplicate, damage")?;
    let index_field = || -> Result<usize, String> {
        let i = req
            .get("index")
            .and_then(|v| v.as_u64())
            .ok_or("object : champ `index` manquant")? as usize;
        if i >= app.scene.objects.len() {
            return Err(format!(
                "object : index {i} hors limites ({} objets)",
                app.scene.objects.len()
            ));
        }
        Ok(i)
    };
    match op {
        "add" => {
            use crate::scene::MeshKind;
            let kind = match req.get("kind").and_then(|v| v.as_str()).unwrap_or("cube") {
                "cube" => MeshKind::Cube,
                "sphere" => MeshKind::Sphere,
                "plane" | "plan" => MeshKind::Plane,
                "cylinder" | "cylindre" => MeshKind::Cylinder,
                "capsule" => MeshKind::Capsule,
                "terrain" => MeshKind::Terrain,
                other => {
                    return Err(format!(
                        "object add : forme inconnue « {other} » — cube, sphere, plane, \
                         cylinder, capsule, terrain"
                    ));
                }
            };
            app.add_object(kind);
            let index = app.scene.objects.len() - 1;
            Ok(serde_json::json!({
                "index": index,
                "name": app.scene.objects[index].name,
            }))
        }
        "import" => {
            let path = req
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("object import : champ `path` manquant")?;
            app.import_gltf(path);
            Ok(serde_json::json!(format!(
                "import de {path} lancé (asynchrone — vérifier via `scene`)"
            )))
        }
        "get" => {
            let i = index_field()?;
            serde_json::to_value(&app.scene.objects[i])
                .map_err(|e| format!("object get : sérialisation impossible ({e})"))
        }
        "set" => {
            let i = index_field()?;
            let patch = req
                .get("patch")
                .and_then(|v| v.as_object())
                .ok_or("object set : champ `patch` manquant (objet JSON attendu)")?;
            apply_object_patch(app, i, patch)
        }
        "delete" => {
            let i = index_field()?;
            let name = app.scene.objects[i].name.clone();
            app.delete_object(i);
            Ok(serde_json::json!(format!("« {name} » supprimé")))
        }
        "duplicate" => {
            let i = index_field()?;
            app.selection = Some(i);
            app.duplicate_selected();
            Ok(serde_json::json!({ "index": app.scene.objects.len() - 1 }))
        }
        "damage" => {
            let i = index_field()?;
            let amount = req.get("amount").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let name = app.scene.objects[i].name.clone();
            if !app
                .scene
                .objects
                .get(i)
                .and_then(|o| o.combat.as_ref())
                .is_some_and(|c| c.attackable)
            {
                return Err(format!("object damage : « {name} » n'est pas attaquable"));
            }
            let killed = app.scene.damage_attackable_by(i, amount);
            Ok(serde_json::json!({ "name": name, "killed": killed }))
        }
        other => Err(format!(
            "object : op inconnue « {other} » — add, import, get, set, delete, duplicate, damage"
        )),
    }
}

/// Applique un patch JSON champ à champ sur `scene.objects[i]` — un `push_undo`
/// avant (annulable comme une édition à l'Inspecteur), `scene_dirty` après.
/// En Play, un champ physique (`pos`, `physics`) est appliqué mais signalé :
/// le corps rigide construit à l'entrée de Play peut écraser la pose.
fn apply_object_patch(
    app: &mut AppState,
    i: usize,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let vec3 = |v: &serde_json::Value, field: &str| -> Result<glam::Vec3, String> {
        let xyz: Vec<f32> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| x as f32)
                    .collect()
            })
            .unwrap_or_default();
        if xyz.len() != 3 {
            return Err(format!("object set : `{field}` doit être [x, y, z]"));
        }
        Ok(glam::Vec3::new(xyz[0], xyz[1], xyz[2]))
    };
    app.push_undo();
    let mut applied: Vec<String> = Vec::new();
    let mut physics_touched = false;
    for (key, value) in patch {
        let obj = &mut app.scene.objects[i];
        match key.as_str() {
            "name" => {
                obj.name = value
                    .as_str()
                    .ok_or("object set : `name` doit être une chaîne")?
                    .to_string()
            }
            "pos" => {
                obj.transform.position = vec3(value, "pos")?;
                physics_touched = true;
            }
            "rot_deg" => {
                let r = vec3(value, "rot_deg")?;
                obj.transform.rotation = glam::Quat::from_euler(
                    glam::EulerRot::XYZ,
                    r.x.to_radians(),
                    r.y.to_radians(),
                    r.z.to_radians(),
                );
            }
            "scale" => obj.transform.scale = vec3(value, "scale")?,
            "color" => {
                let c = vec3(value, "color")?;
                obj.color = [c.x, c.y, c.z];
            }
            "visible" => {
                obj.visible = value
                    .as_bool()
                    .ok_or("object set : `visible` doit être un booléen")?
            }
            "tag" => {
                obj.tag = value
                    .as_str()
                    .ok_or("object set : `tag` doit être une chaîne")?
                    .to_string()
            }
            "script" => {
                obj.script = value
                    .as_str()
                    .ok_or("object set : `script` doit être une chaîne")?
                    .to_string()
            }
            "metallic" => {
                obj.metallic = value
                    .as_f64()
                    .ok_or("object set : `metallic` doit être un nombre")?
                    as f32
            }
            "roughness" => {
                obj.roughness = value
                    .as_f64()
                    .ok_or("object set : `roughness` doit être un nombre")?
                    as f32
            }
            "emissive" => {
                obj.emissive = value
                    .as_f64()
                    .ok_or("object set : `emissive` doit être un nombre")?
                    as f32
            }
            "physics" => {
                use crate::runtime::physics::PhysicsKind;
                obj.physics = match value.as_str().unwrap_or_default() {
                    "none" => PhysicsKind::None,
                    "static" => PhysicsKind::Static,
                    "dynamic" => PhysicsKind::Dynamic,
                    "kinematic" => PhysicsKind::Kinematic,
                    other => {
                        return Err(format!(
                            "object set : physics inconnue « {other} » — none, static, \
                             dynamic, kinematic"
                        ));
                    }
                };
                physics_touched = true;
            }
            "hp" => {
                let hp = value
                    .as_u64()
                    .ok_or("object set : `hp` doit être un entier")?
                    as u32;
                let Some(combat) = obj.combat.as_mut() else {
                    return Err(format!(
                        "object set : « {} » n'a pas de composant combat (hp inapplicable)",
                        obj.name
                    ));
                };
                combat.hp = hp;
            }
            other => {
                return Err(format!(
                    "object set : champ inconnu « {other} » — name, pos, rot_deg, scale, color, \
                 visible, tag, script, metallic, roughness, emissive, physics, hp"
                ));
            }
        }
        applied.push(key.clone());
    }
    app.scene_dirty = true;
    let mut msg = format!("champs appliqués : {}", applied.join(", "));
    if physics_touched && app.playing {
        msg.push_str(
            " — ⚠ Play actif : la physique a été construite à l'entrée en Play, \
             un corps rigide peut écraser cette pose (éditer hors Play pour un effet garanti)",
        );
    }
    Ok(serde_json::json!(msg))
}

/// Lecture « joueur » : tout ce qu'un audit veut savoir après une action.
fn player_dump(app: &AppState) -> serde_json::Value {
    serde_json::json!({
        "index": app.player_index(),
        "pos": app.player_position().map(|p| [p.x, p.y, p.z]),
        "weapon": app.selected_weapon_label(),
        "weapon_index": app.selected_weapon(),
        "health": app.hud_health,
        "score": app.score(),
        "wave": app.wave,
        "won": app.has_won(),
        "lost": app.is_lost(),
        "timer": app.hud_timer(),
    })
}

fn camera_dump(app: &AppState) -> serde_json::Value {
    let c = &app.camera;
    serde_json::json!({
        "target": [c.target.x, c.target.y, c.target.z],
        "yaw_deg": c.yaw.to_degrees(),
        "pitch_deg": c.pitch.to_degrees(),
        "distance": c.distance,
        "follow": app.scene.camera_follow,
    })
}

/// Dump compact de la scène : de quoi vérifier « qu'est-ce qui existe, où, dans
/// quel état » sans transférer les meshes ni les scripts complets.
fn scene_dump(app: &AppState) -> serde_json::Value {
    let objects: Vec<serde_json::Value> = app
        .scene
        .objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let p = o.transform.position;
            serde_json::json!({
                "index": i,
                "name": o.name,
                "pos": [p.x, p.y, p.z],
                "visible": o.visible,
                "tag": o.tag,
                "scripted": !o.script.is_empty(),
            })
        })
        .collect();
    serde_json::json!(objects)
}

fn state_dump(app: &AppState) -> serde_json::Value {
    serde_json::json!({
        "playing": app.playing,
        "paused": app.paused,
        "time_scale": app.time_scale,
        "objects": app.scene.objects.len(),
        "selection": app.selection,
        "hud_health": app.hud_health,
        "connected": app.is_connected(),
        "player": app.player,
        "player_pos": app.player_position().map(|p| [p.x, p.y, p.z]),
        "weapon": app.selected_weapon_label(),
        "score": app.score(),
        "wave": app.wave,
        "won": app.has_won(),
        "lost": app.is_lost(),
        "camera": camera_dump(app),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toute entrée malformée doit produire une réponse JSON `ok: false` avec un
    /// message explicite — jamais une panique ni une ligne non-JSON (le client en
    /// face parse chaque ligne).
    #[test]
    fn malformed_requests_get_an_explicit_json_error() {
        let mut app = AppState::new();
        for bad in [
            "pas du json",
            "{}",
            r#"{"cmd": "frobnicate"}"#,
            r#"{"cmd": 3}"#,
        ] {
            let resp: serde_json::Value =
                serde_json::from_str(&handle_request(bad, &mut app, None)).unwrap();
            assert_eq!(resp["ok"], false, "entrée : {bad}");
            assert!(
                resp["error"].as_str().is_some_and(|e| !e.is_empty()),
                "entrée : {bad}"
            );
        }
    }

    #[test]
    fn screenshot_without_a_renderer_reports_headless_instead_of_failing_silently() {
        let mut app = AppState::new();
        let resp: serde_json::Value = serde_json::from_str(&handle_request(
            r#"{"cmd": "screenshot", "path": "/tmp/x.png"}"#,
            &mut app,
            None,
        ))
        .unwrap();
        assert_eq!(resp["ok"], false);
        assert!(resp["error"].as_str().unwrap().contains("headless"));
    }
}
