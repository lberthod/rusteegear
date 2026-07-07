# Audit du chantier multijoueur — RusteeGear (`motor3derust`)

> Audit réalisé le 7 juillet 2026, sur la branche `main` (dernier commit
> `1a0c0b4`, Sprint 61), dépôt propre (aucune modification locale non commitée
> au démarrage de l'audit). Porte spécifiquement sur le code réseau/Firebase
> ajouté par les Sprints 50→61 (~3 100 lignes neuves : `src/net/`,
> `src/app/multiplayer.rs`, `src/app/combat.rs`, `src/bin/server.rs`,
> `examples/load_test_client.rs`). Ne recouvre pas [AUDIT.md](AUDIT.md), qui
> porte sur le moteur solo.

---

## 1. Contexte et scope revendiqué

Le chantier suit [SPRINT_MMORPG.md](SPRINT_MMORPG.md) : salons de 2 à 16
joueurs (pas un MMO à monde persistant), serveur de jeu Rust autoritaire, et
Firebase Realtime Database en **backend annexe uniquement** (comptes, chat,
classement — jamais le transport temps réel du gameplay). Ce scope est
cohérent et respecté dans le code : `net::protocol`/`net::server_loop` portent
le gameplay (WebSocket + bincode), `net::firebase` ne porte que des données à
faible fréquence.

Point de méthode notable, et assumé explicitement dans la doc de sprint à
chaque étape concernée : l'environnement d'exécution utilisé pour développer
ce chantier n'a pas d'affichage graphique. Toute l'UI prévue (écran de
connexion, lobby, chat, classement en jeu) a donc été **reportée**, au profit
d'un backend testé automatiquement. C'est une décision défendable, mais elle
signifie que **rien de ce chantier n'a été vu tourner dans une fenêtre** — la
seule validation est le test automatisé et la relecture de code.

## 2. Ce qui a été vérifié pour cet audit

- `cargo test` (lib + bin) : **134 tests lib + 2 tests bin, tous verts**
  (dont 1 correctif apporté pendant cet audit, cf. §4).
- `cargo clippy --all-targets -- -D warnings` : propre.
- `cargo fmt --check` : propre.
- `cargo run --bin server` (sans client) : tourne, se termine proprement.
- Relecture ligne à ligne de `src/net/*.rs`, `src/app/multiplayer.rs`,
  `src/app/combat.rs`, `src/bin/server.rs`, `examples/load_test_client.rs`.
- Recoupement des affirmations de `SPRINT_MMORPG.md` avec le code réel (pas
  pris pour argent comptant).

