"""Génère assets/models/creature42.glb … creature51.glb : 10 animaux d'Asie.

Pack « faune d'Asie » — panda géant, tigre, grue du Japon, macaque, buffle
d'eau, panda roux, cobra royal, carpe koï, paon, chameau de Bactriane. Que
des animaux réels, chacun avec une animation signature (mastication de
bambou, danse de la grue, balancement hypnotique du cobra, roue du paon…).
Mêmes conventions que les packs 21/22-26/32-36/37-41 :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/… par créature, mesh unique skinné (1 os / partie, poids 1.0) ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables, chaque clip
  keyframe tous les os animés par l'autre (piège glTF : canaux absents = os figé) ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur) ;
- échelle appliquée AVANT la rotation (piège rotation/scale des cônes) ;
- AUCUN vertex sous z=0 + marge 0,02 (gel par TriMesh incrusté, cf. mémoire
  et commentaire Créature 24 dans scene/demos.rs) ;
- pose remise au neutre avant export ET avant la vignette ; vignette avec
  soleil d'appoint (les teintes sombres se fondent sinon dans le fond noir).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack42_51.py
"""

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

PARTS = []


def material(name, rgb, roughness=0.8):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    return m


def add_part(bone, mat, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    create_op(location=location)
    ob = bpy.context.active_object
    ob.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    ob.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    ob.data.materials.append(mat)
    vg = ob.vertex_groups.new(name=bone)
    vg.add(range(len(ob.data.vertices)), 1.0, "REPLACE")
    PARTS.append(ob)
    return ob


def sphere(bone, mat, location, scale, segments=24, rings=16):
    def op(location):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=segments, ring_count=rings, radius=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale)


def cone(bone, mat, location, scale, rotation=(0, 0, 0)):
    def op(location):
        bpy.ops.mesh.primitive_cone_add(
            vertices=16, radius1=1.0, radius2=0.0, depth=2.0, location=location
        )

    return add_part(bone, mat, op, location, scale, rotation)


def cylinder(bone, mat, location, scale, rotation=(0, 0, 0)):
    def op(location):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=16, radius=1.0, depth=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale, rotation)


def build_creature(name, bones, idle_keys, walk_keys, cam=1.0):
    """Fusionne PARTS, pose l'armature `bones`, bake Idle/Walk, exporte + vignette."""
    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = PARTS[0]
    bpy.ops.object.join()
    creature = bpy.context.active_object
    creature.name = name.capitalize()

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = f"{creature.name}Rig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.35))
    for bname, (parent, head, tail) in bones.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    creature.parent = arm
    creature.modifiers.new("Armature", "ARMATURE").object = arm

    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
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
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    bpy.ops.object.mode_set(mode="OBJECT")

    out = os.path.join(OUT_DIR, f"{name}.glb")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=out,
        export_format="GLB",
        export_skins=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_force_sampling=True,
        export_yup=True,
    )
    print("EXPORTED", out)

    # Vignette : pistes NLA purgées + pose neutre (piège : l'exporteur laisse
    # l'armature posée au dernier frame évalué).
    ad = arm.animation_data
    ad.action = None
    for t in list(ad.nla_tracks):
        ad.nla_tracks.remove(t)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    scene = bpy.context.scene
    scene.frame_set(1)
    bpy.context.view_layer.update()
    bpy.ops.object.camera_add(
        location=(5.2 * cam, -7.0 * cam, 3.6 * cam),
        rotation=(math.radians(74), 0, math.radians(37)),
    )
    scene.camera = bpy.context.active_object
    bpy.ops.object.light_add(
        type="SUN", location=(2, -3, 6),
        rotation=(math.radians(35), math.radians(20), 0),
    )
    bpy.context.active_object.data.energy = 3.0
    bpy.ops.object.light_add(
        type="SUN", location=(-3, 2, 4),
        rotation=(math.radians(55), math.radians(-30), 0),
    )
    bpy.context.active_object.data.energy = 1.6
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = out.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)


def fresh_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.render.fps = 24
    PARTS.clear()


LEGS4 = ("LegFL", "LegFR", "LegBL", "LegBR")


def quad_bones(x, yf, yb, top, body, extra):
    """Squelette quadrupède standard : Body + 4 pattes verticales + extras."""
    bones = {"Body": ("Root", body[0], body[1])}
    bones.update(extra)
    for bname, sx, y in (("LegFL", -1, yf), ("LegFR", 1, yf),
                         ("LegBL", -1, yb), ("LegBR", 1, yb)):
        bones[bname] = ("Body", (sx * x, y, top), (sx * x, y, 0.02))
    return bones


def quad_walk_keys(key_rot, key_loc, swing, extras):
    """Marche diagonale standard + bob du corps ; `extras(key_rot)` par pack."""
    s = math.radians(swing)
    for f, a in ((1, s), (13, -s), (24, s)):
        key_rot("LegFL", f, (a, 0, 0))
        key_rot("LegBR", f, (a, 0, 0))
        key_rot("LegFR", f, (-a, 0, 0))
        key_rot("LegBL", f, (-a, 0, 0))
    for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
        key_loc("Body", f, (0, dz, 0))
    extras(key_rot)


