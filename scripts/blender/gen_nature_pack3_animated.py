# Variantes ANIMÉES du pack nature3 : carillon à vent, pissenlit, glaçon qui
# goutte, lotus qui flotte, en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_nature_pack3_animated.py
#
# Sortie : assets/models/nature_{windchime_sway,dandelion_sway,icicle_drip,
# lotus_bob}.glb (+ un preview PNG par asset, pose de repos).
#
# Recette rig/NLA identique à gen_flora_pack_animated.py (elle-même reprise
# de gen_fairy_hero.py) : chaque partie skinnée à 100 % sur un os, UN clip
# « Idle » en boucle parfaite (première pose = dernière) poussé en piste
# NLA, export `export_animation_mode="NLA_TRACKS"` + `export_force_sampling`.

import math
import os
import random

import bpy
import mathutils
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260719)

WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
METAL = (0.72, 0.70, 0.66)
DANDELION_STEM = (0.28, 0.46, 0.18)
DANDELION_PUFF = (0.92, 0.92, 0.88)
DANDELION_SEED = (0.80, 0.78, 0.70)
ICE = (0.78, 0.90, 0.94)
ICE_DARK = (0.62, 0.80, 0.86)
LOTUS_PAD = (0.20, 0.42, 0.20)
LOTUS_PETAL = (0.90, 0.62, 0.72)
LOTUS_HEART = (0.86, 0.74, 0.20)


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb, alpha=1.0):
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, alpha)
        bsdf.inputs["Roughness"].default_value = 0.85
    return m


PARTS = []


def add_part(bone, material, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    create_op(location=location, rotation=rotation)
    ob = bpy.context.active_object
    ob.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    ob.data.materials.append(material)
    vg = ob.vertex_groups.new(name=bone)
    vg.add(range(len(ob.data.vertices)), 1.0, "REPLACE")
    PARTS.append(ob)
    return ob


def cylinder(bone, material, location, scale, rotation=(0, 0, 0), vertices=10):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=vertices, radius=1.0, depth=1.0, location=location, rotation=rotation
        )
    return add_part(bone, material, op, location, scale, rotation)


def cone(bone, material, location, scale, rotation=(0, 0, 0), vertices=10, radius2=0.0):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cone_add(
            vertices=vertices, radius1=1.0, radius2=radius2, depth=1.0,
            location=location, rotation=rotation,
        )
    return add_part(bone, material, op, location, scale, rotation)


def sphere(bone, material, location, scale, subdiv=2, jitter=0.0):
    def op(location, rotation):
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=subdiv, radius=1.0, location=location)
    ob = add_part(bone, material, op, location, scale)
    if jitter > 0.0:
        for v in ob.data.vertices:
            v.co.x += rng.uniform(-jitter, jitter)
            v.co.y += rng.uniform(-jitter, jitter)
            v.co.z += rng.uniform(-jitter, jitter)
    return ob


def build_rig(name, bones):
    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = PARTS[0]
    if len(PARTS) > 1:
        bpy.ops.object.join()
    mesh = bpy.context.active_object
    mesh.name = name

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = name + "Rig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.3))
    for bname, (parent, head, tail) in bones.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    mesh.parent = arm
    mod = mesh.modifiers.new("Armature", "ARMATURE")
    mod.object = arm
    return arm


def bake_idle(arm, length, keyer):
    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    for pb in arm.pose.bones:
        pb.rotation_mode = "XYZ"

    ad = arm.animation_data_create()
    ad.action = None
    keyer(arm)
    act = ad.action
    act.name = "Idle"
    track = ad.nla_tracks.new()
    track.name = "Idle"
    track.strips.new("Idle", 1, act)
    ad.action = None
    bpy.context.scene.frame_end = length

    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)
    bpy.ops.object.mode_set(mode="OBJECT")


def key_rot(arm, bone, frame, deg_xyz):
    pb = arm.pose.bones[bone]
    pb.rotation_euler = tuple(math.radians(v) for v in deg_xyz)
    pb.keyframe_insert("rotation_euler", frame=frame)


def key_loc(arm, bone, frame, xyz):
    pb = arm.pose.bones[bone]
    pb.location = xyz
    pb.keyframe_insert("location", frame=frame)


def key_scale(arm, bone, frame, xyz):
    pb = arm.pose.bones[bone]
    pb.scale = xyz
    pb.keyframe_insert("scale", frame=frame)


