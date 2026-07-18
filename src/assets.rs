//! Assets embarqués dans le binaire : modèles glTF et sons copiés
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

/// Préfixe d'une référence **stable** vers un asset de projet : un uuid
/// plutôt qu'un nom de fichier, résolu via le manifeste (`register_asset`/
/// `resolve_asset_id`) — survit à un renommage du fichier sous-jacent (`rename_asset`),
/// contrairement à `asset://<nom>` qui casse dès que `<nom>` change. `import_to_assets`
/// délivre ce schéma pour les nouveaux imports ; les scènes existantes qui
/// référencent encore `asset://<nom>` en dur ne sont pas migrées rétroactivement — un
/// asset doit être ré-importé/enregistré pour devenir rename-safe.
pub const ASSET_ID_SCHEME: &str = "asset-id://";

/// Nom du fichier de manifeste dans `assets_dir()` — exclu de
/// `list_assets()` : c'est une donnée interne de ce module, pas un asset à afficher/
/// importer dans l'éditeur.
const MANIFEST_FILE: &str = "manifest.json";

/// Vrai si `path` désigne un asset déjà géré par ce module (embarqué, projet, ou
/// référence stable) — par opposition à un chemin disque externe qui reste à importer.
/// Point de passage unique pour reconnaître les trois schémas (`SCHEME`/`ASSET_SCHEME`/
/// `ASSET_ID_SCHEME`) : les 4 appelants (import glTF, audio, dimensions de texture,
/// collecte d'assets) partagent cette même logique plutôt que de la dupliquer chacun
/// de leur côté.
pub fn is_known_scheme(path: &str) -> bool {
    path.starts_with(SCHEME) || path.starts_with(ASSET_SCHEME) || path.starts_with(ASSET_ID_SCHEME)
}

/// Manifeste `uuid → nom de fichier courant`, persisté dans
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

    use crate::time_compat::{SystemTime, UNIX_EPOCH};
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
pub(crate) fn register_asset_at(dir: &std::path::Path, name: &str) -> String {
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

pub(crate) fn resolve_asset_id_at(dir: &std::path::Path, id: &str) -> Option<String> {
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
/// renommage. `false` si `id` est inconnu du manifeste ou si le renommage disque échoue
/// (ex. nom de destination déjà pris).
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

/// Octets d'un asset embarqué, ou `None` s'il est absent du bundle. Décompressés à la
/// volée (Sprint 127) : `copy_to_bundle` (`editor::export`) écrit chaque asset
/// compressé zstd à l'export — un bundle produit avant ce sprint (contenu brut) ne
/// décode plus, mais `assets/bundle/` est régénéré à chaque export, jamais versionné
/// tel quel entre deux formats.
pub fn bundle_bytes(key: &str) -> Option<Vec<u8>> {
    let compressed = BUNDLE.get_file(key).map(|f| f.contents())?;
    decompress(compressed)
}

/// Nom du fichier `assets/bundle/…` où l'export embarque un `settings.json` par défaut
/// (Sprint 3 de PHASE A, config hors éditeur) : `editor::export::ExportPanel::start` l'écrit
/// juste avant de lancer la compilation — même patron que la scène embarquée
/// (`assets/player_scene.json`), donc figé dans `BUNDLE` à la compilation comme le reste. Texte
/// brut (JSON), pas compressé zstd comme `bundle_bytes` : ce n'est pas un asset importé via
/// `copy_to_bundle`, juste deux champs qu'on écrit nous-mêmes à l'export.
pub const DEFAULT_SETTINGS_FILE: &str = "default_settings.json";

/// Contenu JSON du `settings.json` par défaut embarqué à l'export (Sprint 3), ou `None` si ce
/// build n'en a pas reçu un — export sans clé Firebase renseignée, ou binaire compilé en dehors
/// du panneau Build & Export (`cargo build` direct en développement, cas le plus courant).
pub fn default_settings_json() -> Option<&'static str> {
    BUNDLE.get_file(DEFAULT_SETTINGS_FILE)?.contents_utf8()
}

/// Décompression zstd pure Rust (`ruzstd`, pas de bindings C) : doit rester
/// compilable et correcte sur `wasm32-unknown-unknown`, où le player web lit aussi
/// ses assets embarqués (Sprint 115).
fn decompress(compressed: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(compressed).ok()?;
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).ok()?;
    Some(out)
}

