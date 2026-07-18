# Sprint création 3D — recréation maison du pack village

## État d'avancement

- [x] **Sprint 0 — préparation** : `scripts/blender/hamlet_common.py` (palette figée + helpers `mat`/`cube`/`cylinder`/`cone`/`blob`/`export`/`render_preview`, réutilisant telles quelles les teintes bois/pierre/toit/chaume de `gen_nature_pack.py`) et `scripts/blender/check_hamlet_pack.py` (QA, scan dynamique `hamlet_*.glb`) écrits et testés headless (`Blender --background --python ...`) avec un asset factice — OK. Constat technique confirmé en lisant `src/scene/import.rs:52` : le moteur ignore le canal alpha (`[r, g, b, _a]`), donc **aucune transparence n'est possible** — l'asset Fumée devra être un blob opaque stylisé, pas un plan semi-transparent (voir section Effets ci-dessous, mise à jour).
- [x] **Sprint 1 — structures & architecture** : `scripts/blender/gen_hamlet_structures.py`, 8/8 assets générés et validés par `check_hamlet_pack.py` (0 échec). Vignettes rendues et inspectées visuellement. Deux pièges résolus au passage (voir mémoire `charte-graphique-assets-maison` §Pièges vignette) : (1) caméra fixe recopiée de `creature_kit` montrait le dos des pièces directionnelles → remplacée par une caméra visant l'objet via contrainte `TRACK_TO` ; (2) les deux soleils recopiés de `creature_kit` laissaient la face avant (+Y) en noir pur → ajout d'un fond ambiant (`world` Background, force 0.35) dans `hamlet_common.render_preview`. Ces deux fixes bénéficient automatiquement à tous les sprints suivants (fonction partagée).
- [x] **Sprint 2 — props lot 1** : `scripts/blender/gen_hamlet_props.py`, 10/10 assets générés et validés (`check_hamlet_pack.py` : 18/18 sur l'ensemble du pack à date). Deux bugs corrigés : (1) jitter des blobs (sac, tas de sacs) pouvait faire descendre un sommet sous z=0 par tirage aléatoire → fix générique dans `hamlet_common.blob()` (bornage du z local selon l'échelle `squash`, profite à tous les futurs blobs près du sol) ; (2) croisillons de la caisse 0,02 m plus hauts que le bloc → alignés. Ajustement visuel : les anses du chaudron flottaient à côté de la panse (rayon plein pris à une hauteur où l'icosphère écrasée s'est déjà resserrée) → repositionnées plus bas/plus proches du centre.
- [x] **Sprint 3 — props lot 2 + décor/effets** : `scripts/blender/gen_hamlet_props2.py` (6 : foin, 2 étals de marché, 2 paquets, scie de scierie) + `scripts/blender/gen_hamlet_decor.py` (3 : rochers en amas, feu de camp, fumée). QA globale : 27/27. Deux points notables :
  - Ajout de `emission` à `hamlet_common.mat()` (repris de `creature_kit.material`) pour la flamme du feu de camp — mais **l'émissif glTF n'a aucun effet en jeu** (le moteur ne lit que `base_color_factor`, cf. `src/scene/import.rs`) : il ne sert qu'à la vignette de contrôle. Une vraie lueur en jeu se règlerait côté scène (`obj.emissive`), pas dans l'asset. Documenté dans la charte graphique.
  - Vignette : `scene.view_settings.view_transform` forcé à `"Standard"` dans `hamlet_common.render_preview` — le défaut (AgX) délavait la flamme émissive en jaune pâle au lieu de l'orange attendu, ne représentant plus fidèlement la couleur exportée.
