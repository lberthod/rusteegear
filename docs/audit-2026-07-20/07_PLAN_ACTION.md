# Plan d'action priorisé (2026-07-20)

*Séquencé par valeur débloquée / coût. Chaque item est autonome, committable seul, avec
son critère « fait » vérifiable. Coûts en taille de t-shirt : S ≈ ≤1 h, M ≈ ½ journée,
L ≈ 1-2 jours, XL ≈ 3 jours et plus. Identifiants de risques : voir la table canonique
de [00_SYNTHESE.md](00_SYNTHESE.md).*

## Vague 1 — Désamorcer les bombes et fiabiliser les mesures — ✅ FAITE (2026-07-20)

| # | Action | Ferme | Statut |
|---|---|---|---|
| 1.1 | Garde-fou `preserve_bundled` dans `bundle_scene_json_at` (`src/editor/export.rs`) : les clés `bundle://` référencées sont sauvegardées en mémoire avant le `remove_dir_all` puis réécrites (disque, sinon bundle embarqué recompressé) | **A1** 🔴 | ✅ Test `reexporting_an_already_bundled_scene_preserves_the_bundle` vert |
| 1.2 | `build.rs` avec `rerun-if-changed` sur `assets/bundle` **et** `assets/player_scene.json` | **A3** | ✅ Vérifié : `touch` d'un fichier du bundle → recompilation ; rituel `touch src/assets.rs` obsolète |
| 1.3 | 2 chemins morts de `docs/architecture.md` corrigés (`gfx/renderer/`, `net/client/`) | doc | ✅ |
| 1.4 | Défaut de `smoke_vps` passé à `wss://ws.loicberthod.ch` (surchargeable par argument CLI) | **R4** | ✅ |
| 1.5 | Worktree `.claude/worktrees/compassionate-einstein-575c8b/` supprimé (propre, commit `a936b42` déjà dans main) | mesures | ✅ `grep "#\[test\]"` retombé à 764 |

