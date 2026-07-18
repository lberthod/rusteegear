# RusteeGear — Sprint composition de carte (« Le Hameau des Braises ») — plan à phases

> Traduit en plan de sprint exécutable la réflexion de
> [mapsJeuReflexionAnalyse.md](mapsJeuReflexionAnalyse.md) (§5, §5 bis, §7) — lire ce document
> d'abord, celui-ci n'en est que le découpage en tâches ordonnançables.
>
> Convention identique à [sprint2audijeu0718.md](sprint2audijeu0718.md) : **Objectif · Tâches ·
> Fichiers · Livrable vérifiable · Risques** par phase, plus un tableau de recoupement de fichiers
> qui détermine seul ce qui peut tourner en parallèle sur deux instances. Lettrage **local à ce
> document** (A→I), indépendant du lettrage de `AUDIT_JEU_2026-07-18.md`/`sprint2audijeu0718.md`
> pour éviter toute confusion entre les deux sprints.

---

## 0. Règle absolue avant de commencer : une seule instance sur Blender, jamais deux

Ce point n'est pas négociable et prime sur tout le reste du document — cf. le piège documenté dans
`mapsJeuReflexionAnalyse.md` §8 (« connexions concurrentes sur le port 9876 ») : chaque session
Claude Code ouverte lance son propre bridge `blender-mcp`, et l'add-on Blender ne traite
correctement qu'**une seule connexion à la fois**. Deux instances connectées en même temps ⇒ les
deux timeout, y compris celle qui a raison.

**Avant qu'une instance ouvre une session Blender (n'importe quelle phase ci-dessous marquée
« Blender »)** :
1. Vérifier qu'aucune autre instance ne l'utilise déjà : `lsof -nP -iTCP:9876 | cat` — si une
   entrée `ESTABLISHED` autre que la sienne apparaît, **attendre**, ne pas se connecter.
2. Utiliser le bon nom de serveur : `mcp__blender__*` (minuscule). `mcp__Blender__*` (majuscule)
   est le serveur qui timeout systématiquement dans ce dépôt — ne pas perdre de temps dessus.
3. Une fois la session Blender terminée (captures prises, plan noté), se déconnecter explicitement
   avant qu'une autre instance ne commence sa propre session — ne pas laisser une connexion ouverte
   « au cas où ».
