# Complète le pack « nature » avec 15 assets animés supplémentaires (8 petite
# faune ambiante + 7 décor) en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_fauna_decor_pack.py
#
# Sortie : assets/models/fauna_*.glb (8) et assets/models/nature_*.glb (7,
# noms distincts du pack statique gen_nature_pack.py — aucune collision de
# fichier). Ce script ne touche à rien côté Rust : il ne fait que produire les
# .glb, à charger/placer plus tard à la main dans une scène si besoin.
#
# Même recette que gen_nature_animated.py (rig minuscule, vertex group plein
# par partie, clip « Idle » en piste NLA, pose résiduelle purgée avant export) :
# - base au sol z=0 Blender (= y=0 jeu), Blender +Y → -Z jeu (face « avant »).
# - échelle réelle (comme le pack statique) : ces assets sont posés à
#   l'échelle 1.0 dans une scène, contrairement aux créatures jouables
#   (gen_creature2.py) réduites à 0.35 par le moteur.
# - interpolation LINEAR par défaut pour des boucles régulières (pas de
#   respiration Bézier parasite sur les rotations continues).

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

# Palette faune — cohérente avec la palette bois/pierre du décor (gen_nature_pack.py).
FUR_BROWN = (0.55, 0.36, 0.18)
FUR_RUST = (0.80, 0.36, 0.12)
FUR_GREY = (0.55, 0.54, 0.52)
CREAM = (0.93, 0.86, 0.70)
DARK = (0.14, 0.10, 0.08)
GREEN_SCALE = (0.28, 0.52, 0.24)
FEATHER_WHITE = (0.92, 0.90, 0.85)
FEATHER_ORANGE = (0.85, 0.48, 0.10)
WING_ORANGE = (0.92, 0.42, 0.08)
WING_BLACK = (0.10, 0.09, 0.10)
FISH_SILVER = (0.68, 0.74, 0.80)
FISH_FIN = (0.85, 0.35, 0.25)
OWL_BROWN = (0.42, 0.30, 0.18)
OWL_CREAM = (0.85, 0.78, 0.62)

# Palette décor — reprise de gen_nature_pack.py / gen_nature_animated.py.
BROWN = (0.32, 0.22, 0.11)
WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
STONE = (0.45, 0.44, 0.42)
METAL = (0.32, 0.30, 0.29)
METAL_DARK = (0.20, 0.19, 0.18)
GLOW_YELLOW = (1.0, 0.78, 0.35)
GLASS_AMBER = (0.85, 0.55, 0.20)
CLOTH_RED = (0.72, 0.14, 0.12)
CLOTH_WHITE = (0.88, 0.85, 0.78)
CLOTH_BLUE = (0.18, 0.30, 0.55)
CLOTH_GREEN = (0.20, 0.45, 0.22)
LEAF = (0.22, 0.42, 0.18)
LEAF_DARK = (0.16, 0.33, 0.14)
REED_GREEN = (0.35, 0.48, 0.20)
BOAT_HULL = (0.40, 0.26, 0.13)
WATER_DARK = (0.12, 0.22, 0.28)


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
    print(f"[fauna-decor] exporté {filename}")


# ---------------------------------------------------------------------------
# Faune ambiante (8) — petite échelle réelle, face -Y Blender = avant jeu.
# ---------------------------------------------------------------------------