*Preuves : `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, garde-fou
unwrap (14 whitelistés) et tests du module export tous verts après la vague.*

## Vague 2 — Débloquer le gel `v0.1.0-alpha.1` (3/4 faits le 2026-07-20)

| # | Action | Statut |
|---|---|---|
| 2.1 | Chantier contrôles fini et commité (`9dc0e42`) : masque caméra restreint au bit 0 (décor) + 3 tests de collision caméra (mur / voie libre / créature ignorée) | ✅ suite lib 669/669 verte |
| 2.2 | Scène embarquée : les 11 gardes-fous `the_embedded` passent (« Errant 62 » résolu par le ré-export du 19-20/07) ; l'outil de resynchro copie désormais lui-même `models/` → `bundle/mNN_` compressé (`88f103e`) — ferme **A5** | ✅ |
| 2.3 | Purge : 394 orphelins supprimés (bundle 22 → 12 Mo, 321 clés conservées), `scripts/check_bundle_orphans.py` en mode check dans le job CI `check` (`e7b4332`) — ferme **A2** | ✅ |
| 2.4 | Tag `v0.1.0-alpha.1`, release, builds depuis le tag, lancement du protocole de test externe | ✅ Constat du 20/07 après-midi : les tags `v0.1.0-alpha.1/.2/.3` étaient **déjà posés et poussés** le 19/07 (autre session, `release.yml` construit depuis les tags) — le constat « gel bloqué » de l'audit était périmé. Restes utilisateur : inviter le testeur, test compte macOS neuf, issue GitHub flaky |

## Vague 3 — Sécuriser avant d'élargir le cercle (3/4 faits le 2026-07-20)

| # | Action | Ferme | Statut |
|---|---|---|---|
| 3.1 | Le `Join` transporte un **idToken** vérifié serveur (`accounts:lookup`, thread + canal, jamais de HTTP dans le tick) ; uid brut ignoré dès que le serveur a une session Firebase ; même trame, pas de bump de protocole (`3aba61e`) | **R1** | ✅ ⚠️ Un nouveau client contre l'ANCIEN VPS = Join rejeté (charset) → **redéployer le VPS avant de distribuer de nouveaux clients** |
| 3.2 | `scripts/deploy_vps.sh` versionné : push → pull+build VPS → restart systemd → smoke wss (chemin réel) + route claire optionnelle ; refuse arbre sale/branche ≠ main | **R2** (réduit) | ✅ Script prêt — **premier run à faire** (redémarre la prod). Vrai artefact CI + rollback = plus tard |
| 3.3 | `MAX_TOTAL_CONNECTIONS = 256` (server_loop, testé à plafond bas) + `MAX_ROOMS = 16` avec `JoinRejected` explicite (`d894e07`) | **R3** | ✅ net_tests 69/69 |
| 3.4 | Fermer le port en clair du VPS si non nécessaire (constat R4) | surface | ⏳ Côté VPS/Caddy (SSH), pas dans le dépôt — à faire lors du prochain déploiement |

## Vague 4 — La preuve du fun (entamée le 2026-07-20 ; constats d'audit en partie périmés)

| # | Action | Statut |
|---|---|---|
| 4.1 | Réunifier « les deux jeux » | ✅ **Livré en 6 commits** (`d03581b`→ docs, 2026-07-20, plan d'architecte vérifié ligne à ligne) : grammaire Traqueuse/Meute/Colosse/Furtive sur les 26 créatures servies, patrouille par défaut + chasse ≤ 9 m plafonnée, dégâts de contact, knockback scripté réparé, contrat PV 5/8/11/16 intact (test exact), scène resynchronisée + parité `ai_chaser` en CI, entrée roadmap « preuve du fun » créée. Reste : validation manuelle du ressenti (vitesses dégradables dans la table) |
| 4.2 | Avatar `fairy_hero` | ✅ Restauré (`0506c90`) : mesh skinné + clips Idle/Walk pilotés + garde-fou CI. Reste ⬜ : 3 silhouettes de classe (bump protocole v7, à grouper avec le prochain déploiement couplé) |
| 4.3 | Écran de fin de manche détaillé | ✅ Déjà livré avant l'audit (Phase H Sprint 1, `PROTOCOL_VERSION` 6) — constat d'audit périmé |
| 4.4 | Surfaces contextuelles | ✅ Bannière de vague (déjà livrée), marqueur allié hors-écran (déjà livré — constat périmé, `ally_down_banner`/`offscreen_edge_position`), bannière « palier atteint » (livrée `9e54a73`) |

## Vague 5 — Fond de roulement qualité (au fil de l'eau)

- Rendre déterministe puis réactiver le test roguelike flaky (`src/app/demos.rs:342`) —
  fait quand : `#[ignore]` retiré et 20 runs consécutifs verts.
- Couverture `editor` (1 test/400 lignes) : export, undo-redo, manipulation de scène.
- « Sprint 9 bis » : découper `editor/mod.rs` (2 888 l.) et `runtime/physics.rs` (2 359 l.) ;
  regrouper les 119 champs d'`AppState` en structs de sous-systèmes (**D1**).
- `WeaponPickup` synchronisé réseau ; sélecteur classe/mode dans l'overlay mobile ;
  présence en ligne affichée.
- Audio rangs 2-3 (allié à terre dédié, éveil de créature) + accessibilité minimale
  (taille HUD, réduction de secousses).
- Décision Git LFS vs exclusion pour `assets/models/` (160 Mo versionnés, **A4**).

## Ce qu'il ne faut PAS faire maintenant

- Migrer le transport vers UDP/QUIC — explicitement conditionné à une mesure de perte de
  paquets réelle, jamais faite (**R7**). Mesurer d'abord.
- Appliquer `Archetype` brut à la scène servie — casserait le contrat de PV verrouillé
  (report acté le 18/07) ; passer par un paramètre de comportement découplé (4.1).
- Élargir le budget de frames du test roguelike — le rendre déterministe à la place.