4. Si les deux instances ont besoin de Blender au même moment, elles se **coordonnent en chat**
   (l'une annonce qu'elle prend la main, l'autre attend) — il n'existe pas de verrou automatique,
   c'est une discipline manuelle.

## 0 bis. Règle absolue sur les fichiers partagés : jamais deux écritures concurrentes

Cf. mémoire de session « Sessions concurrentes sur ce dépôt » — avant d'écrire dans
`src/scene/demos.rs`, `src/scene/mod.rs` ou `assets/player_scene.json`, toujours :
1. `git status` — s'assurer que le fichier n'a pas de modifications non commitées par une autre
   instance en cours.
2. Comparer le `mtime` du fichier avec l'heure du dernier commit connu — un écart suggère qu'une
   autre instance a une session ouverte dessus en ce moment.
3. Committer par petites étapes (fin de chaque phase, pas fin de sprint) pour que l'autre instance
   puisse voir l'état à jour avant de démarrer sa propre phase.

Le tableau de recoupement au §1 dit **quelles phases peuvent tourner en parallèle sans jamais se
marcher dessus** — il ne remplace pas cette vérification manuelle, il réduit juste le nombre de fois
où elle est nécessaire.

---

## 1. Vue d'ensemble — fichiers touchés par phase, et ce qui en découle

| Phase | Fichiers touchés | Blender nécessaire ? |
|---|---|---|
| **A** — Réconciliation (échelle sol, décision village) — **FAITE (2026-07-18)** | `mapsJeuReflexionAnalyse.md` (décision actée) — en pratique aucune écriture dans `demos.rs`, voir bilan §4 | Non |
| **B** — Composition Blender de l'habillage eau (`shore_*`) — **FAITE (2026-07-18)** | Aucun fichier source — fichiers nouveaux versionnés : `docs/blender/composition_eau.blend` + `docs/blender/renders/eau_{riviere_ouest,riviere_sud,lac}.png`, plus référence historique `*_historique_mmorpg_demo.*` | **Oui, exclusif** — canal libéré |
| **C** — Composition Blender de `grotto_*` sur la marge ouest — **FAITE (2026-07-18)**, cible dégradée (sol plat, pas de relief dans `hameau_gdd_demo()`, voir bilan §4) | Aucun fichier source — fichiers nouveaux versionnés : `docs/blender/composition_grotto.blend` + `docs/blender/renders/grotto_{ouest_top,ouest_entree}.png` | **Oui, exclusif** — canal libéré |
| **D** — Vérification de la forêt en anneau de `hameau_gdd_demo()` (ex-« forêt NE/rizières SO/promontoire est », cible corrigée par A) — **FAITE (2026-07-18)** | `src/scene/demos.rs` (correctif isolé appliqué : 1 rect ajouté à `excl_eau`) | Non |
| **E** — Traduction Rust de l'habillage eau (depuis B) — **FAITE (2026-07-18), extraction headless** | `src/scene/demos.rs` (écriture, table `SHORE` + boucle de pose) | Non — extraction des données via `blender -b --python` (pas le canal MCP interactif) |
| **F** — Traduction Rust de `grotto_*` (depuis C) — **FAITE (2026-07-18), extraction headless** | `src/scene/demos.rs` (écriture, table `GROTTO` + boucle de pose + 15ᵉ rect `excl_eau`) | Non — extraction via `blender -b ... --python` (pas le canal MCP interactif) |
| **G** — Réintroduction du village hors les murs/promontoire/rizières — **NON RETENUE (décidé en A), bloc sauté** | — (phase annulée, conservée ici pour trace) | — |
| **H** — Régénération `player_scene.json` + assets bundle — **FAITE (2026-07-18)** | `assets/player_scene.json` (régénéré, jamais édité à la main), `assets/bundle/*` (+213 fichiers), `src/assets.rs` (touché) | Non |
| **I** — Tests, playtest, documentation finale — **FAITE (2026-07-18)**, playtest manuel humain restant (voir bilan) | `docs/sprintjeurefelxion.md` (bilan) uniquement, pas de fichier source | Non |

**Verdict de recoupement (mis à jour après A, D, E)** : `src/scene/demos.rs` est en pratique touché par
**D, E, F** (A n'a rien écrit dans `demos.rs`, G est annulée) — trois phases, plus léger que prévu
initialement. D et E sont maintenant faites et committées (voir §4) ; seule **F** reste à écrire dans
`demos.rs`, donc le risque de collision d'écriture entre instances sur ce fichier est retombé à zéro
pour l'instant (personne d'autre n'a de raison d'y écrire tant que C n'a pas produit ses captures).
`B` et `C` ne touchent **aucun** fichier source (uniquement des fichiers Blender/captures nouveaux)
— elles sont donc compatibles avec absolument tout, y compris entre elles sur le plan des fichiers.
**Mais B et C sont mutuellement exclusives sur le canal Blender** (§0) : jamais les deux en même
temps, même si leurs fichiers ne se recoupent pas.

---

## 2. Les blocs — dans cet ordre, chacun attend que le précédent soit fini et mergé

| Bloc | Phases dedans | Compatible en parallèle ? | Doit attendre la fin de |
|---|---|---|---|
| **1** | **A** ✅ + **B** ✅ | Oui — A ne touche pas Blender ni `demos.rs`, B ne touche aucun fichier source | — (démarre tout de suite) |
| **2** | **C** ✅ + **D** ✅ | Oui — C est Blender-exclusif mais sans fichier source ; D est lecture seule sur `demos.rs` (version déjà stabilisée par A) | **Bloc 1** (C reprend le canal Blender juste libéré par B ; D lit la cible corrigée par A — forêt en anneau de `hameau_gdd_demo()`) |
| **3** | **E** ✅ | Non — écrit `demos.rs` | **Bloc 1** (dépend des captures de B) |
| **4** | **F** ✅ | Non — écrit `demos.rs`, repart de la version mergée par E (fait) | **Bloc 2** (dépend des captures de C, faites) **et Bloc 3** (repart de `demos.rs` après E, fait) |
| **5** | **G** — **sautée** (non retenue en A) | — | — |
| **6** | **H** ✅ | Non — seule autorisée à régénérer `player_scene.json`, jamais en parallèle d'une écriture `demos.rs` en cours | **Bloc 4** (G étant sautée) |
| **7** | **I** ✅ (playtest humain restant) | — | **Bloc 6** |

**Seuls les Blocs 1 et 2 permettent un vrai travail à deux instances simultané.** À partir du Bloc 3,
c'est strictement séquentiel — une seule instance à la fois, l'autre peut préparer la phase suivante
en lecture seule (relire le code, préparer son propre brouillon de coordonnées) mais ne doit **rien
écrire** avant que la phase en cours soit committée.

### Pourquoi cet ordre précis
- **A et B en premier** : A élucide l'écart d'échelle et tranche le sort du village — sans ça,
  toute coordonnée choisie ensuite dans Blender (B, C) serait posée sur un référentiel faux. Résultat
  de A (voir §4) : 90 est la taille correcte et volontaire de `hameau_gdd_demo()` (aucune écriture
  dans `demos.rs`), et le village hors les murs/promontoire/rizières de `mmorpg_demo()` ne sont pas
  réintroduits — Bloc 5 (G) est donc annulé. B peut démarrer immédiatement en parallèle, mais sa
  cible a changé : ce sont les **3 rects d'eau de `hameau_gdd_demo()`**
  (`mapsJeuReflexionAnalyse.md` §2 bis.2 point 3), pas les 4 rects de `mmorpg_demo()` (§2 bis.3,
  devenu une référence historique).
- **C et D après** : C a besoin que B ait rendu la main sur Blender (règle d'exclusivité, pas de
  dépendance de contenu). D a besoin que A ait confirmé la cible : ce n'est plus la forêt NE / les
  rizières SO / le promontoire est de `mmorpg_demo()`, mais **la forêt en anneau de
  `hameau_gdd_demo()`** (`demos.rs:8447`, scatter via `foret_scatter`/`faune_scatter`) — la
  description de D ci-dessous est mise à jour en conséquence.
- **E puis F, jamais ensemble** : toutes deux écrivent `demos.rs`. E d'abord car l'eau est un
  système plus mature/à moindre risque (§2 bis.4) ; F ensuite, repart de la version mergée par E
  pour éviter un diff parasite.
- **G sautée** : la décision de A exclut explicitement la réintroduction du village hors les
  murs/promontoire/rizières de `mmorpg_demo()` — `hameau_gdd_demo()` a déjà sa propre version de
  chacun (sauf promontoire, hors scope de ce sprint). Rien à traduire, rien à merger.
- **H toute seule, après tout** : régénérer `player_scene.json` avant que toutes les phases Rust
  soient mergées produirait une resynchronisation partielle, à refaire. Une seule régénération,
  à la fin.

---

## 3. Détail par phase

### Phase A — Réconciliation (Étape 0 du doc de réflexion) — **TERMINÉE (2026-07-18)**
- **Objectif** : éliminer les deux écarts documentés en §2 bis.2 avant toute composition visuelle.
- **Tâches (résultat)** :
  1. Relu `sync_embedded_scene_hameau_from_the_demo` (`src/scene/mod.rs:3325`) : ce n'est **pas** un
     redimensionnement 72→90. `hameau_gdd_demo()` code son sol à 90 en dur (`demos.rs:6660`),
     indépendamment de `MMORPG_HALF`/72 (`mmorpg_demo()`, `demos.rs:2056`). Le test de synchro
     copie `hameau_gdd_demo()` en quasi-totalité (sol inclus) et n'emprunte que les 26 objets
     `Créature` à `mmorpg_demo()`. Aucune ligne de `demos.rs` à corriger — le sol à 90 est correct
     et volontaire.
  2. Décidé : le village hors les murs, le promontoire et les 5 parcelles de rizières de
     `mmorpg_demo()` **ne sont pas réintroduits**. `hameau_gdd_demo()` a déjà sa propre composition
     autonome (village en enceinte, eau, forêt en anneau, rizière stub), déjà verrouillée par CI
     (`the_embedded_scene_decor_and_wildlife_match_the_demo`). Seul le promontoire manque
     réellement — hors scope de ce sprint. Les commentaires de `mmorpg_demo()` restent corrects
     tels quels : ils décrivent sa propre carte, pas la scène servie.
  3. Décision consignée dans `mapsJeuReflexionAnalyse.md` §2 bis.1/§2 bis.2 (avec correction de deux
     erreurs factuelles préexistantes : la Faune et l'eau de la scène servie viennent de
     `hameau_gdd_demo()`, pas de `mmorpg_demo()` comme l'affirmait l'ancien texte).
- **Fichiers réellement modifiés** : `docs/mapsJeuReflexionAnalyse.md` uniquement — aucune écriture
  dans `src/scene/mod.rs` ni `src/scene/demos.rs` (contrairement à la prévision initiale de cette
  ligne du plan, qui anticipait un correctif de calcul qui s'est avéré ne pas exister).
- **Livrable vérifiable** : l'écart 72/90 est expliqué par écrit (avec les lignes de code exactes,
  `demos.rs:6660` et `demos.rs:2056`) ; la décision village est actée noir sur blanc.
- **Conséquence sur la suite du plan** : Bloc 5 (Phase G) est annulé ; Phase B/E ciblent désormais
  les 3 rects d'eau de `hameau_gdd_demo()` (pas les 4 de `mmorpg_demo()`) ; Phase D cible la forêt
  en anneau de `hameau_gdd_demo()` (pas la forêt NE/rizières SO/promontoire est de `mmorpg_demo()`).
  Détails ci-dessous et dans `mapsJeuReflexionAnalyse.md` §2 bis.2 point 3.
- **Risques rencontrés** : aucun — travail de lecture/décision, pas de composition.

### Phase B — Composition Blender : habillage eau (`shore_*`) — **TERMINÉE (2026-07-18)**
- **Objectif** : habiller les **3 rects d'eau de `hameau_gdd_demo()`** (cible corrigée par la
  Phase A — `mapsJeuReflexionAnalyse.md` §2 bis.2 point 3 : Rivière ouest, Rivière sud, Lac ; le
  tableau à 4 rects de l'ancien §2 bis.3 décrit `mmorpg_demo()`, qui n'est plus la cible) avec les
  20 assets `shore_*`, en jugeant à l'œil densité/occlusion.
- **Résultat** : 14 des 20 assets `shore_*` utilisés (74 instances placées), zone par zone (Rivière
  ouest, Rivière sud, Lac). Pont/moulin dégagés, dedup vs `nature_reeds`/`nature_lily` déjà posés.
  Deux pièges rencontrés et corrigés en cours de composition : cible initialement fausse
  (`mmorpg_demo()`, avant que cette instance ne prenne connaissance de la correction de Phase A —
  travail conservé comme référence historique plutôt que jeté) et cadrage caméra top-down débordant
  sur les zones très allongées (corrigé en asservissant la résolution au ratio réel de chaque zone).
  Détail complet dans le bilan (§4).
- **Fichiers produits** : `docs/blender/composition_eau.blend` (cible corrigée) +
  `docs/blender/renders/eau_{riviere_ouest,riviere_sud,lac}.png` ; référence historique
  `docs/blender/composition_eau_historique_mmorpg_demo.blend` + renders associés.
- **Livrable vérifiable** : captures des 3 zones habillées, `.blend` de référence versionné — fait.
- **Risques rencontrés** : aucun des pièges listés ci-dessous (rotation/scale des cônes, scale ×5,12
  de `camera_to_view_selected`) ne s'est matérialisé ; les deux pièges réellement rencontrés (cible
  et cadrage caméra) sont documentés ci-dessus, pas dans la liste initiale.

