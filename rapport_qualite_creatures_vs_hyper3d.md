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

- Remplacer l'assemblage de primitives par un mesh unique sculpté ou subdivisé en dur, avec
  plus de vertices, une topologie continue tête/corps/pattes.
- **Coût** : plusieurs heures par créature (modélisation), plus une refonte du skinning (le
  moteur n'accepte aujourd'hui qu'un poids 1.0 par os/pièce — un mesh continu demanderait un
  vrai skinning multi-os avec poids dégradés, absent du moteur actuel d'après la mémoire du
  pipeline squelette).
- **Risque** : casse la convention "1 os = 1 pièce, poids 1.0" documentée dans
  [creature_kit.py](scripts/blender/creature_kit.py) et le budget de vertices pensé pour le
  coût GPU du skinning + les sondes physiques (TriMesh) des créatures en jeu.
- **Verdict** : pas recommandé sans discussion préalable sur le moteur — gros chantier, pas
  un simple réglage de script.

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

Pour une créature **jouable** : rester sur le pipeline procédural (Option A pour améliorer le
rendu vitrine sans rien casser). Pour une **mascotte ou un visuel de présentation** hors
gameplay : Option C (Hyper3D) une fois la clé API en place.
