# Réflexion — construire la carte du jeu (« Le Hameau des Braises »)

> Document de réflexion/analyse, pas un plan de sprint chiffré comme
> `sprint2audijeu0718.md`. Objectif : rassembler en un seul endroit l'inventaire
> réel des assets 3D disponibles, l'état réel de la carte aujourd'hui, ce que le
> GDD exige d'une bonne carte, et une proposition de guideline pour un sprint
> **dédié à la composition de la carte** (pas à la génération de nouveaux
> assets) mené par une session Claude connectée au MCP Blender.

---

## 1. Pourquoi ce document

La carte du jeu (le hameau fortifié + sa lande environnante) est aujourd'hui
composée « à la main » dans du code Rust (`Scene::hameau_gdd_demo()`,
`src/scene/demos.rs`) : des appels `poser(...)` avec des coordonnées littérales,
écrits sans jamais voir le résultat en 3D pendant l'écriture. Ça a produit un
résultat fonctionnel (le pack siège est intégré, testé, verrouillé par CI —
cf. `docs/AUDIT_JEU_2026-07-18.md` §2/§12) mais **composé à l'aveugle** : le
placement est un « meilleur effort », pas une composition vue et corrigée.

Un sprint dédié, avec le MCP Blender connecté, permet de faire l'inverse :
composer la scène **en la regardant** (viewport, rendus de prévisualisation),
avec les vrais assets, avant de la traduire en code Rust. C'est un changement
de méthode, pas juste une suite de tâches — d'où un document de réflexion
séparé plutôt qu'une nouvelle section du plan de sprint chiffré.

---

## 2. Inventaire réel des prefabs GLB disponibles

Compté directement dans `assets/models/` (412 fichiers `.glb`, hors images de
prévisualisation) :

| Préfixe | Compte | Contenu | Doc source | Script(s) |
|---|---|---|---|---|
| `creature*` / `monster_*` | ~106 | Bestiaire jouable (patrouille, script Lua ou `AiChaser`) | `sprintcration3delement.md`, `rapport_qualite_creatures_vs_hyper3d.md` | `gen_creature*.py`, `creature_kit.py` |
| `hamlet_*` | 40 | Bâtiments et props du hameau (maisons, feu communal, charrette, etc.) | `sprintcration3delement.md` | `gen_hamlet_*.py`, `hamlet_common.py` |
| `siege_*` | 40 | Fortifications + props des modes de jeu (remparts, tours, portes, chariot d'Escorte, autel du Boss, bannières…) | `creation3DBlendersuite.md`, `creationAnimation3DBlendersuite.md` | `gen_siege_*.py`, `hamlet_common.py`, `siege_anim_common.py` |
| `nature_*` | 123 | Flore/pierre/mécanismes (arbres, pins, buissons, rochers, signalétique) | `sprintcration3delement.md` (packs flore successifs) | `gen_flora_pack*.py`, `gen_nature_*.py` |
| `fauna_*` | 29 | Faune décorative neutre (cerf, lapin, écureuil, renard, sanglier, hérisson, chèvre, raton laveur, taupe, oiseaux, chauve-souris, insectes…) | `sprintcration3delement.md` | `gen_fauna_decor_pack*.py`, `gen_menagerie_pack*.py` |
| `item_*` | 30 | Objets ramassables (armes, consommables) | `sprintcration3delement.md` | `gen_items_pack*.py` |
| `grotto_*` | 20 | Décor de grotte/souterrain, style organique (métaballes) | `creation3DBlenderOrganicSuite.md` | `gen_grotto_*.py`, `organic_common.py` |
| `shore_*` | 20 | Décor de rive/lac, style organique (métaballes) | `creation3DBlenderOrganicSuite.md` | `gen_shore_*.py`, `organic_common.py` |
| `fairy_hero` | 1 | Placeholder héroïne/joueur (cf. audits — décrite comme « sphère placeholder » dans le GDD) | — | `gen_fairy_hero.py` |

**Total : un catalogue déjà riche et cohérent en charte** (voir §4) — ce sprint
n'a **pas besoin de générer un seul nouvel asset** pour construire une
première carte complète et crédible. C'est un sprint de **composition**, pas
de **production**.

## 2 bis. La carte réelle aujourd'hui — précisément, pas approximativement

Cette section corrige une première lecture trop rapide : il existe en réalité
**trois choses différentes** qu'il ne faut pas confondre, et leur articulation
n'est **pas entièrement vérifiée** — c'est la première tâche du sprint, avant
toute composition nouvelle.

### 2 bis.1 — Trois fonctions Rust, trois visions de la carte

| Fonction | Fichier | Demi-étendue | Contenu réel |
|---|---|---|---|
| `Scene::mmorpg_demo()` | `src/scene/demos.rs:2043` | `MMORPG_HALF = 36.0` (carte 72×72 m, commentaire à jour) | La grande carte biome : prairie centrale, forêt NE (2 clairières), **lac + 2 rivières + 1 coude à l'ouest** (rects d'eau réels, ci-dessous), **rizières en damier au sud-ouest** (5 parcelles, coordonnées réelles ci-dessous), **promontoire rocheux à l'est** (tour de guet), un village construit avec le pack `hamlet_*` (66 occurrences : maisons, puits, auberge, forge, écurie, étals, clôtures…), relief réel (`MeshKind::Terrain`) sur la marge ouest, système de scatter procédural seedé pour forêt/prairie/lisières avec zones d'exclusion (`EXCL_EAU_ROUTES`, `EXCL_ZONES_AMENAGEES`, `EXCL_CLAIRIERES`). **Aucun usage de `siege_*`** dans cette fonction (0 occurrence vérifiée). |
| `Scene::hameau_gdd_demo()` | `src/scene/demos.rs:6427` | `HALF = 24.0` (« fort 48×48 ») | Le hameau **fortifié** : remparts/tours/portes en `siege_*`, dressing de lande en `siege_*`. C'est la fonction utilisée comme source de vérité par le test de synchro décor (`sync_embedded_scene_hameau_from_the_demo`). |
| `assets/player_scene.json` (la **scène réellement servie**) | — | `Sol.transform.scale = (90, 1, 90)` (vérifié en lisant le JSON directement) | **Corrigé (Phase A, 2026-07-18) : c'est en fait la quasi-totalité de `hameau_gdd_demo()` telle quelle**, pas un mélange des deux fonctions. `Rempart`/`Tour` (44/3), `Faune` (119, construite par l'appel à `faune_scatter` **dans `hameau_gdd_demo()` elle-même**, `src/scene/demos.rs:8497`), le village en enceinte (`hamlet_*` — Forge, Puits, Auberge, Écurie, Moulin… 27 chemins `hamlet_*` distincts vérifiés dans le JSON), l'eau (`Lac`/`Rivière ouest`/`Rivière sud`, coordonnées vérifiées `Lac=(-42,42) 24×24`, `Rivière ouest=(-31.5,0) 5×58`, `Rivière sud=(0,31.5) 58×5`) et la rizière stub (`Rizière du sud`, un seul `Plane`, `demos.rs:7976`) viennent **tous** de `hameau_gdd_demo()` (Sol : `demos.rs:6660` ; eau : `demos.rs:7954-7973`). **Seules les 26 `Créature` sont réellement copiées depuis `mmorpg_demo()`** (filtre `starts_with("Créature")`, `src/scene/mod.rs:3325` → `demos.rs:6683-6714`). L'ancienne ligne de ce tableau affirmant que la Faune et l'eau venaient de `mmorpg_demo()` était **fausse** — vérifié en comparant les coordonnées exactes (celles de `mmorpg_demo()`, §2 bis.3 ci-dessous, ne correspondent à aucun objet du JSON servi). `hamlet` (village) et `Rizière` **ne sont donc pas absents** de la scène servie — c'est le village *en enceinte* et la rizière stub de `hameau_gdd_demo()` qui y sont, pas le village *hors les murs* / les 5 parcelles de `mmorpg_demo()`. Seul `Promontoire` reste à 0 occurrence : `hameau_gdd_demo()` n'en construit aucun. |

