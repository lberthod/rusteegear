# Génère 20 assets animés supplémentaires (petite faune ambiante + mécanismes
# de décor) en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_menagerie_pack.py
#
# Sortie : assets/models/fauna_{bird,butterfly,fish,firefly,rabbit,frog,
# chicken,sheep}.glb et assets/models/nature_{weathervane,drawbridge,
# pendulum_clock,dock_crane,catapult,prayer_wheel,wind_chime,rocking_chair,
# spinning_wheel,water_pump,bellows,market_awning}.glb.
#
# Même recette que gen_nature_animated.py (elle-même reprise de
# gen_fairy_hero.py) : armature minuscule (≤ 4 os utiles), un mesh joint par
# glb, vertex group plein par partie (100 % sur un seul os), UN clip « Idle »
# en boucle parfaite (première pose = dernière pose) exporté en piste NLA
# (`export_animation_mode="NLA_TRACKS"` + `export_force_sampling`). Seuls des
# canaux rotation/scale sont keyframés (pas de translation d'os — non
# éprouvée par le reste du pipeline).
#
# Contraintes moteur reprises telles quelles (cf. gen_nature_pack.py /
# gen_nature_animated.py) :
# - base au sol z=0 Blender (= y=0 jeu), Blender +Y → -Z jeu (face « avant »).
# - la physique du jeu utilise le TriMesh de la POSE DE REPOS : les parties
#   mobiles restent hors de portée du joueur ou l'asset est explicitement
#   non solide dans la scène (petite faune, feu, chimes...).
# - purger la pose résiduelle avant export (piège connu : la pose de liaison
#   embarquerait sinon la dernière pose keyframée).
# - budget : MAX_SKINNED_INSTANCES du renderer est partagé avec créatures et
#   joueurs réseau → une seule instance par asset animé dans la scène.

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

# Palette : reprend les teintes du pack nature animée + quelques ajouts faune.
WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
STONE = (0.45, 0.44, 0.42)
STONE_DARK = (0.36, 0.35, 0.34)
METAL = (0.55, 0.55, 0.58)
METAL_DARK = (0.30, 0.30, 0.33)
ROPE = (0.62, 0.52, 0.30)
CANVAS_RED = (0.68, 0.16, 0.14)
CANVAS_CREAM = (0.85, 0.78, 0.62)
THATCH = (0.62, 0.48, 0.20)

FEATHER_BROWN = (0.42, 0.27, 0.14)
FEATHER_CREAM = (0.82, 0.74, 0.58)
FUR_BROWN = (0.50, 0.36, 0.20)
FUR_CREAM = (0.88, 0.83, 0.72)
WOOL_WHITE = (0.86, 0.84, 0.78)
SCALE_BLUE = (0.20, 0.42, 0.55)
SCALE_SILVER = (0.68, 0.75, 0.80)
FROG_GREEN = (0.24, 0.48, 0.22)
FROG_BELLY = (0.78, 0.80, 0.55)
WING_ORANGE = (0.85, 0.42, 0.08)
WING_BLACK = (0.10, 0.09, 0.09)
GLOW_YELLOW = (1.0, 0.85, 0.35)
BEAK_ORANGE = (0.82, 0.45, 0.10)
COMB_RED = (0.68, 0.12, 0.10)


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb):
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.85
    return m


PARTS = []


def add_part(bone, material, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    """Crée une primitive skinnée à 100 % sur `bone` (vertex group plein)."""
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


def sphere(bone, material, location, scale, rotation=(0, 0, 0), segments=8, rings=6):
    def op(location, rotation):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=segments, ring_count=rings, radius=1.0,
            location=location, rotation=rotation,
        )

    return add_part(bone, material, op, location, scale, rotation)


def build_rig(name, bones):
    """Joint les PARTS en un mesh, crée l'armature `bones` {nom: (parent, head,
    tail)} sous un os Root, parente et pose le modificateur Armature."""
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
    """Crée le clip « Idle » (keyframes via `keyer`) en piste NLA, puis purge la
    pose résiduelle (piège connu : la pose de liaison embarquerait sinon la
    dernière pose keyframée)."""
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
    print(f"[menagerie] exporté {filename}")


# ---------------------------------------------------------------------------
# Petite faune ambiante — non solide, hors physique de la scène.
# ---------------------------------------------------------------------------


