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
- dt : temps écoulé (s), time : temps total (s)
- input.jx, input.jy : axes du joystick (-1..1)
- input.btn.NOM : booléen d'un bouton tactile
Réponds UNIQUEMENT avec le code Lua, sans explication ni balises Markdown.";

/// Appelle DeepSeek pour produire un script Lua à partir d'une consigne en langage
/// naturel. Bloquant : à lancer dans un thread de fond.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn generate_lua(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("Clé API DeepSeek manquante (Outils → Paramètres)".into());
    }
    let model = if model.trim().is_empty() {
        "deepseek-chat"
    } else {
        model.trim()
    };
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "stream": false,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": prompt },
        ],
    });

    let resp = ureq::post("https://api.deepseek.com/chat/completions")
        .set("Authorization", &format!("Bearer {api_key}"))
        .timeout(std::time::Duration::from_secs(30))
        .send_json(body)
        .map_err(|e| format!("Requête DeepSeek échouée : {e}"))?;

    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("Réponse DeepSeek illisible : {e}"))?;

    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("Réponse DeepSeek sans contenu")?;

    Ok(strip_code_fences(content))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn generate_lua(_api_key: &str, _model: &str, _prompt: &str) -> Result<String, String> {
    Err("Génération IA indisponible sur mobile".into())
}

/// Retire d'éventuelles balises Markdown (``` ou ```lua) autour du code.
fn strip_code_fences(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // saute l'éventuel langage sur la première ligne, et la fence finale
        let rest = rest.strip_prefix("lua").unwrap_or(rest);
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
