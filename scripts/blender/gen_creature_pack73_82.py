"""Génère assets/models/creature73.glb … creature82.glb : 10 animaux, pack 1/3.

Kangourou, toucan, iguane, écureuil, otarie, flamant rose, perroquet, lynx,
sanglier, âne. Trois clips par créature (Idle, Walk, + une action signature :
bond, cri, charge…), technique `creature_kit.py` (primitives, un os/pièce,
LOD auto, aucun vertex sous z=0). QA par `check_creatures.py` après génération.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack73_82.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from creature_kit import (  # noqa: E402
    LEGS4, OUT_DIR, PARTS, cone, cylinder, fresh_scene, material,
    quad_bones, quad_walk_keys, sphere,
)

import math as _math
import bpy as _bpy
from mathutils import Vector as _Vector


def build_creature3(name, bones, idle_keys, walk_keys, third_keys, third_name,
                     cam=1.0):
    """Comme `creature_kit.build_creature`, mais bake un 3e clip nommé
    `third_name` en plus d'Idle/Walk (ce pack donne 3 animations par
    créature) — dupliqué ici plutôt que de toucher `creature_kit.py`,
    partagé par une quinzaine d'autres packs déjà en production."""
    _bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    _bpy.context.view_layer.objects.active = PARTS[0]
    _bpy.ops.object.join()
    creature = _bpy.context.active_object
    creature.name = name.capitalize()

    _bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = _bpy.context.active_object
    arm.name = f"{creature.name}Rig"
    _bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = _Vector((0, 0, 0)), _Vector((0, 0, 0.35))
    for bname, (parent, head, tail) in bones.items():
        b = eb.new(bname)
        b.head, b.tail = _Vector(head), _Vector(tail)
        b.parent = eb[parent]
    _bpy.ops.object.mode_set(mode="OBJECT")

    creature.parent = arm
    creature.modifiers.new("Armature", "ARMATURE").object = arm

    _bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    _bpy.context.view_layer.objects.active = arm
    _bpy.ops.object.mode_set(mode="POSE")
    for pb in arm.pose.bones:
        pb.rotation_mode = "XYZ"

    def key_rot(bone, frame, xyz):
        pb = arm.pose.bones[bone]
        pb.rotation_euler = xyz
        pb.keyframe_insert("rotation_euler", frame=frame)

    def key_loc(bone, frame, xyz):
        pb = arm.pose.bones[bone]
        pb.location = xyz
        pb.keyframe_insert("location", frame=frame)

    def bake_clip(clip, keyer):
        ad = arm.animation_data_create()
        ad.action = None
        keyer(key_rot, key_loc)
        act = ad.action
        act.name = clip
        track = ad.nla_tracks.new()
        track.name = clip
        track.strips.new(clip, 1, act)
        ad.action = None

    bake_clip("Idle", idle_keys)
    bake_clip("Walk", walk_keys)
    bake_clip(third_name, third_keys)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    _bpy.ops.object.mode_set(mode="OBJECT")

    out = os.path.join(OUT_DIR, f"{name}.glb")
    _bpy.ops.object.select_all(action="SELECT")
    _bpy.ops.export_scene.gltf(
        filepath=out,
        export_format="GLB",
        export_skins=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_force_sampling=True,
        export_yup=True,
    )
    print("EXPORTED", out)

    ad = arm.animation_data
    ad.action = None
    for t in list(ad.nla_tracks):
        ad.nla_tracks.remove(t)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    scene = _bpy.context.scene
    scene.frame_set(1)
    _bpy.context.view_layer.update()
    _bpy.ops.object.camera_add(
        location=(5.2 * cam, -7.0 * cam, 3.6 * cam),
        rotation=(_math.radians(74), 0, _math.radians(37)),
    )
    scene.camera = _bpy.context.active_object
    _bpy.ops.object.light_add(
        type="SUN", location=(2, -3, 6),
        rotation=(_math.radians(35), _math.radians(20), 0),
    )
    _bpy.context.active_object.data.energy = 3.0
    _bpy.ops.object.light_add(
        type="SUN", location=(-3, 2, 4),
        rotation=(_math.radians(55), _math.radians(-30), 0),
    )
    _bpy.context.active_object.data.energy = 1.6
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = out.replace(".glb", "_preview.png")
    _bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)


