# Plan de sprints — Audit du 19 juillet 2026 à 12 h 24 (version optimisée)

> Plan d'action issu de [AnalyseAudit12h24.md](AnalyseAudit12h24.md), enrichi par une
> session de cartographie du code. **Rédigé pour être exécuté par un agent sans
> redécouverte du dépôt** : chaque sprint donne l'état des lieux vérifié
> (fichier:ligne), des sous-phases concrètes, les pièges connus, et un critère
> « terminé quand » vérifiable par commande.

---

## Guide de l'agent exécutant — à lire avant tout sprint

### Conventions du projet (non négociables)

- **Tout en français** : code visible, docs, messages de commit, textes UI.
- Messages de commit descriptifs, style existant (voir `git log --oneline`) ; les
  audits gameplay utilisent le préfixe « Audit gameplay : … ».
- **Tests-preuves** : chaque fonctionnalité livrée s'accompagne de tests qui prouvent
  le comportement (voir [tests/first_game_example.rs](../tests/first_game_example.rs)
  comme modèle de style : noms de tests en phrases, assertions sur le comportement
  réel, pas sur l'implémentation).
- CI stricte : `cargo fmt --all -- --check` et
  `cargo clippy --all-targets -- -D warnings` doivent passer. Lancer `cargo fmt --all`
  avant chaque commit.
- Il existe un budget `unwrap/panic` contrôlé en CI
  (`python3 scripts/check_unwrap_budget.py`) : préférer les erreurs propagées.

### Pièges connus du dépôt (mémoire des sessions précédentes)

1. **L'export écrase la scène versionnée** : `ExportPanel::start()`
   ([export.rs:135-140](../src/editor/export.rs)) réécrit en place
   `assets/player_scene.json` (le vrai jeu MMORPG servi en ligne) et
   `bundle_scene_json()` (l.819) **supprime puis régénère `assets/bundle/`**. Ne
   jamais committer ces fichiers modifiés par accident ; après régénération légitime
   du bundle, faire `touch src/assets.rs` pour forcer la re-inclusion.
2. **Sessions concurrentes possibles sur ce dépôt** : vérifier `git status` juste
   avant chaque commit ; ne committer que ses propres fichiers.
3. **Test flaky préexistant** : le test roguelike « wave-clear » échoue ~60-80 % du
   temps déjà sur `main` — ce n'est pas une régression, ne pas le « corriger » en
   passant.
4. **`PROTOCOL_VERSION = 6`** ([protocol.rs:42](../src/net/protocol.rs)) : toute
   modification du protocole couple le déploiement client/VPS — hors périmètre de ces
   sprints, ne pas y toucher.
5. **`.app` périmés sur volumes montés** (`/Volumes/RusteeGear`, `/Volumes/MMORPG`) :
   avant de diagnostiquer « un bug qui persiste » dans un bundle, comparer la date de
   build.
6. **Environnement sans sockets** : la sandbox locale peut interdire
   `TcpListener::bind` — c'est précisément l'objet du Sprint 1 ; ne pas conclure que
   Pilot est cassé sur un `Operation not permitted`.

### Vérification standard de fin de sprint

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets        # doit passer même sans sockets après le Sprint 1
```

---

## Priorités immédiates

| Ordre | Travail | Effort | Impact |
|---:|---|---:|---:|
| 1 | Isoler les tests TCP Pilot | Faible | Élevé |
| 2 | Corriger le chemin du DMG | Très faible | Élevé |
| 3 | Créer une Release alpha | Faible | Très élevé |
| 4 | Ajouter le manifeste de projet | Moyen | Très élevé |
| 5 | Transformer le wizard | Moyen | Très élevé |
| 6 | Migrer First Game | Moyen | Très élevé |
| 7 | Sécuriser les sauvegardes | Moyen | Élevé |
| 8 | Tester avec des personnes extérieures | Faible | Décisif |
| 9 | Vérifier le canvas web (11.1, à glisser quand on veut) | Très faible | Moyen |
| 10 | Après la bêta : découpage des monolithes (S9), undo inspecteur (S10), web (S11) | Moyen | Élevé |

---

## Sprint 1 — Stabiliser Pilot

**Objectif** : rendre la suite standard indépendante des permissions réseau.

### État des lieux vérifié

- [tests/pilot_bridge.rs](../tests/pilot_bridge.rs) : 6 tests. 4 TCP
  (`pilot_bridge_drives_lua_play_scene_and_inputs_over_tcp` l.59,
  `pilot_bridge_reports_lua_errors_and_survives_malformed_requests` l.128,
  `pilot_bridge_full_editing_and_gameplay_session` l.168,
  `pilot_bridge_options_and_demo_loading` l.250), démarrés par
  `PilotServer::start(0, None)`, pilotés par les helpers `drive` (l.17), `ask` (l.41),
  `connect` (l.52). 2 tests `advance_steps_*` (l.288, l.304) purs, sans socket.
- **La feature d'isolation existe déjà** : `net_tests` dans la section `[features]` de
  [Cargo.toml](../Cargo.toml). Pattern à imiter :
  `#[cfg(all(test, feature = "net_tests"))]` sur le module de tests réseau de
  [src/bin/server.rs:1107](../src/bin/server.rs) (raison documentée l.1102-1106 :
  certains runners CI restreignent le bind loopback).
- Job CI modèle : `net-tests` ([ci.yml:51](../.github/workflows/ci.yml)) exécute
  `cargo test --features net_tests` avec les deps système Linux (l.29-33) — il couvre
  aussi les tests d'intégration de `tests/`.
- Sécurité du pont : **déjà acquise par construction** — bind exclusif
  `TcpListener::bind(("127.0.0.1", port))` ([pilot.rs:72](../src/pilot.rs)), jamais
  actif par défaut (`pilot_port_requested()`, [lib.rs:869](../src/lib.rs)).

### Sous-phases

- **1.1 — Gater les 4 tests TCP** : dans `tests/pilot_bridge.rs`, annoter chacun des
  4 tests TCP avec `#[cfg(feature = "net_tests")]` (et gater de même les helpers
  `drive`/`ask`/`connect` pour éviter les warnings `dead_code` hors feature).
  **Recommandation : réutiliser `net_tests`**, ne pas créer de feature `pilot_tests` —
  même cause (bind loopback interdit), même job CI, zéro maintenance en plus. Si une
  distinction s'avère nécessaire plus tard, elle pourra être ajoutée alors.
- **1.2 — Suite normale intacte** : les 2 tests `advance_steps_*` restent non gatés.
- **1.3 — CI** : vérifier que le job `net-tests` existant compile bien
  `tests/pilot_bridge.rs` avec la feature (c'est le cas : `cargo test --features
  net_tests` inclut les tests d'intégration). Aucun nouveau job n'est nécessaire ;
  ajouter seulement un commentaire dans `ci.yml` indiquant que ce job couvre aussi
  Pilot TCP.
- **1.4 — Preuve de localité** : ajouter un test (gaté `net_tests`) affirmant que
  `PilotServer::start` binde une adresse dont `ip()` est `127.0.0.1` (lire
  `local_addr`, champ public de [pilot.rs:63](../src/pilot.rs)) ; plus un test **non
  gaté** sur `pilot_port_requested()` (fonction pure) prouvant que sans `--pilot` ni
  env, le pont est désactivé.
- **1.5 — Documentation** : dans [PILOT.md](PILOT.md), préciser : désactivé par
  défaut, écoute locale uniquement, et comment lancer les tests TCP
  (`cargo test --features net_tests --test pilot_bridge`).

### Terminé quand

```bash
cargo test --all-targets                                # passe sans sockets
cargo test --features net_tests --test pilot_bridge     # 6 tests dans un env avec sockets
```

---

## Sprint 2 — Corriger et tester la Release

**Objectif** : une Release GitHub alpha téléchargeable et lançable.

### État des lieux vérifié

- [packaging/build_dmg.sh](../packaging/build_dmg.sh) a **deux modes** :
  - normal (`OUTPUT_NAME=RusteeGear`, défaut) → bundle **éditeur** →
    `target/release/bundle/dmg/RusteeGear.dmg` (l.41) ;
  - export (`OUTPUT_NAME` autre + `PLAYER_BUILD=1`) → identité réécrite au PlistBuddy
    (l.25-39), feature `player_build`, scène embarquée → `target/export/${OUTPUT_NAME}.dmg`.
- [release.yml:27](../.github/workflows/release.yml) attache `Motor3DeRust.dmg`, qui
  **n'existe pas** (le job appelle le mode normal, qui produit `RusteeGear.dmg`).
- Le DMG non signé nécessite un clic droit → Ouvrir au premier lancement (note dans le
  script, l.46-47).
- ⚠ Piège n° 1 du guide : construire un Player en CI ne doit pas passer par le panneau
  éditeur ; le script shell suffit (`PLAYER_BUILD=1` embarque la scène **déjà
  présente** dans `assets/player_scene.json` — pour un DMG « First Game » il faudrait
  une scène embarquée différente, voir 2.2).

### Sous-phases

- **2.1 — Chemin du DMG** (le correctif d'une ligne) : dans `release.yml`,
  `files: target/release/bundle/dmg/RusteeGear.dmg`. Renommer le livrable est possible
  via une étape `mv` explicite si un nom versionné est souhaité
  (`RusteeGear-Editor-${{ github.ref_name }}.dmg`).
- **2.2 — Décision de contenu** : le DMG actuel = **éditeur** (avec démos intégrées).
  C'est le bon livrable alpha n° 1. Un DMG « First Game » (Player) est **optionnel** à
  ce stade : il exigerait d'embarquer `examples/first_game/scene.json` comme scène
  player **sans écraser** `assets/player_scene.json` versionné (par exemple : le
  générer dans la CI puis `git checkout -- assets/` avant tout autre pas, ou
  paramétrer le chemin de scène embarquée). Si l'effort dépasse une demi-journée,
  livrer l'éditeur seul en alpha.1 et reporter le Player en alpha.2.
- **2.3 — Tag alpha** : créer et pousser `v0.1.0-alpha.1` ; le workflow se déclenche
  sur `v*`. Vérifier le job Android au passage (`APP_VERSION` est dérivé du tag —
  contrôler qu'un suffixe `-alpha.1` ne casse pas `build_apk.sh`).
- **2.4 — Test sur machine propre** : télécharger le DMG de la Release, le monter, le
  lancer (clic droit → Ouvrir), dérouler le Quickstart. Piège n° 5 : ne pas tester un
  vieux `.app` d'un volume déjà monté.

### Livrables (cible, adaptée par 2.2)

```text
RusteeGear-Editor-v0.1.0-alpha.1.dmg        (obligatoire)
RusteeGear-FirstGame-v0.1.0-alpha.1.dmg     (optionnel, sinon alpha.2)
RusteeGear-FirstGame-v0.1.0-alpha.1-web.zip (optionnel, sinon alpha.2)
motor3derust.apk                            (déjà produit par le job android)
```

### Terminé quand

La Release `v0.1.0-alpha.1` existe sur GitHub, son DMG éditeur se télécharge, se monte
et démarre sur une installation propre, et `git status` reste vierge (aucun
`player_scene.json`/`bundle/` modifié).

---

## Sprint 3 — Manifeste de projet

**Objectif** : que RusteeGear sache ce qu'est un « projet », pas seulement une scène.

### État des lieux vérifié

- Aucune notion de projet ni de fichiers récents dans `src/` (recherches
  `ProjectManifest|project\.rusteegear|recent|\.rgproj` : néant).
- Ouverture actuelle : `rfd::FileDialog::pick_file` ([menus.rs:173](../src/editor/menus.rs))
  → `AppState::load_from` ([app/persistence.rs:227](../src/app/persistence.rs), thread
  de fond) → `Scene::load` + `migrate()`.
- Les assets sont résolus par URI (`asset://`, `asset-id://`, `bundle://`, `user://`)
  via `read_bytes` ([assets.rs:363](../src/assets.rs)) ancré sur
  `~/.motor3derust/assets` (`assets_dir()`, l.243) — **pas relativement à la scène**.
  Garde anti-traversée : `safe_join` (l.314).
- Préférences persistées existantes à imiter : `Settings`
  ([settings.rs:155/191](../src/app/settings.rs), `~/.motor3derust/settings.json`) et
  `BuildConfig` ([build_config.rs:251-277](../src/app/build_config.rs)).

### Sous-phases

- **3.1 — Struct et format** : nouveau module `src/project.rs` avec
  `ProjectManifest` (serde) :

  ```json
  {
    "format": 1,
    "name": "First Game",
    "main_scene": "scenes/main.scene.json",
    "build": "build.json"
  }
  ```

  `format` contrôlé (erreur lisible si supérieur au connu), `build` optionnel.
  Champs prévus pour plus tard (déclarés dès le format 1 mais optionnels) : identité
  du jeu pour l'export, dossier `assets/` par projet, index d'assets
  (`.rusteegear/asset-index.json`) — voir « cible long terme » en fin de sprint.
- **3.2 — Chargement et validation** : `ProjectManifest::load(dir)` cherche
  `project.rusteegear.json` dans le dossier ; erreurs en français et actionnables
  (fichier manquant, JSON invalide avec ligne, scène principale introuvable). Utiliser
  `safe_join` pour interdire `main_scene` hors racine.
- **3.3 — Racine de projet dans l'app** : champ `AppState`-niveau
  `current_project: Option<ProjectRoot>` (nom + chemin racine). À ce stade, la
  résolution d'un **nouveau** schéma `project://chemin/relatif` dans `read_bytes` peut
  se limiter aux scènes et scripts ; ne pas réécrire le système `asset-id://`
  existant (les assets importés continuent de vivre dans `~/.motor3derust/assets` —
  la migration complète des assets vers le projet est un chantier ultérieur, le noter
  dans [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md)).
- **3.4 — Ouverture par manifeste** : ouvrir un dossier (ou son
  `project.rusteegear.json`) charge `main_scene` via `load_from` et pose
  `current_project`. Étendre le filtre du dialogue « 📂 Ouvrir… » pour accepter le
  manifeste.
- **3.5 — Tests-preuves** : nouveau `tests/project_manifest.rs` — chargement valide,
  `format` inconnu refusé avec message lisible, `main_scene: "../évasion.json"`
  refusé, manifeste absent → erreur claire, ouverture d'un projet minimal fixture →
  scène chargée.

### Cible long terme (hors périmètre du sprint, à garder en tête)

Le modèle complet visé — un projet = un répertoire autonome, déplaçable et
versionnable :

```text
mon-jeu/
├── project.rusteegear.json      (identité, scène de démarrage, export)
├── scenes/
├── assets/{models,audio,textures}/   (assets PAR projet, plus le dossier global)
├── prefabs/
├── scripts/
├── build/build-config.json
└── .rusteegear/asset-index.json      (références stables)
```

Avec, en transition : support conservé des scènes seules et du dossier global
`~/.motor3derust/assets`, plus une commande « Convertir en projet » qui copie les
assets référencés dans le projet. Ce sprint ne livre que le manifeste et l'ouverture ;
la migration des assets est un chantier ultérieur, documenté dans
[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md).

### Terminé quand

Un dossier avec manifeste s'ouvre comme un projet (scène principale chargée,
`current_project` posé), la validation échoue proprement sur les cas d'erreur, et les
tests de `tests/project_manifest.rs` passent dans la suite standard.

---

## Sprint 4 — Gestionnaire de projets

**Objectif** : le cycle de vie complet d'un projet depuis l'éditeur.

### État des lieux vérifié

- Wizard actuel : `new_project_wizard_window`
  ([windows.rs:2143-2199](../src/editor/windows.rs)) — 3 boutons de template
  (Scène vide → `new_scene()` [selection.rs:350](../src/app/selection.rs), Démo
  contrôleur, Niveau de combat). Ni nom, ni dossier, ni persistance.
- ⚠ Piège egui connu du dépôt : une ligne cliquable **et** glissable doit être un seul
  widget `Sense` clic+drag (`dnd_drag_source` avale les clics) — concerne la future
  liste de projets récents si elle devient réordonnable.

### Sous-phases

- **4.1 — Wizard de création réel** : transformer la fenêtre en formulaire
  « Nom du projet / Emplacement (choisi via `rfd::FileDialog::pick_folder`) /
  Template / Créer ». « Créer » génère la structure (manifeste, `scenes/main.scene.json`
  depuis le template, `scripts/`), puis ouvre le projet (Sprint 3.4). Les 3 templates
  existants restent les choix proposés.
- **4.2 — Ouvrir / Fermer** : entrée de menu « Ouvrir un projet… » (sélection du
  dossier ou du manifeste) ; « Fermer le projet » revient à l'état sans projet, en
  passant par la modale existante si `scene_dirty`.
- **4.3 — Projets récents** : persister une liste MRU (chemin + nom + date) dans
  `Settings` ([settings.rs](../src/app/settings.rs)) — c'est la struct de préférences
  déjà chargée/sauvée ; plafonner à 10, ignorer silencieusement les chemins disparus.
  Affichage : sous-menu « Fichier → Projets récents » + liste à l'ouverture du wizard.
- **4.4 — Confort** : « Révéler dans le Finder » (`open -R` sur macOS, gated
  plateforme) ; « Dupliquer le projet » (copie récursive du dossier + renommage dans
  le manifeste).
- **4.5 — Tests-preuves** : création depuis template dans un `tempdir` → manifeste
  valide → ouverture → scène principale chargée ; MRU mis à jour et plafonné ;
  duplication → manifeste renommé.

### Terminé quand

On peut créer, ouvrir, fermer, retrouver (récents) et dupliquer un projet sans toucher
au système de fichiers à la main, tests à l'appui.

---

## Sprint 5 — Migrer First Game

**Objectif** : First Game devient le premier vrai projet RusteeGear.

### État des lieux vérifié

- `examples/first_game/` : `scene.json` (version 2, 10 objets, scripts inline),
  `README.md`, `preview.png`, `scripts/{rotating_object,zone_signal}.lua` (copies
  lisibles des scripts inline — **pas** chargées par le moteur).
- Les 4 tests : [tests/first_game_example.rs](../tests/first_game_example.rs) —
  `example_dir()` (l.11) pointe sur le dossier, `load_scene()` sur `scene.json`.
- Phrases de doc à changer : [QUICKSTART.md](../QUICKSTART.md) §4 (l.47-50 :
  « Sélectionner `examples/first_game/scene.json` ») ; [FIRST_GAME.md](FIRST_GAME.md)
  prérequis (l.3-4) et Étape 1 (l.9), plus l'avertissement l.60-61 (« ne sauvegarde
  pas par-dessus scene.json ») ; [examples/first_game/README.md](../examples/first_game/README.md)
  section « Ouvrir la scène ».