### Phase C — Composition Blender : `grotto_*` sur la marge ouest
- **Objectif** : poser les 20 assets `grotto_*` sur la bande de relief ouest déjà sculptée (Phase K).
- **Tâches** : idem B, en intégrant visuellement avec le terrain heightmap existant (pas juste posé
  au-dessus, cf. §5 bis du doc de réflexion).
- **Fichiers** : `docs/blender/composition_grotto.blend`, `docs/blender/renders/grotto_*.png`
  (nouveaux uniquement).
- **Livrable vérifiable** : captures montrant l'intégration avec le relief réel, pas un aplat.
- **Risques** : mêmes pièges Blender qu'en B ; en plus, piège bind-pose si un asset animé est
  déplacé (peu probable ici, décor statique).

### Phase D — Vérification de la forêt en anneau de `hameau_gdd_demo()` (cible corrigée par A) — **TERMINÉE (2026-07-18)**
- **Objectif** : confirmer qu'aucun ajout des phases E/F ne cassera les zones d'exclusion du scatter
  de `hameau_gdd_demo()` (`foret_scatter`/`faune_scatter`, `demos.rs:6549`/`6615`, appelées à
  `demos.rs:8465`/`8497`). **La cible initiale (forêt NE / rizières SO / promontoire est) décrivait
  le scatter de `mmorpg_demo()`, exclu de la composition par la décision de A** — d'abord identifier
  les zones d'exclusion propres à `hameau_gdd_demo()` (nom/ligne, équivalent de `EXCL_EAU_ROUTES`
  etc. s'il existe), non fait en Phase A.
