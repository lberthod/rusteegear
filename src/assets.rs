//! Assets embarqués dans le binaire (Sprint 24) : modèles glTF et sons copiés
//! dans `assets/bundle/` à l'export, inclus à la compilation. Le player les résout
//! ainsi sans dépendre de chemins disque (qui n'existent pas sur l'appareil cible).
//!
//! Convention : un chemin de scène préfixé `bundle://<clé>` désigne un asset embarqué.

use std::path::PathBuf;

use include_dir::{Dir, include_dir};

static BUNDLE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/bundle");

/// Préfixe identifiant un asset embarqué (figé à la compilation, pour le player exporté).
pub const SCHEME: &str = "bundle://";

/// Préfixe d'un asset de projet (dossier `~/.motor3derust/assets/`, édition desktop).
pub const ASSET_SCHEME: &str = "asset://";

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

/// Lit les octets d'un chemin quel que soit son schéma : `bundle://` (embarqué),
/// `asset://` (dossier projet, repli sur le bundle), ou chemin disque classique.
pub fn read_bytes(path: &str) -> Option<Vec<u8>> {
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

/// Copie un fichier disque dans le dossier d'assets de projet et renvoie son chemin
/// `asset://<nom>` (ou `None` si la copie échoue). Idempotent pour un schéma connu.
pub fn import_to_assets(src: &str) -> Option<String> {
    if src.starts_with(ASSET_SCHEME) || src.starts_with(SCHEME) {
        return Some(src.to_string());
    }
    let dir = assets_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let name = std::path::Path::new(src).file_name()?.to_str()?.to_string();
    let dest = dir.join(&name);
    if std::fs::copy(src, &dest).is_err() {
        return None;
    }
    Some(format!("{ASSET_SCHEME}{name}"))
}

/// Liste les assets disponibles : fichiers du dossier projet + assets embarqués,
/// sous forme de chemins préfixés (`asset://…` / `bundle://…`), triés.
pub fn list_assets() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(dir) = assets_dir() {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if let Some(name) = e.file_name().to_str() {
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
}