### Sous-phases

- **5.1 — Restructuration** (avec `git mv` pour garder l'historique) :

  ```text
  examples/first_game/
  ├── project.rusteegear.json
  ├── scenes/main.scene.json      (ex scene.json)
  ├── scripts/                    (copies lisibles, inchangées)
  ├── preview.png
  └── README.md
  ```

  `build.json` : ne le créer que s'il a un consommateur (sinon l'omettre — le
  manifeste le déclare optionnel depuis 3.1).
- **5.2 — Tests adaptés** : mettre à jour `example_dir()`/`load_scene()` dans
  `first_game_example.rs` ; ajouter un 5ᵉ test : le projet s'ouvre par manifeste et
  charge `scenes/main.scene.json`.
- **5.3 — Documentation** : réécrire les passages listés ci-dessus en
  « Ouvrir le projet `examples/first_game` » ; l'avertissement « garde l'exemple
  intact » reste valable (adapter le chemin).
- **5.4 — Vérification transverse** : `grep -rn "first_game/scene.json"` sur tout le
  dépôt (docs, scripts, tests, code) doit rendre zéro résultat résiduel.

### Terminé quand

First Game s'ouvre comme un projet, les 5 tests passent, le Quickstart est à jour, et
le grep transverse est vide.

---

## Sprint 6 — Sécuriser les sauvegardes

