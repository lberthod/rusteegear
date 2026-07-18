"""Contrôle qualité du pack « siège du hameau » (siege_*.glb) — réimport glb.

Calqué sur check_hamlet_pack.py pour les 28 assets STATIQUES (un seul mesh
joint, pas de vertex sous le sol, pas de texture référencée, budget
indicatif). Étendu (Sprint 7, addendum creationAnimation3DBlendersuite.md)
pour les 12 assets ANIMÉS (squelette + clip) : présence du clip attendu,
boucle parfaite (comparaison de la pose au premier et dernier frame du strip
NLA, tolérance 1e-3 — même méthode que check_creatures.py), budget d'os
(JOINT_CAPACITY = 128 côté moteur). Un asset animé a plusieurs objets mesh
(un par pièce rigide skinnée) avant le join implicite du modificateur
armature — le contrôle "1 seul mesh" ne s'applique donc qu'aux statiques.

La liste des fichiers est découverte dynamiquement (glob siege_*.glb) : ce
script tourne dès le premier asset produit et grossit avec les sprints
suivants. La liste ANIMATED est celle de creationAnimation3DBlendersuite.md
(12 noms) ; tout fichier absent de cette liste est traité comme statique.

Sortie : une ligne OK/ECHEC par asset + « QA DONE n/n » ; code de retour non
nul si au moins un échec (utilisable en CI si besoin).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/check_siege_pack.py
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
    "siege_": (30, 2200),
}

JOINT_CAPACITY = 128

# Les 12 assets animés (squelette + clip) listés dans
# creationAnimation3DBlendersuite.md — tous les autres siege_*.glb restent
# des meshes statiques (pas de squelette inutile).
ANIMATED = {
    "siege_gate_closed.glb": "Idle",
    "siege_gate_burning.glb": "Idle",
    "siege_postern.glb": "Idle",
    "siege_portcullis.glb": "Idle",
    "siege_ember_cart.glb": "Walk",
    "siege_communal_brazier.glb": "Idle",
    "siege_wave_banner.glb": "Idle",
    "siege_mode_banner.glb": "Idle",
    "siege_team_pennant.glb": "Idle",
    "siege_rampart_torch.glb": "Idle",
    "siege_spawn_beacon.glb": "Idle",
    "siege_chief_cage.glb": "Idle",
}


def budget_for(name):
    lo, hi = BUDGETS["siege_"]
    return lo, hi


def check_static(meshes, errors):
    if len(meshes) != 1:
        errors.append(f"{len(meshes)} objets mesh (attendu 1 — join manquant à l'export)")


def check_animated(clip, scene, errors):
    arm = next((o for o in scene.objects if o.type == "ARMATURE"), None)
    if arm is None:
        errors.append("pas d'armature (asset listé comme animé)")
        return None
    if len(arm.pose.bones) > JOINT_CAPACITY:
        errors.append(f"{len(arm.pose.bones)} os > JOINT_CAPACITY {JOINT_CAPACITY}")
    ad = arm.animation_data
    if ad is None:
        errors.append(f"clip {clip} absent (pas d'animation_data)")
        return arm
    # L'importeur glTF laisse un clip en action ACTIVE, qui écraserait sinon
    # la piste NLA à l'évaluation (piège documenté, mémoire
    # blender-headless-asset-pipeline / check_creatures.py).
    ad.action = None
    tracks = {t.name: t for t in ad.nla_tracks}
    if clip not in tracks:
        errors.append(f"clip {clip} absent")
        return arm
    for t in ad.nla_tracks:
        t.mute = t.name != clip
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
    return arm


paths = sorted(glob.glob(os.path.join(MODELS, "siege_*.glb")))

if not paths:
    print("QA DONE 0/0 (aucun siege_*.glb trouvé — normal avant le Sprint 1)")
    sys.exit(0)

ok_count = 0
failures = []
for path in paths:
    name = os.path.basename(path)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=path)
    scene = bpy.context.scene
    scene.render.fps = 24
    errors = []

    # Icosphere parasite : forme d'affichage des os réimportée par erreur
    # dans bpy.data, absente du fichier réel (piège documenté, mémoire
    # blender-headless-asset-pipeline) — exclue comme dans check_creatures.py.
    meshes = [o for o in scene.objects if o.type == "MESH" and "Icosphere" not in o.name]

    clip = ANIMATED.get(name)
    arm = None
    if clip:
        arm = check_animated(clip, scene, errors)
    else:
        check_static(meshes, errors)

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
        extra = f", {len(arm.pose.bones)} os, clip {clip}" if arm else ""
        print(f"OK {name}: {verts} verts{extra}, {size // 1024} Ko, min z {min_z:.3f}{suffix}")

print(f"QA DONE {ok_count}/{len(paths)}")
if failures:
    sys.exit(1)