**Non vérifié** (déjà documenté comme tel dans le code/les sprints, rappelé
ici pour ne pas laisser d'ambiguïté) :
- Le flux Firebase complet contre un **vrai projet** (Auth, RTDB, règles de
  sécurité) — aucun projet Firebase disponible dans cet environnement.
- Tout ce qui touche à une fenêtre réelle (`winit`/`egui`) : le client réseau
  n'est branché nulle part dans l'éditeur desktop.
- Comportement sous charge **réseau réelle** (latence/perte de paquets) : le
  Sprint 61 mesure le coût CPU/bande passante sur une seule machine (latence
  quasi nulle), pas l'effet d'un vrai aller-retour réseau.

## 3. Ce qui est solide

- **Séparation nette** : `net::protocol` (types + sérialisation, zéro I/O,
  compilé même sur mobile) vs `net::client`/`server_loop`/`firebase`
  (desktop-only, gatés comme `rfd`/`ureq` — vérifié dans `net/mod.rs` et
  `Cargo.toml`, cohérent avec la contrainte CI cross-build mobile).
- **`AppState` ne connaît jamais `tokio`** : `NetServer`/`NetClient` exposent
  des canaux `std::sync::mpsc` synchrones, sur le même principe que les
  imports glTF/requêtes IA déjà existants. Le serveur headless
  (`src/bin/server.rs`) réutilise *exactement* la même simulation de jeu que
  l'éditeur desktop (aucune logique dupliquée) — c'est un vrai bénéfice de
  l'extraction `combat.rs`/`multiplayer.rs` (Sprints 50/55).
- **Anti-triche pris au sérieux, pas cosmétique** : le Sprint 60 a trouvé un
  vrai bug (`NaN` réseau non filtré par `f32::clamp`) et l'a corrigé avec un
  test de non-régression. Le cooldown d'attaque réseau est validé côté
  serveur, pas seulement affiché côté client — testé explicitement contre un
  client qui spamme `attack: true`.
- **Mesures de charge réelles, pas des estimations** : le Sprint 61 fait
  tourner 16 vrais `NetClient` contre un vrai `NetServer` et mesure des octets
  réellement encodés, pas une taille de struct calculée à la main. Les
  résultats (~0,4 ms/tick, 368 octets/snapshot pour 16 joueurs) laissent une
  marge confortable.
- **Sécurité Firebase documentée avant d'être nécessaire** : le commentaire
  « Qui écrit la progression ? » (`net::firebase`) explique clairement pourquoi
  le client ne doit jamais écrire sa propre XP/son propre score, et donne un
  exemple concret de règles RTDB — ce n'est pas laissé à deviner.
- **Limites connues signalées dans le code**, pas seulement dans la doc de
  sprint : présence par heartbeat (pas de vrai `onDisconnect` en REST),
  snapshot sans les monstres, IA à cible unique, santé non individualisée par
  joueur. Un futur contributeur qui lit le code tombe sur ces limites sans
  avoir à recouper `SPRINT_MMORPG.md`.

## 4. Anomalies trouvées pendant cet audit

### 4.1 🔴 Corrigée pendant cet audit — objet fantôme sur double `Join`

**Constat** : `net::server_loop::handle_connection` ne borne que la
**première** trame reçue à être un `ClientMsg::Join` (`server_loop.rs:129-136`).
Toute trame suivante — y compris un second `Join` — passe par la boucle
générique `inbound` et est relayée telle quelle au thread principal
(`server_loop.rs:169-179`). Rien côté `AppState` ne rejetait un second `Join`
du même `PlayerId` : `spawn_network_player` faisait
`self.network_players.insert(id, index)`, qui **écrase** silencieusement
l'ancien mapping sans jamais nettoyer l'ancien objet.

**Conséquence** : un client qui renvoie deux fois `Join` (rejeu réseau, bug de
reconnexion côté client, ou trame forgée) laissait un premier objet
« fantôme » : retiré de `network_players` donc invisible du `Snapshot` réseau,
mais toujours un corps rigide simulé par la physique **indéfiniment**, et
chaque spawn en trop reconstruisait toute la physique de la scène (coût qui
grandit avec le nombre d'objets déjà présents — un vecteur de dégradation
progressive si répété).

**Corrigé** : `spawn_network_player` est maintenant idempotent (retourne
l'objet existant si `id` est déjà connu), avec un test de régression
(`spawning_twice_for_the_same_player_reuses_the_existing_object`,
`src/app/multiplayer.rs`). Vérifié : 132/132 tests lib verts après correctif,
clippy/fmt propres.

### 4.2 🟢 Corrigée après cet audit — indices réseau non réinitialisés à un restart

**Constat** : `AppState::restart_game()` (`app/mod.rs:1049`) et la transition
Play→Edit dans `advance_play` (`app/mod.rs:1719`) remettent
`self.scene.objects` à l'état de `play_snapshot` (capturé **avant** que le
moindre joueur réseau n'ait rejoint, puisque `spawn_network_player` n'est
appelé qu'en cours de Play) — mais ne touchaient ni `network_players`, ni
`network_inputs`, ni `network_attack_cooldowns`.

**Conséquence potentielle** : si ces chemins sont un jour empruntés avec des
joueurs réseau connectés, `network_players` continuerait de pointer vers des
indices qui, après restauration, correspondent à d'autres objets (ou
n'existent plus) — un joueur réseau pourrait se retrouver à piloter un objet
qui n'est plus le sien, ou une erreur silencieuse (indices hors bornes filtrés
par les `.get()` existants, donc pas de panique, mais un état incohérent).

**Corrigé** (après la rédaction initiale de cet audit) : nouvelle méthode
`AppState::clear_network_players()` (`app/multiplayer.rs`), appelée aux deux
points de reset (`restart_game`, transition Play→Edit) — elle ne fait
qu'oublier la table de correspondance côté serveur (pas de notification
`PlayerLeft` aux clients : c'est un remaniement de conception plus large,
resté hors scope, cf. la doc de la méthode). Test de régression
(`restart_game_forgets_network_players`) : un joueur réseau spawné puis un
`restart_game()` ne laisse plus d'indice obsolète. Cette échéance était
**actuellement inatteignable en pratique** (le serveur headless ne boucle pas
sur plusieurs manches et n'appelle jamais `restart_game()`), mais corriger
maintenant coûtait peu et évite d'oublier ce point si un Sprint futur fait
boucler le serveur sur plusieurs manches.

### 4.3 🟢 Corrigée après cet audit — un runtime tokio multi-thread complet par connexion

**Constat** : `NetServer::start` et `NetClient::connect` utilisent tous deux
`tokio::runtime::Runtime::new()` (`server_loop.rs:46`, `client.rs:38`), qui
construit par défaut un runtime **multi-thread** (un thread ouvrier par CPU
logique). Sur la machine de cet audit (10 CPU logiques), le test de charge du
Sprint 61 — 1 `NetServer` + 16 `NetClient` — instancie donc **17 runtimes
multi-thread complets**, soit potentiellement plus de 150 threads OS pour un
besoin qui est fondamentalement de l'attente réseau sur une poignée de
connexions (peu de parallélisme CPU réel requis).

**Conséquence** : pas un bug de correction (les 132+2 tests passent, le test
de charge tourne), mais un vrai gaspillage de ressources — significatif sur un
hébergement modeste (Sprint 62) où un runtime `current_thread` par connexion
suffirait largement à ce volume (2-16 joueurs).

**Corrigé** (après la rédaction initiale de cet audit) : `NetServer::start` et
`NetClient::connect` construisent désormais un runtime `tokio::runtime::
Builder::new_current_thread()`, chacun `block_on`é en continu par un thread OS
dédié (spawné explicitement) plutôt que délégué aux threads ouvriers internes
d'un runtime multi-thread. Vérifié : 132 tests lib + 2 tests bin toujours
verts (aucune régression sur le code réseau déjà testé), et mesuré
concrètement — le test de charge du Sprint 61 (1 `NetServer` + 16
`NetClient`) tourne désormais avec **30 threads OS au total** pour le
processus (contre plus de 150 estimés avant, cf. le constat ci-dessus), avec
les mêmes chiffres de performance (~0,4 ms/tick, 368 octets/snapshot).

### 4.4 🟡 Limite déjà connue, reconfirmée — jeton Firebase serveur non renouvelé

**Constat** : `connect_firebase_server()` (`src/bin/server.rs`) se connecte
**une fois** au démarrage du processus et réutilise l'`id_token` obtenu pour
tous les appels `set_progress`/`post_leaderboard_entry` de toute la durée de
vie du processus. Les jetons Firebase Auth expirent après environ une heure.

**Pourquoi ce n'est pas un problème aujourd'hui** : le serveur headless ne
tourne qu'une seule manche par processus (quelques minutes maximum,
cf. §4.2) — largement sous le délai d'expiration. **Deviendrait un problème
réel** si le serveur est un jour adapté pour tourner en service persistant
(plusieurs manches sans redémarrer le processus) sans ajouter de
rafraîchissement de jeton (`refreshToken`, déjà présent dans la réponse
Firebase Auth mais non exploité ici).