def gen_rabbit():
    """Lapin ~0.35 m : corps + tête sur Body, oreilles sur Ears, pattes
    arrière sur Legs — hop vertical + oreilles qui suivent en boucle."""
    fur = mat("fourrure_lapin", FUR_BROWN)
    cream = mat("creme_lapin", CREAM)
    dark = mat("sombre_lapin", DARK)
    sphere("Body", fur, (0, 0, 0.16), (0.20, 0.26, 0.17))
    sphere("Body", fur, (0, -0.20, 0.20), (0.13, 0.14, 0.13))  # tête
    sphere("Body", cream, (0, -0.28, 0.16), (0.05, 0.06, 0.045))  # museau
    cone("Ears", cream, (-0.05, -0.20, 0.30), (0.035, 0.03, 0.22), rotation=(-0.15, 0, 0.08))
    cone("Ears", cream, (0.05, -0.20, 0.30), (0.035, 0.03, 0.22), rotation=(-0.15, 0, -0.08))
    sphere("Body", cream, (0, 0.20, 0.10), (0.06, 0.06, 0.06))  # queue
    for x in (-0.10, 0.10):
        cube("Legs", dark, (x, 0.02, 0.03), (0.06, 0.14, 0.06))

    arm = build_rig("Rabbit", {
        "Ears": ("Root", (0, -0.20, 0.28), (0, -0.20, 0.50)),
        "Body": ("Root", (0, 0, 0.05), (0, 0, 0.25)),
        "Legs": ("Root", (0, 0.02, 0), (0, 0.02, 0.10)),
    })

    def keys(arm):
        # Boucle de saut (frame 1 = frame 49) : le corps monte, les oreilles
        # suivent avec un léger retard/rebond, les pattes se replient.
        for f, z in ((1, 0.0), (13, 0.10), (25, 0.0), (37, -0.02), (49, 0.0)):
            key_loc(arm, "Body", f, (0, 0, z))
        for f, ang in ((1, 0.0), (13, -18.0), (25, 6.0), (37, -4.0), (49, 0.0)):
            key_rot(arm, "Ears", f, (ang, 0, 0))
        for f, z in ((1, 0.0), (13, -0.03), (25, 0.0), (37, 0.01), (49, 0.0)):
            key_loc(arm, "Legs", f, (0, 0, z))

    bake_idle(arm, 49, keys)
    export("fauna_rabbit.glb")


def gen_butterfly():
    """Papillon ~0.16 m d'envergure : corps sur Body, deux ailes sur
    WingL/WingR qui battent et un vol flottant (bob + drift) en boucle."""
    body_m = mat("corps_papillon", DARK)
    wing_o = mat("aile_orange", WING_ORANGE)
    wing_b = mat("aile_noire", WING_BLACK)
    cylinder("Body", body_m, (0, 0, 0.30), (0.012, 0.012, 0.09),
              rotation=(math.pi / 2, 0, 0))
    cube("WingL", wing_o, (-0.05, 0, 0.30), (0.09, 0.005, 0.06))
    cube("WingL", wing_b, (-0.10, 0, 0.30), (0.02, 0.005, 0.025))
    cube("WingR", wing_o, (0.05, 0, 0.30), (0.09, 0.005, 0.06))
    cube("WingR", wing_b, (0.10, 0, 0.30), (0.02, 0.005, 0.025))

    arm = build_rig("Butterfly", {
        "Body": ("Root", (0, 0, 0.28), (0, 0, 0.34)),
        "WingL": ("Root", (-0.01, 0, 0.30), (-0.14, 0, 0.30)),
        "WingR": ("Root", (0.01, 0, 0.30), (0.14, 0, 0.30)),
    })

    def keys(arm):
        # Battement rapide (frame 1 = frame 13) superposé à une dérive lente
        # de la caisse (frame 1 = frame 49) — deux boucles imbriquées dont le
        # PPCM (49) donne la durée totale exportée.
        cycle = [8.0, 70.0, 8.0, 70.0]
        for i, f in enumerate(range(1, 50, 3)):
            ang = cycle[i % len(cycle)]
            key_rot(arm, "WingL", f, (0, 0, ang))
            key_rot(arm, "WingR", f, (0, 0, -ang))
        drift = ((1, 0.0, 0.0), (13, 0.05, 0.03), (25, 0.0, 0.06),
                 (37, -0.05, 0.03), (49, 0.0, 0.0))
        for f, x, z in drift:
            key_loc(arm, "Body", f, (x, 0, z))

    bake_idle(arm, 49, keys)
    export("fauna_butterfly.glb")


