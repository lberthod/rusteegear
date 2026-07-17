"""Génère assets/models/creature22.glb … creature26.glb : 5 monstres animés.

Pack « savane & terreurs » — girafe, crocodile, gorille, autruche, scorpion
géant. Mêmes conventions que creature2/creature21 :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/… par créature, mesh unique skinné (1 os / partie, poids 1.0) ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables, chaque clip
  keyframe tous les os animés par l'autre (piège glTF : canaux absents = os figé) ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur) ;
- échelle appliquée AVANT la rotation dans add_part (piège rotation/scale des
  cônes : sinon l'étirement se fait dans les axes monde et déforme la pièce) ;
- pose remise au neutre avant export ET avant la vignette (l'exporteur glTF
  laisse l'armature posée au dernier frame évalué).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack22_26.py
"""

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

PARTS = []


def material(name, rgb, roughness=0.85):
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

    # Vignette de contrôle : pistes NLA purgées + pose neutre (piège connu).
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
    # `cam` : facteur d'éloignement/hauteur pour cadrer les grands gabarits.
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


# =============================================================================
# Créature 22 — Girafe : haute sur pattes, long cou articulé, robe à taches.
# =============================================================================
def girafe():
    fresh_scene()
    hide = material("Girafe22Hide", (0.88, 0.58, 0.16))
    patch = material("Girafe22Patch", (0.46, 0.26, 0.08))
    dark = material("Girafe22Dark", (0.14, 0.10, 0.08))

    sphere("Body", hide, (0, 0.10, 1.75), (0.62, 1.00, 0.60))
    # Taches : petites lentilles brunes plaquées sur les flancs.
    for sx, y, z in ((-1, -0.25, 2.05), (1, 0.30, 1.95), (-1, 0.55, 1.70),
                     (1, -0.40, 1.65), (-1, 0.10, 1.45), (1, 0.65, 2.05)):
        sphere("Body", patch, (sx * 0.55, y, z), (0.14, 0.16, 0.13))
    # Cou incliné vers l'avant-haut : trois fuseaux qui se chevauchent (os Neck),
    # ancrés dans le poitrail pour éviter toute couture visible.
    sphere("Body", hide, (0, -0.55, 2.00), (0.34, 0.42, 0.42))  # base de cou
    sphere("Neck", hide, (0, -0.68, 2.30), (0.26, 0.30, 0.50))
    sphere("Neck", hide, (0, -0.82, 2.70), (0.22, 0.26, 0.50))
    sphere("Neck", hide, (0, -0.95, 3.10), (0.19, 0.23, 0.45))
    sphere("Neck", patch, (0, -0.80, 2.62), (0.13, 0.15, 0.12))  # tache de cou
    # Tête + museau + ossicones + yeux.
    sphere("Head", hide, (0, -1.06, 3.45), (0.26, 0.34, 0.22))
    sphere("Head", patch, (0, -1.32, 3.38), (0.13, 0.15, 0.11))  # museau
    sphere("Head", dark, (-0.13, -1.28, 3.52), (0.05, 0.04, 0.06))  # œil G
    sphere("Head", dark, (0.13, -1.28, 3.52), (0.05, 0.04, 0.06))  # œil D
    for sx in (-1, 1):
        cone("Head", patch, (sx * 0.10, -1.00, 3.70), (0.045, 0.045, 0.12))
        sphere("Head", dark, (sx * 0.10, -1.00, 3.82), (0.055, 0.055, 0.05))
    # Pattes hautes et fines, sabots sombres.
    # Bas des pattes à z ≥ +0,02 : un mesh qui perce le sol (z < 0) laisse le
    # TriMesh kinématique en pénétration permanente avec le plan du sol et la
    # dépénétration annule chaque déplacement script (créature figée).
    for bone, x, y in (("LegFL", -0.40, -0.50), ("LegFR", 0.40, -0.50),
                       ("LegBL", -0.40, 0.60), ("LegBR", 0.40, 0.60)):
        cylinder(bone, hide, (x, y, 0.77), (0.13, 0.13, 1.48))
        cylinder(bone, dark, (x, y, 0.11), (0.14, 0.14, 0.16))
    # Queue fine à pinceau sombre.
    cone("Tail", hide, (0, 1.08, 1.65), (0.06, 0.06, 0.35),
         rotation=(math.radians(-160), 0, 0))
    sphere("Tail", dark, (0, 1.20, 1.38), (0.09, 0.09, 0.12))

    bones = {
        "Body": ("Root", (0, 0.50, 1.70), (0, -0.50, 1.80)),
        "Neck": ("Body", (0, -0.60, 2.05), (0, -1.00, 3.20)),
        "Head": ("Neck", (0, -1.00, 3.20), (0, -1.35, 3.55)),
        "Tail": ("Body", (0, 1.00, 1.75), (0, 1.30, 1.30)),
        "LegFL": ("Body", (-0.40, -0.50, 1.45), (-0.40, -0.50, 0.02)),
        "LegFR": ("Body", (0.40, -0.50, 1.45), (0.40, -0.50, 0.02)),
        "LegBL": ("Body", (-0.40, 0.60, 1.45), (-0.40, 0.60, 0.02)),
        "LegBR": ("Body", (0.40, 0.60, 1.45), (0.40, 0.60, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Le cou ondule lentement, la tête broute presque, la queue chasse.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.10), (20, -0.10), (40, 0.10)):
            key_rot("Neck", f, (sw, 0, sw * 0.6))
        for f, nod in ((1, 0.0), (20, 0.15), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((1, 0.35), (20, -0.35), (40, 0.35)):
            key_rot("Tail", f, (0, 0, sway))

    def walk(key_rot, key_loc):
        # Amble de girafe : les deux pattes du même côté quasi en phase.
        swing = math.radians(18)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBL", f, (s * 0.8, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBR", f, (-s * 0.8, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.08), (13, -0.08), (24, 0.08)):
            key_rot("Neck", f, (sw, 0, 0))
        for f, nod in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((1, 0.20), (13, -0.20), (24, 0.20)):
            key_rot("Tail", f, (0, 0, sway))

    build_creature("creature22", bones, idle, walk, cam=1.5)


# =============================================================================
# Créature 23 — Crocodile : bas sur pattes, mâchoire qui claque, queue balayeuse.
# =============================================================================
def crocodile():
    fresh_scene()
    scale_g = material("Croco23Scale", (0.20, 0.42, 0.18))
    belly = material("Croco23Belly", (0.72, 0.74, 0.52))
    ridge = material("Croco23Ridge", (0.12, 0.28, 0.10))
    ivory = material("Croco23Ivory", (0.92, 0.90, 0.80))
    dark = material("Croco23Dark", (0.10, 0.10, 0.08))

    sphere("Body", scale_g, (0, 0.15, 0.50), (0.62, 1.25, 0.38))
    sphere("Body", belly, (0, 0.15, 0.38), (0.48, 1.10, 0.26))  # ventre clair
    # Crête dorsale : rangée de petits cônes sombres.
    for i, y in enumerate((-0.55, -0.15, 0.25, 0.65, 1.05)):
        cone("Body" if y < 0.9 else "Tail", ridge, (0, y, 0.86), (0.09, 0.09, 0.10))
    # Tête plate + museau supérieur (os Head), yeux en périscope.
    sphere("Head", scale_g, (0, -1.25, 0.55), (0.40, 0.42, 0.26))
    sphere("Head", scale_g, (0, -1.85, 0.52), (0.26, 0.48, 0.15))  # museau haut
    for sx in (-1, 1):
        sphere("Head", scale_g, (sx * 0.20, -1.32, 0.80), (0.09, 0.09, 0.10))
        sphere("Head", dark, (sx * 0.20, -1.38, 0.84), (0.05, 0.045, 0.05))
        # Dents du haut, pointées vers le bas.
        cone("Head", ivory, (sx * 0.16, -2.05, 0.42), (0.035, 0.035, 0.07),
             rotation=(math.radians(180), 0, 0))
    # Mâchoire inférieure (os Jaw, pivote pour claquer).
    sphere("Jaw", scale_g, (0, -1.80, 0.32), (0.22, 0.44, 0.10))
    sphere("Jaw", belly, (0, -1.78, 0.28), (0.18, 0.40, 0.07))
    for sx in (-1, 1):
        cone("Jaw", ivory, (sx * 0.13, -2.00, 0.40), (0.035, 0.035, 0.07))
    # Queue : fuseaux décroissants vers l'arrière (os Tail).
    for y, s in ((1.35, 0.30), (1.80, 0.22), (2.15, 0.14)):
        sphere("Tail", scale_g, (0, y, 0.42), (s, 0.35, s * 0.75))
    cone("Tail", ridge, (0, 2.40, 0.40), (0.08, 0.08, 0.18),
         rotation=(math.radians(95), 0, 0))
    # Pattes courtes et écartées.
    # Bas des pattes à z ≥ +0,02 (cf. girafe : un mesh sous le sol fige la
    # créature par dépénétration du TriMesh kinématique).
    for bone, x, y in (("LegFL", -0.52, -0.70), ("LegFR", 0.52, -0.70),
                       ("LegBL", -0.52, 0.85), ("LegBR", 0.52, 0.85)):
        cylinder(bone, scale_g, (x, y, 0.30), (0.13, 0.13, 0.55))
        sphere(bone, belly, (x, y - 0.10, 0.10), (0.15, 0.18, 0.07))

    bones = {
        "Body": ("Root", (0, 0.60, 0.50), (0, -0.60, 0.55)),
        "Head": ("Body", (0, -0.90, 0.50), (0, -2.10, 0.50)),
        "Jaw": ("Head", (0, -1.42, 0.38), (0, -2.05, 0.30)),
        "Tail": ("Body", (0, 1.05, 0.48), (0, 2.45, 0.38)),
        "LegFL": ("Body", (-0.52, -0.70, 0.45), (-0.52, -0.70, 0.02)),
        "LegFR": ("Body", (0.52, -0.70, 0.45), (0.52, -0.70, 0.02)),
        "LegBL": ("Body", (-0.52, 0.85, 0.45), (-0.52, 0.85, 0.02)),
        "LegBR": ("Body", (0.52, 0.85, 0.45), (0.52, 0.85, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Gueule qui s'entrouvre puis claque, queue qui ondule.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, open_ in ((1, 0.0), (16, 0.45), (24, 0.45), (28, 0.0), (40, 0.0)):
            key_rot("Jaw", f, (-open_, 0, 0))
        for f, nod in ((1, 0.0), (16, 0.10), (28, -0.02), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.20), (20, -0.20), (40, 0.20)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Reptation : pattes en diagonale, queue en grand balayage latéral.
        swing = math.radians(24)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.02), (13, 0.0), (19, 0.02), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.45), (13, -0.45), (24, 0.45)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.08), (13, 0.08), (24, -0.08)):
            key_rot("Head", f, (0, 0, yaw))
        for f in (1, 24):
            key_rot("Jaw", f, (0, 0, 0))

    build_creature("creature23", bones, idle, walk)


