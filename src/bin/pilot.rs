//! Client CLI du pont de pilotage (`motor3derust::pilot`) : traduit des verbes
//! shell en requêtes JSON-lines vers l'application lancée avec `--pilot`, et
//! affiche la réponse. Interface pensée pour un agent (Claude) ou un script :
//! code de sortie 0 = `ok: true`, 1 = erreur applicative, 2 = usage.
//!
//! Exemples :
//! ```text
//! pilot console "play"
//! pilot move 0 1 800 fire          # avance + tire pendant 800 ms, puis relâche
//! pilot player
//! pilot camera target 0 5 0 yaw 45 follow off
//! pilot object add cube
//! pilot object set 3 color 1 0 0
//! pilot scene demo zombies
//! pilot options music 0.5
//! pilot net connect ws://127.0.0.1:7777 Pseudo
//! pilot screenshot /tmp/capture.png 800 600
//! ```

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::ExitCode;

const USAGE: &str = "usage : pilot [--port N] <verbe> [args…]\n\
  état      : state | player | scene | logs\n\
  jeu       : console <cmd> | step [n] | weapon <0-2> | demo <nom> | undo | redo\n\
  joueur    : input <turn> <thrust> [jump] [attack] [fire] [heal]\n\
  \u{20}           move <turn> <thrust> [ms] [jump] [attack] [fire]  (relâche après ms, défaut 500)\n\
  caméra    : camera [target x y z] [yaw °] [pitch °] [distance d] [follow on|off] [frame]\n\
  objets    : object add <cube|sphere|plane|cylinder|capsule|terrain>\n\
  \u{20}           object import <chemin.glb> | object get <i> | object delete <i>\n\
  \u{20}           object duplicate <i> | object damage <i> [n]\n\
  \u{20}           object set <i> <champ> <valeurs…>  (pos/rot/scale/color x y z ;\n\
  \u{20}             visible on|off ; physics none|static|dynamic|kinematic ;\n\
  \u{20}             metallic/roughness/emissive v ; hp n ; name/tag/script texte)\n\
  scène     : scene save [chemin] | scene load <chemin> | scene new\n\
  options   : options [music v] [sfx v] [timescale v] [reduce_shake on|off]\n\
  \u{20}           [hud] [map] [settings] [multi]\n\
  réseau    : net connect <url> [pseudo] [assaut|eclaireur|soutien] [salon] [vagues|survie|escorte|boss]\n\
  \u{20}           net disconnect\n\
  divers    : lua <src> | screenshot <chemin.png> [l] [h] | raw <json>\n\
  (l'application doit tourner avec --pilot, cf. docs/PILOT.md)";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut port = motor3derust::pilot::DEFAULT_PORT;
    if args.first().map(String::as_str) == Some("--port") {
        if args.len() < 2 {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        match args[1].parse() {
            Ok(p) => port = p,
            Err(_) => {
                eprintln!("port invalide : « {} »\n{USAGE}", args[1]);
                return ExitCode::from(2);
            }
        }
        args.drain(..2);
    }

    // `move` : deux requêtes espacées (tenir puis relâcher) sur la même connexion.
    if args.first().map(String::as_str) == Some("move") {
        return do_move(port, &args[1..]);
    }

    let request = match build_request(&args) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    let mut conn = match Connection::open(port) {
        Ok(c) => c,
        Err(code) => return code,
    };
    match conn.ask(&request) {
        Ok(line) => print_response(&line),
        Err(code) => code,
    }
}

/// Connexion TCP + lecture ligne à ligne — partagée entre le tir unique et `move`.
struct Connection {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Connection {
    fn open(port: u16) -> Result<Self, ExitCode> {
        let addr = format!("127.0.0.1:{port}");
        let stream = TcpStream::connect(&addr).map_err(|e| {
            eprintln!(
                "connexion à {addr} impossible ({e}) — l'application tourne-t-elle avec --pilot ?"
            );
            ExitCode::from(1)
        })?;
        let reader = BufReader::new(stream.try_clone().map_err(|e| {
            eprintln!("clone du flux impossible ({e})");
            ExitCode::from(1)
        })?);
        Ok(Self { stream, reader })
    }