- [x] **Sprint 4 — bâtiments lot 1** : `scripts/blender/gen_hamlet_buildings.py`, 6/6 assets générés et validés (QA globale 33/33). Deux fonctions de toit ajoutées à `hamlet_common.py` (`pitched_roof` deux pans, `hip_roof` quatre pans/pyramide), réutilisables telles quelles par le Sprint 5. Bug important corrigé : la première version de `pitched_roof`, dérivée de `gen_nature_pack.gen_cabin`, reprenait un facteur `× 1.55` sur la position des pans qui n'était valable QUE pour les proportions portée/hauteur de la cabane d'origine — appliqué à d'autres proportions (forge), les deux pans du toit se retrouvaient visiblement disjoints/en porte-à-faux. Remplacé par la position géométriquement correcte (milieu du segment faîte→égout, sans facteur ad hoc).
- [x] **Sprint 5 — bâtiments lot 2** : `scripts/blender/gen_hamlet_buildings2.py`, 6/6 assets générés et validés (QA globale 39/39). Suite au retour utilisateur après le Sprint 4 (« bâtiments trop simplistes, je veux voir des planches, de la pierre, des détails »), trois nouveaux helpers ajoutés à `hamlet_common.py` avant d'attaquer ce lot :
  - `plank_wall` — mur + rainures verticales en saillie (planches individuelles visibles).
  - `stone_coursing` — mur + lignes d'assises horizontales en saillie (pierre/brique appareillée).
  - `shingled_roof` — toit à deux pans en rangées de tuiles teintées en alternance (remplace le pan plein de `pitched_roof` là où le détail compte).
  Vertex count nettement plus élevé qu'au Sprint 4 (672 à 1080 verts/bâtiment contre 130-320) — cohérent avec le budget « bâtiment ~800-2000 » de la charte. Détails bespoke par bâtiment : auberge (soubassement à assises + étage à planches, volets, cheminée, potence à enseigne), scierie (charpente ouverte, pile de planches en désordre), écurie (double porte de grange dont un vantail entrouvert, botte de foin), moulin (tour ronde à bandes d'assises, grande roue à aubes à 10 pales), gloriette (plancher à lattes, garde-corps à balustres), puits (margelle en 10 blocs de pierre radiaux, pas un cylindre lisse).
  Deux bugs corrigés : (1) roue du moulin positionnée trop bas, les aubes basses plongeaient sous le sol → moyeu remonté ; (2) roue du moulin placée du côté -X, invisible sur la vignette (caméra générique côté +X/+Y) → déplacée côté +X.
- [x] **Sprint 6 — QA finale & revue** : 39/39 fichiers `hamlet_*.glb` présents (correspondance exacte avec la liste des 39 assets planifiés), `check_hamlet_pack.py` sans échec sur l'ensemble. Revue visuelle de toutes les vignettes (dont un second passage sur les assets non encore inspectés individuellement — porte droite, étal A, paquet A, botte de foin) : silhouettes lisibles, palette cohérente, aucune régression. **Le pack est terminé.** Intégration bundle/scènes (remplacement ou coexistence avec `village_*` dans `assets/bundle/`) reste hors scope de ce sprint, à traiter séparément si demandé.
- [x] **Sprint 7 — remplacement effectif du pack tiers** (demandé après le Sprint 6) : suppression complète des assets `village_*` et bascule vers `hamlet_*` dans le jeu réel, pas seulement dans `assets/models/`. Étapes :
  1. Ajout de `hamlet_chair.glb` (40ᵉ asset) — seule pièce encore utilisée par les scènes sans équivalent maison (`village_chair.glb`, marché de `hameau_gdd_demo`).
  2. `src/scene/demos.rs` : 106 littéraux `"village_X.glb"` → `"hamlet_X.glb"` (remplacement textuel exact, un seul fichier concerné, vérifié qu'aucune autre occurrence de `"village_` n'existait ailleurs dans `src/`). Commentaires de provenance mis à jour (3 endroits).
  3. **Scène embarquée/livrée** (`assets/player_scene.json` + `assets/bundle/`, ce qui est réellement compilé dans le binaire via `include_dir!`) : 29 entrées `bundle://mNN_village_X.glb` réellement référencées → recompressées zstd depuis les nouveaux `hamlet_X.glb` sous le **même indice mNN**, remplacement chirurgical du seul champ `path` (pas de réécriture JSON complète : `serde_json`/le module `json` de Python ne formatent pas les flottants pareil, ça aurait pollué le diff de milliers de lignes sans rapport — repéré et corrigé avant de committer quoi que ce soit).
  4. Suppression des ~40 fichiers `mNN_village_*.glb` orphelins du bundle (jamais référencés par `player_scene.json`, vérifié avant suppression) + suppression des 40 `assets/models/village_*.glb` sources.
  5. `cargo test` (557 tests, dont `the_embedded_scene_resolves_its_bundle_creatures`), `cargo fmt --check`, `cargo clippy -- -D warnings` : tous verts.
  Non fait volontairement : pas de commit (jamais demandé explicitement) ; `village_cart.glb` n'avait aucun usage actif, pas de remplacement nécessaire.

Note sur l'outillage : le MCP Blender interactif (add-on) ne fonctionne qu'avec une session Blender **GUI** ouverte — en headless (`-b`), l'add-on l'indique lui-même (« cannot start server in background mode ») et se contente de s'enregistrer/se désenregistrer sans bloquer l'export. Le pipeline de ce sprint reste donc le CLI headless déjà en usage dans tout le repo (`/Applications/Blender.app/Contents/MacOS/Blender --background --python <script>`), pas le MCP.

## Objectif

Recréer, en style maison et de façon procédurale (scripts Blender `bpy` headless, comme les packs créatures/flore/pierre déjà présents dans `scripts/blender/`), un équivalent des 39 assets du « Medieval Village Pack » (Quaternius, CC0, via Poly Pizza) déjà retraités en `village_*.glb` par `scripts/blender/import_village_pack.py`.

