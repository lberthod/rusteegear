//! Backend annexe Firebase — comptes joueurs (SPRINT_MMORPG.md, Sprint 56).
//! Desktop uniquement, via l'API REST (`ureq`, déjà utilisé pour l'IA DeepSeek
//! dans `app/ai.rs`) — Firebase n'a pas de SDK Rust officiel.
//!
//! Rappel de scope (SPRINT_MMORPG.md §0) : Firebase RTDB ne transporte **jamais**
//! le gameplay temps réel (position, coups) — uniquement des données peu
//! fréquentes. Ici : l'identité du joueur (email/mot de passe → `uid` + pseudo).
//!
//! **Sécurité** : la clé API Web Firebase est publique par conception (elle
//! s'affiche dans le JS de n'importe quel site utilisant Firebase) — la
//! sécurité vient des **règles RTDB**, pas du secret de cette clé. Avant
//! d'exposer `/users/{uid}/profile` en écriture, configurer des règles du
//! genre :
//! ```json
//! {
//!   "rules": {
//!     "users": {
//!       "$uid": {
//!         ".read": true,
//!         ".write": "auth != null && auth.uid === $uid"
//!       }
//!     }
//!   }
//! }
//! ```
//! sans quoi n'importe qui peut écrire le profil de n'importe qui.

use serde::Deserialize;

/// Où joindre Firebase : clé API Web + URL de la Realtime Database (cf.
/// `app::settings::Settings::firebase_api_key`/`firebase_database_url`).
#[derive(Clone, Debug, PartialEq)]
pub struct FirebaseConfig {
    pub api_key: String,
    pub database_url: String,
}

/// Session obtenue après connexion/inscription : identifiant Firebase du
/// joueur et jeton à joindre aux écritures RTDB (`?auth=...`).
#[derive(Clone, Debug, PartialEq)]
pub struct AuthSession {
    pub uid: String,
    pub id_token: String,
}

#[derive(Deserialize)]
struct SignInResponse {
    #[serde(rename = "localId")]
    local_id: String,
    #[serde(rename = "idToken")]
    id_token: String,
}

#[derive(Deserialize)]
struct FirebaseErrorBody {
    error: FirebaseErrorDetail,
}

#[derive(Deserialize)]
struct FirebaseErrorDetail {
    message: String,
}

/// Progression persistante d'un joueur (SPRINT_MMORPG.md, Sprint 57) : niveau
/// et XP cumulés entre les parties, stockés sous `/users/{uid}/progress`.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, serde::Serialize)]
pub struct PlayerProgress {
    #[serde(default = "default_level")]
    pub level: u32,
    #[serde(default)]
    pub xp: u32,
}

fn default_level() -> u32 {
    1
}

impl Default for PlayerProgress {
    fn default() -> Self {
        Self {
            level: default_level(),
            xp: 0,
        }
    }
}

/// Parse la réponse RTDB d'une lecture de `/users/{uid}/progress` : `null`
/// (nœud absent, premier lancement du joueur) donne la progression par défaut,
/// pas une erreur.
fn parse_progress_response(body: &str) -> Result<PlayerProgress, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Réponse Firebase illisible : {e}"))?;
    if v.is_null() {
        return Ok(PlayerProgress::default());
    }
    serde_json::from_value(v).map_err(|e| format!("Progression Firebase illisible : {e}"))
}

/// Parse la réponse d'un appel `signUp`/`signInWithPassword` réussi. Séparé de
/// l'appel réseau pour rester testable sans identifiants Firebase réels.
fn parse_auth_response(body: &str) -> Result<AuthSession, String> {
    let parsed: SignInResponse =
        serde_json::from_str(body).map_err(|e| format!("Réponse Firebase Auth illisible : {e}"))?;
    Ok(AuthSession {
        uid: parsed.local_id,
        id_token: parsed.id_token,
    })
}

/// Extrait le message d'erreur d'une réponse Firebase Auth en échec (ex.
/// `EMAIL_EXISTS`, `INVALID_PASSWORD`) ; `None` si le corps ne suit pas le
/// format d'erreur attendu (réponse imprévue, HTML d'un proxy, etc.).
fn parse_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<FirebaseErrorBody>(body)
        .ok()
        .map(|e| e.error.message)
}