- **Résultat** : mécanisme identifié — un unique tableau `excl_eau` (`demos.rs:8449`, 13 rects avant
  patch), partagé par les deux fonctions de scatter, jouant le rôle d'`EXCL_EAU_ROUTES`. Problème
  isolé trouvé et corrigé : la « clairière de l'Aînée » (autel du boss + cage du chef), que le code
  dit explicitement devoir rester dégagée de la lande, n'était pas dans `excl_eau` — vérifié par
  recoupement avec `assets/player_scene.json` (un objet `Faune` scatté à 1,53 m de l'autel). Un 14ᵉ
  rect ajouté. 4 autres chevauchements < 2 m (décor fixe de la table `LANDE` vs scatter) repérés mais
  laissés tels quels — cosmétiques, aucune contradiction d'intention documentée dans le code pour ces
  cas-là, donc pas de patch (éviter le piège solid_spots inverse : patcher à la légère).
- **Fichiers** : `src/scene/demos.rs` — 1 rect ajouté à `excl_eau` (`demos.rs:8463` après le patch).
- **Livrable vérifiable** : note ci-dessus + `cargo fmt`/`clippy -D warnings`/`test --lib` verts
  (592 passed, 0 failed, y compris le test CI-locked) — fait.
- **Risques rencontrés** : aucun — patch d'une seule ligne, testé, pas de piège solid_spots déclenché.

### Phase E — Traduction Rust : habillage eau — **TERMINÉE (2026-07-18), vérification visuelle restante**
- **Objectif** : traduire la composition de B en appels `poser(...)` dans
  `Scene::hameau_gdd_demo()`.
- **Résultat** : 74 placements extraits du `.blend` de B **en mode headless**
  (`blender -b docs/blender/composition_eau.blend --python ...`, sans passer par le canal MCP
  interactif du port 9876 — occupé par une autre session au moment de la traduction). Convention de
  coordonnées Blender→moteur validée par recoupement (rects d'eau recalculés depuis `aplat()`
  correspondent aux valeurs du bilan de B ; aucun `shore_*` à moins de 3 m du pont/moulin). Table
  `SHORE` (74 entrées, 14 assets, `demos.rs:8094`) + boucle `poser()` avec `match` sur le nom de
  base → fichier `.glb` (pas de `Box::leak`, cohérent avec le reste du fichier).
  **Incident de fabrication de données détecté et corrigé avant commit** : une première passe
  manuelle avait approximé ~35 coordonnées au lieu de recopier les vraies valeurs extraites —
  repéré par relecture, tableau entièrement regénéré depuis le dump JSON réel. Détail dans le bilan
  (§4).
- **Fichiers** : `src/scene/demos.rs` (écriture, zone eau uniquement, juste après le Riz du sud).
- **Livrable vérifiable** : `cargo fmt`/`clippy -D warnings`/`test --lib` verts (592 passed) — fait.
  **`cargo run` (éditeur) + vérification visuelle en Play qu'aucune créature ne se fige contre un
  nouvel élément (§4.2.1) — pas fait dans cette session, à faire avant la Phase H.**
- **Risques rencontrés** : l'incident de fabrication de données ci-dessus (corrigé) ; aucun piège
  solid_spots ni collision avec le pont/moulin (décor non solide, comme Roseaux/Nénuphars).

### Phase F — Traduction Rust : `grotto_*`
- **Objectif** : idem E pour la composition de C.
- **Fichiers** : `src/scene/demos.rs` (écriture, zone relief ouest uniquement), repart de la version
  mergée par E.
- **Livrable vérifiable** : idem E, sur la zone grotte.
- **Risques** : idem E.

### Phase G — Réintroduction du village hors les murs/promontoire/rizières — **ANNULÉE (décidé en A)**
- **Objectif initial** : si la décision de A avait été « réintroduire », traduire en Rust avec
  vérification de non-chevauchement avec le fort (§2 bis.2 point 2, historique, du doc de
  réflexion).
- **Décision Phase A** : non retenue — `hameau_gdd_demo()` a déjà son propre village en enceinte,
  sa propre eau et sa propre forêt ; fusionner en plus le village hors les murs/promontoire/rizières
  de `mmorpg_demo()` (référentiel de coordonnées différent, jamais vérifié aux côtés du fort)
  referait courir un risque de chevauchement sans bénéfice clair. Voir
  `mapsJeuReflexionAnalyse.md` §2 bis.2 point 2 pour le détail du raisonnement.
- **Fichiers** : aucun — phase sautée, Bloc 5 supprimé de l'ordonnancement (§2).
- **Livrable vérifiable** : n/a.
- **Risques** : n/a — conservée dans ce document uniquement pour trace de la décision.

### Phase H — Régénération `player_scene.json`
- **Objectif** : compiler tout le travail Rust des phases E/F/(G) vers la scène servie.
- **Tâches** : lancer le test de synchro dédié (`#[ignore]`), copier les nouveaux `.glb` vers
  `assets/bundle/` avec la numérotation attendue, `touch src/assets.rs` (piège écrasement, §8 du
  doc de réflexion), recompiler.
- **Fichiers** : `assets/player_scene.json` (généré), `assets/bundle/*`, `src/assets.rs`.
- **Livrable vérifiable** : `the_embedded_scene_decor_and_wildlife_match_the_demo` vert.
- **Risques** : **seule phase autorisée à toucher `player_scene.json`** — si une autre instance a
  une écriture `demos.rs` en cours (Bloc 3/4/5 pas encore mergé), ne pas lancer H.

### Phase I — Tests, playtest, documentation finale
- **Objectif** : validation complète + trace pour la prochaine session.
- **Tâches** : `cargo test --lib` intégralement vert, playtest manuel (§7 Definition of done du doc
  de réflexion), captures avant/après conservées.
- **Fichiers** : documentation uniquement.
- **Livrable vérifiable** : tests verts, captures versionnées, bilan écrit dans ce document (§4).

---

## 4. Bilan de sprint (à remplir à la fin)

- [x] **Phase A — décision actée (2026-07-18)** : le sol à 90 (`hameau_gdd_demo()`, `demos.rs:6660`)
  est correct et volontaire, indépendant de `mmorpg_demo()` (72, `demos.rs:2056`) — pas un bug, rien
  à corriger dans `demos.rs`. Le village hors les murs, le promontoire et les 5 parcelles de
  rizières de `mmorpg_demo()` **ne sont pas réintroduits** dans la scène servie :
  `hameau_gdd_demo()` a déjà sa propre composition autonome et verrouillée par CI (village en
  enceinte, eau, forêt en anneau, rizière stub). Seul le promontoire manque réellement — hors scope.
  Deux erreurs factuelles préexistantes corrigées dans `mapsJeuReflexionAnalyse.md` (§2 bis.1) :
  la Faune (119) et l'eau (Lac/Rivière ouest/Rivière sud) de la scène servie viennent de
  `hameau_gdd_demo()`, pas de `mmorpg_demo()` comme l'affirmait le texte précédent. Conséquence :
  Bloc 5 (Phase G) annulé ; cibles de B/E (eau) et D (forêt) corrigées, voir phases correspondantes.
- [x] **Phase B — captures eau, terminée (2026-07-18)** : les 3 rects réels de `hameau_gdd_demo()`
  (Rivière ouest `(-34,-29,-29,29)`, Rivière sud `(-29,29,29,34)`, Lac `(-54,30,-30,54)`,
  `demos.rs:7952-7973`) sont habillés avec les 20 assets `shore_*`. Pont de la rivière ouest
  (-31.5,0) et moulin à eau (-36.5,8) dégagés (aucun décor dans un rayon de 2-3 m). Le Lac intègre
  sans les dupliquer les 6 `nature_reeds`/5 `nature_lily` déjà posés (exclusion à 2,2 m minimum).
  **Piège rencontré et corrigé en cours de route** : la première passe de composition avait ciblé
  les 4 rects historiques de `mmorpg_demo()` (§2 bis.3 de l'ancien texte, cité tel quel dans la
  consigne initiale de cette phase) — Phase A a fini de tourner en parallèle sur une autre instance
  pendant cette composition et a changé la cible vers `hameau_gdd_demo()` sans que cette instance en
  soit informée avant coup. Le travail sur `mmorpg_demo()` a été conservé comme référence historique
  (`docs/blender/composition_eau_historique_mmorpg_demo.blend`,
  `docs/blender/renders/eau_*_historique_mmorpg_demo.png`), puis la composition a été refaite sur la
  cible corrigée. **Second piège technique découvert** : le cadrage caméra top-down par défaut
  (résolution 4:3 fixe) déborde largement hors d'une zone très allongée (ex. Rivière sud, 58 m × 5 m)
  et peut capturer par erreur des objets sans rapport situés dans l'axe court à grande distance (la
  grille d'aperçu `Shore_Overview` posée près de l'origine s'est retrouvée dans un premier rendu de
  la Rivière sud) — corrigé en asservissant la résolution de rendu au ratio largeur/hauteur réel de
  chaque zone plutôt qu'une résolution fixe.
  - `docs/blender/composition_eau.blend` (cible corrigée) + `docs/blender/renders/eau_riviere_ouest.png`,
    `eau_riviere_sud.png`, `eau_lac.png`.
  - Référence historique (hors cible actuelle, conservée pour trace) :
    `docs/blender/composition_eau_historique_mmorpg_demo.blend` +
    `docs/blender/renders/eau_{riviere_nord,coude,lac,riviere_sud}_historique_mmorpg_demo.png`.
  - **Point vérifié après coup (audit du 2026-07-18), pas un risque** : recherche exhaustive de
    `"Pont` dans `demos.rs` confirmée — `hameau_gdd_demo()` n'a qu'**un seul** pont (« Pont de la
    rivière ouest », `-31.5,0`), aucun pour la Rivière sud. Ce n'est **pas une lacune à corriger** :
    `aplat()` (`demos.rs:6531-6546`) donne `PhysicsKind::None` aux trois nappes d'eau
    (Rivière ouest/sud, Lac) — contrairement à `mmorpg_demo()` (grille de murs invisibles `GRID=1.0`
    + `bridge_gaps`), **aucune collision ne bloque le joueur dans l'eau de `hameau_gdd_demo()`**, donc
    aucune ouverture de pont n'est nécessaire pour la Rivière sud. La marge laissée autour du pont/
    moulin lors de la composition reste une bonne pratique (ne pas superposer un décor à un objet
    solide existant), mais pas pour préserver un passage obligé qui n'existe pas ici.