def gen_fish():
    """Poisson ~0.28 m : corps sur Body, queue sur Tail qui ondule — nage sur
    place en boucle, pour un étang/une rivière peu profonde."""
    silver = mat("ecailles", FISH_SILVER)
    fin_m = mat("nageoire", FISH_FIN)
    sphere("Body", silver, (0, 0, 0.10), (0.06, 0.14, 0.07))
    cone("Body", fin_m, (0, 0.02, 0.16), (0.03, 0.05, 0.02), rotation=(math.pi / 2, 0, 0))  # dorsale
    cone("Tail", silver, (0, -0.16, 0.10), (0.05, 0.10, 0.06), rotation=(-math.pi / 2, 0, 0))

    arm = build_rig("Fish", {
        "Body": ("Root", (0, 0.05, 0.10), (0, 0.14, 0.10)),
        "Tail": ("Body", (0, -0.05, 0.10), (0, -0.20, 0.10)),
    })

    def keys(arm):
        # Ondulation queue + léger virage du corps, boucle parfaite 1 = 41.
        for f, a in ((1, 0.0), (11, 22.0), (21, 0.0), (31, -22.0), (41, 0.0)):
            key_rot(arm, "Tail", f, (0, 0, a))
        for f, a in ((1, 0.0), (11, 6.0), (21, 0.0), (31, -6.0), (41, 0.0)):
            key_rot(arm, "Body", f, (0, 0, a))

    bake_idle(arm, 41, keys)
    export("fauna_fish.glb")


def gen_frog():
    """Grenouille ~0.18 m : corps/tête sur Body, pattes arrière sur Legs —
    respiration marquée (gorge qui gonfle) + petit hop occasionnel simulé par
    un tassement/détente en boucle."""
    green = mat("peau_grenouille", GREEN_SCALE)
    cream = mat("ventre_grenouille", CREAM)
    dark = mat("sombre_grenouille", DARK)
    sphere("Body", green, (0, 0, 0.09), (0.11, 0.14, 0.09))
    sphere("Body", cream, (0, -0.02, 0.05), (0.08, 0.09, 0.05))  # ventre
    sphere("Body", green, (-0.05, -0.10, 0.12), (0.035, 0.035, 0.035))  # oeil G
    sphere("Body", green, (0.05, -0.10, 0.12), (0.035, 0.035, 0.035))  # oeil D
    for x in (-0.09, 0.09):
        cube("Legs", dark, (x, 0.06, 0.03), (0.06, 0.12, 0.05))

    arm = build_rig("Frog", {
        "Body": ("Root", (0, 0, 0.02), (0, 0, 0.16)),
        "Legs": ("Root", (0, 0.06, 0), (0, 0.06, 0.08)),
    })

    def keys(arm):
        # Boucle 1 = 61 : tassement/détente lent (respiration) puis un
        # arrondi plus marqué à mi-parcours (semblant d'un petit hop sur place).
        for f, s in ((1, 1.0), (15, 1.08), (30, 0.92), (45, 1.15), (61, 1.0)):
            key_scale(arm, "Body", f, (s, s, 2.0 - s))
        for f, z in ((1, 0.0), (15, 0.01), (30, -0.01), (45, 0.03), (61, 0.0)):
            key_loc(arm, "Body", f, (0, 0, z))

    bake_idle(arm, 61, keys)
    export("fauna_frog.glb")


def gen_duck():
    """Canard ~0.30 m : corps/tête sur Body, queue sur Tail — dandinement +
    battement de queue en boucle, pose au sol ou flottant."""
    white = mat("plumage_canard", FEATHER_WHITE)
    orange = mat("bec_patte", FEATHER_ORANGE)
    dark = mat("oeil_canard", DARK)
    sphere("Body", white, (0, 0, 0.15), (0.14, 0.22, 0.14))
    sphere("Body", white, (0, -0.22, 0.22), (0.09, 0.10, 0.09))  # tête
    cone("Body", orange, (0, -0.31, 0.20), (0.03, 0.07, 0.03), rotation=(math.pi / 2, 0, 0))  # bec
    sphere("Body", dark, (0, -0.27, 0.25), (0.015, 0.015, 0.015))
    cone("Tail", white, (0, 0.20, 0.18), (0.06, 0.10, 0.06), rotation=(-math.pi / 2, 0, 0))
    for x in (-0.07, 0.07):
        cube("Legs", orange, (x, -0.02, 0.03), (0.03, 0.08, 0.06))

    arm = build_rig("Duck", {
        "Body": ("Root", (0, -0.02, 0.06), (0, -0.02, 0.26)),
        "Tail": ("Body", (0, 0.10, 0.18), (0, 0.28, 0.18)),
        "Legs": ("Root", (0, -0.02, 0), (0, -0.02, 0.08)),
    })

    def keys(arm):
        # Dandinement latéral (roulis) + queue qui suit, boucle 1 = 57.
        for f, a in ((1, 0.0), (15, 8.0), (29, 0.0), (43, -8.0), (57, 0.0)):
            key_rot(arm, "Body", f, (a, 0, 0))
        for f, a in ((1, 0.0), (15, -14.0), (29, 4.0), (43, -10.0), (57, 0.0)):
            key_rot(arm, "Tail", f, (0, 0, a))

    bake_idle(arm, 57, keys)
    export("fauna_duck.glb")