### 4.5 🔴 Corrigée — le serveur perdait la manche tout seul avant qu'un joueur ne rejoigne

**Trouvée en conditions réelles**, pas par relecture : la première fois que le
client réseau (`app::network_client`, écrit après cet audit pour répondre à la
demande « pouvoir générer deux applications et jouer ensemble ») a été
réellement testé contre un vrai `cargo run --bin server`, le serveur perdait
la manche (« défaite ») en 2,5 à 4,5 secondes — **avant même qu'un client
n'ait eu le temps de se connecter**. Aucun test automatisé ne l'avait
détecté, parce que tous les tests existants pilotaient `AppState` sur des
fenêtres de quelques centaines de millisecondes, jamais assez longtemps pour
que la manche se termine.

**Cause racine (double)** :
1. `player_index()` (heuristique « quel objet est le joueur ») retombait,
   faute d'objet pilotable visible, sur « le premier objet visible avec un
   script non vide » — mais les **monstres** (`ai_chaser`) ont eux aussi un
   script (dégâts + couleur). Un monstre se retrouvait donc désigné « le
   joueur », et les autres monstres — dont l'AABB chevauchait la sienne — se
   « déclenchaient » les uns les autres, vidant la vie partagée en quelques
   secondes.
2. Avant tout correctif, le gabarit joueur local (jamais piloté par un
   serveur headless) restait visible et **le repli initial ci-dessus tombait
   sur lui plutôt que sur un monstre** — un mannequin inerte que l'IA
   poursuivait et dont la vie s'épuisait sans qu'il ne bouge jamais, avec le
   même résultat : défaite avant tout joueur.