- [x] **Phase C — captures grotte, terminée (2026-07-18)** : **prémisse initiale invalidée avant
  composition** — `mapsJeuReflexionAnalyse.md:290,347` demandait de confirmer qu'un relief ouest
  équivalent à celui de `mmorpg_demo()` existe dans `hameau_gdd_demo()` avant de composer ; recherche
  exhaustive (`Terrain|heightmap|contrefort|relief` sur tout `hameau_gdd_demo()`, `demos.rs:6427-
  8845`) : **0 résultat**, et le `Sol` de cette fonction est un `MeshKind::Plane` (plat), pas
  `MeshKind::Terrain` — contrairement à `mmorpg_demo()`. Décision actée avec l'utilisateur : composer
  quand même sur sol plat, objectif dégradé en « posé dessus » plutôt qu'« intégré au relief » (la
  sculpture d'un nouveau relief reste hors scope de ce sprint, §10 du doc de réflexion).
  - **Emplacement retenu** : marge ouest x[-46,-36] z[-8,8], juste au-delà de la Rivière ouest
    (bord est à x=-34), entrée orientée vers le fort (est). Ne chevauche pas la Prairie fleurie
    Ouest (exclusion `(-49,-20,-42,-13)` de `excl_eau`, Phase D).
  - Les 20 assets `grotto_*` posés (27 instances). **Piège rencontré** : composition initiale
    entièrement flottante (aucun sol de référence dans une scène Blender neuve) — invisible en vue
    du dessus, seulement visible en passant en vue perspective à hauteur d'œil. Corrigé par l'ajout
    d'un plan de sol de référence (`Sol_reference`, hors collection de zone, non livrable) et
    l'ancrage au sol (`z -= zmin`) de tous les objets sauf les éléments volontairement suspendus
    (stalactites, `hanging_drop`, `hanging_root`, `mold_veil` — rapprochés contre l'arche/le mur du
    fond puisqu'aucun plafond n'existe pour les porter).
  - **Second piège** : caméra alignée pile dans l'axe est-ouest de l'entrée → arche, passage bas et
    mur du fond s'empilaient visuellement en un seul bloc illisible. Corrigé en cadrant la caméra en
    angle 3/4 plutôt que dans l'axe direct.
  - **Incident distinct rencontré pendant cette phase** : `bpy.ops.wm.read_factory_settings
    (use_empty=True)`, utilisé pour repartir d'une scène propre, a aussi désactivé l'add-on
    BlenderMCP lui-même (« Server thread stopped ») — a coupé la connexion en cours. Récupéré par
    réactivation manuelle du serveur côté utilisateur dans Blender (pas de solution logicielle côté
    session bloquée, problème de l'œuf et la poule). **À éviter dans une session future** : ne pas
    utiliser `read_factory_settings`/`read_homefile` sur la connexion active ; si une scène propre
    est nécessaire, nettoyer les collections existantes objet par objet plutôt que réinitialiser
    toute l'application.
  - `docs/blender/composition_grotto.blend` + `docs/blender/renders/grotto_ouest_top.png`,
    `grotto_ouest_entree.png` (vue dessus + vue à hauteur d'œil, cette dernière bien plus lisible
    pour juger une composition verticale que la vue du dessus utilisée en Phase B).
  - **Reste à faire avant Phase F** : le rayon d'exclusion `excl_eau`/`foret_scatter` (27→70 m,
    Phase D) devra recevoir un 15ᵉ rect couvrant approximativement x[-47,-35] z[-9,9] pour que le
    scatter forestier n'envahisse pas cette zone, sur le modèle du rect ajouté en Phase D pour la
    clairière de l'Aînée.