def gen_deer():
    """Faon des bois ~0.75 m au garrot : corps sur Body, oreilles sur Ears,
    queue sur Tail — idle vigilant (oreilles/queue qui frémissent, tête qui
    balaie doucement) en boucle."""
    fur = mat("fourrure_faon", FUR_BROWN)
    cream = mat("tacheté_faon", CREAM)
    dark = mat("sombre_faon", DARK)
    cube("Body", fur, (0, 0, 0.42), (0.20, 0.42, 0.20))
    sphere("Body", fur, (0, -0.42, 0.52), (0.13, 0.16, 0.13))  # tête/cou
    sphere("Body", cream, (0, -0.50, 0.48), (0.05, 0.05, 0.045))  # museau
    for dx, dz in ((-0.10, 0.06), (0.10, 0.06)):
        cone("Ears", cream, (dx, -0.40, 0.62 + dz), (0.045, 0.03, 0.13),
             rotation=(-0.3, 0, math.copysign(0.3, dx)))
    for x, y in ((-0.10, -0.18), (0.10, -0.18), (-0.10, 0.18), (0.10, 0.18)):
        cube("Body", dark, (x, y, 0.18), (0.05, 0.05, 0.36))
    cone("Tail", cream, (0, 0.42, 0.46), (0.04, 0.06, 0.08), rotation=(-math.pi / 2, 0, 0))

    arm = build_rig("Deer", {
        "Body": ("Root", (0, 0, 0.20), (0, 0, 0.58)),
        "Ears": ("Root", (0, -0.40, 0.60), (0, -0.55, 0.68)),
        "Tail": ("Root", (0, 0.42, 0.46), (0, 0.55, 0.46)),
    })

    def keys(arm):
        # Boucle 1 = 97 : tête qui balaie lentement (vigilance), oreilles qui
        # pivotent en écoute, queue qui frémit brièvement en fin de cycle.
        for f, a in ((1, 0.0), (25, 6.0), (49, -4.0), (73, 5.0), (97, 0.0)):
            key_rot(arm, "Body", f, (0, 0, a))
        for f, a in ((1, 0.0), (25, -20.0), (49, 10.0), (73, -15.0), (97, 0.0)):
            key_rot(arm, "Ears", f, (a, 0, 0))
        for f, a in ((1, 0.0), (85, 0.0), (89, 25.0), (93, -15.0), (97, 0.0)):
            key_rot(arm, "Tail", f, (a, 0, 0))

    bake_idle(arm, 97, keys)
    export("fauna_deer.glb")


def gen_squirrel():
    """Écureuil ~0.25 m assis : corps/tête sur Body, grande queue touffue sur
    Tail qui frémit — idle « grignotage » (petits hochements) en boucle."""
    fur = mat("fourrure_ecureuil", FUR_RUST)
    cream = mat("ventre_ecureuil", CREAM)
    dark = mat("sombre_ecureuil", DARK)
    sphere("Body", fur, (0, 0, 0.10), (0.08, 0.09, 0.11))
    sphere("Body", cream, (0, -0.03, 0.08), (0.055, 0.06, 0.07))  # ventre
    sphere("Body", fur, (0, -0.02, 0.20), (0.055, 0.06, 0.055))  # tête
    sphere("Body", cream, (0, -0.06, 0.19), (0.025, 0.025, 0.02))  # museau
    cone("Tail", fur, (0, 0.10, 0.20), (0.06, 0.09, 0.22), rotation=(-0.5, 0, 0))

    arm = build_rig("Squirrel", {
        "Body": ("Root", (0, 0, 0.03), (0, 0, 0.24)),
        "Tail": ("Root", (0, 0.10, 0.10), (0, 0.14, 0.34)),
    })

    def keys(arm):
        # Boucle 1 = 45 : petits hochements du corps (grignotage) + queue qui
        # frémit avec un léger déphasage.
        for f, a in ((1, 0.0), (11, 10.0), (22, -4.0), (33, 8.0), (45, 0.0)):
            key_rot(arm, "Body", f, (a, 0, 0))
        for f, a in ((1, 0.0), (11, -6.0), (22, 8.0), (33, -5.0), (45, 0.0)):
            key_rot(arm, "Tail", f, (a, 0, 0))

    bake_idle(arm, 45, keys)
    export("fauna_squirrel.glb")


