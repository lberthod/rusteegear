# Audit du projet — RusteeGear (`motor3derust`)

> Audit réalisé le 4 juillet 2026, sur la branche `main` (dernier commit `67c0d04`,
> avec des modifications locales non commitées dans `src/app/mod.rs` et `src/scene/mod.rs`).
> Ne pas confondre avec [audit_sprint.md](audit_sprint.md), qui est un audit *gameplay* ciblé.

---

## 1. Identité du projet

| | |
|---|---|
| **Nom** | RusteeGear (crate `motor3derust`, v0.1.0) |
| **Nature** | Moteur / éditeur de jeu 3D minimaliste « à la Unity », écrit from scratch en Rust |
| **Licence** | MIT |
| **Taille** | ~14 250 lignes de Rust dans `src/` (~26 fichiers) |
| **Plateformes** | macOS (éditeur complet, `.dmg`) · Android (`.apk` player) · iOS (player, signature dev personnelle) |
| **Langue du projet** | Français (docs, commits, commentaires) |

## 2. Raison d'exister

RusteeGear n'est **pas un concurrent d'Unity, ni de Bevy**. Sa raison d'être est
explicite et cohérente dans toute la documentation :

1. **Pédagogie et maîtrise totale.** Les moteurs grand public (Unity, Unreal, Godot)
   sont des boîtes noires de millions de lignes ; on y apprend *l'outil*, pas *les
   concepts*. RusteeGear écrit à la main chaque étage du pipeline (fenêtre → événements
   → état → rendu GPU → UI) pour que le moteur entier « tienne dans la tête » d'un
   développeur.
2. **Hackabilité.** ~14 000 lignes lisibles : ajouter une primitive, un collider ou une
   variable de script se fait en quelques lignes.
3. **Portabilité par conception.** La logique (`app/`, `scene/`) ne touche jamais le
   GPU ; seule la couche `gfx/` parle à `wgpu`. C'est ce découpage qui a permis le
   portage iOS/Android sans réécrire le cœur.
4. **Zéro runtime caché.** Pas de GC, pas de moteur embarqué, un seul binaire natif.

Le choix « from scratch plutôt que Bevy » est argumenté dans le README : s'appuyer sur
Bevy reviendrait à remplacer une boîte noire par une autre. Les dépendances sont
choisies pour des problèmes *délimités* (fenêtre, GPU, UI, physique, audio, script),
jamais pour la structure générale du moteur.

## 3. Fonctionnalités

### Rendu (`src/gfx/`, ~2 100 lignes)
- Rendu 3D temps réel via `wgpu` (Metal/Vulkan), shaders WGSL maison
  ([main.wgsl](src/gfx/shaders/main.wgsl), [shadow.wgsl](src/gfx/shaders/shadow.wgsl),
  [gizmo.wgsl](src/gfx/shaders/gizmo.wgsl)), depth buffer, ombres (shadow map + PCF).
- Matériaux PBR par objet (metallic / roughness / emissive), textures albédo.
- Lumières : directionnelle + ambiante, ponctuelles et spots (jusqu'à 8, avec
  culling/LOD des lumières les plus proches).
- Optimisations : rendu instancié (1 draw par lot mesh+texture), frustum culling CPU,
  chemin de rendu **sans allocation par frame** (tampons réutilisés, plan de dessin par
  index avec skip-rebuild par hash, re-tri paresseux).

### Éditeur desktop (`src/editor/`, ~3 950 lignes)
- UI `egui` : toolbar Play/Pause/Stop, hiérarchie (groupes, drag & drop, filtre,
  renommage inline), inspecteur, bandeau d'état, console de logs, profiler FPS/mémoire.
- Primitives (cube, sphère, plan, cylindre, capsule, terrain) + import glTF/GLB
  asynchrone.
- Sélection clic 3D (picking par raycast) et multi-sélection ; gizmos
  translate/rotate/scale multi-objets à pivot commun ; snap sur grille.
- Undo/Redo, couper/copier/coller, dupliquer, aligner/distribuer, grouper.
- Gestionnaire d'assets (schéma `asset://`), sérialisation JSON des scènes.
- Panneau **Build & Export** ([export.rs](src/editor/export.rs)) : exports 1-clic
  `.dmg`/`.apk`/`.ipa` avec presets, config d'identité, install sur device, log de build.
- Contrôle qualité APK, optimisation mobile (réduction textures, limite de lumières),
  diagnostic système ([readiness.rs](src/editor/readiness.rs)).

### Runtime de jeu (`src/runtime/` + partie de `src/app/`)
- Physique `rapier3d` (statique/dynamique, colliders Auto/Box/Sphère/Capsule).
- Audio `kira` : son par objet, autoplay, spatialisation par distance, cache async,
  et **SFX synthétisés** ([sfx.rs](src/runtime/sfx.rs)).