# =============================================================================
# Créature 42 — Panda géant : masque noir, mastication de bambou en Idle.
# =============================================================================
def panda():
    fresh_scene()
    white = material("Panda42White", (0.90, 0.88, 0.84))
    black = material("Panda42Black", (0.10, 0.10, 0.11))

    sphere("Body", white, (0, 0.10, 1.00), (0.72, 0.92, 0.65))
    sphere("Body", black, (0, -0.42, 1.05), (0.68, 0.30, 0.52))  # bande d'épaules
    sphere("Head", white, (0, -0.85, 1.45), (0.48, 0.45, 0.42))
    for sx in (-1, 1):
        sphere("Head", black, (sx * 0.30, -0.72, 1.82), (0.14, 0.10, 0.14))  # oreille
        sphere("Head", black, (sx * 0.20, -1.22, 1.52), (0.10, 0.06, 0.13))  # tache
        sphere("Head", black, (sx * 0.20, -1.27, 1.52), (0.05, 0.035, 0.06))  # œil
    sphere("Head", black, (0, -1.30, 1.36), (0.07, 0.06, 0.05))  # truffe
    for bone, x, y in (("LegFL", -0.42, -0.45), ("LegFR", 0.42, -0.45),
                       ("LegBL", -0.42, 0.60), ("LegBR", 0.42, 0.60)):
        cylinder(bone, black, (x, y, 0.47), (0.19, 0.19, 0.88))
    sphere("Tail", white, (0, 0.98, 1.00), (0.15, 0.14, 0.14))

    bones = quad_bones(0.42, -0.45, 0.60, 0.90, ((0, 0.55, 0.95), (0, -0.50, 1.05)), {
        "Head": ("Body", (0, -0.65, 1.30), (0, -1.30, 1.55)),
        "Tail": ("Body", (0, 0.90, 1.00), (0, 1.15, 1.00)),
    })

    def idle(key_rot, key_loc):
        # Mastication de bambou : petits hochements rapides du museau, entre
        # deux « bouchées » la tête se penche chercher la tige suivante.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.05), (5, 0.14), (9, 0.05), (13, 0.14), (17, 0.05),
                       (24, 0.30), (32, 0.05), (40, 0.05)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.20), (20, -0.20), (40, 0.20)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 20, extras)

    build_creature("creature42", bones, idle, walk, cam=1.0)


# =============================================================================
# Créature 43 — Tigre : rayures sur le dos, fouet de la queue, foulée féline.
# =============================================================================
def tigre():
    fresh_scene()
    orange = material("Tigre43Orange", (0.82, 0.44, 0.12))
    cream = material("Tigre43Cream", (0.92, 0.86, 0.72))
    black = material("Tigre43Black", (0.09, 0.08, 0.08))

    sphere("Body", orange, (0, 0.10, 0.95), (0.55, 1.02, 0.50))
    sphere("Body", cream, (0, 0.10, 0.72), (0.45, 0.88, 0.34))  # ventre
    for y in (-0.55, -0.20, 0.15, 0.50):  # rayures drapées sur le dos
        sphere("Body", black, (0, y, 1.32), (0.50, 0.07, 0.16))
    sphere("Head", orange, (0, -0.95, 1.30), (0.42, 0.40, 0.36))
    sphere("Head", cream, (0, -1.28, 1.20), (0.20, 0.17, 0.15))  # museau
    sphere("Head", black, (0, -1.42, 1.26), (0.06, 0.05, 0.05))  # truffe
    for sx in (-1, 1):
        sphere("Head", black, (sx * 0.18, -1.28, 1.42), (0.055, 0.04, 0.06))  # œil
        sphere("Head", orange, (sx * 0.26, -0.78, 1.62), (0.12, 0.08, 0.13))  # oreille
        sphere("Head", black, (sx * 0.34, -1.12, 1.18), (0.05, 0.10, 0.03))  # rayure joue
    for bone, x, y in (("LegFL", -0.36, -0.55), ("LegFR", 0.36, -0.55),
                       ("LegBL", -0.36, 0.68), ("LegBR", 0.36, 0.68)):
        cylinder(bone, orange, (x, y, 0.42), (0.15, 0.15, 0.80))
        sphere(bone, cream, (x, y - 0.06, 0.10), (0.16, 0.20, 0.09))  # patte
    for y, z, s in ((0.98, 1.05, 0.11), (1.28, 1.15, 0.09)):  # queue
        sphere("Tail", orange, (0, y, z), (s, s * 1.6, s))
    sphere("Tail", black, (0, 1.55, 1.22), (0.09, 0.12, 0.09))  # pointe

    bones = quad_bones(0.36, -0.55, 0.68, 0.80, ((0, 0.55, 0.95), (0, -0.60, 1.00)), {
        "Head": ("Body", (0, -0.72, 1.20), (0, -1.45, 1.30)),
        "Tail": ("Body", (0, 0.90, 1.00), (0, 1.60, 1.25)),
    })

    def idle(key_rot, key_loc):
        # Affût : la queue fouette large, la tête balaie lentement le terrain.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.45), (14, -0.45), (27, 0.45), (40, 0.45)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.20), (20, 0.20), (40, -0.20)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, roll in ((1, 0.06), (13, -0.06), (24, 0.06)):
                kr("Body", f, (0, roll, 0))
            for f, sw in ((1, -0.25), (13, 0.25), (24, -0.25)):
                kr("Tail", f, (0, 0, sw))
            for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
                kr("Head", f, (nod, 0, 0))
        quad_walk_keys(key_rot, key_loc, 26, extras)

    build_creature("creature43", bones, idle, walk, cam=1.0)