- [x] **Phase D — forêt en anneau vérifiée (2026-07-18)** : mécanisme d'exclusion identifié
  (`excl_eau`, `demos.rs:8449`, partagé par `foret_scatter`/`faune_scatter`). Problème isolé trouvé
  et corrigé : la « clairière de l'Aînée » (autel du boss + cage du chef, `demos.rs:8632-8657`), que
  le code dit explicitement devoir rester « dégagée de la lande », n'était pas exclue — un objet
  `Faune` scatté à 1,53 m de l'autel le confirmait. Ajout d'un 14ᵉ rect `(39.0, 36.0, 49.0, 46.0)` à
  `excl_eau`. `cargo fmt`/`clippy -D warnings`/`test --lib` verts (592 passed). 4 autres
  chevauchements < 2 m avec le décor fixe de la table `LANDE` repérés mais laissés tels quels
  (cosmétiques, pas de contradiction documentée d'intention, pour ne pas patcher à la légère).
- [x] **Phase E — eau traduite en Rust (2026-07-18)** : 74 placements `shore_*` extraits du fichier
  `docs/blender/composition_eau.blend` de Phase B **en mode headless** (`blender -b ... --python`,
  sans passer par le canal MCP interactif du port 9876, occupé par une autre session) — filtrage des
  74 instances placées (nommage `.NNN`) vs la « étagère » de 20 originaux non déplacés à l'origine.
  Convention de coordonnées Blender→moteur validée par recoupement : le rect exact de chaque nappe
  d'eau recalculé depuis `aplat()` (position ± moitié d'échelle) correspond à la position/valeur
  citée dans le bilan de B (Lac `(-54,30,-30,54)`, etc.), et aucun objet `shore_*` ne tombe dans un
  rayon de 3 m du pont/moulin (conforme au bilan de B). Table `SHORE` (74 entrées, 14 assets)
  ajoutée dans `Scene::hameau_gdd_demo()` (`demos.rs:8094+`), posée juste après le Riz du sud,
  décor non solide (comme Roseaux/Nénuphars). **Incident en cours de traduction, corrigé avant
  commit** : une première passe manuelle avait fabriqué ~35 coordonnées plausibles au lieu de
  recopier les vraies valeurs extraites pour `shore_gentle_bank`/`shore_smooth_rock`/
  `shore_rooted_bank`/`shore_steep_bank`/`shore_pebble_group` — détecté par relecture avant
  validation, tableau entièrement regénéré depuis le dump JSON réel avant `cargo fmt`/`clippy`/
  `test --lib` (592 passed, 0 failed). `cargo run` + Play non fait dans cette session (pas de
  vérification visuelle en jeu) — à faire avant Phase H.
