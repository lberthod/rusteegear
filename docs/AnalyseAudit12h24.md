# Analyse — Audit du 19 juillet 2026 à 12 h 24 (session 2, approfondie)

> Photographie de l'état du projet à 12 h 24, complétée par une session d'analyse
> approfondie du code (cartographie fichier:ligne de chaque zone concernée par le plan).
> Constat uniquement : le plan d'action détaillé est dans
> [SprintAudit12h24.md](SprintAudit12h24.md), rédigé pour être exécutable par un agent
> sans redécouverte du dépôt.
>
> Toutes les affirmations vérifiables ont été recontrôlées sur le dépôt ; les preuves
> sont citées en place. Deux affirmations de la réflexion initiale ont été **corrigées**
> après lecture du code (voir « Corrections apportées à la réflexion initiale »).

## Verdict immédiat

RusteeGear est maintenant dans un état beaucoup plus propre et crédible pour une préversion.
Les avancées précédemment non intégrées sont désormais :

- commitées ;
- poussées sur `origin/main` ;
- présentes dans un worktree propre ;
- couvertes par des tests.

Le projet se situe autour de **78–82 % d'une bêta testable par des personnes extérieures**.

---

## Corrections apportées à la réflexion initiale

L'analyse approfondie du code corrige deux points de la réflexion de 12 h 24 — dans les
deux cas **en mieux** :

1. **« Avertissement de modifications non enregistrées » manquant → FAUX, il existe.**
   Un dirty-flag complet est en place : champ `scene_dirty` (levé par `push_undo`
   [selection.rs:260](../src/app/selection.rs), le gizmo, le pont Pilot, et une
   empreinte d'inspecteur `ui_scene_fingerprint()`
   [persistence.rs:202](../src/app/persistence.rs)), avec une modale
   « Modifications non sauvegardées » à la fermeture
   ([editor/mod.rs:2406-2431](../src/editor/mod.rs), boutons
   Enregistrer/Quitter/Annuler). Le Sprint 6 se réduit donc à : atomicité, backup,
   autosave, récupération.

2. **« Vérifier que le pont refuse les connexions non locales » → déjà garanti par
   construction.** Le pont Pilot binde exclusivement `TcpListener::bind(("127.0.0.1",
   port))` ([pilot.rs:72](../src/pilot.rs)) et n'est jamais actif par défaut
   (opt-in `--pilot` / `RUSTEEGEAR_PILOT`, décodé par `pilot_port_requested()`
   [lib.rs:869](../src/lib.rs)). Il reste à le prouver par un test, pas à l'implémenter.

---

## État Git

| Vérification | Résultat |
|---|---:|
| Branche | `main` |
| Synchronisée avec `origin/main` | ✅ |
| Fichiers modifiés | 0 |
| Fichiers non suivis | 0 |
| Worktree propre | ✅ |
| Dernier commit d'audit | `d06a1ac` (état moteur : `1d903e0`) |
| Packs créatures 63–112 intégrés | ✅ |
| First Game intégré | ✅ |
| Pilot intégré | ✅ |

C'est une amélioration importante : le projet n'est plus dans un gros état intermédiaire
difficile à évaluer.

---

## État qualité

| Vérification | Résultat |
|---|---:|
| `cargo fmt --all -- --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| Tests principaux de la bibliothèque | ✅ 618 réussis |
| Tests serveur | ✅ 11 réussis |
| First Game | ✅ 4 réussis |
| Exemple volontairement cassé | ✅ |
| Toutes les scènes exemples | ✅ |
| Assets flore | ✅ |
| Tests visuels | ✅ 8 réussis |
| Tests Pilot sans socket | ✅ 2 réussis |
| Tests Pilot TCP | ❌ 4 |
| Tests ignorés | 9 |

### Interprétation des quatre échecs Pilot

Les quatre tests Pilot échouent sur :

```text
pilot : liaison 127.0.0.1:0 impossible
Operation not permitted
```

Le code compile et les tests Pilot ne nécessitant pas de socket passent. L'environnement
d'exécution interdit l'ouverture d'un port TCP local.

**Vérifié sur le dépôt** : [tests/pilot_bridge.rs](../tests/pilot_bridge.rs) contient
6 tests. Les 4 tests TCP (`pilot_bridge_drives_lua_play_scene_and_inputs_over_tcp` l.59,
`pilot_bridge_reports_lua_errors_and_survives_malformed_requests` l.128,
`pilot_bridge_full_editing_and_gameplay_session` l.168,
`pilot_bridge_options_and_demo_loading` l.250) démarrent tous le pont par
`PilotServer::start(0, None)` (port éphémère). Les 2 tests `advance_steps_*` (l.288,
l.304) sont purs `AppState`, sans socket.

Point clé découvert à l'analyse : **le dépôt possède déjà le mécanisme d'isolation qu'il
faut** — la feature `net_tests` ([Cargo.toml](../Cargo.toml), `[features]`) gate déjà les
tests à sockets réels (exemple : [src/bin/server.rs:1107](../src/bin/server.rs),
`#[cfg(all(test, feature = "net_tests"))]`, avec la raison documentée l.1102-1106 :
certains runners CI restreignent le bind loopback), et le job CI `net-tests`
([ci.yml:51](../.github/workflows/ci.yml)) exécute `cargo test --features net_tests`.
Les tests TCP de `pilot_bridge.rs` sont les seuls tests à sockets **hors** de ce gate.