# =============================================================================
# Créature 44 — Grue du Japon : couronne rouge, danse ailes ouvertes.
# =============================================================================
def grue():
    fresh_scene()
    white = material("Grue44White", (0.92, 0.92, 0.90))
    black = material("Grue44Black", (0.10, 0.10, 0.11))
    red = material("Grue44Red", (0.82, 0.12, 0.10))
    beak = material("Grue44Beak", (0.65, 0.58, 0.35))

    sphere("Body", white, (0, 0.15, 1.15), (0.42, 0.55, 0.38))
    sphere("Body", black, (0, 0.62, 1.18), (0.26, 0.30, 0.20))  # plumes caudales
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, white, (sx * 0.36, 0.15, 1.25), (0.12, 0.45, 0.24))
        sphere(bone, black, (sx * 0.40, 0.48, 1.22), (0.09, 0.18, 0.14))  # rémiges
    # Cou noir en chaîne de sphères, tête blanche, couronne rouge, bec.
    for y, z, r in ((-0.32, 1.35, 0.10), (-0.44, 1.65, 0.09), (-0.52, 1.95, 0.085),
                    (-0.58, 2.18, 0.08)):
        sphere("Neck", black, (0, y, z), (r, r, r * 1.3))
    sphere("Head", white, (0, -0.62, 2.35), (0.11, 0.14, 0.10))
    sphere("Head", red, (0, -0.56, 2.45), (0.06, 0.07, 0.035))
    for sx in (-1, 1):
        sphere("Head", black, (sx * 0.065, -0.70, 2.38), (0.03, 0.025, 0.03))
    cone("Head", beak, (0, -0.82, 2.32), (0.035, 0.035, 0.14),
         rotation=(math.radians(97), 0, 0))
    # Deux échasses fines + doigts.
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, black, (sx * 0.13, 0.12, 0.42), (0.045, 0.045, 0.84))
        cone(bone, black, (sx * 0.13, -0.02, 0.05), (0.06, 0.04, 0.10),
             rotation=(math.radians(100), 0, 0))

    bones = {
        "Body": ("Root", (0, 0.40, 1.15), (0, -0.25, 1.22)),
        "Neck": ("Body", (0, -0.25, 1.28), (0, -0.58, 2.25)),
        "Head": ("Neck", (0, -0.58, 2.25), (0, -0.85, 2.38)),
        "WingL": ("Body", (-0.25, 0.15, 1.25), (-0.55, 0.35, 1.25)),
        "WingR": ("Body", (0.25, 0.15, 1.25), (0.55, 0.35, 1.25)),
        "LegL": ("Body", (-0.13, 0.12, 0.85), (-0.13, 0.12, 0.02)),
        "LegR": ("Body", (0.13, 0.12, 0.85), (0.13, 0.12, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Danse nuptiale : les ailes s'ouvrent en éventail, le cou dessine une
        # révérence, la tête se redresse fièrement — la parade du tancho.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, a in ((1, 0.0), (12, 0.85), (22, 0.85), (32, 0.0), (40, 0.0)):
            key_rot("WingL", f, (0, -a, 0))
            key_rot("WingR", f, (0, a, 0))
        for f, dip in ((1, 0.0), (12, 0.30), (22, -0.15), (32, 0.0), (40, 0.0)):
            key_rot("Neck", f, (dip, 0, 0))
        for f, nod in ((1, 0.0), (12, -0.20), (22, 0.15), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, dz in ((1, 0.0), (12, 0.06), (22, 0.02), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    def walk(key_rot, key_loc):
        # Pas d'échassier : grandes enjambées lentes, le cou pompe, ailes pliées.
        s = math.radians(32)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.06), (13, 0.0), (19, 0.06), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, pump in ((1, 0.08), (13, -0.06), (24, 0.08)):
            key_rot("Neck", f, (pump, 0, 0))
        for f, nod in ((1, -0.05), (13, 0.05), (24, -0.05)):
            key_rot("Head", f, (nod, 0, 0))
        for f in (1, 24):
            key_rot("WingL", f, (0, 0, 0))
            key_rot("WingR", f, (0, 0, 0))

    build_creature("creature44", bones, idle, walk, cam=1.15)


# =============================================================================
# Créature 45 — Macaque japonais : face rouge, se gratte la tête en Idle.
# =============================================================================
def macaque():
    fresh_scene()
    fur = material("Macaque45Fur", (0.62, 0.55, 0.45))
    fur_d = material("Macaque45FurD", (0.45, 0.38, 0.30))
    red = material("Macaque45Red", (0.72, 0.28, 0.22))
    dark = material("Macaque45Dark", (0.08, 0.07, 0.06))

    sphere("Body", fur, (0, 0.08, 0.85), (0.50, 0.52, 0.55))
    sphere("Body", fur_d, (0, -0.25, 0.75), (0.36, 0.28, 0.38))  # poitrail
    sphere("Head", fur, (0, -0.30, 1.42), (0.32, 0.32, 0.30))
    sphere("Head", red, (0, -0.55, 1.38), (0.17, 0.13, 0.19))  # face rouge
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.09, -0.64, 1.46), (0.04, 0.03, 0.045))
        sphere("Head", fur, (sx * 0.28, -0.28, 1.50), (0.09, 0.06, 0.10))  # oreille
    sphere("Head", red, (0, -0.66, 1.30), (0.06, 0.05, 0.045))  # museau
    for bone, sx in (("ArmL", -1), ("ArmR", 1)):
        cylinder(bone, fur, (sx * 0.52, -0.12, 0.72), (0.10, 0.10, 0.85),
                 rotation=(0, math.radians(sx * 8), 0))
        sphere(bone, fur_d, (sx * 0.58, -0.18, 0.24), (0.13, 0.15, 0.11))  # main
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, fur, (sx * 0.26, 0.20, 0.30), (0.13, 0.13, 0.55))
        sphere(bone, fur_d, (sx * 0.26, 0.10, 0.09), (0.15, 0.20, 0.08))  # pied
    sphere("Tail", fur, (0, 0.55, 0.90), (0.09, 0.18, 0.09))

    bones = {
        "Body": ("Root", (0, 0.35, 0.85), (0, -0.25, 0.95)),
        "Head": ("Body", (0, -0.15, 1.20), (0, -0.60, 1.50)),
        "ArmL": ("Body", (-0.48, -0.10, 1.10), (-0.60, -0.20, 0.15)),
        "ArmR": ("Body", (0.48, -0.10, 1.10), (0.60, -0.20, 0.15)),
        "LegL": ("Body", (-0.26, 0.20, 0.55), (-0.26, 0.20, 0.02)),
        "LegR": ("Body", (0.26, 0.20, 0.55), (0.26, 0.20, 0.02)),
        "Tail": ("Body", (0, 0.48, 0.90), (0, 0.72, 0.92)),
    }

    def idle(key_rot, key_loc):
        # Toilette : le bras droit remonte se gratter la tête (l'os pivote,
        # la main passe près de l'oreille), la tête se penche complice.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, a in ((1, 0.0), (10, -2.2), (16, -1.9), (22, -2.2), (30, 0.0), (40, 0.0)):
            key_rot("ArmR", f, (a, 0, 0.3 if a != 0.0 else 0.0))
        for f, a in ((1, 0.08), (20, -0.06), (40, 0.08)):
            key_rot("ArmL", f, (a, 0, 0))
        for f, tilt in ((1, 0.0), (10, 0.25), (22, 0.25), (30, 0.0), (40, 0.0)):
            key_rot("Head", f, (0, tilt, 0))
        for f, sw in ((1, 0.2), (20, -0.2), (40, 0.2)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Marche sur les phalanges, bras opposés aux jambes, queue balancier.
        s = math.radians(24)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
            key_rot("ArmL", f, (-a, 0, 0))
            key_rot("ArmR", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.25), (13, -0.25), (24, 0.25)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature("creature45", bones, idle, walk, cam=0.9)


# =============================================================================
# Créature 46 — Buffle d'eau : cornes en croissant, masse placide.
# =============================================================================
def buffle():
    fresh_scene()
    slate = material("Buffle46Slate", (0.30, 0.32, 0.36))
    slate_d = material("Buffle46SlateD", (0.20, 0.21, 0.25))
    horn = material("Buffle46Horn", (0.72, 0.66, 0.52))
    dark = material("Buffle46Dark", (0.06, 0.06, 0.07))

    sphere("Body", slate, (0, 0.12, 1.10), (0.80, 1.08, 0.70))
    sphere("Head", slate, (0, -1.05, 1.00), (0.45, 0.50, 0.40))
    sphere("Head", slate_d, (0, -1.42, 0.90), (0.24, 0.20, 0.18))  # mufle
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.22, -1.38, 1.12), (0.055, 0.045, 0.06))
        sphere("Head", slate_d, (sx * 0.42, -0.95, 1.18), (0.12, 0.08, 0.10))  # oreille
        # Corne en croissant : chaîne de sphères qui arque dehors-arrière-haut
        # (plus fiable que des cônes composés, cf. l'hélice ratée du 1er essai),
        # pointe en cône vers le haut-arrière.
        for hx, hy, hz, hr in ((0.44, -0.90, 1.32, 0.095), (0.62, -0.84, 1.42, 0.08),
                               (0.76, -0.72, 1.52, 0.065)):
            sphere("Head", horn, (sx * hx, hy, hz), (hr, hr, hr))
        cone("Head", horn, (sx * 0.84, -0.60, 1.62), (0.05, 0.05, 0.14),
             rotation=(math.radians(-38), 0, math.radians(sx * 18)))
    for bone, x, y in (("LegFL", -0.45, -0.60), ("LegFR", 0.45, -0.60),
                       ("LegBL", -0.45, 0.72), ("LegBR", 0.45, 0.72)):
        cylinder(bone, slate_d, (x, y, 0.45), (0.17, 0.17, 0.86))
        cylinder(bone, dark, (x, y, 0.09), (0.18, 0.18, 0.12))  # sabot
    cone("Tail", slate, (0, 1.28, 1.15), (0.06, 0.06, 0.38),
         rotation=(math.radians(-160), 0, 0))
    sphere("Tail", dark, (0, 1.42, 0.85), (0.09, 0.09, 0.13))

    bones = quad_bones(0.45, -0.60, 0.72, 0.88, ((0, 0.55, 1.05), (0, -0.60, 1.10)), {
        "Head": ("Body", (0, -0.80, 1.05), (0, -1.50, 0.92)),
        "Tail": ("Body", (0, 1.18, 1.15), (0, 1.48, 0.80)),
    })

    def idle(key_rot, key_loc):
        # Placidité de rizière : la tête broute bas et balaie, la queue chasse
        # les mouches à contretemps.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod, yaw in ((1, 0.15, -0.12), (14, 0.22, 0.10), (27, 0.15, -0.12),
                            (40, 0.15, -0.12)):
            key_rot("Head", f, (nod, 0, yaw))
        for f, sw in ((1, -0.4), (12, 0.4), (26, -0.4), (40, -0.4)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.08), (13, -0.02), (24, 0.08)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.2), (13, -0.2), (24, 0.2)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 17, extras)

    build_creature("creature46", bones, idle, walk, cam=1.05)