# =============================================================================
# Créature 73 — Kangourou : bond puissant, appui sur la queue.
# =============================================================================
def kangourou():
    fresh_scene()
    fur = material("Kangourou73Fur", (0.62, 0.46, 0.30))
    cream = material("Kangourou73Cream", (0.82, 0.72, 0.56))
    dark = material("Kangourou73Dark", (0.08, 0.07, 0.07))

    sphere("Body", fur, (0, 0.15, 0.90), (0.30, 0.42, 0.52))
    sphere("Body", cream, (0, -0.10, 0.72), (0.22, 0.26, 0.30))  # poitrail
    sphere("Head", fur, (0, -0.55, 1.35), (0.18, 0.22, 0.18))
    sphere("Head", fur, (0, -0.42, 1.24), (0.20, 0.26, 0.22))  # cou
    sphere("Head", fur, (0, -0.20, 1.06), (0.22, 0.28, 0.24))  # cou, referme la selle tête-corps
    sphere("Head", cream, (0, -0.74, 1.28), (0.10, 0.12, 0.08))  # museau
    sphere("Head", dark, (0, -0.84, 1.25), (0.03, 0.03, 0.03))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.60, 1.42), (0.03, 0.025, 0.03))
        sphere("Head", fur, (sx * 0.14, -0.44, 1.56), (0.06, 0.04, 0.12))  # oreille longue
    # Petites pattes avant.
    for sx in (-1, 1):
        cylinder("ArmL" if sx < 0 else "ArmR", fur, (sx * 0.16, -0.30, 0.68),
                  (0.045, 0.045, 0.20))
    # Grandes pattes arrière massives.
    for bone, x in (("LegL", -0.16), ("LegR", 0.16)):
        sphere(bone, fur, (x, 0.35, 0.35), (0.14, 0.22, 0.18))
        cylinder(bone, dark, (x, 0.55, 0.10), (0.07, 0.24, 0.06))  # long pied
    # Queue épaisse, appui au sol.
    for y, z, r in ((0.55, 0.62, 0.15), (0.90, 0.42, 0.12), (1.20, 0.20, 0.08),
                    (1.40, 0.06, 0.05)):
        sphere("Tail", fur, (0, y, z), (r, r * 1.3, r))

    bones = {
        "Body": ("Root", (0, 0.35, 0.85), (0, -0.35, 1.00)),
        "Head": ("Body", (0, -0.40, 1.15), (0, -0.75, 1.40)),
        "ArmL": ("Body", (-0.16, -0.20, 0.75), (-0.16, -0.35, 0.60)),
        "ArmR": ("Body", (0.16, -0.20, 0.75), (0.16, -0.35, 0.60)),
        "LegL": ("Body", (-0.16, 0.35, 0.45), (-0.16, 0.58, 0.06)),
        "LegR": ("Body", (0.16, 0.35, 0.45), (0.16, 0.58, 0.06)),
        "Tail": ("Body", (0, 0.42, 0.68), (0, 1.42, 0.05)),
    }

    def idle(key_rot, key_loc):
        # Reste dressé, appuyé sur la queue, oreilles en radar.
        for f in (1, 40):
            for b in ("LegL", "LegR", "ArmL", "ArmR"):
                key_rot(b, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (10, 0.3), (20, -0.3), (30, 0.0), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        # Petits bonds sur les deux pattes arrière ensemble.
        for f, dz in ((1, -0.04), (6, 0.14), (12, -0.04), (18, 0.14), (24, -0.04)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.30), (6, -0.35), (12, 0.10), (18, -0.35), (24, 0.30)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (a, 0, 0))
        for f, sw in ((1, 0.2), (12, -0.2), (24, 0.2)):
            key_rot("Tail", f, (0, 0, sw))

    def bond(key_rot, key_loc):
        # Bond puissant : anticipation accroupie, envol, réception, 24 fr.
        for f, dz in ((1, -0.06), (6, 0.05), (12, 0.32), (18, 0.05), (24, -0.06)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, -18), (6, 40), (12, 55), (18, 10), (24, -18)):
            key_rot("LegL", f, (math.radians(a), 0, 0))
            key_rot("LegR", f, (math.radians(a), 0, 0))
        for f, a in ((1, -10), (12, 25), (24, -10)):
            key_rot("ArmL", f, (math.radians(a), 0, 0))
            key_rot("ArmR", f, (math.radians(a), 0, 0))
        for f, sw in ((1, -0.3), (12, 0.4), (24, -0.3)):
            key_rot("Tail", f, (sw, 0, 0))

    build_creature3("creature73", bones, idle, walk, bond, "Bond", cam=1.0)