    fn ask(&mut self, request: &str) -> Result<String, ExitCode> {
        if writeln!(self.stream, "{request}").is_err() {
            eprintln!("envoi de la requête impossible");
            return Err(ExitCode::from(1));
        }
        let mut line = String::new();
        if self.reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            eprintln!("aucune réponse de l'application (connexion coupée ?)");
            return Err(ExitCode::from(1));
        }
        Ok(line)
    }
}

/// `move <turn> <thrust> [ms] [jump|attack|fire|heal…]` : tient l'entrée pendant
/// l'équivalent de `ms` millisecondes de **temps simulé** (pas fixes exécutés
/// immédiatement côté moteur via `hold_ms`), puis relâche — déterministe,
/// instantané, et insensible à l'App Nap (fenêtre masquée).
fn do_move(port: u16, rest: &[String]) -> ExitCode {
    if rest.len() < 2 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let (Ok(turn), Ok(thrust)) = (rest[0].parse::<f64>(), rest[1].parse::<f64>()) else {
        eprintln!("move : turn/thrust invalides\n{USAGE}");
        return ExitCode::from(2);
    };
    let mut ms: u64 = 500;
    let mut flags = &rest[2..];
    if let Some(first) = flags.first()
        && let Ok(v) = first.parse::<u64>()
    {
        ms = v.min(30_000);
        flags = &flags[1..];
    }
    let flag = |name: &str| flags.iter().any(|a| a == name);
    let hold = serde_json::json!({
        "cmd": "input", "turn": turn, "thrust": thrust, "hold_ms": ms,
        "jump": flag("jump"), "attack": flag("attack"),
        "fire": flag("fire"), "heal": flag("heal"),
    });

    let mut conn = match Connection::open(port) {
        Ok(c) => c,
        Err(code) => return code,
    };
    match conn.ask(&hold.to_string()) {
        Ok(line) => print_response(&line),
        Err(code) => code,
    }
}

/// Traduit `verbe args…` en requête JSON. `Err` = problème d'usage (affiché avec
/// l'aide, code de sortie 2).
fn build_request(args: &[String]) -> Result<String, String> {
    let Some(verb) = args.first() else {
        return Err(USAGE.into());
    };
    let rest = &args[1..];
    let joined = rest.join(" ");
    let req = match verb.as_str() {
        "console" if !rest.is_empty() => serde_json::json!({"cmd": "console", "arg": joined}),
        "lua" if !rest.is_empty() => serde_json::json!({"cmd": "lua", "src": joined}),
        "logs" | "state" | "player" => serde_json::json!({"cmd": verb}),
        // `scene` seul = dump ; suivi d'une sous-commande = action de scène.
        "scene" if rest.is_empty() => serde_json::json!({"cmd": "scene"}),
        "scene" => match rest[0].as_str() {
            "save" => match rest.get(1) {
                Some(path) => serde_json::json!({"cmd": "scene_cmd", "op": "save", "path": path}),
                None => serde_json::json!({"cmd": "scene_cmd", "op": "save"}),
            },
            "load" if rest.len() >= 2 => {
                serde_json::json!({"cmd": "scene_cmd", "op": "load", "path": rest[1]})
            }
            "new" => serde_json::json!({"cmd": "scene_cmd", "op": "new"}),
            "demo" if rest.len() >= 2 => {
                serde_json::json!({"cmd": "console", "arg": format!("demo {}", rest[1])})
            }
            _ => return Err(USAGE.into()),
        },
        // Raccourcis console.
        "step" => serde_json::json!({"cmd": "console", "arg": format!("step {}", joined).trim()}),
        "weapon" if rest.len() == 1 => {
            serde_json::json!({"cmd": "console", "arg": format!("weapon {}", rest[0])})
        }
        "demo" if rest.len() == 1 => {
            serde_json::json!({"cmd": "console", "arg": format!("demo {}", rest[0])})
        }
        "undo" | "redo" => serde_json::json!({"cmd": "console", "arg": verb}),
        "camera" => camera_request(rest)?,
        "object" if !rest.is_empty() => object_request(rest)?,
        "options" if !rest.is_empty() => options_request(rest)?,
        "net" if !rest.is_empty() => net_request(rest)?,
        "input" if rest.len() >= 2 => {
            let turn: f64 = rest[0]
                .parse()
                .map_err(|_| format!("turn invalide : « {} »\n{USAGE}", rest[0]))?;
            let thrust: f64 = rest[1]
                .parse()
                .map_err(|_| format!("thrust invalide : « {} »\n{USAGE}", rest[1]))?;
            let flag = |name: &str| rest[2..].iter().any(|a| a == name);
            serde_json::json!({
                "cmd": "input", "turn": turn, "thrust": thrust,
                "jump": flag("jump"), "attack": flag("attack"),
                "fire": flag("fire"), "heal": flag("heal"),
            })
        }
        "screenshot" if !rest.is_empty() => {
            let mut req = serde_json::json!({"cmd": "screenshot", "path": rest[0]});
            if let (Some(w), Some(h)) = (rest.get(1), rest.get(2)) {
                let (w, h): (u64, u64) = match (w.parse(), h.parse()) {
                    (Ok(w), Ok(h)) => (w, h),
                    _ => return Err(format!("dimensions invalides : « {w} {h} »\n{USAGE}")),
                };
                req["width"] = w.into();
                req["height"] = h.into();
            }
            req
        }
        // Échappatoire : envoyer une requête JSON brute telle quelle.
        "raw" if !rest.is_empty() => return Ok(joined),
        _ => return Err(USAGE.into()),
    };
    Ok(req.to_string())
}