def gen_bird():
    """Oiseau perché ~0,3 m : corps (Root) + ailes qui se replient/déplient et
    queue qui bat, boucle courte (nerveux)."""
    body_m = mat("plumage_corps", FEATHER_BROWN)
    belly_m = mat("plumage_ventre", FEATHER_CREAM)
    beak_m = mat("bec", BEAK_ORANGE)
    sphere("Root", body_m, (0, 0, 0.18), (0.16, 0.22, 0.16))
    sphere("Root", belly_m, (0, -0.05, 0.14), (0.10, 0.12, 0.10))
    sphere("Root", body_m, (0, 0.20, 0.24), (0.10, 0.10, 0.10))  # tête
    cone("Root", beak_m, (0, 0.30, 0.23), (0.035, 0.10, 0.035),
         rotation=(math.pi / 2, 0, 0))
    cube("WingL", body_m, (0.16, 0, 0.18), (0.06, 0.20, 0.10))
    cube("WingR", body_m, (-0.16, 0, 0.18), (0.06, 0.20, 0.10))
    cone("Tail", body_m, (0, -0.24, 0.16), (0.09, 0.20, 0.05), vertices=4,
         rotation=(math.pi / 2, 0, math.pi / 4))

    arm = build_rig("Bird", {
        "WingL": ("Root", (0.10, 0, 0.20), (0.24, 0, 0.20)),
        "WingR": ("Root", (-0.10, 0, 0.20), (-0.24, 0, 0.20)),
        "Tail": ("Root", (0, -0.14, 0.17), (0, -0.34, 0.15)),
    })

    def keys(arm):
        seq = ((1, 0.0), (7, -18.0), (13, 4.0), (19, -10.0), (25, 0.0))
        for f, a in seq:
            key_rot(arm, "WingL", f, (0, 0, -a))
            key_rot(arm, "WingR", f, (0, 0, a))
            key_rot(arm, "Tail", f, (a * 0.4, 0, 0))

    bake_idle(arm, 25, keys)
    export("fauna_bird.glb")


def gen_butterfly():
    """Papillon flottant ~0,25 m : corps (Root, fixe) + 2 ailes qui battent
    vite en ciseaux — asset décoratif pur, plane sur place."""
    body_m = mat("corps_insecte", WING_BLACK)
    wing_m = mat("aile_papillon", WING_ORANGE)
    sphere("Root", body_m, (0, 0, 0.2), (0.02, 0.06, 0.02), segments=6, rings=4)
    cube("WingL", wing_m, (0.10, 0, 0.2), (0.14, 0.02, 0.10))
    cube("WingR", wing_m, (-0.10, 0, 0.2), (0.14, 0.02, 0.10))

    arm = build_rig("Butterfly", {
        "WingL": ("Root", (0, 0, 0.2), (0.22, 0, 0.2)),
        "WingR": ("Root", (0, 0, 0.2), (-0.22, 0, 0.2)),
    })

    def keys(arm):
        seq = ((1, 0.0), (4, -55.0), (7, 5.0), (10, -55.0), (13, 0.0))
        for f, a in seq:
            key_rot(arm, "WingL", f, (0, a, 0))
            key_rot(arm, "WingR", f, (0, -a, 0))

    bake_idle(arm, 13, keys)
    export("fauna_butterfly.glb")


def gen_fish():
    """Poisson ~0,3 m nageant sur place (mare/rivière) : corps (Root) qui
    ondule en S via une chaîne Body→Tail, nageoires fixes sur Root."""
    scale_m = mat("ecailles", SCALE_BLUE)
    fin_m = mat("nageoire", SCALE_SILVER)
    cube("Root", scale_m, (0, 0.10, 0), (0.09, 0.16, 0.09))
    cone("Root", scale_m, (0, 0.26, 0), (0.09, 0.10, 0.09), vertices=6,
         rotation=(math.pi / 2, 0, 0))
    cube("Root", fin_m, (0.11, 0.08, 0), (0.02, 0.07, 0.06))
    cube("Root", fin_m, (-0.11, 0.08, 0), (0.02, 0.07, 0.06))
    cube("Body", scale_m, (0, -0.06, 0), (0.075, 0.12, 0.075))
    cone("Tail", fin_m, (0, -0.26, 0), (0.02, 0.14, 0.10), vertices=3,
         rotation=(math.pi / 2, 0, math.pi / 2))

    arm = build_rig("Fish", {
        "Body": ("Root", (0, 0.02, 0), (0, -0.14, 0)),
        "Tail": ("Body", (0, -0.14, 0), (0, -0.30, 0)),
    })

    def keys(arm):
        seq = ((1, 0.0), (11, 1.0), (21, -1.0), (31, 1.0), (41, 0.0))
        for f, s in seq:
            key_rot(arm, "Body", f, (0, 0, s * 10.0))
            key_rot(arm, "Tail", f, (0, 0, s * -22.0))

    bake_idle(arm, 41, keys)
    export("fauna_fish.glb")


