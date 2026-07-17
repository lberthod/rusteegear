"""Génère assets/models/creature21.glb : monstre n°21 « style Pokémon », un éléphanteau.

Quadrupède gris pachyderme — corps massif, grandes oreilles plates, trompe
articulée (os dédié), défenses crème, pattes épaisses à ongles crème, petite
queue à touffe sombre. Mêmes conventions que les créatures existantes :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/Head/Trunk/Tail/LegFL/LegFR/LegBL/LegBR, mesh unique skinné ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables, chaque clip
  keyframe tous les os animés par l'autre ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur).
"""

import math

import bpy
from mathutils import Vector

OUT = "/Users/berthod/Desktop/motor3derust/assets/models/creature21.glb"

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.fps = 24


def material(name, rgb):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.85
    return m


MAT_HIDE = material("Creature21Hide", (0.46, 0.47, 0.52))  # gris pachyderme
MAT_EAR = material("Creature21Ear", (0.58, 0.46, 0.50))  # intérieur d'oreille rosé
MAT_IVORY = material("Creature21Ivory", (0.93, 0.90, 0.78))  # défenses/ongles
MAT_DARK = material("Creature21Dark", (0.12, 0.10, 0.12))  # yeux/touffe de queue

PARTS = []  # (objet, nom de groupe de vertex / os)


def add_part(bone, mat, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    # Échelle appliquée AVANT la rotation : sinon un cône incliné est étiré dans
    # les axes monde et se déforme (cf. piège rotation/scale des cônes Blender).
    create_op(location=location, rotation=(0, 0, 0))
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
sphere("Body", MAT_HIDE, (0, 0.15, 1.20), (0.95, 1.25, 0.88))

# Tête + yeux
sphere("Head", MAT_HIDE, (0, -1.10, 1.62), (0.62, 0.56, 0.58))
sphere("Head", MAT_DARK, (-0.30, -1.55, 1.80), (0.085, 0.05, 0.10))  # œil G
sphere("Head", MAT_DARK, (0.30, -1.55, 1.80), (0.085, 0.05, 0.10))  # œil D

# Grandes oreilles plates : disque gris + intérieur rosé légèrement décalé
for sx in (-1, 1):
    sphere("Head", MAT_HIDE, (sx * 0.72, -0.92, 1.72), (0.42, 0.10, 0.55))
    sphere("Head", MAT_EAR, (sx * 0.78, -0.96, 1.70), (0.30, 0.06, 0.40))

# Trompe : chaîne de sphères qui s'affinent, du mufle vers le sol
for loc, r in (
    ((0, -1.62, 1.42), 0.185),
    ((0, -1.74, 1.12), 0.160),
    ((0, -1.82, 0.84), 0.135),
    ((0, -1.88, 0.60), 0.110),
):
    sphere("Trunk", MAT_HIDE, loc, (r, r, r * 1.35))

# Défenses : cônes ivoire pointés vers l'avant-bas, de part et d'autre de la trompe
for sx in (-1, 1):
    cone("Head", MAT_IVORY, (sx * 0.32, -1.58, 1.22), (0.11, 0.11, 0.34),
         rotation=(math.radians(125), 0, math.radians(sx * 16)))

# Pattes épaisses (cylindres du sol au ventre) + ongles ivoire
for bone, x, y in (
    ("LegFL", -0.52, -0.60),
    ("LegFR", 0.52, -0.60),
    ("LegBL", -0.52, 0.78),
    ("LegBR", 0.52, 0.78),
):
    # depth=1 mis à l'échelle 0.95 → patte du sol (z=0) jusqu'au ventre (z=0.95)
    cylinder(bone, MAT_HIDE, (x, y, 0.475), (0.24, 0.24, 0.95))
    sphere(bone, MAT_IVORY, (x, y - 0.20, 0.10), (0.14, 0.10, 0.09))  # ongle avant

# Queue fine à touffe sombre
cone("Tail", MAT_HIDE, (0, 1.42, 1.20), (0.09, 0.09, 0.42),
     rotation=(math.radians(-155), 0, 0))
sphere("Tail", MAT_DARK, (0, 1.60, 0.88), (0.12, 0.12, 0.15))

# --- Fusion en un seul mesh ---------------------------------------------------
bpy.ops.object.select_all(action="DESELECT")
for ob in PARTS:
    ob.select_set(True)
bpy.context.view_layer.objects.active = PARTS[0]
bpy.ops.object.join()
creature = bpy.context.active_object
creature.name = "Creature21"

# --- Armature -----------------------------------------------------------------
bpy.ops.object.armature_add(location=(0, 0, 0))
arm = bpy.context.active_object
arm.name = "Creature21Rig"
bpy.ops.object.mode_set(mode="EDIT")
eb = arm.data.edit_bones
root = eb[0]
root.name = "Root"
root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.35))

BONES = {
    "Body": ("Root", (0, 0.60, 1.15), (0, -0.60, 1.25)),
    "Head": ("Body", (0, -0.90, 1.50), (0, -1.60, 1.75)),
    "Trunk": ("Head", (0, -1.58, 1.50), (0, -1.90, 0.55)),
    "Tail": ("Body", (0, 1.30, 1.30), (0, 1.65, 0.80)),
    "LegFL": ("Body", (-0.52, -0.60, 0.95), (-0.52, -0.60, 0.02)),
    "LegFR": ("Body", (0.52, -0.60, 0.95), (0.52, -0.60, 0.02)),
    "LegBL": ("Body", (-0.52, 0.78, 0.95), (-0.52, 0.78, 0.02)),
    "LegBR": ("Body", (0.52, 0.78, 0.95), (0.52, 0.78, 0.02)),
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
    # Respiration lente, trompe qui se balance, oreilles via léger roulis de
    # tête, queue qui chasse les mouches. Pattes keyframées neutres (cf. piège
    # glTF : chaque clip doit couvrir tous les os animés par l'autre).
    for f in (1, 40):
        for leg in ("LegFL", "LegFR", "LegBL", "LegBR"):
            key_rot(leg, f, (0, 0, 0))
    for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
        key_loc("Body", f, (0, dz, 0))
    for f, sw in ((1, 0.28), (20, -0.28), (40, 0.28)):
        key_rot("Trunk", f, (0.10, 0, sw))
    for f, roll in ((1, 0.05), (20, -0.05), (40, 0.05)):
        key_rot("Head", f, (0, roll, 0))
    for f, sway in ((1, 0.35), (20, -0.35), (40, 0.35)):
        key_rot("Tail", f, (0, 0, sway))


def walk_keys():
    # Pas lourd en diagonale (amplitude modérée : c'est un pachyderme), trompe
    # qui balance d'avant en arrière, tête qui dodeline, queue qui fouette.
    swing = math.radians(20)
    for f, s in ((1, swing), (13, -swing), (24, swing)):
        key_rot("LegFL", f, (s, 0, 0))
        key_rot("LegBR", f, (s, 0, 0))
        key_rot("LegFR", f, (-s, 0, 0))
        key_rot("LegBL", f, (-s, 0, 0))
    for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
        key_loc("Body", f, (0, dz, 0))
    for f, sw in ((1, 0.30), (13, -0.30), (24, 0.30)):
        key_rot("Trunk", f, (sw, 0, 0))
    for f, nod in ((1, 0.06), (13, -0.06), (24, 0.06)):
        key_rot("Head", f, (nod, 0, 0))
    for f, sway in ((1, 0.25), (13, -0.25), (24, 0.25)):
        key_rot("Tail", f, (0, 0, sway))


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
bpy.ops.object.camera_add(location=(4.8, -6.4, 3.2),
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
