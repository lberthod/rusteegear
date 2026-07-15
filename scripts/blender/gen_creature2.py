"""Génère assets/models/creature2.glb : deuxième créature « style Pokémon ».

Style volontairement différent de creature.glb (bipède vert) : un quadrupède
roux façon renardeau — corps rond, museau crème, grandes oreilles, queue
touffue à pointe crème. Même conventions que la créature n°1 :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/Head/Tail/LegFL/LegFR/LegBL/LegBR, mesh unique skinné ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur).
"""

import math

import bpy
from mathutils import Vector

OUT = "/Users/berthod/Desktop/motor3derust/assets/models/creature2.glb"

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.fps = 24


def material(name, rgb):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.8
    return m


MAT_FUR = material("Creature2Fur", (0.82, 0.38, 0.12))  # roux
MAT_CREAM = material("Creature2Cream", (0.93, 0.86, 0.70))  # museau/pointe de queue
MAT_DARK = material("Creature2Dark", (0.16, 0.10, 0.08))  # pattes/oreilles/yeux

PARTS = []  # (objet, nom de groupe de vertex / os)


def add_part(bone, mat, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    create_op(location=location, rotation=rotation)
    ob = bpy.context.active_object
    ob.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    ob.data.materials.append(mat)
    vg = ob.vertex_groups.new(name=bone)
    vg.add(range(len(ob.data.vertices)), 1.0, "REPLACE")
    PARTS.append(ob)
    return ob


def sphere(bone, mat, location, scale, segments=24, rings=16):
    def op(location, rotation):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=segments, ring_count=rings, radius=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale)


def cone(bone, mat, location, scale, rotation=(0, 0, 0)):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cone_add(
            vertices=16, radius1=1.0, radius2=0.0, depth=2.0,
            location=location, rotation=rotation,
        )

    return add_part(bone, mat, op, location, scale, rotation)


def cylinder(bone, mat, location, scale):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=16, radius=1.0, depth=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale)


# --- Corps (avant = -Y) ------------------------------------------------------
sphere("Body", MAT_FUR, (0, 0.15, 1.05), (0.78, 1.10, 0.72))
sphere("Body", MAT_CREAM, (0, -0.60, 1.00), (0.46, 0.50, 0.50))  # poitrail crème

# Tête + museau + oreilles + yeux
sphere("Head", MAT_FUR, (0, -0.92, 1.58), (0.60, 0.55, 0.52))
sphere("Head", MAT_CREAM, (0, -1.42, 1.46), (0.24, 0.26, 0.19))  # museau
sphere("Head", MAT_DARK, (0, -1.64, 1.50), (0.07, 0.06, 0.06))  # truffe
sphere("Head", MAT_DARK, (-0.27, -1.38, 1.74), (0.085, 0.05, 0.10))  # œil G
sphere("Head", MAT_DARK, (0.27, -1.38, 1.74), (0.085, 0.05, 0.10))  # œil D
cone("Head", MAT_DARK, (-0.30, -0.85, 2.20), (0.16, 0.10, 0.30),
     rotation=(0, math.radians(-12), 0))  # oreille G
cone("Head", MAT_DARK, (0.30, -0.85, 2.20), (0.16, 0.10, 0.30),
     rotation=(0, math.radians(12), 0))  # oreille D

# Pattes (cylindres du sol au ventre)
for bone, x, y in (
    ("LegFL", -0.45, -0.55),
    ("LegFR", 0.45, -0.55),
    ("LegBL", -0.45, 0.72),
    ("LegBR", 0.45, 0.72),
):
    # depth=1 mis à l'échelle 0.9 → patte du sol (z=0) jusqu'au ventre (z=0.9)
    cylinder(bone, MAT_DARK, (x, y, 0.45), (0.17, 0.17, 0.9))

# Queue touffue : gros fuseau incliné vers le haut/arrière + pointe crème
cone("Tail", MAT_FUR, (0, 1.22, 1.35), (0.30, 0.26, 0.55),
     rotation=(math.radians(-125), 0, 0))
sphere("Tail", MAT_CREAM, (0, 1.62, 1.78), (0.22, 0.24, 0.22))

# --- Fusion en un seul mesh ---------------------------------------------------
bpy.ops.object.select_all(action="DESELECT")
for ob in PARTS:
    ob.select_set(True)
bpy.context.view_layer.objects.active = PARTS[0]
bpy.ops.object.join()
creature = bpy.context.active_object
creature.name = "Creature2"

# --- Armature -----------------------------------------------------------------
bpy.ops.object.armature_add(location=(0, 0, 0))
arm = bpy.context.active_object
arm.name = "Creature2Rig"
bpy.ops.object.mode_set(mode="EDIT")
eb = arm.data.edit_bones
root = eb[0]
root.name = "Root"
root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.35))

