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
| **A** — Réconciliation (échelle sol, décision village) | `src/scene/mod.rs` (lecture), `src/scene/demos.rs` (écriture ciblée : uniquement le calcul du sol), `mapsJeuReflexionAnalyse.md` (décision actée) | Non |
| **B** — Composition Blender de l'habillage eau (`shore_*`) | Aucun fichier source — uniquement des fichiers nouveaux : `.blend` de référence + captures (chemin proposé : `docs/blender/composition_eau.blend`, `docs/blender/renders/eau_*.png`) | **Oui, exclusif** |
| **C** — Composition Blender de `grotto_*` sur la marge ouest | Aucun fichier source — fichiers nouveaux (`docs/blender/composition_grotto.blend`, `docs/blender/renders/grotto_*.png`) | **Oui, exclusif** |
| **D** — Vérification forêt NE / rizières SO / promontoire est | `src/scene/demos.rs` (lecture seule, sauf correctif isolé sur une zone d'exclusion si un problème est trouvé) | Non |
| **E** — Traduction Rust de l'habillage eau (depuis B) | `src/scene/demos.rs` (écriture, zone eau) | Non |
| **F** — Traduction Rust de `grotto_*` (depuis C) | `src/scene/demos.rs` (écriture, zone relief ouest) | Non |
| **G** — Réintroduction du village/promontoire/rizières (si retenue en A) | `src/scene/demos.rs` (écriture), `src/scene/mod.rs` (si le test de synchro doit couvrir les nouveaux éléments) | Non (sauf vérification visuelle optionnelle de non-chevauchement, §2 bis.2 point 3 du doc de réflexion — dans ce cas, Blender exclusif ponctuel) |
| **H** — Régénération `player_scene.json` + assets bundle | `assets/player_scene.json` (généré, jamais édité à la main), `assets/bundle/*`, `src/assets.rs` (touch, cf. piège écrasement scène embarquée) | Non |
| **I** — Tests, playtest, documentation finale | `docs/mapsJeuReflexionAnalyse.md`, `docs/sprintjeurefelxion.md` (bilan), pas de fichier source | Non |

**Verdict de recoupement** : `src/scene/demos.rs` est touché par **A, D, E, F, G** — cinq phases sur
neuf. C'est le vrai goulet, exactement comme `protocol.rs`/`hud.rs` dans le sprint précédent. Aucune
paire de ces cinq phases ne peut tourner en même temps sur deux instances sans risque d'écrasement.
`B` et `C` ne touchent **aucun** fichier source (uniquement des fichiers Blender/captures nouveaux)
— elles sont donc compatibles avec absolument tout, y compris entre elles sur le plan des fichiers.
**Mais B et C sont mutuellement exclusives sur le canal Blender** (§0) : jamais les deux en même
temps, même si leurs fichiers ne se recoupent pas.

---

## 2. Les blocs — dans cet ordre, chacun attend que le précédent soit fini et mergé

| Bloc | Phases dedans | Compatible en parallèle ? | Doit attendre la fin de |
|---|---|---|---|
| **1** | **A** + **B** | Oui — A ne touche pas Blender, B ne touche pas `demos.rs` au-delà du sol (déjà réservé à A) | — (démarre tout de suite) |
| **2** | **C** + **D** | Oui — C est Blender-exclusif mais sans fichier source ; D est lecture seule sur `demos.rs` (version déjà stabilisée par A) | **Bloc 1** (C reprend le canal Blender juste libéré par B ; D doit lire le sol corrigé par A) |
| **3** | **E** seule | Non — écrit `demos.rs` | **Bloc 1** (dépend des captures de B) |
| **4** | **F** seule | Non — écrit `demos.rs`, doit repartir de la version mergée par E | **Bloc 2** (dépend des captures de C) **et Bloc 3** (repart de `demos.rs` après E) |
| **5** | **G** seule (si retenue, sinon bloc sauté) | Non — écrit `demos.rs` et potentiellement `mod.rs` | **Bloc 4** |
| **6** | **H** seule | Non — seule autorisée à régénérer `player_scene.json`, jamais en parallèle d'une écriture `demos.rs` en cours | **Bloc 5** (ou Bloc 4 si G non retenue) |
| **7** | **I** seule | — | **Bloc 6** |

**Seuls les Blocs 1 et 2 permettent un vrai travail à deux instances simultané.** À partir du Bloc 3,
c'est strictement séquentiel — une seule instance à la fois, l'autre peut préparer la phase suivante
en lecture seule (relire le code, préparer son propre brouillon de coordonnées) mais ne doit **rien
écrire** avant que la phase en cours soit committée.

### Pourquoi cet ordre précis
- **A et B en premier** : A élucide l'écart d'échelle et tranche le sort du village — sans ça,
  toute coordonnée choisie ensuite dans Blender (B, C) serait posée sur un référentiel faux. B peut
  démarrer immédiatement en parallèle car composer l'habillage eau ne dépend pas du résultat de A
  (les rects d'eau sont déjà fixes, §2 bis.3 du doc de réflexion).
- **C et D après** : C a besoin que B ait rendu la main sur Blender (règle d'exclusivité, pas de
  dépendance de contenu). D a besoin que A ait corrigé le sol pour vérifier les biomes sur les
  bonnes coordonnées.
- **E puis F, jamais ensemble** : toutes deux écrivent `demos.rs`. E d'abord car l'eau est un
  système plus mature/à moindre risque (§2 bis.4) ; F ensuite, repart de la version mergée par E
  pour éviter un diff parasite.
- **G en dernier des écritures `demos.rs`** : c'est la décision la plus lourde (réintroduction d'un
  biome entier) et la plus incertaine (dépend de la décision actée en A) — la caser en dernier
  limite le risque de devoir défaire du travail si la décision change en cours de sprint.
- **H toute seule, après tout** : régénérer `player_scene.json` avant que toutes les phases Rust
  soient mergées produirait une resynchronisation partielle, à refaire. Une seule régénération,
  à la fin.

---

## 3. Détail par phase

### Phase A — Réconciliation (Étape 0 du doc de réflexion)
- **Objectif** : éliminer les deux écarts documentés en §2 bis.2 avant toute composition visuelle.
- **Tâches** :
  1. Relire `sync_embedded_scene_hameau_from_the_demo` et les tests de synchro pour trouver où le
     sol passe de 72 (calcul `mmorpg_demo()`) à 90 (`player_scene.json` servi).
  2. Décider explicitement : réintroduire le village/promontoire/rizières dans la scène servie, ou
     acter consciemment leur absence (et corriger les commentaires de `mmorpg_demo()` en
     conséquence).
  3. Consigner la décision dans `mapsJeuReflexionAnalyse.md` (mise à jour du §2 bis.2).
- **Fichiers** : `src/scene/mod.rs` (lecture), `src/scene/demos.rs` (écriture ciblée : la ligne du
  calcul du sol uniquement), `docs/mapsJeuReflexionAnalyse.md`.
- **Livrable vérifiable** : l'écart 72/90 est expliqué par écrit (avec la ligne de code exacte
  responsable) ; la décision village est actée noir sur blanc, pas laissée ouverte.