# =============================================================================
# Créature 47 — Panda roux : queue annelée touffue, port de tête espiègle.
# =============================================================================
def panda_roux():
    fresh_scene()
    rust = material("PandaRoux47Rust", (0.68, 0.28, 0.10))
    cream = material("PandaRoux47Cream", (0.90, 0.84, 0.72))
    dark = material("PandaRoux47Dark", (0.15, 0.10, 0.08))

    sphere("Body", rust, (0, 0.08, 0.72), (0.42, 0.62, 0.40))
    sphere("Head", rust, (0, -0.55, 1.02), (0.30, 0.27, 0.25))
    sphere("Head", cream, (0, -0.75, 0.98), (0.15, 0.12, 0.14))  # museau clair
    sphere("Head", dark, (0, -0.86, 1.00), (0.05, 0.04, 0.04))  # truffe
    for sx in (-1, 1):
        sphere("Head", cream, (sx * 0.24, -0.42, 1.22), (0.09, 0.06, 0.10))  # oreille
        sphere("Head", cream, (sx * 0.13, -0.72, 1.10), (0.06, 0.04, 0.07))  # sourcil
        sphere("Head", dark, (sx * 0.12, -0.78, 1.08), (0.04, 0.03, 0.045))  # œil
    for bone, x, y in (("LegFL", -0.26, -0.32), ("LegFR", 0.26, -0.32),
                       ("LegBL", -0.26, 0.42), ("LegBR", 0.26, 0.42)):
        cylinder(bone, dark, (x, y, 0.28), (0.11, 0.11, 0.52))
    # Queue touffue annelée : anneaux roux/crème alternés vers l'arrière-haut.
    for i, (y, z) in enumerate(((0.58, 0.80), (0.80, 0.90), (1.02, 0.98),
                                (1.22, 1.04), (1.40, 1.08))):
        mat = rust if i % 2 == 0 else cream
        r = 0.15 - i * 0.012
        sphere("Tail", mat, (0, y, z), (r, r * 1.1, r))

    bones = quad_bones(0.26, -0.32, 0.42, 0.52, ((0, 0.35, 0.70), (0, -0.35, 0.78)), {
        "Head": ("Body", (0, -0.40, 0.90), (0, -0.90, 1.05)),
        "Tail": ("Body", (0, 0.50, 0.78), (0, 1.48, 1.10)),
    })

    def idle(key_rot, key_loc):
        # Curiosité : la tête penche d'un côté puis de l'autre, la queue
        # annelée s'enroule et se déroule lentement.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, tilt in ((1, 0.25), (14, -0.25), (27, 0.25), (40, 0.25)):
            key_rot("Head", f, (0, tilt, 0))
        for f, curl in ((1, 0.35), (20, -0.30), (40, 0.35)):
            key_rot("Tail", f, (curl * 0.4, 0, curl))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, sw in ((1, 0.3), (13, -0.3), (24, 0.3)):
                kr("Tail", f, (0.1, 0, sw))
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                kr("Head", f, (nod, 0, 0))
        quad_walk_keys(key_rot, key_loc, 24, extras)

    build_creature("creature47", bones, idle, walk, cam=0.8)


