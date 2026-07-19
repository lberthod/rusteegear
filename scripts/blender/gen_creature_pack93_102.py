"""Génère assets/models/creature93.glb … creature102.glb : 10 animaux, pack 3/3.

Antilope, chacal, vautour, belette, castor, pélican, aigle, coyote, blaireau,
cerf. Trois clips par créature (Idle, Walk, + une action signature), technique
`creature_kit.py`. QA par `check_creatures.py` après génération.

Leçons des packs 73-82 et 83-92 (session) :
1. Chevauchement tête/corps : agrandir directement la sphère de tête pour
   qu'elle recouvre largement le corps est plus fiable qu'un bloc de « pont »
   séparé (qui peut lui-même créer un nouveau trou s'il est mal placé).
2. Animation : chaque frame ne reçoit qu'UN SEUL appel `key_rot(bone, ...)`
   par os — deux appels sur la même frame s'écrasent l'un l'autre et peuvent
   rouvrir la boucle Idle→Walk sans avertissement visuel.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack93_102.py
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
    `third_name` en plus d'Idle/Walk — dupliqué ici plutôt que de toucher
    `creature_kit.py`, partagé par une quinzaine d'autres packs."""
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
# Créature 93 — Antilope : cornes fines en spirale, bonds légers.
# =============================================================================
def antilope():
    fresh_scene()
    tan = material("Antilope93Tan", (0.68, 0.52, 0.30))
    cream = material("Antilope93Cream", (0.88, 0.82, 0.68))
    horn = material("Antilope93Horn", (0.30, 0.24, 0.16))
    dark = material("Antilope93Dark", (0.07, 0.06, 0.06))

    sphere("Body", tan, (0, 0.10, 0.85), (0.26, 0.50, 0.28))
    sphere("Body", cream, (0, -0.32, 0.72), (0.18, 0.20, 0.16))
    # Tête large, chevauche largement le corps (leçon session).
    sphere("Head", tan, (0, -0.56, 1.10), (0.22, 0.26, 0.20))
    sphere("Head", cream, (0, -0.92, 1.00), (0.09, 0.13, 0.08))
    sphere("Head", dark, (0, -1.06, 0.98), (0.03, 0.026, 0.026))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.11, -0.78, 1.16), (0.03, 0.025, 0.03))
        sphere("Head", tan, (sx * 0.18, -0.58, 1.22), (0.06, 0.04, 0.07))
        for k in range(10):  # cornes annelées fines, chevauchement dense
            t = k / 9.0
            sphere("Head", horn, (sx * (0.10 + 0.02 * t), -0.56 + 0.06 * math.sin(t * 4),
                                   1.30 + 0.42 * t), (0.028 - 0.012 * t, 0.028, 0.028 - 0.012 * t))
    for bone, x, y in (("LegFL", -0.16, -0.34), ("LegFR", 0.16, -0.34),
                       ("LegBL", -0.16, 0.40), ("LegBR", 0.16, 0.40)):
        cylinder(bone, tan, (x, y, 0.32), (0.055, 0.055, 0.62))
        cylinder(bone, dark, (x, y, 0.04), (0.06, 0.06, 0.06))
    sphere("Tail", cream, (0, 0.60, 0.86), (0.05, 0.05, 0.06))

    bones = quad_bones(0.16, -0.34, 0.40, 0.62, ((0, 0.35, 0.80), (0, -0.35, 0.90)), {
        "Head": ("Body", (0, -0.45, 0.98), (0, -1.10, 1.02)),
        "Tail": ("Body", (0, 0.55, 0.85), (0, 0.72, 0.85)),
    })

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 24, lambda kr: None)

    def bond(key_rot, key_loc):
        # Bonds légers et rebondis, comme un ressort — fuite gracieuse.
        for f, dz in ((1, -0.04), (6, 0.22), (12, -0.04), (18, 0.22), (24, -0.04)):
            key_loc("Body", f, (0, dz, 0))
        for leg in LEGS4:
            for f, a in ((1, 10), (6, -35), (12, 10), (18, -35), (24, 10)):
                key_rot(leg, f, (math.radians(a), 0, 0))

    build_creature3("creature93", bones, idle, walk, bond, "Bond", cam=1.0)