- **Risques** : aucun — travail de lecture/décision, pas de composition.

### Phase B — Composition Blender : habillage eau (`shore_*`)
- **Objectif** : habiller les 4 rects d'eau déjà cartographiés (§2 bis.3) avec les 20 assets
  `shore_*`, en jugeant à l'œil densité/occlusion.
- **Tâches** : suivre le §7 (checklist) du doc de réflexion, zone par zone (rivière nord, coude,
  lac, rivière sud), rendu/capture après chaque zone.
- **Fichiers** : aucun fichier source — uniquement `docs/blender/composition_eau.blend` (nouveau) et
  des captures dans `docs/blender/renders/` (nouveaux).
- **Livrable vérifiable** : captures des 4 zones habillées, `.blend` de référence versionné.
- **Risques** : piège rotation/scale des cônes (arbres/racines effilés), piège scale ×5,12 de
  `camera_to_view_selected` (§8 du doc de réflexion) — ne jamais cadrer toute la scène sélectionnée
  d'un coup.

### Phase C — Composition Blender : `grotto_*` sur la marge ouest
- **Objectif** : poser les 20 assets `grotto_*` sur la bande de relief ouest déjà sculptée (Phase K).
- **Tâches** : idem B, en intégrant visuellement avec le terrain heightmap existant (pas juste posé
  au-dessus, cf. §5 bis du doc de réflexion).