// Redirection de `assets_dir()` pour les tests qui n'ont aucune variante `_at`
// atteignable à leur point d'entrée (ex. `spawn()` Lua, démo MMORPG) : sans ceci,
// ces tests écriraient réellement dans `~/.motor3derust/assets/`, ce qui échoue sur
// une machine où `$HOME` n'est pas accessible en écriture (CI, conteneur) et pollue
// le vrai dossier utilisateur ailleurs. `thread_local`, jamais `std::env::set_var`
// (mutation de process global, non sûre entre threads de test parallèles) : chaque
// test tourne sur son propre thread sous le harnais standard, donc positionner
// l'override juste pour ce thread suffit et n'affecte aucun autre test en cours.
#[cfg(test)]
thread_local! {
    static ASSETS_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Retire l'override d'`assets_dir()` du thread courant à la destruction — pour
/// qu'un panic en cours de test ne laisse pas le thread (réutilisé par le pool du
/// harnais) redirigé indéfiniment.
#[cfg(test)]
pub(crate) struct AssetsDirOverrideGuard;

#[cfg(test)]
impl Drop for AssetsDirOverrideGuard {
    fn drop(&mut self) {
        ASSETS_DIR_OVERRIDE.with(|cell| *cell.borrow_mut() = None);
    }
}

/// Redirige `assets_dir()` vers `dir` sur le thread courant, le temps de vie du
/// guard retourné — cf. `ASSETS_DIR_OVERRIDE`.
#[cfg(test)]
pub(crate) fn override_assets_dir_for_test(dir: PathBuf) -> AssetsDirOverrideGuard {
    ASSETS_DIR_OVERRIDE.with(|cell| *cell.borrow_mut() = Some(dir));
    AssetsDirOverrideGuard
}

/// Dossier des assets de projet (`~/.motor3derust/assets/`) — ou l'override du
/// thread courant si un test en a posé un (cf. `override_assets_dir_for_test`).
pub fn assets_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        if let Some(dir) = ASSETS_DIR_OVERRIDE.with(|cell| cell.borrow().clone()) {
            return Some(dir);
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".motor3derust").join("assets"))
}

/// Préfixe d'une donnée **utilisateur** — sauvegardes de partie, distinct
/// des assets de projet (`asset://`, en lecture pour l'essentiel) : ce schéma désigne
/// un dossier écrit **par le jeu lui-même** en cours de Play, sur desktop comme sur
/// Android (où `$HOME` n'existe pas — cf. `set_android_data_dir`).
pub const USER_SCHEME: &str = "user://";

/// Chemin fourni par `android_app.internal_data_path()` (cf. `lib.rs::android_main`),
/// seule façon d'obtenir un dossier écrivable garanti sur Android — il n'existe pas de
/// `$HOME` là-bas. `OnceLock` : posé une fois au démarrage, jamais réécrit ensuite.
#[cfg(target_os = "android")]
static ANDROID_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_android_data_dir(path: PathBuf) {
    let _ = ANDROID_DATA_DIR.set(path);
}

/// Dossier racine des données applicatives, par plateforme : sur Android, celui
/// posé par `set_android_data_dir` (`None` s'il n'a pas encore été appelé — ne
/// devrait pas arriver après `android_main` ; pas de `$HOME` là-bas) ; ailleurs,
/// `~/.motor3derust/`. Base commune à `user_dir()` et à `app::settings::
/// Settings::path()` — un seul point de résolution par plateforme pour toute
/// donnée persistée hors assets de scène (`assets_dir()` reste séparé : c'est
/// un sous-dossier desktop-only, jamais résolu sur Android).
pub fn app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        ANDROID_DATA_DIR.get().cloned()
    }
    #[cfg(not(target_os = "android"))]
    {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(".motor3derust"))
    }
}

/// Dossier des données utilisateur (`user://`) : sur Android, `app_data_dir()`
/// directement (pas de sous-dossier — l'app dispose déjà d'un dossier isolé par le
/// système) ; ailleurs, `app_data_dir()/save/`, à côté de `assets/` mais distinct
/// (données écrites par le jeu, pas importées par l'éditeur).
pub fn user_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        app_data_dir()
    }
    #[cfg(not(target_os = "android"))]
    {
        Some(app_data_dir()?.join("save"))
    }
}

/// Joint `name` à `dir` en refusant toute évasion du dossier prévu (Sprint
/// 105a-2, durcissement) : `None` si un composant de `name` est `..`
/// (`Component::ParentDir`), une racine (`Component::RootDir`) ou un préfixe
/// Windows-style (`Component::Prefix`) — analyse par composants de chemin
/// (`Path::components`), pas un test de sous-chaîne sur `".."` (qui aurait
/// des faux positifs sur un nom légitime comme `"foo..bar.png"` et des faux
/// négatifs sur des variantes d'encodage). Point de passage unique pour les
/// trois call-sites qui joignent un nom fourni par l'appelant (sauvegarde
/// ou scène potentiellement non fiable) à un dossier de base connu.
pub(crate) fn safe_join(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    use std::path::Component;
    let path = std::path::Path::new(name);
    if path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(dir.join(path))
}

