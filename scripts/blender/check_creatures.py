"""Contrôle qualité des créatures maison (creature21-26, 32-51) — réimport glb.

Vérifie pour chaque glb, sur le fichier exporté (pas la scène de génération) :
- clips « Idle » et « Walk » présents (pistes NLA nommées) ;
- boucle parfaite : pose échantillonnée au premier frame == dernier frame de
  chaque clip (comparaison des matrices de tous les os, tolérance 1e-3) —
  sinon le raccord de boucle « saute » visiblement en jeu ;
- garde au sol : aucun vertex sous z = -0,001 (un mesh qui perce le sol fige
  la créature par dépénétration du TriMesh kinématique, cf. mémoire) — les
  flotteurs assumés (fantôme, poissons…) ont par construction un min-z > 0 ;
- budgets : vertices du mesh et nombre d'os (JOINT_CAPACITY = 128 côté moteur).

Sortie : une ligne OK/ECHEC par créature + « QA DONE n/26 » ; code de retour
non nul si au moins un échec (utilisable en CI si besoin).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/check_creatures.py
"""

import os
import sys

import bpy

MODELS = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)
IDS = [21, 22, 23, 24, 25, 26] + list(range(32, 52))
CLIPS = {"Idle": 40, "Walk": 24}

ok_count = 0
failures = []
for n in IDS:
    path = os.path.join(MODELS, f"creature{n}.glb")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=path)
    scene = bpy.context.scene
    scene.render.fps = 24
    errors = []

    meshes = [o for o in scene.objects if o.type == "MESH" and "Icosphere" not in o.name]
    arm = next((o for o in scene.objects if o.type == "ARMATURE"), None)
    if arm is None:
        errors.append("pas d'armature")
    verts = sum(len(o.data.vertices) for o in meshes)
    min_z = min((o.matrix_world @ v.co).z for o in meshes for v in o.data.vertices)
    if min_z < -0.001:
        errors.append(f"vertex sous le sol (min z = {min_z:.3f})")
    if arm is not None and len(arm.pose.bones) > 128:
        errors.append(f"{len(arm.pose.bones)} os > JOINT_CAPACITY 128")

    if arm is not None:
        ad = arm.animation_data
        if ad is not None:
            # L'importeur glTF laisse un clip en action ACTIVE, qui écrase la
            # NLA à l'évaluation : sans ce None, on échantillonne toujours ce
            # clip-là quelle que soit la piste démutée.
            ad.action = None
        tracks = {t.name: t for t in ad.nla_tracks} if ad else {}
        for clip, length in CLIPS.items():
            if clip not in tracks:
                errors.append(f"clip {clip} absent")
                continue
            for t in ad.nla_tracks:
                t.mute = t.name != clip
            # Bornes réelles du strip réimporté (glTF t=0 → frame 0, pas 1).
            strip = tracks[clip].strips[0]
            first, last = int(strip.frame_start), int(strip.frame_end)
            poses = []
            for frame in (first, last):
                scene.frame_set(frame)
                bpy.context.view_layer.update()
                dg = bpy.context.evaluated_depsgraph_get()
                ev = arm.evaluated_get(dg)
                poses.append([pb.matrix_basis.copy() for pb in ev.pose.bones])
            drift = max(
                abs(a[i][j] - b[i][j])
                for a, b in zip(*poses)
                for i in range(4)
                for j in range(4)
            )
            if drift > 1e-3:
                errors.append(f"boucle {clip} ouverte (écart {drift:.4f})")

    size = os.path.getsize(path)
    if errors:
        failures.append(n)
        print(f"ECHEC creature{n}: {'; '.join(errors)}")
    else:
        ok_count += 1
        print(f"OK creature{n}: {verts} verts, "
              f"{len(arm.pose.bones)} os, {size // 1024} Ko, min z {min_z:.3f}")

print(f"QA DONE {ok_count}/{len(IDS)}")
if failures:
    sys.exit(1)
