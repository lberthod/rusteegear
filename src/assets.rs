//! Assets embarqués dans le binaire (Sprint 24) : modèles glTF et sons copiés
//! dans `assets/bundle/` à l'export, inclus à la compilation. Le player les résout
//! ainsi sans dépendre de chemins disque (qui n'existent pas sur l'appareil cible).
//!
//! Convention : un chemin de scène préfixé `bundle://<clé>` désigne un asset embarqué.

use std::collections::HashMap;
use std::path::PathBuf;

use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

static BUNDLE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/bundle");

/// Préfixe identifiant un asset embarqué (figé à la compilation, pour le player exporté).
pub const SCHEME: &str = "bundle://";

/// Préfixe d'un asset de projet (dossier `~/.motor3derust/assets/`, édition desktop).
pub const ASSET_SCHEME: &str = "asset://";

/// Préfixe d'une référence **stable** vers un asset de projet (Sprint 95) : un uuid
/// plutôt qu'un nom de fichier, résolu via le manifeste (`register_asset`/
/// `resolve_asset_id`) — survit à un renommage du fichier sous-jacent (`rename_asset`),
/// contrairement à `asset://<nom>` qui casse dès que `<nom>` change. `import_to_assets`
/// délivre désormais ce schéma pour les nouveaux imports ; les scènes existantes qui
/// référencent encore `asset://<nom>` en dur ne sont pas migrées rétroactivement — un
/// asset doit être ré-importé/enregistré pour devenir rename-safe.
pub const ASSET_ID_SCHEME: &str = "asset-id://";

/// Nom du fichier de manifeste (Sprint 95) dans `assets_dir()` — exclu de
/// `list_assets()` : c'est une donnée interne de ce module, pas un asset à afficher/
/// importer dans l'éditeur.
const MANIFEST_FILE: &str = "manifest.json";

/// Vrai si `path` désigne un asset déjà géré par ce module (embarqué, projet, ou
/// référence stable) — par opposition à un chemin disque externe qui reste à importer.
/// Centralise ce qui était, avant le Sprint 95, un `starts_with(SCHEME) ||
/// starts_with(ASSET_SCHEME)` répété à 4 endroits (import glTF, audio, dimensions de
/// texture, collecte d'assets) — chacun aurait dû être mis à jour séparément pour
/// reconnaître `asset-id://` sans ce point de passage unique.
pub fn is_known_scheme(path: &str) -> bool {
    path.starts_with(SCHEME) || path.starts_with(ASSET_SCHEME) || path.starts_with(ASSET_ID_SCHEME)
}

/// Manifeste `uuid → nom de fichier courant` (Sprint 95), persisté dans
/// `assets_dir()/manifest.json`. Le nom de fichier peut changer (renommage) sans que
/// l'uuid ne change : c'est cette indirection qui rend une référence `asset-id://`
/// stable dans le temps, contrairement à un chemin `asset://<nom>` en dur.
#[derive(Default, Serialize, Deserialize)]
struct AssetManifest {
    entries: HashMap<String, String>,
}

fn load_manifest_at(dir: &std::path::Path) -> AssetManifest {
    std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_manifest_at(dir: &std::path::Path, manifest: &AssetManifest) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(manifest) {
        let _ = std::fs::write(dir.join(MANIFEST_FILE), json);
    }
}