- Scripting **Lua 5.4** (`mlua`, chunks compilés en cache) : accès transform/couleur
  par objet, `dt`/`time`, joystick/boutons tactiles, gyroscope, `vibrate()`,
  `set_health()`.
- Architecture par **composants optionnels** sur `SceneObject` : `Controller` (joueur
  pilotable), `AudioSource`, `Combat` (attackable/FX), `AiChaser` (IA de poursuite).

### Gameplay « sans script » (chantier actif)
- Personnage pilotable au joystick/clavier, saut, collisions, caméra 3e personne.
- Boucle complète : collectibles, score, chrono, zones mortelles, win/lose, Rejouer.
- **Mode manches façon « Call of Zombies »** : 3 archétypes de monstres, 4 vagues,
  attaque du joueur en missile homing avec recharge — et, dans le diff non commité,
  un temps de **préparation d'attaque** (`attack_windup`) pour garantir un risque en 1v1.
- 5+ démos intégrées (contrôleur, gameplay scripté, arène, tour, course infinie, duel IA).

### Mobile
- Mode **player** plein écran (démarrage auto sur iOS/Android via feature
  `player_build`), contrôles tactiles (joystick virtuel, boutons, barre de vie),
  aperçu device (cadre téléphone) dans l'éditeur, assets embarqués (`include_dir`,
  schéma `bundle://`).

### IA (DeepSeek, desktop uniquement — [ai.rs](src/app/ai.rs))
- Génération/optimisation de scripts Lua et génération de scènes entières depuis une
  consigne texte (clé/modèle/température dans les Paramètres, via `ureq`).

## 4. Stack technique

| Besoin | Crate | Version |
|---|---|---|
| Fenêtre / événements | `winit` | 0.30 |
| Rendu GPU | `wgpu` (WGSL) | 29.0 |
| UI éditeur | `egui` + `egui-wgpu` + `egui-winit` | 0.34 |
| Maths | `glam` | 0.33 |
| Physique | `rapier3d` | 0.33 |
| Audio | `kira` | 0.12 |
| Scripting | `mlua` (Lua 5.4 vendored) | 0.11 |
| Import 3D | `gltf` | 1.4 |
| Sérialisation | `serde` / `serde_json` | 1.0 |
| Assets embarqués | `include_dir` | 0.7 |
| Dialogues fichiers (desktop) | `rfd` | 0.17 |
| HTTP (IA, desktop) | `ureq` | 2.12 |
| Images | `image` (png/jpeg) | 0.25 |

- **Rust édition 2024**, toolchain stable. Crate double `rlib` + `cdylib` (le `.so`
  Android est la même lib que le binaire desktop).
- Packaging : `cargo-bundle` (macOS), `cargo-apk` (Android), `xcodegen` + Xcode (iOS),
  scripts sous [packaging/](packaging/).
- Profil release soigné (LTO thin, `codegen-units=1`, `panic=abort`, strip) + profil
  `dev-fast` pour l'itération.

Les versions sont récentes et l'arbre de dépendances est délibérément court : chaque
crate répond à un besoin précis, aucune ne structure le moteur.

## 5. Architecture

```
src/
├── lib.rs         # boucle winit, run() desktop, android_main (cdylib), resume mobile
├── main.rs        # entrée desktop (5 lignes) → motor3derust::run()
├── assets.rs      # assets embarqués (include_dir, bundle://) pour le player exporté
├── log_buffer.rs  # tampon de logs pour la console intégrée
├── app/           # LOGIQUE SANS GPU : AppState, picking, Play, IA, input, build_config
├── gfx/           # rendu wgpu : renderer, mesh, camera, shaders WGSL
├── scene/         # Scene/SceneObject, composants, import glTF, sérialisation JSON
├── runtime/       # mode Play : physics (rapier3d), audio (kira), sfx synthétisés
└── editor/        # UI egui (desktop) : panneaux, export, readiness
```

**Règle d'or du projet** (documentée dans HANDOFF.md et respectée dans le code) : la
logique (`app/`, `scene/`) ne dépend pas du GPU ; tout ce qui touche `wgpu` reste dans
`gfx/`. C'est le pilier de la portabilité mobile.

Points notables :
- Pas d'ECS : la scène est un simple `Vec<SceneObject>` avec des composants
  optionnels (`Option<Controller>`, `Option<Combat>`, …) — choix assumé et documenté.
- Chargements lourds (glTF, audio, scènes) sur threads de fond, sûreté garantie par
  `Send`/`Sync`.
- Deux gros fichiers concentrent la complexité : [app/mod.rs](src/app/mod.rs)
  (~4 100 lignes) et [editor/mod.rs](src/editor/mod.rs) (~2 800 lignes).

## 6. Qualité, tests, CI