# =============================================================================
# Créature 24 — Gorille : bipède massif, longs bras, se frappe le torse en Idle.
# =============================================================================
def gorille():
    fresh_scene()
    fur = material("Gorille24Fur", (0.16, 0.15, 0.17))
    skin = material("Gorille24Skin", (0.48, 0.42, 0.40))
    dark = material("Gorille24Dark", (0.05, 0.05, 0.06))

    sphere("Body", fur, (0, 0.05, 1.15), (0.80, 0.62, 0.80))
    sphere("Body", skin, (0, -0.52, 1.05), (0.34, 0.18, 0.42))  # plastron
    # Tête basse sur les épaules, crête sagittale, face claire.
    sphere("Head", fur, (0, -0.30, 1.95), (0.40, 0.40, 0.38))
    sphere("Head", fur, (0, -0.12, 2.28), (0.20, 0.24, 0.14))  # crête
    sphere("Head", skin, (0, -0.62, 1.88), (0.24, 0.16, 0.24))  # face
    sphere("Head", skin, (0, -0.70, 1.76), (0.16, 0.12, 0.12))  # muffle
    sphere("Head", dark, (-0.12, -0.66, 1.98), (0.05, 0.04, 0.055))  # œil G
    sphere("Head", dark, (0.12, -0.66, 1.98), (0.05, 0.04, 0.055))  # œil D
    # Bras longs jusqu'au sol, épaules fondues dans le torse, poings fermés.
    for bone, sx in (("ArmL", -1), ("ArmR", 1)):
        sphere(bone, fur, (sx * 0.62, -0.08, 1.58), (0.32, 0.30, 0.32))  # épaule
        cylinder(bone, fur, (sx * 0.74, -0.15, 0.88), (0.18, 0.18, 1.25),
                 rotation=(0, math.radians(sx * 6), 0))
        sphere(bone, dark, (sx * 0.80, -0.18, 0.22), (0.22, 0.26, 0.18))  # poing
    # Jambes courtes et épaisses. Pieds à z ≥ +0,02 : sous le sol, le TriMesh
    # kinématique reste en pénétration et la créature est figée sur place.
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, fur, (sx * 0.32, 0.15, 0.42), (0.22, 0.22, 0.76))
        sphere(bone, dark, (sx * 0.32, 0.02, 0.13), (0.26, 0.32, 0.10))  # pied

    bones = {
        "Body": ("Root", (0, 0.40, 1.10), (0, -0.35, 1.30)),
        "Head": ("Body", (0, -0.15, 1.70), (0, -0.55, 2.10)),
        "ArmL": ("Body", (-0.62, -0.08, 1.60), (-0.82, -0.20, 0.10)),
        "ArmR": ("Body", (0.62, -0.08, 1.60), (0.82, -0.20, 0.10)),
        "LegL": ("Body", (-0.32, 0.15, 0.78), (-0.32, 0.15, 0.02)),
        "LegR": ("Body", (0.32, 0.15, 0.78), (0.32, 0.15, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Se redresse et tambourine le torse (bras alternés, rapide), puis
        # retombe sur ses poings. Boucle propre : frame 1 = frame 40.
        for f in (1, 40):
            for b in ("LegL", "LegR", "Head"):
                key_rot(b, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (10, 0.06), (26, 0.06), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, lean in ((1, 0.0), (10, -0.12), (26, -0.12), (40, 0.0)):
            key_rot("Body", f, (lean, 0, 0))
        beat = math.radians(55)
        seq_l = ((1, 0.0), (10, beat), (14, beat * 0.4), (18, beat),
                 (22, beat * 0.4), (26, beat), (40, 0.0))
        seq_r = ((1, 0.0), (10, beat * 0.4), (14, beat), (18, beat * 0.4),
                 (22, beat), (26, beat * 0.4), (40, 0.0))
        for f, a in seq_l:
            key_rot("ArmL", f, (a, 0, 0))
        for f, a in seq_r:
            key_rot("ArmR", f, (a, 0, 0))

    def walk(key_rot, key_loc):
        # Marche sur les phalanges : bras opposés aux jambes, roulis d'épaules.
        swing = math.radians(22)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegL", f, (s, 0, 0))
            key_rot("LegR", f, (-s, 0, 0))
            key_rot("ArmL", f, (-s * 1.2, 0, 0))
            key_rot("ArmR", f, (s * 1.2, 0, 0))
        for f, roll in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Head", f, (nod, 0, 0))

    build_creature("creature24", bones, idle, walk)


# =============================================================================
# Créature 25 — Autruche : deux grandes pattes, cou qui picore, plumeau caudal.
# =============================================================================
def autruche():
    fresh_scene()
    plume = material("Autruche25Plume", (0.16, 0.14, 0.13))
    plume_w = material("Autruche25PlumeW", (0.88, 0.86, 0.80))
    skin = material("Autruche25Skin", (0.78, 0.60, 0.46))
    beak = material("Autruche25Beak", (0.80, 0.52, 0.18))
    dark = material("Autruche25Dark", (0.08, 0.07, 0.07))

    sphere("Body", plume, (0, 0.10, 1.25), (0.52, 0.68, 0.48))
    # Ailerons blancs plaqués + plumeau caudal.
    for sx in (-1, 1):
        sphere("Body", plume_w, (sx * 0.42, 0.20, 1.30), (0.16, 0.38, 0.28))
    sphere("Tail", plume_w, (0, 0.80, 1.40), (0.26, 0.28, 0.30))
    # Cou fin et haut (os Neck), tête petite + bec + yeux.
    cylinder("Neck", skin, (0, -0.42, 1.95), (0.09, 0.09, 0.95),
             rotation=(math.radians(12), 0, 0))
    sphere("Head", skin, (0, -0.55, 2.55), (0.16, 0.20, 0.15))
    cone("Head", beak, (0, -0.78, 2.52), (0.06, 0.06, 0.14),
         rotation=(math.radians(102), 0, 0))
    sphere("Head", dark, (-0.09, -0.62, 2.62), (0.04, 0.035, 0.045))
    sphere("Head", dark, (0.09, -0.62, 2.62), (0.04, 0.035, 0.045))
    # Deux longues pattes ancrées dans le ventre, genoux marqués, doigts avant.
    # Bas des pattes/doigts à z ≥ +0,02 (mesh sous le sol = créature figée par
    # dépénétration du TriMesh kinématique, cf. girafe).
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, skin, (sx * 0.20, 0.10, 0.74), (0.08, 0.08, 1.42))
        sphere(bone, skin, (sx * 0.20, 0.10, 0.66), (0.11, 0.11, 0.13))  # genou
        sphere(bone, plume, (sx * 0.20, 0.10, 1.15), (0.16, 0.20, 0.22))  # cuisse
        cone(bone, beak, (sx * 0.20, -0.08, 0.09), (0.09, 0.06, 0.14),
             rotation=(math.radians(100), 0, 0))  # doigts

    bones = {
        "Body": ("Root", (0, 0.35, 1.25), (0, -0.30, 1.35)),
        "Neck": ("Body", (0, -0.30, 1.55), (0, -0.52, 2.42)),
        "Head": ("Neck", (0, -0.52, 2.42), (0, -0.85, 2.55)),
        "Tail": ("Body", (0, 0.55, 1.45), (0, 0.95, 1.55)),
        "LegL": ("Body", (-0.20, 0.10, 1.20), (-0.20, 0.10, 0.02)),
        "LegR": ("Body", (0.20, 0.10, 1.20), (0.20, 0.10, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Picore : le cou plonge deux fois, le plumeau frétille.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, dip in ((1, 0.0), (10, 0.55), (16, 0.0), (24, 0.55), (30, 0.0), (40, 0.0)):
            key_rot("Neck", f, (dip, 0, 0))
        for f, nod in ((1, 0.0), (10, 0.35), (16, 0.0), (24, 0.35), (30, 0.0), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.12), (8, -0.12), (16, 0.12), (24, -0.12), (32, 0.12), (40, 0.12)):
            key_rot("Tail", f, (0, sw, 0))
        for f, roll in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_rot("Body", f, (0, roll, 0))

    def walk(key_rot, key_loc):
        # Foulées amples et rapides, cou qui pompe d'avant en arrière.
        swing = math.radians(38)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegL", f, (s, 0, 0))
            key_rot("LegR", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.07), (13, 0.0), (19, 0.07), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, pump in ((1, 0.10), (7, -0.06), (13, 0.10), (19, -0.06), (24, 0.10)):
            key_rot("Neck", f, (pump, 0, 0))
        for f, nod in ((1, -0.06), (13, 0.06), (24, -0.06)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.10), (13, -0.10), (24, 0.10)):
            key_rot("Tail", f, (0, sw, 0))

    build_creature("creature25", bones, idle, walk)


# =============================================================================
# Créature 26 — Scorpion géant : pinces, dard en arc au-dessus du dos.
# =============================================================================
def scorpion():
    fresh_scene()
    shell = material("Scorpion26Shell", (0.42, 0.13, 0.10))
    shell_d = material("Scorpion26ShellD", (0.26, 0.08, 0.07))
    sting = material("Scorpion26Sting", (0.90, 0.78, 0.30))
    dark = material("Scorpion26Dark", (0.06, 0.05, 0.05))

    # Corps plat segmenté (avant = -Y).
    sphere("Body", shell, (0, 0.05, 0.42), (0.62, 0.85, 0.30))
    sphere("Body", shell_d, (0, -0.55, 0.45), (0.45, 0.35, 0.24))  # céphalothorax
    sphere("Body", dark, (-0.14, -0.85, 0.52), (0.05, 0.04, 0.05))  # œil G
    sphere("Body", dark, (0.14, -0.85, 0.52), (0.05, 0.04, 0.05))  # œil D
    # Pinces : bras + pince ouverte (deux doigts), os ClawL/ClawR.
    for bone, sx in (("ClawL", -1), ("ClawR", 1)):
        cylinder(bone, shell_d, (sx * 0.62, -0.75, 0.38), (0.10, 0.10, 0.45),
                 rotation=(math.radians(90), 0, math.radians(-sx * 30)))
        sphere(bone, shell, (sx * 0.75, -1.05, 0.38), (0.24, 0.30, 0.16))
        cone(bone, shell_d, (sx * 0.68, -1.32, 0.44), (0.08, 0.08, 0.16),
             rotation=(math.radians(105), 0, math.radians(sx * 12)))
        cone(bone, shell_d, (sx * 0.85, -1.30, 0.34), (0.07, 0.07, 0.14),
             rotation=(math.radians(100), 0, math.radians(-sx * 10)))
    # Queue : arc de sphères qui monte au-dessus du dos, dard jaune (os Tail).
    for y, z, s in ((0.78, 0.52, 0.22), (1.02, 0.80, 0.19),
                    (1.16, 1.10, 0.17), (1.10, 1.40, 0.15)):
        sphere("Tail", shell, (0, y, z), (s, s * 1.15, s))
    cone("Tail", sting, (0, 0.96, 1.58), (0.10, 0.10, 0.20),
         rotation=(math.radians(35), 0, 0))
    # Huit petites pattes arquées (4 os, 2 pattes par os).
    for bone, x, y in (("LegFL", -0.55, -0.30), ("LegFR", 0.55, -0.30),
                       ("LegBL", -0.55, 0.35), ("LegBR", 0.55, 0.35)):
        sx = 1 if x > 0 else -1
        # Centre à 0,26 : le cylindre incliné (extension verticale ~0,24)
        # reste au-dessus du sol (z < 0 = créature figée, cf. girafe).
        for dy in (0.0, 0.18):
            cylinder(bone, shell_d, (x + sx * 0.10, y + dy, 0.26),
                     (0.07, 0.07, 0.48), rotation=(0, math.radians(sx * 35), 0))

    bones = {
        "Body": ("Root", (0, 0.45, 0.42), (0, -0.60, 0.45)),
        "ClawL": ("Body", (-0.50, -0.60, 0.40), (-0.80, -1.35, 0.38)),
        "ClawR": ("Body", (0.50, -0.60, 0.40), (0.80, -1.35, 0.38)),
        "Tail": ("Body", (0, 0.70, 0.45), (0, 1.05, 1.70)),
        "LegFL": ("Body", (-0.45, -0.30, 0.40), (-0.75, -0.25, 0.02)),
        "LegFR": ("Body", (0.45, -0.30, 0.40), (0.75, -0.25, 0.02)),
        "LegBL": ("Body", (-0.45, 0.35, 0.40), (-0.75, 0.42, 0.02)),
        "LegBR": ("Body", (0.45, 0.35, 0.40), (0.75, 0.42, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Menace : pinces qui s'écartent et se referment, dard qui oscille.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, open_ in ((1, 0.0), (12, 0.35), (20, 0.10), (28, 0.35), (40, 0.0)):
            key_rot("ClawL", f, (0, 0, open_))
            key_rot("ClawR", f, (0, 0, -open_))
        for f, whip in ((1, 0.0), (12, -0.25), (20, 0.10), (28, -0.25), (40, 0.0)):
            key_rot("Tail", f, (whip, 0, 0))
        for f, sw in ((1, 0.04), (20, -0.04), (40, 0.04)):
            key_rot("Body", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Trottinement : deux vagues de pattes en quinconce, pinces en garde.
        swing = math.radians(26)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.02), (13, 0.0), (19, 0.02), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, guard in ((1, 0.12), (13, 0.22), (24, 0.12)):
            key_rot("ClawL", f, (0, 0, guard))
            key_rot("ClawR", f, (0, 0, -guard))
        for f, whip in ((1, -0.10), (13, 0.10), (24, -0.10)):
            key_rot("Tail", f, (whip, 0, 0))

    build_creature("creature26", bones, idle, walk, cam=0.85)


girafe()
crocodile()
gorille()
autruche()
scorpion()
print("PACK DONE")
