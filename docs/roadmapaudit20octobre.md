# Roadmap post-audit (2026-10-20)

*Suite à l'audit complet du 2026-10-20 sur `f38e667` (main = origin/main = water/main, copie de
travail propre). Mesures faites par exécution : `cargo test` (1133 verts, 8 ignorés),
`cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --check`,
`scripts/check_unwrap_budget.py`, `gh run view` job par job, `smoke_vps` contre le serveur public,
démo web ouverte dans le navigateur (GitHub Pages et water.loicberthod.ch). Même format que
[roadmapaudit3septembre.md](roadmapaudit3septembre.md) : vagues par valeur / coût, coûts en taille
de t-shirt (S ≈ ≤ 1 h, M ≈ ½ journée, L ≈ 1-2 jours, XL > 2 jours).*

## Constat d'ensemble

Le produit tourne : le serveur public répond (smoke wss OK, 27 monstres, projectiles), la démo web
se connecte et joue, les certificats TLS sont valides jusqu'en décembre, 1133 tests passent en
local. Mais **la CI est rouge sur chaque push depuis le 14 septembre** (40 runs, dernier vert
`ffe81d3`). L'étape `cargo fmt --check` échoue en premier, donc Clippy, les tests et le budget
unwrap n'ont plus tourné en CI depuis 5 semaines. Derrière le formatage, trois garde-fous sont
eux aussi cassés : clippy `-D warnings` (7 erreurs), budget unwrap (25 sites pour 13 autorisés),
`cargo audit` (RUSTSEC-2026-0285, rustls 0.23.43). C'est la deuxième fois (après juillet-septembre)
que la CI reste rouge des semaines sans que personne le voie.

Côté sécurité, deux failles sont exploitables aujourd'hui en production : **le serveur public
n'accepte que 4 joueurs wss au total** (limite par IP comptée sur 127.0.0.1 derrière Caddy), et
**Lua n'est pas isolé** dans l'éditeur (`io`, `os`, `require` chargés).

Côté architecture, la dette repart à la hausse : 124 290 lignes (+68 % depuis juillet),
`AppState` à 127 champs (76 le 3 septembre), `editor/mod.rs` à 5342 lignes, et les démos
(Rivière, MMORPG, HerRoad, Ball) fuient jusque dans `gfx` et `runtime::physics`.

## Suivi (2026-10-20)

| Lot | Commit | État |
|---|---|---|
| 0.1 fmt | `fdbc0f1` | ✅ |
| 0.2 clippy | `1afaf7d` | ✅ |
| 0.3 budget unwrap | `7fb0ec4` | ✅ (18 sites, tous justifiés) |
| 0.4 rustls 0.23.46 + ruzstd 0.9.1 (nouvel avis RUSTSEC-2026-0344 du 11/10) | `de47beb` | ✅ |
| 0.5-0.6 audit bloquant (`.cargo/audit.toml`) + badge cliquable (le badge existait déjà) | `25174d4` | ✅ CI 9/9 verte (run 39370920402), Pages verte |
| 1.1-1.4 serveur : IP via X-Forwarded-For, Join en 5 s, file bornée, Join unique | `1674e9d` | ✅ code + 5 tests ; **déploiement VPS à faire** |
| 1.5 bac à sable Lua natif + web, budget de lignes, tas plafonné | `ae5f16f` | ✅ ; web redéployé par Pages, **water.loicberthod.ch à reconstruire** |
| 1.6 jeton du pont pilot | `dccb263` | ✅ |
| 1.7-1.8 relais Ball par salon, limites, repli `ws://` après échec TLS seulement | `b99229b` | ✅ code, vérifié en local ; **relais VPS à redéployer, APK Ball et Ball-PC à reconstruire** |