def gen_firefly():
    """Luciole ~0,05 m : minuscule globe lumineux (Glow) qui pulse, monté sur
    un bras (Boom) qui oscille lentement pour simuler la dérive en vol."""
    glow_m = mat("lueur_luciole", GLOW_YELLOW)
    wing_m = mat("aile_luciole", WING_BLACK)
    sphere("Glow", glow_m, (0, 0, 0.25), (0.045, 0.045, 0.045), segments=6, rings=4)
    cube("Glow", wing_m, (0.03, 0, 0.26), (0.03, 0.015, 0.02))
    cube("Glow", wing_m, (-0.03, 0, 0.26), (0.03, 0.015, 0.02))

    arm = build_rig("Firefly", {
        "Boom": ("Root", (0, 0, 0.05), (0, 0, 0.20)),
        "Glow": ("Boom", (0, 0, 0.20), (0, 0, 0.30)),
    })

    def keys(arm):
        seq = ((1, 0.0), (15, 12.0), (29, -6.0), (43, 8.0), (57, 0.0))
        for f, a in seq:
            key_rot(arm, "Boom", f, (a * 0.6, a, 0))
        pulses = ((1, 1.0), (10, 1.4), (20, 0.8), (30, 1.4), (40, 0.9), (57, 1.0))
        for f, s in pulses:
            key_scale(arm, "Glow", f, (s, s, s))

    bake_idle(arm, 57, keys)
    export("fauna_firefly.glb")


def gen_rabbit():
    """Lapin assis ~0,3 m : corps (Root) + oreilles qui frémissent + nez qui
    remue, aucune patte animée (posture assise statique)."""
    fur_m = mat("fourrure_lapin", FUR_BROWN)
    belly_m = mat("fourrure_ventre", FUR_CREAM)
    sphere("Root", fur_m, (0, 0, 0.14), (0.14, 0.18, 0.13))
    sphere("Root", belly_m, (0, -0.08, 0.10), (0.09, 0.08, 0.09))
    sphere("Root", fur_m, (0, 0.14, 0.24), (0.09, 0.09, 0.09))  # tête
    sphere("Root", fur_m, (0, -0.02, 0.03), (0.03, 0.03, 0.03))  # queue
    cube("EarL", fur_m, (0.04, 0.14, 0.24), (0.03, 0.02, 0.14))
    cube("EarR", fur_m, (-0.04, 0.14, 0.24), (0.03, 0.02, 0.14))
    sphere("Nose", belly_m, (0, 0.22, 0.22), (0.025, 0.025, 0.025), segments=6, rings=4)

    arm = build_rig("Rabbit", {
        "EarL": ("Root", (0.04, 0.14, 0.28), (0.05, 0.13, 0.42)),
        "EarR": ("Root", (-0.04, 0.14, 0.28), (-0.05, 0.13, 0.42)),
        "Nose": ("Root", (0, 0.20, 0.22), (0, 0.26, 0.22)),
    })

    def keys(arm):
        seq = ((1, 0.0), (13, -10.0), (25, 3.0), (37, -6.0), (49, 0.0))
        for f, a in seq:
            key_rot(arm, "EarL", f, (a, 0, 3.0))
            key_rot(arm, "EarR", f, (a, 0, -3.0))
        sniff = ((1, 1.0), (7, 1.3), (13, 1.0), (49, 1.0))
        for f, s in sniff:
            key_scale(arm, "Nose", f, (s, s, s))

    bake_idle(arm, 49, keys)
    export("fauna_rabbit.glb")


def gen_frog():
    """Grenouille ~0,2 m posée sur une feuille : corps (Root) + gorge qui se
    gonfle (coassement) + yeux fixes, léger rebond avant/arrière du buste."""
    green_m = mat("peau_grenouille", FROG_GREEN)
    belly_m = mat("ventre_grenouille", FROG_BELLY)
    eye_m = mat("oeil_grenouille", (0.05, 0.05, 0.05))
    sphere("Root", green_m, (0, 0, 0.09), (0.14, 0.17, 0.09))
    sphere("Root", eye_m, (0.07, 0.09, 0.16), (0.03, 0.03, 0.03), segments=6, rings=4)
    sphere("Root", eye_m, (-0.07, 0.09, 0.16), (0.03, 0.03, 0.03), segments=6, rings=4)
    sphere("Throat", belly_m, (0, 0.10, 0.04), (0.06, 0.06, 0.05))

    arm = build_rig("Frog", {
        "Bust": ("Root", (0, 0, 0.05), (0, 0.05, 0.14)),
        "Throat": ("Bust", (0, 0.10, 0.04), (0, 0.16, 0.04)),
    })

    def keys(arm):
        croak = ((1, 1.0), (9, 1.9), (17, 1.0), (25, 1.6), (33, 1.0))
        for f, s in croak:
            key_scale(arm, "Throat", f, (s, s, s))
            key_rot(arm, "Bust", f, ((s - 1.0) * -6.0, 0, 0))

    bake_idle(arm, 33, keys)
    export("fauna_frog.glb")