- **Fichiers** : `docs/blender/composition_grotto.blend`, `docs/blender/renders/grotto_*.png`
  (nouveaux uniquement).
- **Livrable vérifiable** : captures montrant l'intégration avec le relief réel, pas un aplat.
- **Risques** : mêmes pièges Blender qu'en B ; en plus, piège bind-pose si un asset animé est
  déplacé (peu probable ici, décor statique).

### Phase D — Vérification forêt NE / rizières SO / promontoire est
- **Objectif** : confirmer qu'aucun ajout des phases E/F/G ne cassera les zones d'exclusion
  existantes (`EXCL_EAU_ROUTES`, `EXCL_ZONES_AMENAGEES`, `EXCL_CLAIRIERES`).
- **Tâches** : relecture de `demos.rs` sur ces trois biomes, sans les refaire (§5 Étape 3 du doc de
  réflexion).
- **Fichiers** : `src/scene/demos.rs` (lecture ; écriture uniquement si un problème isolé est
  trouvé, auquel cas le documenter avant de patcher).
- **Livrable vérifiable** : note confirmant qu'aucune zone d'exclusion n'a besoin d'ajustement, ou
  liste des ajustements faits.
- **Risques** : piège solid_spots (§4.3 du doc de réflexion) si un patch est fait à la légère.

### Phase E — Traduction Rust : habillage eau
- **Objectif** : traduire la composition de B en appels `poser(...)` dans
  `Scene::hameau_gdd_demo()`.
- **Fichiers** : `src/scene/demos.rs` (écriture, zone eau uniquement).
- **Livrable vérifiable** : `cargo run` (éditeur), vérification visuelle en Play qu'aucune créature
  ne se fige contre un nouvel élément (§4.2.1).
- **Risques** : piège solid_spots, murs invisibles/ouvertures de pont déjà calibrés (ne pas y
  toucher, §5 Étape 1 du doc de réflexion).

### Phase F — Traduction Rust : `grotto_*`
- **Objectif** : idem E pour la composition de C.
- **Fichiers** : `src/scene/demos.rs` (écriture, zone relief ouest uniquement), repart de la version
  mergée par E.
- **Livrable vérifiable** : idem E, sur la zone grotte.
- **Risques** : idem E.

### Phase G — Réintroduction du village/promontoire/rizières (conditionnelle)
- **Objectif** : si la décision de A est « réintroduire », traduire en Rust avec vérification de
  non-chevauchement avec le fort (§2 bis.2 point 2 du doc de réflexion).
- **Fichiers** : `src/scene/demos.rs`, `src/scene/mod.rs` (si le test de synchro doit être étendu).
- **Livrable vérifiable** : capture Blender (ponctuelle, exclusive) confirmant l'absence de
  chevauchement, puis `cargo run` + Play sans régression.
- **Risques** : c'est le point le plus sensible du sprint (biome entier jamais posé) — prévoir plus
  de marge que E/F.

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

- [ ] Phase A — décision actée : _(à compléter)_
- [ ] Phase B — captures eau : _(à compléter)_
- [ ] Phase C — captures grotte : _(à compléter)_
- [ ] Phase D — biomes existants vérifiés : _(à compléter)_
- [ ] Phase E — eau traduite en Rust : _(à compléter)_
- [ ] Phase F — grotte traduite en Rust : _(à compléter)_
- [ ] Phase G — village (si retenu) : _(à compléter)_
- [ ] Phase H — `player_scene.json` régénéré, tests verts : _(à compléter)_
- [ ] Phase I — playtest final : _(à compléter)_