Déploiement du 2026-10-20 (`a4fa337`, `scripts/deploy_vps.sh`) : serveur et relais Ball
reconstruits, écoute `127.0.0.1` (override systemd `listen-local.conf`), smoke wss OK. Vérifié en
production : 4 sockets ouvertes depuis une IP → la 5ᵉ de la même IP est refusée, une 6ᵉ depuis une
autre IP (le VPS lui-même, via Caddy) est acceptée ; les sockets muettes sont coupées à ~5 s ;
relais Ball en salon (`quest_sim` + `ball_pc` en wss), un autre salon reste isolé.
water.loicberthod.ch reconstruit (`wasm-bindgen` 0.2.127 exigé par le lockfile ; sauvegarde
`/mnt/data/water/dist.bak` = build du 24/09). Rendu web non vérifié visuellement (navigateur
intégré masqué) ; rendu natif de Rivière vérifié. Le serveur n'écrit aucun log applicatif dans
journald (`RUST_LOG` absent de l'unité) : à ajouter pour diagnostiquer en production.

## Règle

Vague 0 avant tout nouveau commit de fonctionnalité. Ajouter un badge CI au README et ne plus
pousser sur `main` avec une CI rouge.

---

## Vague 0 — CI verte (S, ~1 h)

| # | Action | Critère de fait |
|---|---|---|
| 0.1 | `cargo fmt --all` (40 fichiers : `racing/*`, `xr/ball/*`, `app/race.rs`, `scene/demos/herroad.rs`, `riviere.rs`, examples…) | `cargo fmt --check` muet |
| 0.2 | Clippy : 2× `is_multiple_of`, 1 `return` inutile, 1 `too_many_arguments`, 3 `needless_range_loop` (`COURSES`, `GEARS`, `c`), + `collapsible_if` dans `examples/probe_riviere_walk_only.rs:32` | `cargo clippy --all-targets --all-features -- -D warnings` vert |
| 0.3 | Budget unwrap : 12 sites hors liste (`gfx/multiview.rs:50,51,54,77`, `bin/ball_pc.rs:643,653`, `bin/ball_relay.rs:13,17`, `xr/ball/net.rs:158,161`, `xr/ball/world.rs:437`, `xr/hello.rs:568`). Durcir en `Result` ou whitelister avec commentaire | `python3 scripts/check_unwrap_budget.py` vert |
| 0.4 | `cargo update -p rustls` (0.23.43 → ≥ 0.23.45, contraintes déjà satisfaites) | `cargo audit` sans vulnérabilité |
| 0.5 | `audit.toml` qui documente bincode (RUSTSEC-2025-0141) et ttf-parser (RUSTSEC-2026-0192), puis retirer `continue-on-error` du job audit | Job audit bloquant et vert |
| 0.6 | Badge CI en tête du README | Badge visible |

## Vague 1 — Sécurité exploitable en production (S à M, ~½ journée)

| # | Action | Coût |
|---|---|---|
| 1.1 | **Limite par IP derrière Caddy** : `MAX_CONNECTIONS_PER_IP = 4` (`net/server_loop.rs:70`) est compté sur `peer.ip()`, toujours 127.0.0.1 via Caddy. Résultat : 4 joueurs wss max sur tout le serveur, et 4 sockets muettes bloquent tout le monde. Lire `X-Forwarded-For` via `accept_hdr_async` quand le pair est loopback ; écouter sur `127.0.0.1:7777` au lieu de `0.0.0.0` (le port est filtré de l'extérieur, mais autant ne pas en dépendre). Redéployer le VPS | S |
| 1.2 | Délai de 5 s sur le handshake et le premier `Join` (`server_loop.rs:496-501`, aucun timeout aujourd'hui) | S |
| 1.3 | File d'envoi bornée par client (`unbounded_channel`, `server_loop.rs:544`) : ~256 messages, déconnexion si pleine | S |
| 1.4 | Ignorer les `Join` après le premier (`server.rs:349-378`) : une connexion peut occuper les 16 salons ou lancer 120 vérifications Firebase/s | S |
| 1.5 | **Bac à sable Lua** : `Lua::new()` (`app/mod.rs:1635`) charge `io`, `os`, `package`, `load`, `dofile`. Passer à `Lua::new_with(TABLE \| STRING \| MATH \| COROUTINE \| UTF8)`, retirer `load`/`dofile`/`loadfile`/`require`, fournir `os.clock`/`os.time` si utilisés, ajouter `set_memory_limit` et un quota d'instructions. Idem côté rilua | S + M (vérifier les scripts existants) |
| 1.6 | Pont pilot (`127.0.0.1:4517`, verbe `lua`) : couper à la première ligne invalide ou commençant par une méthode HTTP, exiger un jeton aléatoire. Sinon une page web visitée pendant une session `--pilot` peut exécuter du Lua (et, avec 1.5 non fait, du shell) | S |
| 1.7 | Relais Ball : code de salle affiché dans le casque (aujourd'hui un nouveau venu éjecte le joueur VR), message max 64 Kio, canal borné, plafond de connexions | M |
| 1.8 | Repli `ws://` en clair (`xr/ball/net.rs:21`) : diagnostiquer l'échec wss avec le message désormais affiché, puis retirer le repli. Il transmet en clair les positions de tête et de mains, ce que contredit `VR_PUBLICATION.md:53` | S |

## Vague 2 — Infra, release et docs à jour (S à M)

| # | Action | Coût |
|---|---|---|
| 2.1 | Versionner le Caddyfile et les unités systemd (`rusteegear-server`, `ball-relay`) dans `packaging/vps/`. Aujourd'hui ils n'existent que sur le VPS | S |
| 2.2 | Secrets release Android : `gh secret list` est vide. Générer le keystore, définir `RUSTEEGEAR_KEYSTORE_PASS` et `RUSTEEGEAR_KEYSTORE_B64`, sauvegarder le keystore hors dépôt. Sans `_B64`, chaque release génère un nouveau keystore et l'APK ne se met plus à jour | S |
| 2.3 | Workflows : `permissions: contents: read` par défaut, `write` seulement à l'étape de publication ; épingler les actions par SHA (`dtolnay/rust-toolchain@master` est une branche mobile) ; passer `actions/checkout@v4` / `cache@v4` en v5 (Node 20 déprécié) | S |
| 2.4 | Roadmap VR : ajouter un §22 « Test casque du 24/09 » (sphère blanche, saccades Rivière/RageQuit, lancer Ball, relais injoignable), repasser P2 Performance en ❌, P0 en ✅ | S |
| 2.5 | README : section VR (P10, Ball, `quest_sim`, `ball_pc`, `ball_relay`), HerRoad ; README-water : prod réelle `0ab912b`, route `/ball` ; HERROAD.md : mode VR, `HERROAD_PADLOG`, page web | S |
| 2.6 | `docs/STATUS.md` (actif / clos / abandonné) et déplacement des ~33 sprints/roadmaps/audits clos dans `docs/archive/`. Marquer `roadmapExportWindowsSteam11septembre.md` comme abandonnée ou la relancer | M |
| 2.7 | Retirer l'IP du VPS de `scripts/deploy_vps.sh:30`, `docs/sprintreflecion.md:650` (avec utilisateur et nom de clé SSH), `docs/ROADMAP_SPRINTS.md:2311`, `docs/reflexion.md:17`, `src/app/network_client.rs:34` | S |
| 2.8 | Versionner les règles Firebase RTDB et restreindre la clé web dans la console GCP | S |
| 2.9 | Web : le `.wasm` pèse ~25 Mo compressé ; water.loicberthod.ch le sert en `cache-control: no-cache` (re-validation à chaque visite). Ajouter un nom haché + cache long, ou au moins un ETag vérifié | S |
| 2.10 | Nettoyer les worktrees `.claude/worktrees/lucid-tharp-*` et `trusting-tereshkova-*` (HEAD détachées de septembre) et `target/` (138 Go) | S |

## Vague 3 — Stopper la régression d'architecture (M à L)

| # | Action | Coût |
|---|---|---|
| 3.1 | Sortir `apply_editor_actions` et le chargement des démos de `gfx/renderer/frame.rs` (1167-1702) : le renderer ne dépend plus de `editor` ni de `app::load_*_demo` | M |
| 3.2 | Sortir le terrain MMORPG de `runtime/physics/build.rs:317-370` et `gfx/mesh.rs:401-622` : terrain paramétré décrit dans la scène | M |
| 3.3 | Regrouper `AppState` (127 champs) : un `Option<GameState>` par jeu (`race`, `boss`, `boss2`, `rage_*`, `pose`, `hands`, `vr_*`), comme les 13 lots de septembre | L |
| 3.4 | Découper `lib.rs` (2699 lignes) en `platform/{desktop,web,android}.rs` + `app_shell.rs` ; sortir les ponts JS Mouvéo | M |
| 3.5 | `gfx/multiview.rs:45-77` réécrit le WGSL par recherche de texte et panique si le motif change : points d'insertion explicites ou passage par naga | M |
| 3.6 | Feature-gater `xr::ball` et `racing` (aujourd'hui compilés pour wasm et iOS) | S |
| 3.7 | `// SAFETY:` sur les 21 blocs `unsafe` inline de `xr/hello.rs` ; remplacer le `&'static Physics` de `scripting_web.rs:209` par un accès par closure | S |

## Vague 4 — Le moteur au service des jeux (L à XL, à arbitrer)

| # | Action | Coût |
|---|---|---|
| 4.1 | Registre de démos (`trait Demo { build, on_load, world }`) qui remplace les 4 aiguillages en dur (`editor/mod.rs:526-716`, `menus.rs`, `console.rs`, `lib.rs:1984`) | L |
| 4.2 | Décider : HerRoad et Ball deviennent des crates `games/*`, ou leurs règles passent en Lua. Aujourd'hui Ball réimplémente rendu, physique et réseau à côté du moteur | L |
| 4.3 | Renderer allégé « mobile/VR » : le test casque montre que le Renderer complet saccade sur Quest | XL |
| 4.4 | Trait `ScriptBackend` + table déclarative des fonctions hôtes pour générer les deux backends Lua (parité actuelle totale, mais tenue à la main) | L |
| 4.5 | Migrer bincode 1.3.3 → postcard ou bincode 2 avec bump de `PROTOCOL_VERSION` et redéploiement coordonné VPS/web/Android/Quest | M-L |
| 4.6 | Tests : relais Ball (intégration), `editor` et `gfx` (ratio 0,11), binaires sans test (`glbviewer`, `ball_pc`, `pilot`) | M |

## Non vérifié par cet audit

- Comportement au vrai casque Quest (seuls les messages de commit du 24/09 en parlent).
- Règles de sécurité Firebase RTDB (non versionnées).
- La saturation du serveur public par 4 sockets muettes n'a pas été testée contre la production
  (déduite du code et de la config Caddy) ; à reproduire en local avant correctif.
- Builds iOS, Android et Windows non relancés en local (verts en CI le 24/09).