/// Lit les octets d'un fichier `user://<nom>` (sauvegarde de partie). `None` si le
/// dossier utilisateur est indisponible, si `nom` tente de sortir du dossier prévu
/// (cf. `safe_join`), ou si le fichier n'existe pas encore (première utilisation —
/// pas une erreur).
pub fn read_user_bytes(name: &str) -> Option<Vec<u8>> {
    read_user_bytes_at(&user_dir()?, name)
}

/// Comme `read_user_bytes`, mais avec un dossier explicite plutôt que le vrai
/// `user_dir()` (Sprint 105a-3, isolation des tests) — même patron que
/// `register_asset_at` pour `assets_dir()` : le travail réel prend `dir` en
/// paramètre, `read_user_bytes` n'est qu'une enveloppe qui résout le vrai
/// dossier utilisateur.
pub fn read_user_bytes_at(dir: &std::path::Path, name: &str) -> Option<Vec<u8>> {
    std::fs::read(safe_join(dir, name)?).ok()
}

/// Écrit `data` dans `user://<nom>`, en créant le dossier utilisateur si besoin.
pub fn write_user_bytes(name: &str, data: &[u8]) -> Result<(), String> {
    let dir = user_dir().ok_or_else(|| "dossier utilisateur indisponible".to_string())?;
    write_user_bytes_at(&dir, name, data)
}

/// Comme `write_user_bytes`, mais avec un dossier explicite (Sprint 105a-3,
/// isolation des tests) — cf. la doc de `read_user_bytes_at`.
pub fn write_user_bytes_at(dir: &std::path::Path, name: &str, data: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target =
        safe_join(dir, name).ok_or_else(|| format!("nom de fichier invalide : « {name} »"))?;
    std::fs::write(target, data).map_err(|e| e.to_string())
}

/// Lit les octets d'un chemin quel que soit son schéma : `asset-id://` (référence
/// stable, résolue puis relue récursivement), `bundle://` (embarqué),
/// `asset://` (dossier projet, repli sur le bundle), ou chemin disque classique.
pub fn read_bytes(path: &str) -> Option<Vec<u8>> {
    if let Some(resolved) = resolve_asset_id(path) {
        return read_bytes(&resolved);
    }
    if let Some(key) = path.strip_prefix(SCHEME) {
        return bundle_bytes(key);
    }
    if let Some(key) = path.strip_prefix(ASSET_SCHEME) {
        if let Some(dir) = assets_dir()
            && let Some(target) = safe_join(&dir, key)
            && let Ok(b) = std::fs::read(target)
        {
            return Some(b);
        }
        // repli : l'asset peut être embarqué (player exporté)
        return bundle_bytes(key);
    }
    std::fs::read(path).ok()
}

/// Copie un fichier disque dans le dossier d'assets de projet et renvoie une référence
/// stable `asset-id://<uuid>` (contrairement à un `asset://<nom>` en dur, qui casse
/// dès qu'on renomme le fichier), ou `None` si la copie échoue. Idempotent pour un
/// schéma déjà connu (renvoyé tel quel, y compris un `asset://` hérité d'anciennes scènes).
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

/// Portée d'un prefab (complément UI, en plus du délete/de la confirmation de
/// création) : **général** (`prefabs/`, visible depuis n'importe quelle scène,
/// comportement historique) ou propre à une **scène/projet** nommé librement par
/// l'utilisateur (`prefabs/scenes/<nom nettoyé>/`) — ce moteur n'a pas de notion
/// de « projet » séparée d'une scène, donc pas de troisième niveau : le nom tapé
/// dans l'éditeur sert à la fois de nom de scène et de nom de projet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefabScope {
    General,
    Scene(String),
}

impl PrefabScope {
    /// Sous-dossier de `assets_dir()` où vivent les prefabs de cette portée.
    pub(crate) fn subdir(&self) -> std::path::PathBuf {
        match self {
            PrefabScope::General => std::path::PathBuf::from("prefabs"),
            PrefabScope::Scene(name) => {
                std::path::PathBuf::from("prefabs/scenes").join(sanitize_scene_name(name))
            }
        }
    }
}