### 2 bis.2 — Deux écarts concrets, réconciliés Phase A (2026-07-18)

1. **L'écart 72/90 n'est pas un bug de redimensionnement — ce sont deux
   fonctions différentes, jamais censées partager une échelle.**
   `sol.transform.with_scale(Vec3::new(90.0, 1.0, 90.0))` est écrit **en dur**
   dans `hameau_gdd_demo()` elle-même (`src/scene/demos.rs:6660`) ; ce n'est
   pas dérivé de `MMORPG_HALF`. `mmorpg_demo()` construit de son côté un sol
   à `2 × MMORPG_HALF = 72` (`demos.rs:2056`), mais **cette valeur n'entre à
   aucun moment dans le calcul du sol de `hameau_gdd_demo()`**.
   `sync_embedded_scene_hameau_from_the_demo` (`src/scene/mod.rs:3325`) prend
   `Scene::hameau_gdd_demo()` comme base complète de la scène servie et n'en
   copie que les 26 objets `Créature` depuis `Scene::mmorpg_demo()`
   (`demos.rs:6683-6714`, filtre `starts_with("Créature")`) — remparts,
   village, eau, faune et **le sol lui-même** restent ceux de
   `hameau_gdd_demo()`, à son échelle 90×90, indépendamment de tout calcul
   fait dans `mmorpg_demo()`. Il n'y a donc **rien à corriger dans
   `demos.rs`** : le sol de la scène servie est correct et volontaire à
   90×90 m ; l'attente initiale de « 72 » était fondée sur l'hypothèse fausse
   que `hameau_gdd_demo()` dérivait sa taille de `mmorpg_demo()`, ce qui n'a
   jamais été le cas.
2. **Décision actée : le village hors les murs, le promontoire et les 5
   parcelles de rizières de `mmorpg_demo()` ne sont PAS réintroduits dans la
   scène servie.** `hameau_gdd_demo()` est une composition **autonome et déjà
   cohérente** : elle a son propre village en enceinte (`hamlet_*` — Forge,
   Puits, Auberge, Écurie, Moulin, étals…), sa propre eau (`Lac`, `Rivière
   ouest`, `Rivière sud`, coordonnées ci-dessous), sa propre forêt en anneau
   et une rizière stub — tout cela déjà verrouillé par
   `the_embedded_scene_decor_and_wildlife_match_the_demo` (CI). Fusionner en
   plus le village hors les murs, le promontoire et les rizières de
   `mmorpg_demo()` (système de coordonnées 72×72, origine et scatter propres,
   jamais visualisés à côté du fort — cf. l'ancien point 2 sur l'alignement
   non prouvé) referait courir le risque de chevauchement avec les remparts
   sans bénéfice clair, alors que `hameau_gdd_demo()` couvre déjà chacun de
   ces rôles (village, eau, forêt, un peu de rizière) à sa manière. **Seul
   manque réellement absent de toute composition existante : un promontoire**
   — hors scope de ce sprint (aucune des étapes 1-2 n'en a besoin ; à
   reconsidérer dans un sprint futur si le GDD l'exige).
   `mmorpg_demo()` reste une fonction indépendante et plus ancienne (démo
   MMORPG PC↔mobile, 72×72 m), toujours utile comme **source des définitions
   de créatures** réutilisées par `hameau_gdd_demo()`, mais sa propre
   composition d'environnement (village hors les murs, promontoire, 5
   rizières, forêt NE) n'a pas vocation à être synchronisée vers la scène
   servie — ses commentaires (`demos.rs:2029-2042`) décrivent correctement
   *sa propre* carte, pas la scène servie, donc ne nécessitent pas de
   correction.