**Objectif** : aucun testeur extérieur ne doit pouvoir perdre son travail.

### État des lieux vérifié

- **Déjà fait** (ne pas réimplémenter) : dirty-flag `scene_dirty` (levé par
  `push_undo` [selection.rs:260](../src/app/selection.rs), gizmo, Pilot, empreinte
  d'inspecteur [persistence.rs:202](../src/app/persistence.rs)) ; modale
  « Modifications non sauvegardées » à la fermeture
  ([editor/mod.rs:2406-2431](../src/editor/mod.rs)).
- **À faire** : `Scene::save` ([scene/persistence.rs:125](../src/scene/persistence.rs))
  est un `fs::write` direct — non atomique, sans backup. Quick-save par défaut :
  `~/motor3derust_scene.json` (`scene_path()`, [app/mod.rs:1625](../src/app/mod.rs)).
- Emplacement naturel des données d'app : `~/.motor3derust/`
  (`app_data_dir()`, [assets.rs:278](../src/assets.rs)).

### Sous-phases

- **6.1 — Sauvegarde atomique** : dans `Scene::save`, écrire vers
  `<chemin>.tmp` **dans le même dossier** (même volume, sinon `rename` non atomique)
  puis `fs::rename` sur le chemin final. Couvre d'un coup tous les appelants
  (`save`/`save_to`/export).
- **6.2 — Backup** : avant le rename, si le fichier cible existe, le renommer en
  `<chemin>.backup` (une seule génération suffit pour l'alpha).
- **6.3 — Autosave** : sur une cadence simple (par exemple toutes les 2 minutes si
  `scene_dirty`), sérialiser vers `~/.motor3derust/autosave/<horodatage>.json`
  (jamais par-dessus le fichier de l'utilisateur) ; garder les 5 plus récents.
  Brancher dans la boucle app existante (là où Pilot est pollé,
  [lib.rs:759](../src/lib.rs), tick déjà disponible).
- **6.4 — Récupération au redémarrage** : au lancement, si un autosave est plus
  récent que la dernière sauvegarde manuelle connue, proposer une modale
  « Restaurer / Ignorer » (réutiliser le style de la modale de fermeture).
- **6.5 — Tests-preuves** : atomicité (simuler l'échec entre `.tmp` et rename : le
  fichier cible d'origine est intact) ; `.backup` présent après deux sauvegardes et
  contenant la version N-1 ; rotation des autosaves à 5 ; round-trip autosave →
  restauration. Les tests d'écriture utilisent des `tempdir`, pas `~`.

### Terminé quand

Tuer le processus pendant une sauvegarde ne corrompt jamais la scène (prouvé par
test), et un crash ne fait perdre au pire que l'intervalle d'autosave.

---

## Sprint 7 — Serveur local depuis l'éditeur

**Objectif** : le multijoueur local sans ligne de commande.

### État des lieux vérifié

- Serveur headless : binaire `server` ([src/bin/server.rs](../src/bin/server.rs)),
  adresse par défaut `127.0.0.1:7777` (l.54), surcharge **par env**
  `RUSTEEGEAR_SERVER_ADDR` (l.677) — pas d'argument CLI. Transport WebSocket,
  multi-salons (`ClientMsg::Join::lobby`).
- L'éditeur sait déjà **se connecter** : fenêtre multijoueur
  ([windows.rs:1083, 1194](../src/editor/windows.rs)), action
  `connect_to_server` ([editor/mod.rs:369](../src/editor/mod.rs)), URL par défaut
  `wss://ws.loicberthod.ch` ([network_client.rs:39](../src/app/network_client.rs)).
- Piège n° 4 : ne pas toucher à `PROTOCOL_VERSION`.

### Sous-phases

- **7.1 — Démarrer / arrêter** : depuis la fenêtre multijoueur, lancer le binaire
  `server` en processus enfant (`std::process::Command`, env
  `RUSTEEGEAR_SERVER_ADDR=127.0.0.1:7777`) ; bouton Arrêter = kill propre du child ;
  arrêt automatique à la fermeture de l'éditeur.
- **7.2 — État visible** : panneau affichant : serveur arrêté/en cours (PID), adresse,
  et — si disponible via la connexion locale — le nombre de joueurs.
- **7.3 — Copier l'adresse** : bouton copiant `ws://127.0.0.1:7777` dans le
  presse-papiers (`ctx.copy_text` côté egui).
- **7.4 — Auto-connexion de l'hôte** : après démarrage réussi (attendre que le port
  accepte), poser `actions.connect_to_server` vers l'adresse locale — le chemin de
  connexion existant fait le reste.
- **7.5 — Code de salon** : le protocole a déjà `lobby` — exposer un champ « Salon »
  dans la fenêtre (réutiliser `DEFAULT_LOBBY`) et l'inclure dans l'adresse copiée
  (`ws://…/?salon=x` ou consigne texte), sans changement de protocole.
- **7.6 — Tests-preuves** : gatés `net_tests` — cycle démarrer le vrai binaire →
  se connecter → arrêter ; et un test que l'arrêt de l'éditeur ne laisse pas de
  processus orphelin. Automatisation Pilot bienvenue (`net connect` existe déjà,
  [pilot.rs:403](../src/pilot.rs)).

### Terminé quand

Deux instances sur la même machine jouent ensemble sans jamais ouvrir un terminal, et
aucun processus serveur ne survit à la fermeture de l'éditeur.

---

## Sprint 8 — Bêta extérieure

**Objectif** : valider avec 3–5 personnes extérieures.

### Sous-phases

- **8.1 — Kit testeur** : Release alpha (Sprint 2) + [QUICKSTART.md](../QUICKSTART.md)
  + [TEST_SCENARIO.md](TEST_SCENARIO.md) + [TEST_FEEDBACK_FORM.md](TEST_FEEDBACK_FORM.md).
  Vérifier que le Quickstart correspond bien à la version taguée (pas à `main`).
- **8.2 — Scénario imposé** : 1. suivre le Quickstart ; 2. ouvrir First Game ;
  3. jouer ; 4. ajouter un cube ; 5. ajouter le script ; 6. sauvegarder ;
  7. rouvrir ; 8. exporter ; 9. envoyer un retour.
- **8.3 — Collecte et tri** : centraliser les retours dans
  `docs/audits/retours-alpha1.md`, classés bloquant / gênant / cosmétique, avec
  machine et version.
- **8.4 — Boucle corrective** : corriger les bloquants, publier `v0.1.0-alpha.2` si
  nécessaire (le pipeline du Sprint 2 rend cela peu coûteux).

### Terminé quand

Au moins 3 personnes extérieures ont déroulé le scénario de bout en bout et leurs
retours sont triés et adressés.

---

# Sprints complémentaires (issus du second retour, vérifiés)

Trois chantiers supplémentaires retenus après tri (voir la section « Second retour »
de [AnalyseAudit12h24.md](AnalyseAudit12h24.md)). Ils ne bloquent pas la bêta
extérieure (Sprint 8) mais deviennent prioritaires juste après — sauf 11.1 qui peut
se glisser à tout moment.

---

## Sprint 9 — Découper les fichiers monolithes

**Objectif** : ramener chaque module à une seule raison de changer, par extraction
mécanique, **sans réécriture ni changement de comportement**.

### État des lieux vérifié (`wc -l`)

| Fichier | Lignes |
|---|---:|
| [src/scene/demos.rs](../src/scene/demos.rs) | 10 820 |
| [src/app/mod.rs](../src/app/mod.rs) | 4 393 |
| [src/scene/mod.rs](../src/scene/mod.rs) | 4 016 |
| [src/gfx/renderer.rs](../src/gfx/renderer.rs) | 3 432 |
| [src/app/network_client.rs](../src/app/network_client.rs) | 3 228 |
| [src/app/simulation.rs](../src/app/simulation.rs) | 3 050 |

### Sous-phases

- **9.1 — `scene/demos.rs` d'abord** (le pire, et le plus mécanique) : le convertir
  en dossier `scene/demos/` (`mod.rs` réexportant tout à l'identique, puis
  `controller.rs`, `gameplay.rs`, et un sous-dossier `mmorpg/` — terrain, village,
  créatures, vagues, validation). Un commit par extraction ; `pub(crate)` et
  réexports pour ne casser aucun chemin d'import.
- **9.2 — `gfx/renderer.rs`** : dossier `gfx/renderer/` — ressources, frame,
  synchro scène, ombres, post-process, UI. Attention au piège n° 3 du guide (les
  goldens sont sensibles aux changements de shader : ici on ne touche **pas** aux
  shaders, seulement au découpage Rust ; si un golden bouge, c'est un signal d'erreur).
- **9.3 — `app/mod.rs` et `scene/mod.rs`** : extraire d'abord ce qui est déjà
  thématique (le modèle de données de scène vs ses migrations vs sa logique de jeu).
  S'arrêter quand chaque fichier repasse sous ~1 500 lignes ; ne pas viser la pureté.
- **9.4 — Garde-fou** : après chaque extraction, la vérification standard du guide,
  plus `cargo test --features net_tests` une fois à la fin. Zéro diff de comportement
  attendu ; tout test golden ou visuel qui change invalide l'extraction.

### Terminé quand

Aucun fichier de `src/` ne dépasse ~4 000 lignes (cible ~1 500 pour les nouveaux
modules), la suite complète passe, et `git log` montre des extractions atomiques
relisibles une par une.

---

## Sprint 10 — Undo complet des éditions d'inspecteur

**Objectif** : tout ce qui modifie la scène depuis l'éditeur est annulable.

### État des lieux vérifié

- L'undo structurel existe : `push_undo` dans
  [app/selection.rs](../src/app/selection.rs) (créations, suppressions, duplications,
  gizmo…).
- **Aucun `push_undo` dans `src/editor/`** : les champs d'inspecteur (couleur, script,
  physique, collider, lumières…) ne sont pas annulables — ils sont seulement détectés
  comme « dirty » par l'empreinte `ui_scene_fingerprint()`
  ([app/persistence.rs:202](../src/app/persistence.rs)).

### Sous-phases

- **10.1 — Regroupement par interaction** : ne PAS pousser un undo par variation de
  slider. Utiliser le cycle d'interaction egui (`drag_started`/`drag_stopped`,
  `gained_focus`/`lost_focus`) : capturer l'état de l'objet au **début** de
  l'interaction, pousser une seule entrée d'undo à la **fin** si la valeur a changé.
  L'infrastructure d'undo existante (snapshot de scène via `push_undo`) peut suffire —
  commencer par « snapshot au début d'interaction » avant d'introduire un
  `EditCommand` granulaire ; n'introduire les commandes fines que si les snapshots
  s'avèrent trop lourds en mémoire.
- **10.2 — Couverture systématique** : passer en revue les panneaux d'inspecteur
  ([editor/windows.rs](../src/editor/windows.rs)) et brancher le mécanisme 10.1 sur
  chaque champ éditable (transform textuel, couleur, script, physique, trigger,
  lumières, contrôleur…).
- **10.3 — Import GLB transactionnel** : l'import copie l'asset
  (`import_to_assets`, [assets.rs:387](../src/assets.rs)) puis crée l'objet. Grouper
  en une transaction : l'annulation supprime l'objet, et ne retire l'asset du
  catalogue que s'il vient d'être importé et n'est référencé nulle part ailleurs.
- **10.4 — Tests-preuves** : éditer une couleur → undo → couleur d'origine ;
  glisser un slider (plusieurs frames) → **une seule** entrée d'undo ; import GLB →
  undo → ni objet ni asset orphelin ; undo/redo symétriques.

### Terminé quand

N'importe quelle édition faite dans l'inspecteur revient à l'état antérieur par
Ctrl+Z (prouvé par tests), avec une entrée d'undo par interaction, pas par frame.

---

## Sprint 11 — Web : parité minimale crédible

**Objectif** : qu'un jeu exporté web ne surprenne pas son créateur.

### État des lieux vérifié

- **Lua portable : déjà largement traité** — [LUA_PORTABLE.md](LUA_PORTABLE.md)
  (mlua natif vs rilua web), API moteur portée à l'identique
  (`scripting.rs`/`scripting_web.rs`), tests différentiels existants
  (`cargo test official_scripts_match`).
- **Canvas : problème NON confirmé** — [packaging/web/index.html](../packaging/web/index.html)
  donne au canvas `width:100%; height:100%` et le wasm embarque un `ResizeObserver`
  (winit web). Reproduire avant de corriger.
- **Musique en streaming : absente sur web, confirmé** —
  [runtime/audio.rs](../src/runtime/audio.rs) : `kira::sound::streaming` en natif,
  `StreamingHandles = ()` en cfg wasm (la musique est chargée entière).

### Sous-phases

- **11.1 — Vérifier le redimensionnement du canvas** (rapide, faisable à tout
  moment) : `./packaging/build_web.sh`, servir `packaging/web/`, redimensionner la
  fenêtre et vérifier taille physique × `devicePixelRatio`. Si le bug est réel :
  brancher le recalcul sur l'évènement de resize et appeler le resize du renderer.
  S'il ne l'est pas : le noter dans l'analyse et fermer le point.
- **11.2 — Validation « Lua portable » à l'export Web** : au moment de l'export web
  ([editor/export.rs](../src/editor/export.rs)), passer les scripts de la scène au
  crible du sous-ensemble documenté dans [LUA_PORTABLE.md](LUA_PORTABLE.md) ;
  avertissement précis par script fautif (nom de l'objet + API non portable). Les
  tests différentiels existants servent d'oracle pour la liste des API garanties.
- **11.3 — Musique en flux sur web** (priorité moyenne, en dernier) : chemin séparé
  du système SFX, via `HTMLAudioElement` ou Web Audio streaming, derrière la même API
  `play_music_streaming_gain`. Test manuel dans deux navigateurs.

### Terminé quand

Le comportement du canvas est établi (corrigé ou disculpé), un export web d'une scène
au script non portable produit un avertissement nommant l'API en cause, et une
musique longue démarre sans télécharger le fichier entier.

---

## Backlog explicite (décisions : ne PAS en faire des sprints)

Consigné pour éviter que ces sujets reviennent par réflexe généraliste :

- **GLB étendu (textures PBR, normal maps, multi-matériaux)** : la spécialisation
  `base_color_factor` ([scene/import.rs:52](../src/scene/import.rs)) est un choix
  cohérent avec la charte graphique maison. Seule action retenue, peu coûteuse et
  faisable dans n'importe quel sprint : **avertissements d'import** listant les
  propriétés de matériau ignorées. Le sous-ensemble complet (baseColorTexture →
  metallicRoughness → normal → émissive, espaces colorimétriques, multi-primitives)
  ne se justifie que si « importer des assets de marketplace » devient un objectif
  produit.
- **WebGL en secours de WebGPU** : refusé — un deuxième chemin de rendu coûte plus
  qu'il ne rapporte. À la place : page de compatibilité claire quand WebGPU est
  absent (peut se glisser dans le Sprint 11 si trivial).
- **Migration complète des assets vers le projet** (`assets/` par projet +
  `asset-index.json` + « Convertir en projet ») : cible long terme du Sprint 3,
  planifiée après la bêta.

---

## Vue d'ensemble et dépendances

```text
Sprint 1 (Pilot/net_tests) ─┐
Sprint 2 (DMG + alpha.1)   ─┤  indépendants, à faire d'abord (petits, gros déblocage)
                            ▼
Sprint 3 (manifeste) → Sprint 4 (gestionnaire) → Sprint 5 (migration First Game)
                            ▼
Sprint 6 (sauvegardes sûres, indépendant de 3-5) ; Sprint 7 (serveur local, indépendant)
                            ▼
Sprint 8 (bêta extérieure — exige au minimum 2, idéalement 2+5+6)
                            ▼
Après la bêta : Sprint 9 (monolithes) ; Sprint 10 (undo inspecteur) ; Sprint 11 (web)
                (11.1 canvas = micro-vérification, faisable à tout moment)
```

- Sprints 1, 2, 6, 7 sont indépendants entre eux et de la chaîne 3→4→5.
- Le Sprint 8 exige le Sprint 2 ; il est nettement plus crédible avec 5 et 6.
- Chaque sprint = un ou plusieurs commits dédiés, `git status` vérifié avant chaque
  commit (piège n° 2), vérification standard du guide en fin de sprint.

Voir le constat complet dans [AnalyseAudit12h24.md](AnalyseAudit12h24.md) et la
feuille de route générale dans [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md).