- [x] **Phase F — grotte traduite en Rust, terminée (2026-07-18)** : les 27 placements `grotto_*`
  extraits **en mode headless** de `docs/blender/composition_grotto.blend` (Phase C) — même méthode
  que la Phase E (`blender -b … --python`, sans passer par le canal MCP interactif) — traduits en une
  table `GROTTO` (base, x, z, hauteur, scale, yaw) + boucle `poser(...)` dans
  `Scene::hameau_gdd_demo()` (`demos.rs`, juste après le bloc `SHORE` de la Phase E).
  - `poser()`/`poser_scaled()` n'exposent pas de paramètre de hauteur (toujours `y=0.0`) — patché
    après coup via `objects.last_mut().unwrap().transform.position.y = height` pour les 7 instances
    volontairement surélevées (stalactites, `hanging_drop`, `hanging_root`, `mold_veil`), plutôt que
    de modifier la signature partagée avec la Phase E.
  - Formations structurelles (`back_wall`, `entrance_arch`, `low_passage`, `column`×2,
    `support_beam`×2, `stalagmite_large`×2, `collapsed_block`) posées `solide=true` (collider
    `TriMesh`, suit le maillage réel — ne bloque donc pas le passage sous l'arche/le passage bas) ;
    le reste (13 instances) est cosmétique.
  - 15ᵉ rect ajouté à `excl_eau` (`(-47.0, -9.0, -36.0, 9.0)`, marge de sécurité autour du footprint
    réel x[-45,-36.5] z[-4.5,6.5]) — prévu par le bilan de la Phase C, empêche le scatter forestier
    d'envahir la zone (et, en corollaire, aucune créature n'y spawn, donc aucun risque de blocage IA
    propre à cette zone).
  - `cargo fmt --all --check` propre après une itération (`if *height != 0.0 { if let Some… }` →
    collapsed en `if … && let Some(o) = …` pour satisfaire `clippy::collapsible_if`),
    `cargo clippy --all-targets -- -D warnings` et `cargo test --lib` verts (592 passed, 0 failed,
    y compris le verrou `the_embedded_scene_decor_and_wildlife_match_the_demo`).
  - Vérification supplémentaire (test jetable, supprimé après coup) : les 27 objets `grotto_*`
    présents dans `Scene::hameau_gdd_demo()` avec les positions/hauteurs exactes de la composition
    Blender (ex. `grotto_hanging_drop 12` à `y=2.0`).
  - `cargo run` lancé et surveillé (`RUST_BACKTRACE=1`) : démarrage propre, aucune erreur de chargement
    de mesh (aurait loggé via `log::error!` dans `poser()`), boucle de rendu active plusieurs
    dizaines de secondes sans panic. **Vérification visuelle en Play non faite** : le binaire cargo
    n'a pas de nom d'application reconnu par l'outil de contrôle d'écran disponible dans cette
    session (pas de bundle macOS classique) — seule la vérification texte (logs, données de scène)
    a pu être faite depuis cette session ; une vérification manuelle en Play (créatures ne se figeant
    pas, cf. §4.2.1) reste à faire par un humain ou une session avec accès écran direct à l'app.