def gen_owl():
    """Chouette ~0.30 m perchée : corps sur Body, tête sur Head (rotation
    élargie façon hibou), ailes sur Wings — idle immobile ponctué d'une
    rotation de tête et d'un ébouriffage d'ailes, en boucle."""
    brown = mat("plumage_hibou", OWL_BROWN)
    cream = mat("face_hibou", OWL_CREAM)
    dark = mat("oeil_hibou", DARK)
    sphere("Body", brown, (0, 0, 0.16), (0.11, 0.10, 0.16))
    sphere("Head", brown, (0, 0, 0.34), (0.095, 0.09, 0.09))
    sphere("Head", cream, (0, -0.06, 0.34), (0.06, 0.04, 0.06))  # disque facial
    sphere("Head", dark, (-0.03, -0.09, 0.36), (0.017, 0.017, 0.017))
    sphere("Head", dark, (0.03, -0.09, 0.36), (0.017, 0.017, 0.017))
    for dx in (-0.03, 0.03):
        cone("Head", brown, (dx, 0.0, 0.43), (0.025, 0.02, 0.05),
             rotation=(0, 0, math.copysign(0.2, dx)))  # aigrettes
    cube("Wings", brown, (-0.12, 0.0, 0.16), (0.03, 0.09, 0.14))
    cube("Wings", brown, (0.12, 0.0, 0.16), (0.03, 0.09, 0.14))

    arm = build_rig("Owl", {
        "Body": ("Root", (0, 0, 0.05), (0, 0, 0.24)),
        "Head": ("Body", (0, 0, 0.26), (0, 0, 0.42)),
        "Wings": ("Body", (0, 0, 0.16), (0, 0, 0.28)),
    })

    def keys(arm):
        # Boucle 1 = 89 : tête pivote (façon hibou) puis revient, ailes se
        # gonflent brièvement (ébouriffage) en fin de cycle.
        for f, a in ((1, 0.0), (25, 45.0), (49, -30.0), (73, 10.0), (89, 0.0)):
            key_rot(arm, "Head", f, (0, 0, a))
        for f, s in ((1, 1.0), (77, 1.0), (81, 1.15), (85, 0.95), (89, 1.0)):
            key_scale(arm, "Wings", f, (s, s, 1.0))

    bake_idle(arm, 89, keys)
    export("fauna_owl.glb")


# ---------------------------------------------------------------------------
# Décor animé (7) — noms distincts du pack statique, échelle réelle.
# ---------------------------------------------------------------------------


def gen_lantern_hanging():
    """Lanterne suspendue ~1.6 m (potence + lanterne) : potence/crochet sur
    Root (solide), corps de lanterne sur Lantern qui se balance doucement au
    vent en boucle."""
    dark = mat("bois_potence", WOOD_DARK)
    metal_m = mat("metal_lanterne", METAL_DARK)
    glass = mat("verre_lanterne", GLASS_AMBER)
    cylinder("Root", dark, (0, 0, 0.75), (0.06, 0.06, 1.5), vertices=8)
    cube("Root", dark, (0.30, 0, 1.48), (0.62, 0.06, 0.06))
    cylinder("Chain", metal_m, (0.58, 0, 1.30), (0.012, 0.012, 0.35),
              rotation=(math.pi / 2, 0, 0))
    cube("Lantern", metal_m, (0.58, 0, 1.02), (0.16, 0.16, 0.03))  # toit
    cylinder("Lantern", glass, (0.58, 0, 0.90), (0.13, 0.13, 0.20), vertices=8)
    for a in (0, math.pi / 2):
        cube("Lantern", metal_m, (0.58, 0, 0.90), (0.14, 0.02, 0.20), rotation=(0, 0, a))
    cube("Lantern", metal_m, (0.58, 0, 0.78), (0.15, 0.15, 0.02))  # base

    arm = build_rig("LanternHanging", {
        "Chain": ("Root", (0.58, 0, 1.46), (0.58, 0, 1.10)),
        "Lantern": ("Chain", (0.58, 0, 1.10), (0.58, 0, 0.78)),
    })

    def keys(arm):
        # Balancement pendulaire lent, boucle parfaite 1 = 81.
        for f, a in ((1, 0.0), (21, 9.0), (41, 0.0), (61, -9.0), (81, 0.0)):
            key_rot(arm, "Chain", f, (0, a, 0))

    bake_idle(arm, 81, keys)
    export("nature_lantern_hanging.glb")


