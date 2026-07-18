# Création 3D — pack « siège du hameau » (suite de sprintcration3delement.md)

## Objectif

Après le pack « hameau maison » (`hamlet_*.glb`, 40 assets, terminé et intégré au jeu — cf. `sprintcration3delement.md`), ce document planifie une nouvelle salve d'une quarantaine d'assets 3D **directement motivés par le GDD** (`GDD_MMORPG.md`), pas par une envie générique d'ajouter du contenu.

Toute création doit continuer à respecter la charte graphique : mémoire [charte-graphique-assets-maison](../../.claude/projects/-Users-berthod-Desktop-motor3derust/memory/charte-graphique-assets-maison.md) *(fichier mémoire persistant du projet, à relire avant d'écrire chaque script)*. Aucune règle n'a changé depuis le pack hameau : ≤3 teintes par objet, aucune texture (`base_color_factor` uniquement), un mesh joint par objet, sol à z=0 Blender / Y-up glTF, échelle avant rotation, vignette EEVEE 640×480, émissif Blender = vignette uniquement (le moteur ignore l'émissif glTF, cf. `src/scene/import.rs`).

## Analyse GDD → assets : ce qui manque vraiment

Avant de choisir quoi que ce soit, on a vérifié ce qui existe déjà pour éviter les doublons :

| Catégorie | Pack existant | Compte | Statut |
|---|---|---|---|
| Créatures/bestiaire | `creature_*`, `monster_*` | ~106 | couvert, le GDD ne réclame rien de neuf (juste un recasting de données, §5.4) |
| Faune décorative | `fauna_*` | 38 | couvert |
| Flore/pierre/mécanismes | `nature_*` | 89 | couvert |
| Objets ramassables | `item_*` | 30 | couvert |
| Hameau (bâtiments/props) | `hamlet_*` | 40 | couvert, terminé au sprint précédent |

Le GDD, lui, mentionne des éléments concrets qui ne correspondent à AUCUN de ces packs :

- **Mode Escorte** : *« Amener un chariot lent d'une porte du hameau à l'autre ; les créatures le ciblent en priorité »* (§4/§9.3) — un chariot de braises thématique, pas la charrette générique du hameau.
- **Feu communal** : foyer central de la fiction, teinte orange/émissive (§2.1/§10.1) — `hamlet_bonfire` existe mais en version « prop de décor », pas « pièce signature de la place ».
- **Signal de vague** : *« portes qui s'embrasent à l'arrivée d'une vague »* comme alternative à la minimap (§17.5).
- **Arme nommée** : le tableau d'armes du GDD cite « Boule de feu / Éclair / **Boulet** » (§5.1) — un projectile visuel distinct des icônes `item_*`.
- **Boss** : « Aînée de la lande », combat unique à PV massifs (§4) — a minima un socle/autel de mise en scène.

Et surtout, un écart vérifié directement dans le code, pas dans la doc : **les remparts du hameau sont aujourd'hui des primitives sans détail.** `src/scene/demos.rs:6555-6650` (fonction `hameau_gdd_demo`) construit les 8 pans de remparts avec `box_seg` — des boîtes à couleur plate (`WALL_COLOR = [0.34, 0.33, 0.36]`), sans assise de pierre, sans créneau, sans tour d'angle — alors que les bâtiments du même hameau (Sprint 5) ont des murs à assises visibles et des toits en tuiles. C'est l'écart de qualité le plus net entre deux parties de la même scène, et le plus simple à corriger avec des assets dédiés.

**Conclusion** : le nouveau pack se concentre sur les **fortifications, le siège, et les props des modes de jeu** — pas sur un élargissement de la vie du hameau ou un donjon (options écartées, cf. décision utilisateur).

## Méthode

- **Nouveau préfixe** : `siege_*.glb` — vérifié sans collision avec `hamlet_`/`nature_`/`item_`/`fauna_`/`creature_`/`monster_` (`ls assets/models/ | grep siege` vide).
- **Réutilisation intégrale de `scripts/blender/hamlet_common.py`** : `mat`, `cube`, `cylinder`, `cone`, `blob`, `export`, `render_preview`, `pitched_roof`, `hip_roof`, `shingled_roof`, `plank_wall`, `stone_coursing`, et la palette existante (`STONE`, `STONE_DARK`, `WOOD`, `WOOD_DARK`, `METAL`, `METAL_DARK`, `FIRE`, `CLOTH`, `CLOTH_DARK`). **Aucune nouvelle palette** : le hameau et ses remparts doivent utiliser la même pierre, le même bois — sinon la place et les fortifications ne se répondront pas visuellement (règle « une teinte par système » de la charte).
- **Deux nouveaux helpers à ajouter à `hamlet_common.py`** (pas un nouveau module séparé — le siège est une extension directe du vocabulaire du hameau) :
  - `crenellations(prefix, mat, width, n, base_z)` — rangée de merlons (blocs alternés) en haut d'un mur, pour donner aux remparts le même niveau de détail que `stone_coursing` donne aux façades.
  - `banner(prefix, cloth_mat, pole_mat, location, width, height)` — poteau + panneau de tissu, réutilisé par les 4 assets de bannière/fanion de la liste ci-dessous (évite de dupliquer la même géométrie 4 fois).
- **QA** : `check_siege_pack.py`, copie directe de `check_hamlet_pack.py` avec `hamlet_*` → `siege_*` (scan dynamique, mêmes contraintes : mesh joint, pas de vertex sous z=0, pas de texture).
- **Intégration en jeu (remplacer les `box_seg` par les nouveaux imports) : hors scope de ce document**, comme convenu pour le hameau — un sprint de génération d'assets ne mélange pas avec un sprint d'intégration scène/bundle.

## Liste complète des ~40 assets

### Fortifications (10) — habillent les remparts, aujourd'hui des `box_seg` plats

| Nom cible | Rôle | Complexité | Script prévu |
|---|---|---|---|
| Segment de mur | Remplace un pan de `box_seg`, assises de pierre visibles | moyenne | `gen_siege_walls.py` |
| Segment d'angle | Coin de rempart, deux faces d'assises | moyenne | `gen_siege_walls.py` |
| Tour d'angle | Tour ronde/carrée + plateforme de tir | haute | `gen_siege_walls.py` |
| Porte de rempart (fermée) | Porte principale, vantaux + ferrures | haute | `gen_siege_walls.py` |
| Porte de rempart (embrasée) | Variante signal de vague (§17.5), flammes émissives en vignette | haute | `gen_siege_walls.py` |
| Module de créneau | Rangée de merlons (helper `crenellations`) | faible | `gen_siege_walls.py` |
| Chemin de ronde | Plateforme de bois en haut de mur | faible | `gen_siege_walls2.py` |
| Escalier de rempart | Version plus large de `hamlet_stairs`, pour l'accès aux remparts | faible | `gen_siege_walls2.py` |
| Poterne | Petite porte secondaire discrète | moyenne | `gen_siege_walls2.py` |
| Bastion de renfort | Contrefort d'angle, silhouette massive | moyenne | `gen_siege_walls2.py` |

### Props des modes de jeu (10)

| Nom cible | Rôle GDD | Complexité | Script prévu |
|---|---|---|---|
| Chariot de braises | Convoi du mode Escorte (§4/§9.3) | moyenne | `gen_siege_modes.py` |
| Brasero communal | Feu signature de la place (§2.1/§10.1) | moyenne (émissif vignette) | `gen_siege_modes.py` |
| Boulet | Projectile de l'arme « Boulet » (§5.1) | faible | `gen_siege_modes.py` |
| Bannière de vague | Change d'état visuel selon la progression (helper `banner`) | faible | `gen_siege_modes.py` |
| Autel de l'Aînée | Socle de mise en scène du boss (§4) | moyenne | `gen_siege_modes.py` |
| Tas de trophées | Repère de progression du mode Survie | faible | `gen_siege_modes.py` |
| Balise de spawn | Marqueur visuel de point d'apparition de vague | faible | `gen_siege_modes2.py` |
| Caisse de réserve | Munitions/ressources du mode défense | faible | `gen_siege_modes2.py` |
| Herse | Grille de porte relevable | moyenne | `gen_siege_modes2.py` |
| Rangée de pieux | Défense anti-monstre, ligne de piquets aiguisés | faible | `gen_siege_modes2.py` |

### Lande environnante (10) — au-delà de `nature_*`, décor extérieur au hameau

| Nom cible | Complexité | Script prévu |
|---|---|---|
| Rocher de lande | faible | `gen_siege_lande.py` |
| Arbre mort tourmenté | faible | `gen_siege_lande.py` |
| Ossements épars | faible | `gen_siege_lande.py` |
| Menhir de lande | faible | `gen_siege_lande.py` |
| Broussaille épineuse | faible | `gen_siege_lande.py` |
| Mare stagnante | faible | `gen_siege_lande.py` |
| Ravine de terrain | faible | `gen_siege_lande2.py` |
| Poteau de bannière en ruine | faible | `gen_siege_lande2.py` |
| Cairn de guerre | faible | `gen_siege_lande2.py` |
| Touffe de brume basse | faible (opaque, pas de transparence — cf. charte) | `gen_siege_lande2.py` |

### Signalétique et effets 3D (10)

| Nom cible | Complexité | Script prévu |
|---|---|---|
| Bannière de mode (Escorte/Boss/Survie/Vagues) | faible (helper `banner`, ×4 teintes) | `gen_siege_signal.py` |
| Corne d'alerte | faible | `gen_siege_signal.py` |
| Torche de rempart | faible | `gen_siege_signal.py` |
| Marqueur de zone au sol | faible | `gen_siege_signal.py` |
| Fanion de couleur d'équipe | faible (helper `banner`) | `gen_siege_signal.py` |
| Cage du chef | moyenne | `gen_siege_signal2.py` |
| Statue commémorative | moyenne | `gen_siege_signal2.py` |
| Trophée de fin de manche | faible | `gen_siege_signal2.py` |
| Portail de fin stylisé | moyenne | `gen_siege_signal2.py` |
| Panneau directionnel de rempart | faible | `gen_siege_signal2.py` |

**Total : 10 + 10 + 10 + 10 = 40 assets.**

## Plan d'exécution par sprints

- **Sprint 0 — préparation** : ajouter `crenellations()` et `banner()` à `hamlet_common.py`, les tester isolément sur un mur factice (même méthode que pour `plank_wall`/`stone_coursing` au Sprint 5 du hameau : générer un petit test, vérifier la vignette, avant de les utiliser dans les vrais scripts). Écrire `check_siege_pack.py`.
- **Sprint 1 — fortifications lot 1** (5 : mur, angle, tour, porte fermée, porte embrasée) : `gen_siege_walls.py`.
- **Sprint 2 — fortifications lot 2** (5 : créneau, chemin de ronde, escalier, poterne, bastion) : `gen_siege_walls2.py`.
- **Sprint 3 — props des modes lot 1** (6 : chariot, brasero, boulet, bannière de vague, autel, trophées) : `gen_siege_modes.py`.
- **Sprint 4 — props des modes lot 2** (4 : balise, caisse, herse, pieux) + **lande lot 1** (6 : rocher, arbre mort, ossements, menhir, broussaille, mare) : `gen_siege_modes2.py` + `gen_siege_lande.py`.
- **Sprint 5 — lande lot 2** (4 : ravine, poteau en ruine, cairn, brume) + **signalétique lot 1** (5 : bannières de mode, corne, torche, marqueur, fanion) : `gen_siege_lande2.py` + `gen_siege_signal.py`.
- **Sprint 6 — signalétique lot 2** (5 : cage, statue, trophée, portail, panneau) : `gen_siege_signal2.py`.
- **Sprint 7 — QA finale & revue** : `check_siege_pack.py` sur les 40 fichiers, revue visuelle groupée des vignettes.

Chaque sprint se clôt par : script(s) exécutés en headless, vignettes générées et inspectées, `check_siege_pack.py` sans échec, commit au format en usage (français, cf. mémoire `rusteegear-project-conventions`).

## Critères de validation

- Palette ≤3 teintes/objet, réutilisant exclusivement les constantes déjà définies dans `hamlet_common.py` (pas de nouvelle teinte sans raison — cf. règle « une teinte par système »).
- `check_siege_pack.py` : 0 échec (mesh joint, pas de vertex sous le sol, pas de texture référencée).
- Vignette de contrôle pour chaque asset, inspectée visuellement avant de clore le sprint.
- Nommage `siege_*.glb`, sans collision avec les préfixes existants.
- Les remparts et le hameau doivent se répondre visuellement une fois les deux packs côte à côte (même pierre, même bois) — vérifié en plaçant mentalement/visuellement une tour `siege_*` à côté d'un bâtiment `hamlet_*` en comparant les vignettes.

## Hors scope explicite

- L'intégration réelle dans `hameau_gdd_demo()` (remplacer les segments `box_seg` des remparts par les nouveaux `siege_*.glb`, mettre à jour `assets/bundle/` et `assets/player_scene.json`) est un **chantier séparé**, à ouvrir explicitement après que le pack soit généré et validé — même logique que pour le hameau (génération d'assets ≠ intégration scène/bundle).
- Aucun script Blender n'est écrit ni exécuté par ce document : c'est un tour de planification, la génération réelle commence au Sprint 0 listé ci-dessus.
