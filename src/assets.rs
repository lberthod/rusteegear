//! Assets embarqués dans le binaire (Sprint 24) : modèles glTF et sons copiés
//! dans `assets/bundle/` à l'export, inclus à la compilation. Le player les résout
//! ainsi sans dépendre de chemins disque (qui n'existent pas sur l'appareil cible).
//!
//! Convention : un chemin de scène préfixé `bundle://<clé>` désigne un asset embarqué.

use include_dir::{Dir, include_dir};

static BUNDLE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/bundle");

/// Préfixe identifiant un asset embarqué.
pub const SCHEME: &str = "bundle://";

/// Si `path` désigne un asset embarqué, renvoie sa clé (le nom dans le bundle).
pub fn strip_scheme(path: &str) -> Option<&str> {
    path.strip_prefix(SCHEME)
}

/// Octets d'un asset embarqué, ou `None` s'il est absent du bundle.
pub fn bundle_bytes(key: &str) -> Option<&'static [u8]> {
    BUNDLE.get_file(key).map(|f| f.contents())
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
