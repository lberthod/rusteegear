"""Contrôle qualité du pack « grottes & rives » (grotto_*.glb / shore_*.glb) —
réimport glb, calqué sur check_hamlet_pack.py.

Décor STATIQUE (pas d'armature/animation), mais généré par métaballes
(organic_common.organic_core) plutôt que par primitives dures — d'où un
budget de vertices plus généreux (la conversion metaball→mesh produit
naturellement plus de triangles qu'un assemblage de cubes/cylindres à faible
segmentation) et pas de vérif spécifique à la technique : le fichier exporté
est un mesh glTF ordinaire, les mêmes règles s'appliquent.

Vérifie pour chaque asset :
- un seul objet mesh (join + transform_apply à l'export) ;
- aucun vertex sous z = -0,001 ;
- aucune texture référencée (seul base_color_factor est lu par le moteur) ;
- budget indicatif de vertices (alerte, pas d'échec).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/check_organic_pack.py
"""

import glob
import os
import sys

import bpy

MODELS = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

BUDGET = (50, 3500)  # vertices indicatifs (métaballes converties = plus dense)

paths = sorted(glob.glob(os.path.join(MODELS, "grotto_*.glb"))) + sorted(
    glob.glob(os.path.join(MODELS, "shore_*.glb"))
)

if not paths:
    print("QA DONE 0/0 (aucun grotto_*/shore_*.glb trouvé — normal avant le Sprint 1)")
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

    lo, hi = BUDGET
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