/// Construit l'URL REST RTDB pour `path` (ex. `"users/abc/profile"`), avec les
/// éventuels paramètres de requête (ex. `auth=...`). Gère un `database_url`
/// avec ou sans `/` final.
fn rtdb_url(database_url: &str, path: &str, query: &str) -> String {
    let base = database_url.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if query.is_empty() {
        format!("{base}/{path}.json")
    } else {
        format!("{base}/{path}.json?{query}")
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod net_io {
    use super::*;

    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

    fn auth_call(
        config: &FirebaseConfig,
        endpoint: &str,
        email: &str,
        password: &str,
    ) -> Result<AuthSession, String> {
        if config.api_key.trim().is_empty() {
            return Err("Clé API Firebase manquante (Outils → Paramètres)".into());
        }
        let url = format!(
            "https://identitytoolkit.googleapis.com/v1/accounts:{endpoint}?key={}",
            config.api_key
        );
        let body = serde_json::json!({
            "email": email,
            "password": password,
            "returnSecureToken": true,
        });
        match ureq::post(&url).timeout(TIMEOUT).send_json(body) {
            Ok(resp) => {
                let text = resp
                    .into_string()
                    .map_err(|e| format!("Réponse Firebase illisible : {e}"))?;
                parse_auth_response(&text)
            }
            Err(ureq::Error::Status(_, resp)) => {
                let text = resp.into_string().unwrap_or_default();
                Err(parse_error_message(&text)
                    .map(|m| format!("Firebase Auth : {m}"))
                    .unwrap_or_else(|| "Firebase Auth : erreur inconnue".to_string()))
            }
            Err(e) => Err(format!("Requête Firebase Auth échouée : {e}")),
        }
    }

    /// Crée un compte joueur (email/mot de passe).
    pub fn sign_up(
        config: &FirebaseConfig,
        email: &str,
        password: &str,
    ) -> Result<AuthSession, String> {
        auth_call(config, "signUp", email, password)
    }

    /// Connecte un joueur existant.
    pub fn sign_in(
        config: &FirebaseConfig,
        email: &str,
        password: &str,
    ) -> Result<AuthSession, String> {
        auth_call(config, "signInWithPassword", email, password)
    }

    /// Écrit le pseudo du joueur dans `/users/{uid}/profile` (RTDB). Nécessite
    /// des règles de sécurité qui n'autorisent l'écriture qu'au propriétaire
    /// (cf. le commentaire de sécurité en tête de module).
    pub fn set_profile_name(
        config: &FirebaseConfig,
        session: &AuthSession,
        name: &str,
    ) -> Result<(), String> {
        if config.database_url.trim().is_empty() {
            return Err("URL Firebase Database manquante (Outils → Paramètres)".into());
        }
        let path = format!("users/{}/profile", session.uid);
        let url = rtdb_url(
            &config.database_url,
            &path,
            &format!("auth={}", session.id_token),
        );
        let body = serde_json::json!({ "name": name });
        ureq::put(&url)
            .timeout(TIMEOUT)
            .send_json(body)
            .map(|_| ())
            .map_err(|e| format!("Écriture du profil Firebase échouée : {e}"))
    }

    /// Lit le pseudo d'un joueur depuis `/users/{uid}/profile/name` (lecture
    /// publique attendue dans les règles, cf. le commentaire de sécurité).
    pub fn get_profile_name(config: &FirebaseConfig, uid: &str) -> Result<Option<String>, String> {
        if config.database_url.trim().is_empty() {
            return Err("URL Firebase Database manquante (Outils → Paramètres)".into());
        }
        let path = format!("users/{uid}/profile/name");
        let url = rtdb_url(&config.database_url, &path, "");
        let resp = ureq::get(&url)
            .timeout(TIMEOUT)
            .call()
            .map_err(|e| format!("Lecture du profil Firebase échouée : {e}"))?;
        let text = resp
            .into_string()
            .map_err(|e| format!("Réponse Firebase illisible : {e}"))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Réponse Firebase illisible : {e}"))?;
        Ok(v.as_str().map(str::to_string))
    }

    /// Lit la progression d'un joueur (`/users/{uid}/progress`) ; renvoie la
    /// progression par défaut (niveau 1, 0 XP) si le nœud n'existe pas encore
    /// (premier lancement de ce joueur).
    pub fn get_progress(config: &FirebaseConfig, uid: &str) -> Result<PlayerProgress, String> {
        if config.database_url.trim().is_empty() {
            return Err("URL Firebase Database manquante (Outils → Paramètres)".into());
        }
        let path = format!("users/{uid}/progress");
        let url = rtdb_url(&config.database_url, &path, "");
        let resp = ureq::get(&url)
            .timeout(TIMEOUT)
            .call()
            .map_err(|e| format!("Lecture de la progression Firebase échouée : {e}"))?;
        let text = resp
            .into_string()
            .map_err(|e| format!("Réponse Firebase illisible : {e}"))?;
        parse_progress_response(&text)
    }

    /// Écrit la progression d'un joueur. `auth_token` est délibérément explicite
    /// (pas pris sur une `AuthSession` du joueur) : cf. le commentaire
    /// « Qui écrit la progression ? » en tête de module — c'est le **serveur de
    /// jeu**, avec ses propres identifiants, qui doit appeler cette fonction en
    /// fin de manche, jamais le client avec son propre token.
    pub fn set_progress(
        config: &FirebaseConfig,
        uid: &str,
        progress: PlayerProgress,
        auth_token: &str,
    ) -> Result<(), String> {
        if config.database_url.trim().is_empty() {
            return Err("URL Firebase Database manquante (Outils → Paramètres)".into());
        }
        let path = format!("users/{uid}/progress");
        let url = rtdb_url(&config.database_url, &path, &format!("auth={auth_token}"));
        ureq::put(&url)
            .timeout(TIMEOUT)
            .send_json(serde_json::to_value(progress).map_err(|e| e.to_string())?)
            .map(|_| ())
            .map_err(|e| format!("Écriture de la progression Firebase échouée : {e}"))
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use net_io::{
    get_profile_name, get_progress, set_profile_name, set_progress, sign_in, sign_up,
};

// --- Qui écrit la progression ? (SPRINT_MMORPG.md, Sprint 57) ---------------
//
// `set_progress` prend un `auth_token` explicite plutôt que de dépendre d'une
// `AuthSession` de joueur, parce que la progression (XP, niveau) est une
// donnée **compétitive** : si le client pouvait l'écrire avec son propre
// token, il pourrait s'attribuer n'importe quel score. Les règles RTDB
// doivent donc refuser l'écriture au propriétaire (contrairement au profil,
// cf. Sprint 56) et ne l'autoriser qu'à un compte serveur dédié, ex. :
// ```json
// "progress": { "$uid": {
//   ".read": "auth != null && auth.uid === $uid",
//   ".write": "auth != null && auth.uid === '<UID_DU_COMPTE_SERVEUR>'"
// }}
// ```
// Le serveur de jeu (`src/bin/server.rs`) doit alors se connecter une fois au
// démarrage avec un compte Firebase dédié (`sign_in`, cf. ci-dessus) et
// réutiliser son `id_token` pour tous les appels `set_progress`. Une vraie
// mise en production irait plus loin avec le **Firebase Admin SDK** (jeton
// signé par compte de service, contourne les règles RTDB) — non implémenté
// ici faute de crate Rust mature pour l'Admin SDK ; l'approche « compte
// serveur dédié + règles » ci-dessus est une alternative REST-only suffisante
// à l'échelle visée (2-16 joueurs/salon).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_successful_auth_response() {
        let body = r#"{
            "kind": "identitytoolkit#SignupNewUserResponse",
            "idToken": "abc.def.ghi",
            "email": "joueur@example.com",
            "refreshToken": "refresh-token",
            "expiresIn": "3600",
            "localId": "uid-1234"
        }"#;
        let session = parse_auth_response(body).expect("réponse valide");
        assert_eq!(session.uid, "uid-1234");
        assert_eq!(session.id_token, "abc.def.ghi");
    }

    #[test]
    fn rejects_a_malformed_auth_response() {
        assert!(parse_auth_response("pas du json").is_err());
        assert!(parse_auth_response(r#"{"foo": "bar"}"#).is_err());
    }

    #[test]
    fn parses_a_firebase_error_message() {
        let body = r#"{
            "error": {
                "code": 400,
                "message": "EMAIL_EXISTS",
                "errors": [{"message": "EMAIL_EXISTS", "domain": "global", "reason": "invalid"}]
            }
        }"#;
        assert_eq!(parse_error_message(body), Some("EMAIL_EXISTS".to_string()));
    }

    #[test]
    fn error_message_is_none_for_unexpected_bodies() {
        assert_eq!(parse_error_message("<html>proxy error</html>"), None);
    }

    #[test]
    fn rtdb_url_handles_a_trailing_slash_on_the_base() {
        let with_slash = rtdb_url(
            "https://x-default-rtdb.firebaseio.com/",
            "users/abc/profile",
            "",
        );
        let without_slash = rtdb_url(
            "https://x-default-rtdb.firebaseio.com",
            "users/abc/profile",
            "",
        );
        assert_eq!(with_slash, without_slash);
        assert_eq!(
            with_slash,
            "https://x-default-rtdb.firebaseio.com/users/abc/profile.json"
        );
    }

    #[test]
    fn rtdb_url_appends_the_query_string_when_present() {
        let url = rtdb_url(
            "https://x.firebaseio.com",
            "users/abc/profile",
            "auth=tok123",
        );
        assert_eq!(
            url,
            "https://x.firebaseio.com/users/abc/profile.json?auth=tok123"
        );
    }

    #[test]
    fn progress_defaults_to_level_one_zero_xp_when_the_node_is_absent() {
        // RTDB renvoie littéralement `null` quand le nœud n'existe pas encore
        // (premier lancement du joueur) — pas une erreur à traiter.
        let progress = parse_progress_response("null").expect("null est un cas valide");
        assert_eq!(progress, PlayerProgress { level: 1, xp: 0 });
    }

    #[test]
    fn progress_parses_an_existing_node() {
        let progress = parse_progress_response(r#"{"level": 4, "xp": 1250}"#).expect("nœud valide");
        assert_eq!(progress, PlayerProgress { level: 4, xp: 1250 });
    }

    #[test]
    fn progress_rejects_a_malformed_node() {
        assert!(parse_progress_response(r#"{"level": "pas un nombre"}"#).is_err());
    }
}
