# Réflexion — après les deux plans de sprints (`sprint10audit.md` + `sprintoptimation3daudit10h.md`)

Pense-bête pour la suite, pas un plan d'exécution. Révisé le 2026-07-18 : commandes et références
vérifiées directement dans le dépôt (CI, tests golden, protocole réseau, procédure de déploiement)
plutôt que supposées.

---

## ✅ Priorité immédiate — résolue (vérifié 18 juillet 2026)

**Incident original** : un client local (`PROTOCOL_VERSION = 5`) qui tentait de se connecter au
VPS recevait « version de protocole 5 incompatible (serveur : 2) — mettez le jeu à jour ». Le
serveur VPS tournait sur `PROTOCOL_VERSION = 2`, soit 3 versions de retard — aucun joueur ne
pouvait se connecter en prod.

**Statut vérifié** : `cargo run --example smoke_vps` contre `wss://ws.loicberthod.ch` et contre
`ws://179.237.71.235:80` répondent tous les deux `✅ Serveur VPS OK` (Welcome, snapshot de 27
entités, tir de projectile confirmé), sans aucun message d'incompatibilité — le VPS accepte
désormais des clients à `PROTOCOL_VERSION = 5`. Aucun commit de déploiement identifié dans
l'historique local pour expliquer *qui*/*quand* — à garder en tête pour la Phase O (hygiène
mémoire/process) de `sprintreflecion.md` si l'absence de traçabilité des déploiements devient
gênante. Détail complet : Section 4 (« Réseau : version de protocole & anti-triche »).

**Ce que ça change concrètement pour l'ordre des opérations** : le redéploiement du VPS n'est pas
une étape de fin de plan (Section 9, comme rangée initialement) — c'est la **toute première chose
à faire**, avant même de continuer le reste de ce document, et indépendamment du fait que les
Phases E/F/G/H/I de `sprintreflecion.md` soient complètement vertes ou non (le redéploiement du
VPS y est désormais sa propre Phase A, en tête du document). Un redéploiement avec
le code actuellement testé et vert (`cargo test`/`clippy`/`fmt` déjà confirmés propres) est
préférable à laisser la prod inaccessible en attendant que chaque phase du plan soit formellement
close.

---

## 1. Gate technique avant tout commit final

- [ ] `cargo fmt --all --check` et `cargo clippy --all-targets -- -D warnings` (exactement les
  commandes de `.github/workflows/ci.yml:36,39`) + `cargo test` complet — sur l'ensemble du dépôt,
  pas seulement les fichiers touchés par les deux plans.
- [ ] **Golden tests de rendu** : si la Phase B/D de `sprintoptimation3daudit10h.md` (instancing du
  skinning, LOD) a changé un shader ou le pipeline, régénérer sur worktree propre avec
  `UPDATE_GOLDEN=1 cargo test --test golden_render` et `--test golden_skinning`
  (`tests/golden_render.rs:11`, `tests/golden_skinning.rs:14`) — bisecter d'abord pour ne pas
  confondre une vraie régression avec un changement de rendu attendu.

## 2. Scène jouable : `mmorpg_demo` (code) vs `player_scene.json` (embarquée) — priorité haute

Tout le travail des deux plans (Vagues/Survie/Escorte/Boss, classes, archétypes, décor, culling,
LOD, compression) a été écrit et testé contre `Scene::mmorpg_demo()`
(`src/scene/demos.rs`) — mais ce n'est **pas** la scène que joue un vrai joueur. La scène
réellement servie (desktop `cargo run -- --player`, build web, APK, et tout ce qui se connecte
au VPS par défaut) est `assets/player_scene.json`, **embarquée à la compilation**
(`include_str!`, `Scene::embedded_player()`, `src/scene/demos.rs:8710-8715`) et chargée via
`AppState::load_embedded_player_scene()` (`src/app/demos.rs:98-101`, `src/lib.rs:735-738`). Les
deux scènes ne se synchronisent **jamais automatiquement** : il faut ouvrir `mmorpg_demo` dans
l'éditeur puis lancer un Export (`src/editor/export.rs:129`, `bundle_scene_json` réécrit
`assets/player_scene.json`), et reconstruire pour que `include_str!` prenne le nouveau contenu.
C'est une tension déjà connue et documentée (Phase G, Sprint 14 de `sprint10audit.md` : « décor
hameau + ménagerie animée : toujours seulement dans `mmorpg_demo`, pas dans la scène servie »).

