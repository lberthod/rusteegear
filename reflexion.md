# Réflexion — après les deux plans de sprints (`sprint10audit.md` + `sprintoptimation3daudit10h.md`)

Pense-bête pour la suite, pas un plan d'exécution. Révisé le 2026-07-18 : commandes et références
vérifiées directement dans le dépôt (CI, tests golden, protocole réseau, procédure de déploiement)
plutôt que supposées.

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
C'est une tension déjà connue et documentée (`sprintG10haudit.md` : « décor hameau + ménagerie
animée : toujours seulement dans `mmorpg_demo`, pas dans la scène servie »).

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

## 4. Réseau : version de protocole & anti-triche

- [ ] `PROTOCOL_VERSION` (`src/net/protocol.rs:29`, actuellement `4`) doit monter à chaque champ
  ajouté au protocole par les phases gameplay (classe, mode de manche, contrat du jour) — un client
  ancien ne doit jamais se connecter silencieusement à un serveur incompatible. Le déploiement doit
  rester couplé client/VPS sur ce point.
- [ ] Tout nouveau champ réseau doit suivre la discipline déjà en place pour `PlayerClass`
  (`from_u8` avec repli sur Assaut plutôt qu'un panic) : un client malveillant ne doit jamais
  pouvoir envoyer une valeur qui fait paniquer le serveur.
- [ ] Revue de sécurité ciblée (`/security-review` ou lecture manuelle) avant déploiement VPS, en
  particulier si des champs texte libres ont été ajoutés (chat, salon).

## 5. Capacité skinnée en conditions réelles

- [ ] Re-tester le scénario de vue large/plongée sur `mmorpg_demo` (celui qui a historiquement fait
  déborder `MAX_SKINNED_INSTANCES`) après *toutes* les phases d'optimisation d'un coup, pas
  seulement après chacune isolément — l'effet cumulé peut différer de la somme des effets
  individuels.
- [ ] Revalider la marge choisie en Phase A avec le contenu *réellement* livré : si la Phase E/G de
  `sprint10audit.md` (archétypes de créatures, décor) ajoute encore des objets skinnés après coup,
  la capacité peut redevenir insuffisante — ce chiffre a déjà été relevé 3 fois par le passé pour
  cette même raison.

## 6. Coordination des sessions concurrentes

- [ ] Les deux plans partagent des fichiers (`src/scene/demos.rs` notamment, entre archétypes de
  créatures et catégorisation du décor animé) — avant de merger, `git status` + mtimes récents pour
  vérifier qu'aucune session concurrente n'a un travail non commité qui se chevauche.
- [ ] Ne pas committer par-dessus un état de build cassé par une autre session en cours ; attendre
  la fin ou coordonner explicitement l'ordre des commits.

## 7. Documentation à resynchroniser

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

## 8. Playtest réel, pas seulement des tests unitaires

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

## 9. Déploiement

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

## 10. Prochaines étapes produit

- [ ] Une fois Survie/Escorte/Boss livrés (Phase C de `sprint10audit.md`), le **Contrat du jour**
  (Phase D) devient possible — c'est la pièce qui donne un objectif quotidien rejouable au jeu, à
  ne pas laisser traîner.
- [ ] Réévaluer le périmètre volontairement exclu du GDD (artisanat/économie/guildes) une fois le
  cœur de boucle (modes de manche, classes, contrats) stable — pas un manque aujourd'hui, mais la
  discussion produit logique suivante.
- [ ] Resituer ces deux plans d'audit dans les phases K→S de `ROADMAP_SPRINTS.md` (filet de
  sécurité, animation, image, audio/HUD, web) plutôt que de les traiter comme un travail isolé.

## 11. Hygiène de mémoire/process

- [ ] Une fois les deux plans terminés, passer une revue de mémoire pour capturer ce qui a été
  surprenant pendant l'exécution (ex. `MAX_SKINNED_INSTANCES` relevé une 4e fois, ou un mode de
  manche révélant un piège d'IA non documenté) — les plans eux-mêmes ne sont pas de la mémoire long
  terme, leurs leçons opérationnelles le sont.