def export_and_preview(filename):
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=OUT_DIR + filename,
        export_format="GLB",
        export_skins=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_force_sampling=True,
        export_yup=True,
    )
    print(f"[nature3-anim] exporté {filename}")

    mesh = next(o for o in bpy.context.scene.objects if o.type == "MESH")
    bpy.context.view_layer.update()
    pts = [(mesh.matrix_world @ v.co) for v in mesh.data.vertices]
    min_z = min(p.z for p in pts)
    max_z = max(p.z for p in pts)
    span = max(
        max(p.x for p in pts) - min(p.x for p in pts),
        max(p.y for p in pts) - min(p.y for p in pts),
        max_z - min_z,
    )
    target = (0.0, 0.0, (min_z + max_z) / 2)
    dist = span * 1.9 + 1.0
    cam_loc = (dist * 0.72, -dist * 0.72, target[2] + dist * 0.45)
    bpy.ops.object.camera_add(location=cam_loc)
    cam = bpy.context.active_object
    direction = mathutils.Vector(target) - mathutils.Vector(cam_loc)
    cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    scene = bpy.context.scene
    scene.camera = cam
    bpy.ops.object.light_add(type="SUN", location=(4, -3, 8))
    bpy.context.active_object.data.energy = 3.0
    bpy.context.active_object.rotation_euler = (math.radians(35), 0, math.radians(40))
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = OUT_DIR + filename.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print(f"[nature3-anim] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Assets
# ---------------------------------------------------------------------------


def gen_windchime_sway():
    """Carillon à vent ~1,1 m : poutre suspendue + 5 tubes qui se balancent
    en pendule, léger déphasage — décor de véranda/jardin."""
    wood = mat("bois_carillon", WOOD)
    metal = mat("metal_carillon", METAL)
    cord = mat("corde_carillon", WOOD_DARK)
    cylinder("Root", cord, (0, 0, 1.05), (0.012, 0.012, 0.10), vertices=5)
    cylinder("Root", wood, (0, 0, 0.95), (0.28, 0.05, 0.05), vertices=8)
    for i in range(5):
        x = -0.24 + i * 0.12
        cylinder(f"Tube{i}", cord, (x, 0, 0.90), (0.006, 0.006, 0.06), vertices=4)
        cylinder(f"Tube{i}", metal, (x, 0, 0.78 - i * 0.02),
                  (0.018, 0.018, 0.22 + i * 0.02), vertices=8)

    bones = {f"Tube{k}": ("Root", (-0.24 + k * 0.12, 0, 0.92), (-0.24 + k * 0.12, 0, 0.60))
             for k in range(5)}
    arm = build_rig("WindchimeSway", bones)

    def keys(arm):
        # Pendule léger, déphasé tube par tube. Boucle 1 = 73.
        wave = ((1, 0.0), (19, 1.0), (37, 0.0), (55, -1.0), (73, 0.0))
        lags = [0, 10, 20, 30, 40]
        for k in range(5):
            for f, s in wave:
                ff = ((f - 1 + lags[k]) % 72) + 1
                key_rot(arm, f"Tube{k}", ff, (0, 0, s * 6.0))

    bake_idle(arm, 73, keys)
    export_and_preview("nature_windchime_sway.glb")


def gen_dandelion_sway():
    """Pissenlit ~0,4 m : tige + boule de graines qui frémit et perd des
    aigrettes qui dérivent au vent (petites sphères qui s'écartent en
    boucle) — touche vivante de prairie."""
    stem_m = mat("tige_pissenlit", DANDELION_STEM)
    puff_m = mat("aigrette_pissenlit", DANDELION_PUFF)
    seed_m = mat("graine_pissenlit", DANDELION_SEED)
    # Tige assez épaisse pour rester visible/connectée au rendu (un fil trop
    # fin ne « touche » jamais franchement une icosphère à facettes basses,
    # même quand le calcul de recouvrement est correct sur le papier —
    # piège constaté en session sur ce même asset).
    cylinder("Root", stem_m, (0, 0, 0.24), (0.018, 0.018, 0.48), vertices=6)
    sphere("Puff", puff_m, (0, 0, 0.34), (0.12, 0.12, 0.12), subdiv=1, jitter=0.01)
    seeds = []
    for i in range(6):
        a = i * math.tau / 6
        seeds.append((0.10 * math.cos(a), 0.10 * math.sin(a), 0.38 + rng.uniform(-0.03, 0.03)))
        sphere(f"Seed{i}", seed_m, seeds[-1], (0.018, 0.018, 0.018), subdiv=0)

    bones = {"Puff": ("Root", (0, 0, 0.34), (0, 0, 0.42))}
    for k in range(6):
        bones[f"Seed{k}"] = ("Puff", (0, 0, 0.38), seeds[k])
    arm = build_rig("DandelionSway", bones)

    def keys(arm):
        # La boule frémit doucement, les graines s'écartent puis reviennent
        # (dérive au vent, sans jamais vraiment se détacher). Boucle 1 = 65.
        wave = ((1, 0.0), (17, 1.0), (33, 0.3), (49, -0.6), (65, 0.0))
        for f, s in wave:
            key_rot(arm, "Puff", f, (s * 3.0, s * 2.0, 0))
        for k in range(6):
            lag = k * 6
            for f, s in wave:
                ff = ((f - 1 + lag) % 64) + 1
                out = 1.0 + max(s, 0.0) * 0.35
                key_scale(arm, f"Seed{k}", ff, (out, out, out))

    bake_idle(arm, 65, keys)
    export_and_preview("nature_dandelion_sway.glb")


def gen_icicle_drip():
    """Glaçon suspendu ~0,5 m : la pointe s'étire lentement puis une goutte
    tombe et disparaît (échelle à zéro), boucle qui reprend — décor de
    grotte/hiver, cf. `organic_common.py` pour le style spire déjà en place."""
    ice_m = mat("glace", ICE)
    ice_d_m = mat("glace_sombre", ICE_DARK)
    cylinder("Root", ice_d_m, (0, 0, 0.44), (0.09, 0.09, 0.12), vertices=8)
    # Cône retourné (base large en haut, pointe en bas) : le primitive cone de
    # Blender met par défaut sa base au niveau -Z et sa pointe en +Z — sans
    # ce retournement, la pointe se retrouve en haut contre le cylindre de
    # montage (rayon quasi nul) et laisse un trou visible au rendu.
    cone("Root", ice_m, (0, 0, 0.32), (0.09, 0.09, 0.30), vertices=8,
         rotation=(math.pi, 0, 0))
    sphere("Drop", ice_m, (0, 0, 0.10), (0.035, 0.035, 0.05), subdiv=1)

    bones = {"Drop": ("Root", (0, 0, 0.14), (0, 0, 0.02))}
    arm = build_rig("IcicleDrip", bones)

    def keys(arm):
        # La goutte s'étire (échelle Z) vers le bas, descend, puis
        # disparaît d'un coup à l'échelle 0 avant de réapparaître en haut
        # (échelle 0 → 1 quasi instantanée = pas de « saut » visible d'une
        # goutte qui remonterait). Boucle 1 = 49.
        for f, sz, dz in ((1, 0.4, 0.0), (18, 1.4, -0.03), (30, 0.5, -0.09),
                          (31, 0.0, -0.09), (32, 0.4, 0.0), (49, 0.4, 0.0)):
            key_scale(arm, "Drop", f, (1.0, 1.0, sz))
            key_loc(arm, "Drop", f, (0, 0, dz))

    bake_idle(arm, 49, keys)
    export_and_preview("nature_icicle_drip.glb")


def gen_lotus_bob():
    """Lotus flottant ~0,5 m : feuille ronde + fleur qui bercent doucement
    sur l'eau, tangage et roulis légers — variante vivante du nature_lily
    statique, pour l'étang/le lac."""
    pad_m = mat("feuille_lotus", LOTUS_PAD)
    petal_m = mat("petale_lotus", LOTUS_PETAL)
    heart_m = mat("coeur_lotus", LOTUS_HEART)
    cylinder("Root", pad_m, (0, 0, 0.015), (0.24, 0.24, 0.03), vertices=14)
    for i in range(8):
        a = i * math.tau / 8
        # Pétales élargis (0.05→0.07) et plus ronds (5→7 côtés) : au rayon
        # d'implantation 0.09, des cônes trop étroits/facettés laissent des
        # coins de pétale à coin visibles entre eux plutôt qu'une fleur
        # continue.
        cone(f"Petal{i}", petal_m, (0.09 * math.cos(a), 0.09 * math.sin(a), 0.03),
             (0.07, 0.07, 0.14), vertices=7, rotation=(math.radians(60), 0, a))
    sphere("Heart", heart_m, (0, 0, 0.09), (0.045, 0.045, 0.03), subdiv=1)

    bones = {}
    for k in range(8):
        bones[f"Petal{k}"] = ("Root", (0, 0, 0.02), (0, 0, 0.16))
    bones["Heart"] = ("Root", (0, 0, 0.02), (0, 0, 0.10))
    arm = build_rig("LotusBob", bones)

    def keys(arm):
        # Tangage/roulis lents façon clapot, tous les pétales suivent le
        # même mouvement rigide (pas de déphasage : c'est la fleur entière
        # qui berce, pas chaque pétale indépendamment). Boucle 1 = 81.
        wave = ((1, 0.0), (21, 1.0), (41, 0.0), (61, -1.0), (81, 0.0))
        for bone in ["Heart"] + [f"Petal{k}" for k in range(8)]:
            for f, s in wave:
                key_rot(arm, bone, f, (s * 2.5, s * 1.8, 0))

    bake_idle(arm, 81, keys)
    export_and_preview("nature_lotus_bob.glb")


ASSETS = [gen_windchime_sway, gen_dandelion_sway, gen_icicle_drip, gen_lotus_bob]

for gen in ASSETS:
    reset_scene()
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[nature3-anim] pack animé complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
