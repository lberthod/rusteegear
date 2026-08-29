# Roadmap post-audit (2026-08-29)

*Suite au [Rapport d'audit RusteeGear](https://claude.ai/code/artifact/ffc6987d-f1ef-48a9-8903-9266fe0b434a)
du 2026-08-29 : quatre lectures indépendantes du dépôt (architecture, réseau/sécurité,
cohérence gameplay Chasse 4.1, cohérence doc/code) à l'état du commit `163e081`,
copie de travail propre. Même format que
[docs/audit-2026-07-20/07_PLAN_ACTION.md](audit-2026-07-20/07_PLAN_ACTION.md) : vagues
priorisées par valeur débloquée / coût, coûts en taille de t-shirt (S ≈ ≤1 h, M ≈ ½ journée,
L ≈ 1-2 jours, XL ≈ 3 jours et plus).*

## Constat d'ensemble

L'audit du 20 juillet avait identifié une dette d'architecture précise (fichiers trop gros,
`AppState` à regrouper, couverture de tests faible dans `editor/`). Un mois de développement
plus tard, le rythme a clairement privilégié les livraisons fonctionnelles (Chasse 4.1,
silhouettes de classe v7, sécurisation réseau) — toutes confirmées solides et testées — mais
**aucun des constats de dette de juillet n'a été traité, et `AppState` a même grossi de 47 %**
(119 → 175 champs). Rien n'est urgent au sens « ça casse » (CI verte, tests réseau réels, aucune
faille de sécurité critique), mais le coût de toute modification touchant `editor/mod.rs`,
`runtime/physics.rs` ou `AppState` continue d'augmenter plus vite qu'il ne diminue.

## Vague 1 — Désamorcer avant que ça ne coûte plus cher — 3/4 faites (2026-08-29)

| # | Action | Ferme | Statut |
|---|---|---|---|
| 1.1 | Trancher Git LFS vs exclusion pour `assets/models/` (159 Mo, 826 fichiers versionnés en Git normal, `.git/` déjà à 146 Mo) | dette bloquante | ⏳ Seul point classé « bloquant » de l'audit — irréversible sans réécriture d'historique si on attend encore ; **décision à prendre avec l'utilisateur** (LFS a un coût de stockage/bande passante) |
| 1.2 | Réactiver le test roguelike (`src/app/demos.rs:341-342`) | qualité | ✅ Plus flaky : 35/35 passages verts (isolé + sous charge) constatés le 2026-08-29 — les changements ultérieurs (`attack_range`/`cooldown`/`windup` figés, budget 400 frames) ont éliminé la cause. `#[ignore]` retiré, suite complète 684/684 verte (`d71b3f9`) |
| 1.3 | Corriger les 3 dérives README : lignes de code (« ~32 000 » annoncé, 75 422 réel), chemin mort `src/scene/demos.rs` (→ `src/scene/demos/`), compteur `.glb` (412 annoncé, 482 réel) | doc | ✅ `14c47a7` — chiffre de lignes recalé sur ~65 000 (hors tests) |
| 1.4 | Mettre à jour le commentaire `Cargo.toml:4-5` (« deux binaires » → 4 : `motor3derust`, `server`, `pilot`, `glbviewer`) | doc | ✅ `14c47a7` |

## Vague 2 — Découper les trois points de passage obligés — 1/4 faite (2026-08-29)

