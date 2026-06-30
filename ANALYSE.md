# Analyse du projet — RusteeGear (motor3derust)

> Document d'analyse transversale : compréhension du projet, audit technique, pertinence
> des choix technologiques et possibilités d'évolution.
> 📸 **Snapshot daté du 2026-06-19** (`src/` ~7 300 lignes alors ; **~10 400 aujourd'hui**, Phases A→H).
> Les recommandations de fond restent valides ; pour l'état **à jour** + la suite, voir
> **[SPRINTS.md](SPRINTS.md)** (Phase I, sprints 45→49). Détail : [README.md](README.md),
> [AUDIT.md](AUDIT.md), [ROADMAP_SPRINTS.md](ROADMAP_SPRINTS.md), [HANDOFF.md](HANDOFF.md).

---

## 1. Compréhension du projet

**RusteeGear** est un moteur / éditeur de jeu 3D minimaliste « à la Unity », écrit
*from scratch* en Rust, sans moteur tiers. L'objectif n'est pas de concurrencer Unity
ou Bevy mais d'offrir une base **compréhensible de bout en bout** : chaque étage du
pipeline (fenêtre → événements → état → rendu GPU → UI) est écrit à la main.

### Ce que le projet fait aujourd'hui

| Domaine | Capacités |
|---|---|
| **Rendu** | Pipeline `wgpu`/WGSL maison, depth buffer, **matériaux PBR par objet** (metallic / roughness / emissive), éclairage de scène éditable, **frustum culling CPU**, **rendu instancié** (storage buffer d'instances) |
| **Édition** | Éditeur `egui` (toolbar, hiérarchie, inspecteur, bandeau d'état), gizmos translate/rotate/scale (W/E/R), picking par raycast, multi-sélection, undo/redo, copier-coller, dupliquer, renommage inline, groupes glisser-déposer |
| **Scène** | Primitives (cube/sphère/plan) + import glTF/GLB asynchrone, sérialisation JSON |
| **Runtime (mode Play)** | Scripting Lua par objet (`mlua`, chunks compilés en cache), physique `rapier3d` (statique/dynamique, gravité, collisions), audio `kira` |
| **IA** | Génération de scripts et de scènes entières via API HTTP (`src/app/ai.rs`) |
| **Build/Export** | Export 1-clic `.dmg` / `.apk` / `.ipa` avec assets embarqués, config persistée, pré-vol des toolchains, install device |
| **Plateformes** | macOS (éditeur complet), iOS et Android (mode player tactile) — réellement déployés |

### Architecture

Le point fort structurel est la **séparation logique / rendu** :

```
src/
├── lib.rs         # event loop winit, run() desktop, android_main (cdylib), resume mobile
├── app/           # logique SANS GPU : AppState, picking, sélection, input, IA, build_config
├── gfx/           # couche rendu wgpu : renderer, mesh, camera, shaders WGSL
├── scene/         # Transform, MeshKind, Scene, groupes, lumière, import glTF, sérialisation
├── runtime/       # mode Play : physics (rapier3d), audio (kira)
├── editor/        # UI egui (toolbar, hiérarchie, inspecteur, export) — desktop
└── assets.rs      # assets embarqués (include_dir, schéma bundle://) pour le player exporté
```

L'état métier (scène, caméra, entrées, picking) ne dépend **pas** du GPU : seule la
couche `gfx/` parle à `wgpu`. C'est ce découpage qui a rendu le portage mobile direct.

**Répartition du code** (les deux gros modules concentrent la complexité) :
`editor/mod.rs` (1729 l.), `app/mod.rs` (1404 l.), `gfx/renderer.rs` (1253 l.),
`editor/export.rs` (641 l.), `scene/mod.rs` (599 l.).

---

## 2. Audit technique — synthèse

> L'audit détaillé (P1→P10) est dans [AUDIT.md](AUDIT.md). Les correctifs P1→P9 ont été
> traités ; ce qui suit en est la lecture transversale, mise à jour avec le Sprint 33.

**Verdict global : base saine, idiomatique, bien commentée et bien découpée.** Aucun
problème de sécurité bloquant. Les axes d'amélioration restants relèvent surtout de la
performance fine, de la robustesse mobile et de la couverture de tests.

### Points forts confirmés
- Découpage état/rendu/runtime exemplaire pour la taille du projet.
- Code **clippy-clean** (`-D warnings`), `rustfmt`, **CI GitHub Actions** (fmt + clippy +
  tests + cross-build Android/iOS).
- 11 tests unitaires sur la math critique (picking, ray/AABB, sérialisation).
- Optimisations rendu déjà engagées : cache de chunks Lua, présentation vsync + cadence
  adaptative, matrices de picking en cache, frustum culling, rendu instancié.

### Points de vigilance (résiduels)
| # | Sujet | Sévérité | Statut |
|---|---|---|---|
| P4 | Panics d'init (`unwrap`/`expect`) sur chemins faillibles — crash silencieux à froid sur mobile | 🟠 Robustesse | partiel (indexation défensive faite ; init à propager) |
| P10 | Pas d'import d'assets sur mobile (`rfd` désactivé, pas de remplacement) | 🟢 Fonctionnel | ouvert |
| — | **Simulation pilotée par la cadence de rendu** (`advance_play` appelé depuis `render`) : physique dépendante du framerate | 🟠 Archi | ouvert |
| — | **Validation bout-en-bout** des nouveautés (assets embarqués, matériaux/lumière, resume, PBR/instancing) : vertes en CI mais peu exécutées sur device | 🟠 Qualité | ouvert (Sprint 28) |

### Recommandations prioritaires
1. **Découpler simulation et rendu** : boucle de mise à jour séparée, pas de temps fixe
   pour la physique. C'est le principal levier de correction architecturale restant.
2. **Durcir l'initialisation** : propager les `Result` d'init GPU/fenêtre et logguer
   (`log::error!`) avant une sortie contrôlée — surtout critique sur Android/iOS.
3. **Valider sur device réel** les chaînes de rendu récentes (PBR, instancing) et le
   resume mobile avant d'empiler de nouvelles features.
4. **Étendre les tests** : matériaux, round-trip de sérialisation des nouveaux champs,
   frustum culling.

---

## 3. Pertinence des choix technologiques

### Rust — adapté au domaine
Un moteur cumule précisément les contraintes que Rust adresse : performance native sans
GC (rendu temps réel + physique), sécurité mémoire à la compilation (le borrow checker
élimine use-after-free et data races sur un système concurrent rendu + I/O async + audio
+ physique), et un écosystème graphique de premier plan **écrit en Rust**. La
portabilité (`cargo` + abstraction `wgpu`) a permis de cibler macOS, un `.so` Android et
un binaire iOS depuis un cœur unique.

### Stack — briques ciblées et remplaçables
| Besoin | Crate | Pertinence |
|---|---|---|
| Fenêtre / événements | `winit` | Standard de fait, multiplateforme (desktop + mobile). ✅ |
| Rendu GPU | `wgpu` (WGSL) | Abstraction moderne Metal/Vulkan/DX12/WebGPU — **clé de la portabilité**. ✅ |
| Maths | `glam` | SIMD, ergonomique, omniprésent dans l'écosystème. ✅ |
| UI éditeur | `egui` | UI immédiate, intégration `wgpu` directe, rapide à itérer. ✅ |
| Sérialisation | `serde` / `serde_json` | Référence absolue. ✅ |
| Import 3D | `gltf` | Format ouvert standard. ✅ |
| Scripting | `mlua` (Lua 5.4) | Léger, embarquable, idéal pour du script par objet. ✅ |
| Physique | `rapier3d` | Le moteur physique de référence en Rust. ✅ |
| Audio | `kira` | Pensé pour le jeu (mixage, sons asynchrones). ✅ |

**Choix structurant : ne pas s'appuyer sur Bevy.** Cohérent avec l'objectif
pédagogique — s'appuyer sur Bevy remplacerait une boîte noire (Unity) par une autre.
RusteeGear assemble lui-même boucle d'événements, pipeline de rendu, picking, gizmos et
sérialisation, et ne tire de dépendances que pour des problèmes *précis et délimités*.
Ce positionnement est assumé et bien défendu dans le [README.md](README.md).

**Édition 2024 / Rust récent** : projet moderne, à jour, sans dette de version.

---

## 4. Possibilités futures

### Court terme (consolidation)
- **Ombres** (shadow mapping) — le shader `shadow.wgsl` existe déjà, à câbler.
- **Textures** (au-delà du PBR par valeurs) : albédo / normal / metallic-roughness maps.
- Multi-sélection au **clic 3D**, sous-groupes et réordonnancement dans la hiérarchie.
- Import d'assets **sur mobile** (picker natif ou assets embarqués) — lever P10.
- Boucle de simulation **découplée du rendu** (pas de temps fixe physique).

### Moyen terme (montée en gamme moteur)
- **Cible WebGPU / WASM** : `wgpu` la supporte déjà — un export navigateur serait
  un débouché naturel et à fort impact de démonstration.
- Migration progressive vers un **ECS léger** (la scène est aujourd'hui un
  `Vec<SceneObject>`) si la complexité le justifie — à arbitrer contre la lisibilité.
- **Rendu** : passes supplémentaires (post-process, SSAO, bloom sur l'emissive),
  pipeline batché plus poussé en complément du rendu instancié actuel.
- Enrichir l'**API de scripting** Lua (entrées, requêtes de scène, événements de
  collision, audio piloté par script).

### Long terme (produit & distribution)
- **Signature distribution store** (App Store / Play Store) + icônes / launch screen,
  IPA signé en CI via secrets.
- **Intégration IA approfondie** : la génération de scènes/scripts (`app/ai.rs`) est un
  différenciateur ; assistant d'édition contextuel, génération d'assets, debug de script.
- Documentation/onboarding pour usage **pédagogique** (le cœur de la proposition de
  valeur) : tutoriels « lire le moteur en une après-midi ».

---

## 5. Conclusion

RusteeGear est un projet **cohérent, mature pour sa taille et techniquement sain**. Le
découpage logique/rendu est sa plus grande force ; il sous-tend la portabilité réelle
(3 plateformes) et la lisibilité revendiquée. Les choix technologiques sont pertinents
et alignés avec un écosystème Rust de qualité, et le positionnement *from scratch /
pédagogique* est défendable et bien argumenté.

Les chantiers les plus rentables désormais ne sont plus correctifs mais **structurels et
de validation** : découpler la simulation du rendu, durcir l'initialisation pour le
mobile, et valider sur device réel les chaînes de rendu récentes (PBR, instancing,
resume) avant d'ajouter de nouvelles fonctionnalités. Les pistes WebGPU/WASM et
l'approfondissement de la génération par IA constituent les axes d'évolution à plus fort
potentiel de différenciation.
