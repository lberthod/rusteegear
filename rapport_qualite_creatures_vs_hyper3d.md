# Rapport : rapprocher la qualité graphique des créatures du niveau du diablotin Hyper3D

Contexte : comparaison entre le renard procédural [creature62.glb](assets/models/creature62.glb)
(généré par [gen_creature62_fox.py](scripts/blender/gen_creature62_fox.py), style bas-poly du
bestiaire existant) et le diablotin fourni en référence, généré par IA (Hyper3D Rodin) via
Blender MCP.

## Pourquoi l'écart existe

| | Bestiaire actuel (renard) | Diablotin (référence) |
|---|---|---|
| Origine | Primitives assemblées à la main (sphères/cônes/cylindres), script Python | Génération IA (Hyper3D Rodin), maillage sculpté |
| Topologie | Facettée, low-poly volontaire | Continue, subdivisée, organique |
| Shading | Flat/plat par facette | Lissé (Shade Smooth + Subdivision Surface) |
| Vertices | Réduits exprès (~60 % en moins, `_lod()` dans [creature_kit.py](scripts/blender/creature_kit.py)) | Non contraint |
| Rig | 1 os par pièce, poids 1.0, conçu pour le skinning GPU du moteur | Aucun rig (mascotte statique) |
| Export | GLB avec clips Idle/Walk bouclables, prêt à charger dans le jeu | GLB brut, pas de squelette |

L'écart n'est donc pas un écart d'effort : c'est deux pipelines différents avec des objectifs
différents (asset de jeu skinné et budgeté vs. rendu vitrine).

## Option A — Rendu vitrine seulement (recommandé si tu veux juste un joli PNG)

- Garder [creature62.glb](assets/models/creature62.glb) exactement tel quel pour le jeu.
- Générer un second rendu du même mesh avec `Shade Smooth` + modificateur `Subdivision Surface`
  (2 niveaux) + matériau plus glossy (roughness plus basse, un soupçon de spéculaire) + un
  éclairage à 3 points au lieu de 2 soleils plats.
- **Coût** : ~15 min, aucun risque, ne touche pas au GLB exporté ni au pipeline de skinning.
- **Résultat attendu** : le renard reste "cartoon", mais perd le facettage dur, se rapproche
  visuellement d'un mascot lissé (pas du réalisme du diablotin, mais un cran au-dessus).
- **Limite** : le PNG vitrine ne reflète plus exactement ce que verra le joueur en jeu.

## Option B — Vrai mesh organique sculpté/subdivisé, intégré au jeu

**Correction après vérification du moteur** : contrairement à ce qu'affirmait la première
version de ce rapport, le moteur supporte déjà le skinning glTF standard à **4 os/poids par
vertex avec vrai blending** (`Vertex::joints`/`weights` dans
[mesh.rs:52-69](src/gfx/mesh.rs), `compute_joint_matrices_blended_into` dans
[import.rs:735](src/scene/import.rs)). La règle "1 os/pièce, poids 1.0" n'est **pas** une
limite du moteur : c'est juste un choix d'écriture de `creature_kit.py`, plus simple à coder
à la main pour des primitives rigides. Le moteur, l'export glTF (`export_skins=True`) et le
chemin de rendu géreraient déjà un dégradé de poids aux articulations sans aucun changement
technique.

Ça rend Option B réaliste comme vraie pipeline reproductible, pas un one-shot :

1. **Modélisation** : mesh continu par créature — corps/tête/pattes fusionnés via modificateur
   `Subdivision Surface`, ou un `Skin Modifier` le long de courbes/os, au lieu d'assembler des
   primitives séparées et de les `join()` en un mesh à coutures dures.
2. **Poids** : remplacer `vertex_groups.new(...).add(range(...), 1.0, "REPLACE")` par
   `bpy.ops.object.parent_set(type='ARMATURE_AUTO')` (Automatic Weights de Blender) — calcule
   un vrai dégradé de poids aux articulations au lieu d'un poids rigide par pièce.
3. **Export** : `build_creature()` exporte déjà avec `export_skins=True` — aucun changement
   nécessaire, le format transporte nativement les 4 influences par vertex.
4. **Coût réel** : uniquement la modélisation/rigging par créature (quelques heures si sculpté
   à la main), pas de refonte moteur. Alternative scriptable en masse : garder une génération
   procédurale mais organique (metaballs fusionnées + retopo automatique, ou mesh loft le long
   de courbes) pour rester dans l'esprit "un script = un pack de créatures".
5. **Vigilance** : le budget de vertices existe pour une vraie raison (coût GPU du skinning +
   sondes physiques TriMesh des créatures, cf. `_lod()` dans creature_kit.py) — un mesh
   organique doit rester raisonnable en densité, pas juste viser le photoréalisme.
- **Verdict** : faisable en pipeline propre, mais plus lourd à écrire/maintenir qu'un script de
  primitives ; à réserver aux créatures où la silhouette lisse compte vraiment (héros, boss),
  pas nécessairement à tout le bestiaire.

## Option C — Activer Hyper3D Rodin (le vrai chemin du diablotin)

- Dans Blender : panneau latéral **N** → onglet **BlenderMCP** → cocher
  **« Use Hyper3D Rodin 3D model generation »** → relancer la connexion MCP.
- Nécessite une clé API Hyper3D (Rodin) configurée côté add-on — à vérifier si tu en as déjà
  une, sinon il faut en créer une sur leur service.
- Je pourrai alors lancer `generate_hyper3d_model_via_text` pour un animal, dans un style
  proche du diablotin.
- **Limite** : le résultat sera un mesh statique sans rig ni animation, comme le diablotin
  fourni — il faudrait ensuite un travail manuel de retopologie/rig pour l'intégrer au jeu
  (le mesh généré n'aura pas la structure "1 os/pièce" attendue par le moteur).
- **Usage réaliste** : bien adapté pour du concept art, une mascotte, un asset de menu/UI —
  pas pour une créature de jeu skinnée directement.

## Recommandation

- Retouche rapide et sans risque : Option A.
- Investissement ciblé sur quelques créatures clés (héros, boss, mascotte jouable) : Option B —
  c'est la seule option qui donne un vrai mesh organique **et** reste intégrable au jeu tel
  quel, le moteur étant déjà prêt.
- Concept art / mascotte hors gameplay uniquement : Option C (Hyper3D), une fois la clé API en
  place.
