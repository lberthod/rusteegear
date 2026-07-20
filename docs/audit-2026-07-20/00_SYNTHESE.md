# Audit complet — Synthèse exécutive (2026-07-20)

*Audit multi-agents : 6 analyses parallèles, puis contre-vérification adversariale dans le
code (8/8 affirmations lourdes CONFIRMÉES) et relecture qualité.
Photographie au commit `429a764` + un chantier non commité (contrôles). **Périme dès que**
le chantier est commité (→ [06](06_CHANTIER_EN_COURS.md) obsolète) ou qu'une vague du
[plan d'action](07_PLAN_ACTION.md) est exécutée.*

## Verdict en une phrase

**Le projet est à ~80 % d'une bêta testable par des externes** : moteur, éditeur et réseau
sont solides et disciplinés (CI stricte, 764 tests, garde-fou unwrap au vert), mais le
lancement du premier test externe est bloqué par le gel `v0.1.0-alpha.1`, et deux risques
majeurs — vérifiés dans le code — dorment dans l'ombre : un **ré-export du player qui
viderait le bundle d'assets** (BOMB-1) et l'**absence de vérification de l'identité
Firebase côté serveur** (BOMB-2).

## Échelle de sévérité (commune à tous les fichiers)

🔴 Critique (perte de données / panne totale) · 🟠 Élevé · 🟡 Moyen · ⚪ Faible

## Tableau de bord

| Dimension | État | Note | Détail |
|---|---|---|---|
| Moteur (rendu/physique/audio) | 🟢 | ~90 % | Manque : SSAO, particules, IK (Phase S, repoussée) |
| Éditeur | 🟢 | ~85 % | Couverture de tests très faible (1 test / 400 lignes) |
| Gameplay solo | 🟢 | ~80 % | 7 démos jouables, 4 archétypes, courbe de vagues |
| Réseau / MMORPG | 🟡 | ~70 % | Prédiction/réconciliation auditée ; trous de parité et fin de manche minimale |
| Assets 3D | 🟡 | ~90 % livré | 482 GLB générés, 321 intégrés ; avatar = sphère placeholder |
| Qualité / tests | 🟢 | — | fmt+clippy+garde-fou verts ; trous `editor` et `gfx` |
| Architecture | 🟢 | — | Dette concentrée dans 3 monolithes, pas diffuse |
| Déploiement | 🔴 | — | Chaîne VPS 100 % manuelle, incident réel déjà survenu |

## Top 8 des risques (identifiants canoniques, repris dans tous les fichiers)

| ID | Sév. | Risque (titre court) | Où | Détail |
|---|---|---|---|---|
| **A1** | 🔴 | Ré-export du player = bundle d'assets vidé, rien recopié | `src/editor/export.rs:819,877` | [05](05_ASSETS_PIPELINE.md) |
| **R1** | 🟠 | `firebase_uid` client jamais vérifié → spoofing de progression | `src/bin/server.rs:286` | [04](04_RESEAU_DEPLOIEMENT.md) |
| **R2** | 🟠 | Déploiement VPS manuel + couplage `PROTOCOL_VERSION` → panne 100 % joueurs (déjà arrivé) | `docs/reflexion.md` §11 | [04](04_RESEAU_DEPLOIEMENT.md) |
| **A3** | 🟠 | Pas de `build.rs` : binaire aux assets silencieusement périmés | `src/assets.rs:13` | [05](05_ASSETS_PIPELINE.md) |
| **A2** | 🟠 | ~394 GLB orphelins embarqués dans le binaire | `assets/bundle/` | [05](05_ASSETS_PIPELINE.md) |
| **D1** | 🟡 | 3 monolithes : `AppState` 119 champs, `editor/mod.rs`, `physics.rs` | `src/app/mod.rs` | [01](01_ARCHITECTURE_DETTE.md) |
| **C1** | 🟡 | Collision caméra du chantier en cours : sans test, masque `u32::MAX` | diff non commité | [06](06_CHANTIER_EN_COURS.md) |
| **R5** | 🟡 | Bout-en-bout VPS jamais en CI + smoke test sur le mauvais chemin (défaut `ws://` en clair) | `examples/smoke_vps.rs:20` | [04](04_RESEAU_DEPLOIEMENT.md) |

*(BOMB-1 = A1, BOMB-2 = R1. La numérotation A* vient de [05](05_ASSETS_PIPELINE.md),
R* de [04](04_RESEAU_DEPLOIEMENT.md), D*/C* sont propres à cette synthèse.)*

Note résolue pendant l'audit : la contradiction « `net_tests` en CI ou pas » est tranchée —
le job `net-tests` **existe** dans `ci.yml:51` (vérifié) ; seuls les 2 tests VPS réels
restent `#[ignore]`. Voir [03](03_QUALITE_TESTS.md).

## Delta vs audits précédents (17-19/07)

| Constat | Statut |
|---|---|
| Config Firebase inaccessible hors éditeur (AUDIT 18/07, le plus grave alors) | ✅ Résolu (overlay ⚙ + bake à l'export) |
| Parité GDD : classes, assists, 4 modes, contrat, archétypes (sprint10audit) | ✅ Résolu (7 phases ✅) |
| God-modules >2 500 lignes (sprint9audit) | 🟠 Partiel : renderer/demos découpés, restent `editor/mod.rs`, `physics.rs`, `AppState` (D1) |
| Tension n°7 « les deux jeux » + jalon « preuve du fun » | 🔴 Persistant (3ᵉ audit consécutif) |
| Gel `v0.1.0-alpha.1` bloqué (sprint.19matin) | 🔴 Persistant |
| Ré-export destructif du bundle (A1), uid Firebase (R1) | 🆕 Nouveaux — jamais relevés avant cet audit |

## Priorités

L'**ordre d'exécution** fait autorité dans [07_PLAN_ACTION.md](07_PLAN_ACTION.md) :
Vague 1 = désamorcer A1/A3 (½ journée), Vague 2 = gel `v0.1.0-alpha.1`, Vague 3 =
sécuriser R1/R2 avant d'élargir le cercle, Vague 4 = « preuve du fun » (tension n°7,
avatar `fairy_hero`, écran de fin), Vague 5 = fond de roulement qualité.

## Mini-glossaire

- **Errant 62** : créature de la scène servie dont le test de resynchro de la scène
  embarquée échoue — c'est l'échec qui bloque le gel `v0.1.0-alpha.1`.
- **Contrat de PV verrouillé** : la courbe de points de vie des vagues (5/8/11/16) est
  garantie par des tests ; toute mécanique qui la modifierait implicitement est refusée.
- **Tension n°7 / « les deux jeux »** : le contenu riche (hameau fortifié, ménagerie) vit
  dans une démo d'éditeur, pas dans la scène réellement servie aux joueurs.
- **`AiChaser` / `Archetype`** : grammaire de chasse et castings de créatures
  (Traqueuse/Meute/Colosse/Furtive) existants dans le code mais non appliqués à la scène servie.
- **`mNN`** : préfixe de renumérotation des imports lors de la resynchro de scène
  (`m01_`, `m02_`…) — source des collisions/orphelins du bundle (A2).
- **Mode « Temple Run »** : mode auto-run (`auto_run_speed > 0`) où la caméra libre est désactivée.

## Sommaire

[00 Synthèse](00_SYNTHESE.md) · [01 Architecture](01_ARCHITECTURE_DETTE.md) ·
[02 Produit](02_PRODUIT_AVANCEMENT.md) · [03 Qualité/tests](03_QUALITE_TESTS.md) ·
[04 Réseau](04_RESEAU_DEPLOIEMENT.md) · [05 Assets](05_ASSETS_PIPELINE.md) ·
[06 Chantier en cours](06_CHANTIER_EN_COURS.md) · [07 Plan d'action](07_PLAN_ACTION.md)