Aucune géométrie ni fichier tiers n'est copié : seules la fonction et la silhouette générale de chaque objet d'origine servent de référence. Les nouveaux fichiers coexistent avec `village_*` — pas de remplacement dans le bundle ou les scènes pour l'instant (décision reportée à un sprint ultérieur, hors scope ici).

Toute création doit respecter la charte graphique : mémoire [charte-graphique-assets-maison](../../.claude/projects/-Users-berthod-Desktop-motor3derust/memory/charte-graphique-assets-maison.md) *(fichier mémoire persistant du projet, à relire avant d'écrire chaque script)*. En résumé : ≤3 teintes par objet, aucune texture (`base_color_factor` uniquement), un mesh joint par objet, sol à z=0 Blender / Y-up glTF, échelle avant rotation, vignette EEVEE 640×480.

## Méthode

- Un script `gen_hamlet_*.py` par lot thématique (jamais un script unique pour les 39 assets — trop gros pour QA/déboguer incrémentalement).
- Patron à suivre : `scripts/blender/gen_nature_pack.py` / `scripts/blender/gen_stone_pack.py` (pas `creature_kit.py`, pensé pour du skinné/animé) :
  1. `reset_scene()`
  2. `mat()` — matériaux définis une fois à partir de la table de couleurs de la charte graphique
  3. fonctions de construction par primitives (assemblage de cubes/cylindres/cônes)
  4. `bpy.ops.object.join()` puis `transform_apply` (échelle avant rotation)
  5. export GLB (`export_yup=True`) + rendu vignette
- QA après chaque lot via `check_hamlet_pack.py` (à écrire au Sprint 0, adapté de `scripts/blender/check_creatures.py` : retirer les vérifications de clips/os, garder vertex-sous-z=0, un seul mesh joint par fichier, absence de texture référencée).
- Nommage : préfixe `hamlet_*.glb`, suffixes `_a`/`_b` pour variantes, vignette `<nom>_preview.png`.

## Liste complète des 39 assets

### Bâtiments (12) — complexité haute/moyenne

| Nom cible (maison) | Origine pack | Complexité | Script prévu |
|---|---|---|---|
| Tour à cloche | Bell Tower | haute | `gen_hamlet_buildings.py` |
| Forge | Blacksmith | haute | `gen_hamlet_buildings.py` |
| Caserne | Fantasy Barracks | haute | `gen_hamlet_buildings.py` |
| Maison A | Fantasy House (var. 1) | moyenne | `gen_hamlet_buildings.py` |
| Maison B | Fantasy House (var. 2) | moyenne | `gen_hamlet_buildings.py` |
| Maison C | Fantasy House (var. 3) | moyenne | `gen_hamlet_buildings.py` |
| Auberge | Fantasy Inn | haute | `gen_hamlet_buildings2.py` |
| Scierie | Fantasy Sawmill | haute | `gen_hamlet_buildings2.py` |
| Écurie | Fantasy Stable | moyenne | `gen_hamlet_buildings2.py` |
| Moulin | Mill | haute | `gen_hamlet_buildings2.py` |
| Gloriette | Gazebo | moyenne | `gen_hamlet_buildings2.py` |
| Puits | Well | faible | `gen_hamlet_buildings2.py` |

### Structures / architecture (8) — complexité faible

| Nom cible | Origine | Script prévu |
|---|---|---|
| Porte ronde | Door Round | `gen_hamlet_structures.py` |
| Porte droite | Door Straight | `gen_hamlet_structures.py` |
| Fenêtre ronde | Round Window | `gen_hamlet_structures.py` |
| Fenêtre A | Window (var. 1) | `gen_hamlet_structures.py` |
| Fenêtre B | Window (var. 2) | `gen_hamlet_structures.py` |
| Clôture | Fence | `gen_hamlet_structures.py` |
| Escalier | Stairs | `gen_hamlet_structures.py` |
| Dalle de chemin | Path Straight | `gen_hamlet_structures.py` |

### Mobilier / props (16) — complexité faible/moyenne

| Nom cible | Origine | Complexité | Script prévu |
|---|---|---|---|
| Sac ouvert | Bag Open | faible | `gen_hamlet_props.py` |
| Sac | Bag | faible | `gen_hamlet_props.py` |
| Sacs (tas) | Bags | faible | `gen_hamlet_props.py` |
| Tonneau | Barrel | faible | `gen_hamlet_props.py` |
| Cloche | Bell | faible | `gen_hamlet_props.py` |
| Banc A | Bench (var. 1) | faible | `gen_hamlet_props.py` |
| Banc B | Bench (var. 2) | faible | `gen_hamlet_props.py` |
| Chariot | Cart | moyenne | `gen_hamlet_props.py` |
| Chaudron | Cauldron | faible | `gen_hamlet_props.py` |
| Caisse | Crate | faible | `gen_hamlet_props.py` |
| Botte de foin | Hay | faible | `gen_hamlet_props2.py` |
| Étal de marché A | Market Stand (var. 1) | moyenne | `gen_hamlet_props2.py` |
| Étal de marché B | Market Stand (var. 2) | moyenne | `gen_hamlet_props2.py` |
| Paquet A | Package (var. 1) | faible | `gen_hamlet_props2.py` |
| Paquet B | Package (var. 2) | faible | `gen_hamlet_props2.py` |
| Scie de scierie | Sawmill Saw | moyenne | `gen_hamlet_props2.py` |

### Décor naturel (1)

| Nom cible | Origine | Script prévu |
|---|---|---|
| Rochers | Rocks | `gen_hamlet_decor.py` (ou fusion dans `gen_stone_pack.py` si cohérent) |

### Effets (2)

| Nom cible | Origine | Complexité | Script prévu |
|---|---|---|---|
| Feu de camp | Bonfire | moyenne (émissif) | `gen_hamlet_decor.py` |
| Fumée | Smoke | moyenne (blob opaque stylisé — le moteur ignore l'alpha, confirmé `src/scene/import.rs:52`, pas de plan semi-transparent) | `gen_hamlet_decor.py` |

**Total : 12 + 8 + 16 + 1 + 2 = 39 assets.**

## Plan d'exécution par sprints

- **Sprint 0 — préparation.** Valider/relire la mémoire `charte-graphique-assets-maison`, figer la table de couleurs RGB linéaire par type de matériau (bois clair, bois foncé, pierre, chaume, toit, tissu) en constantes de script, écrire `check_hamlet_pack.py`.
- **Sprint 1 — structures & architecture** (8 assets, faible complexité) : premier lot pour valider le patron de script sur des objets simples avant les bâtiments. `gen_hamlet_structures.py`.
- **Sprint 2 — props lot 1** (10 assets : sacs, tonneau, cloche, bancs, chariot, chaudron, caisse). `gen_hamlet_props.py`.
- **Sprint 3 — props lot 2** (6 : foin, étals, paquets, scie) + décor/effets (rochers, feu, fumée). `gen_hamlet_props2.py` + `gen_hamlet_decor.py`.
- **Sprint 4 — bâtiments lot 1** (6 : tour, forge, caserne, 3 maisons). `gen_hamlet_buildings.py` — factoriser murs/toit/porte/fenêtre en fonctions réutilisables (partagées avec le lot 2).
- **Sprint 5 — bâtiments lot 2** (6 : auberge, scierie, écurie, moulin, gloriette, puits). `gen_hamlet_buildings2.py`, réutilise les fonctions du sprint 4.
- **Sprint 6 — QA finale & revue.** `check_hamlet_pack.py` sans échec sur les 39 fichiers, revue visuelle groupée des vignettes. Intégration bundle/scènes (remplacement éventuel de `village_*`) explicitement **hors scope** — sprint séparé si demandé plus tard.

Chaque sprint se clôt par : script(s) exécutés en headless (`Blender --background --python ...`), vignettes générées, `check_hamlet_pack.py` sans échec, commit au format déjà en usage dans le projet (français, cf. mémoire `rusteegear-project-conventions`).

## Critères de validation

- Aucun asset ne réutilise de géométrie ou de fichier du pack tiers — génération procédurale pure.
- `check_hamlet_pack.py` : 0 échec sur vertex-sous-sol, 1 mesh joint par fichier, pas de texture référencée.
- Respect de la palette et des ≤3 teintes/objet de la charte graphique.
- Vignette de contrôle produite pour chaque asset.
- Nommage cohérent (`hamlet_*`), sans collision avec `village_*`.
- Mémoire charte graphique déjà en place avant l'écriture du premier script de génération (fait : voir `charte-graphique-assets-maison.md`).

## Fichiers de référence (à lire, pas à modifier pour ce sprint)

- `scripts/blender/gen_nature_pack.py`, `scripts/blender/gen_stone_pack.py` — patron statique à suivre.
- `scripts/blender/import_village_pack.py` — proportions/orientations d'origine à consulter sans copier.
- `scripts/blender/check_creatures.py` — modèle pour `check_hamlet_pack.py`.
- `scripts/blender/creature_kit.py` — pour réimporter `material()`/`_lod()`.
- `ANALYSE_DESIGN_VISUEL.md` — règles de charte visuelle existantes (référencées, pas dupliquées).
