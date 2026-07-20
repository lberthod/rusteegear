# Assets et pipeline 3D (2026-07-20)

*Sévérité : échelle commune définie dans [00_SYNTHESE.md](00_SYNTHESE.md). Les risques A1
et A3 ont été contre-vérifiés ligne par ligne dans le code : **confirmés, sans garde-fou
existant**.*

## Inventaire

- `assets/models/` : **160 Mo, 482 GLB** + 344 vignettes PNG (dossier plat).
  Par famille : nature 143, creature 113, monster 45, siege 40, hamlet 40, item 30,
  fauna 29, grotto 20, shore 20. Plus gros : `creature13.glb` 2,7 Mo.
- `assets/bundle/` : **22 Mo, 715 GLB compressés zstd** (un par un, pas d'archive).
- `assets/player_scene.json` : 1,65 Mo, **321 imports `bundle://`** — la scène servie.
- `scripts/blender/` : **68 scripts** (~1 Mo) — modules communs (`hamlet_common`,
  `organic_common`, `creature_kit`, `siege_anim_common`), ~50 `gen_*.py`, 5 scripts QA
  (`check_*.py` : mesh joint, z≥0, pas de texture, clips bouclés, ≤128 os).

## Mécanisme d'embarquement (`src/assets.rs`)

- `include_dir!("assets/bundle")` fige tout le bundle dans le binaire à la compilation.
- Lecture : `bundle://<clé>` → décompression zstd à la volée via `ruzstd` (pur Rust,
  compatible wasm32). Écriture : `editor::export::copy_to_bundle` via crate `zstd` (desktop).
- `player_scene.json` embarqué séparément par `include_str!` (`Scene::embedded_player()`).

## État des packs (docs vs disque) — tout est livré

| Pack | Claim doc | Disque | Verdict |
|---|---|---|---|
| Hameau | 40 intégrés | 40 GLB | ✅ |
| Siège | 40/40 | 40 GLB | ✅ |
| Siège animé | 12/12 squelette+clip | rig+clip vérifiés sur échantillon | ✅ |
| Organique | 40/40 (grotto+shore) | 20+20 GLB | ✅ |

**Intégration en jeu** (les docs du 18/07 sont antérieures à l'intégration du 19/07) :
la scène servie contient siege 37/40, grotto 20/20, shore 14/20, hamlet 27/40,
nature 92/143, creature 72/113, fauna 29/29, item 30/30 → **321 imports intégrés**,
~160 GLB générés mais non placés.

⚠️ **Deux comptes d'« orphelins » à ne pas confondre** :
- **~160 GLB de `models/` non intégrés** (482 générés − 321 placés) : du contenu en
  réserve, sans coût — rien à faire.
- **~394 GLB de `bundle/` non référencés** (715 fichiers − 321 clés utilisées par
  `player_scene.json`) : du poids mort **embarqué dans le binaire** — c'est la cible de la
  purge A2. Le bundle (715) dépasse `models/` (482) parce que chaque resynchro renumérote
  les imports en `mNN_` sans purger l'ancienne numérotation : les générations successives
  s'accumulent.

Note qualité : bestiaire procédural low-poly assumé (choix de pipeline, pas une limite
moteur — skinning 4 os/poids supporté). Option B (mesh organique métaballes) recommandée
pour héros/boss uniquement. `siege_common.py` promis par la doc animation est absent
(rig des 12 animés non factorisé).

## Risques priorisés

| # | Sévérité | Risque |
|---|---|---|
| A1 | 🔴 **Critique** | **Ré-export destructif de la scène servie.** `bundle_scene_json` (`src/editor/export.rs:819`) fait `remove_dir_all(assets/bundle/)` puis recopie chaque import — mais `copy_to_bundle` retourne `Ok(None)` pour tout chemin déjà `bundle://` (`:877`). Or `player_scene.json` ne contient QUE des `bundle://`. **Ré-exporter le player depuis l'éditeur vide le bundle sans rien recopier** → le binaire suivant n'a plus aucun asset. Rien ne l'empêche aujourd'hui. |
| A2 | 🟠 Élevé | **~394 GLB orphelins embarqués** : bundle 715 fichiers vs 321 référencés. Bloat du binaire + risque de collision de clés `mNN` entre numérotations successives. |
| A3 | 🟠 Élevé | **Pas de `build.rs` `rerun-if-changed=assets/bundle`** : modifier le bundle sans le rituel manuel `touch src/assets.rs` produit un binaire aux assets silencieusement périmés. Piège structurel déjà à l'origine d'incidents (cf. mémoire d'équipe). |
| A4 | 🟡 Moyen | **182 Mo de binaires versionnés** : `.gitignore` n'exclut ni `models/` ni `bundle/`, contredisant le commentaire d'`assets.rs:173` (« jamais versionné tel quel »). Historique git alourdi. |
| A5 | 🟡 Moyen | **Désynchro JSON ↔ bundle** : le test-outil `#[ignore] sync_embedded_scene_hameau_from_the_demo` (`src/scene/mod_tests.rs:1894`) régénère `player_scene.json` en renumérotant tous les imports `mNN` sans toucher `bundle/` — l'étape 3 manuelle d'`integration_siege_scene.md` doit être rejouée. Aggravé par le `player_scene.json` divergent du worktree `.claude/worktrees/…`. |

## Pièges d'export confirmés dans le code (à connaître)

- **Y-up** : `export_yup=True`, sol Blender z=0.
- **Bind-pose** : `ad.action = None` avant export, sinon la pose écrase la piste NLA.
- **Pas de fallback keyframe d'objet** : `load_gltf_clips` (`src/scene/import.rs:472-488`)
  ignore tout ce qui n'est pas skin+armature — un élément animé sans mini-squelette ne
  bougera jamais en jeu.

## Actions recommandées

1. **Garde-fou sur A1** : dans `bundle_scene_json`, refuser (ou court-circuiter en no-op)
   le ré-export d'une scène dont les imports sont déjà `bundle://`, ou faire recopier les
   octets depuis le BUNDLE embarqué. + un test de non-régression. *À faire avant tout
   autre travail d'export.*
2. **`build.rs`** avec `cargo:rerun-if-changed=assets/bundle` (ferme A3, supprime le rituel).
3. **Purge du bundle** : script qui compare les clés de `player_scene.json` aux fichiers
   de `bundle/` et supprime les orphelins (ferme A2). Le brancher dans la CI en mode check.
4. **Git LFS ou exclusion** pour `assets/models/` (A4) — décision à prendre en connaissance
   de cause (le bundle, lui, doit rester versionné tant qu'il est embarqué).
5. **Automatiser l'étape 3 d'intégration** (copie `models/x.glb` → `bundle/mNN_x.glb`)
   dans le test de resynchro lui-même (ferme A5).