| Critère | État observé |
|---|---|
| Tests unitaires | **70 tests**, dont des tests de *gameplay* remarquables (nommés en intention : `attack_windup_finally_guarantees_risk_in_a_1v1`, `ai_chaser_actively_closes_distance_to_the_player`…) |
| Résultat après la passe de correction | ✅ **70 verts** (1 était rouge à la première passe — bug réel de gameplay, corrigé, cf. §7.1) |
| CI GitHub Actions | ✅ [ci.yml](.github/workflows/ci.yml) : `fmt --check`, `clippy -D warnings`, `cargo test`, **plus cross-build `aarch64-linux-android` et `aarch64-apple-ios`** à chaque push. Un workflow release existe aussi. |
| Documentation | ✅ Excellente : README (vision, comparatif Bevy, état honnête par plateforme), HANDOFF.md (passation), SPRINTS.md + ROADMAP_SPRINTS.md (44 sprints tracés), packaging/EXPORT.md |
| Commentaires | Denses et en français, expliquant le *pourquoi* (les commits d'audit gameplay renvoient à audit_sprint.md) |
| Historique git | 146 commits incrémentaux, messages descriptifs, développement par sprints (Phases A→H livrées, Phase I planifiée) |

## 7. Risques et points faibles — état après correction (4 juillet 2026)

Les points relevés par la première passe de cet audit ont été traités ou requalifiés :

1. ✅ **Test rouge — corrigé.** `attack_windup_finally_guarantees_risk_in_a_1v1`
   échouait pour une cause racine réelle de gameplay : la détection de morsure testait
   « centre du joueur **dans** l'AABB du monstre », or les colliders solides (joueur et
   chasseur ont tous deux un corps rigide) empêchent toute interpénétration — un
   monstre-chasseur ne mordait donc *jamais*, même en contact continu. La détection de
   trigger est désormais une **intersection d'AABB** (`Scene::world_aabb_intersects`,
   le contact suffit), avec un test de régression dédié
   (`chasing_monster_with_solid_body_can_bite_the_player_on_contact`). **70/70 tests
   verts**, clippy et fmt propres.
2. ✅ **Faux positif — déjà résolu par le projet.** La simulation est **déjà découplée**
   du rendu : pas fixe 1/60 s avec accumulateur et cap de sous-pas
   (`fixed_substeps`, src/app/mod.rs, testé par
   `fixed_substeps_is_framerate_independent`). Le Sprint 45 est livré ; la première
   passe de l'audit citait la roadmap sans vérifier le code.
3. ✅ **Faux positif.** Après vérification fichier par fichier, **tous** les `unwrap()`
   de `src/` sont dans les modules `#[cfg(test)]` — zéro dans le code de production.
   Le Sprint 46 (durcissement) est bien passé par là.
4. ⏳ **Reste ouvert.** Une extraction du gameplay combat (attaque, manches) vers un
   module `src/app/combat.rs` a été réalisée puis **annulée** : une session d'édition
   parallèle a réécrit `app/mod.rs` pendant l'opération, et rejouer un déplacement de
   ~1 000 lignes en concurrence aurait risqué de corrompre son travail. La
   recommandation demeure : extraire attaque/manches/IA de `app/mod.rs` (~4 100 l.)
   dans un module dédié, à faire hors de toute édition concurrente.
5. ⏳ **Non corrigeable ici** — gyroscope Android réel : nécessite l'intégration du
   capteur natif et un device de test (Sprint 48, inchangé).
6. ⏳ **Non corrigeable ici** — signature store : nécessite certificats, comptes
   développeur et secrets CI (Sprint 49, inchangé). Assumé et documenté.
7. ✅ **Faux positif.** `packaging/release.keystore` existe sur le disque mais est
   **déjà dans `.gitignore` et non suivi par git** (il est régénéré par
   `build_apk.sh`). Seule reste la consigne d'hygiène : ne jamais réutiliser cette clé
   de test (mot de passe `android` dans `Cargo.toml`) pour une vraie clé store.
8. ✅ **Faux positif.** `packaging/ios-xcode/build/` et le `.xcodeproj` généré sont
   **déjà gitignorés et non suivis** ; la première passe listait le disque, pas le
   dépôt (`git ls-files` fait foi).

## 8. Verdict

Projet **remarquablement sain pour un moteur solo** : la raison d'exister est claire et
tenue (pédagogie, maîtrise, portabilité), l'architecture respecte sa propre règle de
découplage logique/GPU, la doc de passation est réelle, la CI vérifie format, lints,
tests et cross-compilation mobile à chaque push, et la roadmap identifie honnêtement
ses propres dettes. Après la passe de correction de cet audit : **70 tests verts**,
`clippy --all-targets -D warnings` propre, `fmt` propre, et un vrai bug de gameplay
(morsure impossible entre corps solides) corrigé à la racine avec test de régression.
Restent : l'extraction du gameplay combat hors de `app/mod.rs` (reportée à cause d'une
édition concurrente, cf. §7.4) et, planifiés et hors de portée d'un audit local, le
capteur gyroscope Android (Sprint 48) et la distribution signée store (Sprint 49).
