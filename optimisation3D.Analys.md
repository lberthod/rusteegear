# Analyse d'optimisation 3D — scène `mmorpg_demo`

Analyse en lecture seule du pipeline de rendu (`src/gfx/`) et de la scène actuelle
(`Scene::mmorpg_demo()` dans `src/scene/demos.rs`), la « map » servie en jeu.
Chiffres mesurés en compilant la lib et en sondant `Scene::mmorpg_demo()` directement
(pas d'estimation à vue) : **887 objets**, **315 meshes glTF importés** (320 clés
mesh/texture au total), **201 objets skinnés simultanés**, **26 créatures** nommées.

---

## Constat prioritaire — capacité skinnée dépassée par le contenu actuel ✅ traité

**Historique (avant Phase A)** : `MAX_SKINNED_INSTANCES = 160` alors que la scène contient
**201 objets skinnés** (créatures + PNJ errants + décor animé des packs menagerie/monstres) —
en vue large/plongée avec les 201 simultanément dans le frustum, jusqu'à 41 objets skinnés
auraient été silencieusement non dessinés.

**Traité (Phase A, Sprint 1, 18 juillet 2026)** : `MAX_SKINNED_INSTANCES` relevée à **256**
(`src/gfx/renderer.rs:174`), marge de ~55 au-dessus des 201 objets skinnés mesurés dans le code.
Re-mesuré à `skinned_dropped == 0` en vue large/zénithale (Sprint 1) et confirmé par la mesure
headless de la Phase F ci-dessous (`skinned_dropped: 0`). Cf.
[sprintoptimation3daudit10h.md § Phase A](sprintoptimation3daudit10h.md#phase-a).

---

## Mesure AVANT/APRÈS (Phase F, Sprint 7 — 18 juillet 2026)

Protocole détaillé, écarts de méthode et discussion : voir
[sprintoptimation3daudit10h.md § Phase F](sprintoptimation3daudit10h.md#phase-f). Résumé :

| Métrique | AVANT (Phase 0, vue large en jeu) | APRÈS (Phase F, benchmark headless reproductible) |
|---|---|---|
| `skinned_dropped` | 0 | **0** (confirmé, marge ×1,3 avec `MAX_SKINNED_INSTANCES = 256`) |
| `gpu_draw_calls` | ~382 (Sprint 0) / ~590 (Sprint 1, vue différente) | **592** (887 objets scène, vue englobant le plus d'objets possible sans dépasser le plan éloigné à 100 m) |
| FPS | 125 (Sprint 0) / 67 (Sprint 1) — mesuré en jeu, fenêtré, vsync/present inclus | 156 FPS-équivalent — **non comparable directement** : boucle headless sans fenêtre/vsync/UI egui, cf. limite méthodologique ci-dessous |
| Temps GPU par passe | Non mesuré (timestamp queries désactivées, gel Metal) | Toujours indisponible (`gpu_profiling = false`, non réactivé) |
| Compression texture (Phase E) | — | **Sans effet mesurable sur `mmorpg_demo`** : 0 objet de cette scène n'utilise `obj.texture` (tous les meshes visibles sont des imports glTF sans texture image via ce chemin) |

**Limite méthodologique assumée** : les FPS de Phase 0/Sprint 1 viennent d'une session interactive
réelle (fenêtre, vsync, egui) pilotée manuellement ; la mesure Phase F vient d'un benchmark headless
reproductible (`cargo run --release --example phase_f_measure`, ajouté à ce sprint) qui n'a ni
fenêtre ni present — les FPS ne sont donc pas comparables terme à terme entre les deux. En
revanche `gpu_draw_calls` et `skinned_dropped` sont des compteurs GPU exacts, comparables. Note :
`render_scene_headless` ne renseignait jusqu'ici jamais `last_frame_draw_calls` (toujours 0) —
corrigé dans ce sprint (`src/gfx/renderer.rs`) pour que ce benchmark ait un sens.

**Non couvert par cette mesure** : le pack « siège du hameau » (40 assets `siege_*`,
sprints 0-7, complet et commité le 18 juillet 2026 — postérieur à ce benchmark) a été ajouté après
le benchmark ci-dessus — décor statique non skinné confirmé (`check_siege_pack.py` : un seul mesh
joint par asset, aucun skin), mais pas encore intégré à `mmorpg_demo`/la scène servie (chantier
séparé, non commencé) donc pas encore mesuré en conditions réelles. Le
« re-test cumulé » et la « revalidation de la marge avec le contenu final » restent donc à faire
tels quels (Phase G, Sprints 11-12 de [sprintreflecion.md](sprintreflecion.md#phase-g),
non cochés) — ne pas considérer les 201 objets skinnés/887 objets ci-dessus comme définitifs tant
que ces deux sprints ne sont pas passés après stabilisation du contenu de scène.

---

## 1. Draw calls / Batching

**État actuel.** Les objets **statiques** sont triés par `(mesh_key, texture)` (`src/gfx/passes.rs:112-122`)
et dessinés en **plages contiguës d'instances GPU** (`draw_indexed` par plage, `renderer.rs:2375-2386`)
— du vrai instancing, malgré 315 meshes distincts. Le compteur `scene_draw_calls` est déjà exposé
dans le Profiler.

Les objets **skinnés** en sortent explicitement (« palette de joints incompatible avec le batching
par instances », `renderer.rs:1315-1317`) : **un `draw_indexed` par instance skinnée visible**
(`draw_skinned_objects`, `renderer.rs:860-885`), doublé pour la passe d'ombre
(`draw_skinned_shadow_objects`, `renderer.rs:910-928`).

**Impact.** Les statiques sont déjà bien optimisés. Le skinné (jusqu'à 201 instances) est le vrai
poste de coût CPU (soumission de commandes) — gérable sur desktop, plus sensible sur mobile/Android
(ciblé par le projet, cf. `Cargo.toml`).

**Recommandations.**
1. **Prioritaire** : instancing GPU pour le décor animé « statique en place » (PNJ errants, monstres
   décoratifs en `Idle` seul, packs menagerie) — matrices de joints échantillonnées dans une texture
   indexée par instance, plutôt qu'un draw par objet. Résout draw calls *et* capacité en même temps
   (voir constat prioritaire ci-dessus).
2. À défaut, séparer un chemin « anim locale simple » (ex. respiration, quelques os) rendu sans draw
   individuel, réservé aux vraies créatures IA pour le chemin actuel.

## 2. Culling

**État actuel.** Frustum culling correct (Gribb-Hartmann, `passes.rs:25-72`) appliqué aux statiques
(`renderer.rs:1332-1335`) et aux skinnés (`renderer.rs:1348-1356`, avec AABB de bind-pose plutôt que
pose animée — simplification assumée et documentée). Budget de lumières ponctuelles déjà en place
(`nearest_point_lights`, `renderer.rs:1224-1226`). **Pas d'occlusion culling**, **pas de culling par
distance** séparé du frustum.

**Impact.** Le frustum culling limite déjà bien la charge en vue normale. Le risque concret est la
vue large/plongée où une grande partie du hameau + de la faune entrent dans le frustum en même
temps — c'est justement le scénario derrière le constat prioritaire ci-dessus. Sans occlusion
culling, des objets cachés derrière les murs du hameau fortifié sont quand même envoyés au GPU.

**Recommandations.**
1. Culling par distance simple (rayon par type d'objet : herbe/fougères à faible rayon, arbres/rochers
   à rayon moyen) — peu coûteux, réduit la charge en vue large sans occlusion culling complet.
2. Occlusion culling : pas prioritaire à 887 objets ; à reconsidérer seulement si un profiling réel
   montre un goulot GPU en overdraw.

## 3. LOD (Level of Detail)

**État actuel.** **Aucun LOD géométrique** — chaque objet est rendu à pleine résolution de triangles
quelle que soit la distance caméra. Le seul « LOD » existant concerne les lumières et la qualité
globale (MSAA), pas la géométrie.

**Impact.** Modéré à l'échelle actuelle (une seule zone). Devient le premier levier à activer si la
scène s'agrandit (nouvelles zones) ou si le nombre de skinnés visibles continue de grimper.

**Recommandation.** LOD/impostor pour les meshes les plus instanciés en masse : `nature_grass_tuft.glb`
(112×), `nature_fern.glb` (69×), `nature_reeds.glb` (19×) — réduirait le fill-rate du feuillage dense
à distance.

## 4. Skinning / Animation

**État actuel.** Skinning GPU (storage buffer de joints, `JOINT_CAPACITY = 128` os/instance,
`renderer.rs:131`). Palette de joints recalculée et réécrite **chaque frame** pour chaque instance
skinnée visible (`prepare_skinned_draws`, sans skip-rebuild contrairement au chemin statique qui a
un hash de skip, `renderer.rs:1278-1286`) — correct puisque l'anim change réellement à chaque frame,
mais confirme que ce poste scale linéairement avec le nombre d'instances skinnées visibles.

Voir « Constat prioritaire » ci-dessus pour le chiffre clé (201 skinnés vs capacité 160).

## 5. Textures / Mémoire GPU

**État actuel.** Format uniforme `Rgba8UnormSrgb` (`renderer.rs:487`), **aucune compression GPU**
(pas de BC7/ASTC/ETC2). Mipmaps générés à l'import (`mipgen_pipeline`, `renderer.rs:336-340`).
Cache de texture par chemin (`sync_textures`, `renderer.rs:1148-1179`) — pas de duplication GPU par
instance, déjà bien fait. 158 images, 329 fichiers `.glb` (106 Mo) sur disque.

**Impact.** Pas un problème de VRAM sur desktop à cette échelle. Devient un vrai risque pour tout
build Android sérieux (VRAM contrainte, `min_sdk_version` déjà configuré dans `Cargo.toml`).

**Recommandation.** Compression de texture (ASTC mobile, BC7 desktop) à ajouter au pipeline d'import
avant tout ciblage Android sérieux — pas urgent sur desktop.

## 6. Scène / Nombre d'objets

887 objets, densité de décor construite avec rejection-sampling anti-chevauchement
(`scatter`/`scatter_clustered`/`scatter_each`, `demos.rs:5001-5707`), pas de placement naïf. Pas de
doublons de mesh qui devraient être instanciés et ne le sont pas — le point faible est
spécifiquement le skinné (sections 1 et 4), pas le placement statique.

**Pas de chunking/streaming** : toute la scène est construite en mémoire/GPU au chargement. Choix
raisonnable pour une zone unique de cette taille ; à reconsidérer seulement si le monde s'étend en
plusieurs zones contiguës.

## 7. Profiling existant

Outillage déjà solide, intégré à l'éditeur : panneau « 📊 Profiler FPS » (`src/editor/windows.rs:75-152`,
historique FPS, timestamps GPU par passe, compteur de draw calls, alerte `skinned_dropped` déjà
testée unitairement à `windows.rs:1745-1761`). Coût des timestamp queries limité à l'ouverture du
panneau (`renderer.rs:183-188`).

**Limite de cette analyse** : pas de mesure FPS réelle en conditions de jeu (le binaire fenêtré n'a
pas été lancé, hors périmètre lecture-seule). Les chiffres ci-dessus sont des comptages d'objets/
instances mesurés dans le code, pas des FPS. Utiliser le Profiler existant en jeu pour obtenir le
chiffre réel plutôt que d'estimer.

---

## Priorités classées

1. **Vérifier et probablement relever `MAX_SKINNED_INSTANCES`** (`src/gfx/renderer.rs:160`) —
   201 skinnés mesurés vs capacité 160, risque concret de perte silencieuse en vue large.
2. **Instancing GPU du skinning** pour le décor animé statique — résout draw calls et capacité
   ensemble, plus structurant qu'un nouveau relèvement de constante.
3. **Culling par distance** en complément du frustum, pour les vues larges/plongée.
4. **LOD géométrique** pour les meshes les plus denses en instances (herbe/fougères), si un
   profiling réel confirme un goulot de fill-rate.
5. **Compression de texture GPU** — à traiter avant tout ciblage Android sérieux, pas urgent desktop.