def gen_weathervane():
    """Girouette ~2.4 m sur poteau : poteau/croix cardinale sur Root (solide),
    flèche sur Arrow qui tourne (vent variable) en boucle."""
    dark = mat("bois_girouette", WOOD_DARK)
    metal_m = mat("metal_girouette", METAL)
    cylinder("Root", dark, (0, 0, 1.0), (0.06, 0.06, 2.0), vertices=8)
    for a in range(4):
        ang = a * math.pi / 2
        cube("Root", metal_m, (0.35 * math.cos(ang), 0.35 * math.sin(ang), 2.05),
             (0.30, 0.03, 0.03), rotation=(0, 0, ang))
    cylinder("Arrow", metal_m, (0, 0, 2.12), (0.015, 0.015, 0.20),
              rotation=(math.pi / 2, 0, 0))
    cone("Arrow", metal_m, (0.55, 0, 2.12), (0.05, 0.10, 0.05),
         rotation=(math.pi / 2, 0, 0))
    cube("Arrow", metal_m, (-0.35, 0, 2.12), (0.16, 0.14, 0.02))

    arm = build_rig("Weathervane", {
        "Arrow": ("Root", (0, 0, 2.12), (0.6, 0, 2.12)),
    })

    def keys(arm):
        # Vent qui tourne lentement puis un coup de vent plus franc, boucle 1 = 121.
        for f, a in ((1, 0.0), (31, 40.0), (61, 20.0), (91, 150.0), (121, 360.0)):
            key_rot(arm, "Arrow", f, (0, 0, a))

    bake_idle(arm, 121, keys)
    export("nature_weathervane.glb")


def gen_prayer_flags():
    """Guirlande de fanions ~1.8 m de portée entre 2 poteaux (Root, solide) :
    5 fanions (Flag1-5) qui flottent au vent en boucle, couleurs traditionnelles."""
    dark = mat("bois_fanions", WOOD_DARK)
    rope_m = mat("corde_fanions", (0.55, 0.48, 0.35))
    colors = [CLOTH_WHITE, CLOTH_BLUE, CLOTH_RED, CLOTH_GREEN, GLOW_YELLOW]
    cylinder("Root", dark, (-0.95, 0, 0.75), (0.05, 0.05, 1.5), vertices=8)
    cylinder("Root", dark, (0.95, 0, 0.75), (0.05, 0.05, 1.5), vertices=8)
    cylinder("Root", rope_m, (0, 0, 1.48), (0.01, 0.01, 1.9),
              rotation=(0, math.pi / 2, 0))
    for i, col in enumerate(colors):
        x = -0.76 + i * 0.38
        flag = mat(f"fanion_{i}", col)
        cube(f"Flag{i+1}", flag, (x, 0, 1.30), (0.16, 0.02, 0.20))

    bones = {}
    for i in range(5):
        x = -0.76 + i * 0.38
        bones[f"Flag{i+1}"] = ("Root", (x, 0, 1.46), (x, 0, 1.14))
    arm = build_rig("PrayerFlags", bones)

    def keys(arm):
        # Ondulation en vague le long de la guirlande, boucle 1 = 65.
        wave = ((1, 0.0), (17, 1.0), (33, 0.0), (49, -1.0), (65, 0.0))
        for i in range(5):
            bone = f"Flag{i+1}"
            lag = i * 4
            for f, s in wave:
                ff = ((f - 1 + lag) % 64) + 1
                key_rot(arm, bone, ff, (0, s * 22.0, s * 8.0))

    bake_idle(arm, 65, keys)
    export("nature_prayer_flags.glb")