# =============================================================================
# Créature 48 — Cobra royal : dressé sur ses anneaux, capuchon déployé.
# =============================================================================
def cobra():
    fresh_scene()
    olive = material("Cobra48Olive", (0.36, 0.38, 0.16))
    olive_d = material("Cobra48OliveD", (0.24, 0.26, 0.10))
    belly = material("Cobra48Belly", (0.82, 0.76, 0.55))
    dark = material("Cobra48Dark", (0.07, 0.07, 0.05))

    # Anneaux lovés au sol + colonne dressée (os Neck).
    sphere("Body", olive, (0, 0.05, 0.18), (0.46, 0.46, 0.16))
    sphere("Body", olive_d, (0, 0.12, 0.36), (0.34, 0.34, 0.13))
    for y, z, r in ((-0.02, 0.55, 0.13), (-0.08, 0.80, 0.12), (-0.13, 1.05, 0.11),
                    (-0.17, 1.28, 0.10), (-0.20, 1.46, 0.10)):
        sphere("Neck", olive, (0, y, z), (r, r, r * 1.5))
        sphere("Neck", belly, (0, y - 0.06, z - 0.02), (r * 0.6, r * 0.5, r * 1.1))
    # Capuchon déployé + tête + yeux + langue bifide.
    sphere("Head", olive, (0, -0.16, 1.68), (0.30, 0.09, 0.34))  # capuchon
    sphere("Head", belly, (0, -0.22, 1.64), (0.20, 0.06, 0.24))  # avant du capuchon
    sphere("Head", olive_d, (0, -0.28, 1.74), (0.13, 0.16, 0.11))  # tête
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.07, -0.40, 1.78), (0.035, 0.03, 0.04))
        cone("Head", material(f"Cobra48Tongue{sx}", (0.75, 0.15, 0.18)),
             (sx * 0.025, -0.48, 1.68), (0.012, 0.012, 0.07),
             rotation=(math.radians(105), 0, math.radians(sx * 10)))

    bones = {
        "Body": ("Root", (0, 0.30, 0.25), (0, -0.15, 0.35)),
        "Neck": ("Body", (0, 0.0, 0.45), (0, -0.20, 1.55)),
        "Head": ("Neck", (0, -0.20, 1.55), (0, -0.50, 1.78)),
    }

    def idle(key_rot, key_loc):
        # Balancement hypnotique du charmeur : la colonne oscille en huit,
        # la tête reste braquée — c'est le socle qui danse.
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))
        for f, sx_, sy in ((1, 0.28, 0.05), (11, 0.0, 0.14), (21, -0.28, 0.05),
                           (31, 0.0, -0.06), (40, 0.28, 0.05)):
            key_rot("Neck", f, (sy, 0, sx_))
        for f, cx in ((1, -0.18), (21, 0.18), (40, -0.18)):
            key_rot("Head", f, (0.05, 0, cx))

    def walk(key_rot, key_loc):
        # Reptation : le socle ondule (lacet du corps), la colonne reste
        # dressée et absorbe le mouvement, petites pulsations verticales.
        for f, yaw in ((1, 0.25), (13, -0.25), (24, 0.25)):
            key_rot("Body", f, (0, 0, yaw))
        for f, sw in ((1, -0.18), (13, 0.18), (24, -0.18)):
            key_rot("Neck", f, (0.05, 0, sw))
        for f, cx in ((1, 0.10), (13, -0.10), (24, 0.10)):
            key_rot("Head", f, (0, 0, cx))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature("creature48", bones, idle, walk, cam=0.85)