| # | Action | Ferme | Statut |
|---|---|---|---|
| 2.1 | Découper `build_ui` (`src/editor/mod.rs:1362-2744`, 1382 lignes, 0 test dessus) par panneau, comme déjà fait pour `windows.rs` / `hud.rs` / `export.rs` | dette | ⏳ |
| 2.2 | Scinder `impl Physics` en modules thématiques | dette | ✅ `76b0967` — `src/runtime/physics.rs` → `physics/{mod,build,control,query,step,tests}.rs`. **Correction du chiffre de l'audit en cours de route** : « 58 méthodes » était un artefact `grep` (comptait aussi les fonctions de test) ; `impl Physics` n'en avait réellement que 14 sur ~978 lignes, la plupart concentrées dans `build()` (406 l.) et `control_kinematic()` (146 l.). Découpage multi-fichiers fait quand même sur choix explicite de l'utilisateur. Vérifié : 684/684 tests verts avant/après, `cargo build --all-targets` propre, garde-fou unwrap toujours vert |
| 2.3 | Regrouper `AppState` en sous-structs par domaine | dette | 🔶 6 lots faits : FX/HUD → `FxState` (9 champs, `6f4cbfd`) ; panneaux Multijoueur → `NetPanelsState` (12 champs, `62695fc`, incident de script corrigé avant commit) ; connexion Firebase → `FirebaseAuthState` (5 champs, `f5fd82e`) ; chargements asynchrones → `AsyncLoadState` (5 champs, `a41afd0`) ; état des joueurs réseau → `NetworkPlayersState` (12 champs, `c9f5f20`, 123 sites) ; **projectiles → `ProjectilesState`** (8 champs, `b3ce0e7`, 82 sites dans `creature_attack.rs`/`fireball.rs`). 51 champs regroupés, 684/684 tests verts à chaque lot. **Ampleur réelle mesurée** : ~120-175 champs, 71 fichiers concernés — bien plus gros qu'un simple refactor `L`. Reste par lots séparés (état drag/gizmo éditeur, IA scène, undo/redo, script Lua...) ; `build_ui` (0 test) reste le point le plus risqué à toucher |
| 2.4 | Ajouter des tests sur `src/editor/menus.rs` (765 lignes, 0 test) | qualité | ⏳ |

## Vague 3 — Petits nettoyages sécurité/infra — non commencée, non bloquant

| # | Action | Ferme | Statut |
|---|---|---|---|
| 3.1 | Retirer l'IP publique et l'utilisateur SSH en dur de `scripts/deploy_vps.sh:23` (variable d'env obligatoire sans défaut) | surface | ⏳ Pas un secret, mais une divulgation d'infra si le dépôt devient public |
| 3.2 | Clarifier dans la doc que « parité `ai_chaser` vérifiée en CI » (`src/scene/mod_tests.rs:2060-2110`) est un test unitaire noyé dans `cargo test --all-targets`, pas un job CI dédié isolé | doc | ⏳ Le contrôle est réel, la formulation prête à confusion |
| 3.3 | Revalider côté VPS que le port en clair est bien fermé si non nécessaire (item déjà noté dans le plan d'action du 20/07, jamais confirmé) | surface | ⏳ Hors dépôt (SSH/Caddy), à faire lors du prochain déploiement |

## Ce qui est déjà solide (à ne pas re-questionner sans raison)

- **Sécurité réseau** : anti-usurpation Firebase (idToken vérifié serveur), anti-DoS
  (`MAX_TOTAL_CONNECTIONS=256`, `MAX_ROOMS=16`), rate-limiting **par connexion** (pas seulement
  global), entrées réseau nettoyées contre NaN/infini avant simulation — tous vérifiés actifs sur
  le chemin critique et couverts par des tests d'intégration sur vrais sockets. Aucune faille
  critique trouvée.
- **Chasse 4.1 / silhouettes v7** : grammaire Traqueuse/Meute/Colosse/Furtive sur les 26
  créatures, contrat de PV 5/8/11/16, portée de chasse plafonnée à 9 m, knockback et dégâts de
  contact, `PROTOCOL_VERSION` 7 avec rejet propre des clients v6 — tout confirmé dans le code et
  par exécution réelle des tests, pas seulement par le discours des commits.
- **Outillage** : garde-fou `unwrap`/`panic` testé en CI (14 occurrences whitelistées,
  justifiées), CI mature (fmt, clippy strict, tests réseau réels, goldens de rendu Metal,
  cross-build Android/iOS/wasm32), `unsafe` et `dead_code` quasi absents, `Cargo.toml`
  exemplairement documenté.

## Ce qu'il ne faut pas faire maintenant

- Réécrire l'historique Git pour purger `assets/models/` rétroactivement avant d'avoir tranché
  LFS vs exclusion (1.1) — décider d'abord la cible, migrer ensuite en une seule fois.
- Refactorer `AppState`/`Physics`/`build_ui` en même temps qu'un chantier fonctionnel en cours —
  ce sont des changements à isoler dans leur propre commit, testables indépendamment.
- Réagir à l'écoute en clair du serveur applicatif (`src/bin/server.rs:62`) : c'est une
  architecture voulue (TLS délégué à Caddy), pas une faille — seul le point 3.3 (infra VPS) reste
  à confirmer.