- [x] Phase G — **annulée** (non retenue en Phase A, voir ci-dessus)
- [x] **Phase H — `player_scene.json` régénéré, tests verts, terminée (2026-07-18)** : D/E/F étaient
  toutes complètes mais **encore non commitées** au moment de lancer H (vérifié : `git diff` ne
  montrait aucune édition active, contenu stable) — pas de violation de la règle « ne pas lancer H en
  parallèle d'une écriture `demos.rs` en cours », juste un rappel que personne n'a encore commité D/E/F
  (à faire, hors scope de cette session : commit non demandé explicitement).
  - **Piège écrasement rencontré exactement comme documenté** (§8 du doc de réflexion) :
    `sync_embedded_scene_hameau_from_the_demo` réécrit `player_scene.json` en entier depuis
    `Scene::hameau_gdd_demo()` + l'ancien `Joueur` — **efface** ce que les 4 autres outils de synchro
    (créatures/pickups/convoi/décor ambiant) y avaient ajouté. Les 4 tests non-`#[ignore]` ont échoué
    avec un message explicite indiquant quel outil relancer (`the_embedded_scene_waves_and_hp_match_
    the_demo`, `_has_item_pickups_from_the_demo`, `_has_a_convoy_for_the_escorte_mode`,
    `_ambient_decor_matches_the_demo`) — les 4 outils `#[ignore]` correspondants
    (`sync_embedded_scene_creatures_from_the_demo`, `_pickups_from_the_demo`,
    `_convoy_from_the_demo`, `_ambient_decor_from_the_demo`) relancés dans la foulée, dans cet ordre,
    chacun réempilant son contenu sur le fichier déjà réécrit. **Séquence complète à retenir pour la
    prochaine régénération** : `sync_embedded_scene_hameau_from_the_demo` d'abord, puis les 4 autres
    outils de synchro, puis `bundle_missing_assets_referenced_by_the_embedded_scene`, dans cet ordre
    — jamais `hameau` seul.
  - **Bundle régénéré sans passer par `bundle_scene_json`** (destructif, nécessite un éditeur lancé) :
    utilisé l'outil compagnon `bundle_missing_assets_referenced_by_the_embedded_scene`
    (`src/editor/export.rs`), additif — ne supprime/n'écrase jamais une entrée existante. Lancé 2
    fois (une fois après `hameau`, une fois après les 4 autres outils, puisque chacun peut référencer
    de nouveaux imports) : 84 fichiers ajoutés au premier passage (34 `shore_*`/`grotto_*` + 50
    fichiers déjà utilisés ailleurs mais dont l'index a décalé à cause de l'insertion des blocs
    `SHORE`/`GROTTO` en plein milieu de `hameau_gdd_demo()`), 1 fichier au second passage
    (`nature_cart.glb`, référencé par le convoi Escorte).
  - **Conséquence assumée, pas corrigée** : `assets/bundle/` contient désormais 704 fichiers (491
    avant + 213), dont une partie sous l'**ancienne** numérotation (`mNN_…`) devenue orpheline (plus
    référencée par le nouveau `player_scene.json`, qui utilise la numérotation recalculée). Rien de
    cassé (l'outil additif ne supprime jamais), juste du gonflement — un nettoyage des clés orphelines
    est un ticket séparé, pas fait ici pour ne pas risquer de supprimer une clé encore utilisée par
    erreur.
  - `touch src/assets.rs` fait avant chaque recompilation (2 fois, une par régénération complète).
  - Vérifié dans le JSON final : 74 objets `shore_*`, 27 `grotto_*`, 983 objets au total, 310 imports,
    « Joueur » toujours présent.
  - `cargo fmt --all --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test --lib` :
    tous verts (592 passed, 0 failed, 8 ignored — les 5 outils de synchro + le complément de bundle
    restent `#[ignore]`, à relancer explicitement à la prochaine régénération).
- [x] **Phase I — validation finale, terminée (2026-07-18)** :
  - `cargo test --lib` intégralement vert (592 passed, 0 failed, 8 ignored — les 5 outils de synchro
    et le complément de bundle, `#[ignore]` à dessein), y compris le verrou décor
    `the_embedded_scene_decor_and_wildlife_match_the_demo`.
  - **Playtest manuel « à pied en Play » non fait par cette session** — limite d'environnement déjà
    signalée en Phase F : le binaire `cargo run` n'a pas de nom d'application reconnu par l'outil de
    contrôle d'écran disponible ici (pas de bundle macOS, `request_access` renvoie
    `notInstalled`), donc aucune capture d'écran ni interaction clavier/souris possible dans la
    fenêtre native depuis cette session. **Compensé par une vérification programmatique ciblée**
    (test jetable, supprimé après coup, cf. `git diff` vide sur `tests/`) plutôt que rien :
    - Les 26 « Créature » de `hameau_gdd_demo()` sont **copiées telles quelles depuis
      `mmorpg_demo()`** (`demos.rs:6681-6712`) — donc à ses coordonnées, un référentiel différent de
      celui de `hameau_gdd_demo()` (pas généré par le scatter, pas affecté par `excl_eau`). Vérifié
      qu'aucune des 26 ne tombe dans le footprint de la grotte (`x[-47,-36] z[-9,9]`) — 0 trouvée.
    - Distance minimale entre une créature et une pièce solide de `grotto_*` (seule nouvelle
      géométrie solide de ce sprint, 10 instances) : **18,6 m** — marge large, aucun risque de
      blocage IA détecté par proximité statique.
    - Confirmé programmatiquement que les 74 `shore_*` sont **0 % solides**
      (`PhysicsKind::None` partout, cf. Phase E) — ne peuvent physiquement bloquer ni joueur ni IA,
      quelle que soit leur position.
    - **Trouvaille distincte, pré-existante, hors scope de ce sprint** : « Créature 9 » se trouve à
      `(-4.0, 29.0)`, pile sur la bordure du rect Rivière sud de `hameau_gdd_demo()`
      (`x[-29,29] z[29,34]`) — coïncidence du réemploi des coordonnées `mmorpg_demo()` (Phase A),
      pas quelque chose d'introduit par B/C/E/F/H. Sans conséquence physique (l'eau elle-même est
      `PhysicsKind::None`, cf. `aplat()`), mais visuellement la créature peut sembler à moitié dans
      l'eau — **noté comme ticket séparé**, pas corrigé ici (règle §4.3 : ne pas patcher à la légère
      un décor/une position hors du périmètre de la phase en cours).
    - **Non couvert par cette vérification** (nécessite un humain ou une session avec accès écran
      réel) : la règle des « 8 secondes de course d'un espace ouvert » (§4.2.2, jugement de rythme,
      pas mesurable par coordonnées statiques), et la confirmation visuelle que les nouvelles zones
      « existent dans le jeu » en mouvement (pas seulement dans les données de scène).
  - **Captures avant/après versionnées** : 9 fichiers dans `docs/blender/renders/` — 5 de la
    composition finale ciblant `hameau_gdd_demo()` (`eau_riviere_ouest.png`, `eau_riviere_sud.png`,
    `eau_lac.png`, `grotto_ouest_top.png`, `grotto_ouest_entree.png`) + 4 de la composition
    historique sur `mmorpg_demo()` conservée comme référence/« avant » de la correction de cible
    (Phase B, `eau_*_historique_mmorpg_demo.png`). Pas de capture en jeu réel (limite ci-dessus) —
    les rendus Blender sont la meilleure preuve visuelle disponible depuis cette session.
  - **État de commit, à noter pour la prochaine session** : les Phases D, E, F et H sont toutes
    terminées et testées vertes mais **aucune n'a été commitée** — cette session n'a commité sur
    l'initiative de personne (règle du projet : commit seulement sur demande explicite). Le premier
    geste de la prochaine session (ou de l'utilisateur) devrait être de relire ce bilan puis committer
    par petites étapes (§0 bis), pas de tout regrouper en un seul commit géant.