/// Nettoyage d'un nom de scène/projet en nom de dossier valide — même règle que
/// `BuildConfig::safe_name`/`sanitize_prefab_name` (alphanumérique/`-`/`_`).
fn sanitize_scene_name(name: &str) -> String {
    let n: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if n.is_empty() { "Scene".into() } else { n }
}

/// Cœur de `list_prefabs`, paramétré par `dir` (testable sans toucher
/// `~/.motor3derust/assets/`, même raison que `register_asset_at` et consorts).
pub(crate) fn list_prefabs_at(dir: &std::path::Path, scope: &PrefabScope) -> Vec<(String, String)> {
    let subdir = scope.subdir();
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir.join(&subdir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let file_name = e.file_name();
            let name = file_name.to_str()?;
            let display = name.strip_suffix(".json")?.to_string();
            let key = subdir.join(name);
            let id = register_asset_at(dir, &key.to_string_lossy());
            Some((display, id))
        })
        .collect();
    out.sort();
    out
}

/// Prefabs disponibles pour une portée donnée (cf. `PrefabScope`) :
/// `(nom affiché, référence stable asset-id://<uuid>)`. `list_assets` ne les liste pas
/// — lecture non récursive du dossier d'assets, `prefabs/` n'y apparaît que comme un
/// seul sous-dossier, pas ses fichiers.
pub fn list_prefabs(scope: &PrefabScope) -> Vec<(String, String)> {
    match assets_dir() {
        Some(dir) => list_prefabs_at(&dir, scope),
        None => Vec::new(),
    }
}

/// Cœur de `delete_prefab`, paramétré par `dir`.
fn delete_prefab_at(dir: &std::path::Path, scope: &PrefabScope, name: &str) -> bool {
    let Some(path) = safe_join(&dir.join(scope.subdir()), &format!("{name}.json")) else {
        return false;
    };
    // L'entrée de manifeste correspondante (uuid → nom) n'est pas nettoyée : un
    // uuid orphelin se comporte déjà comme un prefab introuvable partout ailleurs
    // (`resolve_asset_id` renvoie toujours un chemin, mais plus rien à lire au bout —
    // `sync_prefab_instances`/`instantiate_prefab` tolèrent déjà ce cas, cf. leur doc).
    std::fs::remove_file(path).is_ok()
}

