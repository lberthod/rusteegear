# Sprint 3D — Audit de la scène face au GDD

> Audit mené le 18 juillet 2026 sur l'état **réellement servi** du jeu
> (`assets/player_scene.json`, chargé par l'éditeur et par le client — ce
> sont les deux captures d'écran qui ont déclenché cet audit), comparé à
> [GDD_MMORPG.md](../GDD_MMORPG.md) et à
> [ANALYSE_DESIGN_VISUEL.md](ANALYSE_DESIGN_VISUEL.md). Sources inspectées :
> `assets/player_scene.json` (790 objets, 241 meshes importés, lu et
> compté programmatiquement — pas d'estimation), `src/scene/demos.rs`
> (`Scene::mmorpg_demo()` et `Scene::hameau_gdd_demo()`), les tests
> garde-fous existants (`mod tests` de `demos.rs`).
>
> **Mise à jour du 18 juillet 2026 (suite de session).** Le point 1 du
> backlog (§7) a été tranché et exécuté : `Scene::hameau_gdd_demo()` est
> désormais la source de vérité de l'environnement servi,
> `assets/player_scene.json` a été régénéré par les outils de
> synchronisation déjà présents dans le dépôt (`src/scene/mod.rs`, `mod
> tests`) plutôt que par une édition manuelle. Le détail exact des
> commandes exécutées et de ce qui a changé est en §8 (nouveau). Le corps
> du document ci-dessous décrit l'état **avant** cette resynchronisation
> et reste correct comme diagnostic — chaque ligne close par la
> resynchronisation est marquée `[RÉSOLU §8]`.

---

## 0. Le constat qui chapeaute tout le reste : trois cartes, pas une

C'est la tension n°7 du GDD (« la scène servie et la scène vitrine
divergent ») rendue concrète par les chiffres. Le dépôt contient **trois
définitions de carte indépendantes**, et elles ont déjà divergé entre
elles :

| Carte | Où | Rôle | Preuve de contenu |
|---|---|---|---|
| `Scene::mmorpg_demo()` | `src/scene/demos.rs:1985-6179` | générateur de code, la « carte source » historique (72×72 m, prairie/rivières/forêt, hameau en angle) | génère `MMORPG_ITEMS` (potions/baies/clé, `ItemPickup`), `MMORPG_CREATURES` (26 créatures, vagues 1-4), la faune paisible complète, « Feu du hameau » + « Feu de camp » |
| `Scene::hameau_gdd_demo()` | `src/scene/demos.rs:6180-7972` | prototype **conçu explicitement pour le GDD** (commentaire de tête cite §7/§7.3/§5.4/§10) : fort carré 48×48, remparts à 4 portes + 2 brèches, anneau de **16** spawns joueur, 4 îlots bâtis, « Feu communal », lanternes/bannières | reprend les créatures de `mmorpg_demo` à l'identique (même noms/vagues), mobile/HUD/pickups/lumières **non renseignés** (`Scene::default()` implicite) |
| `assets/player_scene.json` | racine `assets/` | **la carte réellement chargée** (éditeur des captures, client, serveur) | 790 objets, 241 imports, **3** `point_lights` seulement, **0** objet émissif, **0** `weapon_pickup`, **0** `item_pickup`, contient à la fois des motifs de nommage de `mmorpg_demo` (« Mobilier du hameau », « Décor du hameau ») et un objet « Feu communal » que ni `mmorpg_demo` ni son historique récent ne produisent sous ce nom (`mmorpg_demo` a aujourd'hui un « Feu du hameau ») |

**Lecture.** La carte servie est un instantané ancien, probablement
resynchronisé une fois puis retouché à la main dans l'éditeur depuis (elle
contient des objets qu'aucune des deux fonctions Rust actuelles ne
génère à l'identique). Chaque sprint « décor » qui enrichit
`mmorpg_demo()` ou `hameau_gdd_demo()` sans re-synchroniser
`player_scene.json` **n'atteint jamais le joueur** — exactement le risque
que le GDD nomme à sa tension n°7 et que la règle d'authoring de la
tension n°6 demande de fermer.

**Conséquence méthodologique pour tout le reste de cet audit.** Les
constats ci-dessous portent sur `player_scene.json` (ce que le joueur
voit réellement), avec, quand c'est pertinent, ce que le code source
*a déjà* et qui n'a pas encore traversé jusqu'à la carte servie — ces
cas-là ne sont pas « à créer » mais « à resynchroniser », une catégorie de
travail moins chère et à traiter en premier.

**Ce que ça implique pour le travail déjà fait cette session.** Le
correctif apporté juste avant cet audit (glow + lumière sur « Feu du
hameau »/« Feu de camp », voir diff en cours sur `demos.rs`) vit dans
`Scene::mmorpg_demo()`. Au vu du tableau ci-dessus, **rien ne garantit que
cette fonction soit celle qui alimente `player_scene.json`** : le nom de
l'objet ciblé (« Feu du hameau ») n'existe pas dans la carte servie
(qui a « Feu communal »). Ce correctif est donc probablement **invisible
tant qu'une resynchronisation n'a pas lieu** — premier exemple concret de
la tension n°7, produit par cette session elle-même.

---

## 1. Fiction et identité visuelle (GDD §2, §10.1-10.2)

| # | Constat (carte servie) | Cible GDD | Écart / à créer |
|---|---|---|---|
| 1.1 | 1 objet nommé « Feu communal » existe (position non ré-auditée), **0 objet émissif dans toute la carte** (`emissive` non renseigné/à 0 sur les 790 objets) | §2.1 : « [le feu communal] c'est lui qui attire les hordes » ; §10.1 : « au centre, les braises » ; §10.2 : orange = système feu/joueur, budget émissif réservé aux enjeux | Donner au feu communal un matériau/point light chauds (le pipeline `emissive` + `PointLight` existe déjà et sert ailleurs dans le moteur — aucun nouveau système à écrire, juste des valeurs de scène) |
| 1.2 | Avatar joueur = capsule orange sans variation par classe (aucune classe encore branchée serveur, cf. §14) | §2.2 : couleur de braise personnelle ; §10.3 : silhouette par classe (Flamme ×1, Éclaireur ×0,9 + traînée cyan, Foyer ×1,1 + sac dorsal) | Non actionnable côté *scène* seule — dépend du système de classes (§8.1, priorité 2 de la feuille de route). À noter comme dépendance, pas comme trou de carte. |
| 1.3 | Aucune créature de la carte servie ne porte de marque émissive de menace | §10.1 : « l'éveil d'une créature se voit dans sa couleur » ; §10.2 : magenta = menace éveillée, réservé | Nécessite un état runtime (endormie/éveillée), pas seulement de la donnée de scène statique — c'est un **système**, à sortir de ce document et à traiter comme un item de `SPRINT_MMORPG.md`, pas comme un ajustement de carte. |
| 1.4 | Faune paisible déjà bien présente et nommée (« Mouton Nord-Est », « Poule Sud-Est », « Chouette 1-3 », « Grenouille 1-4 », « Luciole de la place 1-6 », « Luciole du camp de chasseurs 1-3 », etc.) | §7.3 : moutons/poules en cour, chouette aux remparts, lucioles autour du feu — « l'atout dormant du bundle » | **Déjà fait et déjà servi.** Vérifier uniquement que ces objets restent `attackable: false` / hors `ai_chaser` (règle stricte §7.3) — contrôle rapide à ajouter en test garde-fou, pas un ajout de contenu. |

---

## 2. Casting des créatures — archétypes (GDD §5.4)

| Constat | Cible GDD | Écart / à créer |
|---|---|---|
| Les 26 créatures attaquables de la carte servie utilisent les fichiers génériques `creature.glb` … `creature26.glb` (silhouettes non thématisées). Le bestiaire nommé (`monster_alien.glb`, `monster_orc.glb`, `monster_yeti.glb`, variantes `_evolved`, ~45 modèles) est **uniquement décoratif** dans `mmorpg_demo()` (`MONSTER_DECOR`, non attaquable, posé en bande nord) et **absent** de la carte servie (0 objet « Monstre » trouvé dans `player_scene.json`). | §5.4 : grammaire d'archétypes Traqueuse/Meute/Colosse/Furtive castée sur le pack `monster_*`, chef = variante `_evolved` d'une espèce déjà présente dans la vague. | **Piège identifié, à ne pas retenter sans passer par l'asset pipeline.** Le pack `monster_*` a été exporté **sans squelette** (`scripts/blender/import_monster_pack.py`, commentaire en tête de `MONSTER_DECOR` : « armature/skin retirés à l'export ») précisément parce que `MAX_SKINNED_INSTANCES` (`src/gfx/renderer.rs`) est déjà à ~66/96 avec les créatures MMORPG + le décor nature animé. Remplacer directement les fichiers des 26 créatures de combat par des `monster_*.glb` casserait leur animation (pas d'`Idle` possible sans squelette) et/ou dépasserait le budget d'instances skinnées. **Ce chantier a un prérequis d'asset pipeline** (ré-exporter un sous-ensemble skinné du pack, ou libérer du budget ailleurs) avant d'être une simple édition de données de scène — à planifier comme sprint dédié, pas comme retouche de carte. |
| Le budget de PV par vague est déjà strictement croissant (1: 5 créatures/5 PV, 2: 6/8, 3: 7/11, 4: 8/16, vérifié dans la carte servie ET verrouillé par `mmorpg_demo_waves_follow_the_gdd_authoring_rules`) | §5.5 : dent de scie ascendante, chef à 3 PV dès la vague 2, dernière vague ≥ 4/3 de l'avant-dernière | **Déjà conforme**, y compris dans la carte servie (vérifié : budget 4 = 16, budget 3 = 11, 16×3=48 ≥ 11×4=44 ✅). Rien à créer ici — seul le *casting visuel* (archétypes) manque, cf. ligne au-dessus. |
| Règle de casting « au plus 2 archétypes par vague, sauf la dernière » | §5.4 | Non applicable tant que le casting archétype n'existe pas (item précédent) — dépendance directe. |

---

## 3. Feedback et parité mobile — ce qui est *donnée de scène* (GDD §6.1-6.3, §16.3-16.4, §17.1)

Le feedback de dégâts (vignette, recul, son) et le HUD proprement dit sont
du code (`src/app/*`), hors de ce document. Ce qui **vit dans la scène**
et conditionne leur affichage :

| Constat (carte servie) | Cible GDD | Écart / à créer |
|---|---|---|
| `mobile.buttons` = `["Saut", "Feu", "Arme", "Soin"]` | §6.3 : « toute action a une touche ET un bouton tactile, dès son sprint de livraison » | **Déjà réparé dans la carte servie** (contredit l'état "cassé" documenté au §14/§18.2 du GDD, qui date du 16-17 juillet — à corriger dans le GDD lui-même, cf. §6 de ce document). |
| `mobile.health_bar = false`, `mobile.safe_area = false` | §16.4 : barre de vie tactile à activer avec le HUD (« sur mobile la vignette compte double, l'écran est petit ») ; zone sûre pour ne pas passer sous les encoches | À activer dans la config de scène **en même temps que** le HUD roster/vie sera branché côté code — sinon activer le flag seul n'a aucun effet visible et ne teste rien. Dépendance croisée à noter dans `SPRINT_MMORPG.md`. |
| `hud_layout` : les 6 emplacements (`crosshair`, `weapon_hud`, `kills`, `weapon_inventory`, `item_inventory`, `roster`) sont tous à `(0.0, 0.0)` | §17.1 : chaque surface a un emplacement réel, anneau « Corps »/« Groupe » du §16.3 | **Non fait.** C'est un placement de coordonnées de scène (pas un nouveau système) mais il doit être co-conçu avec les vrais widgets HUD (priorité 1 de la feuille de route, §14) — à traiter dans le même sprint que le roster, pas isolément (des coordonnées sans widget derrière ne prouvent rien). |
| `hud_widgets = []` (liste vide) | §17.2 : bannières de vague, allié à terre + marqueur hors-écran, palier, contrat, spectateur, reconnexion, soin actif | Rien n'est posé. Comme au-dessus : dépend des systèmes correspondants (roster, réanimation, contrat) plus que de la scène elle-même — la scène doit *recevoir* ces widgets à mesure que chaque système livre, pas les anticiper à vide. |

---

## 4. Économie d'objets — pickups (GDD §5.1, §15.4, §17.1)

| Constat (carte servie) | Cible GDD | Écart / à créer |
|---|---|---|
| **0 `weapon_pickup`, 0 `item_pickup`** dans `player_scene.json` alors que `Scene::mmorpg_demo()` définit `MMORPG_ITEMS` (potion de soin ×3, buisson à baies, clé du village…) avec `ItemPickup` peuplé | §5.1 : « le *drop* est le piment de la nuit » ; §15.4 : trois états visuels obligatoires (repos/portée/saisi) pour tout objet interactif ; §17.1 : la sacoche est l'inventaire du jeu, elle doit avoir du contenu à afficher | **Régression de resynchronisation, priorité haute.** Ce n'est pas un manque de contenu à concevoir — le contenu existe déjà en Rust et n'a simplement jamais atteint la carte servie (même diagnostic que le §0). Une nuit jouée sur la carte actuelle n'a **aucun** objet à ramasser : aucune arme au sol, donc le triangle d'armes (§5.1) ne se déclenche jamais par le *drop*, seulement par le palier de compte. |
| Idem pour le loot garanti du chef (§15.4 : « un chef `_evolved` abattu laisse une arme au sol garanti ») | §15.4 | Dépend du casting archétype (§2 ci-dessus, chefs `_evolved`) **et** d'un système de drop à la mort (non trouvé dans `demos.rs` — à vérifier côté `src/app/*`/combat avant de le considérer comme un trou de carte). |

---

## 5. Level design (GDD §7)

| Constat | Cible GDD | Écart / à créer |
|---|---|---|
| La carte servie contient des noms cohérents avec un plan de type hameau/fort (« Mobilier du hameau », « Décor du hameau », « Feu communal », lucioles « de la place » et « du camp de chasseurs ») mais je n'ai **pas** revérifié géométriquement dans ce passage si elle respecte le vocabulaire spatial complet (place / ruelles / cours / remparts) du §7.1, ni le budget « aucun point à > 8 s de course d'un espace ouvert » (§7.2 règle 2) | §7.1-7.2 : place ouverte, ruelles-goulets, cours-refuges, remparts-verticalité ; obstacles détectables par les sondes IA (raycast 0,6 m) ; anneau de spawn à 16 positions distinctes | **À auditer en jeu, pas sur plan** — le GDD lui-même le dit (§7.2 règle 1 : « le level design se valide en jeu, pas à l'œil »). Recommandation : un test garde-fou géométrique (16 positions de spawn distinctes, distance minimale entre elles) est peu coûteux à écrire et manque aujourd'hui dans `mod tests` de `demos.rs` — aucun test actuel ne porte sur le vocabulaire spatial du §7, seulement sur le décor traversable/solide. |
| `hameau_gdd_demo()` prétend déjà à un anneau de **16** spawns joueur (commentaire de tête) — mais cette fonction n'alimente probablement pas la carte servie (§0) | §7.2 règle 4 : « l'anneau de spawn joueurs doit tenir 16 positions distinctes » | Si `hameau_gdd_demo()` est le plan retenu à terme, c'est *lui* qu'il faut resynchroniser vers `player_scene.json`, pas continuer à enrichir `mmorpg_demo()` en parallèle — sinon le travail sur les 16 spawns ne sert jamais au jeu réel. **Décision à trancher avant tout autre travail de carte** : quelle fonction est la source de vérité ? (cf. §6 recommandations). |

---

## 6. Ce que le GDD lui-même doit corriger (trouvé en creusant, hors scène)

Pas un manque de carte, mais un écart de documentation détecté pendant
cet audit, à signaler pour ne pas laisser une contradiction vivre sans
arbitrage (règle §18.7 du GDD) :

- **GDD_MMORPG.md §14/§18.2** affirme encore que la parité mobile est
  cassée (« seul “Saut” subsiste dans la scène exportée, régression
  bloquante »). La carte servie a aujourd'hui les 4 boutons
  (`Saut/Feu/Arme/Soin`). Soit la régression a été corrigée sans mise à
  jour du GDD, soit `player_scene.json` a été retouché à la main sans
  passer par le pipeline normal (auquel cas la correction n'est pas
  reproductible si la carte est resynchronisée depuis `mmorpg_demo()`,
  dont le `mobile.buttons` **source** ne contient toujours que `["Saut"]`
  à la ligne `demos.rs:6118`). Ce deuxième cas serait une régression
  **en sommeil** : la prochaine resynchronisation depuis le code
  effacerait silencieusement les 3 boutons manquants. À vérifier en
  premier, avant tout autre travail — un test garde-fou du type
  `mmorpg_demo_mobile_buttons_cover_every_action` sur `Scene::mmorpg_demo()`
  fermerait ce risque pour de bon.

---

## 7. Backlog priorisé (par levier, pas par ordre alphabétique)

1. **`[RÉSOLU §8]` Trancher la source de vérité de la carte** (§0, §5) :
   `mmorpg_demo()` ou `hameau_gdd_demo()` ? Tant que cette décision n'est
   pas prise, tout travail de contenu risque d'être posé dans la mauvaise
   fonction (déjà arrivé à l'instant avec le correctif « Feu du
   hameau »/« Feu de camp » de cette session).
2. **`[RÉSOLU §8]` Fermer le trou des pickups** (§4) :
   `weapon_pickup`/`item_pickup` à 0 dans la carte servie alors qu'ils
   existent en source — la resynchronisation seule répare probablement ce
   point, sans écrire une ligne de contenu nouvelle.
3. **`[OBSOLÈTE, voir §8]` Verrouiller `mobile.buttons` dans la fonction
   source** (§6) avant qu'une resynchronisation n'efface la correction
   déjà visible en jeu. Diagnostic corrigé au §8 : `mobile` n'est jamais
   écrasé par les outils de synchro (préservé tel quel depuis la carte
   servie existante), ce risque n'existait donc pas — seule la ligne
   `GDD_MMORPG.md §14/§18.2` reste à corriger.
4. **`[RÉSOLU §8]` Charte DA minimale sur la carte servie** (§1.1) :
   émissif + lumière sur le feu communal.
5. **Test garde-fou du vocabulaire spatial §7** (16 spawns distincts,
   distance mini) — aujourd'hui non couvert par `mod tests`. Toujours
   ouvert.
6. **Casting archétypes §5.4** : chantier d'asset pipeline (ré-export
   skinné partiel du pack `monster_*` ou arbitrage de budget
   `MAX_SKINNED_INSTANCES`) avant toute édition de données de scène —
   le plus gros chantier de cette liste, à sprinter séparément. Toujours
   ouvert, inchangé.
7. **Marques émissives de menace (créature éveillée)** et **HUD
   `hud_layout`/`hud_widgets`** : dépendent de systèmes runtime (état
   éveillé, roster, réanimation) non encore livrés — à ne pas anticiper
   côté carte avant que ces systèmes existent. Toujours ouvert, inchangé.
8. **Nouveau, trouvé en clôturant le point 2** : le pack `monster_*`
   (voir point 6) n'a toujours aucun `weapon_pickup` en source, dans
   aucune des deux fonctions — contrairement aux `item_pickup`, ce n'est
   pas une régression de resynchronisation mais un vrai trou de contenu à
   auteurer (§5.1 « ramassage d'armes »), indépendant du casting
   d'archétypes.

---

## 8. Ce qui a été exécuté après l'audit (même session, sur consigne explicite)

Contrairement au reste de ce document (analyse seule), cette section
documente des **actions réellement effectuées** sur le dépôt, à la
demande explicite de trancher §7 point 1 et de continuer le backlog.
Rejoue exactement ces commandes pour reproduire l'état actuel :

1. **Décision** : `Scene::hameau_gdd_demo()` devient la source de vérité
   de l'environnement servi. Justification : c'est la seule des deux
   fonctions dont l'en-tête cite explicitement le GDD (§7/§7.3/§5.4/§10),
   la seule pensée pour l'anneau de 16 spawns (§7.2 règle 4), et le dépôt
   avait **déjà** l'outillage de synchronisation pour ce choix précis
   (`sync_embedded_scene_hameau_from_the_demo`, `src/scene/mod.rs`) —
   trancher dans l'autre sens aurait supposé d'écrire cet outil, pas
   seulement de l'invoquer. `Scene::mmorpg_demo()` **n'a pas été
   supprimée** : `hameau_gdd_demo()` l'utilise comme base pour ses
   créatures (`let base = Scene::mmorpg_demo();`), et ~25 sites de test
   (`src/app/simulation.rs`, `creature_attack.rs`, `network_client.rs`,
   `scene/mod.rs`) s'en servent comme fixture de physique/combat/réseau
   sans rapport avec le plan de niveau — la supprimer aurait cassé la
   compilation pour un gain nul (ces tests ne dépendent pas du décor).
2. **Régénération de `assets/player_scene.json`**, dans cet ordre exact
   (chaque étape a un garde-fou compagnon qui repasse au vert ensuite) :
   1. `cargo test sync_embedded_scene_hameau_from_the_demo -- --ignored --nocapture`
      — remplace tout l'environnement + créatures + joueur par
      `hameau_gdd_demo()`, en préservant `mobile`/`hud_layout`/
      `hud_widgets`/`point_lights`/`camera_follow`/`game_camera`/
      `version`/`groups` tels quels et en forçant un ciel nocturne câblé
      en dur dans l'outil (déjà conforme GDD §2.3/§10, aucune action
      nécessaire de mon côté) : 790 → 614 objets (perte attendue et
      documentée par l'outil lui-même : le décor ambiant n'est pas dans
      `hameau_gdd_demo()`, il vient de l'étape suivante).
   2. `cargo test sync_embedded_scene_ambient_decor_from_the_demo -- --ignored --nocapture`
      — réinjecte, de façon additive et idempotente, le décor ambiant de
      `mmorpg_demo()` (faune 27-61, flore, objets — liste de préfixes
      `AMBIENT_DECOR_PREFIXES`) : 614 → 790 objets, copie/compresse zstd
      les assets manquants dans `assets/bundle/` (14 nouveaux fichiers
      `mNN_creatureXX.glb`, faune/flore).
   3. **Nouvel outil créé pour combler un trou qu'aucun des trois
      outils existants ne couvrait** (§4) :
      `sync_embedded_scene_pickups_from_the_demo` + son garde-fou
      `the_embedded_scene_has_item_pickups_from_the_demo`
      (`src/scene/mod.rs`) — synchronise les `ItemPickup` de
      `MMORPG_ITEMS` (potions, baies, clé, gemme, meshes primitifs donc
      aucun bundle à gérer). Exécuté : 790 → 797 objets, 7 objets
      ramassables désormais présents (0 auparavant).
   4. **Fix ciblé (§1.1), sur la bonne fonction cette fois** : émissif
      1.2 sur l'objet « Feu communal » de `hameau_gdd_demo()` (le
      correctif précédent de cette session, sur « Feu du hameau » dans
      `mmorpg_demo()`, ne pouvait pas atteindre la carte servie — cf.
      §0). Après re-régénération des trois étapes ci-dessus (l'ordre
      complet doit être rejoué à chaque changement de `demos.rs`) : 8
      objets émissifs dans la carte servie (7 pickups + le feu communal),
      `Feu communal.emissive == 1.2`.
3. **Vérification** : `cargo test --lib` → 507 tests passés, 0 échec, 7
   ignorés (les 3 outils de synchro + 4 outils préexistants, marqués
   `#[ignore]` par construction) ; `cargo fmt --check` propre ;
   `cargo clippy --lib -- -D warnings` propre.
4. **Point de vigilance découvert en cours de route, sans rapport avec
   ce travail** : `git status` montre aussi `src/runtime/physics.rs`,
   `src/app/mod.rs` et `src/app/network_client.rs` modifiés, avec des
   horodatages de fichier concomitants à cette session — cohérent avec
   une **autre session concurrente sur ce même dépôt**, pas avec un effet
   de bord de ce travail (je n'ai touché aucun de ces trois fichiers).
   Je ne les ai ni relus ni intégrés à cette resynchronisation ; à
   vérifier avant tout commit groupé pour ne pas mélanger deux travaux
   indépendants dans le même commit.
5. **Ce qui reste ouvert** : voir §7 points 5, 6, 7, 8 — inchangés par
   ce travail, qui n'a fait que fermer les points 1, 2, 4 et clarifier
   le point 3.

---

## 9. Méthode de vérification proposée

Cohérent avec la méthode du projet (tests-preuves, cf. GDD §11) : chaque
ligne du backlog ci-dessus devrait, avant d'être marquée faite, avoir un
test qui la verrouille sur `Scene::mmorpg_demo()` (ou la fonction
tranchée au point 1) **et** une vérification que `assets/player_scene.json`
a bien été resynchronisé après (un test qui charge le JSON servi et
compare son nombre d'objets `weapon_pickup`/`item_pickup`/émissifs à un
minimum attendu fermerait la classe entière de régressions silencieuses
identifiée au §0).

---

## 10. État au moment du commit

Points 1, 2 et 4 du backlog (§7) et leur exécution (§8) sont commités
dans ce même commit : `src/scene/demos.rs` (émissif « Feu communal »,
et par cohérence « Feu du hameau »/« Feu de camp » dans `mmorpg_demo()`),
`src/scene/mod.rs` (nouvel outil `sync_embedded_scene_pickups_from_the_demo`
+ son garde-fou), `assets/player_scene.json` régénéré (797 objets, 7
pickups, 8 objets émissifs), et les nouveaux fichiers `assets/bundle/`
copiés par l'outil de synchro du décor ambiant. Suite de tests complète
(507 tests), `cargo fmt`/`clippy` vérifiés avant commit.

**Hors périmètre de ce commit, volontairement** : les modifications
concurrentes détectées sur `src/runtime/physics.rs`, `src/app/mod.rs` et
`src/app/network_client.rs` (§8 point 4) — non écrites par ce travail,
non relues, donc non committées ici pour ne pas mélanger deux travaux
indépendants. De même, les fichiers déjà modifiés/supprimés en tout
début de session (`ANALYSE_DESIGN_VISUEL.md`, `GDD_MMORPG.md`,
`README.md`, `ROADMAP_SPRINTS.md`, `SPRINT_MMORPG.md`, et les documents
d'audit supprimés) préexistaient à ce travail et restent hors de ce
commit.