**Conséquence directe et décision produit** : tant que cet export n'a pas été refait, **rien de ce
qui a été livré dans les deux plans de sprints n'est réellement jouable** par quelqu'un qui lance
`cargo run -- --player` ou un build déployé — tout existe en code + tests unitaires contre
`mmorpg_demo`, pas dans la carte réellement servie. Objectif désormais : **travailler la carte en
lien avec `player_scene.json`**, pas seulement enrichir `mmorpg_demo` en espérant l'exporter plus
tard.

- [ ] Avant tout playtest ou déploiement, ré-exporter `mmorpg_demo()` vers
  `assets/player_scene.json` (éditeur → Export) pour que le contenu des sprints (modes de manche,
  classes, décor, archétypes) soit effectivement présent dans la scène jouable.
- [ ] Vérifier après export avec `cargo run -- --player` (README.md:450) — c'est la commande qui
  lance le mode joueur sur `player_scene.json`, donc le test de vérité pour tout ce qui a été
  construit dans les deux plans — que les nouveaux modes (Survie/Escorte/Boss), le sélecteur de
  classe et le décor sont bien là.
- [ ] Traiter la synchronisation `mmorpg_demo` → `player_scene.json` comme une étape récurrente du
  flux de travail désormais, pas comme un export ponctuel oublié en fin de sprint.

## 3. Démarrer une vraie partie à plusieurs instances : serveur, salon, mode

Comment lancer concrètement une session multijoueur en local (deux instances de l'éditeur/du
player sur la même machine) pour playtester (Section 7) — vérifié directement dans le code, pas
supposé.

**Démarrer le serveur** : `cargo run --bin server`, écoute par défaut sur `127.0.0.1:7777`
(`DEFAULT_ADDR`, `src/bin/server.rs:54`), surchargeable via la variable d'environnement
`RUSTEEGEAR_SERVER_ADDR` — utile pour lancer plusieurs serveurs de test en parallèle sur des ports
différents. Une seule instance de serveur suffit pour plusieurs clients.

**Connecter deux instances** : lancer deux instances de l'éditeur (`cargo run`) ou du mode player
(`cargo run -- --player`), ouvrir la fenêtre « 🌐 Multijoueur » dans chacune, entrer la même
adresse (`ws://127.0.0.1:7777`) et cliquer « ▶ Se connecter ». **Elles atterrissent
automatiquement dans le même salon** — pas besoin de code à saisir, mais aussi **pas de choix
possible** : voir le point suivant.

**⚠️ Piège découvert en lisant le code — deux notions de « Salon » coexistent sous le même nom :**
- Le champ **« Salon »** visible dans la fenêtre Multijoueur (`src/editor/windows.rs:801-802`,
  `lobby_code`) ne contrôle **que le chat Firebase** (texte, indépendant du jeu) — il n'est même
  affiché que si un compte Firebase est configuré.
- Le **vrai salon de jeu** (`ClientMsg::Join::lobby`, ce qui route les joueurs dans la même
  `Room` côté serveur, `src/net/protocol.rs:52-56`) n'a **aucun champ dans l'UI** :
  `connect_to_server_as` (`src/app/network_client.rs:212-241`) envoie toujours
  `protocol::DEFAULT_LOBBY` (`"default"`, `src/net/protocol.rs:39`), codé en dur. L'API
  bas niveau (`NetClient::connect_to_lobby`) accepte pourtant déjà un code de salon arbitraire
  (32 caractères max, `MAX_LOBBY_LEN`) — elle n'est simplement appelée qu'avec le défaut partout
  dans l'éditeur.
