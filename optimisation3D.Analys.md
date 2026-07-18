# Analyse d'optimisation 3D — scène `mmorpg_demo`

Analyse en lecture seule du pipeline de rendu (`src/gfx/`) et de la scène actuelle
(`Scene::mmorpg_demo()` dans `src/scene/demos.rs`), la « map » servie en jeu.
Chiffres mesurés en compilant la lib et en sondant `Scene::mmorpg_demo()` directement
(pas d'estimation à vue) : **887 objets**, **315 meshes glTF importés** (320 clés
mesh/texture au total), **201 objets skinnés simultanés**, **26 créatures** nommées.

---

## Constat prioritaire — capacité skinnée dépassée par le contenu actuel

**`MAX_SKINNED_INSTANCES = 160`** (`src/gfx/renderer.rs:160`) alors que la scène contient
**201 objets skinnés** (créatures + PNJ errants + décor animé des packs menagerie/monstres).
Si les 201 sont simultanément dans le frustum — le scénario de vue large/plongée qui a déjà
justifié 3 relèvements successifs de cette constante par le passé (8 → 32 → 96 → 160,
commentaires détaillés à `renderer.rs:137-159`) — **jusqu'à 41 objets skinnés seraient
silencieusement non dessinés**. Ce n'est pas un crash : le garde-fou existe déjà
(`skinned_dropped_last_frame`, remonté dans le Profiler avec alerte, `src/editor/mod.rs:207-209`,
`src/editor/windows.rs:152`), mais c'est un appauvrissement visible de la scène (créatures qui
disparaissent) si la marge n'a pas été revalidée depuis les derniers ajouts de contenu (paysage
de prairie, faune ambiante — commits récents `f7a3de0`, `0da59dc`).

**Action immédiate recommandée** : ouvrir le panneau Profiler en jeu sur `mmorpg_demo`, se placer
en vue large/plongée, lire `skinned_dropped`. Si > 0, relever `MAX_SKINNED_INSTANCES` (changement
d'une ligne, `renderer.rs:160`, ex. → 224 ou 256) le temps de traiter le point structurel ci-dessous.

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
