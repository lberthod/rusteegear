# Réseau, serveur et déploiement (2026-07-20)

*Sévérité : échelle commune définie dans [00_SYNTHESE.md](00_SYNTHESE.md). R1 (uid
Firebase) a été contre-vérifié ligne par ligne : **confirmé, sans garde-fou existant**.*

## Architecture — points solides

- **Protocole** (`src/net/protocol.rs`) : `PROTOCOL_VERSION = 6`, bincode ; `Join` est le
  variant 0 avec `protocol` en premier champ (un serveur peut toujours lire la version
  d'un client, même après divergence du reste). `JoinRejected` ajouté en fin d'enum.
- **Snapshot** : état complet de tous les joueurs à chaque tick (pas un delta par client
  malgré le nom — doc corrigée Sprint 70). `tick` monotone contre les paquets hors-ordre.
- **Client** (`src/app/network_client.rs`) : prédiction + réconciliation par trajectoire
  (fenêtre 1 s, `CORRECTION_PULL = 0.15`, fin du rubber-banding Sprint 74), fantômes
  interpolés à `RENDER_DELAY = 100 ms`, débit `Input` plafonné à 16 ms.
- **Reconnexion** : watchdog `NET_SILENCE_TIMEOUT = 8 s` (détecte le TCP half-open),
  backoff 1→15 s, 5 tentatives max, `JoinRejected` fatal (pas de boucle), connexion
  déportée en thread (jamais de gel du rendu). Fallback hors-ligne net
  (`RUSTEEGEAR_OFFLINE=1`, reprise de simulation locale des créatures après 2,5 s).
- **Validation d'entrées réelle** : `valid_join_fields`, `sanitize_network_input`
  (NaN/infini, clamp [-1,1], normalisation yaw), cadence de tir validée **côté serveur**.
- **Serveur** (`src/bin/server.rs`) : tick 16 ms, timeout client 60 s, multi-salons,
  progression Firebase via compte serveur dédié ; si le bind échoue → salon local sans
  crash. Garde-fous DoS modestes : 4 connexions/IP, rate limit 120 msg/s + 64 KB/s,
  messages ≤ 64 KB, inbox 4 096.

## Chaîne de déploiement (fragile)

**Aucun script de déploiement, unité systemd ni config Caddy dans le dépôt.** La chaîne
est manuelle, documentée en prose (`docs/reflexion.md` §11) :

```
push GitHub → pull + cargo build --release sur le VPS → restart systemd
(rusteegear-server) → cargo run --example smoke_vps
```

- TLS terminé par Caddy ; clients → `wss://ws.loicberthod.ch`, serveur en clair sur
  `127.0.0.1:7777`.
- `packaging/` ne couvre que les clients (APK/IPA/DMG/web), pas le serveur.
- La carte servie (`assets/player_scene.json`) est **embarquée à la compilation** : un
  déploiement avec une scène non ré-exportée sert une carte obsolète malgré un code à jour.

## Risques priorisés

| # | Sévérité | Risque | Preuve / lieu |
|---|---|---|---|
| R1 | 🟠 Élevé | **`firebase_uid` jamais vérifié** : le serveur insère l'uid tel que fourni par le client et `award_progress` crédite l'XP dessus. N'importe quel client peut réclamer l'uid d'autrui (vol/pollution de progression). `valid_join_fields` ne borne que longueur/charset. | `src/bin/server.rs:286`, `:580+` |
| R2 | 🟠 Élevé | **Couplage de version non automatisé + rayon de panne total** : bump `PROTOCOL_VERSION` ⇒ redéploiement client+VPS ensemble, manuel ; les builds player se connectent auto au VPS ⇒ un déploiement raté coupe 100 % des joueurs. Incident réel : VPS resté 3 versions en retard, personne ne pouvait jouer. Pas de rollback, pas d'artefact CI, build sur place. | `docs/reflexion.md:12-18`, §11 |
| R3 | 🟠 Élevé | **Pas d'authentification pour rejoindre + DoS** : tout `Join` valide est accepté ; limite par IP contournable (CGNAT/proxy) ; salons créés à la demande par code arbitraire → `HashMap` non bornée. | `src/net/server_loop.rs:40-183` |
| R4 | 🟡 Moyen | **Smoke test sur le mauvais chemin** : `smoke_vps` cible par défaut `ws://179.237.71.235:80` (clair, bypass Caddy/TLS ; surchargeable via `argv[1]` mais le défaut est celui qu'on lance). Un smoke vert ne prouve pas que le chemin `wss://` des vrais joueurs marche — et révèle que le port répond aussi en clair. | `examples/smoke_vps.rs:20` |
| R5 | 🟠 Moyen | **Bout-en-bout jamais en CI** : le job CI `net-tests` couvre le loopback, mais les 2 tests VPS restent `#[ignore]` et `smoke_vps` est manuel. La robustesse en conditions réelles repose sur des runs à la main. | `src/net/client/native.rs:195,206` |
| R6 | 🟠 Moyen | **Charge croissante non bornée** : `wave_window = ceil(joueurs/2)` révèle plus de manches simultanées quand la population monte, et le Snapshot complet est rediffusé à chacun à 60 Hz. OK mesuré à 16 joueurs (~368 o), mais pas de cap ni de delta par client. | `src/app/combat.rs:94` |
| R7 | 🟡 Moyen | **TCP head-of-line blocking** sous perte de paquets ; migration UDP/QUIC explicitement conditionnée à une mesure de perte réelle — jamais faite. `RENDER_DELAY` non calibré en conditions VPS. | `SPRINTNETWORK.md` §69/71 |
| R8 | ⚪ Faible | **`aim_yaw` sans clamp de vitesse angulaire** (aimbot possible) — décision documentée, enjeu limité en PvE coop, à revoir si PvP. | `src/app/multiplayer.rs:359-397` |

## Reste ouvert dans les docs réseau

- Sprint 69 (RTT VPS 150-250 ms : distance vs applicatif, non tranché) — bloqué.
- « Revue de sécurité ciblée avant déploiement VPS » (`docs/reflexion.md:235`) — non cochée.
  R1 + R3 ci-dessus en seraient les premiers items.

## Actions recommandées

1. **Vérifier le token Firebase au Join** (idToken → uid vérifié côté serveur) — ferme R1.
2. **Scripter le déploiement** (script versionné dans le dépôt : build artefact en CI,
   push binaire, restart, smoke) + changer le **défaut** de `smoke_vps` pour
   `wss://ws.loicberthod.ch` (l'endpoint est déjà surchargeable par argument CLI) —
   réduit R2 et ferme R4.
3. **Fermer le port en clair** en façade si non nécessaire (constaté par R4).
4. **Borner salons et connexions globales** (cap de salons, éviction LRU) — réduit R3.
5. Mesurer la perte de paquets réelle en prod avant toute décision transport (R7).