# =============================================================================
# Créature 94 — Chacal : opportuniste, hurle avec la meute.
# =============================================================================
def chacal():
    fresh_scene()
    grey = material("Chacal94Grey", (0.52, 0.42, 0.28))
    cream = material("Chacal94Cream", (0.78, 0.72, 0.58))
    dark = material("Chacal94Dark", (0.08, 0.07, 0.06))

    sphere("Body", grey, (0, 0.10, 0.60), (0.20, 0.44, 0.22))
    sphere("Body", cream, (0, -0.24, 0.52), (0.14, 0.18, 0.14))
    sphere("Head", grey, (0, -0.44, 0.78), (0.18, 0.20, 0.17))
    sphere("Head", cream, (0, -0.66, 0.70), (0.09, 0.12, 0.07))
    sphere("Head", dark, (0, -0.78, 0.68), (0.028, 0.024, 0.024))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -0.52, 0.86), (0.028, 0.024, 0.03))
        cone("Head", grey, (sx * 0.15, -0.34, 1.00), (0.06, 0.04, 0.12),
             rotation=(math.radians(-8), 0, math.radians(sx * 10)))
    for bone, x, y in (("LegFL", -0.14, -0.30), ("LegFR", 0.14, -0.30),
                       ("LegBL", -0.14, 0.36), ("LegBR", 0.14, 0.36)):
        cylinder(bone, grey, (x, y, 0.24), (0.06, 0.06, 0.46))
        sphere(bone, cream, (x, y - 0.03, 0.05), (0.065, 0.08, 0.03))
    for y, z, r in ((0.55, 0.56, 0.07), (0.75, 0.44, 0.06)):
        sphere("Tail", grey, (0, y, z), (r, r * 1.3, r))
    sphere("Tail", dark, (0, 0.90, 0.34), (0.05, 0.06, 0.05))

    bones = quad_bones(0.14, -0.30, 0.36, 0.46, ((0, 0.28, 0.56), (0, -0.28, 0.62)), {
        "Head": ("Body", (0, -0.38, 0.70), (0, -0.78, 0.78)),
        "Tail": ("Body", (0, 0.48, 0.56), (0, 0.95, 0.32)),
    })

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (14, 0.25), (28, -0.25), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 26, extras)

    def hurle(key_rot, key_loc):
        # Hurle : museau au ciel, tient la note, queue dressée.
        f0, f_up, f_hold, f_end = 1, 8, 20, 28
        for f, up in ((f0, 0.0), (f_up, -0.85), (f_hold, -0.85), (f_end, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, up in ((f0, 0.1), (f_up, -0.3), (f_hold, -0.3), (f_end, 0.1)):
            key_rot("Tail", f, (up, 0, 0))

    build_creature3("creature94", bones, idle, walk, hurle, "Hurle", cam=0.8)


# =============================================================================
# Créature 95 — Vautour : plane immobile, dispute une charogne.
# =============================================================================
def vautour():
    fresh_scene()
    dark = material("Vautour95Dark", (0.16, 0.15, 0.15))
    pink = material("Vautour95Pink", (0.72, 0.42, 0.38))
    cream = material("Vautour95Cream", (0.66, 0.60, 0.48))

    sphere("Body", dark, (0, 0.05, 0.66), (0.22, 0.32, 0.30))
    sphere("Body", cream, (0, -0.16, 0.62), (0.13, 0.16, 0.18))
    sphere("Head", pink, (0, -0.08, 0.98), (0.13, 0.13, 0.12))
    cone("Head", cream, (0, -0.24, 0.92), (0.045, 0.045, 0.14),
         rotation=(math.radians(100), 0, 0))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.08, -0.10, 1.04), (0.024, 0.02, 0.024))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        for k in range(3):
            t = k / 2.0
            sphere(bone, dark, (sx * (0.26 + 0.30 * t), 0.05 + 0.05 * t, 0.66 - 0.04 * t),
                   (0.10 - 0.02 * t, 0.18 - 0.03 * t, 0.03))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, cream, (sx * 0.08, 0.02, 0.22), (0.025, 0.025, 0.16))
        sphere(bone, dark, (sx * 0.08, -0.05, 0.05), (0.045, 0.065, 0.02))
    sphere("Tail", dark, (0, 0.35, 0.60), (0.06, 0.12, 0.03))

    bones = {
        "Body": ("Root", (0, 0.28, 0.64), (0, -0.14, 0.70)),
        "Head": ("Body", (0, -0.10, 0.85), (0, -0.15, 1.00)),
        "WingL": ("Body", (-0.22, 0.05, 0.68), (-0.68, 0.20, 0.55)),
        "WingR": ("Body", (0.22, 0.05, 0.68), (0.68, 0.20, 0.55)),
        "Tail": ("Body", (0, 0.32, 0.62), (0, 0.55, 0.58)),
        "LegL": ("Body", (-0.08, 0.02, 0.34), (-0.08, 0.02, 0.02)),
        "LegR": ("Body", (0.08, 0.02, 0.34), (0.08, 0.02, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Immobile, ailes mi-ouvertes, tête qui pivote lentement.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, a in ((1, 0.15), (20, 0.25), (40, 0.15)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, yaw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        s = math.radians(16)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, a in ((1, 0.4), (13, 0.5), (24, 0.4)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))

    def dispute(key_rot, key_loc):
        # Dispute la charogne : ailes déployées en menace, bonds latéraux.
        for f, a in ((1, 0.3), (6, 1.0), (12, 0.3), (18, 1.0), (24, 0.3)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, sway in ((1, 0.0), (6, 0.4), (12, -0.4), (18, 0.4), (24, 0.0)):
            key_rot("Body", f, (0, 0, sway))

    build_creature3("creature95", bones, idle, walk, dispute, "Dispute", cam=0.65)


# =============================================================================
# Créature 96 — Belette : corps serpentin, plonge dans un terrier.
# =============================================================================
def belette():
    fresh_scene()
    fur = material("Belette96Fur", (0.62, 0.42, 0.20))
    cream = material("Belette96Cream", (0.92, 0.88, 0.78))
    dark = material("Belette96Dark", (0.07, 0.06, 0.06))

    sphere("Body", fur, (0, -0.05, 0.14), (0.10, 0.36, 0.10))
    sphere("Body", cream, (0, -0.05, 0.07), (0.075, 0.32, 0.06))
    sphere("Head", fur, (0, -0.38, 0.16), (0.085, 0.10, 0.075))
    sphere("Head", cream, (0, -0.46, 0.11), (0.045, 0.05, 0.035))
    sphere("Head", dark, (0, -0.51, 0.10), (0.016, 0.014, 0.014))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.05, -0.40, 0.21), (0.017, 0.015, 0.017))
        sphere("Head", fur, (sx * 0.06, -0.30, 0.24), (0.024, 0.018, 0.024))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, fur, (sx * 0.06, 0.02, 0.06), (0.018, 0.018, 0.06))
    for k in range(5):  # queue longue effilée
        t = k / 4.0
        sphere("Tail", fur, (0, 0.30 + 0.24 * t, 0.13 - 0.02 * t),
               (0.06 - 0.03 * t, 0.11, 0.06 - 0.03 * t))

    bones = {
        "Body": ("Root", (0, 0.18, 0.13), (0, -0.15, 0.14)),
        "Head": ("Body", (0, -0.15, 0.15), (0, -0.48, 0.16)),
        "LegL": ("Body", (-0.06, 0.02, 0.09), (-0.06, 0.02, 0.01)),
        "LegR": ("Body", (0.06, 0.02, 0.09), (0.06, 0.02, 0.01)),
        "Tail": ("Body", (0, 0.24, 0.13), (0, 0.68, 0.05)),
    }

    def idle(key_rot, key_loc):
        # Dressée, vigilante, corps ondule très légèrement.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, up in ((1, 0.0), (12, -0.4), (28, -0.4), (40, 0.0)):
            key_rot("Body", f, (up, 0, 0))
        for f, yaw in ((1, 0.0), (14, 0.4), (28, -0.4), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        # Ondulation serpentine, corps qui zigzague au ras du sol.
        for f, yaw in ((1, 0.3), (13, -0.3), (24, 0.3)):
            key_rot("Body", f, (0, 0, yaw))
        for f, dz in ((1, 0.0), (7, 0.02), (13, 0.0), (19, 0.02), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.4), (13, -0.4), (24, 0.4)):
            key_rot("Tail", f, (0, 0, sw))

    def plonge(key_rot, key_loc):
        # Plonge tête la première dans un terrier — corps s'étire et disparaît.
        f0, f_dive, f_end = 1, 10, 18
        for f, down, dy in ((f0, 0.0, 0.0), (f_dive, 0.8, 0.18), (f_end, 0.0, 0.0)):
            key_rot("Head", f, (down, 0, 0))
            key_loc("Body", f, (0, dy, 0))

    build_creature3("creature96", bones, idle, walk, plonge, "Plonge", cam=0.3)


# =============================================================================
# Créature 97 — Castor : queue plate, ronge une bûche.
# =============================================================================
def castor():
    fresh_scene()
    brown = material("Castor97Brown", (0.42, 0.28, 0.16))
    cream = material("Castor97Cream", (0.78, 0.70, 0.56))
    dark = material("Castor97Dark", (0.06, 0.06, 0.06))
    orange = material("Castor97Orange", (0.82, 0.58, 0.18))

    sphere("Body", brown, (0, 0.05, 0.28), (0.20, 0.34, 0.22))
    sphere("Head", brown, (0, -0.32, 0.32), (0.16, 0.17, 0.14))
    sphere("Head", cream, (0, -0.46, 0.24), (0.08, 0.09, 0.06))
    sphere("Head", dark, (0, -0.52, 0.22), (0.026, 0.022, 0.022))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.36, 0.40), (0.028, 0.024, 0.028))
        sphere("Head", brown, (sx * 0.13, -0.26, 0.46), (0.038, 0.03, 0.045))
        cylinder("Head", orange, (sx * 0.022, -0.48, 0.16), (0.014, 0.014, 0.05),
                 rotation=(math.radians(12), 0, 0))  # incisive
    for bone, x, y in (("LegFL", -0.13, -0.20), ("LegFR", 0.13, -0.20),
                       ("LegBL", -0.13, 0.24), ("LegBR", 0.13, 0.24)):
        cylinder(bone, brown, (x, y, 0.14), (0.05, 0.05, 0.14))
        sphere(bone, dark, (x, y, 0.02), (0.055, 0.07, 0.02))
    # Queue plate large, écailleuse (texture par petites bosses).
    sphere("Tail", dark, (0, 0.42, 0.16), (0.16, 0.22, 0.045))
    for k in range(6):
        sphere("Tail", brown, (0, 0.28 + (k % 3) * 0.14, 0.17 + (k // 3) * 0.02),
               (0.02, 0.02, 0.012))

    bones = {
        "Body": ("Root", (0, 0.22, 0.26), (0, -0.14, 0.30)),
        "Head": ("Body", (0, -0.14, 0.32), (0, -0.50, 0.30)),
        "Tail": ("Body", (0, 0.24, 0.24), (0, 0.55, 0.16)),
        "LegFL": ("Body", (-0.13, -0.20, 0.16), (-0.13, -0.20, 0.02)),
        "LegFR": ("Body", (0.13, -0.20, 0.16), (0.13, -0.20, 0.02)),
        "LegBL": ("Body", (-0.13, 0.24, 0.16), (-0.13, 0.24, 0.02)),
        "LegBR": ("Body", (0.13, 0.24, 0.16), (0.13, 0.24, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Ronge une bûche assise : petite mastication rapide de la tête.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, nod in ((1, 0.0), (5, 0.12), (10, 0.0), (15, 0.12), (20, 0.0),
                       (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.05), (20, -0.05), (40, 0.05)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 14, lambda kr: None)

    def claque(key_rot, key_loc):
        # Claque la queue plate sur l'eau, signal d'alarme.
        for f, a in ((1, 0.0), (5, 0.4), (9, -0.5), (13, 0.1), (17, 0.0)):
            key_rot("Tail", f, (a, 0, 0))

    build_creature3("creature97", bones, idle, walk, claque, "Claque", cam=0.4)


# =============================================================================
# Créature 98 — Pélican : grand bec-poche, gobe un poisson.
# =============================================================================
def pelican():
    fresh_scene()
    white = material("Pelican98White", (0.92, 0.91, 0.88))
    grey = material("Pelican98Grey", (0.66, 0.66, 0.64))
    orange = material("Pelican98Orange", (0.86, 0.56, 0.20))

    sphere("Body", white, (0, 0.10, 0.60), (0.26, 0.36, 0.30))
    sphere("Head", white, (0, -0.30, 0.82), (0.15, 0.15, 0.14))
    cone("Head", orange, (0, -0.55, 0.76), (0.08, 0.08, 0.32),
         rotation=(math.radians(95), 0, 0))  # long bec
    sphere("Head", orange, (0, -0.62, 0.62), (0.09, 0.16, 0.07))  # poche gulaire
    for sx in (-1, 1):
        sphere("Head", grey, (sx * 0.09, -0.20, 0.90), (0.022, 0.02, 0.022))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, grey, (sx * 0.28, 0.05, 0.60), (0.09, 0.24, 0.24))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, orange, (sx * 0.10, 0.02, 0.20), (0.028, 0.028, 0.16))
        sphere(bone, orange, (sx * 0.10, -0.05, 0.04), (0.06, 0.09, 0.02))

    bones = {
        "Body": ("Root", (0, 0.30, 0.58), (0, -0.14, 0.66)),
        "Head": ("Body", (0, -0.14, 0.75), (0, -0.20, 0.90)),
        "WingL": ("Body", (-0.20, 0.05, 0.65), (-0.55, 0.10, 0.40)),
        "WingR": ("Body", (0.20, 0.05, 0.65), (0.55, 0.10, 0.40)),
        "LegL": ("Body", (-0.10, 0.02, 0.32), (-0.10, 0.02, 0.02)),
        "LegR": ("Body", (0.10, 0.02, 0.32), (0.10, 0.02, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for b in ("LegL", "LegR", "WingL", "WingR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, nod in ((1, 0.0), (14, 0.2), (28, 0.0), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        s = math.radians(14)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    def gobe(key_rot, key_loc):
        # Plonge le bec, la poche gonfle en gobant, tête se redresse.
        f0, f_dip, f_swallow, f_end = 1, 8, 16, 24
        for f, down in ((f0, 0.0), (f_dip, 0.7), (f_swallow, -0.3), (f_end, 0.0)):
            key_rot("Head", f, (down, 0, 0))

    build_creature3("creature98", bones, idle, walk, gobe, "Gobe", cam=0.7)


# =============================================================================
# Créature 99 — Aigle : serres puissantes, se pose avec autorité.
# =============================================================================
def aigle():
    fresh_scene()
    brown = material("Aigle99Brown", (0.30, 0.22, 0.15))
    white = material("Aigle99White", (0.92, 0.90, 0.85))
    yellow = material("Aigle99Yellow", (0.85, 0.62, 0.14))
    dark = material("Aigle99Dark", (0.07, 0.06, 0.06))

    sphere("Body", brown, (0, 0.05, 0.66), (0.20, 0.28, 0.28))
    sphere("Head", white, (0, -0.06, 0.98), (0.13, 0.13, 0.12))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.08, -0.14, 1.04), (0.026, 0.022, 0.026))
    cone("Head", yellow, (0, -0.20, 0.90), (0.04, 0.04, 0.10),
         rotation=(math.radians(115), 0, 0))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        for k in range(3):
            t = k / 2.0
            sphere(bone, brown, (sx * (0.24 + 0.28 * t), 0.05 + 0.04 * t, 0.66 - 0.03 * t),
                   (0.10 - 0.02 * t, 0.17 - 0.02 * t, 0.03))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, yellow, (sx * 0.09, 0.02, 0.24), (0.028, 0.028, 0.18))
        for k in range(3):  # serres puissantes
            a = math.radians(-30 + k * 30)
            sphere(bone, dark, (sx * 0.09 + 0.05 * math.sin(a), 0.02 + 0.05 * math.cos(a), 0.02),
                   (0.02, 0.03, 0.018))

    bones = {
        "Body": ("Root", (0, 0.28, 0.64), (0, -0.14, 0.70)),
        "Head": ("Body", (0, -0.10, 0.85), (0, -0.15, 1.00)),
        "WingL": ("Body", (-0.20, 0.05, 0.68), (-0.60, 0.15, 0.50)),
        "WingR": ("Body", (0.20, 0.05, 0.68), (0.60, 0.15, 0.50)),
        "LegL": ("Body", (-0.09, 0.02, 0.36), (-0.09, 0.02, 0.02)),
        "LegR": ("Body", (0.09, 0.02, 0.36), (0.09, 0.02, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, a in ((1, 0.1), (20, 0.2), (40, 0.1)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, yaw in ((1, 0.0), (14, 0.25), (28, -0.25), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        s = math.radians(12)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, a in ((1, 0.3), (13, 0.35), (24, 0.3)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))

    def pose(key_rot, key_loc):
        # Se pose avec autorité : ailes freinent grand ouvertes, serres tendues.
        f0, f_brake, f_land, f_end = 1, 8, 14, 22
        for f, a in ((f0, 0.0), (f_brake, 1.2), (f_land, 0.3), (f_end, 0.0)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, dz in ((f0, 0.10), (f_brake, 0.10), (f_land, -0.04), (f_end, 0.10)):
            key_loc("Body", f, (0, dz, 0))

    build_creature3("creature99", bones, idle, walk, pose, "Atterrit", cam=0.65)


# =============================================================================
# Créature 100 — Coyote : opportuniste, jappe et guette.
# =============================================================================
def coyote():
    fresh_scene()
    grey = material("Coyote100Grey", (0.58, 0.52, 0.42))
    cream = material("Coyote100Cream", (0.84, 0.80, 0.68))
    dark = material("Coyote100Dark", (0.08, 0.07, 0.06))

    sphere("Body", grey, (0, 0.10, 0.62), (0.21, 0.44, 0.22))
    sphere("Body", cream, (0, -0.24, 0.54), (0.15, 0.18, 0.14))
    sphere("Head", grey, (0, -0.38, 0.78), (0.21, 0.24, 0.19))
    sphere("Head", cream, (0, -0.60, 0.72), (0.09, 0.12, 0.07))
    sphere("Head", dark, (0, -0.72, 0.70), (0.028, 0.024, 0.024))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -0.46, 0.88), (0.028, 0.024, 0.03))
        cone("Head", grey, (sx * 0.14, -0.34, 0.92), (0.065, 0.045, 0.13),
             rotation=(math.radians(-8), 0, math.radians(sx * 10)))
    for bone, x, y in (("LegFL", -0.15, -0.30), ("LegFR", 0.15, -0.30),
                       ("LegBL", -0.15, 0.36), ("LegBR", 0.15, 0.36)):
        cylinder(bone, grey, (x, y, 0.25), (0.065, 0.065, 0.48))
        sphere(bone, cream, (x, y - 0.03, 0.05), (0.07, 0.085, 0.03))
    for y, z, r in ((0.56, 0.58, 0.075), (0.78, 0.46, 0.065)):
        sphere("Tail", grey, (0, y, z), (r, r * 1.3, r))
    sphere("Tail", dark, (0, 0.94, 0.36), (0.05, 0.06, 0.05))

    bones = quad_bones(0.15, -0.30, 0.36, 0.48, ((0, 0.28, 0.58), (0, -0.28, 0.64)), {
        "Head": ("Body", (0, -0.40, 0.72), (0, -0.80, 0.80)),
        "Tail": ("Body", (0, 0.50, 0.58), (0, 0.98, 0.34)),
    })

    def idle(key_rot, key_loc):
        # Guette, oreilles pivotent, corps tassé prêt à bondir.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, -0.02), (20, 0.0), (40, -0.02)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 24, extras)

    def jappe(key_rot, key_loc):
        # Jappements courts et vifs, tête qui pointe en avant à chaque cri.
        for f, a in ((1, 0.0), (4, -0.3), (7, 0.1), (10, -0.3), (13, 0.1),
                     (16, -0.3), (20, 0.0)):
            key_rot("Head", f, (a, 0, 0))

    build_creature3("creature100", bones, idle, walk, jappe, "Jappe", cam=0.85)


# =============================================================================
# Créature 101 — Blaireau : masque facial, fouisseur robuste.
# =============================================================================
def blaireau():
    fresh_scene()
    grey = material("Blaireau101Grey", (0.52, 0.50, 0.48))
    white = material("Blaireau101White", (0.92, 0.91, 0.88))
    black = material("Blaireau101Black", (0.08, 0.07, 0.07))

    sphere("Body", grey, (0, 0.10, 0.30), (0.22, 0.42, 0.22))
    sphere("Head", grey, (0, -0.42, 0.34), (0.16, 0.17, 0.14))
    sphere("Head", white, (0, -0.55, 0.30), (0.09, 0.10, 0.07))
    # Masque : bandes sombres verticales sur la face blanche.
    for sx in (-1, 1):
        sphere("Head", black, (sx * 0.06, -0.50, 0.36), (0.025, 0.09, 0.10))
        sphere("Head", black, (sx * 0.11, -0.44, 0.48), (0.03, 0.026, 0.03))
        sphere("Head", grey, (sx * 0.16, -0.28, 0.54), (0.045, 0.03, 0.06))
    sphere("Head", black, (0, -0.66, 0.26), (0.028, 0.024, 0.024))
    for bone, x, y in (("LegFL", -0.15, -0.22), ("LegFR", 0.15, -0.22),
                       ("LegBL", -0.15, 0.26), ("LegBR", 0.15, 0.26)):
        cylinder(bone, black, (x, y, 0.15), (0.06, 0.06, 0.16))
        sphere(bone, black, (x, y, 0.032), (0.07, 0.09, 0.025))
    sphere("Tail", grey, (0, 0.46, 0.32), (0.06, 0.08, 0.06))

    bones = {
        "Body": ("Root", (0, 0.24, 0.28), (0, -0.14, 0.32)),
        "Head": ("Body", (0, -0.14, 0.34), (0, -0.60, 0.30)),
        "Tail": ("Body", (0, 0.30, 0.28), (0, 0.58, 0.30)),
        "LegFL": ("Body", (-0.15, -0.22, 0.18), (-0.15, -0.22, 0.02)),
        "LegFR": ("Body", (0.15, -0.22, 0.18), (0.15, -0.22, 0.02)),
        "LegBL": ("Body", (-0.15, 0.26, 0.18), (-0.15, 0.26, 0.02)),
        "LegBR": ("Body", (0.15, 0.26, 0.18), (0.15, 0.26, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.015), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.15), (20, 0.30), (40, 0.15)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 16, lambda kr: None)

    def fouille(key_rot, key_loc):
        # Fouisseur : gratte le sol des pattes avant, tête plongeante.
        for f, a in ((1, 0.0), (5, -40), (9, 15), (13, -40), (17, 15), (21, 0.0)):
            key_rot("LegFL", f, (math.radians(a), 0, 0))
            key_rot("LegFR", f, (math.radians(a), 0, 0))
        for f, down in ((1, 0.2), (11, 0.4), (21, 0.2)):
            key_rot("Head", f, (down, 0, 0))
        for f in (1, 21):
            key_rot("LegBL", f, (0, 0, 0))
            key_rot("LegBR", f, (0, 0, 0))

    build_creature3("creature101", bones, idle, walk, fouille, "Fouille", cam=0.55)


# =============================================================================
# Créature 102 — Cerf : bois ramifiés majestueux, brame en Idle.
# =============================================================================
def cerf():
    fresh_scene()
    brown = material("Cerf102Brown", (0.44, 0.32, 0.20))
    cream = material("Cerf102Cream", (0.78, 0.72, 0.58))
    antler = material("Cerf102Antler", (0.62, 0.54, 0.40))
    dark = material("Cerf102Dark", (0.08, 0.07, 0.06))

    sphere("Body", brown, (0, 0.10, 1.00), (0.30, 0.62, 0.32))
    sphere("Body", cream, (0, -0.34, 0.90), (0.20, 0.22, 0.20))
    sphere("Head", brown, (0, -0.62, 1.28), (0.24, 0.38, 0.22))
    sphere("Head", dark, (0, -0.98, 1.20), (0.055, 0.045, 0.045))
    sphere("Head", cream, (0, -0.90, 1.16), (0.10, 0.14, 0.08))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.12, -0.72, 1.38), (0.038, 0.03, 0.04))
        sphere("Head", brown, (sx * 0.20, -0.48, 1.46), (0.07, 0.045, 0.09))
        # Bois ramifiés : tige principale + 3 andouillers, chevauchement dense.
        for k in range(9):
            t = k / 8.0
            sphere("Head", antler, (sx * (0.14 + 0.16 * t), -0.50 + 0.30 * t,
                                     1.56 + 0.60 * t), (0.055 - 0.02 * t, 0.055, 0.055 - 0.02 * t))
        for br in range(3):
            by = -0.55 + br * 0.22
            for k in range(3):
                t = k / 2.0
                sphere("Head", antler,
                       (sx * (0.18 + 0.14 * br + 0.10 * t), by - 0.05 * t,
                        1.75 + 0.10 * br + 0.14 * t), (0.04 - 0.012 * t,) * 3)
    for bone, x, y in (("LegFL", -0.20, -0.44), ("LegFR", 0.20, -0.44),
                       ("LegBL", -0.20, 0.52), ("LegBR", 0.20, 0.52)):
        cylinder(bone, brown, (x, y, 0.38), (0.09, 0.09, 0.72))
        cylinder(bone, dark, (x, y, 0.04), (0.10, 0.10, 0.08))
    sphere("Tail", cream, (0, 0.68, 1.02), (0.06, 0.06, 0.07))

    bones = quad_bones(0.20, -0.44, 0.52, 0.76, ((0, 0.45, 0.95), (0, -0.45, 1.05)), {
        "Head": ("Body", (0, -0.55, 1.15), (0, -1.05, 1.30)),
        "Tail": ("Body", (0, 0.62, 1.00), (0, 0.80, 1.00)),
    })

    def idle(key_rot, key_loc):
        # Brame : tête levée, museau au ciel, TIENT la pose (le cri du rut).
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, 0.0), (8, -0.6), (12, -0.7), (26, -0.7), (32, 0.0),
                      (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
                kr("Head", f, (nod, 0, 0))
        quad_walk_keys(key_rot, key_loc, 22, extras)

    def combat(key_rot, key_loc):
        # Combat de bois : charge frontale, tête baissée, choc et recul.
        f0, f_lower, f_impact, f_end = 1, 8, 14, 24
        for f, down in ((f0, 0.0), (f_lower, 0.5), (f_impact, 0.65), (f_end, 0.0)):
            key_rot("Head", f, (down, 0, 0))
        for f, dy in ((f0, 0.0), (f_lower, 0.05), (f_impact, 0.18), (f_end, 0.0)):
            key_loc("Body", f, (0, dy, 0))

    build_creature3("creature102", bones, idle, walk, combat, "Combat", cam=1.05)


antilope()
chacal()
vautour()
belette()
castor()
pelican()
aigle()
coyote()
blaireau()
cerf()
print("PACK DONE")
