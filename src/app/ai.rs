//! Génération de scripts Lua par IA via l'API DeepSeek (compatible OpenAI).
//! Desktop uniquement (la génération se fait depuis l'éditeur).

/// Contexte donné au modèle : les variables exposées aux scripts du moteur.
const SYSTEM_PROMPT: &str = "\
Tu écris des scripts Lua courts pour un moteur de jeu 3D. À chaque frame, le \
script peut lire/écrire ces variables globales :
- obj.x, obj.y, obj.z : position
- obj.rx, obj.ry, obj.rz : rotation en degrés
- obj.sx, obj.sy, obj.sz : échelle
- obj.r, obj.g, obj.b : couleur (0..1)
- obj.tapped : booléen, vrai la frame où l'objet est touché
- obj.triggered : booléen, vrai quand le joueur entre dans la zone de l'objet
- dt : temps écoulé (s), time : temps total (s)
- input.jx, input.jy : axes du joystick (-1..1)
- input.btn.NOM : booléen d'un bouton tactile
- vibrate(ms) : retour haptique (mobile)
Réponds UNIQUEMENT avec le code Lua, sans explication ni balises Markdown.";

/// Paramètres d'un appel de génération de script.
#[derive(Clone)]
pub struct AiRequest {
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub prompt: String,
}

/// Système pour la génération de scène entière (JSON contraint).
const SCENE_SYSTEM_PROMPT: &str = "\
Tu génères une scène pour un moteur de jeu 3D, au format JSON STRICT (aucun texte \
autour, pas de Markdown). Schéma :
{\"objects\":[{\"name\":str,\"mesh\":\"cube|sphere|plane|cylinder|capsule\",\
\"x\":num,\"y\":num,\"z\":num,\"color\":[r,g,b (0..1)],\"script\":str (Lua, peut être vide),\
\"physics\":\"none|static|dynamic\",\"tappable\":bool}],\
\"joystick\":bool,\"buttons\":[str],\"camera_follow\":bool}
Variables Lua disponibles : obj.x/y/z, obj.rx/ry/rz (°), obj.sx/sy/sz, obj.r/g/b, \
obj.tapped, obj.triggered, dt, time, input.jx, input.jy, input.btn.NOM.
Mets un sol (mesh plane, physics static) si pertinent. Réponds UNIQUEMENT le JSON.";

/// Appel bas-niveau à l'API chat de DeepSeek. Retourne le contenu texte du message.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn chat(req: &AiRequest, system: &str) -> Result<String, String> {
    if req.api_key.trim().is_empty() {
        return Err("Clé API DeepSeek manquante (Outils → Paramètres)".into());
    }
    let model = if req.model.trim().is_empty() {
        "deepseek-chat"
    } else {
        req.model.trim()
    };
    log::info!(
        "Requête IA via « {model} » (température {:.1})",
        req.temperature
    );
    let body = serde_json::json!({
        "model": model,
        "temperature": req.temperature,
        "stream": false,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": req.prompt },
        ],
    });

    let resp = ureq::post("https://api.deepseek.com/chat/completions")
        .set("Authorization", &format!("Bearer {}", req.api_key))
        .timeout(std::time::Duration::from_secs(60))
        .send_json(body)
        .map_err(|e| format!("Requête DeepSeek échouée : {e}"))?;

    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("Réponse DeepSeek illisible : {e}"))?;

    v["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Réponse DeepSeek sans contenu".into())
}

/// Produit un script Lua à partir d'une consigne. Bloquant (thread de fond).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn generate_lua(req: &AiRequest) -> Result<String, String> {
    chat(req, SYSTEM_PROMPT).map(|s| strip_code_fences(&s))
}

/// Produit le JSON d'une scène complète à partir d'une consigne. Bloquant.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn generate_scene_json(req: &AiRequest) -> Result<String, String> {
    chat(req, SCENE_SYSTEM_PROMPT).map(|s| strip_code_fences(&s))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn generate_lua(_req: &AiRequest) -> Result<String, String> {
    Err("Génération IA indisponible sur mobile".into())
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn generate_scene_json(_req: &AiRequest) -> Result<String, String> {
    Err("Génération IA indisponible sur mobile".into())
}

/// Retire d'éventuelles balises Markdown (``` éventuellement suivi d'un langage
/// comme `lua` ou `json`) autour du contenu.
fn strip_code_fences(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // saute le tag de langage (lettres) éventuel sur la première ligne
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
        let rest = rest.trim_start_matches('\n');
        return rest
            .trim_end()
            .trim_end_matches("```")
            .trim_end()
            .to_string();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::strip_code_fences;

    #[test]
    fn strips_lua_fences() {
        let s = "```lua\nobj.x = obj.x + dt\n```";
        assert_eq!(strip_code_fences(s), "obj.x = obj.x + dt");
    }

    #[test]
    fn leaves_plain_code() {
        assert_eq!(strip_code_fences("obj.y = 1"), "obj.y = 1");
    }
}
