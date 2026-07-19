/// Marqueur nommé de la mini-carte (position monde x/z + étiquette) — cf.
/// `AppState::minimap_data`.
pub struct MinimapPoint {
    pub x: f32,
    pub z: f32,
    pub label: String,
}

/// Catégorie de décor repérable sur la mini-carte (cf. `classify_decor`) —
/// juste de quoi distinguer visuellement les types de terrain les plus
/// fréquents dans les scènes du jeu (hameau fortifié, biome forêt/rive), pas
/// une taxonomie exhaustive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MinimapDecorKind {
    Water,
    Building,
    Wall,
    Forest,
}

/// Marqueur de décor de la mini-carte (position monde x/z + catégorie) — cf.
/// `AppState::minimap_data`.
pub struct MinimapDecor {
    pub x: f32,
    pub z: f32,
    pub kind: MinimapDecorKind,
}

/// Marqueur de créature de la mini-carte (position monde x/z + appartenance
/// à la manche en cours) — cf. `AppState::minimap_data`. `active_wave` : ce
/// monstre appartient à la manche affichée par `wave_hud`
/// (`combat.wave == AppState::wave`), donc à la vague qui attaque
/// actuellement le joueur ; `false` pour toute créature hors système de
/// manches ou d'une manche déjà passée/pas encore révélée.
pub struct MinimapCreature {
    pub x: f32,
    pub z: f32,
    pub active_wave: bool,
}

/// Cf. `AppState::minimap_data`. `bounds` = (min_x, min_z, max_x, max_z).
pub struct MinimapData {
    pub player: Option<(f32, f32)>,
    pub allies: Vec<MinimapPoint>,
    pub creatures: Vec<MinimapCreature>,
    pub decor: Vec<MinimapDecor>,
    /// Taille (unités monde) de la grille utilisée par `thin_decor` pour
    /// `decor` — permet au rendu (`draw_minimap_decor`) de dimensionner ses
    /// pastilles pour qu'elles se rejoignent en régions continues (rendu
    /// « carte peinte ») plutôt que de laisser des points isolés.
    pub decor_cell: f32,
    pub bounds: (f32, f32, f32, f32),
}

/// Devine une catégorie de décor affichable sur la mini-carte (eau, bâtiment,
/// mur/rempart, forêt) à partir du nom de l'objet et du chemin de l'asset
/// glTF importé, le cas échéant. La scène n'a pas de champ de catégorie dédié
/// — seulement des noms descriptifs en français posés par les générateurs
/// procéduraux (cf. `scene/demos.rs`, ex. « Halte Sud-Ouest arbre ») et des
/// chemins de fichiers en anglais (`hamlet_house_a.glb`,
/// `nature_tree_windswept.glb`…). Heuristique par **mots entiers** (pas de
/// simple sous-chaîne) découpés sur la ponctuation/les espaces/underscores :
/// un `contains("eau")` naïf matchait à tort « hameau », « château »… — le
/// hameau fortifié de la démo en est plein. Même esprit pragmatique que la
/// détection de « Sol »/du joueur ailleurs dans ce module — approximatif par
/// construction, pensé comme un repère visuel en jeu, pas une classification
/// garantie exhaustive. Fonction pure (pas de `&self`) : testable sans
/// construire de scène complète.
pub(super) fn classify_decor(name: &str, asset_path: &str) -> Option<MinimapDecorKind> {
    let haystack = format!("{name} {asset_path}").to_lowercase();
    let words: std::collections::HashSet<&str> = haystack
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    const WATER: [&str; 6] = ["eau", "water", "shore", "lac", "riviere", "bassin"];
    const BUILDING: [&str; 5] = ["maison", "house", "hamlet", "cabane", "hut"];
    const WALL: [&str; 4] = ["mur", "wall", "rempart", "rampart"];
    const FOREST: [&str; 5] = ["arbre", "sapin", "tree", "foret", "forest"];
    let has_any = |keywords: &[&str]| keywords.iter().any(|k| words.contains(k));
    if has_any(&WATER) {
        Some(MinimapDecorKind::Water)
    } else if has_any(&BUILDING) {
        Some(MinimapDecorKind::Building)
    } else if has_any(&WALL) {
        Some(MinimapDecorKind::Wall)
    } else if has_any(&FOREST) {
        Some(MinimapDecorKind::Forest)
    } else {
        None
    }
}

/// Réduit `decor` à un marqueur par catégorie et par cellule d'une grille de
/// pas `cell` calée sur `bounds` — un décor scatter dense (une forêt de
/// centaines d'arbres, une rive faite de dizaines de tuiles d'eau) produit
/// sinon un nuage de points illisible sur la mini-carte (constaté en jeu).
///
/// Recale chaque marqueur gardé sur le **centre** de sa cellule plutôt que
/// la position brute du premier objet rencontré : combiné à un rayon de
/// rendu qui couvre la cellule (cf. `draw_minimap_decor`), des cellules
/// voisines de même catégorie se rejoignent alors en régions colorées
/// continues (terrain « peint », comme une vraie carte de jeu) au lieu d'un
/// semis de points disjoints — deuxième itération demandée en jeu après une
/// première version qui gardait la position brute (toujours un nuage, juste
/// plus clairsemé).
///
/// Fonction pure : testable sans construire de scène.
pub(super) fn thin_decor(
    decor: Vec<MinimapDecor>,
    bounds: (f32, f32, f32, f32),
    cell: f32,
) -> Vec<MinimapDecor> {
    let (min_x, min_z, ..) = bounds;
    let cell = cell.max(0.01);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for d in decor {
        let cx = ((d.x - min_x) / cell).floor() as i32;
        let cz = ((d.z - min_z) / cell).floor() as i32;
        if seen.insert((d.kind, cx, cz)) {
            out.push(MinimapDecor {
                x: min_x + (cx as f32 + 0.5) * cell,
                z: min_z + (cz as f32 + 0.5) * cell,
                kind: d.kind,
            });
        }
    }
    out
}