/// Supprime le fichier d'un prefab (`false` si absent ou suppression impossible).
/// N'affecte aucune instance déjà placée dans une scène (elles gardent leurs champs
/// actuels, `sync_prefab_instances` deviendra simplement un no-op pour elles).
pub fn delete_prefab(scope: &PrefabScope, name: &str) -> bool {
    match assets_dir() {
        Some(dir) => delete_prefab_at(&dir, scope, name),
        None => false,
    }
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

    #[test]
    fn decompress_reads_what_zstd_actually_wrote() {
        // Interopérabilité réelle : `copy_to_bundle` compresse avec le crate `zstd`
        // (bindings C, desktop), `decompress` décode avec `ruzstd` (pur Rust, tourne
        // aussi sur wasm32) — deux implémentations distinctes du même format, pas
        // supposées compatibles sans preuve.
        let original: Vec<u8> = b"abababababababababababababababababababab"
            .iter()
            .cycle()
            .take(4096)
            .copied()
            .collect();
        let compressed = zstd::stream::encode_all(&original[..], 0).unwrap();
        assert!(
            compressed.len() < original.len(),
            "un contenu répétitif doit rétrécir : {} -> {}",
            original.len(),
            compressed.len()
        );
        let decoded = decompress(&compressed).expect("décodage attendu");
        assert_eq!(decoded, original);
    }

    #[test]
    fn decompress_rejects_garbage() {
        assert!(decompress(b"pas un flux zstd").is_none());
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
    fn list_prefabs_at_lists_json_files_with_stable_ids_and_ignores_non_prefabs() {
        let dir = temp_assets_dir("list_prefabs");
        let prefabs = dir.join("prefabs");
        std::fs::create_dir_all(&prefabs).unwrap();
        std::fs::write(prefabs.join("gemme.json"), b"{}").unwrap();
        std::fs::write(prefabs.join("caisse.json"), b"{}").unwrap();
        // Un fichier non-JSON dans le même dossier (ex. temporaire) ne doit pas
        // apparaître comme prefab.
        std::fs::write(prefabs.join("notes.txt"), b"pas un prefab").unwrap();

        let listed = list_prefabs_at(&dir, &PrefabScope::General);
        let names: Vec<&str> = listed.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["caisse", "gemme"], "triés, sans notes.txt");
        for (_, id) in &listed {
            assert!(id.starts_with(ASSET_ID_SCHEME));
        }

        // Idempotence : un second appel renvoie les mêmes uuid (pas de doublons dans
        // le manifeste à chaque ouverture du navigateur d'assets).
        let listed_again = list_prefabs_at(&dir, &PrefabScope::General);
        assert_eq!(listed, listed_again);
    }

    #[test]
    fn list_prefabs_at_keeps_general_and_scene_scopes_separate() {
        let dir = temp_assets_dir("list_prefabs_scoped");
        let general = dir.join("prefabs");
        let scene = dir.join("prefabs/scenes/Mmorpg");
        std::fs::create_dir_all(&general).unwrap();
        std::fs::create_dir_all(&scene).unwrap();
        std::fs::write(general.join("cube.json"), b"{}").unwrap();
        std::fs::write(scene.join("repere.json"), b"{}").unwrap();

        let general_listed = list_prefabs_at(&dir, &PrefabScope::General);
        let gen_names: Vec<&str> = general_listed.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(gen_names, vec!["cube"]);

        let scene_listed = list_prefabs_at(&dir, &PrefabScope::Scene("Mmorpg".into()));
        let scene_names: Vec<&str> = scene_listed.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(scene_names, vec!["repere"]);
    }

    #[test]
    fn delete_prefab_at_removes_the_file_and_is_idempotent_on_a_second_call() {
        let dir = temp_assets_dir("delete_prefab");
        let prefabs = dir.join("prefabs");
        std::fs::create_dir_all(&prefabs).unwrap();
        std::fs::write(prefabs.join("jetable.json"), b"{}").unwrap();

        assert!(delete_prefab_at(&dir, &PrefabScope::General, "jetable"));
        assert!(!prefabs.join("jetable.json").exists());
        assert!(
            !delete_prefab_at(&dir, &PrefabScope::General, "jetable"),
            "supprimer un prefab déjà absent doit échouer proprement, pas paniquer"
        );
    }

    #[test]
    fn sanitize_scene_name_never_produces_an_empty_folder_name() {
        assert_eq!(sanitize_scene_name(""), "Scene");
        assert_eq!(sanitize_scene_name("   "), "Scene");
        assert_eq!(sanitize_scene_name("Mon Projet !"), "Mon_Projet__");
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
        // Un renommage ne doit pas casser la référence stable — c'est précisément
        // ce que `asset://<nom>` en dur ne permettait pas.
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

    #[test]
    fn safe_join_rejects_directory_traversal() {
        let dir = std::path::Path::new("/base");
        for evil in ["..", "../x", "a/../../b", "../../etc/passwd"] {
            assert!(
                safe_join(dir, evil).is_none(),
                "« {evil} » doit être rejeté (tentative d'évasion du dossier)"
            );
        }
    }

    #[test]
    fn safe_join_rejects_an_absolute_path() {
        let dir = std::path::Path::new("/base");
        assert!(
            safe_join(dir, "/etc/passwd").is_none(),
            "un chemin absolu doit être rejeté (ignorerait `dir`, cf. `PathBuf::join`)"
        );
    }

    #[test]
    fn safe_join_accepts_ordinary_relative_names() {
        let dir = std::path::Path::new("/base");
        assert_eq!(
            safe_join(dir, "foo.png"),
            Some(std::path::PathBuf::from("/base/foo.png"))
        );
        assert_eq!(
            safe_join(dir, "sub/foo.png"),
            Some(std::path::PathBuf::from("/base/sub/foo.png"))
        );
        // Un nom contenant ".." comme sous-chaîne (pas un composant ".." à part
        // entière) est légitime — `safe_join` analyse les composants de chemin,
        // pas une sous-chaîne brute.
        assert!(safe_join(dir, "foo..bar.png").is_some());
    }

    #[test]
    fn read_user_bytes_and_write_user_bytes_reject_traversal() {
        // Dossier temporaire isolé (Sprint 105a-3) plutôt que le vrai
        // `user_dir()` — variantes `_at`, aucune dépendance à `$HOME`.
        let dir = temp_assets_dir("user_traversal");
        assert!(
            write_user_bytes_at(&dir, "../evil.json", b"x").is_err(),
            "une tentative d'évasion doit être rejetée, pas silencieusement écrite ailleurs"
        );
        assert!(
            read_user_bytes_at(&dir, "../evil.json").is_none(),
            "une tentative d'évasion en lecture doit échouer, pas lire hors du dossier prévu"
        );
    }
}