Ce n'est donc pas la preuve que Pilot est fonctionnellement cassé. C'est un problème
d'organisation des tests, et la correction est petite :

> `cargo test --all-targets` doit passer dans un environnement qui interdit les sockets ;
> les tests TCP Pilot rejoignent le gate `net_tests` existant.

---

## Avancement par domaine

| Domaine | Niveau | État |
|---|---:|---|
| Moteur 3D | 92 % | 🟢 |
| Démo MMORPG | 95 % | 🟢 |
| First Game | 90 % | 🟢 |
| Documentation d'entrée | 90 % | 🟢 |
| Diagnostic d'installation | 85 % | 🟢 |
| Tests du cœur | 92 % | 🟢 |
| Pilot externe | 80 % | 🟡 (sécurité déjà acquise, reste l'isolation des tests) |
| Export | 72 % | 🟡 |
| Format de projet | 25 % | 🔴 |
| Protection des sauvegardes | 55 % | 🟡 (dirty-flag + modale déjà faits) |
| Multijoueur local graphique | 55 % | 🟡 |
| Préparation bêta | 78–82 % | 🟡 proche |

---

## Ce qui est maintenant acquis

### Phase A — Onboarding : ✅ presque terminée

Les éléments suivants sont intégrés :

- [QUICKSTART.md](../QUICKSTART.md) ;
- [First Game](../examples/first_game/README.md) ;
- [tutoriel de dix minutes](FIRST_GAME.md) ;
- modèle mental du moteur ([MENTAL_MODEL.md](MENTAL_MODEL.md)) ;
- limites connues ([KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md)) ;
- scénario de test ([TEST_SCENARIO.md](TEST_SCENARIO.md)) ;
- formulaire de retour ([TEST_FEEDBACK_FORM.md](TEST_FEEDBACK_FORM.md)) ;
- script de diagnostic (`scripts/doctor.sh`) ;
- exemple volontairement cassé.

Une nouvelle personne dispose désormais d'un chemin clair :

```text
Cloner
→ exécuter doctor.sh
→ lancer l'éditeur
→ ouvrir First Game
→ jouer
→ ajouter un objet
→ écrire un script
→ sauvegarder
```

C'est un changement qualitatif majeur.

### Phase B — Petite démo reproductible : ✅ presque terminée

`examples/first_game` est un dossier de données pur (aucun `.rs`) :

- `scene.json` (version 2, 10 objets : Sol, Joueur à `controller`, 3 Caisses,
  Cube tournant scripté, Zone d'éveil à `trigger`, 3 Pièces collectibles) ;
- aucun asset externe (pas d'`imported`, pas de texture) — primitives uniquement ;
- 2 scripts Lua inline, dupliqués en copies lisibles `scripts/*.lua` ;
- documenté et illustré (`preview.png`) ;
- couvert par 4 tests dans
  [tests/first_game_example.rs](../tests/first_game_example.rs) : chargement conforme au
  README, vraie boucle Play (le cube tourne, une pièce se ramasse), round-trip
  save/load, synchronisation copies lisibles ↔ scripts inline.

**Limite** : c'est encore un fichier `scene.json` ouvert via « 📂 Ouvrir… », pas un
projet reconnu par RusteeGear.

### Phase C — Automatisation externe : 🟡 avancée

Pilot ([PILOT.md](PILOT.md), implémentation [src/pilot.rs](../src/pilot.rs), client CLI
[src/bin/pilot.rs](../src/bin/pilot.rs)) permet d'automatiser : état de l'application,
Play/Pause/Stop/Step, entrées joueur, console, Lua, scène, captures, logs. Protocole
JSON-lines sur `127.0.0.1:4517` (par défaut), verbes routés par `dispatch()`
([pilot.rs:184](../src/pilot.rs)).

C'est une excellente base pour tester automatiquement le tutoriel et les exports.

**Travail restant** : isoler les 4 tests TCP derrière le gate `net_tests` existant et
prouver par test le refus des connexions non locales (Sprint 1).

---

## Cartographie du code (repères pour l'exécution)

Résumé de la session d'analyse approfondie — les détails opérationnels sont dans chaque
sprint de [SprintAudit12h24.md](SprintAudit12h24.md).

### Sauvegarde

- Cœur unique : `Scene::save` — `serde_json::to_string_pretty` + `fs::write` **direct,
  non atomique** ([scene/persistence.rs:125](../src/scene/persistence.rs)).
- Appelé par `AppState::save_to` ([app/persistence.rs:185](../src/app/persistence.rs)) ;
  chemin par défaut du quick-save : `~/motor3derust_scene.json`
  (`scene_path()`, [app/mod.rs:1625](../src/app/mod.rs)).
- Menu Fichier : « Enregistrer » / « Enregistrer sous… » via `rfd::FileDialog`
  ([menus.rs:158-172](../src/editor/menus.rs)).
- Dirty-flag et modale de fermeture : **déjà en place** (voir Corrections ci-dessus).
- Autosave, backup, écriture atomique : **absents** (le seul `fs::rename` du dépôt est
  un renommage d'asset sans rapport).

### Ouverture et assets

- Ouverture : `rfd` → `AppState::load_from` en thread de fond
  ([app/persistence.rs:227](../src/app/persistence.rs)) → `Scene::load` + `migrate()` +
  `reload_imported`. Variante synchrone `load_from_blocking` pour Pilot.
- **Aucune liste de projets récents**, aucun chemin de projet mémorisé.
- Les assets ne sont **pas relatifs à la scène** : résolution par schémas d'URI
  (`asset://`, `asset-id://`, `bundle://`, `user://`) dans le résolveur central
  `read_bytes` ([assets.rs:363](../src/assets.rs)), ancrés sur les dossiers globaux
  `~/.motor3derust/{assets,save}` (`assets_dir()` [assets.rs:243](../src/assets.rs)),
  avec garde anti-traversée `safe_join` ([assets.rs:314](../src/assets.rs)).

### « Nouveau projet »

- Le wizard actuel ([windows.rs:2143-2199](../src/editor/windows.rs),
  `new_project_wizard_window`) est un simple sélecteur de 3 templates (Scène vide /
  Démo contrôleur / Niveau de combat). Aucun nom, aucun dossier, aucune persistance.
- Seules préférences persistées : `Settings` (`~/.motor3derust/settings.json`,
  [settings.rs](../src/app/settings.rs)) et `BuildConfig`
  (`~/.motor3derust/build_config.json` + presets,
  [build_config.rs](../src/app/build_config.rs)). Le concept de projet est à créer
  de zéro.

### Export et Release

- Panneau « Build & Export » : [src/editor/export.rs](../src/editor/export.rs).
  `ExportPanel::start()` (l.127-186) écrit la scène ouverte dans
  `assets/player_scene.json`, `bundle_scene_json()` (l.816) **vide et régénère
  `assets/bundle/`** (assets zstd, chemins réécrits en `bundle://`), puis `run()`
  (l.940) lance le script de la cible avec `PLAYER_BUILD=1` et
  `OUTPUT_NAME=cfg.safe_name()`.
- ⚠ **Piège critique versionné** : `assets/player_scene.json` est le vrai jeu MMORPG
  servi en ligne ; chaque export l'écrase en place, ainsi que `assets/bundle/`. Un
  garde-fou de test existe
  (`the_embedded_scene_ships_monsters_and_the_fire_button`,
  [fireball.rs:1052](../src/app/fireball.rs)) mais le danger pour le dépôt demeure.
- [packaging/build_dmg.sh](../packaging/build_dmg.sh) a deux modes : normal
  (`OUTPUT_NAME=RusteeGear` → `target/release/bundle/dmg/RusteeGear.dmg`, **bundle
  éditeur**) et export (`OUTPUT_NAME` autre → identité réécrite au PlistBuddy →
  `target/export/${OUTPUT_NAME}.dmg`, **jeu Player**).
- [release.yml](../.github/workflows/release.yml) job macos appelle le mode **normal**
  puis attache `Motor3DeRust.dmg` — nom qui n'existe pas (le script produit
  `RusteeGear.dmg`). Le DMG publié, une fois corrigé, contiendra donc **l'éditeur** ;
  un DMG First Game demande un second appel en mode export.

### Réseau et serveur

- Serveur headless : [src/bin/server.rs](../src/bin/server.rs) —
  `DEFAULT_ADDR = "127.0.0.1:7777"` (l.54), surcharge par env `RUSTEEGEAR_SERVER_ADDR`
  (pas d'argument CLI), tick 16 ms, transport WebSocket
  (`NetServer::start`, [server_loop.rs:222](../src/net/server_loop.rs)), multi-salons.
- `PROTOCOL_VERSION = 6` ([protocol.rs:42](../src/net/protocol.rs)) — couplage
  client/serveur : tout déploiement doit être coordonné.
- L'éditeur sait déjà **se connecter** (fenêtre multijoueur,
  [windows.rs:1083](../src/editor/windows.rs), action `connect_to_server`
  [editor/mod.rs:369](../src/editor/mod.rs), défaut `wss://ws.loicberthod.ch`) mais ne
  sait pas **lancer** un serveur local.

---

## Les trois blocages structurants

### 1. Aucun véritable format de projet — 🔴 principal chantier

**Vérifié sur le dépôt** : aucune occurrence de `ProjectManifest` ni de
`project.rusteegear.json` dans `src/`. Le seul `PROJECT_ROOT` existant est la constante
`env!("CARGO_MANIFEST_DIR")` de [src/editor/export.rs:14](../src/editor/export.rs) —
c'est la racine du **dépôt du moteur**, pas celle d'un projet utilisateur. Recherches
`recent|manifest|.rgproj` : aucun résultat pertinent.

Il manque toujours : `ProjectManifest`, `project_root` (au sens projet utilisateur),
`project.rusteegear.json`, projets récents, résolution des assets relativement au projet
(aujourd'hui : dossiers globaux `~/.motor3derust/`, voir Cartographie).

First Game s'ouvre comme un fichier (`examples/first_game/scene.json`). L'objectif reste :

```text
examples/first_game/
├── project.rusteegear.json
├── scenes/main.scene.json
├── scripts/
├── models/
├── textures/
├── audio/
└── build.json
```

Tant que cette structure n'existe pas, RusteeGear reste principalement un éditeur de
scènes à l'intérieur de son propre dépôt.

### 2. Release macOS toujours incohérente

**Vérifié sur le dépôt** :

- [packaging/build_dmg.sh:41](../packaging/build_dmg.sh) produit
  `target/release/bundle/dmg/RusteeGear.dmg` en mode normal ;
- [.github/workflows/release.yml:27](../.github/workflows/release.yml) publie
  `target/release/bundle/dmg/Motor3DeRust.dmg`.

**Conséquence** : le DMG est correctement construit puis introuvable au moment de créer
la Release GitHub. Précision issue de l'analyse : ce DMG est le bundle **éditeur** ; les
livrables « First Game » (Player) demandent le mode export du script
(`PLAYER_BUILD=1 OUTPUT_NAME=…` → `target/export/`), et l'export **écrase
`assets/player_scene.json`** s'il passe par le panneau de l'éditeur — en CI il faudra
produire le Player sans casser la scène versionnée (voir Sprint 2).

### 3. Sauvegarde utilisateur encore insuffisamment protégée

La sauvegarde et le rechargement fonctionnent (First Game le prouve par test), et le
dirty-flag + la confirmation de fermeture existent déjà (voir Corrections).

**Manquent réellement** ([scene/persistence.rs:125](../src/scene/persistence.rs) est un
`fs::write` direct) :

- fichier temporaire + remplacement atomique ;
- backup de la version précédente ;
- autosave ;
- restauration après crash.

Pour une bêta d'éditeur, c'est plus important qu'ajouter de nouveaux assets.

---

## Conclusion

À 12 h 24, RusteeGear est dans son meilleur état observé jusqu'ici : dépôt propre,
branche synchronisée, onboarding intégré, First Game validé, cœur technique vert,
documentation utilisable, automatisation Pilot disponible. L'analyse approfondie a en
outre montré que deux chantiers supposés sont **déjà partiellement faits** (dirty-flag,
sécurité du pont Pilot) et que l'isolation des tests Pilot peut **réutiliser** la feature
`net_tests` existante au lieu d'en créer une.

Le cap suivant n'est plus de créer la démonstration : **elle existe**. Le cap suivant est :

```text
Scène exemple
→ véritable projet
→ Release téléchargeable
→ sauvegarde sûre
→ testeur extérieur
```

Les deux actions les plus urgentes restent minuscules : isoler les tests TCP Pilot et
corriger le chemin du DMG. Le plan détaillé, sous-phase par sous-phase avec repères de
code, est dans [SprintAudit12h24.md](SprintAudit12h24.md).