def gen_boat_bob():
    """Petite barque ~2.4 m qui flotte : coque sur Root, bercement complet
    (roulis + tangage léger) sur l'os Hull en boucle — posée sur l'eau, non
    solide dans la scène."""
    hull_m = mat("coque_barque", BOAT_HULL)
    dark = mat("bois_sombre_barque", WOOD_DARK)
    cube("Hull", hull_m, (0, 0, 0.20), (0.75, 2.1, 0.30))
    for side in (-1, 1):
        pan = cube("Hull", hull_m, (side * 0.42, 0, 0.36), (0.10, 2.0, 0.22))
        pan.rotation_euler = (0, 0, side * math.radians(18))
    cube("Hull", dark, (0, 0.85, 0.42), (0.55, 0.14, 0.14))  # banc avant
    cube("Hull", dark, (0, -0.60, 0.42), (0.55, 0.14, 0.14))  # banc arrière
    cylinder("Hull", dark, (0.55, -0.60, 0.55), (0.02, 0.02, 0.9),
              rotation=(0, math.radians(70), 0.3))  # aviron

    arm = build_rig("BoatBob", {
        "Hull": ("Root", (0, 0, 0.10), (0, 0, 0.55)),
    })

    def keys(arm):
        # Bercement bidirectionnel (roulis + tangage déphasés) + flottaison
        # verticale légère, boucle 1 = 97.
        for f, r, p, z in ((1, 0.0, 0.0, 0.0), (25, 4.0, -2.0, 0.03),
                           (49, 0.0, 3.0, -0.02), (73, -4.0, -1.5, 0.03), (97, 0.0, 0.0, 0.0)):
            key_rot(arm, "Hull", f, (p, r, 0))
            key_loc(arm, "Hull", f, (0, 0, z))

    bake_idle(arm, 97, keys)
    export("nature_boat_bob.glb")


def gen_reeds_sway():
    """Touffe de roseaux ~1.2 m : base plantée sur Root (solide, socle), 5
    tiges hautes sur Reed1-5 qui ondulent au vent en boucle, pour bordure de
    rivière/étang."""
    reed_m = mat("roseau", REED_GREEN)
    reed_dark = mat("roseau_epi", (0.45, 0.30, 0.14))
    mud = mat("terre_roseaux", (0.24, 0.18, 0.10))
    cylinder("Root", mud, (0, 0, 0.03), (0.22, 0.22, 0.06), vertices=8)
    offsets = [(-0.08, -0.05), (0.06, -0.08), (0.0, 0.07), (0.10, 0.03), (-0.10, 0.04)]
    heights = [0.9, 1.1, 0.75, 0.95, 1.0]
    for i, ((x, y), h) in enumerate(zip(offsets, heights)):
        cylinder(f"Reed{i+1}", reed_m, (x, y, h / 2), (0.014, 0.014, h), vertices=6)
        cone(f"Reed{i+1}", reed_dark, (x, y, h + 0.08), (0.022, 0.022, 0.16), vertices=6)

    bones = {}
    for i, ((x, y), h) in enumerate(zip(offsets, heights)):
        bones[f"Reed{i+1}"] = ("Root", (x, y, 0.06), (x, y, h + 0.16))
    arm = build_rig("ReedsSway", bones)

    def keys(arm):
        # Chaque tige oscille avec amplitude/phase propre, boucle commune 1 = 73.
        wave = ((1, 0.0), (19, 1.0), (37, 0.0), (55, -1.0), (73, 0.0))
        amps = [10.0, 13.0, 8.0, 11.0, 9.0]
        lags = [0, 6, 12, 3, 9]
        for i in range(5):
            bone = f"Reed{i+1}"
            for f, s in wave:
                ff = ((f - 1 + lags[i]) % 72) + 1
                key_rot(arm, bone, ff, (s * amps[i] * 0.4, s * amps[i], 0))

    bake_idle(arm, 73, keys)
    export("nature_reeds_sway.glb")


