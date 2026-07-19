"""Génère assets/models/creature83.glb … creature92.glb : 10 animaux, pack 2/3.

Mouflon, lama, ocelot, faucon, tapir, wombat, gecko, mangouste, raton laveur,
cigogne. Trois clips par créature (Idle, Walk, + une action signature),
technique `creature_kit.py`. QA par `check_creatures.py` après génération.

Leçon du pack 73-82 (session) : une tête juste « posée à touche-touche »
contre le corps laisse un trou vu depuis la caméra en plongée — chaque
créature ici pose donc un ou deux blocs de cou qui recouvrent largement le
corps ET la tête (pas un simple pont tangent) dès la première passe.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack83_92.py
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


def neck_bridge(bone, mat, head_pos, body_pos, r=0.22):
    """Un bloc de cou massif qui recouvre largement tête ET corps (pas un
    pont tangent sur leur ligne de centres, trop bas) — piège documenté au
    pack 73-82 (session) : une tête juste posée contre le corps, ou un pont
    trop petit/trop bas, laisse un trou vu depuis la caméra en plongée. Le
    bloc est un seul volume généreux (rayon comparable à tête/corps, pas un
    simple accent) centré au 2/3 vers la tête et remonté vers la crête haute
    (pas la ligne de centres, plus basse)."""
    hx, hy, hz = head_pos
    bx, by, bz = body_pos
    t = 0.45
    cy = hy + (by - hy) * t
    cz = max(hz, bz) + 0.06
    sphere(bone, mat, (0, cy, cz), (r * 1.3, r * 1.5, r * 1.15))


# =============================================================================
# Créature 83 — Mouflon : cornes enroulées, combat de têtes.
# =============================================================================
def mouflon():
    fresh_scene()
    tan = material("Mouflon83Tan", (0.56, 0.46, 0.32))
    cream = material("Mouflon83Cream", (0.82, 0.76, 0.62))
    horn = material("Mouflon83Horn", (0.58, 0.48, 0.34))
    dark = material("Mouflon83Dark", (0.08, 0.07, 0.06))

    sphere("Body", tan, (0, 0.10, 0.92), (0.44, 0.72, 0.42))
    sphere("Body", cream, (0, -0.40, 0.82), (0.30, 0.30, 0.26))
    sphere("Head", tan, (0, -0.82, 1.18), (0.24, 0.26, 0.22))
    neck_bridge("Head", tan, (0, -0.82, 1.18), (0, -0.10, 0.98))
    sphere("Head", cream, (0, -1.05, 1.08), (0.11, 0.13, 0.09))
    sphere("Head", dark, (0, -1.18, 1.06), (0.04, 0.035, 0.035))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.12, -0.94, 1.26), (0.04, 0.032, 0.045))
        sphere("Head", tan, (sx * 0.20, -0.70, 1.32), (0.07, 0.05, 0.08))
        # Cornes enroulées : grande spirale de sphères denses.
        for k in range(14):
            t = k / 13.0
            a = t * 3.4
            hy = -0.66 + 0.34 * math.sin(a)
            hz = 1.36 + 0.30 * (1 - math.cos(a)) - 0.18 * t
            hr = 0.06 - t * 0.03
            sphere("Head", horn, (sx * (0.20 + t * 0.05 + 0.10 * math.sin(a * 0.5)),
                                   hy, hz), (hr, hr, hr))
    for bone, x, y in (("LegFL", -0.26, -0.44), ("LegFR", 0.26, -0.44),
                       ("LegBL", -0.26, 0.52), ("LegBR", 0.26, 0.52)):
        cylinder(bone, tan, (x, y, 0.36), (0.10, 0.10, 0.70))
        cylinder(bone, dark, (x, y, 0.05), (0.11, 0.11, 0.09))
    sphere("Tail", cream, (0, 0.78, 0.98), (0.07, 0.07, 0.08))

    bones = quad_bones(0.26, -0.44, 0.52, 0.72, ((0, 0.44, 0.88), (0, -0.44, 0.98)), {
        "Head": ("Body", (0, -0.58, 1.06), (0, -1.15, 1.20)),
        "Tail": ("Body", (0, 0.70, 0.98), (0, 0.92, 0.98)),
    })

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, -0.1), (20, -0.2), (40, -0.1)):
            key_rot("Head", f, (up, 0, 0))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 20, lambda kr: None)

    def combat(key_rot, key_loc):
        # Charge tête baissée puis choc frontal violent, tête qui recule.
        f0, f_lower, f_impact, f_end = 1, 8, 14, 24
        for f, down in ((f0, 0.0), (f_lower, 0.55), (f_impact, 0.75),
                        (f_end, 0.0)):
            key_rot("Head", f, (down, 0, 0))
        for f, dz in ((f0, 0.0), (f_lower, 0.05), (f_impact, -0.10), (f_end, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, dy in ((f0, 0.0), (f_lower, 0.05), (f_impact, 0.20), (f_end, 0.0)):
            key_loc("Head", f, (0, dy, 0))

    build_creature3("creature83", bones, idle, walk, combat, "Combat", cam=1.0)


# =============================================================================
# Créature 84 — Lama : long cou, crache sur l'intrus.
# =============================================================================
def lama():
    fresh_scene()
    beige = material("Lama84Beige", (0.78, 0.68, 0.52))
    cream = material("Lama84Cream", (0.90, 0.86, 0.76))
    dark = material("Lama84Dark", (0.08, 0.07, 0.07))

    sphere("Body", beige, (0, 0.10, 1.00), (0.28, 0.55, 0.40))
    for k in range(5):  # long cou, chaîne dense
        t = k / 4.0
        sphere("Neck", beige, (0, -0.40 - 0.42 * t, 1.15 + 0.55 * t),
               (0.16 - 0.02 * t, 0.20, 0.16 - 0.02 * t))
    sphere("Head", beige, (0, -1.28, 1.72), (0.13, 0.17, 0.12))
    sphere("Head", cream, (0, -1.44, 1.66), (0.07, 0.09, 0.06))
    sphere("Head", dark, (0, -1.54, 1.64), (0.025, 0.022, 0.022))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.08, -1.32, 1.80), (0.028, 0.024, 0.028))
        sphere("Head", beige, (sx * 0.10, -1.16, 1.90), (0.045, 0.03, 0.10))  # oreille
    for bone, x, y in (("LegFL", -0.16, -0.42), ("LegFR", 0.16, -0.42),
                       ("LegBL", -0.16, 0.48), ("LegBR", 0.16, 0.48)):
        cylinder(bone, beige, (x, y, 0.40), (0.075, 0.075, 0.80))
        cylinder(bone, dark, (x, y, 0.05), (0.08, 0.08, 0.08))
    sphere("Tail", beige, (0, 0.62, 0.95), (0.06, 0.06, 0.07))

    bones = {
        "Body": ("Root", (0, 0.42, 0.95), (0, -0.30, 1.05)),
        "Neck": ("Body", (0, -0.30, 1.05), (0, -1.10, 1.65)),
        "Head": ("Neck", (0, -1.10, 1.65), (0, -1.50, 1.80)),
        "Tail": ("Body", (0, 0.55, 0.95), (0, 0.70, 0.90)),
        "LegFL": ("Body", (-0.16, -0.42, 0.42), (-0.16, -0.42, 0.02)),
        "LegFR": ("Body", (0.16, -0.42, 0.42), (0.16, -0.42, 0.02)),
        "LegBL": ("Body", (-0.16, 0.48, 0.42), (-0.16, 0.48, 0.02)),
        "LegBR": ("Body", (0.16, 0.48, 0.42), (0.16, 0.48, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.0), (12, 0.2), (24, -0.2), (40, 0.0)):
            key_rot("Neck", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                kr("Neck", f, (nod, 0, 0))
        quad_walk_keys(key_rot, key_loc, 22, extras)

    def crache(key_rot, key_loc):
        # Recule la tête, puis la projette en avant façon crachat.
        f0, f_pull, f_spit, f_end = 1, 8, 14, 24
        for f, curl in ((f0, 0.0), (f_pull, 0.5), (f_spit, -0.6), (f_end, 0.0)):
            key_rot("Neck", f, (curl, 0, 0))
        for f, curl in ((f0, 0.0), (f_pull, 0.2), (f_spit, -0.4), (f_end, 0.0)):
            key_rot("Head", f, (curl, 0, 0))

    build_creature3("creature84", bones, idle, walk, crache, "Crache", cam=1.1)


# =============================================================================
# Créature 85 — Ocelot : robe tachetée, chasse tapie dans les herbes.
# =============================================================================
def ocelot():
    fresh_scene()
    tan = material("Ocelot85Tan", (0.76, 0.58, 0.34))
    cream = material("Ocelot85Cream", (0.92, 0.88, 0.76))
    spot = material("Ocelot85Spot", (0.18, 0.13, 0.09))

    sphere("Body", tan, (0, 0.10, 0.44), (0.22, 0.48, 0.22))
    sphere("Body", cream, (0, -0.22, 0.36), (0.16, 0.20, 0.14))
    for sx, y, z in ((-0.14, -0.15, 0.58), (0.16, 0.05, 0.52), (-0.10, 0.25, 0.48),
                     (0.12, 0.40, 0.50), (-0.16, 0.50, 0.42)):
        sphere("Body", spot, (sx, y, z), (0.038, 0.038, 0.032))
    sphere("Head", tan, (0, -0.38, 0.56), (0.17, 0.20, 0.15))
    sphere("Head", cream, (0, -0.52, 0.50), (0.08, 0.08, 0.06))
    sphere("Head", spot, (0, -0.60, 0.51), (0.028, 0.024, 0.024))
    for sx in (-1, 1):
        sphere("Head", spot, (sx * 0.08, -0.46, 0.62), (0.028, 0.022, 0.03))
        cone("Head", tan, (sx * 0.13, -0.30, 0.72), (0.05, 0.035, 0.08),
             rotation=(math.radians(-8), 0, math.radians(sx * 10)))
    for bone, x, y in (("LegFL", -0.12, -0.24), ("LegFR", 0.12, -0.24),
                       ("LegBL", -0.12, 0.28), ("LegBR", 0.12, 0.28)):
        cylinder(bone, tan, (x, y, 0.20), (0.05, 0.05, 0.40))
        sphere(bone, cream, (x, y, 0.03), (0.055, 0.07, 0.025))
    sphere("Tail", tan, (0, 0.45, 0.40), (0.045, 0.045, 0.05))
    for k, r in ((0.62, 0.04), (0.78, 0.035)):
        sphere("Tail", spot, (0, k, 0.34), (r, r, r))

    bones = quad_bones(0.12, -0.24, 0.28, 0.38, ((0, 0.25, 0.42), (0, -0.25, 0.48)), {
        "Head": ("Body", (0, -0.32, 0.52), (0, -0.65, 0.58)),
        "Tail": ("Body", (0, 0.42, 0.40), (0, 0.85, 0.32)),
    })

    def idle(key_rot, key_loc):
        # Tapi dans les herbes : corps très bas, oreilles pivotent.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, -0.05), (20, -0.03), (40, -0.05)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 28, extras)

    def chasse(key_rot, key_loc):
        # Avance furtive, corps rasant le sol, pattes précises.
        for f, dz in ((1, -0.08), (12, -0.06), (24, -0.08)):
            key_loc("Body", f, (0, dz, 0))
        s = math.radians(14)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, down in ((1, 0.15), (24, 0.15)):
            key_rot("Head", f, (down, 0, 0))

    build_creature3("creature85", bones, idle, walk, chasse, "Chasse", cam=0.55)


# =============================================================================
# Créature 86 — Faucon : plané, piqué en chasse.
# =============================================================================
def faucon():
    fresh_scene()
    brown = material("Faucon86Brown", (0.42, 0.30, 0.20))
    cream = material("Faucon86Cream", (0.86, 0.78, 0.60))
    dark = material("Faucon86Dark", (0.08, 0.07, 0.06))

    sphere("Body", brown, (0, 0.05, 0.62), (0.16, 0.24, 0.24))
    sphere("Body", cream, (0, -0.10, 0.56), (0.10, 0.14, 0.16))
    sphere("Head", brown, (0, -0.02, 0.92), (0.11, 0.11, 0.10))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.07, -0.10, 0.96), (0.026, 0.022, 0.026))
    cone("Head", dark, (0, -0.13, 0.86), (0.03, 0.03, 0.07),
         rotation=(math.radians(110), 0, 0))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        for k in range(3):
            t = k / 2.0
            sphere(bone, brown, (sx * (0.24 + 0.24 * t), 0.05 + 0.04 * t, 0.62 - 0.03 * t),
                   (0.09 - 0.02 * t, 0.16 - 0.03 * t, 0.03))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, dark, (sx * 0.07, 0.02, 0.20), (0.02, 0.02, 0.14))
        sphere(bone, dark, (sx * 0.07, -0.05, 0.05), (0.04, 0.06, 0.02))

    bones = {
        "Body": ("Root", (0, 0.30, 0.62), (0, -0.15, 0.68)),
        "Head": ("Body", (0, 0.0, 0.80), (0, -0.05, 0.98)),
        "WingL": ("Body", (-0.20, 0.05, 0.62), (-0.55, 0.15, 0.55)),
        "WingR": ("Body", (0.20, 0.05, 0.62), (0.55, 0.15, 0.55)),
        "LegL": ("Body", (-0.07, 0.02, 0.32), (-0.07, 0.02, 0.02)),
        "LegR": ("Body", (0.07, 0.02, 0.32), (0.07, 0.02, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, a in ((1, 0.0), (10, 0.15), (20, 0.0), (30, 0.15), (40, 0.0)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, yaw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        s = math.radians(14)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, a in ((1, 0.3), (13, 0.4), (24, 0.3)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))

    def piqué(key_rot, key_loc):
        # Piqué en chasse : ailes repliées, plongée puis freinage brutal.
        f0, f_dive, f_brake, f_end = 1, 10, 18, 24
        for f, a in ((f0, 0.0), (f_dive, -0.9), (f_brake, 1.1), (f_end, 0.0)):
            key_rot("WingL", f, (0, a, 0))
            key_rot("WingR", f, (0, -a, 0))
        for f, down in ((f0, 0.0), (f_dive, 0.7), (f_brake, -0.4), (f_end, 0.0)):
            key_rot("Body", f, (down, 0, 0))
        for f, dz in ((f0, 0.0), (f_dive, -0.15), (f_brake, 0.10), (f_end, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature3("creature86", bones, idle, walk, piqué, "Pique", cam=0.6)


# =============================================================================
# Créature 87 — Tapir : trompe courte préhensile, plonge dans la boue.
# =============================================================================
def tapir():
    fresh_scene()
    dark_h = material("Tapir87DarkH", (0.14, 0.13, 0.13))
    cream = material("Tapir87Cream", (0.88, 0.86, 0.82))
    dark = material("Tapir87Dark", (0.05, 0.05, 0.05))

    sphere("Body", dark_h, (0, -0.10, 0.62), (0.30, 0.46, 0.34))
    sphere("Body", cream, (0, 0.42, 0.60), (0.26, 0.24, 0.28))  # arrière-train clair
    sphere("Head", dark_h, (0, -0.72, 0.68), (0.20, 0.22, 0.18))
    neck_bridge("Head", dark_h, (0, -0.72, 0.68), (0, -0.20, 0.62), r=0.20)
    # Trompe préhensile courte, chaîne de segments.
    for k in range(4):
        t = k / 3.0
        sphere("Head", dark_h, (0, -1.00 - 0.16 * t, 0.56 - 0.10 * t),
               (0.09 - 0.02 * t, 0.09, 0.08 - 0.02 * t))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.11, -0.80, 0.80), (0.03, 0.026, 0.03))
        sphere("Head", dark_h, (sx * 0.20, -0.56, 0.86), (0.06, 0.04, 0.07))
    for bone, x, y in (("LegFL", -0.20, -0.32), ("LegFR", 0.20, -0.32),
                       ("LegBL", -0.20, 0.42), ("LegBR", 0.20, 0.42)):
        cylinder(bone, dark_h, (x, y, 0.26), (0.09, 0.09, 0.50))
        cylinder(bone, dark, (x, y, 0.03), (0.10, 0.10, 0.06))
    sphere("Tail", dark_h, (0, 0.80, 0.60), (0.035, 0.035, 0.04))

    bones = quad_bones(0.20, -0.32, 0.42, 0.52, ((0, 0.35, 0.58), (0, -0.35, 0.64)), {
        "Head": ("Body", (0, -0.45, 0.66), (0, -1.10, 0.48)),
        "Tail": ("Body", (0, 0.60, 0.60), (0, 0.80, 0.58)),
    })

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.02), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.0), (12, 0.3), (24, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, sw, 0))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 16, lambda kr: None)

    def plonge(key_rot, key_loc):
        # Plonge tête la première dans la boue, puis émerge en secouant.
        # Chaque frame ne reçoit qu'UN SEUL appel `key_rot("Head", ...)` :
        # deux appels sur la même frame s'écrasent l'un l'autre (piège déjà
        # documenté ailleurs) — ici la 2e passe effaçait le retour à zéro de
        # f_end et rouvrait la boucle Idle→Walk.
        f0, f_dive, f_shake1, f_shake2, f_end = 1, 10, 16, 20, 28
        for f, down, sw in ((f0, 0.0, 0.0), (f_dive, 0.7, 0.0), (f_shake1, 0.3, -0.3),
                            (f_shake2, 0.3, 0.3), (f_end, 0.0, 0.0)):
            key_rot("Head", f, (down, 0, sw))
        for f, dz in ((f0, 0.0), (f_dive, -0.15), (f_shake1, 0.0), (f_end, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature3("creature87", bones, idle, walk, plonge, "Plonge", cam=0.9)


# =============================================================================
# Créature 88 — Wombat : trapu, terrier, coup de tête défensif.
# =============================================================================
def wombat():
    fresh_scene()
    brown = material("Wombat88Brown", (0.44, 0.34, 0.24))
    cream = material("Wombat88Cream", (0.72, 0.62, 0.48))
    dark = material("Wombat88Dark", (0.07, 0.06, 0.06))

    sphere("Body", brown, (0, 0.10, 0.34), (0.28, 0.42, 0.28))
    sphere("Head", brown, (0, -0.42, 0.36), (0.22, 0.22, 0.20))
    neck_bridge("Head", brown, (0, -0.42, 0.36), (0, -0.05, 0.32), r=0.20)
    sphere("Head", cream, (0, -0.60, 0.28), (0.10, 0.09, 0.08))
    sphere("Head", dark, (0, -0.68, 0.27), (0.03, 0.026, 0.026))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.11, -0.44, 0.46), (0.03, 0.026, 0.03))
        sphere("Head", brown, (sx * 0.17, -0.30, 0.52), (0.055, 0.04, 0.06))
    for bone, x, y in (("LegFL", -0.18, -0.24), ("LegFR", 0.18, -0.24),
                       ("LegBL", -0.18, 0.28), ("LegBR", 0.18, 0.28)):
        cylinder(bone, brown, (x, y, 0.16), (0.08, 0.08, 0.20))
        sphere(bone, dark, (x, y, 0.03), (0.085, 0.10, 0.03))

    bones = {
        "Body": ("Root", (0, 0.28, 0.32), (0, -0.14, 0.36)),
        "Head": ("Body", (0, -0.14, 0.36), (0, -0.62, 0.32)),
        "LegFL": ("Body", (-0.18, -0.24, 0.24), (-0.18, -0.24, 0.02)),
        "LegFR": ("Body", (0.18, -0.24, 0.24), (0.18, -0.24, 0.02)),
        "LegBL": ("Body", (-0.18, 0.28, 0.24), (-0.18, 0.28, 0.02)),
        "LegBR": ("Body", (0.18, 0.28, 0.24), (0.18, 0.28, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.015), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.0), (14, 0.15), (28, 0.0), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 16, lambda kr: None)

    def defense(key_rot, key_loc):
        # Coup de tête défensif : recule puis frappe du crâne blindé.
        f0, f_pull, f_strike, f_end = 1, 8, 14, 22
        for f, dy in ((f0, 0.0), (f_pull, 0.10), (f_strike, -0.14), (f_end, 0.0)):
            key_loc("Body", f, (0, dy, 0))
        for f, down in ((f0, 0.0), (f_pull, -0.2), (f_strike, 0.3), (f_end, 0.0)):
            key_rot("Head", f, (down, 0, 0))

    build_creature3("creature88", bones, idle, walk, defense, "Defense", cam=0.55)


# =============================================================================
# Créature 89 — Gecko : ventouses aux doigts, grimpe au mur (Idle vertical).
# =============================================================================
def gecko():
    fresh_scene()
    green = material("Gecko89Green", (0.42, 0.68, 0.30))
    cream = material("Gecko89Cream", (0.86, 0.90, 0.72))
    dark = material("Gecko89Dark", (0.07, 0.07, 0.06))

    sphere("Body", green, (0, 0.05, 0.16), (0.14, 0.30, 0.11))
    sphere("Head", green, (0, -0.34, 0.18), (0.13, 0.15, 0.10))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.36, 0.26), (0.035, 0.03, 0.035))
    sphere("Head", cream, (0, -0.46, 0.14), (0.06, 0.06, 0.04))
    for bone, x, y in (("LegFL", -0.15, -0.20), ("LegFR", 0.15, -0.20),
                       ("LegBL", -0.15, 0.24), ("LegBR", 0.15, 0.24)):
        cylinder(bone, green, (x, y, 0.10), (0.03, 0.03, 0.10))
        for k in range(4):  # doigts écartés en ventouse
            a = math.radians(-40 + k * 26)
            sphere(bone, cream, (x + 0.05 * math.sin(a), y + 0.05 * math.cos(a), 0.01),
                   (0.018, 0.018, 0.01))
    for k in range(6):  # longue queue effilée
        t = k / 5.0
        sphere("Tail", green, (0, 0.35 + 0.28 * t, 0.16 - 0.05 * t),
               (0.07 - 0.045 * t, 0.14, 0.07 - 0.045 * t))

    bones = quad_bones(0.15, -0.20, 0.24, 0.14, ((0, 0.25, 0.16), (0, -0.25, 0.17)), {
        "Head": ("Body", (0, -0.24, 0.17), (0, -0.48, 0.16)),
        "Tail": ("Body", (0, 0.35, 0.15), (0, 1.00, 0.05)),
    })

    def idle(key_rot, key_loc):
        # Figé au mur, seule la gorge palpite, queue s'enroule doucement.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, sw in ((1, 0.0), (20, 0.3), (40, 0.0)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.4), (13, -0.4), (24, 0.4)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 30, extras)

    def bascule(key_rot, key_loc):
        # Détale en rafale : foulée très rapide et basse.
        s = math.radians(45)
        for f, a in ((1, s), (7, -s), (14, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, sw in ((1, 0.5), (7, -0.5), (14, 0.5)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature3("creature89", bones, idle, walk, bascule, "Detale", cam=0.35)


# =============================================================================
# Créature 90 — Mangouste : dressée en vigie, esquive le serpent.
# =============================================================================
def mangouste():
    fresh_scene()
    fur = material("Mangouste90Fur", (0.58, 0.52, 0.38))
    cream = material("Mangouste90Cream", (0.82, 0.78, 0.64))
    dark = material("Mangouste90Dark", (0.07, 0.06, 0.06))

    sphere("Body", fur, (0, 0.05, 0.24), (0.11, 0.32, 0.11))
    sphere("Head", fur, (0, -0.30, 0.28), (0.09, 0.10, 0.08))
    sphere("Head", cream, (0, -0.40, 0.24), (0.045, 0.05, 0.04))
    sphere("Head", dark, (0, -0.46, 0.23), (0.018, 0.015, 0.015))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.05, -0.32, 0.34), (0.02, 0.017, 0.02))
        sphere("Head", fur, (sx * 0.07, -0.24, 0.38), (0.028, 0.02, 0.03))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        sphere(bone, fur, (sx * 0.06, 0.02, 0.14), (0.03, 0.10, 0.09))
        cylinder(bone, fur, (sx * 0.05, -0.10, 0.10), (0.018, 0.018, 0.09))
    for k in range(6):  # queue touffue longue
        t = k / 5.0
        sphere("Tail", fur, (0, 0.28 + 0.26 * t, 0.22 - 0.03 * t),
               (0.075 - 0.02 * t, 0.13, 0.075 - 0.02 * t))

    bones = {
        "Body": ("Root", (0, 0.20, 0.22), (0, -0.14, 0.25)),
        "Head": ("Body", (0, -0.14, 0.26), (0, -0.42, 0.28)),
        "LegL": ("Body", (-0.06, 0.05, 0.16), (-0.06, 0.05, 0.02)),
        "LegR": ("Body", (0.06, 0.05, 0.16), (0.06, 0.05, 0.02)),
        "Tail": ("Body", (0, 0.24, 0.22), (0, 0.80, 0.14)),
    }

    def idle(key_rot, key_loc):
        # Se dresse en vigie sur les pattes arrière, tête pivote en radar.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, up in ((1, 0.0), (10, -0.5), (30, -0.5), (40, 0.0)):
            key_rot("Body", f, (up, 0, 0))
        for f, yaw in ((1, 0.0), (14, 0.35), (28, -0.35), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        s = math.radians(35)
        for f, a in ((1, s), (7, -s), (14, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (a, 0, 0))
        for f, dz in ((1, -0.02), (4, 0.08), (7, -0.02), (11, 0.08), (14, -0.02)):
            key_loc("Body", f, (0, dz, 0))

    def esquive(key_rot, key_loc):
        # Esquive vive de côté puis riposte, corps qui zigzague.
        for f, sway in ((1, 0.0), (6, 0.5), (12, -0.5), (18, 0.3), (24, 0.0)):
            key_rot("Body", f, (0, 0, sway))
        for f, jump in ((1, 0.0), (6, 0.08), (12, 0.0), (18, 0.06), (24, 0.0)):
            key_loc("Body", f, (0, 0, jump))

    build_creature3("creature90", bones, idle, walk, esquive, "Esquive", cam=0.35)


# =============================================================================
# Créature 91 — Raton laveur : masque facial, lave sa nourriture.
# =============================================================================
def raton_laveur():
    fresh_scene()
    grey = material("Raton91Grey", (0.42, 0.40, 0.38))
    dark = material("Raton91Dark", (0.10, 0.09, 0.09))
    cream = material("Raton91Cream", (0.80, 0.78, 0.72))

    sphere("Body", grey, (0, 0.08, 0.32), (0.16, 0.26, 0.18))
    sphere("Head", grey, (0, -0.26, 0.38), (0.13, 0.14, 0.12))
    neck_bridge("Head", grey, (0, -0.26, 0.38), (0, 0.08, 0.32), r=0.13)
    sphere("Head", cream, (0, -0.38, 0.32), (0.07, 0.07, 0.06))
    sphere("Head", dark, (0, -0.44, 0.30), (0.02, 0.018, 0.018))
    # Masque facial sombre autour des yeux.
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.32, 0.42), (0.05, 0.03, 0.045))
        sphere("Head", cream, (sx * 0.09, -0.34, 0.42), (0.022, 0.018, 0.022))
        sphere("Head", grey, (sx * 0.14, -0.16, 0.50), (0.045, 0.03, 0.06))  # oreille
    for bone, sx in (("ArmL", -1), ("ArmR", 1)):
        cylinder(bone, grey, (sx * 0.14, -0.18, 0.24), (0.035, 0.035, 0.16))
        sphere(bone, dark, (sx * 0.14, -0.24, 0.10), (0.04, 0.05, 0.02))  # patte agile
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, grey, (sx * 0.10, 0.16, 0.12), (0.04, 0.04, 0.12))
        sphere(bone, dark, (sx * 0.10, 0.22, 0.02), (0.045, 0.06, 0.02))
    # Queue annelée, anneaux clairs/sombres alternés.
    for k in range(6):
        t = k / 5.0
        sphere("Tail", cream if k % 2 == 0 else dark,
               (0, 0.25 + 0.14 * t, 0.28 + 0.10 * math.sin(t * 2.5)),
               (0.08 - 0.015 * t, 0.09, 0.08 - 0.015 * t))

    bones = {
        "Body": ("Root", (0, 0.18, 0.28), (0, -0.10, 0.32)),
        "Head": ("Body", (0, -0.10, 0.34), (0, -0.42, 0.38)),
        "ArmL": ("Body", (-0.14, -0.18, 0.30), (-0.14, -0.24, 0.06)),
        "ArmR": ("Body", (0.14, -0.18, 0.30), (0.14, -0.24, 0.06)),
        "LegL": ("Body", (-0.10, 0.16, 0.18), (-0.10, 0.22, 0.02)),
        "LegR": ("Body", (0.10, 0.16, 0.18), (0.10, 0.22, 0.02)),
        "Tail": ("Body", (0, 0.22, 0.26), (0, 0.65, 0.55)),
    }

    def idle(key_rot, key_loc):
        # Lave sa nourriture : les deux « mains » frottent l'une contre
        # l'autre, la tête se penche pour regarder.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, a in ((1, 0.0), (8, 0.4), (16, -0.4), (24, 0.4), (32, 0.0), (40, 0.0)):
            key_rot("ArmL", f, (a, 0, 0))
            key_rot("ArmR", f, (-a, 0, 0))
        for f, nod in ((1, 0.0), (16, 0.2), (32, 0.0), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        s = math.radians(24)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
            key_rot("ArmL", f, (-a, 0, 0))
            key_rot("ArmR", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    def furète(key_rot, key_loc):
        # Fouille avidement de ses deux pattes avant, tête plongeante.
        for f, a in ((1, -0.3), (6, 0.5), (12, -0.3), (18, 0.5), (24, -0.3)):
            key_rot("ArmL", f, (a, 0, 0))
            key_rot("ArmR", f, (a, 0, 0))
        for f, down in ((1, 0.3), (12, 0.5), (24, 0.3)):
            key_rot("Head", f, (down, 0, 0))

    build_creature3("creature91", bones, idle, walk, furète, "Furete", cam=0.4)


# =============================================================================
# Créature 92 — Cigogne : long bec claquant, niche sur un pied.
# =============================================================================
def cigogne():
    fresh_scene()
    white = material("Cigogne92White", (0.94, 0.93, 0.90))
    black = material("Cigogne92Black", (0.10, 0.10, 0.11))
    red = material("Cigogne92Red", (0.82, 0.24, 0.16))

    sphere("Body", white, (0, 0.10, 1.05), (0.20, 0.30, 0.26))
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, black, (sx * 0.22, 0.10, 1.00), (0.08, 0.24, 0.20))
    for k in range(5):  # long cou fin
        t = k / 4.0
        sphere("Neck", white, (0, -0.20 - 0.28 * t, 1.20 + 0.42 * t),
               (0.075 - 0.01 * t, 0.10, 0.075 - 0.01 * t))
    sphere("Head", white, (0, -0.62, 1.66), (0.08, 0.09, 0.07))
    cone("Head", red, (0, -0.78, 1.62), (0.025, 0.025, 0.24),
         rotation=(math.radians(100), 0, 0))  # long bec droit
    for sx in (-1, 1):
        sphere("Head", black, (sx * 0.045, -0.62, 1.72), (0.014, 0.012, 0.014))
    cylinder("LegL", red, (0, 0.08, 0.55), (0.03, 0.03, 0.55))
    sphere("LegL", red, (0, 0.13, 0.03), (0.05, 0.09, 0.02))

    bones = {
        "Body": ("Root", (0, 0.30, 1.00), (0, -0.10, 1.10)),
        "WingL": ("Body", (-0.18, 0.10, 1.10), (-0.42, 0.10, 0.85)),
        "WingR": ("Body", (0.18, 0.10, 1.10), (0.42, 0.10, 0.85)),
        "Neck": ("Body", (0, -0.10, 1.15), (0, -0.55, 1.60)),
        "Head": ("Neck", (0, -0.55, 1.60), (0, -0.80, 1.65)),
        "LegL": ("Body", (0, 0.08, 1.00), (0, 0.08, 0.02)),
    }

    def idle(key_rot, key_loc):
        for f in (1, 40):
            key_rot("LegL", f, (0, 0, 0))
        for f, sw in ((1, 0.0), (14, 0.15), (28, -0.15), (40, 0.0)):
            key_rot("Neck", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        s = math.radians(16)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.04), (13, 0.0), (19, 0.04), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Neck", f, (nod, 0, 0))

    def claque(key_rot, key_loc):
        # Claquement de bec rituel : la tête part en arrière puis frappe
        # vite d'avant en arrière plusieurs fois (parade nuptiale).
        for f, curl in ((1, 0.0), (4, -0.5), (7, 0.4), (10, -0.5), (13, 0.4),
                        (16, -0.5), (19, 0.4), (24, 0.0)):
            key_rot("Neck", f, (curl, 0, 0))

    build_creature3("creature92", bones, idle, walk, claque, "Claque", cam=1.2)


mouflon()
lama()
ocelot()
faucon()
tapir()
wombat()
gecko()
mangouste()
raton_laveur()
cigogne()
print("PACK DONE")