# =============================================================================
# Créature 74 — Toucan : grand bec coloré, hoche la tête.
# =============================================================================
def toucan():
    fresh_scene()
    black = material("Toucan74Black", (0.08, 0.08, 0.09))
    white = material("Toucan74White", (0.94, 0.93, 0.88))
    beak_o = material("Toucan74BeakO", (0.92, 0.55, 0.10))
    beak_y = material("Toucan74BeakY", (0.95, 0.85, 0.20))

    sphere("Body", black, (0, 0.05, 0.60), (0.22, 0.28, 0.30))
    sphere("Body", white, (0, -0.14, 0.56), (0.14, 0.14, 0.20))  # poitrail
    sphere("Head", black, (0, -0.20, 0.84), (0.15, 0.16, 0.15))
    # Grand bec en 2 tons, s'effilant.
    cone("Head", beak_o, (0, -0.42, 0.86), (0.09, 0.09, 0.32),
         rotation=(math.radians(95), 0, 0))
    cone("Head", beak_y, (0, -0.36, 0.82), (0.06, 0.06, 0.20),
         rotation=(math.radians(95), 0, 0))
    for sx in (-1, 1):
        sphere("Head", white, (sx * 0.09, -0.14, 0.90), (0.035, 0.03, 0.035))
        sphere("Head", black, (sx * 0.10, -0.16, 0.89), (0.018, 0.015, 0.018))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, black, (sx * 0.28, 0.05, 0.62), (0.08, 0.18, 0.24))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, black, (sx * 0.09, 0.02, 0.22), (0.03, 0.03, 0.16))
        sphere(bone, beak_o, (sx * 0.09, -0.06, 0.06), (0.06, 0.09, 0.025))

    bones = {
        "Body": ("Root", (0, 0.20, 0.60), (0, -0.10, 0.66)),
        "Head": ("Body", (0, 0.0, 0.80), (0, -0.30, 0.86)),
        "WingL": ("Body", (-0.20, 0.02, 0.72), (-0.36, 0.05, 0.42)),
        "WingR": ("Body", (0.20, 0.02, 0.72), (0.36, 0.05, 0.42)),
        "LegL": ("Body", (-0.09, 0.02, 0.30), (-0.09, 0.02, 0.02)),
        "LegR": ("Body", (0.09, 0.02, 0.30), (0.09, 0.02, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for b in ("LegL", "LegR", "WingL", "WingR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, nod in ((1, 0.0), (10, 0.35), (18, 0.0), (26, 0.35), (34, 0.0),
                       (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        s = math.radians(18)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.15), (13, 0.22), (24, 0.15)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))

    def cri(key_rot, key_loc):
        # Cri : tête rejetée en arrière, bec pointé au ciel, ailes ouvertes.
        for f, up in ((1, 0.0), (8, -0.9), (16, -0.9), (22, 0.0), (28, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, a in ((1, 0.0), (8, 0.6), (16, 0.6), (22, 0.0), (28, 0.0)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f in (1, 28):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))

    build_creature3("creature74", bones, idle, walk, cri, "Cri", cam=0.65)


# =============================================================================
# Créature 75 — Iguane : crête épineuse, bascule la tête au soleil.
# =============================================================================
def iguane():
    fresh_scene()
    green = material("Iguane75Green", (0.28, 0.48, 0.22))
    green_d = material("Iguane75GreenD", (0.18, 0.34, 0.15))
    dark = material("Iguane75Dark", (0.08, 0.08, 0.07))

    sphere("Body", green, (0, 0.10, 0.30), (0.18, 0.55, 0.16))
    sphere("Head", green, (0, -0.55, 0.34), (0.13, 0.18, 0.12))
    sphere("Head", green_d, (0, -0.74, 0.30), (0.07, 0.10, 0.06))  # museau
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.08, -0.60, 0.42), (0.025, 0.022, 0.025))
    # Crête d'épines le long du dos.
    for k, y in enumerate((-0.45, -0.25, -0.05, 0.15, 0.35, 0.55)):
        cone("Body", green_d, (0, y, 0.44 - 0.02 * k), (0.03, 0.05, 0.09),
             rotation=(math.radians(-90), 0, 0))
    # Fanon sous le menton.
    sphere("Head", green_d, (0, -0.55, 0.16), (0.05, 0.10, 0.10))
    for bone, x, y in (("LegFL", -0.16, -0.20), ("LegFR", 0.16, -0.20),
                       ("LegBL", -0.16, 0.30), ("LegBR", 0.16, 0.30)):
        cylinder(bone, green, (x, y, 0.14), (0.04, 0.04, 0.14))
        for k in range(4):
            sphere(bone, dark, (x + (k - 1.5) * 0.025, y + 0.10, 0.02),
                   (0.012, 0.012, 0.01))  # griffes
    # Longue queue effilée.
    for k in range(6):
        t = k / 5.0
        sphere("Tail", green, (0, 0.65 + 0.30 * t, 0.30 - 0.10 * t),
               (0.09 - 0.06 * t, 0.16, 0.09 - 0.06 * t))

    bones = quad_bones(0.16, -0.20, 0.30, 0.20, ((0, 0.35, 0.30), (0, -0.35, 0.32)), {
        "Head": ("Body", (0, -0.40, 0.32), (0, -0.75, 0.34)),
        "Tail": ("Body", (0, 0.55, 0.30), (0, 1.55, 0.10)),
    })

    def idle(key_rot, key_loc):
        # Bascule au soleil : la tête se tourne pour capter la lumière.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, yaw in ((1, 0.0), (14, 0.5), (26, -0.3), (40, 0.0)):
            key_rot("Head", f, (0.15, 0, yaw))
        for f, sw in ((1, 0.15), (20, -0.15), (40, 0.15)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.3), (13, -0.3), (24, 0.3)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 26, extras)

    def bascule(key_rot, key_loc):
        # Pompes territoriales : le corps s'élève et retombe sur les pattes.
        for f, dz in ((1, 0.0), (6, 0.12), (12, 0.0), (18, 0.12), (24, 0.0)):
            key_loc("Body", f, (0, 0, dz))
        for f, up in ((1, 0.0), (6, -0.3), (12, 0.0), (18, -0.3), (24, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for leg in LEGS4:
            for f, a in ((1, 0), (6, -20), (12, 0), (18, -20), (24, 0)):
                key_rot(leg, f, (math.radians(a), 0, 0))

    build_creature3("creature75", bones, idle, walk, bascule, "Bascule", cam=0.6)


# =============================================================================
# Créature 76 — Écureuil : queue en panache, grignote assis.
# =============================================================================
def ecureuil():
    fresh_scene()
    fur = material("Ecureuil76Fur", (0.62, 0.36, 0.16))
    cream = material("Ecureuil76Cream", (0.90, 0.84, 0.72))
    dark = material("Ecureuil76Dark", (0.08, 0.07, 0.06))

    sphere("Body", fur, (0, 0.05, 0.30), (0.14, 0.20, 0.16))
    sphere("Head", fur, (0, -0.20, 0.38), (0.11, 0.12, 0.10))
    sphere("Head", cream, (0, -0.30, 0.33), (0.06, 0.06, 0.05))  # museau
    sphere("Head", dark, (0, -0.36, 0.32), (0.02, 0.018, 0.018))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.06, -0.24, 0.44), (0.025, 0.02, 0.025))
        cone("Head", fur, (sx * 0.08, -0.14, 0.50), (0.045, 0.045, 0.09),
             rotation=(math.radians(-10), 0, math.radians(sx * 10)))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        sphere(bone, fur, (sx * 0.10, 0.10, 0.18), (0.06, 0.10, 0.10))
        cylinder(bone, fur, (sx * 0.07, -0.12, 0.16), (0.03, 0.03, 0.10))
    # Grande queue en panache, courbée par-dessus le dos.
    for k in range(6):
        t = k / 5.0
        sphere("Tail", fur, (0, 0.25 + 0.10 * math.sin(t * 2.4),
                             0.30 + 0.35 * t), (0.11 - 0.02 * t, 0.12, 0.11 - 0.02 * t))

    bones = {
        "Body": ("Root", (0, 0.15, 0.24), (0, -0.10, 0.30)),
        "Head": ("Body", (0, -0.14, 0.32), (0, -0.36, 0.38)),
        "LegL": ("Body", (-0.10, 0.05, 0.18), (-0.10, 0.05, 0.02)),
        "LegR": ("Body", (0.10, 0.05, 0.18), (0.10, 0.05, 0.02)),
        "Tail": ("Body", (0, 0.22, 0.28), (0, 0.55, 0.62)),
    }

    def idle(key_rot, key_loc):
        # Assis, grignote une noix des deux pattes avant.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, nod in ((1, 0.0), (6, 0.25), (11, 0.0), (16, 0.25), (21, 0.0),
                       (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.1), (20, -0.1), (40, 0.1)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Petits bonds, la queue rebondit.
        for f, dz in ((1, -0.02), (6, 0.08), (12, -0.02), (18, 0.08), (24, -0.02)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.3), (6, -0.4), (12, 0.1), (18, -0.4), (24, 0.3)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (a, 0, 0))
        for f, sw in ((1, 0.1), (12, -0.1), (24, 0.1)):
            key_rot("Tail", f, (sw, 0, 0))

    def grignote(key_rot, key_loc):
        # Grignotage rapide, tête qui vibre.
        for f, nod in ((1, 0.0), (4, 0.15), (8, 0.0), (12, 0.15), (16, 0.0),
                       (20, 0.15), (24, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f in (1, 24):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (math.radians(-20), 0, 0))

    build_creature3("creature76", bones, idle, walk, grignote, "Grignote", cam=0.5)


# =============================================================================
# Créature 77 — Otarie : aboie, se dandine sur ses nageoires.
# =============================================================================
def otarie():
    fresh_scene()
    grey = material("Otarie77Grey", (0.42, 0.40, 0.38))
    grey_d = material("Otarie77GreyD", (0.30, 0.28, 0.27))
    dark = material("Otarie77Dark", (0.06, 0.06, 0.06))

    sphere("Body", grey, (0, 0.05, 0.42), (0.30, 0.62, 0.32))
    sphere("Tail", grey, (0, 0.65, 0.34), (0.16, 0.20, 0.14))
    for sx in (-1, 1):
        sphere("Tail", grey_d, (sx * 0.14, 0.85, 0.30), (0.09, 0.14, 0.045))
        sphere(f"Flip{'L' if sx < 0 else 'R'}", grey_d,
               (sx * 0.34, -0.15, 0.20), (0.11, 0.20, 0.05))
    sphere("Head", grey, (0, -0.55, 0.56), (0.19, 0.20, 0.17))
    sphere("Head", grey_d, (0, -0.74, 0.48), (0.10, 0.09, 0.08))  # museau
    sphere("Head", dark, (0, -0.85, 0.48), (0.03, 0.026, 0.026))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -0.62, 0.62), (0.032, 0.028, 0.032))

    bones = {
        "Body": ("Root", (0, 0.30, 0.42), (0, -0.30, 0.50)),
        "Head": ("Body", (0, -0.35, 0.52), (0, -0.85, 0.50)),
        "Tail": ("Body", (0, 0.55, 0.36), (0, 0.95, 0.28)),
        "FlipL": ("Body", (-0.28, -0.15, 0.28), (-0.48, -0.15, 0.05)),
        "FlipR": ("Body", (0.28, -0.15, 0.28), (0.48, -0.15, 0.05)),
    }

    def idle(key_rot, key_loc):
        # Aboie : la tête part en arrière et rebondit, gueule ouverte.
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))
        for f, up in ((1, 0.0), (8, -0.4), (12, -0.15), (16, -0.4), (20, 0.0),
                      (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, a in ((1, 0.0), (10, 0.2), (24, 0.0), (40, 0.0)):
            key_rot("FlipL", f, (0, 0, a))
            key_rot("FlipR", f, (0, 0, -a))

    def walk(key_rot, key_loc):
        # Dandine sur les nageoires arrière, comme un phoque hors de l'eau.
        for f, a in ((1, 0.4), (9, -0.2), (17, 0.1), (24, 0.4)):
            key_rot("FlipL", f, (a, 0, 0))
            key_rot("FlipR", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (9, 0.08), (17, 0.0), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.2), (13, -0.2), (24, 0.2)):
            key_rot("Tail", f, (0, 0, sw))

    def aboie(key_rot, key_loc):
        # Applaudit des nageoires, museau au ciel.
        for f, up in ((1, 0.0), (8, -0.5), (20, -0.5), (28, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, a in ((1, 0.0), (8, -0.6), (14, -1.0), (20, -0.6), (26, 0.0), (28, 0.0)):
            key_rot("FlipL", f, (0, 0, a))
            key_rot("FlipR", f, (0, 0, -a))

    build_creature3("creature77", bones, idle, walk, aboie, "Aboie", cam=0.8)


# =============================================================================
# Créature 78 — Flamant rose : sur une patte, cou en S.
# =============================================================================
def flamant():
    fresh_scene()
    pink = material("Flamant78Pink", (0.92, 0.48, 0.58))
    pink_d = material("Flamant78PinkD", (0.80, 0.32, 0.42))
    dark = material("Flamant78Dark", (0.08, 0.07, 0.07))

    sphere("Body", pink, (0, 0.05, 1.10), (0.18, 0.24, 0.20))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, pink_d, (sx * 0.18, 0.05, 1.08), (0.06, 0.16, 0.16))
    # Cou en S, chaîne de segments qui se chevauchent.
    for k in range(6):
        t = k / 5.0
        cy = -0.15 - 0.10 * math.sin(t * 3.0)
        cz = 1.20 + 0.42 * t
        sphere("Neck", pink, (0, cy, cz), (0.065, 0.065, 0.10))
    sphere("Head", pink, (0, -0.30, 1.62), (0.09, 0.10, 0.08))
    cone("Head", dark, (0, -0.44, 1.56), (0.03, 0.03, 0.10),
         rotation=(math.radians(115), 0, 0))  # bec crochu
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.05, -0.32, 1.68), (0.018, 0.015, 0.018))
    # Une seule patte fine, longue, repliée sous le corps.
    cylinder("LegL", dark, (0, 0.05, 0.55), (0.028, 0.028, 0.55))
    sphere("LegL", dark, (0, 0.10, 0.04), (0.05, 0.08, 0.02))

    bones = {
        "Body": ("Root", (0, 0.20, 1.05), (0, -0.10, 1.15)),
        "WingL": ("Body", (-0.15, 0.05, 1.15), (-0.30, 0.05, 0.95)),
        "WingR": ("Body", (0.15, 0.05, 1.15), (0.30, 0.05, 0.95)),
        "Neck": ("Body", (0, -0.10, 1.18), (0, -0.30, 1.55)),
        "Head": ("Neck", (0, -0.30, 1.55), (0, -0.45, 1.68)),
        "LegL": ("Body", (0, 0.05, 1.05), (0, 0.05, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Équilibre sur une patte : le corps oscille légèrement, le cou
        # ondule pour compenser.
        for f in (1, 40):
            key_rot("LegL", f, (0, 0, 0))
        for f, roll in ((1, 0.0), (14, 0.06), (28, -0.06), (40, 0.0)):
            key_rot("Body", f, (0, roll, 0))
        for f, sw in ((1, 0.0), (14, 0.15), (28, -0.15), (40, 0.0)):
            key_rot("Neck", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Marche précautionneuse sur une seule patte visible (l'autre est
        # symboliquement repliée) : petit sautillement.
        for f, a in ((1, 0.2), (13, -0.2), (24, 0.2)):
            key_rot("LegL", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.1), (13, -0.1), (24, 0.1)):
            key_rot("Neck", f, (nod, 0, 0))

    def equilibre(key_rot, key_loc):
        # Lisse ses plumes : le cou plonge vers le corps puis remonte.
        for f, curl in ((1, 0.0), (10, 1.1), (20, 1.1), (30, 0.0), (40, 0.0)):
            key_rot("Neck", f, (curl, 0, 0))
        for f, curl in ((1, 0.0), (10, 0.6), (20, 0.6), (30, 0.0), (40, 0.0)):
            key_rot("Head", f, (curl, 0, 0))

    build_creature3("creature78", bones, idle, walk, equilibre, "Toilette", cam=1.3)


# =============================================================================
# Créature 79 — Perroquet : plumage vif, tête inclinée qui « parle ».
# =============================================================================
def perroquet():
    fresh_scene()
    green = material("Perroquet79Green", (0.20, 0.62, 0.24))
    red = material("Perroquet79Red", (0.78, 0.16, 0.14))
    blue = material("Perroquet79Blue", (0.16, 0.32, 0.72))
    dark = material("Perroquet79Dark", (0.08, 0.07, 0.06))

    sphere("Body", green, (0, 0.05, 0.42), (0.16, 0.22, 0.22))
    sphere("Body", red, (0, -0.14, 0.40), (0.08, 0.10, 0.14))  # poitrail rouge
    sphere("Head", green, (0, -0.16, 0.62), (0.11, 0.11, 0.11))
    sphere("Head", blue, (0, -0.20, 0.72), (0.07, 0.05, 0.06))  # calotte bleue
    cone("Head", dark, (0, -0.27, 0.58), (0.045, 0.045, 0.10),
         rotation=(math.radians(115), 0, 0))  # bec crochu
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.07, -0.10, 0.66), (0.024, 0.02, 0.024))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, blue, (sx * 0.20, 0.05, 0.42), (0.06, 0.15, 0.18))
    # Longue queue à plumes rouges/bleues.
    for k in range(4):
        t = k / 3.0
        sphere("Tail", red if k % 2 == 0 else blue, (0, 0.30 + 0.18 * t, 0.36 - 0.10 * t),
               (0.05, 0.10, 0.04))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, dark, (sx * 0.06, 0.02, 0.16), (0.02, 0.02, 0.12))
        sphere(bone, dark, (sx * 0.06, -0.04, 0.04), (0.045, 0.06, 0.02))

    bones = {
        "Body": ("Root", (0, 0.18, 0.42), (0, -0.10, 0.48)),
        "Head": ("Body", (0, 0.0, 0.58), (0, -0.20, 0.72)),
        "WingL": ("Body", (-0.16, 0.05, 0.50), (-0.28, 0.05, 0.28)),
        "WingR": ("Body", (0.16, 0.05, 0.50), (0.28, 0.05, 0.28)),
        "Tail": ("Body", (0, 0.24, 0.42), (0, 0.62, 0.20)),
        "LegL": ("Body", (-0.06, 0.02, 0.22), (-0.06, 0.02, 0.02)),
        "LegR": ("Body", (0.06, 0.02, 0.22), (0.06, 0.02, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for b in ("LegL", "LegR", "WingL", "WingR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, tilt in ((1, 0.0), (10, 0.4), (20, -0.3), (30, 0.0), (40, 0.0)):
            key_rot("Head", f, (0, 0, tilt))

    def walk(key_rot, key_loc):
        s = math.radians(16)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.025), (13, 0.0), (19, 0.025), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, roll in ((1, 0.1), (13, -0.1), (24, 0.1)):
            key_rot("Body", f, (0, roll, 0))

    def parle(key_rot, key_loc):
        # « Parle » : la tête dodeline vite d'un côté à l'autre, ailes
        # entrouvertes à chaque « syllabe ».
        for f, tilt in ((1, 0.0), (4, 0.5), (8, -0.5), (12, 0.5), (16, -0.5),
                        (20, 0.0)):
            key_rot("Head", f, (0, 0, tilt))
        for f, a in ((1, 0.0), (8, 0.3), (16, 0.3), (20, 0.0)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))

    build_creature3("creature79", bones, idle, walk, parle, "Parle", cam=0.5)


# =============================================================================
# Créature 80 — Lynx : oreilles à pinceaux, bondit à l'affût.
# =============================================================================
def lynx():
    fresh_scene()
    fur = material("Lynx80Fur", (0.72, 0.62, 0.48))
    spot = material("Lynx80Spot", (0.42, 0.34, 0.24))
    cream = material("Lynx80Cream", (0.90, 0.86, 0.76))
    dark = material("Lynx80Dark", (0.08, 0.07, 0.06))

    sphere("Body", fur, (0, 0.10, 0.52), (0.24, 0.44, 0.24))
    for sx, y, z in ((-0.12, -0.10, 0.68), (0.14, 0.10, 0.64), (-0.10, 0.25, 0.58),
                     (0.10, -0.20, 0.60)):
        sphere("Body", spot, (sx, y, z), (0.035, 0.035, 0.03))
    sphere("Head", fur, (0, -0.48, 0.66), (0.18, 0.19, 0.16))
    sphere("Head", fur, (0, -0.34, 0.60), (0.18, 0.18, 0.16))  # cou, referme la selle tête-corps
    sphere("Head", cream, (0, -0.64, 0.58), (0.10, 0.10, 0.08))  # collerette/museau
    sphere("Head", dark, (0, -0.72, 0.60), (0.03, 0.026, 0.026))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.56, 0.72), (0.032, 0.026, 0.036))
        cone("Head", fur, (sx * 0.14, -0.38, 0.86), (0.06, 0.04, 0.11),
             rotation=(math.radians(-8), 0, math.radians(sx * 12)))
        cylinder("Head", dark, (sx * 0.14, -0.36, 0.97), (0.006, 0.006, 0.035),
                 rotation=(math.radians(-8), 0, math.radians(sx * 12)))  # pinceau
    for bone, x, y in (("LegFL", -0.15, -0.28), ("LegFR", 0.15, -0.28),
                       ("LegBL", -0.15, 0.32), ("LegBR", 0.15, 0.32)):
        cylinder(bone, fur, (x, y, 0.24), (0.065, 0.065, 0.46))
        sphere(bone, cream, (x, y, 0.045), (0.07, 0.09, 0.035))
    sphere("Tail", fur, (0, 0.55, 0.44), (0.06, 0.06, 0.06))
    sphere("Tail", dark, (0, 0.62, 0.42), (0.045, 0.045, 0.045))

    bones = quad_bones(0.15, -0.28, 0.32, 0.46, ((0, 0.28, 0.50), (0, -0.28, 0.58)), {
        "Head": ("Body", (0, -0.36, 0.62), (0, -0.72, 0.68)),
        "Tail": ("Body", (0, 0.48, 0.44), (0, 0.62, 0.42)),
    })

    def idle(key_rot, key_loc):
        # Affût félin : corps tassé, oreilles pivotent, regard fixe.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz, up in ((1, -0.04, 0.08), (20, -0.02, 0.04), (40, -0.04, 0.08)):
            key_loc("Body", f, (0, dz, 0))
            key_rot("Body", f, (up, 0, 0))
        for f, yaw in ((1, 0.0), (12, 0.25), (24, -0.25), (40, 0.0)):
            key_rot("Head", f, (0.05, 0, yaw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.1), (13, -0.1), (24, 0.1)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 24, extras)

    def bondit(key_rot, key_loc):
        # Bondit à l'affût : anticipation basse puis détente explosive.
        for f, dz in ((1, -0.06), (8, -0.10), (14, 0.20), (20, 0.02), (26, -0.06)):
            key_loc("Body", f, (0, dz, 0))
        for leg in LEGS4:
            for f, a in ((1, 15), (8, 30), (14, -25), (20, 10), (26, 15)):
                key_rot(leg, f, (math.radians(a), 0, 0))
        for f, sw in ((1, 0.0), (14, 0.5), (26, 0.0)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature3("creature80", bones, idle, walk, bondit, "Bondit", cam=0.8)


# =============================================================================
# Créature 81 — Sanglier : défenses, charge tête baissée.
# =============================================================================
def sanglier():
    fresh_scene()
    bristle = material("Sanglier81Bristle", (0.28, 0.22, 0.16))
    bristle_d = material("Sanglier81BristleD", (0.16, 0.12, 0.09))
    tusk = material("Sanglier81Tusk", (0.90, 0.88, 0.78))
    dark = material("Sanglier81Dark", (0.06, 0.05, 0.05))

    sphere("Body", bristle, (0, 0.10, 0.55), (0.28, 0.52, 0.30))
    sphere("Body", bristle_d, (0, -0.35, 0.68), (0.22, 0.22, 0.24))  # garrot bombé
    # Crinière hérissée sur l'échine.
    for k, y in enumerate((-0.45, -0.25, -0.05, 0.15, 0.35)):
        cone("Body", bristle_d, (0, y, 0.82 - 0.03 * k), (0.035, 0.08, 0.10),
             rotation=(math.radians(-90), 0, 0))
    sphere("Head", bristle, (0, -0.68, 0.58), (0.20, 0.22, 0.18))
    sphere("Head", bristle_d, (0, -0.92, 0.48), (0.11, 0.10, 0.09))  # groin
    sphere("Head", dark, (0, -1.02, 0.46), (0.045, 0.03, 0.03))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -0.74, 0.68), (0.03, 0.025, 0.03))
        sphere("Head", bristle_d, (sx * 0.18, -0.50, 0.72), (0.06, 0.04, 0.07))  # oreille
        cone("Head", tusk, (sx * 0.11, -0.88, 0.36), (0.025, 0.025, 0.14),
             rotation=(math.radians(150), 0, math.radians(sx * 20)))  # défense
    for bone, x, y in (("LegFL", -0.20, -0.36), ("LegFR", 0.20, -0.36),
                       ("LegBL", -0.20, 0.42), ("LegBR", 0.20, 0.42)):
        cylinder(bone, bristle, (x, y, 0.28), (0.085, 0.085, 0.52))
        cylinder(bone, dark, (x, y, 0.04), (0.09, 0.09, 0.08))  # sabot
    sphere("Tail", bristle, (0, 0.62, 0.55), (0.035, 0.035, 0.04))

    bones = quad_bones(0.20, -0.36, 0.42, 0.56, ((0, 0.40, 0.55), (0, -0.40, 0.62)), {
        "Head": ("Body", (0, -0.55, 0.65), (0, -0.98, 0.55)),
        "Tail": ("Body", (0, 0.58, 0.55), (0, 0.72, 0.55)),
    })

    def idle(key_rot, key_loc):
        # Renifle le sol, groin qui fouille.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, down in ((1, 0.15), (12, 0.35), (22, 0.15), (32, 0.35), (40, 0.15)):
            key_rot("Head", f, (down, 0, 0))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 18, lambda kr: None)

    def charge(key_rot, key_loc):
        # Charge tête baissée, foulée large et rapide.
        s = math.radians(38)
        for f, a in ((1, s), (9, -s), (16, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, down in ((1, 0.45), (16, 0.45)):
            key_rot("Head", f, (down, 0, 0))
        for f, dz in ((1, -0.02), (5, 0.08), (9, -0.02), (13, 0.08), (16, -0.02)):
            key_loc("Body", f, (0, dz, 0))

    build_creature3("creature81", bones, idle, walk, charge, "Charge", cam=0.9)


# =============================================================================
# Créature 82 — Âne : grandes oreilles, braiment tête renversée.
# =============================================================================
def ane():
    fresh_scene()
    grey = material("Ane82Grey", (0.52, 0.48, 0.46))
    cream = material("Ane82Cream", (0.86, 0.82, 0.74))
    dark = material("Ane82Dark", (0.08, 0.07, 0.07))

    sphere("Body", grey, (0, 0.10, 0.85), (0.30, 0.62, 0.32))
    sphere("Body", cream, (0, -0.30, 0.72), (0.20, 0.24, 0.20))  # poitrail
    sphere("Head", grey, (0, -0.75, 1.10), (0.16, 0.20, 0.16))
    sphere("Head", grey, (0, -0.62, 1.06), (0.20, 0.26, 0.20))  # cou
    sphere("Head", grey, (0, -0.38, 0.92), (0.24, 0.30, 0.24))  # cou, referme la selle tête-corps
    sphere("Head", cream, (0, -1.02, 1.02), (0.09, 0.13, 0.08))  # museau
    sphere("Head", dark, (0, -1.20, 1.00), (0.03, 0.026, 0.026))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.90, 1.16), (0.03, 0.025, 0.03))
        # Grandes oreilles dressées.
        cone("Head", grey, (sx * 0.14, -0.62, 1.42), (0.07, 0.045, 0.24),
             rotation=(math.radians(-10), 0, math.radians(sx * 8)))
    # Crinière courte hérissée.
    for k, y in enumerate((-0.75, -0.55, -0.35, -0.15)):
        cone("Head" if y < -0.60 else "Body", dark, (0, y, 1.28 - 0.04 * k),
             (0.03, 0.06, 0.07), rotation=(math.radians(-90), 0, 0))
    for bone, x, y in (("LegFL", -0.20, -0.42), ("LegFR", 0.20, -0.42),
                       ("LegBL", -0.20, 0.48), ("LegBR", 0.20, 0.48)):
        cylinder(bone, grey, (x, y, 0.34), (0.09, 0.09, 0.66))
        cylinder(bone, dark, (x, y, 0.03), (0.10, 0.10, 0.06))  # sabot
    for y, z, r in ((0.88, 0.72, 0.06), (1.05, 0.55, 0.05)):
        sphere("Tail", grey, (0, y, z), (r, r, r))
    sphere("Tail", dark, (0, 1.18, 0.42), (0.05, 0.09, 0.05))

    bones = quad_bones(0.20, -0.42, 0.48, 0.68, ((0, 0.45, 0.82), (0, -0.45, 0.92)), {
        "Head": ("Body", (0, -0.60, 1.00), (0, -1.25, 1.05)),
        "Tail": ("Body", (0, 0.82, 0.72), (0, 1.20, 0.42)),
    })

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (10, 0.3), (20, -0.3), (30, 0.0), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 20, extras)

    def braiment(key_rot, key_loc):
        # Braiment : tête rejetée en arrière et TIENT la pose, oreilles à plat.
        for f, up in ((1, 0.0), (8, -0.7), (12, -0.8), (24, -0.8), (30, 0.0),
                      (36, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, sw in ((1, 0.0), (12, 0.6), (24, -0.6), (36, 0.0)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature3("creature82", bones, idle, walk, braiment, "Braiment", cam=0.95)


kangourou()
toucan()
iguane()
ecureuil()
otarie()
flamant()
perroquet()
lynx()
sanglier()
ane()
print("PACK DONE")