- **Conséquence pratique aujourd'hui** : impossible de séparer deux parties de test en isolant
  chacune dans son propre salon depuis l'UI — tout le monde qui se connecte au même serveur
  atterrit dans `"default"`. Pour tester plusieurs parties isolées en parallèle sur le même
  serveur, il faut soit lancer un second processus serveur sur un autre port, soit ajouter un
  champ « Code de salon » à `multiplayer_window` (câblage trivial : le paramètre existe déjà côté
  `connect_to_lobby`, juste pas exposé).

**Second piège lié, même symptôme** : le **mode de manche** (`RoundObjective` — Vagues/Survie/
Escorte/Boss, Phase C de `sprint10audit.md`, toute la logique déjà livrée côté serveur) n'a lui
non plus **aucun sélecteur en UI réseau**. Chaque site d'appel de `connect_to_lobby`
(`src/net/client/native.rs:76-78`, `src/net/client/web.rs`, `src/app/network_client.rs`) envoie
`objective: 0` (Vagues) en dur, avec le commentaire explicite « pas encore câblée à une UI ».
`Lobby::objective` (`src/bin/server.rs`) est fixé par le **premier** `Join` reçu par un salon vide
— donc même si un futur champ de salon permettait d'isoler une partie, il faudrait aussi un
sélecteur de mode pour que ce premier `Join` porte autre chose que Vagues. Les démos solo
(`Scene::escorte_demo()`, `Scene::boss_demo()`, menu Fichier → Démos) restent aujourd'hui le
**seul** moyen de jouer ces modes — pas accessibles en multijoueur réseau tant que ce câblage
n'existe pas.

- [ ] Pour playtester Survie/Escorte/Boss à plusieurs (Section 7, Sprint 14 de
  `sprintreflecion.md`) **avant** qu'un sélecteur de mode réseau existe : soit lancer les démos
  solo pour valider la logique de manche indépendamment du réseau, soit ajouter le câblage minimal
  (un `egui::ComboBox` de plus dans `multiplayer_window`, sur le modèle exact du sélecteur de
  classe déjà là) avant de considérer le playtest multijoueur de ces modes possible.
- [ ] Si le besoin de plusieurs parties isolées en parallèle se confirme, ajouter un champ « Code
  de salon » à `multiplayer_window` — le paramètre existe déjà de bout en bout
  (`connect_to_lobby`/`Room`/`HashMap<String, Room>`), il ne manque qu'un `TextEdit` de plus,
  distinct du champ « Salon » du chat pour ne pas perpétuer la confusion de nommage actuelle.

## 4. Menu pause/paramètres avec redémarrer (y compris si le joueur tombe)

**État vérifié dans le code** : un mécanisme de restart existe déjà —
`AppState::restart_game()` (`src/app/persistence.rs:16-36`, restaure `play_snapshot`, vide
joueurs réseau/boules de feu/tirs de créature, remet `time`/`win_time` à zéro) est câblé à un
bouton **« 🔄 Rejouer »** (`restart_button`, `src/editor/hud.rs:880-900`) qui n'apparaît
qu'**après coup**, sur la bannière de victoire/défaite (`self.lost`, déclenché entre autres par
une zone mortelle — cf. `tower_demo_lava_style_void_kills_a_falling_player`,
`src/scene/mod.rs:1816`). **Il n'existe en revanche aucun menu pause/paramètres accessible à la
demande pendant la partie** (pas de touche Échap ni d'overlay équivalent trouvés dans
`run_player_overlay`, `src/editor/mod.rs:484`) — en mode Player (le build que joue réellement un
joueur, cf. Section 2), il n'y a que le HUD de jeu, sans moyen de mettre en pause, ajuster un
réglage ou redémarrer volontairement avant d'être mort/tombé.

**Ce qui manque concrètement** :
- Un menu pause accessible à tout moment (touche dédiée, ex. Échap), qui met le jeu en pause
  (ou au moins masque le HUD de combat) sans nécessiter une défaite.
