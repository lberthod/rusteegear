# Troisième vague d'assets animés (20 : 10 petite faune + 10 décor) en Blender
# headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_fauna_decor_pack3.py
#
# Sortie : assets/models/fauna_*.glb (10) et assets/models/nature_*.glb (10).
# Noms choisis pour ne collisionner avec AUCUN fichier existant (packs
# statique + animés précédents, y compris gen_menagerie_pack{,2}.py générés
# par une autre session en parallèle sur ce dépôt — vérifié via
# `grep -oh 'export("...")' scripts/blender/*.py` avant d'écrire ce script).
# Ce script ne touche à rien côté Rust : il ne fait que produire les .glb.
#
# Même recette que gen_fauna_decor_pack.py (rig minuscule, vertex group plein
# par partie, clip « Idle » en piste NLA, pose résiduelle purgée avant export,
# interpolation LINEAR par défaut) : voir ce fichier pour le détail des
# contraintes moteur (repère, échelle réelle, solidité des sondes).

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

# Palette faune
FUR_ORANGE = (0.78, 0.34, 0.10)  # renard
FUR_GOAT = (0.72, 0.68, 0.60)
FUR_BOAR = (0.30, 0.24, 0.16)
FUR_RACCOON = (0.42, 0.40, 0.38)
CREAM = (0.93, 0.86, 0.70)
DARK = (0.14, 0.10, 0.08)
SHELL_GREEN = (0.24, 0.42, 0.22)
SHELL_DARK = (0.16, 0.30, 0.16)
HERON_GREY = (0.62, 0.64, 0.66)
HERON_WHITE = (0.90, 0.88, 0.84)
BILL_ORANGE = (0.85, 0.48, 0.10)
DRAGONFLY_BLUE = (0.20, 0.55, 0.62)
WING_CLEAR = (0.75, 0.85, 0.85)
LADYBUG_RED = (0.78, 0.10, 0.08)
CROW_BLACK = (0.08, 0.07, 0.08)
GOOSE_GREY = (0.55, 0.55, 0.50)

# Palette décor
WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
STONE = (0.45, 0.44, 0.42)
STONE_LIGHT = (0.58, 0.56, 0.52)
METAL = (0.32, 0.30, 0.29)
METAL_DARK = (0.20, 0.19, 0.18)
CLOTH_RED = (0.72, 0.14, 0.12)
CLOTH_WHITE = (0.88, 0.85, 0.78)
CLAY = (0.55, 0.30, 0.18)
HAY = (0.70, 0.58, 0.22)
WATER_BLUE = (0.22, 0.42, 0.55)
BRASS = (0.62, 0.50, 0.20)


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb):
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.8
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


def cube(bone, material, location, scale, rotation=(0, 0, 0)):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=location, rotation=rotation)

    return add_part(bone, material, op, location, scale, rotation)


def cylinder(bone, material, location, scale, rotation=(0, 0, 0), vertices=10):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=vertices, radius=1.0, depth=1.0, location=location, rotation=rotation
        )

    return add_part(bone, material, op, location, scale, rotation)


def cone(bone, material, location, scale, rotation=(0, 0, 0), vertices=10):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cone_add(
            vertices=vertices, radius1=1.0, radius2=0.0, depth=1.0,
            location=location, rotation=rotation,
        )

    return add_part(bone, material, op, location, scale, rotation)


def sphere(bone, material, location, scale, rotation=(0, 0, 0), segments=16, rings=10):
    def op(location, rotation):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=segments, ring_count=rings, radius=1.0, location=location, rotation=rotation
        )

    return add_part(bone, material, op, location, scale, rotation)


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
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.15))
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


def export(filename):
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
    print(f"[fauna-decor-3] exporté {filename}")


# ---------------------------------------------------------------------------
# Faune ambiante (10)
# ---------------------------------------------------------------------------