/// `camera [target x y z] [yaw °] [pitch °] [distance d] [follow on|off] [frame]`
fn camera_request(rest: &[String]) -> Result<serde_json::Value, String> {
    let mut req = serde_json::json!({"cmd": "camera"});
    let mut it = rest.iter();
    let num = |s: Option<&String>, field: &str| -> Result<f64, String> {
        s.and_then(|s| s.parse().ok())
            .ok_or(format!("camera : nombre attendu après `{field}`\n{USAGE}"))
    };
    while let Some(key) = it.next() {
        match key.as_str() {
            "target" => {
                let (x, y, z) = (
                    num(it.next(), "target")?,
                    num(it.next(), "target")?,
                    num(it.next(), "target")?,
                );
                req["target"] = serde_json::json!([x, y, z]);
            }
            "yaw" => req["yaw"] = num(it.next(), "yaw")?.into(),
            "pitch" => req["pitch"] = num(it.next(), "pitch")?.into(),
            "distance" => req["distance"] = num(it.next(), "distance")?.into(),
            "follow" => {
                req["follow"] = serde_json::Value::Bool(matches!(
                    it.next().map(String::as_str),
                    Some("on") | Some("true") | Some("1")
                ));
            }
            "frame" => req["frame"] = serde_json::Value::Bool(true),
            other => return Err(format!("camera : option inconnue « {other} »\n{USAGE}")),
        }
    }
    Ok(req)
}

/// `object add <kind>` / `import <path>` / `get|delete|duplicate <i>` /
/// `damage <i> [n]` / `set <i> <champ> <valeurs…>`
fn object_request(rest: &[String]) -> Result<serde_json::Value, String> {
    let op = rest[0].as_str();
    let index = |pos: usize| -> Result<u64, String> {
        rest.get(pos)
            .and_then(|s| s.parse().ok())
            .ok_or(format!("object {op} : index attendu\n{USAGE}"))
    };
    Ok(match op {
        "add" => serde_json::json!({
            "cmd": "object", "op": "add",
            "kind": rest.get(1).cloned().unwrap_or_else(|| "cube".into()),
        }),
        "import" if rest.len() >= 2 => {
            serde_json::json!({"cmd": "object", "op": "import", "path": rest[1]})
        }
        "get" | "delete" | "duplicate" => {
            serde_json::json!({"cmd": "object", "op": op, "index": index(1)?})
        }
        "damage" => {
            let mut req = serde_json::json!({"cmd": "object", "op": "damage", "index": index(1)?});
            if let Some(n) = rest.get(2).and_then(|s| s.parse::<u64>().ok()) {
                req["amount"] = n.into();
            }
            req
        }
        "set" if rest.len() >= 3 => {
            let i = index(1)?;
            let field = rest[2].as_str();
            let vals = &rest[3..];
            let nums = || -> Result<Vec<f64>, String> {
                let v: Option<Vec<f64>> = vals.iter().map(|s| s.parse().ok()).collect();
                v.filter(|v| v.len() == 3)
                    .ok_or(format!("object set {field} : 3 nombres attendus\n{USAGE}"))
            };
            let one = || -> Result<f64, String> {
                vals.first()
                    .and_then(|s| s.parse().ok())
                    .ok_or(format!("object set {field} : nombre attendu\n{USAGE}"))
            };
            let patch = match field {
                "pos" => serde_json::json!({"pos": nums()?}),
                "rot" | "rot_deg" => serde_json::json!({"rot_deg": nums()?}),
                "scale" => serde_json::json!({"scale": nums()?}),
                "color" => serde_json::json!({"color": nums()?}),
                "visible" => serde_json::json!({"visible": matches!(
                    vals.first().map(String::as_str), Some("on") | Some("true") | Some("1"))}),
                "physics" => {
                    serde_json::json!({"physics": vals.first().cloned().unwrap_or_default()})
                }
                "metallic" => serde_json::json!({"metallic": one()?}),
                "roughness" => serde_json::json!({"roughness": one()?}),
                "emissive" => serde_json::json!({"emissive": one()?}),
                "hp" => serde_json::json!({"hp": one()? as u64}),
                "name" => serde_json::json!({"name": vals.join(" ")}),
                "tag" => serde_json::json!({"tag": vals.join(" ")}),
                "script" => serde_json::json!({"script": vals.join(" ")}),
                other => return Err(format!("object set : champ inconnu « {other} »\n{USAGE}")),
            };
            serde_json::json!({"cmd": "object", "op": "set", "index": i, "patch": patch})
        }
        _ => return Err(USAGE.into()),
    })
}