- Dans ce menu : au minimum un bouton **Reprendre** (ferme le menu, aucun changement d'état) et un
  bouton **Redémarrer** qui réutilise `restart_game()` déjà existant — pas besoin de réinventer la
  logique de restauration, seulement de l'exposer ailleurs qu'après une mort.
- Le cas « le joueur tombe » (zone mortelle/void) déclenche déjà `self.lost` → bannière → bouton
  Rejouer (chemin existant, fonctionnel) ; le menu pause est un **chemin volontaire**
  supplémentaire, pas un remplacement de ce filet de sécurité automatique.

- [ ] Ajouter un état de pause à `AppState` (ex. `paused: bool`), déclenché par une touche dédiée
  en mode Play/Player, qui suspend la simulation (comme le fait déjà `is_room_lost`/`win_time`
  pour la fin de manche, sur le même principe de gel).
- [ ] Overlay de menu pause (nouveau panneau HUD, sur le modèle de `defeated_banner`/
  `restart_button`) avec au moins Reprendre et Redémarrer (réutilise `restart_game()`).
- [ ] Vérifier que rouvrir/fermer ce menu ne casse pas la reconnexion réseau en cours ni les
  timers de manche (Survie a un chrono de 180 s, `AppState::update_survie` — la pause ne doit pas
  laisser ce chrono continuer à courir pendant que le joueur est dans un menu, sous peine de finir
  la manche pendant une pause).

## 5. Terrain avec relief réel (herbe/sol partout, pentes, collines, tunnels, creux, lacs)

**État vérifié dans le code** : `MeshKind::Terrain` (`src/scene/mod.rs:107,119`) génère via
`mesh::terrain()` (`src/gfx/mesh.rs:387-421`) une **grille 24×24 unique** avec un relief
procédural analytique très léger (`AMP = 0.08`, une onde sinusoïdale de faible amplitude,
normales calculées analytiquement) — c'est un **seul objet primitif à l'échelle voulue via son
Transform**, pas un système de terrain continu couvrant toute la carte. Le sol de `mmorpg_demo`
est aujourd'hui composé de décor scatterné (herbe/fougères/rochers, cf.
`optimisation3D.Analys.md`) posé sur ce qui semble être des plans/quelques instances de Terrain,
pas d'un vrai maillage de hauteur (heightmap) couvrant l'ensemble de la zone. Les lacs existent
déjà comme éléments décoratifs isolés (`mmorpg_water_is_sealed_all_the_way_around...`,
`mmorpg_creature_13_drifts_in_its_lake_without_getting_stuck`, `src/scene/demos.rs`/
`src/scene/mod.rs`) — pas intégrés à un relief global. **Aucune trace de tunnels, grottes,
creux/fosses, ou pentes/collines à grande échelle** dans `src/scene/mod.rs`/`MeshKind` (les
primitives disponibles restent `Cube/Sphere/Plane/Cylinder/Capsule/Terrain/Billboard`, cf. Section
3 de `sprintreflecion.md` sur l'absence de primitive billboard avant son ajout récent — même
limite de fond : pas de géométrie de terrain paramétrable par le design).

**Objectif produit exprimé** : une carte où le sol (herbe) est continu partout (pas de trous/zones
nues), avec un vrai relief exploitable en gameplay — pentes, collines, tunnels, creux, lacs
intégrés au terrain plutôt que posés dessus.

- [ ] Étendre `mesh::terrain()` (ou créer un nouveau générateur dédié) pour produire un maillage de
  hauteur paramétrable (taille de grille configurable, fonction de hauteur par bruit procédural
  ou heightmap chargée), plutôt qu'une grille 24×24 à amplitude fixe.
- [ ] Couverture d'herbe continue : soit une texture/couleur de sol appliquée sur tout le maillage
  de terrain, soit un scatter d'herbe densifié pour qu'aucune zone ne reste nue — à trancher selon
  le budget de perf (cf. `optimisation3D.Analys.md` §3 LOD, déjà vigilant sur le fill-rate du
  feuillage dense).