def gen_chicken():
    """Poule ~0,35 m qui picore : corps (Root) + tête/cou qui plonge vers le
    sol et remonte, en boucle."""
    body_m = mat("plumage_poule", FEATHER_CREAM)
    comb_m = mat("crete", COMB_RED)
    beak_m = mat("bec_poule", BEAK_ORANGE)
    sphere("Root", body_m, (0, 0, 0.20), (0.15, 0.20, 0.17))
    cube("Root", body_m, (0, -0.16, 0.14), (0.10, 0.10, 0.14))  # queue relevée
    sphere("Head", body_m, (0, 0.18, 0.30), (0.08, 0.08, 0.08))
    cone("Head", beak_m, (0, 0.28, 0.29), (0.03, 0.08, 0.03),
         rotation=(math.pi / 2, 0, 0))
    cone("Head", comb_m, (0, 0.16, 0.38), (0.02, 0.05, 0.06), vertices=4)

    arm = build_rig("Chicken", {
        "Head": ("Root", (0, 0.14, 0.28), (0, 0.14, 0.40)),
    })

    def keys(arm):
        seq = ((1, 0.0), (9, 55.0), (13, 60.0), (18, 0.0), (30, 0.0),
               (38, 55.0), (42, 60.0), (47, 0.0))
        for f, a in seq:
            key_rot(arm, "Head", f, (-a, 0, 0))

    bake_idle(arm, 47, keys)
    export("fauna_chicken.glb")


def gen_sheep():
    """Mouton ~0,7 m qui broute : corps laineux (Root, socle plein — visible
    des sondes) + tête/cou qui plonge vers l'herbe et remonte."""
    wool_m = mat("laine", WOOL_WHITE)
    face_m = mat("museau_mouton", (0.20, 0.17, 0.15))
    cube("Root", wool_m, (0, 0, 0.32), (0.28, 0.42, 0.26))
    for lx in (0.16, -0.16):
        for ly in (0.14, -0.14):
            cube("Root", face_m, (lx, ly, 0.10), (0.05, 0.05, 0.10))  # pattes
    sphere("Head", face_m, (0, 0.34, 0.36), (0.11, 0.11, 0.10))
    cube("EarL", face_m, (0.11, 0.34, 0.36), (0.05, 0.02, 0.03))
    cube("EarR", face_m, (-0.11, 0.34, 0.36), (0.05, 0.02, 0.03))

    arm = build_rig("Sheep", {
        "Head": ("Root", (0, 0.28, 0.38), (0, 0.44, 0.38)),
        "EarL": ("Head", (0.09, 0.34, 0.36), (0.16, 0.34, 0.36)),
        "EarR": ("Head", (-0.09, 0.34, 0.36), (-0.16, 0.34, 0.36)),
    })

    def keys(arm):
        seq = ((1, 0.0), (20, 42.0), (28, 48.0), (40, 0.0), (85, 0.0))
        for f, a in seq:
            key_rot(arm, "Head", f, (-a, 0, 0))
        flick = ((1, 0.0), (55, 0.0), (60, -18.0), (65, 0.0), (85, 0.0))
        for f, a in flick:
            key_rot(arm, "EarL", f, (0, 0, a))
            key_rot(arm, "EarR", f, (0, 0, -a))

    bake_idle(arm, 85, keys)
    export("fauna_sheep.glb")


# ---------------------------------------------------------------------------
# Mécanismes de décor — même socle solide/non-solide que gen_nature_animated.
# ---------------------------------------------------------------------------


