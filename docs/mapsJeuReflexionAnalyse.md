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
| `assets/player_scene.json` (la **scène réellement servie**) | — | `Sol.transform.scale = (90, 1, 90)` (vérifié en lisant le JSON directement) | Un **mélange partiel** des deux précédentes : décor `Rempart`/`Tour` (44/3 occurrences, vient de `hameau_gdd_demo()`), `Faune` (119, vient de `mmorpg_demo()`), `Créature` (26, vient de `mmorpg_demo()`), objets `Lac`/`Rivière` (18/3 — le système d'eau de `mmorpg_demo()` **est bien présent**). **Mais 0 occurrence de `hamlet` (village), 0 `Promontoire`, seulement 1 `Rizière`** (probablement juste un panneau, pas les 5 parcelles réelles) — le village, le promontoire et les rizières décrits dans les commentaires de `mmorpg_demo()` **n'ont jamais été synchronisés vers la scène servie**. Ce que les joueurs voient réellement aujourd'hui est plus pauvre que ce que le code de la démo décrit. |

### 2 bis.2 — Deux écarts concrets à réconcilier **avant** de composer quoi que ce soit

1. **L'échelle du sol ne correspond à aucune des deux fonctions** :
   `Sol.scale = 90` dans le JSON servi, alors que `mmorpg_demo()` construit un
   sol à `2 × MMORPG_HALF = 72` (le calcul exact, vérifié dans le code :
   `sol.transform.with_scale(Vec3::new(2.0 * half, 1.0, 2.0 * half))`, base
   `MeshKind::Terrain` de demi-étendue locale 0,5, donc scale = taille pleine).
   Il y a donc un agrandissement de 72 → 90 quelque part dans la chaîne de
   synchronisation, non documenté à ce jour. **À élucider en premier** (relire
   `sync_embedded_scene_hameau_from_the_demo` et les tests de synchro pour
   trouver où ce redimensionnement a lieu), sinon toute composition sera faite
   sur de mauvaises proportions.
2. **L'alignement spatial hameau fortifié / village n'est pas prouvé.**
   `hameau_gdd_demo()` (fort `siege_*`, `HALF=24`, vraisemblablement centré à
   l'origine) et `mmorpg_demo()` (village `hamlet_*`, ex. `"Place du hameau"`
   posée à `(10.0, 0.031, 7.0)`, pas à l'origine) sont deux fonctions écrites
   séparément, à des moments différents, **sans jamais avoir été composées
   ensemble ni visualisées côte à côte**. Que le village tombe par coïncidence
   à l'intérieur du footprint du fort (`(10,7)` est bien dans `±24`) ne prouve
   pas que les rues/bâtiments s'alignent avec les remparts/portes réels. Comme
   le village n'est de toute façon pas synchronisé vers la scène servie (point
   2 bis.1), la question ne casse rien *aujourd'hui* — mais elle redevient
   critique dès qu'on veut réintroduire le village dans la carte réelle.

### 2 bis.3 — Coordonnées réelles du système d'eau (à réutiliser, pas à réinventer)

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
  posés nulle part** — ni dans la scène servie, ni même dans `mmorpg_demo()`.
  Le système d'eau et la marge de relief ouest qui les attendent naturellement
  existent déjà (§2 bis.3) mais sont habillés en primitives génériques.
- Le village (`hamlet_*`), le promontoire et les rizières existent dans
  `mmorpg_demo()` mais **ne sont jamais arrivés dans la scène servie** — un
  écart plus large que ce qu'un premier passage de cet audit avait supposé
  (« ménagerie de patrouille générique » ne concernait que le casting
  d'archétype des créatures, pas l'absence de biomes entiers).
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

### Étape 0 (préalable, avant toute composition visuelle) — réconciliation
1. Élucider l'écart d'échelle du sol (90 servi vs 72 attendu, §2 bis.2 point 1)
   — sans ça, toute coordonnée choisie pendant la composition risque d'être
   fausse une fois régénérée.
2. Décider explicitement du sort du village (`hamlet_*`), du promontoire et
   des rizières : **les réintroduire dans la scène servie**, ou **acter
   consciemment leur absence** (auquel cas retirer/clarifier les commentaires
   de `mmorpg_demo()` qui les décrivent comme si elles y étaient). Ne pas
   laisser la question ouverte par défaut — c'est exactement le genre d'écart
   documentation/code que les audits précédents ont dû corriger a posteriori.
3. Si le village est réintroduit : vérifier **visuellement**, dans Blender,
   que son emprise ne chevauche pas les remparts/portes du fort — la
   coïncidence de coordonnées (§2 bis.2 point 2) n'est pas une preuve.

### Étape 1 — habiller le système d'eau existant avec `shore_*`
Les 4 rects d'eau et leurs coordonnées exactes sont donnés au §2 bis.3 — ne
pas les redessiner, les **habiller** : berges organiques, rochers lissés,
brume basse, faune échouée neutre autour du lac (zone calme, cohérente avec
§7.3 du GDD), sans toucher aux murs invisibles ni aux ouvertures de pont déjà
calibrées à ~3 m (fragiles, cf. le commentaire de `demos.rs` sur `GRID=1.0`).

### Étape 2 — poser `grotto_*` sur la marge de relief ouest existante
Le relief réel (Phase K, `sprintreflecion.md`) a déjà commencé à creuser un
« petit bassin intégré à un contrefort + tunnel/arceau statique » sur cette
même bande ouest — c'est l'emplacement naturel pour les 20 assets `grotto_*`
(entrée de grotte, stalactites/stalagmites, champignons lumineux…), pas une
nouvelle zone à inventer ailleurs sur la carte.

### Étape 3 — vérifier la cohérence du reste (forêt NE, rizières SO, promontoire est)
Ces trois biomes sont déjà cartographiés avec un système de scatter
procédural mature (graine + zones d'exclusion `EXCL_EAU_ROUTES`/
`EXCL_ZONES_AMENAGEES`/`EXCL_CLAIRIERES`) — ne pas les refaire. Le sprint se
contente de vérifier, après les étapes 1-2, qu'aucun nouvel élément ne casse
leurs zones d'exclusion existantes (piège solid_spots, §4.3).

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
| Étape 0 — réconciliation (écart d'échelle 90/72, sort du village) | **Rust/lecture de code seule**, pas Blender | C'est un problème de calcul et de décision produit, pas de composition visuelle — relire `sync_embedded_scene_hameau_from_the_demo` et les tests de synchro suffit (§5 Étape 0). |
| Habillage du système d'eau (`shore_*` sur les 4 rects, §2 bis.3) | **Blender d'abord**, coordonnées déjà connues à ajuster visuellement | Les rects/ouvertures sont fixes et documentés, mais la densité et le placement des berges/rochers (éviter le "collier de perles", §7.5) sont un jugement d'œil — exactement le cas d'usage visé par ce sprint (§6). |
| `grotto_*` sur la marge de relief ouest (§5 Étape 2) | **Blender d'abord** | Emplacement déjà identifié, mais l'intégration avec le relief à heightmap existant (bassin/contrefort de la Phase K) demande de voir le terrain réel en 3D pour ne pas faire flotter/enfoncer les assets. |
| Vérification forêt NE / rizières SO / promontoire est (§5 Étape 3) | **Rust/lecture de code seule** | Scatter procédural déjà mature (graine + zones d'exclusion) — le sprint ne fait que vérifier qu'aucun nouvel élément ne casse `EXCL_EAU_ROUTES` etc., pas re-composer ; une relecture de `demos.rs` suffit, pas besoin du viewport. |
| Réintroduction du village (`hamlet_*`) dans la scène servie, si retenue (§5 Étape 0 point 2) | **Blender d'abord, obligatoire** | C'est justement le cas où la coïncidence de coordonnées (§2 bis.2 point 2) n'a jamais été vérifiée visuellement — composer ça directement en Rust reproduirait l'erreur méthodologique que ce sprint cherche à corriger. |
| Ajustements mineurs sur du décor déjà posé et testé (hameau fortifié `siege_*`) | **Ni l'un ni l'autre par défaut** | Hors scope (§10) — ne pas retoucher sans raison de gameplay. |

### Ce que "Blender d'abord" veut dire concrètement
Composer/ajuster dans Blender ne dispense jamais de l'étape manuelle de
traduction en Rust (§6, §7 points 7-10) : Blender produit un plan de
coordonnées et des captures d'écran de référence, **jamais** un export
automatique vers `demos.rs` ou vers le JSON. Si le MCP Blender est
indisponible au moment de composer un élément listé "Blender d'abord",
l'alternative de repli est d'écrire directement en Rust avec les coordonnées
connues du §2 bis.3 puis de corriger visuellement plus tard en jeu (`cargo run`
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
   zones en cours de composition — rects d'eau (§2 bis.3), zones d'exclusion
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