- [ ] Pentes/collines : variation d'amplitude et de fréquence du relief par zone (pas un bruit
  uniforme partout), pour créer des zones distinctes plutôt qu'un terrain homogène.
- [ ] Creux/fosses et tunnels : nécessitent une géométrie non-heightmap à certains endroits (un
  heightmap classique ne peut pas représenter un surplomb/tunnel) — probablement des meshes
  d'ouverture insérés manuellement dans le terrain plutôt qu'une fonction de hauteur pure ; à
  concevoir comme une extension, pas comme une réécriture du système de heightmap.
- [ ] Lacs intégrés au relief : le niveau d'eau doit correspondre à un creux réel du terrain (pas
  un plan d'eau posé au-dessus d'un sol plat comme aujourd'hui) pour que la rive suive
  naturellement la pente.
- [ ] Collision/physique : vérifier que le nouveau relief reste un obstacle réel pour les sondes
  IA et les joueurs (piège déjà documenté en mémoire projet — tout obstacle doit être visible au
  raycast à 0,6 m, sinon patrouille figée) — un terrain à relief prononcé change la hauteur de sol
  sous les créatures/joueurs, pas seulement l'esthétique.

## 6. Réseau : version de protocole & anti-triche

> **✅ Incident résolu, vérifié le 18 juillet 2026** : un client lancé localement
> (`PROTOCOL_VERSION = 5`, `src/net/protocol.rs:35`) recevait « version de protocole 5
> incompatible (serveur : 2) — mettez le jeu à jour » en tentant de se connecter au VPS (écart de
> 3 versions, probablement dû aux bumps successifs des Phases A/B/C de `sprint10audit.md` : classe,
> mode de manche, contrat du jour). **Revérifié via `cargo run --example smoke_vps`** contre
> `wss://ws.loicberthod.ch` et `ws://179.237.71.235:80` : les deux répondent `✅ Serveur VPS OK`
> sans message d'incompatibilité — le VPS accepte désormais `PROTOCOL_VERSION = 5`. Pas de commit
> de déploiement retrouvé dans l'historique local pour dater/attribuer précisément le correctif.
>
> **Reste valable pour la suite** : garder la discipline ci-dessous (redéploiement couplé à chaque
> bump de `PROTOCOL_VERSION`) pour éviter qu'un écart similaire ne se reproduise.

- [x] **VPS redéployé** (cf. Section 9, [[vps-deploy-procedure]]) — écart `PROTOCOL_VERSION` 2 → 5
  résorbé, vérifié en conditions réelles ci-dessus.
- [ ] `PROTOCOL_VERSION` (`src/net/protocol.rs:35`, actuellement `5`) doit monter à chaque champ
  ajouté au protocole par les phases gameplay (classe, mode de manche, contrat du jour) — un client
  ancien ne doit jamais se connecter silencieusement à un serveur incompatible. Le déploiement doit
  rester couplé client/VPS sur ce point.
