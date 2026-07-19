# Pont de pilotage « pilot » — télécommander l'application vivante

> `--pilot` + `cargo run --bin pilot` : piloter l'éditeur ou le player **en
> train de tourner** depuis un terminal, un script, une CI ou un agent (Claude) —
> Play/Stop, éval Lua, inspection de scène, injection d'entrées, captures
> d'écran, lecture des logs. Livré le 19 juillet 2026 (audit C1, gestion
> d'erreur Lua observable de l'extérieur).

## Pourquoi ce pont existe

La fenêtre RusteeGear est du winit/wgpu pur : **aucun arbre d'accessibilité**
(pas d'accesskit), donc aucun outil de contrôle d'écran (computer-use, tests UI
génériques) ne peut cliquer ses boutons ni lire son état. Les audits « manuels
dans l'éditeur » se faisaient donc en tests headless indirects — lourds, et
aveugles sur ce que l'application *vivante* fait réellement.

Plutôt que de simuler des clics pixel par pixel, le pont expose **la sémantique**
directement : les mêmes commandes que la fenêtre Console de l'éditeur, plus
l'éval Lua, les captures et l'état de jeu — par TCP local, en JSON ligne à ligne.

## Démarrage

```bash
# Terminal 1 : l'application, pont activé (opt-in, jamais actif par défaut)
cargo run -- --pilot                # éditeur, port 4517
cargo run -- --pilot=5000           # port au choix
RUSTEEGEAR_PILOT=1 cargo run -- --player   # marche aussi en mode player

# Terminal 2 : le client
cargo run --bin pilot -- state
```

Au démarrage, l'application annonce en clair :
`Pont de pilotage actif sur 127.0.0.1:4517 — toute commande locale (console, Lua, captures) est acceptée sur ce port.`

## Les verbes du client `pilot`

**État & observation**

| Commande | Effet | Exemple |
|---|---|---|
| `state` | État complet : Play/pause, objets, vie, position joueur, arme, score, manche, victoire/défaite, pose caméra, réseau | `pilot state` |
| `player` | Le joueur en détail : index, position, arme, vie, score, manche, chrono, gagné/perdu | `pilot player` |
| `scene` | Dump compact : index, nom, position, visibilité, tag, script par objet | `pilot scene` |
| `logs` | Les 500 dernières lignes de log en mémoire (`log_buffer`) | `pilot logs` |
| `screenshot <chemin.png> [l] [h]` | Rendu hors-écran de l'état courant (défaut 800×600) | `pilot screenshot /tmp/s.png` |

**Jouer**

| Commande | Effet | Exemple |
|---|---|---|
| `console <cmd>` | Console développeur : `play`, `pause`, `stop`, `step [n]`, `timescale`, `tp`, `select`, `spawn`, `health`, `weapon`, `demo`, `restart`, `undo`, `redo`, `music`, `sfx`, `net_stats` | `pilot console play` |
| `move <turn> <thrust> [ms] [jump\|attack\|fire]` | Tient l'entrée pendant l'équivalent de `ms` (défaut 500) de **temps simulé** (pas fixes exécutés immédiatement) puis relâche — déterministe, instantané, insensible à l'App Nap. La réponse donne la position avant/après | `pilot move 0 1 1000 fire` |
| `console restart` | Rejoue la manche (équivalent « Rejouer ») — une victoire/défaite **gèle toute la simulation** jusqu'à ce geste | `pilot console restart` |
| `input <turn> <thrust> [jump] [attack] [fire]` | Pose l'état des entrées sans le relâcher (assignation absolue) | `pilot input 0 1 jump` |
| `step [n]` | N pas fixes de 1/60 s, déterministes et instantanés (en pause) | `pilot step 60` |
| `weapon <0-2>` | Change d'arme (Boule de feu / Éclair / Boulet) | `pilot weapon 2` |

**Caméra & captures cadrées**

| Commande | Effet | Exemple |
|---|---|---|
| `camera [target x y z] [yaw °] [pitch °] [distance d] [follow on\|off] [frame]` | Pose la caméra (champs fournis seulement) et renvoie la pose. `follow off` coupe le suivi joueur — indispensable pour cadrer en Play. `pitch` positif = vue plongeante | `pilot camera target 0 3 0 distance 10 pitch 25 follow off` |

**Créer & éditer**

| Commande | Effet | Exemple |
|---|---|---|
| `object add <cube\|sphere\|plane\|cylinder\|capsule\|terrain>` | Ajoute une primitive (annulable), renvoie son index | `pilot object add cube` |
| `object set <i> <champ> <valeurs…>` | Modifie un champ : `pos/rot/scale/color x y z`, `visible on\|off`, `physics none\|static\|dynamic\|kinematic`, `metallic/roughness/emissive v`, `hp n`, `name/tag/script texte` | `pilot object set 983 color 1 0 0` |
| `object get <i>` | L'objet complet (JSON sérialisé) | `pilot object get 983` |
| `object delete/duplicate <i>` | Supprime / duplique (annulables) | `pilot object delete 983` |
| `object damage <i> [n]` | Inflige n dégâts à un objet attaquable, dit s'il est tué | `pilot object damage 42 3` |
| `object import <chemin.glb>` | Import glTF (asynchrone — vérifier via `scene`) | `pilot object import assets/models/creature70.glb` |

**Scène, options, réseau**

| Commande | Effet | Exemple |
|---|---|---|
| `scene save [chemin]` / `scene load <chemin>` / `scene new` | Sauver / charger / vider la scène. `load` est **synchrone** (répond avec le nombre d'objets réellement chargés) | `pilot scene save /tmp/s.json` |
| `demo <nom>` | Charge une démo : mmorpg, gameplay, controleur, tower, temple, zombies, mobile, roguelike, brawl, boss, escorte, components, hameau | `pilot demo zombies` |
| `undo` / `redo` | Annuler / rétablir | `pilot undo` |
| `options [music v] [sfx v] [timescale v] [reduce_shake on\|off] [hud] [map] [settings] [multi]` | Volumes, vitesse du temps, toggles UI (fenêtre réelle requise pour les toggles) | `pilot options music 0.5 timescale 2` |
| `net connect <url> [pseudo] [classe] [salon] [mode]` / `net disconnect` | Rejoindre un serveur (classes : assaut/eclaireur/soutien ; modes : vagues/survie/escorte/boss) | `pilot net connect ws://127.0.0.1:7777 Bot soutien`
| `lua <src>` | Évalue du Lua sur l'instance partagée du moteur | `pilot lua "return #find_tag('ennemi')"` |
| `raw <json>` | Requête JSON brute, telle quelle (échappatoire) | `pilot raw '{"cmd":"state"}'` |

`pilot --port N <verbe>` cible un port non standard. Codes de sortie : 0 = ok,
1 = erreur applicative (affichée sur stderr), 2 = usage.

## Session type — jouer et créer, sans toucher la fenêtre

```bash
cargo run -- --pilot &                        # l'éditeur s'ouvre sur le hameau
pilot object add cube                         # → index 983
pilot object set 983 pos 0 3 0
pilot object set 983 color 1 0 0
pilot object set 983 emissive 2               # cube rouge incandescent
pilot camera target 0 3 0 distance 10 pitch 25 follow off
pilot screenshot /tmp/creation.png            # vérifier à l'œil (via Read)
pilot console play                            # Play (snapshot pris)
pilot move 0 1 1000 fire                      # avance 1 s en tirant, puis relâche
pilot player                                  # la position a bougé, score éventuel
pilot console pause && pilot step 120         # 2 s de simulation, déterministes
pilot console stop                            # scène restaurée
pilot demo zombies                            # changer de niveau entier
pilot undo                                    # ou revenir en arrière
```

## Protocole (pour écrire son propre client)

TCP `127.0.0.1:4517`, **une requête JSON par ligne, une réponse JSON par ligne** :

```
→ {"cmd": "console", "arg": "play"}
← {"ok": true, "result": "Play démarré"}
→ {"cmd": "lua", "src": "return 1 +"}
← {"ok": false, "error": "syntax error: ..."}
```

Requêtes : `{"cmd": "console", "arg": …}` · `{"cmd": "lua", "src": …}` ·
`{"cmd": "logs"}` · `{"cmd": "scene"}` · `{"cmd": "state"}` · `{"cmd": "player"}` ·
`{"cmd": "input", "turn": …, "thrust": …, "mx": …, "my": …, "jump": bool, "attack": bool, "fire": bool, "heal": bool}` ·
`{"cmd": "screenshot", "path": …, "width": …, "height": …}` ·
`{"cmd": "camera", "target": [x,y,z], "yaw": °, "pitch": °, "distance": d, "follow": bool, "frame": bool}` ·
`{"cmd": "object", "op": "add|import|get|set|delete|duplicate|damage", "index": i, "kind": …, "path": …, "patch": {…}, "amount": n}` ·
`{"cmd": "scene_cmd", "op": "save|load|new", "path": …}` ·
`{"cmd": "options", "music": v, "sfx": v, "timescale": v, "reduce_shake": bool, "hud": bool, "map": bool, "settings_overlay": bool, "multiplayer_window": bool}` ·
`{"cmd": "net", "op": "connect|disconnect", "url": …, "name": …, "class": …, "room": …, "objective": …}`.
La réponse est toujours du JSON valide (`ok`/`result` ou `ok`/`error`) — jamais
de panique sur une entrée malformée, même contrat que la Console.

## Architecture

```
client (pilot, nc, agent)          thread pilot-accept        thread principal (winit)
        │  TCP 127.0.0.1:4517            │                          │
        │ ──── ligne JSON ─────▶  thread pilot-conn ── mpsc ──▶  about_to_wait
        │                                │      └─ waker ──▶ EventLoopProxy (réveil immédiat)
        │ ◀─── ligne JSON ────── réponse ◀───── canal retour ──── PilotServer::poll
```

- **Tout le traitement a lieu sur le thread principal** (`PilotServer::poll`,
  drainé dans `about_to_wait` comme le hot-reload d'assets) : seul détenteur
  légitime de `AppState`/`Renderer`, zéro état partagé entre threads — seules
  des lignes de texte transitent par le canal.
- **Latence** : chaque requête donne un coup de coude à la boucle d'événements
  (`EventLoopProxy::send_event`) — sans ça, l'application au repos dort 60 ms
  entre deux tours. Mesuré : ~13 ms par commande, connexion comprise.
- **Anti-famine** : au plus 32 requêtes traitées par tour de boucle — un client
  qui inonde le pont ne peut pas geler le rendu ; l'excédent attend le tour
  suivant (délai de réponse max : 10 s par requête).
- **Captures** : `Renderer::screenshot_png` → `render_scene_headless` (le même
  chemin que les golden tests), donc disponible aussi depuis le renderer
  fenêtré — avec resolve MSAA et swizzle BGRA→RGBA (surface macOS/Metal).

Code : [`src/pilot.rs`](../src/pilot.rs) (serveur + dispatch),
[`src/bin/pilot.rs`](../src/bin/pilot.rs) (client CLI),
[`tests/pilot_bridge.rs`](../tests/pilot_bridge.rs) (test d'intégration TCP
headless, sans GPU — tourne en CI).

## Sécurité

- **Jamais actif par défaut** : l'éval Lua est de l'exécution de code
  arbitraire dans le process. Activation explicite (`--pilot` /
  `RUSTEEGEAR_PILOT`), annoncée en clair dans les logs au démarrage.
- Écoute **`127.0.0.1` uniquement** — jamais exposé au réseau. Pour un accès
  distant volontaire, passer par un tunnel SSH, pas par un bind public.
- `RUSTEEGEAR_PILOT=0` et l'absence de flag désactivent tout (l'état de tous
  les builds distribués).

## Fenêtre masquée / occultée (App Nap) — ce qui marche et ce qui dort

Audité en conditions réelles le 19 juillet 2026 : quand la fenêtre est masquée
(Cmd+H) ou complètement recouverte, macOS met le process en **App Nap** — les
redraws passent à ~1 Hz et les timers du process sont étranglés. Conséquences,
et ce que le pont garantit malgré tout :

- **Fiable même masqué** : toutes les requêtes (l'arrivée d'une commande réveille
  la boucle), `move`/`step` (temps **simulé**, exécuté immédiatement — le front
  d'entrée en Play est déclenché au besoin par `advance_steps`), `scene load`
  (synchrone), `screenshot`, `object`/`camera`/`options`/`net`.
- **Au ralenti masqué** : le temps **réel** du jeu (une entrée posée par `input`
  et « laissée courir » n'avance qu'au rythme des rares réveils) et les
  chargements asynchrones (`object import`, `load_from` de l'UI). Pour un
  gameplay en temps réel fidèle, garder la fenêtre visible — ou piloter en
  temps simulé (`move`/`step`), ce qui est de toute façon plus reproductible.

## Limites connues (assumées)

- `lua` évalue avec les globales laissées par le dernier tick de scripts :
  `find_tag`, `emit`, `save.*` restent utilisables entre deux ticks, mais
  `raycast`/`overlap_sphere` (fermetures scopées, expirées à chaque fin de
  tick) répondent une erreur explicite hors d'un tick — c'est attendu.
- `input`/`move` posent `key_turn`/`key_thrust`/boutons directement : un vrai
  appui clavier/manette simultané recalculera ces champs et écrasera la valeur
  injectée (les deux sources ne se cumulent pas).
- `screenshot` rend l'état courant **sans** grille, gizmos ni UI egui (même
  cadrage que les goldens) — c'est la scène qu'on capture, pas l'éditeur.
- **Caméra en Play** : le suivi joueur (`camera_follow`) réécrit la caméra
  chaque frame — passer `follow off` avant de cadrer une capture en Play.
- **`object set` en Play** : la physique est construite à l'entrée en Play, un
  corps rigide peut écraser une position posée à la main — la réponse le
  signale ; éditer hors Play pour un effet garanti.
- **Soin (`heal`) en solo** : le moteur ne lit `input_state.heal` qu'en réseau
  (`update_network_heal`) — le poser en solo n'a aucun effet.
- **Qualité rendu / MSAA** : figées à la création du renderer (relues de
  `BuildConfig` à l'entrée en Play pour `render_quality`/`bloom`) — pas de
  réglage à chaud par le pont.
- Pas de flux d'événements poussés (le client interroge) ; si le besoin
  apparaît, le canal existe déjà pour y ajouter un verbe `watch`.

## Étendre le pont

Un nouveau verbe = un bras dans `dispatch()` (`src/pilot.rs`) ; une nouvelle
commande console = un bras dans `run_console_command()`
(`src/app/console.rs`), qui profite du même coup à la fenêtre Console de
l'éditeur. Ajouter le cas correspondant dans `tests/pilot_bridge.rs` : le pont
entier se teste sans GPU.