3. **Correction du tableau §2 bis.3 en conséquence** : les coordonnées d'eau
   qui y étaient données étaient celles de `mmorpg_demo()`, une fonction dont
   l'eau n'est plus la cible de composition (décision ci-dessus). L'Étape 1
   du plan (§5) doit habiller l'eau de `hameau_gdd_demo()` — voir les
   coordonnées corrigées ci-dessous, remplaçant l'ancien tableau.

   | Zone d'eau (`hameau_gdd_demo()`, source réelle de la scène servie) | Position (x, z) | Taille (X × Z) | Habillage `shore_*` à ajouter |
   |---|---|---|---|
   | Rivière ouest | (-31.5, 0.0) | 5 × 58 | Berges, rochers lissés, racines immergées |
   | Rivière sud | (0.0, 31.5) | 58 × 5 | Berges, débris flottants |
   | Lac | (-42.0, 42.0) | 24 × 24 | Ondulations (`shore_water_ripple`), berge naturelle, nid (`shore_nest`), faune échouée |

   Les ponts, exclusions de scatter (`EXCL_EAU_ROUTES` et équivalents) et la
   forêt en anneau propres à `hameau_gdd_demo()` restent à vérifier
   précisément (nom/ligne) avant l'Étape 1 — non fait dans cette Phase A,
   volontairement limitée à la réconciliation d'échelle et à la décision
   village/promontoire/rizières.

### 2 bis.3 — Coordonnées du système d'eau de `mmorpg_demo()` (référence historique — **plus la cible**, voir §2 bis.2 point 3)

> **Mise à jour Phase A (2026-07-18)** : ce tableau documente le système
> d'eau de `mmorpg_demo()`, pas celui de la scène servie. La décision §2 bis.2
> point 2 exclut `mmorpg_demo()` de la composition — **utiliser le tableau
> corrigé du §2 bis.2 point 3** (coordonnées `hameau_gdd_demo()`) pour
> l'Étape 1, pas celui-ci. Conservé ici à titre de référence historique et
> pour documenter fidèlement `mmorpg_demo()` elle-même.

Le système d'eau de `mmorpg_demo()` est déjà mature : 4 rectangles d'eau,
murs invisibles rastérisés automatiquement sur une grille de 1 m (évite les
brèches de continuité), 2 ponts comme seuls passages. **Zéro asset organique
(`shore_*`) n'y est utilisé aujourd'hui** — juste un plan d'eau plat (`aplat`)
et des murs invisibles. C'est la cible naturelle et déjà cartographiée pour
le pack `shore_*` (20 assets validés, jamais posés) :

| Zone d'eau | Rect (x_min, z_min, x_max, z_max) | Ouverture | Habillage `shore_*` à ajouter |
|---|---|---|---|
| Rivière nord | (-28.0, -36.0, -24.0, -6.0) | Pont 2 (z≈-10) | Berges, rochers lissés, brume basse |
| Coude | (-28.0, -8.0, -16.0, -4.0) | — | Rochers de rivière, débris flottants |
| Lac | (-26.0, -2.0, -12.0, 10.0) | — | Ondulations (`shore_water_ripple`), berge naturelle, nid (`shore_nest`), faune échouée |
| Rivière sud | (-18.0, 10.0, -14.0, 36.0) | Pont 1 (z≈14) | Berges, racines immergées, souche submergée |

### 2 bis.4 — Ce qui n'est pas encore posé dans la scène servie (mise à jour)

D'après `docs/AUDIT_JEU_2026-07-18.md` §2 et §14 (Phase B), complété par les
constats ci-dessus :
- Le pack `siege_*` est intégré (remparts, tours, portes, dressing de place et
  de lande) — **verrouillé par test non-`#[ignore]`**
  (`the_embedded_scene_decor_and_wildlife_match_the_demo`, `src/scene/mod.rs`).
- Les packs `grotto_*`/`shore_*` (organiques, 40 assets validés) **ne sont
  posés nulle part** — ni dans la scène servie, ni dans `hameau_gdd_demo()`,
  ni dans `mmorpg_demo()`. Le système d'eau qui les attend naturellement
  existe déjà **dans la scène servie elle-même** (`hameau_gdd_demo()`, voir
  le tableau corrigé du §2 bis.2 point 3, pas celui du §2 bis.3 qui décrit
  `mmorpg_demo()`) mais est habillé en primitives génériques. La marge de
  relief ouest, elle, n'a été creusée que pour `mmorpg_demo()` (§2 bis.2
  Étape 2 révisée) — son équivalent dans `hameau_gdd_demo()` reste à
  vérifier.
- **Mise à jour Phase A (2026-07-18)** : contrairement à ce qu'affirmait
  cette ligne jusqu'ici, le village et une rizière stub **sont bien arrivés**
  dans la scène servie — via `hameau_gdd_demo()`, pas via `mmorpg_demo()`
  (voir §2 bis.1, §2 bis.2 point 2). Seul le **promontoire** n'existe dans
  aucune des deux fonctions autrement que comme description dans les
  commentaires de `mmorpg_demo()`, et la décision Phase A ne le réintroduit
  pas (hors scope de ce sprint).