- [ ] Tout nouveau champ réseau doit suivre la discipline déjà en place pour `PlayerClass`
  (`from_u8` avec repli sur Assaut plutôt qu'un panic) : un client malveillant ne doit jamais
  pouvoir envoyer une valeur qui fait paniquer le serveur.
- [ ] Revue de sécurité ciblée (`/security-review` ou lecture manuelle) avant déploiement VPS, en
  particulier si des champs texte libres ont été ajoutés (chat, salon).

## 7. Capacité skinnée en conditions réelles

- [ ] Re-tester le scénario de vue large/plongée sur `mmorpg_demo` (celui qui a historiquement fait
  déborder `MAX_SKINNED_INSTANCES`) après *toutes* les phases d'optimisation d'un coup, pas
  seulement après chacune isolément — l'effet cumulé peut différer de la somme des effets
  individuels.
- [ ] Revalider la marge choisie en Phase A avec le contenu *réellement* livré : si la Phase E/G de
  `sprint10audit.md` (archétypes de créatures, décor) ajoute encore des objets skinnés après coup,
  la capacité peut redevenir insuffisante — ce chiffre a déjà été relevé 3 fois par le passé pour
  cette même raison.

## 8. Coordination des sessions concurrentes

- [ ] Les deux plans partagent des fichiers (`src/scene/demos.rs` notamment, entre archétypes de
  créatures et catégorisation du décor animé) — avant de merger, `git status` + mtimes récents pour
  vérifier qu'aucune session concurrente n'a un travail non commité qui se chevauche.
- [ ] Ne pas committer par-dessus un état de build cassé par une autre session en cours ; attendre
  la fin ou coordonner explicitement l'ordre des commits.

## 9. Documentation à resynchroniser

- [ ] `GDD_MMORPG.md` §14 : la Phase G de `sprint10audit.md` le couvre déjà (XP, roster, audio en
  avance sur ce que dit le GDD) — vérifier qu'elle a bien été faite, pas oubliée en route.
- [ ] `optimisation3D.Analys.md` : la Phase F de `sprintoptimation3daudit10h.md` prévoit sa mise à
  jour avec les chiffres avant/après réels — s'assurer qu'elle n'est pas restée à l'état de
  recommandation théorique.
- [ ] Décider si les deux plans de sprints restent des documents satellites ou sont rattachés à la
  numérotation officielle de `ROADMAP_SPRINTS.md`.
- [ ] Le dépôt a plusieurs anciens fichiers d'audit supprimés (`AUDIT.md`, `AUDIT_MMORPG.md`,
  `HANDOFF.md`...) — vérifier qu'aucune information utile n'a été perdue sans être reprise ailleurs
  avant de confirmer leur suppression définitive.

## 10. Playtest réel, pas seulement des tests unitaires

- [ ] Les tests couvrent la logique (`health.rs`, XP, décodage réseau...) mais l'équilibrage
  (feedback de dégâts, seuils de culling par distance, transition de LOD) ne se juge qu'en jouant
  réellement, à plusieurs, latence réseau incluse.
- [ ] Session multijoueur à plusieurs clients réels (au-delà de `examples/load_test_client.rs`,
  scripté) sur les nouveaux modes de manche (Survie/Escorte/Boss) avant de les considérer prêts —
  **bloqué tant que le sélecteur de mode réseau de la Section 3 n'existe pas** : Escorte/Boss ne
  sont aujourd'hui jouables qu'en solo via les démos dédiées, pas en salon réseau.
- [ ] Jouer via `cargo run -- --player` sur `player_scene.json` **après** l'export de la Section 2
  — un playtest en mode Play dans l'éditeur sur `mmorpg_demo` ne prouve pas que le contenu est
  réellement dans la carte servie aux joueurs.

## 11. Déploiement

- [ ] **Préalable bloquant** : confirmer que `assets/player_scene.json` a bien été ré-exporté
  depuis `mmorpg_demo` (Section 2) avant de déployer — sinon le VPS sert une carte qui n'a aucun
  des modes de manche/classes/décor livrés par les deux plans, malgré un code source à jour.
- [ ] Suivre la procédure existante : push GitHub → pull + build release sur le VPS → restart du
  service systemd → `examples/smoke_vps.rs`. Les builds « player » (desktop et Android) se
  connectent automatiquement au serveur VPS au lancement (README.md:376-380) — un déploiement
  cassé impacte donc immédiatement tous les joueurs, pas seulement les nouvelles connexions.
- [ ] Déployer d'abord l'optimisation 3D (pas de changement de protocole, risque faible) séparément
  du gameplay réseau (changement de protocole, risque plus élevé) pour isoler la source d'un
  problème éventuel en prod.

## 12. Prochaines étapes produit

- [ ] Une fois Survie/Escorte/Boss livrés (Phase C de `sprint10audit.md`), le **Contrat du jour**
  (Phase D) devient possible — c'est la pièce qui donne un objectif quotidien rejouable au jeu, à
  ne pas laisser traîner.