# =============================================================================
# Créature 49 — Carpe koï : robe blanche à taches orange, nage de bassin.
# =============================================================================
def koi():
    fresh_scene()
    white = material("Koi49White", (0.90, 0.88, 0.84), roughness=0.4)
    orange = material("Koi49Orange", (0.88, 0.42, 0.10), roughness=0.4)
    fin = material("Koi49Fin", (0.92, 0.90, 0.86), roughness=0.5)
    dark = material("Koi49Dark", (0.07, 0.06, 0.06))

    # Fuseau flottant (z ~0,55 : habitante des bassins et rivières).
    sphere("Body", white, (0, 0.05, 0.55), (0.30, 0.60, 0.32))
    sphere("Body", orange, (0, -0.15, 0.72), (0.20, 0.24, 0.14))  # tache dorsale
    sphere("Body", orange, (0.16, 0.22, 0.62), (0.13, 0.18, 0.12))  # tache flanc
    sphere("Head", white, (0, -0.52, 0.55), (0.22, 0.26, 0.24))
    sphere("Head", orange, (0, -0.62, 0.68), (0.11, 0.12, 0.08))  # tache de tête
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.14, -0.66, 0.62), (0.04, 0.035, 0.045))
        sphere("Body", fin, (sx * 0.32, -0.15, 0.48), (0.07, 0.18, 0.11))  # pectorale
    cone("Body", fin, (0, 0.12, 0.90), (0.05, 0.16, 0.16))  # dorsale
    sphere("Tail", white, (0, 0.60, 0.55), (0.15, 0.22, 0.17))
    cone("Tail", fin, (0, 0.90, 0.55), (0.05, 0.22, 0.24),
         rotation=(math.radians(-90), 0, 0))  # caudale en éventail

    bones = {
        "Body": ("Root", (0, 0.35, 0.55), (0, -0.35, 0.56)),
        "Head": ("Body", (0, -0.40, 0.55), (0, -0.80, 0.55)),
        "Tail": ("Body", (0, 0.45, 0.55), (0, 1.00, 0.55)),
    }

    def idle(key_rot, key_loc):
        # Nage de bassin : godille douce, la robe ondule à peine — sérénité.
        for f, sw in ((1, 0.25), (20, -0.25), (40, 0.25)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.08), (20, 0.08), (40, -0.08)):
            key_rot("Head", f, (0, 0, yaw))
        for f, roll in ((1, 0.06), (20, -0.06), (40, 0.06)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (20, 0.06), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    def walk(key_rot, key_loc):
        # Coup de nageoire : grands battements de caudale, la tête contre-braque.
        for f, sw in ((1, 0.5), (13, -0.5), (24, 0.5)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.14), (13, 0.14), (24, -0.14)):
            key_rot("Head", f, (0, 0, yaw))
        for f, yaw in ((1, 0.07), (13, -0.07), (24, 0.07)):
            key_rot("Body", f, (0, 0, yaw))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature("creature49", bones, idle, walk, cam=0.75)