def gen_weathervane():
    """Girouette de toiture ~2,4 m (posée sur un pignon de pierre, socle plein
    → sondes) : flèche qui oscille lentement au gré du vent."""
    stone = mat("pierre_girouette", STONE)
    metal = mat("metal_girouette", METAL)
    dark = mat("metal_sombre", METAL_DARK)
    cone("Root", stone, (0, 0, 0.5), (0.9, 0.9, 1.0), vertices=4)  # pignon
    cylinder("Root", dark, (0, 0, 1.1), (0.04, 0.04, 0.5))
    sphere("Root", metal, (0, 0, 1.38), (0.05, 0.05, 0.05), segments=6, rings=4)
    cube("Arrow", metal, (0, 0.22, 1.42), (0.03, 0.55, 0.14))
    cone("Arrow", metal, (0, 0.55, 1.42), (0.09, 0.18, 0.14), vertices=3,
         rotation=(math.pi / 2, 0, math.pi / 2))
    cube("Arrow", dark, (0, -0.20, 1.42), (0.02, 0.20, 0.08))

    arm = build_rig("Weathervane", {
        "Arrow": ("Root", (0, 0, 1.42), (0, 0.3, 1.42)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (37, 35.0), (73, -20.0), (109, 15.0), (145, 0.0)):
            key_rot(arm, "Arrow", f, (0, 0, a))

    bake_idle(arm, 145, keys)
    export("nature_weathervane.glb")


def gen_drawbridge():
    """Pont-levis de bois ~3 m (culée de pierre, Root — solide) : tablier
    (Plank) qui se lève et se rabaisse lentement, chaînes fixes."""
    stone = mat("pierre_pont_levis", STONE_DARK)
    wood = mat("bois_pont_levis", WOOD)
    dark = mat("bois_sombre_pont_levis", WOOD_DARK)
    cube("Root", stone, (0, -0.3, 0.5), (1.6, 0.6, 1.0))  # culée
    for x in (-0.7, 0.7):
        cube("Root", dark, (x, 0.05, 1.1), (0.08, 0.08, 1.8))  # potences
    for i in range(6):
        cube("Plank", wood, (0, 0.15 + i * 0.5, 0), (0.75, 0.24, 0.06))

    arm = build_rig("Drawbridge", {
        "Plank": ("Root", (0, 0.0, 1.0), (0, 3.0, 1.0)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (30, 0.0), (55, 70.0), (80, 70.0), (105, 0.0), (145, 0.0)):
            key_rot(arm, "Plank", f, (-a, 0, 0))

    bake_idle(arm, 145, keys)
    export("nature_drawbridge.glb")


def gen_pendulum_clock():
    """Horloge de tour ~2,8 m (socle de pierre plein, Root) : cadran fixe et
    balancier (Pendulum) qui oscille en boucle parfaite."""
    stone = mat("pierre_horloge", STONE)
    wood = mat("bois_horloge", WOOD_DARK)
    face_m = mat("cadran", (0.92, 0.90, 0.82))
    metal = mat("metal_balancier", METAL)
    cube("Root", stone, (0, 0, 1.0), (0.7, 0.5, 2.0))
    cylinder("Root", face_m, (0, -0.26, 1.9), (0.5, 0.5, 0.06),
              rotation=(math.pi / 2, 0, 0), vertices=16)
    cube("Root", wood, (0, -0.28, 1.9), (0.04, 0.02, 0.9))
    cube("Root", wood, (0, -0.28, 1.9), (0.9, 0.02, 0.04))
    cylinder("Pendulum", metal, (0, -0.1, 1.55), (0.02, 0.02, 0.7))
    sphere("Pendulum", metal, (0, -0.1, 1.15), (0.10, 0.10, 0.10))

    arm = build_rig("PendulumClock", {
        "Pendulum": ("Root", (0, -0.1, 1.85), (0, -0.1, 1.15)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (13, 12.0), (25, 0.0), (37, -12.0), (49, 0.0)):
            key_rot(arm, "Pendulum", f, (a, 0, 0))

    bake_idle(arm, 49, keys)
    export("nature_pendulum_clock.glb")


def gen_dock_crane():
    """Grue de quai en bois ~3,2 m (embase de pierre, Root — solide) : flèche
    qui monte/descend (Arm) et crochet qui se balance (Hook)."""
    stone = mat("pierre_grue", STONE_DARK)
    wood = mat("bois_grue", WOOD)
    dark = mat("bois_sombre_grue", WOOD_DARK)
    metal = mat("metal_grue", METAL_DARK)
    cylinder("Root", stone, (0, 0, 0.4), (0.7, 0.7, 0.8), vertices=8)
    cube("Root", dark, (0, 0, 1.6), (0.22, 0.22, 2.4))
    cube("Arm", wood, (0.9, 0, 2.9), (1.8, 0.16, 0.16))
    cylinder("Hook", metal, (1.7, 0, 2.75), (0.02, 0.02, 0.35))
    cone("Hook", metal, (1.7, 0, 2.5), (0.06, 0.06, 0.12), vertices=6)

    arm = build_rig("DockCrane", {
        "Arm": ("Root", (0, 0, 2.9), (1.8, 0, 2.9)),
        "Hook": ("Arm", (1.7, 0, 2.75), (1.7, 0, 2.4)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (25, 12.0), (49, -4.0), (73, 0.0)):
            key_rot(arm, "Arm", f, (0, a, 0))
        for f, a in ((1, 0.0), (18, 8.0), (37, -6.0), (55, 4.0), (73, 0.0)):
            key_rot(arm, "Hook", f, (0, 0, a))

    bake_idle(arm, 73, keys)
    export("nature_dock_crane.glb")


def gen_catapult():
    """Catapulte de siège ~2,6 m (chassis solide, Root) : bras de tir (Arm)
    qui se balance doucement, corde tendue fixe."""
    wood = mat("bois_catapulte", WOOD)
    dark = mat("bois_sombre_catapulte", WOOD_DARK)
    rope_m = mat("corde_catapulte", ROPE)
    metal = mat("metal_catapulte", METAL_DARK)
    for x in (-0.55, 0.55):
        cube("Root", dark, (x, 0.4, 0.35), (0.14, 1.2, 0.7))
        cube("Root", dark, (x, -0.4, 0.35), (0.14, 1.2, 0.7))
    cube("Root", wood, (0, 0, 0.05), (1.5, 1.6, 0.14))  # plateforme
    cylinder("Root", metal, (0, 0, 0.75), (0.07, 0.07, 1.3),
              rotation=(math.pi / 2, 0, 0))
    cube("Arm", wood, (0, -0.5, 0.75), (0.14, 1.5, 0.14))
    cube("Arm", dark, (0, -1.7, 0.75), (0.35, 0.35, 0.14))  # cuillère
    cube("Root", rope_m, (0, 1.1, 0.35), (0.05, 0.05, 0.6))

    arm = build_rig("Catapult", {
        "Arm": ("Root", (0, 0, 0.75), (0, -1.0, 0.75)),
    })

    def keys(arm):
        for f, a in ((1, -12.0), (37, 8.0), (73, -12.0)):
            key_rot(arm, "Arm", f, (a, 0, 0))

    bake_idle(arm, 73, keys)
    export("nature_catapult.glb")


def gen_prayer_wheel():
    """Moulin à prières ~1,3 m (socle de pierre, Root — solide) : tambour
    gravé qui tourne en continu autour de son axe vertical."""
    stone = mat("pierre_moulin_priere", STONE)
    metal = mat("metal_moulin_priere", (0.62, 0.50, 0.20))
    dark = mat("metal_sombre_moulin_priere", METAL_DARK)
    cylinder("Root", stone, (0, 0, 0.15), (0.18, 0.18, 0.3), vertices=8)
    cylinder("Root", dark, (0, 0, 0.4), (0.03, 0.03, 0.2))
    cylinder("Drum", metal, (0, 0, 0.72), (0.28, 0.28, 0.5), vertices=12)
    for i in range(8):
        a = i * math.tau / 8
        cube("Drum", dark, (0.29 * math.cos(a), 0.29 * math.sin(a), 0.72),
             (0.02, 0.02, 0.5))

    arm = build_rig("PrayerWheel", {
        "Drum": ("Root", (0, 0, 0.5), (0, 0, 1.0)),
    })

    def keys(arm):
        for f, ang in ((1, 0.0), (25, 120.0), (49, 240.0), (73, 360.0)):
            key_rot(arm, "Drum", f, (0, 0, ang))

    bake_idle(arm, 73, keys)
    export("nature_prayer_wheel.glb")


def gen_wind_chime():
    """Carillon à vent suspendu ~0,8 m (support fixe, Root — non solide) :
    3 tubes métalliques (Chime1-3) qui oscillent avec déphasage, comme les
    segments de gen_banner."""
    wood = mat("bois_carillon", WOOD_DARK)
    metal = mat("metal_carillon", METAL)
    cube("Root", wood, (0, 0, 0.78), (0.35, 0.06, 0.05))  # traverse
    for i, x in enumerate((-0.24, 0.0, 0.24)):
        cylinder(f"Chime{i+1}", metal, (x, 0, 0.55), (0.025, 0.025, 0.3))

    arm = build_rig("WindChime", {
        "Chime1": ("Root", (-0.24, 0, 0.76), (-0.24, 0, 0.40)),
        "Chime2": ("Root", (0.0, 0, 0.76), (0.0, 0, 0.40)),
        "Chime3": ("Root", (0.24, 0, 0.76), (0.24, 0, 0.40)),
    })

    def keys(arm):
        wave = ((1, 0.0), (17, 1.0), (33, 0.0), (49, -1.0), (65, 0.0))
        for bone, (amp, lag) in (("Chime1", (10.0, 0)), ("Chime2", (12.0, 5)), ("Chime3", (9.0, 10))):
            for f, s in wave:
                key_rot(arm, bone, f + lag if f + lag <= 65 else f + lag - 64, (s * amp, 0, s * amp * 0.4))

    bake_idle(arm, 65, keys)
    export("nature_wind_chime.glb")


def gen_rocking_chair():
    """Chaise à bascule ~0,9 m (posée sur une véranda, Root fixe = pieds
    avant, non solide) : assise + patins qui basculent doucement d'avant en
    arrière autour du point de contact au sol."""
    wood = mat("bois_bascule", WOOD)
    dark = mat("bois_sombre_bascule", WOOD_DARK)
    cube("Seat", wood, (0, 0, 0.42), (0.42, 0.42, 0.06))
    cube("Seat", wood, (0, -0.19, 0.72), (0.42, 0.06, 0.56))  # dossier
    for x in (-0.18, 0.18):
        cube("Seat", dark, (x, 0.18, 0.20), (0.05, 0.05, 0.36))
        cube("Seat", dark, (x, -0.18, 0.20), (0.05, 0.05, 0.36))
    for x in (-0.20, 0.20):
        rocker = cube("Seat", dark, (x, 0, 0.02), (0.05, 0.55, 0.05))
        rocker.rotation_euler = (0, 0, 0)

    arm = build_rig("RockingChair", {
        "Seat": ("Root", (0, 0, 0.02), (0, 0, 0.62)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (20, 6.0), (40, -6.0), (60, 0.0)):
            key_rot(arm, "Seat", f, (a, 0, 0))

    bake_idle(arm, 60, keys)
    export("nature_rocking_chair.glb")


def gen_spinning_wheel():
    """Rouet à filer ~1,1 m (socle solide, Root) : grande roue qui tourne en
    continu, pédale qui va-et-vient."""
    wood = mat("bois_rouet", WOOD)
    dark = mat("bois_sombre_rouet", WOOD_DARK)
    metal = mat("metal_rouet", METAL_DARK)
    cube("Root", wood, (0, 0, 0.10), (0.55, 0.22, 0.10))  # socle
    cylinder("Root", dark, (-0.28, 0, 0.55), (0.05, 0.05, 0.9))
    cylinder("Wheel", metal, (-0.28, 0, 0.95), (0.42, 0.42, 0.04),
              rotation=(math.pi / 2, 0, 0), vertices=14)
    for i in range(6):
        a = i * math.tau / 6
        cube("Wheel", dark, (-0.28 + 0.4 * math.cos(a), 0.4 * math.sin(a), 0.95),
             (0.03, 0.03, 0.4))
    cube("Pedal", wood, (-0.10, 0, 0.16), (0.14, 0.20, 0.03))

    arm = build_rig("SpinningWheel", {
        "Wheel": ("Root", (-0.28, 0, 0.95), (-0.28, 0.4, 0.95)),
        "Pedal": ("Root", (-0.10, 0, 0.16), (-0.10, 0.20, 0.16)),
    })

    def keys(arm):
        for f, ang in ((1, 0.0), (25, 120.0), (49, 240.0), (73, 360.0)):
            key_rot(arm, "Wheel", f, (ang, 0, 0))
        for f, a in ((1, 0.0), (19, 10.0), (37, 0.0), (55, -10.0), (73, 0.0)):
            key_rot(arm, "Pedal", f, (a, 0, 0))

    bake_idle(arm, 73, keys)
    export("nature_spinning_wheel.glb")


def gen_water_pump():
    """Pompe à eau de puits ~1,2 m (socle de pierre, Root — solide) : bras de
    levier (Handle) qui pompe de haut en bas en boucle."""
    stone = mat("pierre_pompe", STONE)
    metal = mat("metal_pompe", METAL_DARK)
    dark = mat("metal_pompe_clair", METAL)
    cylinder("Root", stone, (0, 0, 0.3), (0.22, 0.22, 0.6), vertices=8)
    cylinder("Root", metal, (0, 0, 0.75), (0.08, 0.08, 0.3))
    cube("Root", metal, (0.10, 0, 0.35), (0.05, 0.05, 0.5))  # bec verseur
    cube("Handle", dark, (0, -0.30, 1.0), (0.05, 0.55, 0.05))

    arm = build_rig("WaterPump", {
        "Handle": ("Root", (0, 0, 1.0), (0, -0.55, 1.0)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (13, 22.0), (25, 0.0), (37, 22.0), (49, 0.0)):
            key_rot(arm, "Handle", f, (a, 0, 0))

    bake_idle(arm, 49, keys)
    export("nature_water_pump.glb")


def gen_bellows():
    """Soufflet de forge ~0,8 m (posé sur son support, Root — solide) : la
    poche (Bag) se comprime/étire, le manche (Handle) actionne le mouvement."""
    leather = mat("cuir_soufflet", (0.42, 0.26, 0.14))
    wood = mat("bois_soufflet", WOOD_DARK)
    metal = mat("metal_soufflet", METAL_DARK)
    cube("Root", wood, (0, 0, 0.12), (0.5, 0.3, 0.10))  # support
    cube("Root", wood, (-0.35, 0, 0.30), (0.06, 0.06, 0.30))  # pied
    cylinder("Root", metal, (0.42, 0, 0.30), (0.03, 0.03, 0.16),
              rotation=(math.pi / 2, 0, 0))
    cube("Bag", leather, (0, 0, 0.30), (0.42, 0.26, 0.14))
    cube("Handle", wood, (-0.22, 0, 0.42), (0.20, 0.05, 0.04))

    arm = build_rig("Bellows", {
        "Bag": ("Root", (0, 0, 0.30), (0, 0, 0.48)),
        "Handle": ("Bag", (-0.22, 0, 0.42), (-0.42, 0, 0.42)),
    })

    def keys(arm):
        pump = ((1, 1.0), (13, 0.55), (25, 1.0), (37, 0.55), (49, 1.0))
        for f, s in pump:
            key_scale(arm, "Bag", f, (1.0, 1.0, s))
            key_rot(arm, "Handle", f, (0, 0, (1.0 - s) * -22.0))

    bake_idle(arm, 49, keys)
    export("nature_bellows.glb")


def gen_market_awning():
    """Auvent d'étal de marché ~2,2 m (montants fixes, Root — solide) : toile
    à rayures qui ondule au vent, en 2 segments comme la bannière."""
    wood = mat("bois_auvent", WOOD_DARK)
    red_m = mat("toile_rouge_auvent", CANVAS_RED)
    cream_m = mat("toile_creme_auvent", CANVAS_CREAM)
    for x in (-0.9, 0.9):
        cube("Root", wood, (x, 0, 1.0), (0.08, 0.08, 2.0))
    cube("Root", wood, (0, 0, 1.95), (1.0, 0.08, 0.08))  # traverse
    for i, x in enumerate((-0.75, -0.25, 0.25, 0.75)):
        stripe_m = red_m if i % 2 == 0 else cream_m
        cube("Awning1", stripe_m, (x, 0.35, 1.75), (0.25, 0.4, 0.04))
    for i, x in enumerate((-0.75, -0.25, 0.25, 0.75)):
        stripe_m = red_m if i % 2 == 0 else cream_m
        cube("Awning2", stripe_m, (x, 0.75, 1.55), (0.25, 0.4, 0.04))

    arm = build_rig("MarketAwning", {
        "Awning1": ("Root", (0, 0.0, 1.85), (0, 0.55, 1.70)),
        "Awning2": ("Awning1", (0, 0.55, 1.70), (0, 1.10, 1.45)),
    })

    def keys(arm):
        wave = ((1, 0.0), (16, 1.0), (31, 0.0), (46, -1.0), (61, 0.0))
        for f, s in wave:
            key_rot(arm, "Awning1", f, (s * 8.0, 0, 0))
            key_rot(arm, "Awning2", f, (s * 14.0, 0, 0))

    bake_idle(arm, 61, keys)
    export("nature_market_awning.glb")


ASSETS = [
    gen_bird, gen_butterfly, gen_fish, gen_firefly, gen_rabbit, gen_frog,
    gen_chicken, gen_sheep,
    gen_weathervane, gen_drawbridge, gen_pendulum_clock, gen_dock_crane,
    gen_catapult, gen_prayer_wheel, gen_wind_chime, gen_rocking_chair,
    gen_spinning_wheel, gen_water_pump, gen_bellows, gen_market_awning,
]

for gen in ASSETS:
    reset_scene()
    # Interpolation LINEAR par défaut (rotation/scale à vitesse constante,
    # évite l'effet « respiration » du easing Bézier par défaut sur les
    # boucles continues). Re-posé après chaque reset_scene.
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[menagerie] {len(ASSETS)} assets générés.")