- Le relief réel (terrain à heightmap, `sprintreflecion.md` Phase K) **ne
  couvre qu'une bande étroite à l'ouest de la carte**, pas la carte entière —
  le reste de la lande est un plan plat avec du décor posé dessus, pas un
  terrain sculpté. C'est un compromis assumé, documenté comme tel.

---

## 3. Charte graphique à respecter (rappel, ne pas re-décider)

Référence : mémoire de session `charte-graphique-assets-maison` — à relire
avant toute décision de composition, pas seulement de génération :
- ≤ 3 teintes par objet, aucune texture (`base_color_factor` uniquement).
- Un seul mesh joint par objet exporté (sauf skinné animé).
- Sol Blender à z=0, export Y-up (glTF), échelle appliquée avant rotation.
- Émissif = vignette de preview uniquement (le moteur ignore l'émissif glTF,
  cf. `src/scene/import.rs`) — ne pas compter dessus pour du bloom en jeu, le
  brillant réel vient des propriétés posées côté Rust (`obj.emissive`).
- Règle GDD §10.1 : « froid = décor, chaud/saturé = enjeu » — la lande et le
  hameau restent dans une palette pierre/bois/froid ; le orange/magenta/cyan
  restent réservés au joueur, aux projectiles et aux menaces (créatures).
- La faune (`fauna_*`) est **neutre et intouchable** (§7.3 du GDD) —
  jamais `attackable`, jamais de composant `ai_chaser`.

Aucune de ces règles n'est à re-décider dans ce sprint : elles sont déjà
posées, vérifiées, et le nouveau contenu (composition) doit s'y conformer,
pas les rouvrir.

---

## 4. Ce que le GDD exige d'une bonne carte (§7, à ne pas oublier en composant)

### 4.1 Vocabulaire spatial (§7.1)
- **La place** (feu communal) : espace ouvert, aucune protection — zone
  dangereuse en cas de Meute.
- **Les ruelles** : goulets à 1-2 créatures de front — contre-jeu de la Meute,
  piège si on y recule face au Colosse.
- **Cours et recoins** : poches défendables à une seule entrée — refuge du
  soin/réanimation.
- **Remparts/plateformes basses** : verticalité légère, postes de tir et
  chemins d'Éclaireur — hauteur = répit, jamais invulnérabilité (les
  créatures attendent en bas).

### 4.2 Règles de construction (§7.2) — non négociables, vérifiables en jeu
1. **Tout obstacle doit être détectable par les sondes IA** (raycast à 0,6 m
   de hauteur) — un muret « visuel » qui bloque le joueur mais pas l'IA fige
   les patrouilles. Piège déjà rencontré et documenté (mémoire de session
   « Décor solide vs sondes créatures ») — se valide **en jeu**, pas à l'œil
   dans Blender.
2. **Aucun point de la carte à plus de ~8 s de course d'un espace ouvert** —
   se faire piéger doit être une erreur de jugement, jamais une fatalité de
   géométrie.
3. **Spawns de créatures aux lisières, jamais dans le dos du joueur** —
   cohérence fiction (la horde vient de la lande) et équité.
4. **L'anneau de spawn joueurs doit tenir 16 positions distinctes** sans
   interpénétration — vérifié, pas supposé.
5. **La faune (§7.3) est du dressing narratif**, pas du remplissage : moutons
   et poules dans les cours, chouette sur les remparts, lucioles autour du
   feu communal (la luciole = motif visuel du jeu, « une braise qui vole »).

### 4.3 Contrainte de process déjà apprise (à ne pas répéter)
- **Piège solid_spots** (mémoire « Piège solid_spots : 3 tableaux DemoDecor ») :
  un objet solide placé dans le mauvais tableau de décor échappe au scatter
  procédural et peut bloquer une voie de circulation sans que personne ne le
  remarque avant un playtest. Toute nouvelle zone de décor doit être vérifiée
  contre les voies de circulation existantes (`EXCL_EAU_ROUTES` et
  équivalents), pas juste "ajoutée quelque part qui semblait libre".
- **Écrasement de la scène embarquée à l'export** (mémoire du même nom) :
  toute composition doit être faite dans `Scene::hameau_gdd_demo()`
  (`src/scene/demos.rs`), **jamais directement dans `assets/player_scene.json`**
  — ce fichier est régénéré par un test de synchro et toute modification
  manuelle directe serait perdue au prochain export. Le fichier JSON n'est
  qu'une sortie compilée, jamais une source à éditer à la main.

---

## 5. À quoi la carte doit ressembler — plan de composition fondé sur l'existant

Contrairement à une première approche qui proposait un schéma générique
inventé, la carte réelle (§2 bis) a déjà une géographie précise et cohérente.
Le travail de ce sprint n'est donc **pas d'inventer une macro-structure**,
c'est d'**étendre et d'habiller ce qui existe déjà**, dans cet ordre :

### Étape 0 (préalable, avant toute composition visuelle) — réconciliation — **RÉSOLUE (Phase A, 2026-07-18)**
1. ~~Élucider l'écart d'échelle du sol (90 servi vs 72 attendu)~~ — **fait,
   voir §2 bis.2 point 1** : ce n'est pas un écart à corriger, `90` est la
   valeur voulue et codée en dur dans `hameau_gdd_demo()`, indépendante de
   `mmorpg_demo()`. Aucun changement de code nécessaire.