/// Identifiant local court, suffisant pour un manifeste par utilisateur (pas un
/// contexte de sécurité) : pas de dépendance `uuid` pour ça — l'horloge, le PID et le
/// hasher aléatoire du process (`RandomState`, déjà dans la std) suffisent à éviter
/// toute collision pratique sur un même poste.
fn new_asset_id() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    nanos.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Cœur de `register_asset`, paramétré par `dir` (testable sans toucher
/// `~/.motor3derust/assets/` ni l'environnement global — cf. les tests plus bas).
fn register_asset_at(dir: &std::path::Path, name: &str) -> String {
    let mut manifest = load_manifest_at(dir);
    if let Some(id) = manifest
        .entries
        .iter()
        .find(|(_, n)| n.as_str() == name)
        .map(|(id, _)| id.clone())
    {
        return format!("{ASSET_ID_SCHEME}{id}");
    }
    let id = new_asset_id();
    manifest.entries.insert(id.clone(), name.to_string());
    save_manifest_at(dir, &manifest);
    format!("{ASSET_ID_SCHEME}{id}")
}

/// Enregistre `name` (déjà présent dans `assets_dir()`) dans le manifeste et renvoie sa
/// référence stable `asset-id://<uuid>`. Idempotent par nom : un fichier déjà enregistré
/// garde son uuid existant plutôt que d'en créer un second (repéré par nom, le seul
/// identifiant disponible avant un premier enregistrement).
pub fn register_asset(name: &str) -> String {
    match assets_dir() {
        Some(dir) => register_asset_at(&dir, name),
        // Pas de $HOME (environnement dégradé) : uuid renvoyé mais rien à persister —
        // se comporte comme un asset introuvable au prochain `resolve_asset_id`, pas
        // pire que l'ancien `asset://<nom>` dans ce même environnement.
        None => format!("{ASSET_ID_SCHEME}{}", new_asset_id()),
    }
}

fn resolve_asset_id_at(dir: &std::path::Path, id: &str) -> Option<String> {
    let uuid = id.strip_prefix(ASSET_ID_SCHEME)?;
    load_manifest_at(dir)
        .entries
        .get(uuid)
        .map(|name| format!("{ASSET_SCHEME}{name}"))
}

/// Résout une référence `asset-id://<uuid>` vers son chemin `asset://<nom>` **actuel** —
/// c'est cette indirection qui rend un renommage transparent pour les scènes qui
/// référencent l'uuid plutôt que le nom de fichier. `None` si `id` n'a pas ce schéma, ou
/// si l'uuid est inconnu du manifeste (asset supprimé, ou jamais enregistré ici).
pub fn resolve_asset_id(id: &str) -> Option<String> {
    resolve_asset_id_at(&assets_dir()?, id)
}

fn rename_asset_at(dir: &std::path::Path, id: &str, new_name: &str) -> bool {
    let Some(uuid) = id.strip_prefix(ASSET_ID_SCHEME) else {
        return false;
    };
    let mut manifest = load_manifest_at(dir);
    let Some(old_name) = manifest.entries.get(uuid).cloned() else {
        return false;
    };
    if std::fs::rename(dir.join(&old_name), dir.join(new_name)).is_err() {
        return false;
    }
    manifest
        .entries
        .insert(uuid.to_string(), new_name.to_string());
    save_manifest_at(dir, &manifest);
    true
}

/// Renomme un asset de projet en gardant son uuid stable dans le manifeste : les scènes
/// qui référencent `asset-id://<uuid>` continuent de résoudre vers le fichier après
/// renommage — le livrable du Sprint 95 (« renommer un asset ne casse plus une scène »).
/// `false` si `id` est inconnu du manifeste ou si le renommage disque échoue (ex. nom de
/// destination déjà pris).
pub fn rename_asset(id: &str, new_name: &str) -> bool {
    match assets_dir() {
        Some(dir) => rename_asset_at(&dir, id, new_name),
        None => false,
    }
}

/// Si `path` désigne un asset embarqué, renvoie sa clé (le nom dans le bundle).
pub fn strip_scheme(path: &str) -> Option<&str> {
    path.strip_prefix(SCHEME)
}

/// Octets d'un asset embarqué, ou `None` s'il est absent du bundle.
pub fn bundle_bytes(key: &str) -> Option<&'static [u8]> {
    BUNDLE.get_file(key).map(|f| f.contents())
}

/// Dossier des assets de projet (`~/.motor3derust/assets/`).
pub fn assets_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".motor3derust").join("assets"))
}

/// Lit les octets d'un chemin quel que soit son schéma : `asset-id://` (référence
/// stable, résolue puis relue récursivement — Sprint 95), `bundle://` (embarqué),
/// `asset://` (dossier projet, repli sur le bundle), ou chemin disque classique.
pub fn read_bytes(path: &str) -> Option<Vec<u8>> {
    if let Some(resolved) = resolve_asset_id(path) {
        return read_bytes(&resolved);
    }
    if let Some(key) = path.strip_prefix(SCHEME) {
        return bundle_bytes(key).map(|b| b.to_vec());
    }
    if let Some(key) = path.strip_prefix(ASSET_SCHEME) {
        if let Some(dir) = assets_dir()
            && let Ok(b) = std::fs::read(dir.join(key))
        {
            return Some(b);
        }
        // repli : l'asset peut être embarqué (player exporté)
        return bundle_bytes(key).map(|b| b.to_vec());
    }
    std::fs::read(path).ok()
}

