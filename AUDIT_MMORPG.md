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

- `cargo test` (lib + bin) : **131 tests lib + 2 tests bin, tous verts**
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
`src/app/multiplayer.rs`). Vérifié : 131/131 tests lib verts après correctif,
clippy/fmt propres.

### 4.2 🟠 Non corrigée — indices réseau non réinitialisés à un restart

**Constat** : `AppState::restart_game()` (`app/mod.rs:1049`) et la transition
Play→Edit dans `advance_play` (`app/mod.rs:1719`) remettent
`self.scene.objects` à l'état de `play_snapshot` (capturé **avant** que le
moindre joueur réseau n'ait rejoint, puisque `spawn_network_player` n'est
appelé qu'en cours de Play) — mais ne touchent ni `network_players`, ni
`network_inputs`, ni `network_attack_cooldowns`.

**Conséquence potentielle** : si ces chemins sont un jour empruntés avec des
joueurs réseau connectés, `network_players` continuerait de pointer vers des
indices qui, après restauration, correspondent à d'autres objets (ou
n'existent plus) — un joueur réseau pourrait se retrouver à piloter un objet
qui n'est plus le sien, ou une erreur silencieuse (indices hors bornes filtrés
par les `.get()` existants, donc pas de panique, mais un état incohérent).

**Pourquoi ce n'est pas corrigé ici** : **actuellement inatteignable** — le
serveur headless (`src/bin/server.rs`) ne boucle pas sur plusieurs manches
(le `main()` sort du programme après une seule manche, `break` puis fin de
fonction) et n'appelle jamais `restart_game()` ni ne repasse `playing` à
`false`. Corriger proprement demande une décision de conception (les joueurs
réseau doivent-ils survivre à un restart, re-spawnés à leurs positions
décalées, ou tous être déconnectés/re-attendus ?) qui dépasse le cadre d'un
correctif d'audit. **À traiter avant tout Sprint qui ferait boucler le
serveur sur plusieurs manches** (un « Sprint 62+ » naturel une fois le
déploiement réel abordé).

### 4.3 🟡 Non corrigée — un runtime tokio multi-thread complet par connexion

**Constat** : `NetServer::start` et `NetClient::connect` utilisent tous deux
`tokio::runtime::Runtime::new()` (`server_loop.rs:46`, `client.rs:38`), qui
construit par défaut un runtime **multi-thread** (un thread ouvrier par CPU
logique). Sur la machine de cet audit (10 CPU logiques), le test de charge du
Sprint 61 — 1 `NetServer` + 16 `NetClient` — instancie donc **17 runtimes
multi-thread complets**, soit potentiellement plus de 150 threads OS pour un
besoin qui est fondamentalement de l'attente réseau sur une poignée de
connexions (peu de parallélisme CPU réel requis).

**Conséquence** : pas un bug de correction (les 131+2 tests passent, le test
de charge tourne), mais un vrai gaspillage de ressources — significatif sur un
hébergement modeste (Sprint 62) où un runtime `current_thread` par connexion
suffirait largement à ce volume (2-16 joueurs).

**Pourquoi ce n'est pas corrigé ici** : passer en `current_thread` n'est pas
un simple changement de constructeur — un runtime `current_thread` n'a pas de
thread ouvrier propre : les tâches `spawn`ées ne progressent que pendant qu'un
thread appelle `block_on` sur ce runtime. Le code actuel s'appuie
implicitement sur les threads ouvriers internes du runtime multi-thread pour
faire progresser `outbound`/`inbound` en tâche de fond sans jamais rappeler
`block_on`. Un passage à `current_thread` demande donc de dédier un thread OS
qui bloque sur ce runtime en continu — un vrai remaniement de
`NetServer::start`/`NetClient::connect`, avec un risque de régression sur du
code réseau déjà testé et qui fonctionne. **Recommandé avant un déploiement à
plusieurs salons simultanés** (chaque salon = un `NetServer` = un runtime
aujourd'hui), mais pas urgent à l'échelle actuelle (un salon, 2-16 joueurs).

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
L'audit a trouvé et corrigé un bug réel (objet fantôme sur double `Join`,
§4.1) et documenté deux limites latentes non urgentes (§4.2 indices réseau
au restart, §4.3 coût des runtimes tokio) à traiter avant, respectivement,
un serveur multi-manches et un déploiement à plusieurs salons simultanés.
Le point qui reste hors de portée de cet audit — comme de tout le chantier —
est la validation en conditions réelles : aucune UI vue tourner, aucun projet
Firebase réel testé. C'est un chantier **backend prêt à être branché**, pas
encore un produit vérifié de bout en bout.