/// `options [music v] [sfx v] [timescale v] [reduce_shake on|off] [hud] [map] [settings] [multi]`
fn options_request(rest: &[String]) -> Result<serde_json::Value, String> {
    let mut req = serde_json::json!({"cmd": "options"});
    let mut it = rest.iter();
    while let Some(key) = it.next() {
        match key.as_str() {
            "music" | "sfx" | "timescale" => {
                let v: f64 = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or(format!("options : nombre attendu après `{key}`\n{USAGE}"))?;
                req[key.as_str()] = v.into();
            }
            "reduce_shake" => {
                req["reduce_shake"] = serde_json::Value::Bool(matches!(
                    it.next().map(String::as_str),
                    Some("on") | Some("true") | Some("1")
                ));
            }
            "hud" => req["hud"] = true.into(),
            "map" => req["map"] = true.into(),
            "settings" => req["settings_overlay"] = true.into(),
            "multi" => req["multiplayer_window"] = true.into(),
            other => return Err(format!("options : option inconnue « {other} »\n{USAGE}")),
        }
    }
    Ok(req)
}

/// `net connect <url> [pseudo] [classe] [salon] [mode]` / `net disconnect`
fn net_request(rest: &[String]) -> Result<serde_json::Value, String> {
    match rest[0].as_str() {
        "connect" if rest.len() >= 2 => {
            let mut req = serde_json::json!({"cmd": "net", "op": "connect", "url": rest[1]});
            if let Some(name) = rest.get(2) {
                req["name"] = serde_json::json!(name);
            }
            if let Some(class) = rest.get(3) {
                req["class"] = serde_json::json!(class);
            }
            if let Some(room) = rest.get(4) {
                req["room"] = serde_json::json!(room);
            }
            if let Some(objective) = rest.get(5) {
                req["objective"] = serde_json::json!(objective);
            }
            Ok(req)
        }
        "disconnect" => Ok(serde_json::json!({"cmd": "net", "op": "disconnect"})),
        _ => Err(USAGE.into()),
    }
}

/// Affiche la réponse : les chaînes en texte brut, les listes de chaînes ligne à
/// ligne (logs), le reste en JSON indenté — lisible pour un humain comme pour un
/// agent, sans double sérialisation.
fn print_response(line: &str) -> ExitCode {
    let parsed: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            // Ne devrait pas arriver (le serveur ne répond que du JSON) ; on
            // relaie tel quel plutôt que de masquer la réponse.
            println!("{line}");
            return ExitCode::from(1);
        }
    };
    if parsed["ok"] == serde_json::Value::Bool(true) {
        match &parsed["result"] {
            serde_json::Value::String(s) => println!("{s}"),
            serde_json::Value::Array(items) if items.iter().all(serde_json::Value::is_string) => {
                for item in items {
                    println!("{}", item.as_str().unwrap_or_default());
                }
            }
            other => println!(
                "{}",
                serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string())
            ),
        }
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "erreur : {}",
            parsed["error"].as_str().unwrap_or("réponse illisible")
        );
        ExitCode::from(1)
    }
}