/// Copie un fichier disque dans le dossier d'assets de projet et renvoie une référence
/// stable `asset-id://<uuid>` (Sprint 95 — avant, un `asset://<nom>` en dur, cassé par
/// un renommage ultérieur du fichier), ou `None` si la copie échoue. Idempotent pour un
/// schéma déjà connu (renvoyé tel quel, y compris un `asset://` hérité d'avant ce sprint).
pub fn import_to_assets(src: &str) -> Option<String> {
    if is_known_scheme(src) {
        return Some(src.to_string());
    }
    let dir = assets_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let name = std::path::Path::new(src).file_name()?.to_str()?.to_string();
    let dest = dir.join(&name);
    if std::fs::copy(src, &dest).is_err() {
        return None;
    }
    Some(register_asset(&name))
}

/// Liste les assets disponibles : fichiers du dossier projet + assets embarqués,
/// sous forme de chemins préfixés (`asset://…` / `bundle://…`), triés.
pub fn list_assets() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(dir) = assets_dir() {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if let Some(name) = e.file_name().to_str()
                && name != MANIFEST_FILE
            {
                out.push(format!("{ASSET_SCHEME}{name}"));
            }
        }
    }
    for f in BUNDLE.files() {
        if let Some(name) = f.path().to_str() {
            out.push(format!("{SCHEME}{name}"));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_scheme_detects_bundle_paths() {
        assert_eq!(strip_scheme("bundle://m0_arbre.glb"), Some("m0_arbre.glb"));
        assert_eq!(strip_scheme("/Users/moi/arbre.glb"), None);
        assert_eq!(strip_scheme(""), None);
    }

    #[test]
    fn bundle_bytes_missing_is_none() {
        // Le bundle par défaut est vide (.gitkeep) : une clé inconnue renvoie None
        // au lieu de paniquer.
        assert!(bundle_bytes("inexistant.glb").is_none());
    }

    /// Dossier temporaire unique par test (pas de mutation de `$HOME`/état global —
    /// les tests tournent en parallèle dans le même process, cf. `register_asset_at`
    /// et consorts, paramétrés par répertoire pour cette raison).
    fn temp_assets_dir(tag: &str) -> std::path::PathBuf {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        tag.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        let dir =
            std::env::temp_dir().join(format!("rusteegear_assets_test_{:x}", hasher.finish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn register_asset_is_idempotent_by_name() {
        let dir = temp_assets_dir("idempotent");
        let a = register_asset_at(&dir, "arbre.glb");
        let b = register_asset_at(&dir, "arbre.glb");
        assert_eq!(
            a, b,
            "enregistrer deux fois le même nom doit renvoyer le même uuid"
        );
        assert!(a.starts_with(ASSET_ID_SCHEME));
    }

    #[test]
    fn renaming_an_asset_keeps_its_id_resolvable() {
        // Le livrable du Sprint 95 : un renommage ne doit pas casser la référence
        // stable — c'est précisément ce que `asset://<nom>` en dur ne permettait pas.
        let dir = temp_assets_dir("rename");
        std::fs::write(dir.join("arbre.glb"), b"contenu glb factice").unwrap();
        let id = register_asset_at(&dir, "arbre.glb");

        assert!(rename_asset_at(&dir, &id, "arbre_v2.glb"));
        assert_eq!(
            resolve_asset_id_at(&dir, &id),
            Some(format!("{ASSET_SCHEME}arbre_v2.glb"))
        );
        assert!(
            !dir.join("arbre.glb").exists(),
            "l'ancien nom ne doit plus exister sur le disque"
        );
        assert!(dir.join("arbre_v2.glb").exists());
    }

    #[test]
    fn resolve_asset_id_is_none_for_an_unknown_uuid() {
        let dir = temp_assets_dir("unknown");
        assert_eq!(
            resolve_asset_id_at(&dir, &format!("{ASSET_ID_SCHEME}inconnu")),
            None
        );
    }

    #[test]
    fn rename_asset_fails_gracefully_for_an_unknown_id() {
        let dir = temp_assets_dir("rename_unknown");
        assert!(!rename_asset_at(
            &dir,
            &format!("{ASSET_ID_SCHEME}inconnu"),
            "peu importe.glb"
        ));
    }

    #[test]
    fn is_known_scheme_recognizes_all_three_prefixes() {
        assert!(is_known_scheme("bundle://x"));
        assert!(is_known_scheme("asset://x"));
        assert!(is_known_scheme("asset-id://x"));
        assert!(!is_known_scheme("/disque/x.glb"));
    }
}