# =============================================================================
# Créature 50 — Paon : roue de plumes ocellées, démarche de parade.
# =============================================================================
def paon():
    fresh_scene()
    blue = material("Paon50Blue", (0.10, 0.28, 0.62), roughness=0.5)
    teal = material("Paon50Teal", (0.10, 0.48, 0.38), roughness=0.5)
    gold = material("Paon50Gold", (0.80, 0.62, 0.15))
    dark = material("Paon50Dark", (0.06, 0.06, 0.08))
    beak = material("Paon50Beak", (0.60, 0.52, 0.32))

    sphere("Body", blue, (0, 0.10, 0.85), (0.36, 0.48, 0.40))
    # Cou + petite tête huppée + bec.
    for y, z, r in ((-0.26, 1.05, 0.10), (-0.36, 1.30, 0.09), (-0.42, 1.52, 0.08)):
        sphere("Neck", blue, (0, y, z), (r, r, r * 1.4))
    sphere("Head", blue, (0, -0.48, 1.68), (0.10, 0.12, 0.09))
    for sx in (-0.06, 0.0, 0.06):  # huppe : 3 tiges à pompon doré
        cylinder("Head", dark, (sx, -0.44, 1.82), (0.008, 0.008, 0.10))
        sphere("Head", gold, (sx, -0.44, 1.89), (0.022, 0.022, 0.02))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.06, -0.56, 1.70), (0.028, 0.024, 0.03))
    cone("Head", beak, (0, -0.64, 1.65), (0.03, 0.03, 0.10),
         rotation=(math.radians(100), 0, 0))
    # La roue (os Tail) : éventail de plumes ocellées incliné vers l'arrière.
    for deg in (-52, -26, 0, 26, 52):
        rad = math.radians(deg)
        ux = math.sin(rad)
        uz = math.cos(rad)
        px, py, pz = ux * 0.72, 0.52 + 0.18, 0.85 + uz * 0.72
        sphere("Tail", teal, (px, py, pz), (0.16, 0.05, 0.30),
               )
        sphere("Tail", gold, (px * 1.18, py + 0.02, 0.85 + uz * 1.05 - 0.02),
               (0.07, 0.04, 0.09))
        sphere("Tail", dark, (px * 1.18, py + 0.04, 0.85 + uz * 1.05 - 0.02),
               (0.035, 0.03, 0.05))
    # Deux pattes fines.
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, beak, (sx * 0.12, 0.12, 0.28), (0.04, 0.04, 0.56))
        cone(bone, beak, (sx * 0.12, 0.0, 0.05), (0.055, 0.035, 0.09),
             rotation=(math.radians(100), 0, 0))

    bones = {
        "Body": ("Root", (0, 0.35, 0.85), (0, -0.20, 0.92)),
        "Neck": ("Body", (0, -0.20, 0.98), (0, -0.44, 1.58)),
        "Head": ("Neck", (0, -0.44, 1.58), (0, -0.66, 1.70)),
        "Tail": ("Body", (0, 0.42, 0.90), (0, 0.75, 1.75)),
        "LegL": ("Body", (-0.12, 0.12, 0.55), (-0.12, 0.12, 0.02)),
        "LegR": ("Body", (0.12, 0.12, 0.55), (0.12, 0.12, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Parade : la roue frémit et tangue, le cou se redresse, la tête
        # pivote fièrement d'un côté à l'autre.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, sw in ((1, 0.10), (8, -0.06), (15, 0.10), (22, -0.06), (29, 0.10),
                      (40, 0.10)):
            key_rot("Tail", f, (0.05, sw * 0.4, sw))
        for f, up in ((1, 0.0), (14, -0.12), (28, 0.05), (40, 0.0)):
            key_rot("Neck", f, (up, 0, 0))
        for f, yaw in ((1, -0.30), (20, 0.30), (40, -0.30)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        # Démarche de parade : pas hauts et précieux, la roue ballotte,
        # le cou picore le rythme.
        s = math.radians(30)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, pump in ((1, 0.08), (13, -0.05), (24, 0.08)):
            key_rot("Neck", f, (pump, 0, 0))
        for f, sw in ((1, 0.08), (13, -0.08), (24, 0.08)):
            key_rot("Tail", f, (0, 0, sw))
        for f in (1, 24):
            key_rot("Head", f, (0, 0, 0))

    build_creature("creature50", bones, idle, walk, cam=1.05)


# =============================================================================
# Créature 51 — Chameau de Bactriane : deux bosses, rumination, amble chaloupé.
# =============================================================================
def chameau():
    fresh_scene()
    sand = material("Chameau51Sand", (0.72, 0.55, 0.32))
    sand_d = material("Chameau51SandD", (0.55, 0.40, 0.22))
    dark = material("Chameau51Dark", (0.10, 0.08, 0.06))

    sphere("Body", sand, (0, 0.10, 1.30), (0.58, 0.98, 0.52))
    sphere("Body", sand_d, (0, -0.28, 1.82), (0.26, 0.28, 0.26))  # bosse avant
    sphere("Body", sand_d, (0, 0.38, 1.85), (0.28, 0.30, 0.28))  # bosse arrière
    # Cou qui plonge puis remonte (os Neck), tête au museau lippu.
    for y, z, r in ((-0.85, 1.35, 0.19), (-1.02, 1.62, 0.17), (-1.12, 1.90, 0.16)):
        sphere("Neck", sand, (0, y, z), (r, r, r * 1.35))
    sphere("Head", sand, (0, -1.22, 2.12), (0.17, 0.26, 0.16))
    sphere("Head", sand_d, (0, -1.42, 2.02), (0.11, 0.12, 0.09))  # museau
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -1.32, 2.20), (0.04, 0.035, 0.045))
        sphere("Head", sand_d, (sx * 0.13, -1.10, 2.26), (0.05, 0.035, 0.06))  # oreille
    for bone, x, y in (("LegFL", -0.32, -0.55), ("LegFR", 0.32, -0.55),
                       ("LegBL", -0.32, 0.62), ("LegBR", 0.32, 0.62)):
        cylinder(bone, sand, (x, y, 0.65), (0.13, 0.13, 1.26))
        sphere(bone, sand_d, (x, y, 0.08), (0.15, 0.17, 0.09))  # coussinet
    cone("Tail", sand, (0, 1.15, 1.35), (0.05, 0.05, 0.30),
         rotation=(math.radians(-160), 0, 0))
    sphere("Tail", dark, (0, 1.28, 1.10), (0.07, 0.07, 0.10))

    bones = quad_bones(0.32, -0.55, 0.62, 1.28, ((0, 0.50, 1.25), (0, -0.55, 1.30)), {
        "Neck": ("Body", (0, -0.70, 1.35), (0, -1.15, 2.05)),
        "Head": ("Neck", (0, -1.15, 2.05), (0, -1.50, 2.15)),
        "Tail": ("Body", (0, 1.05, 1.35), (0, 1.35, 1.05)),
    })

    def idle(key_rot, key_loc):
        # Rumination : la mâchoire chaloupe en petits cercles (yaw rapide de la
        # tête), le cou se balance, la queue chasse.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.10), (6, -0.10), (11, 0.10), (16, -0.10), (21, 0.10),
                       (30, 0.0), (40, 0.10)):
            key_rot("Head", f, (0.05, 0, yaw))
        for f, sw in ((1, 0.06), (20, -0.08), (40, 0.06)):
            key_rot("Neck", f, (sw, 0, sw * 0.5))
        for f, sw in ((1, 0.35), (20, -0.35), (40, 0.35)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Amble : les deux pattes du même côté partent ensemble — la démarche
        # chaloupée du chameau, avec roulis marqué.
        s = math.radians(19)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBL", f, (a * 0.85, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBR", f, (-a * 0.85, 0, 0))
        for f, roll in ((1, 0.08), (13, -0.08), (24, 0.08)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (7, 0.04), (13, 0.0), (19, 0.04), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Neck", f, (sw, 0, 0))
        for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature("creature51", bones, idle, walk, cam=1.2)


panda()
tigre()
grue()
macaque()
buffle()
panda_roux()
cobra()
koi()
paon()
chameau()
print("PACK DONE")