def gen_fox():
    """Renard ~0.55 m : corps/tête sur Body, oreilles sur Ears, queue
    touffue sur Tail — idle avec balancement de queue et oreilles à l'écoute."""
    fur = mat("fourrure_renard", FUR_ORANGE)
    cream = mat("creme_renard", CREAM)
    dark = mat("sombre_renard", DARK)
    cube("Body", fur, (0, 0, 0.22), (0.15, 0.30, 0.14))
    sphere("Body", fur, (0, -0.24, 0.26), (0.09, 0.11, 0.09))
    sphere("Body", cream, (0, -0.32, 0.22), (0.04, 0.05, 0.035))
    for dx in (-0.05, 0.05):
        cone("Ears", dark, (dx, -0.24, 0.36), (0.03, 0.02, 0.10),
             rotation=(-0.1, 0, math.copysign(0.15, dx)))
    for x, y in ((-0.06, -0.10), (0.06, -0.10), (-0.06, 0.10), (0.06, 0.10)):
        cube("Body", dark, (x, y, 0.08), (0.035, 0.035, 0.16))
    cone("Tail", cream, (0, 0.28, 0.20), (0.06, 0.20, 0.07), rotation=(-math.pi / 2.4, 0, 0))

    arm = build_rig("Fox", {
        "Ears": ("Root", (0, -0.24, 0.34), (0, -0.30, 0.44)),
        "Body": ("Root", (0, 0, 0.14), (0, 0, 0.30)),
        "Tail": ("Root", (0, 0.28, 0.20), (0, 0.42, 0.30)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, -18.0), (33, 6.0), (49, -10.0), (65, 0.0)):
            key_rot(arm, "Ears", f, (a, 0, 0))
        for f, a in ((1, 0.0), (17, 14.0), (33, -8.0), (49, 10.0), (65, 0.0)):
            key_rot(arm, "Tail", f, (0, 0, a))

    bake_idle(arm, 65, keys)
    export("fauna_fox.glb")


def gen_goat():
    """Chèvre naine ~0.6 m : corps sur Body, tête sur Head, cornes fixes —
    idle mastication (mâchoire/tête qui hoche) en boucle."""
    fur = mat("fourrure_chevre", FUR_GOAT)
    dark = mat("sombre_chevre", DARK)
    horn = mat("corne_chevre", (0.35, 0.30, 0.22))
    cube("Body", fur, (0, 0, 0.32), (0.18, 0.36, 0.20))
    for x, y in ((-0.11, -0.15), (0.11, -0.15), (-0.11, 0.15), (0.11, 0.15)):
        cube("Body", dark, (x, y, 0.14), (0.045, 0.045, 0.28))
    sphere("Head", fur, (0, -0.36, 0.42), (0.10, 0.11, 0.10))
    sphere("Head", dark, (0, -0.45, 0.40), (0.035, 0.04, 0.03))
    for dx in (-0.05, 0.05):
        cone("Head", horn, (dx, -0.38, 0.52), (0.02, 0.02, 0.10),
             rotation=(0.3, 0, math.copysign(0.1, dx)))

    arm = build_rig("Goat", {
        "Body": ("Root", (0, 0, 0.22), (0, 0, 0.42)),
        "Head": ("Body", (0, -0.36, 0.42), (0, -0.36, 0.55)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (13, 8.0), (25, -3.0), (37, 6.0), (49, 0.0)):
            key_rot(arm, "Head", f, (a, 0, 0))

    bake_idle(arm, 49, keys)
    export("fauna_goat.glb")


def gen_boar():
    """Sanglier ~0.8 m : corps massif sur Body, défenses fixes, queue sur
    Tail qui remue, tête qui fouille le sol (rotation basse) en boucle."""
    fur = mat("fourrure_sanglier", FUR_BOAR)
    dark = mat("sombre_sanglier", DARK)
    tusk_m = mat("defense_sanglier", CREAM)
    cube("Body", fur, (0, 0, 0.30), (0.22, 0.42, 0.22))
    cube("Body", fur, (0, 0.20, 0.28), (0.20, 0.10, 0.22))  # crinière dorsale
    sphere("Head", fur, (0, -0.38, 0.28), (0.13, 0.16, 0.12))
    for dx in (-0.06, 0.06):
        cone("Head", tusk_m, (dx, -0.50, 0.20), (0.015, 0.015, 0.08),
             rotation=(math.pi / 2.5, 0, math.copysign(0.2, dx)))
    for x, y in ((-0.12, -0.16), (0.12, -0.16), (-0.12, 0.16), (0.12, 0.16)):
        cube("Body", dark, (x, y, 0.10), (0.05, 0.05, 0.20))
    cone("Tail", dark, (0, 0.36, 0.30), (0.02, 0.02, 0.14), rotation=(math.pi / 2.8, 0, 0))

    arm = build_rig("Boar", {
        "Body": ("Root", (0, 0, 0.18), (0, 0, 0.40)),
        "Head": ("Body", (0, -0.38, 0.28), (0, -0.55, 0.24)),
        "Tail": ("Root", (0, 0.36, 0.30), (0, 0.48, 0.36)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (15, -12.0), (30, 4.0), (45, -8.0), (61, 0.0)):
            key_rot(arm, "Head", f, (a, 0, 0))
        for f, a in ((1, 0.0), (15, 20.0), (30, -15.0), (45, 12.0), (61, 0.0)):
            key_rot(arm, "Tail", f, (0, 0, a))

    bake_idle(arm, 61, keys)
    export("fauna_boar.glb")


def gen_heron():
    """Héron cendré ~1.1 m au repos, une patte repliée : corps/aile sur Body,
    long cou/tête sur Neck qui scrute lentement — pose d'échassier au bord
    de l'eau."""
    grey = mat("plumage_heron", HERON_GREY)
    white = mat("plumage_clair_heron", HERON_WHITE)
    bill = mat("bec_heron", BILL_ORANGE)
    dark = mat("sombre_heron", DARK)
    cone("Body", grey, (0, 0, 0.55), (0.10, 0.20, 0.22), rotation=(math.pi / 2.2, 0, 0))
    cube("Legs", dark, (0, 0, 0.28), (0.02, 0.02, 0.56))
    cylinder("Neck", grey, (0, -0.10, 0.72), (0.035, 0.035, 0.35),
              rotation=(0.35, 0, 0))
    sphere("Neck", white, (0, -0.24, 0.95), (0.06, 0.07, 0.06))
    cone("Neck", bill, (0, -0.36, 0.97), (0.015, 0.13, 0.015),
         rotation=(math.pi / 2, 0, 0))

    arm = build_rig("Heron", {
        "Legs": ("Root", (0, 0, 0), (0, 0, 0.56)),
        "Body": ("Legs", (0, 0, 0.50), (0, 0.2, 0.60)),
        "Neck": ("Body", (0, -0.08, 0.70), (0, -0.30, 1.0)),
    })

    def keys(arm):
        # Scrutation lente du cou + très léger reste sur une patte, boucle 1 = 121.
        for f, a in ((1, 0.0), (31, 18.0), (61, -10.0), (91, 12.0), (121, 0.0)):
            key_rot(arm, "Neck", f, (0, a, 0))
        for f, a in ((1, 0.0), (61, 1.5), (121, 0.0)):
            key_rot(arm, "Body", f, (a, 0, 0))

    bake_idle(arm, 121, keys)
    export("fauna_heron.glb")


def gen_dragonfly():
    """Libellule ~0.14 m : corps allongé sur Body, 2 paires d'ailes sur
    WingF/WingB qui vibrent en léger déphasage, vol stationnaire."""
    body_m = mat("corps_libellule", DRAGONFLY_BLUE)
    wing_m = mat("aile_libellule", WING_CLEAR)
    cylinder("Body", body_m, (0, 0, 0.25), (0.008, 0.008, 0.11),
              rotation=(math.pi / 2, 0, 0))
    sphere("Body", body_m, (0, 0.06, 0.25), (0.014, 0.014, 0.014))
    cube("WingF", wing_m, (-0.045, 0.02, 0.26), (0.06, 0.003, 0.02))
    cube("WingF", wing_m, (0.045, 0.02, 0.26), (0.06, 0.003, 0.02))
    cube("WingB", wing_m, (-0.045, -0.01, 0.24), (0.05, 0.003, 0.017))
    cube("WingB", wing_m, (0.045, -0.01, 0.24), (0.05, 0.003, 0.017))

    arm = build_rig("Dragonfly", {
        "Body": ("Root", (0, 0, 0.23), (0, 0, 0.30)),
        "WingF": ("Root", (-0.01, 0.02, 0.26), (-0.10, 0.02, 0.26)),
        "WingB": ("Root", (-0.01, -0.01, 0.24), (-0.09, -0.01, 0.24)),
    })

    def keys(arm):
        cycle = [10.0, 65.0]
        for i, f in enumerate(range(1, 50, 3)):
            key_rot(arm, "WingF", f, (0, 0, cycle[i % 2]))
            key_rot(arm, "WingB", f, (0, 0, cycle[(i + 1) % 2]))
        for f, z in ((1, 0.0), (13, 0.015), (25, 0.0), (37, -0.015), (49, 0.0)):
            key_loc(arm, "Body", f, (0, 0, z))

    bake_idle(arm, 49, keys)
    export("fauna_dragonfly.glb")


def gen_ladybug():
    """Coccinelle ~0.05 m posée : corps/élytres sur Body, pattes fixes —
    idle très subtil (respiration + micro-bascule) en boucle courte."""
    red = mat("elytre_coccinelle", LADYBUG_RED)
    dark = mat("sombre_coccinelle", DARK)
    sphere("Body", red, (0, 0, 0.022), (0.025, 0.03, 0.02))
    sphere("Body", dark, (0, -0.024, 0.022), (0.010, 0.010, 0.010))
    for dx, dy in ((-0.012, -0.006), (0.012, -0.006), (0, 0.01)):
        sphere("Body", dark, (dx, dy, 0.033), (0.003, 0.003, 0.003))

    arm = build_rig("Ladybug", {
        "Body": ("Root", (0, 0, 0.005), (0, 0, 0.04)),
    })

    def keys(arm):
        for f, s in ((1, 1.0), (13, 1.06), (25, 0.97), (37, 1.03), (49, 1.0)):
            key_scale(arm, "Body", f, (s, s, 2.0 - s))

    bake_idle(arm, 49, keys)
    export("fauna_ladybug.glb")


def gen_turtle():
    """Tortue ~0.35 m : carapace sur Body (solide, socle bas), tête sur Head
    qui sort/rentre lentement — idle très lent, boucle longue."""
    shell = mat("carapace_tortue", SHELL_GREEN)
    shell_dark = mat("carapace_sombre_tortue", SHELL_DARK)
    skin = mat("peau_tortue", (0.42, 0.48, 0.30))
    sphere("Body", shell, (0, 0, 0.12), (0.17, 0.20, 0.11))
    sphere("Body", shell_dark, (0, 0, 0.16), (0.10, 0.12, 0.06))
    for x, y in ((-0.10, -0.10), (0.10, -0.10), (-0.10, 0.10), (0.10, 0.10)):
        cube("Body", skin, (x, y, 0.03), (0.04, 0.05, 0.05))
    sphere("Head", skin, (0, -0.20, 0.10), (0.045, 0.06, 0.045))

    arm = build_rig("Turtle", {
        "Body": ("Root", (0, 0, 0.03), (0, 0, 0.20)),
        "Head": ("Body", (0, -0.16, 0.10), (0, -0.28, 0.10)),
    })

    def keys(arm):
        # Boucle très lente 1 = 145 : la tête sort, marque une pause, rentre.
        for f, y in ((1, 0.0), (36, 0.06), (72, 0.06), (108, 0.0), (145, 0.0)):
            key_loc(arm, "Head", f, (0, y, 0))

    bake_idle(arm, 145, keys)
    export("fauna_turtle.glb")


def gen_raccoon():
    """Raton laveur ~0.4 m assis : corps/tête sur Body, queue annelée sur
    Tail — idle qui « lave » ses pattes avant (petits hochements) en boucle."""
    fur = mat("fourrure_raton", FUR_RACCOON)
    dark = mat("masque_raton", DARK)
    cream = mat("clair_raton", CREAM)
    sphere("Body", fur, (0, 0, 0.14), (0.12, 0.14, 0.15))
    sphere("Body", fur, (0, -0.10, 0.24), (0.08, 0.08, 0.08))
    cube("Body", dark, (-0.03, -0.15, 0.25), (0.025, 0.02, 0.02))
    cube("Body", dark, (0.03, -0.15, 0.25), (0.025, 0.02, 0.02))
    for x, y in ((-0.07, 0.02), (0.07, 0.02)):
        cube("Paws", cream, (x, y, 0.06), (0.04, 0.05, 0.04))
    cone("Tail", fur, (0, 0.16, 0.14), (0.05, 0.05, 0.24), rotation=(-math.pi / 2.3, 0, 0))

    arm = build_rig("Raccoon", {
        "Body": ("Root", (0, 0, 0.02), (0, 0, 0.24)),
        "Paws": ("Root", (0, 0.02, 0.02), (0, 0.02, 0.10)),
        "Tail": ("Root", (0, 0.16, 0.14), (0, 0.30, 0.22)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (11, -14.0), (22, 4.0), (33, -8.0), (45, 0.0)):
            key_rot(arm, "Paws", f, (a, 0, 0))
        for f, a in ((1, 0.0), (11, 6.0), (22, -6.0), (33, 4.0), (45, 0.0)):
            key_rot(arm, "Tail", f, (a, 0, 0))

    bake_idle(arm, 45, keys)
    export("fauna_raccoon.glb")


def gen_crow():
    """Corbeau ~0.30 m perché : corps sur Body, tête sur Head qui pivote,
    ailes croisées fixes — idle vigilant, hochements de tête caractéristiques."""
    black = mat("plumage_corbeau", CROW_BLACK)
    bill = mat("bec_corbeau", (0.10, 0.09, 0.08))
    sphere("Body", black, (0, 0, 0.14), (0.08, 0.11, 0.10))
    sphere("Head", black, (0, -0.09, 0.22), (0.055, 0.06, 0.055))
    cone("Head", bill, (0, -0.16, 0.21), (0.014, 0.06, 0.014), rotation=(math.pi / 2, 0, 0))
    cube("Body", black, (-0.09, 0.02, 0.13), (0.02, 0.10, 0.06), rotation=(0, 0, 0.15))
    cube("Body", black, (0.09, 0.02, 0.13), (0.02, 0.10, 0.06), rotation=(0, 0, -0.15))

    arm = build_rig("Crow", {
        "Body": ("Root", (0, 0, 0.04), (0, 0, 0.20)),
        "Head": ("Body", (0, -0.08, 0.22), (0, -0.18, 0.22)),
    })

    def keys(arm):
        # Hochements rapides caractéristiques + rotation lente, boucle 1 = 73.
        for f, a in ((1, 0.0), (9, 20.0), (17, 0.0), (49, 0.0), (57, -25.0), (65, 0.0), (73, 0.0)):
            key_rot(arm, "Head", f, (0, a, 0))

    bake_idle(arm, 73, keys)
    export("fauna_crow.glb")


def gen_goose():
    """Oie ~0.55 m : corps/cou/tête sur Body, ailes croisées sur Wings —
    dandinement + cou qui s'étire pour siffler, idle boucle."""
    grey = mat("plumage_oie", GOOSE_GREY)
    white = mat("plumage_clair_oie", CREAM)
    bill = mat("bec_oie", BILL_ORANGE)
    sphere("Body", grey, (0, 0, 0.24), (0.15, 0.24, 0.16))
    sphere("Body", white, (0, 0.05, 0.15), (0.10, 0.14, 0.09))
    cylinder("Body", grey, (0, -0.24, 0.34), (0.045, 0.045, 0.28),
              rotation=(0.5, 0, 0))
    sphere("Body", grey, (0, -0.42, 0.50), (0.06, 0.07, 0.06))
    cone("Body", bill, (0, -0.53, 0.49), (0.02, 0.08, 0.02), rotation=(math.pi / 2, 0, 0))
    cube("Wings", grey, (-0.16, 0.0, 0.24), (0.03, 0.14, 0.16))
    cube("Wings", grey, (0.16, 0.0, 0.24), (0.03, 0.14, 0.16))

    arm = build_rig("Goose", {
        "Body": ("Root", (0, -0.05, 0.10), (0, -0.05, 0.30)),
        "Wings": ("Body", (0, 0.0, 0.24), (0, 0.0, 0.36)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, 7.0), (33, 0.0), (49, -7.0), (65, 0.0)):
            key_rot(arm, "Body", f, (a, 0, 0))
        for f, s in ((1, 1.0), (57, 1.0), (61, 1.1), (65, 1.0)):
            key_scale(arm, "Wings", f, (s, s, 1.0))

    bake_idle(arm, 65, keys)
    export("fauna_goose.glb")


# ---------------------------------------------------------------------------
# Décor animé (10)
# ---------------------------------------------------------------------------


def gen_windsock():
    """Manche à air ~2.2 m sur mât : mât/anneau sur Root (solide), tube
    orange rayé sur Sock qui flotte et se gonfle au vent en boucle."""
    dark = mat("mat_windsock", METAL_DARK)
    orange = mat("tissu_windsock_o", (0.85, 0.42, 0.08))
    white = mat("tissu_windsock_b", CLOTH_WHITE)
    cylinder("Root", dark, (0, 0, 1.1), (0.04, 0.04, 2.2), vertices=8)
    ring_m = mat("anneau_windsock", METAL)
    cylinder("Root", ring_m, (0, 0, 2.15), (0.14, 0.14, 0.03), vertices=12)
    for i in range(3):
        col = orange if i % 2 == 0 else white
        cone(f"Sock", col, (0, 0.14 + i * 0.10, 2.10 - i * 0.02), (0.13 - i * 0.02, 0.13 - i * 0.02, 0.11),
             rotation=(math.pi / 2, 0, 0))

    arm = build_rig("Windsock", {
        "Sock": ("Root", (0, 0.05, 2.15), (0, 0.45, 2.10)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, 12.0), (33, 4.0), (49, 15.0), (65, 0.0)):
            key_rot(arm, "Sock", f, (a, 0, a * 0.3))

    bake_idle(arm, 65, keys)
    export("nature_windsock.glb")


def gen_sundial():
    """Cadran solaire ~0.9 m : socle de pierre + cadran (Root, solide),
    gnomon métallique sur Gnomon — idle très lent, léger scintillement
    (rotation minime, ombre symbolique) en boucle."""
    stone = mat("pierre_cadran", STONE_LIGHT)
    brass = mat("laiton_cadran", BRASS)
    cylinder("Root", stone, (0, 0, 0.45), (0.35, 0.35, 0.9), vertices=10)
    cylinder("Root", brass, (0, 0, 0.92), (0.32, 0.32, 0.03), vertices=16)
    cube("Gnomon", brass, (0, 0, 0.98), (0.02, 0.24, 0.16))

    arm = build_rig("Sundial", {
        "Gnomon": ("Root", (0, 0, 0.94), (0, 0, 1.14)),
    })

    def keys(arm):
        for f, s in ((1, 1.0), (61, 1.03), (121, 1.0)):
            key_scale(arm, "Gnomon", f, (s, s, s))

    bake_idle(arm, 121, keys)
    export("nature_sundial.glb")


def gen_birdhouse():
    """Nichoir ~1.8 m sur poteau : poteau sur Root (solide), maisonnette sur
    House qui se balance légèrement au vent (fixée par une cordelette), un
    oiseau minuscule sur Perch qui hoche la tête en boucle."""
    dark = mat("bois_nichoir", WOOD_DARK)
    roof_m = mat("toit_nichoir", (0.55, 0.20, 0.14))
    bird_m = mat("oiseau_nichoir", (0.75, 0.35, 0.10))
    cylinder("Root", dark, (0, 0, 0.8), (0.05, 0.05, 1.6), vertices=8)
    house_m = mat("bois_clair_nichoir", WOOD)
    cube("House", house_m, (0, 0, 1.65), (0.30, 0.28, 0.30))
    cone("House", roof_m, (0, 0, 1.85), (0.24, 0.22, 0.16), vertices=4, rotation=(0, 0, math.pi / 4))
    cylinder("House", dark, (0, 0.15, 1.55), (0.015, 0.015, 0.10), rotation=(math.pi / 2, 0, 0))  # perchoir
    sphere("Perch", bird_m, (0.10, 0.15, 1.58), (0.03, 0.035, 0.03))

    arm = build_rig("Birdhouse", {
        "House": ("Root", (0, 0, 1.55), (0, 0, 1.95)),
        "Perch": ("House", (0.10, 0.15, 1.58), (0.14, 0.15, 1.58)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (23, 3.0), (45, -2.0), (67, 2.0), (89, 0.0)):
            key_rot(arm, "House", f, (0, a, 0))
        for f, a in ((1, 0.0), (11, 20.0), (22, 0.0), (67, 0.0), (78, -15.0), (89, 0.0)):
            key_rot(arm, "Perch", f, (0, a, 0))

    bake_idle(arm, 89, keys)
    export("nature_birdhouse.glb")


def gen_fountain():
    """Petite fontaine de pierre ~1.4 m : vasques sur Root (solide), jet
    d'eau central sur Jet qui pulse verticalement en boucle, non solide au
    niveau du jet."""
    stone = mat("pierre_fontaine", STONE)
    water = mat("eau_fontaine", WATER_BLUE)
    cylinder("Root", stone, (0, 0, 0.20), (0.65, 0.65, 0.40), vertices=14)
    cylinder("Root", stone, (0, 0, 0.55), (0.10, 0.10, 0.30), vertices=10)
    cylinder("Root", stone, (0, 0, 0.75), (0.30, 0.30, 0.10), vertices=14)
    cylinder("Jet", water, (0, 0, 0.85), (0.03, 0.03, 0.25), vertices=8)
    sphere("Jet", water, (0, 0, 1.05), (0.05, 0.05, 0.05))

    arm = build_rig("Fountain", {
        "Jet": ("Root", (0, 0, 0.80), (0, 0, 1.10)),
    })

    def keys(arm):
        for f, s, z in ((1, 1.0, 0.0), (13, 1.4, 0.05), (25, 0.8, -0.02), (37, 1.3, 0.04), (49, 1.0, 0.0)):
            key_scale(arm, "Jet", f, (1, 1, s))
            key_loc(arm, "Jet", f, (0, 0, z))

    bake_idle(arm, 49, keys)
    export("nature_fountain.glb")


def gen_wheelbarrow():
    """Brouette ~1.0 m : caisse/brancards sur Root (solide, posée), roue sur
    Wheel qui tourne doucement (comme heurtée par le vent) en boucle."""
    wood = mat("bois_brouette", WOOD)
    dark = mat("bois_sombre_brouette", WOOD_DARK)
    metal_m = mat("metal_brouette", METAL)
    cube("Root", wood, (0, 0.10, 0.35), (0.55, 0.75, 0.28))
    for side in (-1, 1):
        cube("Root", dark, (side * 0.30, 0.35, 0.22), (0.05, 1.1, 0.05))
    cylinder("Wheel", metal_m, (0, -0.55, 0.20), (0.20, 0.20, 0.05),
              rotation=(math.pi / 2, 0, 0), vertices=12)
    cylinder("Axle", dark, (0, -0.55, 0.20), (0.02, 0.02, 0.14), rotation=(math.pi / 2, 0, 0))

    arm = build_rig("Wheelbarrow", {
        "Wheel": ("Root", (0, -0.55, 0.20), (0, -0.75, 0.20)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (25, 25.0), (49, -10.0), (73, 15.0), (97, 0.0)):
            key_rot(arm, "Wheel", f, (a, 0, 0))

    bake_idle(arm, 97, keys)
    export("nature_wheelbarrow.glb")


def gen_hay_roller():
    """Botte de foin ronde ~1.2 m qui roule doucement sur place (oscillation,
    pas un vrai déplacement) — Root = socle non solide (posée dans un champ
    fauché), Bale = la botte qui tourne."""
    hay_m = mat("foin_botte", HAY)
    hay_dark = mat("foin_sombre_botte", (0.55, 0.44, 0.16))
    cylinder("Bale", hay_m, (0, 0, 0.60), (0.60, 0.60, 1.0),
              rotation=(math.pi / 2, 0, 0), vertices=16)
    for a in (0.4, 1.6, 2.7):
        cylinder("Bale", hay_dark, (0.62 * math.cos(a), 0.62 * math.sin(a), 0.60),
                  (0.03, 0.03, 0.60), rotation=(math.pi / 2, 0, a), vertices=6)

    arm = build_rig("HayRoller", {
        "Bale": ("Root", (0, 0, 0.60), (0, 0.7, 0.60)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (25, 12.0), (49, 0.0), (73, -12.0), (97, 0.0)):
            key_rot(arm, "Bale", f, (a, 0, 0))

    bake_idle(arm, 97, keys)
    export("nature_hay_roller.glb")


def gen_grindstone():
    """Meule à aiguiser à pédale ~1.1 m : bâti sur Root (solide), disque de
    pierre sur Wheel qui tourne en continu en boucle."""
    dark = mat("bois_meule", WOOD_DARK)
    stone = mat("pierre_meule", STONE)
    metal_m = mat("metal_meule", METAL_DARK)
    cube("Root", dark, (0, 0, 0.35), (0.14, 0.55, 0.70))
    cylinder("Wheel", stone, (0, 0.05, 0.70), (0.32, 0.32, 0.10),
              rotation=(math.pi / 2, 0, 0), vertices=16)
    cylinder("Axle", metal_m, (0, 0.05, 0.70), (0.02, 0.02, 0.16), rotation=(math.pi / 2, 0, 0))

    arm = build_rig("Grindstone", {
        "Wheel": ("Root", (0, 0.05, 0.70), (0, 0.30, 0.70)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (13, 90.0), (25, 180.0), (37, 270.0), (49, 360.0)):
            key_rot(arm, "Wheel", f, (a, 0, 0))

    bake_idle(arm, 49, keys)
    export("nature_grindstone.glb")


def gen_potters_wheel():
    """Tour de potier ~0.7 m : bâti/siège sur Root (solide), plateau sur
    Wheel qui tourne en continu, argile en cours de façonnage fixe dessus."""
    dark = mat("bois_tour", WOOD_DARK)
    stone = mat("plateau_tour", STONE)
    clay = mat("argile_tour", CLAY)
    cylinder("Root", dark, (0, 0, 0.25), (0.20, 0.20, 0.50), vertices=10)
    cylinder("Wheel", stone, (0, 0, 0.52), (0.28, 0.28, 0.04), vertices=16)
    cone("Wheel", clay, (0, 0, 0.60), (0.10, 0.10, 0.16), vertices=10)

    arm = build_rig("PottersWheel", {
        "Wheel": ("Root", (0, 0, 0.52), (0, 0, 0.70)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, 130.0), (33, 260.0), (49, 360.0)):
            key_rot(arm, "Wheel", f, (0, 0, a))

    bake_idle(arm, 49, keys)
    export("nature_potters_wheel.glb")


def gen_mast_flag():
    """Petit mât à pavillon ~2.0 m (quai/hameau côtier) : mât sur Root
    (solide), fanion triangulaire sur Flag qui flotte au vent en boucle."""
    dark = mat("mat_pavillon", WOOD_DARK)
    cloth = mat("tissu_pavillon", CLOTH_RED)
    cylinder("Root", dark, (0, 0, 1.0), (0.045, 0.045, 2.0), vertices=8)
    cube("Flag", cloth, (0.28, 0, 1.85), (0.55, 0.02, 0.28))

    arm = build_rig("MastFlag", {
        "Flag": ("Root", (0.02, 0, 1.90), (0.55, 0, 1.90)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, 18.0), (33, 0.0), (49, -18.0), (65, 0.0)):
            key_rot(arm, "Flag", f, (0, 0, a))

    bake_idle(arm, 65, keys)
    export("nature_mast_flag.glb")


def gen_swing_bench():
    """Balancelle de porche ~1.8 m : portique sur Root (solide), banquette
    suspendue sur Seat qui se balance doucement en boucle."""
    dark = mat("bois_portique", WOOD_DARK)
    wood = mat("bois_banquette", WOOD)
    metal_m = mat("chaine_balancelle", METAL_DARK)
    cylinder("Root", dark, (-0.55, -0.35, 0.9), (0.05, 0.05, 1.8), vertices=8)
    cylinder("Root", dark, (-0.55, 0.35, 0.9), (0.05, 0.05, 1.8), vertices=8)
    cylinder("Root", dark, (0.55, -0.35, 0.9), (0.05, 0.05, 1.8), vertices=8)
    cylinder("Root", dark, (0.55, 0.35, 0.9), (0.05, 0.05, 1.8), vertices=8)
    cube("Root", dark, (0, -0.35, 1.78), (1.20, 0.08, 0.08))
    cube("Root", dark, (0, 0.35, 1.78), (1.20, 0.08, 0.08))
    for x in (-0.5, 0.5):
        cylinder("Chain", metal_m, (x, -0.30, 1.35), (0.01, 0.01, 0.75), rotation=(math.pi / 2, 0, 0))
        cylinder("Chain", metal_m, (x, 0.30, 1.35), (0.01, 0.01, 0.75), rotation=(math.pi / 2, 0, 0))
    cube("Seat", wood, (0, 0, 0.95), (1.0, 0.65, 0.10))
    cube("Seat", wood, (0, -0.30, 1.20), (1.0, 0.06, 0.55))

    arm = build_rig("SwingBench", {
        "Chain": ("Root", (0, 0, 1.75), (0, 0, 1.40)),
        "Seat": ("Chain", (0, 0, 1.40), (0, 0, 0.95)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (25, 7.0), (49, 0.0), (73, -7.0), (97, 0.0)):
            key_rot(arm, "Chain", f, (0, a, 0))

    bake_idle(arm, 97, keys)
    export("nature_swing_bench.glb")


ASSETS = [
    gen_fox,
    gen_goat,
    gen_boar,
    gen_heron,
    gen_dragonfly,
    gen_ladybug,
    gen_turtle,
    gen_raccoon,
    gen_crow,
    gen_goose,
    gen_windsock,
    gen_sundial,
    gen_birdhouse,
    gen_fountain,
    gen_wheelbarrow,
    gen_hay_roller,
    gen_grindstone,
    gen_potters_wheel,
    gen_mast_flag,
    gen_swing_bench,
]

for gen in ASSETS:
    reset_scene()
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[fauna-decor-3] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
