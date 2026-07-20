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

## Vague 2 — Débloquer le gel `v0.1.0-alpha.1` (1-2 jours)

| # | Action | Coût | Fait quand |
|---|---|---|---|
| 2.1 | Finir et committer le chantier contrôles : + test `update_camera_collision`, décision sur le masque de couches (voir [06](06_CHANTIER_EN_COURS.md)) | M | `git status` propre ; test collision (mur → distance réduite ; rien → `None`) vert ; [06](06_CHANTIER_EN_COURS.md) archivé |
| 2.2 | Resynchroniser la scène embarquée (résoudre « Errant 62 ») en **automatisant** la copie `models/` → `bundle/mNN_` dans le test de resynchro (`src/scene/mod_tests.rs:1894`) — ferme aussi **A5** | M | Le test de resynchro passe sans étape manuelle ; la scène servie affiche Errant 62 correctement |
| 2.3 | Purger les ~394 GLB orphelins du bundle : script comparant les clés de `player_scene.json` aux fichiers de `bundle/`, branché en mode check dans la CI | **A2**, L du binaire | M | `bundle/` = exactement les 321 clés référencées ; job CI échoue si un orphelin réapparaît |
| 2.4 | Tag `v0.1.0-alpha.1`, release, builds depuis le tag, lancement du protocole de test externe (matériel prêt : QUICKSTART, doctor.sh, TEST_SCENARIO, FEEDBACK_FORM) | M | Tag poussé ; builds téléchargeables issus du tag ; premier testeur externe invité |

## Vague 3 — Sécuriser avant d'élargir le cercle (2-3 jours)

| # | Action | Ferme | Coût | Fait quand |
|---|---|---|---|---|
| 3.1 | Vérifier le token Firebase (idToken) au `Join` côté serveur au lieu de faire confiance à l'uid client | **R1** | L | Un `Join` avec uid ≠ celui du token est rejeté ; test le prouvant |
| 3.2 | Script de déploiement versionné : build artefact en CI → push binaire → restart → smoke | **R2** | L | Un déploiement complet = une seule commande, rejouable, tracée dans le dépôt |
| 3.3 | Cap global de salons/connexions + éviction | **R3** | M | Test : au-delà du cap, création de salon refusée proprement |
| 3.4 | Fermer le port en clair du VPS si non nécessaire (constat R4) | surface | S | Le port 80 direct ne répond plus au handshake WS |

## Vague 4 — La preuve du fun (~1 semaine)

| # | Action | Coût | Fait quand |
|---|---|---|---|
| 4.1 | Réunifier « les deux jeux » : grammaire de comportement (vitesse/agressivité par créature) appliquée à `player_scene.json` **sans toucher au contrat de PV verrouillé** ; créer l'entrée de roadmap « preuve du fun » | XL | Les créatures de la scène servie chassent/patrouillent selon leur casting ; les tests de PV de vagues restent verts ; entrée de roadmap créée |
| 4.2 | Avatar `fairy_hero` + 3 silhouettes de classe à la place de la sphère | L | En jeu réseau, la classe d'un joueur est identifiable à sa silhouette seule |
| 4.3 | Écran de fin de manche détaillé (ligne par joueur, frags/assists au même rang, XP, contrat) | L | Fin de manche à 2+ joueurs : chaque joueur voit sa ligne et le contrat |
| 4.4 | Surfaces contextuelles : bannière de vague, palier atteint, marqueur allié hors-écran | M | Les 3 surfaces visibles en partie réseau |

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