2. ~~Décider du sort du village/promontoire/rizières~~ — **fait, voir §2 bis.2
   point 2** : le village hors les murs, le promontoire et les 5 parcelles de
   rizières de `mmorpg_demo()` ne sont **pas** réintroduits — `hameau_gdd_demo()`
   a déjà sa propre version de chacun (sauf promontoire, hors scope). Les
   commentaires de `mmorpg_demo()` restent corrects tels quels : ils décrivent
   sa propre carte (72×72, démo MMORPG PC↔mobile), jamais la scène servie.
3. Point 3 (vérification visuelle de non-chevauchement village/fort) devient
   **sans objet** : ce village-là n'est plus candidat à la réintroduction.

### Étape 1 — habiller le système d'eau existant avec `shore_*`
Les 3 rects d'eau de `hameau_gdd_demo()` (source réelle de la scène servie)
et leurs coordonnées exactes sont donnés au §2 bis.2 point 3 (corrigé Phase
A — remplace l'ancien tableau à 4 rects issu de `mmorpg_demo()`, qui n'est
plus la cible) — ne pas les redessiner, les **habiller** : berges
organiques, rochers lissés, brume basse, faune échouée neutre autour du lac
(zone calme, cohérente avec §7.3 du GDD). Les ouvertures de pont et le
`GRID` invoqués par l'ancien texte appartiennent à `mmorpg_demo()` ; leurs
équivalents dans `hameau_gdd_demo()` (s'il y en a) restent à identifier
avant de composer — non fait dans cette Phase A.

### Étape 2 — poser `grotto_*` sur la marge de relief ouest existante
Le relief réel (Phase K, `sprintreflecion.md`) a déjà commencé à creuser un
« petit bassin intégré à un contrefort + tunnel/arceau statique » sur cette
même bande ouest — c'est l'emplacement naturel pour les 20 assets `grotto_*`
(entrée de grotte, stalactites/stalagmites, champignons lumineux…), pas une
nouvelle zone à inventer ailleurs sur la carte. **À vérifier avant de
composer** : ce relief (Phase K) a été creusé pour `mmorpg_demo()` —
confirmer qu'un relief ouest équivalent existe bien dans `hameau_gdd_demo()`
(sol `Plane` plat, pas `Terrain`, d'après §2 bis.1) avant de poser quoi que
ce soit ; sinon cette étape n'a pas de cible dans la scène servie.

### Étape 3 — **à revoir** (visait à tort les biomes de `mmorpg_demo()`)
Le texte original (« forêt NE, rizières SO, promontoire est », zones
d'exclusion `EXCL_EAU_ROUTES`/`EXCL_ZONES_AMENAGEES`/`EXCL_CLAIRIERES`)
décrit le scatter procédural de `mmorpg_demo()`, pas celui de
`hameau_gdd_demo()`. Or la décision ci-dessus exclut ces biomes-là de la
scène servie. `hameau_gdd_demo()` a sa propre forêt (« forêt en anneau »,
27→70 m autour du fort, `demos.rs:8447`) et ses propres
fonctions de scatter (`foret_scatter` à `demos.rs:6549`, `faune_scatter` à
`demos.rs:6615`, appelées respectivement `demos.rs:8465` et `demos.rs:8497`)
— l'étape doit être
réécrite pour vérifier **cette** forêt-là et ses propres zones d'exclusion
(à identifier), pas celles de `mmorpg_demo()`. Non fait dans cette Phase A.

### Ce qui reste vrai du raisonnement initial (règles GDD, §4)
- Le hameau fortifié (place, ruelles, cours) reste le cœur du gameplay de
  manche — ne pas le retoucher sans raison de gameplay, ce sprint compose
  **autour**, pas dedans.
- Les lisières de spawn (§7.2 règle 3) doivent rester lisibles après l'ajout
  de `shore_*`/`grotto_*` — un rocher de rive mal placé à une lisière de
  vague briserait la règle « la horde vient de la lande, jamais dans le dos ».
- Chemins lisibles entre zones : les deux ponts existants (Pont 1/Pont 2)
  sont déjà les points de passage pensés pour l'eau — le sprint peut les
  mettre en valeur (bannière, signalétique `nature_*` déjà existante) plutôt
  que d'en ajouter de nouveaux sans raison.

Ce plan est un point de départ à corriger **en le voyant** dans Blender, pas
une carte à imposer sans vérification visuelle — c'est justement tout
l'intérêt de faire ce sprint avec le MCP Blender plutôt qu'en éditant des
coordonnées à l'aveugle dans `src/scene/demos.rs`.

---

## 5 bis. Quoi composer via Blender, quoi écrire directement en Rust

Ce n'est **pas un choix binaire global** ("tout Blender" ou "tout Rust") — c'est
une décision **par élément de carte**, tranchée par une seule question : *cet
élément demande-t-il un jugement visuel non trivial (densité, occlusion,
largeur perçue, alignement avec de l'existant), ou ses coordonnées sont-elles
déjà connues/déductibles ?*

### Toujours interdit, quel que soit le cas
- **Éditer `assets/player_scene.json` à la main.** Ce n'est jamais une source,
  seulement une sortie compilée (§4.3, §8) — que la composition en amont ait
  été faite via Blender ou directement en Rust ne change rien à cette règle.
  La seule destination d'écriture reste `Scene::hameau_gdd_demo()`
  (`src/scene/demos.rs`), régénérée ensuite par le test de synchro.

### Table de décision par élément

| Élément | Méthode recommandée | Pourquoi |
|---|---|---|
| Étape 0 — réconciliation (écart d'échelle 90/72, sort du village) | **Rust/lecture de code seule**, pas Blender — **résolue Phase A** | Question de calcul et de décision produit, pas de composition visuelle. Résultat : 90 est correct et volontaire (pas un bug), village/promontoire/rizières de `mmorpg_demo()` non réintroduits (§2 bis.2). |
| Habillage du système d'eau (`shore_*` sur les 3 rects de `hameau_gdd_demo()`, §2 bis.2 point 3) | **Blender d'abord**, coordonnées déjà connues à ajuster visuellement | Les rects sont fixes et documentés (corrigés Phase A), mais la densité et le placement des berges/rochers (éviter le "collier de perles", §7.5) sont un jugement d'œil — exactement le cas d'usage visé par ce sprint (§6). |
| `grotto_*` sur la marge de relief ouest (§5 Étape 2) | **Blender d'abord**, après vérification qu'un relief équivalent existe dans `hameau_gdd_demo()` | Emplacement identifié dans `mmorpg_demo()` (Phase K) ; à confirmer côté `hameau_gdd_demo()` avant de composer (non fait en Phase A). |
| Vérification de la forêt en anneau de `hameau_gdd_demo()` (§5 Étape 3, à revoir) | **Rust/lecture de code seule** | Scatter déjà en place (`foret_scatter`/`faune_scatter`) — l'étape doit d'abord être réécrite pour cibler cette forêt-ci (pas celle de `mmorpg_demo()`) avant d'être exécutée. |
| Réintroduction du village hors les murs (`mmorpg_demo()`) dans la scène servie | **Non retenu (Phase A)** | `hameau_gdd_demo()` a déjà son propre village en enceinte ; fusionner en plus celui de `mmorpg_demo()` referait courir le risque de chevauchement jamais vérifié (§2 bis.2 point 2, historique) sans bénéfice clair. |
| Ajustements mineurs sur du décor déjà posé et testé (hameau fortifié `siege_*`) | **Ni l'un ni l'autre par défaut** | Hors scope (§10) — ne pas retoucher sans raison de gameplay. |

### Ce que "Blender d'abord" veut dire concrètement
Composer/ajuster dans Blender ne dispense jamais de l'étape manuelle de
traduction en Rust (§6, §7 points 7-10) : Blender produit un plan de
coordonnées et des captures d'écran de référence, **jamais** un export
automatique vers `demos.rs` ou vers le JSON. Si le MCP Blender est
indisponible au moment de composer un élément listé "Blender d'abord",
l'alternative de repli est d'écrire directement en Rust avec les coordonnées
déjà connues (§2 bis.2 point 3 pour l'eau de `hameau_gdd_demo()` — pas le
§2 bis.3, qui documente `mmorpg_demo()`) puis de corriger visuellement plus tard en jeu (`cargo run`
+ Play) — moins bon qu'une vraie itération Blender, mais strictement meilleur
que de laisser le sprint bloqué en attendant que le MCP réponde.

---

## 6. Pourquoi c'est un sprint à part, avec Blender MCP

Composer une carte, contrairement à générer un asset isolé, demande de juger
en continu des questions qu'aucune règle écrite ne peut trancher à l'avance :
« est-ce que cette allée est trop large ? », « est-ce que ce bosquet cache
trop la vue depuis la place ? », « est-ce que la rive casse le rythme entre
deux ruelles ? ». Ces questions ne se répondent qu'en **regardant** — d'où le
besoin d'un outil qui affiche vraiment la scène (viewport Blender, rendu de
prévisualisation), pas d'écrire des triplets `(x, y, z)` en aveugle et
d'attendre un `cargo run` pour découvrir le résultat.

C'est aussi un sprint dont le livrable est différent des sprints de
génération d'assets précédents : **pas de nouveau fichier `.glb`**, mais une
scène Blender de composition (fichier `.blend` de référence, optionnel mais
recommandé pour itérer visuellement) qui sert ensuite de plan pour écrire
`Scene::hameau_gdd_demo()` en Rust — la traduction code reste manuelle
(les prefabs sont importés par `poser()`/`import_single_model` en Rust, pas
par un export Blender→scène automatique), mais le *plan* de placement est
désormais vu et corrigé avant d'être codé, pas inventé pendant qu'on écrit du
Rust.

---

## 7. Guideline pour la session dédiée (checklist à suivre dans l'ordre)

### Avant de commencer
1. Relire ce document en entier, plus la mémoire `charte-graphique-assets-maison`
   et le §7 du GDD (déjà résumés ci-dessus, mais la source fait foi).
2. Lister dans Blender les assets déjà disponibles par catégorie (import
   groupé en aperçu, pas un par un) pour se donner une vision d'ensemble
   avant de composer — utiliser `get_objects_summary`/`get_blendfile_summary_*`
   du MCP Blender si un fichier de référence existe déjà, sinon importer les
   `.glb` cités au §2 dans une scène de travail dédiée (pas le fichier de
   production).
3. Confirmer le périmètre exact de la zone à composer (dimensions actuelles
   du hameau : `HALF=24` selon `integration_siege_scene.md` — vérifier la
   valeur courante dans `src/scene/demos.rs` avant de dessiner, elle a pu
   changer) et la taille totale de la lande environnante souhaitée.

### Pendant la composition
4. Composer **zone par zone**, pas toute la carte d'un coup : hameau (déjà
   fait, à ne pas retoucher sans raison) → une zone d'eau → une zone de
   lisière de spawn → une zone de grotte → le reste de la lande de
   remplissage. Rendre/regarder après chaque zone, pas seulement à la fin.
5. Pour chaque zone, vérifier **avant de passer à la suivante** :
   - Aucun élément de décor ne bloque un chemin de circulation évident (règle
     §4.2.1 — même si la vérification technique raycast se fera plus tard en
     jeu, l'œil humain doit déjà repérer les blocages visuellement flagrants).
   - Cohérence de palette avec la charte (§3) — pas de teinte qui jure.
   - Densité raisonnable (éviter le "collier de perles" ou le remplissage
     compact façon supermarché — cf. leçon retenue du pack organique, §2).
6. Prendre des captures d'écran/rendus de prévisualisation à chaque étape
   significative (`render_viewport_to_path`/`render_thumbnail_to_path` du MCP
   Blender) — elles servent de preuve et de référence au moment de traduire
   en coordonnées Rust, et de point de comparaison si une itération ultérieure
   dérive du plan initial.

### Traduire en jeu
7. Traduire la composition en appels `poser(...)`/import dans
   `Scene::hameau_gdd_demo()` (`src/scene/demos.rs`) — zone par zone, dans le
   même ordre que la composition Blender, avec les captures d'écran de
   l'étape 6 comme référence de coordonnées relatives (le passage
   Blender→Rust n'est pas automatisé, cf. §6).
8. Après chaque zone traduite : lancer `cargo run` (éditeur) et vérifier
   visuellement en Play, **en particulier** :
   - Aucune créature ne se fige contre un élément de décor nouvellement
     ajouté (règle §4.2.1 — c'est le seul test qui vaut vraiment, celui du
     jeu réel, pas Blender).
   - Aucun élément solide n'a atterri dans le mauvais tableau de décor
     (piège solid_spots, §4.3).
9. Une fois toutes les zones traduites : régénérer `assets/player_scene.json`
   via le test de synchro dédié (`sync_embedded_scene_hameau_from_the_demo`,
   `#[ignore]`, cf. `integration_siege_scene.md`), copier les nouveaux `.glb`
   vers `assets/bundle/` avec la numérotation attendue, recompiler.
10. Faire tourner la suite de tests complète, en particulier
    `the_embedded_scene_decor_and_wildlife_match_the_demo` (le verrou CI posé
    en Phase B de `sprint2audijeu0718.md`) et les tests d'authoring de vagues
    (`mmorpg_demo_waves_follow_the_gdd_authoring_rules` ou équivalent) — la
    composition ne doit jamais casser un test déjà vert.

### Definition of done
- Chaque zone du schéma §5 (ou sa version corrigée après composition
  visuelle) existe dans le jeu, pas seulement dans un fichier `.blend`.
- Playtest manuel : parcourir la carte à pied en Play, confirmer qu'aucune
  créature ne se bloque, qu'aucun chemin n'est bouché, que les 8 secondes de
  la règle §4.2.2 sont plausibles à l'œil.
- `cargo test --lib` intégralement vert, y compris le verrou décor de la
  Phase B.
- Captures d'écran avant/après conservées dans le PR ou le rapport de fin de
  sprint, pour que la prochaine session comprenne l'intention sans avoir à
  rejouer toute la carte pour deviner.

---

## 8. Pièges déjà rencontrés à ne pas répéter (mémoire de session)

- **Piège deux serveurs MCP Blender déclarés (casse différente)** : ce dépôt a
  vu coexister `mcp__Blender__*` (majuscule) et `mcp__blender__*` (minuscule).
  Le premier a timeout systématiquement (>30 s sur des appels triviaux comme
  `get_objects_summary` ou `execute_blender_code` avec juste
  `result = {"ok": True}`), le second (`mcp__blender__get_scene_info`,
  `mcp__blender__execute_blender_code`, etc.) répond normalement. **Toujours
  vérifier avec un appel trivial (`get_scene_info`) lequel des deux répond
  avant de commencer une session de composition**, plutôt que de déboguer à
  l'aveugle un serveur qui ne sera jamais utilisé.
- **Piège connexions concurrentes sur le port 9876** : chaque session Claude
  Code ouverte en parallèle lance son propre bridge `blender-mcp` (process
  `uvx blender-mcp` ou équivalent) qui se connecte au socket TCP de l'add-on
  Blender. L'add-on ne gère généralement qu'**une seule connexion utile à la
  fois** — avec plusieurs bridges connectés simultanément (plusieurs fenêtres/
  onglets Claude Code + l'extension Claude Desktop), les réponses partent sur
  le mauvais canal et *tous* les clients timeout, y compris le bon serveur
  (`mcp__blender__*`). Diagnostic : `lsof -nP -iTCP:9876` — si plusieurs
  process autres que Blender apparaissent en `ESTABLISHED`, c'est la cause.
  Correctif : fermer les sessions Claude Code superflues (ou tuer leurs
  process bridge orphelins) pour ne garder qu'une seule connexion active avant
  de relancer un appel MCP. Même famille de risque que « Sessions concurrentes
  sur ce dépôt » (mémoire de session), mais appliquée au canal Blender plutôt
  qu'aux fichiers du dépôt.
- **Piège rotation/scale des cônes Blender** : un twist de π/4 fait basculer
  la longueur voulue vers l'axe vertical — vérifier tout objet conique/effilé
  (arbres, stalactites, piquets) après rotation, pas seulement avant.
- **Piège scale ×5,12 de `camera_to_view_selected`** : ne jamais l'appeler
  avec toute la scène sélectionnée en même temps — ça peut re-scaler toute la
  scène par erreur au lieu de juste cadrer la caméra. Cadrer objet par objet
  ou par petit groupe.
- **Bind-pose des assets skinnés** lors d'une conversion/réplique de scène —
  concerne les créatures/héroïne si elles sont repositionnées, pas les décors
  statiques de ce sprint, mais à garder en tête si un asset animé (chariot
  d'Escorte, herse) est repositionné.
- **Écrasement de la scène embarquée à l'export** — déjà signalé au §4.3,
  répété ici car c'est le piège le plus coûteux si oublié (perte silencieuse
  du travail de composition).
- **Sessions concurrentes sur ce dépôt** : avant d'écrire dans
  `src/scene/demos.rs`/`assets/player_scene.json`, vérifier qu'aucune autre
  session ne les modifie déjà (mtimes, `git status`) — ces deux fichiers sont
  des points de friction connus (cf. `sprint2audijeu0718.md`, frictions de
  fichiers).

---

## 9. Une minimap réelle — outil de vérification, pas une fonctionnalité joueur

### 9.1 Ce qui existe déjà (dev-only)

Il existe **déjà** une vraie minimap dans le code, mais elle n'est visible que
dans l'éditeur, jamais côté joueur : `AppState::minimap_data()`
(`src/app/mod.rs:1314`) calcule les positions (x, z) du joueur, des alliés
réseau et des créatures, plus les bornes du monde (déduites de l'objet `Sol`) ;
`minimap_window()` (`src/editor/windows.rs:289`) les affiche dans un panneau
egui zoomable/déplaçable (`panels.minimap`, ouvert depuis le menu éditeur,
`src/editor/menus.rs:580`). C'est un vrai outil fonctionnel, pas une ébauche —
mais il n'affiche que des **points dynamiques** (entités vivantes), jamais le
décor statique (remparts, eau, forêt, chemins).

### 9.2 Pourquoi il ne faut pas en faire une fonctionnalité joueur permanente

Le GDD **refuse explicitement** une minimap permanente dans le HUD du joueur
(`GDD_MMORPG.md:1223`, tableau §17.5 « Les surfaces qu'on refuse ») :

> « Minimap / carte d'écran » — *pourquoi refusée* : « la carte servie est
> compacte et le danger est toujours *proche* (éveil 9 m) ; une minimap vole
> l'anneau corps en permanence pour une info d'anneau méta » — *remplacée
> par* : « le feu communal visible de partout (orientation), les **portes qui
> s'embrasent** à l'arrivée d'une vague (provenance), les marqueurs
> hors-écran ponctuels ».

Ce refus est une décision de design assumée, pas un oubli — ce sprint ne doit
**pas** la contourner en ajoutant discrètement une minimap au HUD sous couvert
d'outil de composition. Le panneau `minimap_window` existant reste un outil
d'éditeur/débogage, jamais exposé en `run_player_overlay` (mode `--player`) —
cf. la même distinction déjà documentée dans
`docs/AUDIT_JEU_2026-07-18.md` §12 pour la fenêtre Paramètres.

### 9.3 Ce que ce sprint doit vraiment produire : une carte de composition

Deux artefacts distincts, tous deux légitimes et utiles, ni l'un ni l'autre
n'étant une fonctionnalité joueur :

1. **Un rendu top-down de référence, produit par le MCP Blender**
   (`render_viewport_to_path`/`render_thumbnail_to_path` depuis une caméra
   orthographique vue de dessus) — l'équivalent d'une carte d'état-major :
   zones (hameau, eau, forêt, rizières, promontoire, grottes), lisières de
   spawn, ponts, chemins. Ce rendu est un **livrable de documentation** (à
   inclure dans le rapport de fin de sprint et/ou ce document), pas un asset
   du jeu — il sert à toute personne qui reprend le projet à comprendre la
   géographie de la carte sans relancer Blender.
2. **Une extension optionnelle du panneau `minimap_window` existant**, pour
   la phase de vérification pendant la composition (pas pour le joueur final) :
   superposer aux points dynamiques déjà affichés les **bornes statiques** des
   zones en cours de composition — rects d'eau de `hameau_gdd_demo()` (§2 bis.2
   point 3), zones d'exclusion
   du scatter procédural (`EXCL_EAU_ROUTES` etc.), footprint du fort — pour
   repérer un chevauchement ou un chemin bouché sans repasser par Blender à
   chaque itération. Reste dans l'éditeur, gardé par le même `Panels::minimap`
   qu'aujourd'hui — aucun changement de statut vis-à-vis du GDD.

### 9.4 Definition of done pour ce point précis
- Un rendu top-down existe et est versionné (image, pas seulement un fichier
  `.blend` de travail).
- Si l'extension du panneau debug est faite : elle reste strictement
  éditeur-only, vérifiable en confirmant qu'aucun appel n'existe dans
  `run_player_overlay()` (`src/editor/mod.rs`) pour cette nouvelle
  superposition.
- Le tableau §17.5 du GDD n'est pas contredit par ce sprint.

---

## 10. Ce que ce sprint n'est pas

- **Pas un sprint de génération d'assets** — le catalogue existant (§2) est
  suffisant pour composer une première carte complète et crédible. Un besoin
  d'asset supplémentaire découvert *pendant* la composition (ex. un type de
  chemin/pavage manquant) doit être noté comme ticket séparé, pas traité en
  urgence dans ce sprint-ci.
- **Pas un sprint de terrain à heightmap** — le relief réel reste un chantier
  séparé (Phase K de `sprintreflecion.md`, déjà en cours, portée volontairement
  restreinte). Ce sprint compose du décor sur le terrain existant (plat ou
  partiellement en relief), il ne sculpte pas de nouveau relief.
- **Pas un sprint de gameplay** — le déclenchement d'animations (portes qui
  s'embrasent, chariot qui avance en Escorte) reste hors scope, comme déjà
  posé dans `integration_siege_scene.md`. Ce sprint pose les assets au repos,
  bien composés visuellement ; le câblage gameplay est un sprint séparé.