def gen_tree_windswept():
    """Arbre ~4.5 m au vent : tronc sur Root (solide), houppier en 2 sections
    (Canopy1 bas, Canopy2 haut) qui se balancent avec un léger décalage,
    boucle continue — variante animée du feuillu statique."""
    bark = mat("ecorce_vent", BROWN)
    leaf_m = mat("feuillage_vent", LEAF)
    leaf_dark = mat("feuillage_sombre_vent", LEAF_DARK)
    cylinder("Root", bark, (0, 0, 1.1), (0.28, 0.28, 2.2), vertices=8)
    sphere("Canopy1", leaf_m, (0, 0, 2.6), (1.3, 1.3, 1.1))
    sphere("Canopy1", leaf_dark, (0.4, 0.3, 2.4), (0.6, 0.6, 0.5))
    sphere("Canopy2", leaf_m, (0.1, -0.2, 3.6), (0.95, 0.95, 0.85))
    sphere("Canopy2", leaf_dark, (-0.3, 0.2, 3.8), (0.5, 0.5, 0.45))

    arm = build_rig("TreeWindswept", {
        "Canopy1": ("Root", (0, 0, 2.2), (0, 0, 3.0)),
        "Canopy2": ("Canopy1", (0, 0, 3.0), (0, 0, 4.2)),
    })

    def keys(arm):
        # Balancement au vent, la cime (Canopy2) amplifie le mouvement de la
        # base (Canopy1) avec un léger retard. Boucle 1 = 113.
        for f, a in ((1, 0.0), (29, 5.0), (57, -3.0), (85, 4.0), (113, 0.0)):
            key_rot(arm, "Canopy1", f, (0, a, a * 0.4))
        for f, a in ((5, 0.0), (33, 9.0), (61, -6.0), (89, 7.0), (113, 0.0), (1, 0.0)):
            key_rot(arm, "Canopy2", f, (0, a, a * 0.5))

    bake_idle(arm, 113, keys)
    export("nature_tree_windswept.glb")


def gen_well_pulley():
    """Puits ~2 m : margelle de pierre + potence sur Root (solide), seau sur
    Bucket qui descend/remonte au bout de la corde en boucle."""
    stone = mat("pierre_puits", STONE)
    dark = mat("bois_potence_puits", WOOD_DARK)
    metal_m = mat("metal_puits", METAL_DARK)
    cylinder("Root", stone, (0, 0, 0.5), (0.7, 0.7, 1.0), vertices=12)
    cylinder("Root", dark, (-0.55, 0, 1.4), (0.06, 0.06, 1.8), vertices=8)
    cylinder("Root", dark, (0.55, 0, 1.4), (0.06, 0.06, 1.8), vertices=8)
    cube("Root", dark, (0, 0, 2.28), (1.3, 0.10, 0.10))
    cylinder("Root", dark, (0, 0, 1.95), (0.05, 0.05, 0.7),
              rotation=(math.pi / 2, 0, 0))  # manivelle axe
    cylinder("Rope", metal_m, (0, 0, 1.6), (0.008, 0.008, 0.7),
              rotation=(math.pi / 2, 0, 0))
    cube("Bucket", metal_m, (0, 0, 1.2), (0.16, 0.16, 0.20))

    arm = build_rig("WellPulley", {
        "Rope": ("Root", (0, 0, 2.28), (0, 0, 1.55)),
        "Bucket": ("Rope", (0, 0, 1.55), (0, 0, 1.15)),
    })

    def keys(arm):
        # Descente/remontée du seau (échelle de l'os = longueur de corde
        # visible), boucle parfaite 1 = 97.
        for f, s, z in ((1, 1.0, 0.0), (25, 2.2, -0.9), (49, 2.2, -0.9),
                        (73, 1.0, 0.0), (97, 1.0, 0.0)):
            key_scale(arm, "Rope", f, (1, 1, s))
            key_loc(arm, "Bucket", f, (0, 0, z))

    bake_idle(arm, 97, keys)
    export("nature_well_pulley.glb")


ASSETS = [
    gen_rabbit,
    gen_butterfly,
    gen_fish,
    gen_frog,
    gen_duck,
    gen_deer,
    gen_squirrel,
    gen_owl,
    gen_lantern_hanging,
    gen_weathervane,
    gen_prayer_flags,
    gen_boat_bob,
    gen_reeds_sway,
    gen_tree_windswept,
    gen_well_pulley,
]

for gen in ASSETS:
    reset_scene()
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[fauna-decor] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