BONES = {
    "Body": ("Root", (0, 0.55, 1.05), (0, -0.55, 1.15)),
    "Head": ("Body", (0, -0.80, 1.45), (0, -1.45, 1.70)),
    "Tail": ("Body", (0, 1.05, 1.25), (0, 1.70, 1.80)),
    "LegFL": ("Body", (-0.45, -0.55, 0.90), (-0.45, -0.55, 0.02)),
    "LegFR": ("Body", (0.45, -0.55, 0.90), (0.45, -0.55, 0.02)),
    "LegBL": ("Body", (-0.45, 0.72, 0.90), (-0.45, 0.72, 0.02)),
    "LegBR": ("Body", (0.45, 0.72, 0.90), (0.45, 0.72, 0.02)),
}
for name, (parent, head, tail) in BONES.items():
    b = eb.new(name)
    b.head, b.tail = Vector(head), Vector(tail)
    b.parent = eb[parent]
bpy.ops.object.mode_set(mode="OBJECT")

# Skinning : groupes de vertex déjà posés (1 os / partie), l'armature suffit.
creature.parent = arm
mod = creature.modifiers.new("Armature", "ARMATURE")
mod.object = arm

# --- Animations ---------------------------------------------------------------
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


def bake_clip(name, length, keyer):
    """Crée l'action `name` (keyframes via `keyer`) et la pousse en piste NLA."""
    ad = arm.animation_data_create()
    ad.action = None
    keyer()
    act = ad.action
    act.name = name
    track = ad.nla_tracks.new()
    track.name = name
    strip = track.strips.new(name, 1, act)
    strip.name = name
    ad.action = None
    return act


def idle_keys():
    # Respiration : le corps se soulève, la queue balance, les oreilles pivotent.
    for f in (1, 40):
        for leg in ("LegFL", "LegFR", "LegBL", "LegBR"):
            key_rot(leg, f, (0, 0, 0))
    for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
        key_loc("Body", f, (0, dz, 0))
    for f, sway in ((1, 0.25), (20, -0.25), (40, 0.25)):
        key_rot("Tail", f, (0, 0, sway))
    for f, nod in ((1, 0.0), (20, 0.08), (40, 0.0)):
        key_rot("Head", f, (nod, 0, 0))


def walk_keys():
    # Trot en diagonale : FL+BR en phase, FR+BL en opposition ; petit bob du
    # corps (2 pas par cycle) et queue qui fouette.
    swing = math.radians(28)
    for f, s in ((1, swing), (13, -swing), (24, swing)):
        key_rot("LegFL", f, (s, 0, 0))
        key_rot("LegBR", f, (s, 0, 0))
        key_rot("LegFR", f, (-s, 0, 0))
        key_rot("LegBL", f, (-s, 0, 0))
    for f, dz in ((1, 0.0), (7, 0.04), (13, 0.0), (19, 0.04), (24, 0.0)):
        key_loc("Body", f, (0, dz, 0))
    for f, sway in ((1, 0.35), (13, -0.35), (24, 0.35)):
        key_rot("Tail", f, (0, 0, sway))
    for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
        key_rot("Head", f, (nod, 0, 0))


bake_clip("Idle", 40, idle_keys)
bake_clip("Walk", 24, walk_keys)
for pb in arm.pose.bones:
    pb.location = (0, 0, 0)
    pb.rotation_euler = (0, 0, 0)
bpy.ops.object.mode_set(mode="OBJECT")

# --- Export -------------------------------------------------------------------
bpy.ops.object.select_all(action="SELECT")
bpy.ops.export_scene.gltf(
    filepath=OUT,
    export_format="GLB",
    export_skins=True,
    export_animations=True,
    export_animation_mode="NLA_TRACKS",
    export_force_sampling=True,
    export_yup=True,
)
print("EXPORTED", OUT)

# --- Rendu de contrôle (vue 3/4 avant) -----------------------------------------
bpy.ops.object.mode_set(mode="OBJECT")
# Pose de repos pour le rendu : sans quoi les pistes NLA (Idle/Walk)
# posent les pattes en pleine foulée sur la vignette.
ad = arm.animation_data
ad.action = None
for t in list(ad.nla_tracks):
    ad.nla_tracks.remove(t)
# L'exporteur glTF échantillonne les clips et laisse la pose au dernier
# frame évalué : remise au neutre pour la vignette.
for pb in arm.pose.bones:
    pb.location = (0, 0, 0)
    pb.rotation_euler = (0, 0, 0)
scene.frame_set(1)
bpy.context.view_layer.update()
dg = bpy.context.evaluated_depsgraph_get()
pb = arm.evaluated_get(dg).pose.bones["LegFL"]
print("POSE LegFL euler:", tuple(round(v, 3) for v in pb.matrix_basis.to_euler()))
bpy.ops.object.camera_add(location=(4.6, -6.0, 3.0),
                          rotation=(math.radians(74), 0, math.radians(37)))
scene.camera = bpy.context.active_object
bpy.ops.object.light_add(type="SUN", location=(2, -3, 6),
                         rotation=(math.radians(35), math.radians(20), 0))
bpy.context.active_object.data.energy = 3.0
scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = 640
scene.render.resolution_y = 480
scene.render.filepath = OUT.replace(".glb", "_preview.png")
bpy.ops.render.render(write_still=True)
print("RENDERED", scene.render.filepath)