**Corrigé** :
- `player_index()` exclut désormais explicitement les objets `ai_chaser`/
  `combat.attackable` de son repli « objet scripté » (`app/mod.rs`).
- Le repli final « premier objet de la scène, quel qu'il soit » a été
  **retiré** : il pouvait désigner un décor statique (le sol, à l'AABB
  immense) comme « le joueur », avec le même effet de déclenchement en
  cascade — `None` (aucun joueur trouvable) doit laisser l'IA/les
  déclencheurs inactifs, pas désigner un objet au hasard.
- Nouvelle méthode `AppState::hide_local_player_template()`, appelée par
  `src/bin/server.rs` juste après le chargement de la scène et avant
  `playing = true` : masque le gabarit avant même le premier join (en plus du
  masquage déjà fait par `spawn_network_player` dès qu'un joueur rejoint).

**Test de régression** (`waiting_for_the_first_player_never_drains_health_
via_monster_scripts`, `app/multiplayer.rs`) : 80 pas de simulation sans
aucun joueur réseau connecté, `hud_health` doit rester `None` et la manche ne
doit jamais se perdre. Vérifié manuellement en plus du test : le serveur réel
(`cargo run --bin server`) reste stable indéfiniment (15 s+ testées) en
attente d'un joueur, contre 2,5-4,5 s avant correctif.

**Leçon** : c'est le seul problème de cet audit trouvé en **exécutant
réellement l'application** plutôt qu'en relisant le code ou en lançant les
tests existants — aucun test présent avant ce correctif ne faisait tourner
une manche assez longtemps pour l'exposer. Les 134 tests lib + 2 tests bin
actuels restent verts après correctif, clippy/fmt propres.

## 5. Vérifications de cohérence doc ↔ code

Les affirmations de `SPRINT_MMORPG.md` recoupées pendant cet audit se sont
révélées exactes : nombre de tests, chiffres du Sprint 61 (rejoués pendant cet
audit, cf. §2), présence effective des limites documentées (santé non
individualisée, snapshot sans monstres, etc.). Aucune divergence trouvée entre
« ce qui est écrit comme fait » et « ce qui est réellement dans le code », en
dehors des deux anomalies latentes ci-dessus (§4.2, §4.3) qui n'étaient pas
mentionnées dans la doc de sprint avant cet audit.

## 6. Verdict

Le chantier multijoueur est **solide pour son échelle visée** (2-16 joueurs,
serveur autoritaire) : architecture cohérente, tests réels (y compris
bout-en-bout à travers de vrais sockets), et une discipline de documentation
des limites plutôt rare (les zones non testées sont signalées, pas cachées).
L'audit a trouvé et corrigé quatre problèmes réels, tous avec test de
régression et sans rien casser (**134 tests lib + 2 tests bin verts** après
correctifs, clippy/fmt propres) :
- §4.1 — objet fantôme sur double `Join` (bug de correction).
- §4.2 — indices de joueurs réseau non réinitialisés à un restart de manche
  (latent, corrigé par précaution).
- §4.3 — un runtime tokio multi-thread complet par connexion (gaspillage de
  ressources, mesuré : 30 threads OS au total pour 17 connexions après
  correctif, contre plus de 150 estimés avant).
- §4.5 — le serveur perdait la manche tout seul avant qu'un joueur ne
  rejoigne (un monstre, puis le gabarit inerte, se faisaient désigner
  « le joueur » par l'heuristique `player_index`). **Le seul des cinq trouvé
  en exécutant réellement l'application** plutôt qu'en relisant le code —
  confirme que la relecture, aussi rigoureuse soit-elle, ne remplace pas un
  vrai test d'exécution bout en bout.

Il ne reste **aucune anomalie ouverte** de cet audit, à l'exception d'une
limite de conception à garder à l'esprit (§4.4, jeton Firebase serveur non
renouvelé — sans impact tant que le serveur ne tourne qu'une seule manche par
processus). Le client réseau (`app::network_client`, écrit après la rédaction
initiale de cet audit) a depuis été testé bout-en-bout (deux `AppState`
connectées au même serveur, chacune voit un fantôme de l'autre mais jamais
d'elle-même) — mais reste **non vérifié visuellement** : aucune fenêtre
graphique n'a été vue tourner dans cet environnement, et aucun projet
Firebase réel n'a été testé. C'est un chantier **backend prêt à être
branché**, avec un premier bout-en-bout applicatif fonctionnel, mais pas
encore un produit vérifié visuellement de bout en bout.