- [ ] Réévaluer le périmètre volontairement exclu du GDD (artisanat/économie/guildes) une fois le
  cœur de boucle (modes de manche, classes, contrats) stable — pas un manque aujourd'hui, mais la
  discussion produit logique suivante.
- [ ] Resituer ces deux plans d'audit dans les phases K→S de `ROADMAP_SPRINTS.md` (filet de
  sécurité, animation, image, audio/HUD, web) plutôt que de les traiter comme un travail isolé.

## 13. Nettoyer le `README.md` — en tout dernier, une fois tout le reste stabilisé

**Pourquoi en dernier et pas avant** : le README documente l'état du jeu pour un lecteur externe
(GitHub, contributeur, joueur curieux) — le nettoyer avant que les phases A→O soient réellement
livrées et déployées produirait un document qui redevient faux dès le sprint suivant. Ce nettoyage
n'a de sens qu'une fois Phase M (déploiement) faite, pour décrire ce qui est **réellement en ligne**
plutôt qu'un état intermédiaire.

**État constaté (déjà périmé sur plusieurs points, vérifié en lisant le fichier)** :
- § « Limites connues » (`README.md:384-395`) liste encore « Pas de rôles/classes » — faux depuis
  la Phase A de `sprint10audit.md` (sélecteur de classe livré) — et « Pas de sélection de salon
  dans l'UI » — qui reste vrai aujourd'hui, mais deviendra faux si la Phase I de ce plan est
  livrée : cette ligne doit être retirée seulement à ce moment-là, pas avant.
- § « Multijoueur en ligne (chantier en cours) » (`README.md:237`) qualifie tout le multijoueur de
  chantier en cours, alors qu'une bonne partie (assists, modes de manche, roster, chat, mute) est
  désormais livrée et testée — le titre et l'intro de cette section méritent une relecture, pas
  seulement les sous-listes.
- § « Fonctionnalités (disponibles aujourd'hui) » (`README.md:163`) ne mentionne pas encore les
  modes Survie/Escorte/Boss, le contrat du jour, les archétypes de créatures, ni (si livrées) les
  Phases J/K de ce plan (menu pause, terrain à relief) — à compléter, pas à réécrire en bloc.
- § « La suite — analyse & sprints » (`README.md:530`) référence probablement encore l'ancienne
  structure de sprints ; vérifier qu'elle pointe vers les documents actuels
  (`auditGDD10h.md`/`sprint10audit.md`/`optimisation3D.Analys.md`/`sprintoptimation3daudit10h.md`/
  `reflexion.md`/`sprintreflecion.md`) plutôt que vers des fichiers déjà supprimés (cf. Section 9).

- [ ] Ne pas commencer ce nettoyage avant que les Phases L et M de ce plan soient vertes — sinon
  il faudra le refaire.
- [ ] Relire section par section (pas une réécriture globale) : Limites connues, Multijoueur en
  ligne, Fonctionnalités disponibles, La suite — analyse & sprints — chacune contre l'état réel du
  code au moment du nettoyage, avec la même discipline de vérification que ce document (grep le
  code avant d'affirmer qu'une limite existe encore ou qu'une fonctionnalité manque).
- [ ] Vérifier qu'aucun lien mort ne subsiste vers les anciens fichiers d'audit déjà supprimés
  (Section 9) — le README a pu en garder des références que la Section 9 n'a pas couvertes (portée
  limitée à `GDD_MMORPG.md` à l'origine).

## 14. Hygiène de mémoire/process

- [ ] Une fois les deux plans terminés, passer une revue de mémoire pour capturer ce qui a été
  surprenant pendant l'exécution (ex. `MAX_SKINNED_INSTANCES` relevé une 4e fois, ou un mode de
  manche révélant un piège d'IA non documenté) — les plans eux-mêmes ne sont pas de la mémoire long
  terme, leurs leçons opérationnelles le sont.
