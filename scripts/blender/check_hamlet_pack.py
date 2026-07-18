"""Contrôle qualité du pack « hameau maison » (hamlet_*.glb) — réimport glb.

Contrairement à check_creatures.py (créatures riggées, clips Idle/Walk), ce
pack est du décor STATIQUE : pas d'armature ni d'animation à vérifier. Pour
chaque hamlet_*.glb, on vérifie sur le fichier exporté (pas la scène de
génération) :
- un seul objet mesh (join + transform_apply appliqués avant export, cf.
  hamlet_common.export — sinon load_gltf recolle les morceaux de travers,
  cf. mémoire blender-headless-asset-pipeline) ;
- aucun vertex sous z = -0,001 (garde au sol, même contrainte que les
  créatures : un mesh qui perce le sol perturbe la physique/les sondes) ;
- aucune texture référencée par les matériaux (seul base_color_factor est lu
  par le moteur, cf. src/scene/import.rs) ;
- budget indicatif : nombre de vertices (alerte, pas d'échec, si hors budget
  de la charte graphique : bâtiment ~800-2000, prop simple ~50-300,
  structure ~100-500 — cf. mémoire charte-graphique-assets-maison).

La liste des fichiers est découverte dynamiquement (glob hamlet_*.glb) : ce
script tourne dès le premier asset produit et grossit avec les sprints
suivants, pas besoin de maintenir une liste d'IDs à la main.

Sortie : une ligne OK/ECHEC par asset + « QA DONE n/n » ; code de retour non
nul si au moins un échec (utilisable en CI si besoin).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/check_hamlet_pack.py
"""

import glob
import os
import sys

import bpy

MODELS = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

BUDGETS = {
    # préfixe de nom de fichier -> (min, max) vertices indicatifs (alerte seule)
    "hamlet_": (30, 2200),
}


def budget_for(name):
    lo, hi = BUDGETS["hamlet_"]
    return lo, hi


paths = sorted(glob.glob(os.path.join(MODELS, "hamlet_*.glb")))

if not paths:
    print("QA DONE 0/0 (aucun hamlet_*.glb trouvé — normal avant le Sprint 1)")
    sys.exit(0)

ok_count = 0
failures = []
for path in paths:
    name = os.path.basename(path)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=path)
    scene = bpy.context.scene
    errors = []

    meshes = [o for o in scene.objects if o.type == "MESH"]
    if len(meshes) != 1:
        errors.append(f"{len(meshes)} objets mesh (attendu 1 — join manquant à l'export)")

    verts = sum(len(o.data.vertices) for o in meshes)
    if meshes:
        min_z = min((o.matrix_world @ v.co).z for o in meshes for v in o.data.vertices)
        if min_z < -0.001:
            errors.append(f"vertex sous le sol (min z = {min_z:.3f})")
    else:
        min_z = float("nan")

    textured = []
    for o in meshes:
        for slot in o.data.materials:
            if slot is None or slot.node_tree is None:
                continue
            for node in slot.node_tree.nodes:
                if node.type == "TEX_IMAGE":
                    textured.append(slot.name)
    if textured:
        errors.append(f"texture référencée sur {', '.join(sorted(set(textured)))}")

    lo, hi = budget_for(name)
    warn = None
    if not (lo <= verts <= hi):
        warn = f"hors budget indicatif ({verts} verts, attendu {lo}-{hi})"

    size = os.path.getsize(path)
    if errors:
        failures.append(name)
        print(f"ECHEC {name}: {'; '.join(errors)}")
    else:
        ok_count += 1
        suffix = f" — {warn}" if warn else ""
        print(f"OK {name}: {verts} verts, {size // 1024} Ko, min z {min_z:.3f}{suffix}")

print(f"QA DONE {ok_count}/{len(paths)}")
if failures:
    sys.exit(1)
