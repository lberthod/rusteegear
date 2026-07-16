# Génère 20 assets animés supplémentaires (2e vague : petite faune + méca-
# nismes de décor) en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_menagerie_pack2.py
#
# Sortie : assets/models/fauna_{squirrel,deer,owl,bat,snail,crab,duck,
# hedgehog,bee,mole}.glb et assets/models/nature_{lighthouse_lamp,seesaw,
# forge_hammer,weaving_loom,kite,merry_go_round,rope_swing,well_windlass,
# toll_gate,bell_tower}.glb.
#
# Même recette que gen_menagerie_pack.py / gen_nature_animated.py : armature
# minuscule (≤ 4 os utiles), un mesh joint par glb, vertex group plein par
# partie, UN clip « Idle » en boucle parfaite exporté en piste NLA
# (`export_animation_mode="NLA_TRACKS"` + `export_force_sampling`). Seuls des
# canaux rotation/scale sont keyframés (pas de translation d'os).
#
# Contraintes moteur reprises telles quelles : base au sol z=0 Blender
# (= y=0 jeu), Blender +Y → -Z jeu ; TriMesh physique = pose de repos, donc
# parties mobiles hors de portée du joueur ou asset explicitement non solide
# dans la scène ; purge de la pose résiduelle avant export ; une seule
# instance par asset animé (budget MAX_SKINNED_INSTANCES partagé).

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
STONE = (0.45, 0.44, 0.42)
STONE_DARK = (0.36, 0.35, 0.34)
METAL = (0.55, 0.55, 0.58)
METAL_DARK = (0.30, 0.30, 0.33)
ROPE = (0.62, 0.52, 0.30)
CANVAS_RED = (0.68, 0.16, 0.14)
CANVAS_CREAM = (0.85, 0.78, 0.62)
GLASS_AMBER = (1.0, 0.75, 0.30)
BRASS = (0.62, 0.50, 0.20)

FUR_SQUIRREL = (0.55, 0.32, 0.16)
FUR_SQUIRREL_BELLY = (0.88, 0.83, 0.72)
FUR_DEER = (0.58, 0.38, 0.22)
FUR_DEER_BELLY = (0.85, 0.78, 0.65)
ANTLER = (0.62, 0.54, 0.42)
FEATHER_OWL = (0.42, 0.34, 0.24)
FEATHER_OWL_FACE = (0.72, 0.64, 0.50)
EYE_YELLOW = (0.85, 0.68, 0.15)
WING_BAT = (0.16, 0.14, 0.16)
FUR_BAT = (0.22, 0.18, 0.16)
SHELL_SNAIL = (0.60, 0.42, 0.20)
BODY_SNAIL = (0.55, 0.42, 0.30)
SHELL_CRAB = (0.78, 0.28, 0.14)
SHELL_CRAB_DARK = (0.58, 0.18, 0.08)
FEATHER_DUCK = (0.30, 0.26, 0.10)
FEATHER_DUCK_HEAD = (0.10, 0.30, 0.16)
BEAK_DUCK = (0.85, 0.55, 0.10)
SPINE_HEDGEHOG = (0.35, 0.30, 0.24)
FACE_HEDGEHOG = (0.62, 0.52, 0.42)
BODY_BEE = (0.85, 0.68, 0.10)
STRIPE_BEE = (0.10, 0.09, 0.08)
WING_BEE = (0.85, 0.90, 0.92)
FUR_MOLE = (0.24, 0.22, 0.24)
EARTH_MOUND = (0.30, 0.22, 0.14)


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
    print(f"[menagerie2] exporté {filename}")


# ---------------------------------------------------------------------------
# Petite faune ambiante — non solide, hors physique de la scène.
# ---------------------------------------------------------------------------


def gen_squirrel():
    """Écureuil assis ~0,3 m grignotant : corps (Root) + queue touffue qui
    s'enroule/se déroule, tête qui hoche en grignotant."""
    fur = mat("fourrure_ecureuil", FUR_SQUIRREL)
    belly = mat("ventre_ecureuil", FUR_SQUIRREL_BELLY)
    sphere("Root", fur, (0, 0, 0.15), (0.12, 0.15, 0.16))
    sphere("Root", belly, (0, -0.06, 0.13), (0.08, 0.08, 0.10))
    sphere("Head", fur, (0, 0.10, 0.28), (0.09, 0.09, 0.09))
    sphere("Head", belly, (0, 0.02, 0.05), (0.02, 0.02, 0.02), segments=6, rings=4)
    cube("Tail", fur, (0, -0.22, 0.30), (0.09, 0.10, 0.30))

    arm = build_rig("Squirrel", {
        "Head": ("Root", (0, 0.06, 0.30), (0, 0.20, 0.30)),
        "Tail": ("Root", (0, -0.12, 0.16), (0, -0.14, 0.55)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (9, -14.0), (17, 2.0), (25, -8.0), (33, 0.0)):
            key_rot(arm, "Head", f, (-a, 0, 0))
        for f, a in ((1, 0.0), (17, 6.0), (33, -3.0)):
            key_rot(arm, "Tail", f, (a, 0, 0))

    bake_idle(arm, 33, keys)
    export("fauna_squirrel.glb")


def gen_deer():
    """Faon/biche ~1,1 m qui broute : corps (Root, socle plein → sondes) +
    tête/cou qui plonge vers l'herbe, oreilles et queue qui frémissent."""
    fur = mat("fourrure_biche", FUR_DEER)
    belly = mat("ventre_biche", FUR_DEER_BELLY)
    cube("Root", fur, (0, 0, 0.55), (0.28, 0.62, 0.30))
    for lx in (0.16, -0.16):
        for ly in (0.22, -0.22):
            cube("Root", fur, (lx, ly, 0.20), (0.06, 0.06, 0.40))  # pattes
    sphere("Head", fur, (0, 0.62, 0.68), (0.13, 0.16, 0.13))
    cube("EarL", belly, (0.10, 0.66, 0.78), (0.06, 0.02, 0.10))
    cube("EarR", belly, (-0.10, 0.66, 0.78), (0.06, 0.02, 0.10))
    cube("Tail", belly, (0, -0.62, 0.62), (0.06, 0.06, 0.10))

    arm = build_rig("Deer", {
        "Head": ("Root", (0, 0.50, 0.68), (0, 0.82, 0.68)),
        "EarL": ("Head", (0.08, 0.62, 0.78), (0.20, 0.62, 0.78)),
        "EarR": ("Head", (-0.08, 0.62, 0.78), (-0.20, 0.62, 0.78)),
        "Tail": ("Root", (0, -0.60, 0.62), (0, -0.72, 0.62)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (22, 46.0), (30, 50.0), (44, 0.0), (90, 0.0)):
            key_rot(arm, "Head", f, (-a, 0, 0))
        for f, a in ((1, 0.0), (58, 0.0), (63, -20.0), (68, 0.0), (90, 0.0)):
            key_rot(arm, "EarL", f, (0, 0, a))
            key_rot(arm, "EarR", f, (0, 0, -a))
        for f, a in ((1, 0.0), (75, 30.0), (82, 0.0), (90, 0.0)):
            key_rot(arm, "Tail", f, (a, 0, 0))

    bake_idle(arm, 90, keys)
    export("fauna_deer.glb")


def gen_owl():
    """Chouette perchée ~0,3 m : corps (Root) figé, tête qui pivote de côté
    (rotation caractéristique) et paupières qui clignent (scale)."""
    body_m = mat("plumage_chouette", FEATHER_OWL)
    face_m = mat("face_chouette", FEATHER_OWL_FACE)
    eye_m = mat("oeil_chouette", EYE_YELLOW)
    sphere("Root", body_m, (0, 0, 0.20), (0.15, 0.18, 0.20))
    cube("Root", body_m, (0.13, 0, 0.10), (0.05, 0.08, 0.12))
    cube("Root", body_m, (-0.13, 0, 0.10), (0.05, 0.08, 0.12))
    sphere("Head", face_m, (0, 0.02, 0.38), (0.13, 0.12, 0.12))
    sphere("Head", eye_m, (0.06, 0.11, 0.40), (0.035, 0.02, 0.035), segments=6, rings=4)
    sphere("Head", eye_m, (-0.06, 0.11, 0.40), (0.035, 0.02, 0.035), segments=6, rings=4)
    cone("Head", body_m, (0, 0.13, 0.35), (0.02, 0.05, 0.02), rotation=(math.pi / 2, 0, 0))

    arm = build_rig("Owl", {
        "Head": ("Root", (0, 0, 0.32), (0, 0, 0.50)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (30, 60.0), (60, -50.0), (90, 0.0)):
            key_rot(arm, "Head", f, (0, 0, a))
        for f, s in ((1, 1.0), (85, 1.0), (87, 0.1), (89, 1.0), (90, 1.0)):
            key_scale(arm, "Head", f, (1.0, 1.0, s))

    bake_idle(arm, 90, keys)
    export("fauna_owl.glb")


def gen_bat():
    """Chauve-souris ~0,25 m suspendue tête en bas à un moignon de branche
    planté au sol (Root, solide au pied — le perchoir tient debout tout
    seul, tout le mesh reste à z ≥ 0 : `demo_obj` n'offre pas d'offset en Y,
    contrairement à une vraie branche d'arbre en hauteur) : ailes repliées
    qui respirent lentement."""
    fur_m = mat("fourrure_chauve_souris", FUR_BAT)
    wing_m = mat("aile_chauve_souris", WING_BAT)
    dark = mat("bois_perchoir_chauve_souris", WOOD_DARK)
    cylinder("Root", dark, (0, 0, 0.28), (0.03, 0.03, 0.55))
    sphere("Body", fur_m, (0, 0, 0.32), (0.08, 0.10, 0.12))
    sphere("Body", fur_m, (0, 0.02, 0.20), (0.05, 0.05, 0.05))  # tête
    cube("WingL", wing_m, (0.10, 0, 0.34), (0.05, 0.03, 0.18))
    cube("WingR", wing_m, (-0.10, 0, 0.34), (0.05, 0.03, 0.18))

    arm = build_rig("Bat", {
        "Body": ("Root", (0, 0, 0.50), (0, 0, 0.20)),
        "WingL": ("Body", (0.06, 0, 0.34), (0.20, 0, 0.34)),
        "WingR": ("Body", (-0.06, 0, 0.34), (-0.20, 0, 0.34)),
    })

    def keys(arm):
        breathe = ((1, 1.0), (25, 1.12), (49, 1.0), (73, 1.12), (97, 1.0))
        for f, s in breathe:
            key_scale(arm, "Body", f, (s, s, 1.0 / max(s, 1.0)))
            key_rot(arm, "WingL", f, (0, 0, (s - 1.0) * -40.0))
            key_rot(arm, "WingR", f, (0, 0, (s - 1.0) * 40.0))

    bake_idle(arm, 97, keys)
    export("fauna_bat.glb")


def gen_snail():
    """Escargot ~0,15 m : coquille fixe (Root) + 2 tentacules oculaires qui
    ondulent lentement."""
    shell_m = mat("coquille_escargot", SHELL_SNAIL)
    body_m = mat("corps_escargot", BODY_SNAIL)
    sphere("Root", shell_m, (0, -0.02, 0.09), (0.10, 0.10, 0.09))
    cube("Root", body_m, (0, 0.05, 0.03), (0.05, 0.14, 0.04))
    cylinder("TentacleL", body_m, (0.03, 0.16, 0.07), (0.012, 0.012, 0.08))
    cylinder("TentacleR", body_m, (-0.03, 0.16, 0.07), (0.012, 0.012, 0.08))

    arm = build_rig("Snail", {
        "TentacleL": ("Root", (0.03, 0.16, 0.05), (0.05, 0.19, 0.16)),
        "TentacleR": ("Root", (-0.03, 0.16, 0.05), (-0.05, 0.19, 0.16)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, 12.0), (33, -8.0), (49, 0.0)):
            key_rot(arm, "TentacleL", f, (a, 0, a * 0.3))
            key_rot(arm, "TentacleR", f, (a, 0, -a * 0.3))

    bake_idle(arm, 49, keys)
    export("fauna_snail.glb")


def gen_crab():
    """Crabe de plage ~0,2 m : carapace (Root) + 2 pinces qui s'ouvrent et se
    referment en alternance."""
    shell_m = mat("carapace_crabe", SHELL_CRAB)
    dark_m = mat("carapace_crabe_sombre", SHELL_CRAB_DARK)
    sphere("Root", shell_m, (0, 0, 0.08), (0.16, 0.13, 0.07))
    for lx in (0.12, -0.12):
        for ly in (0.06, -0.06):
            cube("Root", dark_m, (lx, ly, 0.03), (0.02, 0.02, 0.06))
    cube("ClawL", dark_m, (0.20, 0.05, 0.09), (0.06, 0.05, 0.05))
    cube("ClawL", dark_m, (0.24, 0.10, 0.09), (0.03, 0.04, 0.03))
    cube("ClawR", dark_m, (-0.20, 0.05, 0.09), (0.06, 0.05, 0.05))
    cube("ClawR", dark_m, (-0.24, 0.10, 0.09), (0.03, 0.04, 0.03))

    arm = build_rig("Crab", {
        "ClawL": ("Root", (0.14, 0, 0.09), (0.30, 0.08, 0.09)),
        "ClawR": ("Root", (-0.14, 0, 0.09), (-0.30, 0.08, 0.09)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (13, 22.0), (25, 0.0), (37, 22.0), (49, 0.0)):
            key_rot(arm, "ClawL", f, (0, 0, -a))
            key_rot(arm, "ClawR", f, (0, 0, a))

    bake_idle(arm, 49, keys)
    export("fauna_crab.glb")


def gen_duck():
    """Canard ~0,3 m flottant sur l'eau : corps (Root) + tête qui plonge pour
    picorer (dandinement), queue qui frétille."""
    body_m = mat("plumage_canard", FEATHER_DUCK)
    head_m = mat("plumage_tete_canard", FEATHER_DUCK_HEAD)
    beak_m = mat("bec_canard", BEAK_DUCK)
    sphere("Root", body_m, (0, 0, 0.13), (0.15, 0.22, 0.13))
    sphere("Head", head_m, (0, 0.20, 0.24), (0.08, 0.08, 0.08))
    cone("Head", beak_m, (0, 0.30, 0.22), (0.03, 0.09, 0.025),
         rotation=(math.pi / 2, 0, 0))
    cube("Tail", body_m, (0, -0.24, 0.18), (0.06, 0.10, 0.08))

    arm = build_rig("Duck", {
        "Head": ("Root", (0, 0.14, 0.22), (0, 0.14, 0.34)),
        "Tail": ("Root", (0, -0.16, 0.15), (0, -0.30, 0.15)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (11, 70.0), (17, 74.0), (27, 0.0), (55, 0.0)):
            key_rot(arm, "Head", f, (-a, 0, 0))
        for f, a in ((1, 0.0), (34, 10.0), (41, -10.0), (48, 0.0), (55, 0.0)):
            key_rot(arm, "Tail", f, (0, 0, a))

    bake_idle(arm, 55, keys)
    export("fauna_duck.glb")


def gen_hedgehog():
    """Hérisson ~0,15 m : corps hérissé (Root) qui respire, museau qui
    renifle (scale)."""
    spine_m = mat("piquants_herisson", SPINE_HEDGEHOG)
    face_m = mat("museau_herisson", FACE_HEDGEHOG)
    sphere("Body", spine_m, (0, 0, 0.08), (0.13, 0.16, 0.10))
    sphere("Body", face_m, (0, 0.14, 0.06), (0.06, 0.06, 0.06))
    sphere("Nose", face_m, (0, 0.20, 0.05), (0.02, 0.02, 0.02), segments=6, rings=4)

    arm = build_rig("Hedgehog", {
        "Body": ("Root", (0, 0, 0.02), (0, 0, 0.14)),
        "Nose": ("Body", (0, 0.16, 0.05), (0, 0.24, 0.05)),
    })

    def keys(arm):
        breathe = ((1, 1.0), (17, 1.10), (33, 1.0), (49, 1.10), (65, 1.0))
        for f, s in breathe:
            key_scale(arm, "Body", f, (s, s, s))
        sniff = ((1, 1.0), (9, 1.4), (17, 1.0), (65, 1.0))
        for f, s in sniff:
            key_scale(arm, "Nose", f, (s, s, s))

    bake_idle(arm, 65, keys)
    export("fauna_hedgehog.glb")


def gen_bee():
    """Abeille ~0,05 m butinant sur place : corps rayé (Glow) qui vibre + 2
    ailes qui battent très vite, portées par un bras (Boom) en dérive lente."""
    body_m = mat("corps_abeille", BODY_BEE)
    stripe_m = mat("rayure_abeille", STRIPE_BEE)
    wing_m = mat("aile_abeille", WING_BEE)
    sphere("Glow", body_m, (0, 0, 0.25), (0.035, 0.05, 0.035), segments=6, rings=4)
    cube("Glow", stripe_m, (0, 0.01, 0.25), (0.037, 0.012, 0.037))
    cube("Glow", stripe_m, (0, -0.015, 0.25), (0.037, 0.012, 0.037))
    cube("Glow", wing_m, (0.035, 0, 0.27), (0.03, 0.015, 0.015))
    cube("Glow", wing_m, (-0.035, 0, 0.27), (0.03, 0.015, 0.015))

    arm = build_rig("Bee", {
        "Boom": ("Root", (0, 0, 0.05), (0, 0, 0.20)),
        "Glow": ("Boom", (0, 0, 0.20), (0, 0, 0.30)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (11, 10.0), (21, -8.0), (31, 6.0), (41, 0.0)):
            key_rot(arm, "Boom", f, (a * 0.5, a, 0))
        for f, s in ((1, 1.0), (3, 1.2), (5, 0.9), (7, 1.2), (9, 1.0)):
            key_scale(arm, "Glow", f, (1.0, 1.0, s))

    bake_idle(arm, 41, keys)
    export("fauna_bee.glb")


def gen_mole():
    """Taupe ~0,2 m qui pointe hors de sa taupinière : monticule de terre
    (Root, solide) + tête/buste qui émerge puis se ré-enfonce, en boucle."""
    earth_m = mat("terre_taupiniere", EARTH_MOUND)
    fur_m = mat("fourrure_taupe", FUR_MOLE)
    cone("Root", earth_m, (0, 0, 0.06), (0.30, 0.30, 0.12), vertices=10)
    sphere("Head", fur_m, (0, 0, 0.10), (0.10, 0.12, 0.09))
    cone("Head", fur_m, (0, 0.12, 0.08), (0.04, 0.06, 0.04),
         rotation=(math.pi / 2, 0, 0))

    arm = build_rig("Mole", {
        "Head": ("Root", (0, 0, 0.06), (0, 0, 0.26)),
    })

    def keys(arm):
        for f, a in ((1, -70.0), (13, 0.0), (23, 5.0), (35, 0.0), (48, -70.0)):
            key_rot(arm, "Head", f, (a, 0, 0))

    bake_idle(arm, 48, keys)
    export("fauna_mole.glb")


# ---------------------------------------------------------------------------
# Mécanismes de décor.
# ---------------------------------------------------------------------------


def gen_lighthouse_lamp():
    """Phare côtier ~4 m (tour de pierre, Root — solide) : lanterne vitrée qui
    tourne en continu au sommet."""
    stone = mat("pierre_phare", STONE)
    stripe = mat("rayure_phare", CANVAS_RED)
    glass = mat("verre_phare", GLASS_AMBER)
    metal = mat("metal_phare", METAL_DARK)
    for i in range(4):
        z = 0.5 + i * 0.9
        m = stone if i % 2 == 0 else stripe
        cylinder("Root", m, (0, 0, z), (0.9 - i * 0.06, 0.9 - i * 0.06, 0.95), vertices=12)
    cylinder("Root", metal, (0, 0, 4.05), (0.7, 0.7, 0.12), vertices=12)
    cylinder("Lamp", glass, (0, 0, 4.35), (0.5, 0.5, 0.4), vertices=10)
    cone("Lamp", metal, (0, 0, 4.68), (0.55, 0.55, 0.25), vertices=10)

    arm = build_rig("LighthouseLamp", {
        "Lamp": ("Root", (0, 0, 4.15), (0, 0, 4.8)),
    })

    def keys(arm):
        for f, ang in ((1, 0.0), (33, 120.0), (65, 240.0), (97, 360.0)):
            key_rot(arm, "Lamp", f, (0, 0, ang))

    bake_idle(arm, 97, keys)
    export("nature_lighthouse_lamp.glb")


def gen_seesaw():
    """Bascule à balancier ~2,4 m (support central fixe, Root — solide) :
    planche qui bascule alternativement de part et d'autre du pivot."""
    wood = mat("bois_bascule2", WOOD)
    dark = mat("bois_sombre_bascule2", WOOD_DARK)
    cube("Root", dark, (0, 0, 0.35), (0.20, 0.20, 0.7))
    cube("Plank", wood, (0, 0, 0.62), (0.22, 2.2, 0.10))
    cube("Plank", dark, (0.9, 0, 0.72), (0.06, 0.06, 0.20))
    cube("Plank", dark, (-0.9, 0, 0.72), (0.06, 0.06, 0.20))

    arm = build_rig("Seesaw", {
        "Plank": ("Root", (0, 0, 0.62), (0, 1.0, 0.62)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (25, 18.0), (50, -18.0), (75, 0.0)):
            key_rot(arm, "Plank", f, (a, 0, 0))

    bake_idle(arm, 75, keys)
    export("nature_seesaw.glb")


def gen_forge_hammer():
    """Marteau-pilon de forge ~2 m (bâti solide, Root) : tête de marteau
    (Hammer) qui se lève puis frappe l'enclume en boucle."""
    wood = mat("bois_pilon", WOOD_DARK)
    metal = mat("metal_pilon", METAL_DARK)
    metal_l = mat("metal_pilon_clair", METAL)
    cube("Root", wood, (-0.5, 0, 1.0), (0.16, 0.16, 2.0))
    cube("Root", wood, (0.5, 0, 1.0), (0.16, 0.16, 2.0))
    cube("Root", wood, (0, 0, 1.95), (1.1, 0.20, 0.14))
    cube("Root", metal_l, (0, 0, 0.35), (0.35, 0.35, 0.35))  # enclume
    cube("Hammer", metal, (0, 0, 1.6), (0.10, 0.10, 0.9))
    cube("Hammer", metal, (0, 0, 1.15), (0.28, 0.28, 0.20))

    arm = build_rig("ForgeHammer", {
        "Hammer": ("Root", (0, 0, 1.9), (0, 0, 1.0)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (10, 55.0), (16, 60.0), (18, 0.0), (32, 0.0),
                     (41, 55.0), (47, 60.0), (49, 0.0)):
            key_rot(arm, "Hammer", f, (0, a, 0))

    bake_idle(arm, 49, keys)
    export("nature_forge_hammer.glb")


def gen_weaving_loom():
    """Métier à tisser ~1,4 m (cadre fixe, Root — solide) : peigne-battant
    (Beam) qui va-et-vient en tassant la trame, en boucle."""
    wood = mat("bois_metier", WOOD)
    dark = mat("bois_sombre_metier", WOOD_DARK)
    thread_m = mat("fil_metier", CANVAS_CREAM)
    for x in (-0.55, 0.55):
        cube("Root", wood, (x, 0, 0.7), (0.10, 0.10, 1.4))
    cube("Root", wood, (0, 0, 1.35), (1.1, 0.10, 0.10))
    cube("Root", thread_m, (0, 0.15, 0.55), (0.9, 0.02, 0.55))  # trame tissée
    cube("Beam", dark, (0, 0.02, 0.75), (0.95, 0.06, 0.7))

    arm = build_rig("WeavingLoom", {
        "Beam": ("Root", (0, -0.1, 1.2), (0, 0.6, 0.5)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (11, -18.0), (17, 0.0), (27, -18.0), (33, 0.0)):
            key_rot(arm, "Beam", f, (a, 0, 0))

    bake_idle(arm, 33, keys)
    export("nature_weaving_loom.glb")


def gen_kite():
    """Cerf-volant ~0,6 m en vol, retenu par un fil (non solide, décor pur) :
    losange (Root) qui tangue au vent + queue à rubans qui ondule."""
    fabric_m = mat("toile_cerf_volant", CANVAS_RED)
    fabric2_m = mat("toile_cerf_volant_creme", CANVAS_CREAM)
    dark = mat("baguette_cerf_volant", WOOD_DARK)
    cube("Root", fabric_m, (0.15, 0, 0.15), (0.02, 0.30, 0.30), rotation=(0, 0, math.pi / 4))
    cube("Root", fabric2_m, (-0.15, 0, 0.15), (0.02, 0.30, 0.30), rotation=(0, 0, math.pi / 4))
    cube("Root", dark, (0, 0, 0.15), (0.02, 0.42, 0.02))
    cube("Root", dark, (0, 0, 0.15), (0.02, 0.02, 0.42))
    for i, z in enumerate((-0.10, -0.30, -0.50)):
        cube(f"Tail{i+1}", fabric_m if i % 2 == 0 else fabric2_m, (0, 0, z),
             (0.015, 0.10, 0.05))

    arm = build_rig("Kite", {
        "Sway": ("Root", (0, 0, 0.15), (0, 0.4, 0.15)),
        "Tail1": ("Root", (0, 0, -0.02), (0, 0, -0.20)),
        "Tail2": ("Tail1", (0, 0, -0.20), (0, 0, -0.40)),
        "Tail3": ("Tail2", (0, 0, -0.40), (0, 0, -0.60)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (17, 10.0), (33, -6.0), (49, 4.0), (65, 0.0)):
            key_rot(arm, "Sway", f, (a * 0.5, 0, a))
        wave = ((1, 0.0), (17, 1.0), (33, 0.0), (49, -1.0), (65, 0.0))
        for i, bone in enumerate(("Tail1", "Tail2", "Tail3")):
            lag = i * 6
            for f, s in wave:
                ff = f + lag if f + lag <= 65 else f + lag - 64
                key_rot(arm, bone, ff, (0, 0, s * 20.0))

    bake_idle(arm, 65, keys)
    export("nature_kite.glb")


def gen_merry_go_round():
    """Petit manège de foire ~2,6 m (socle et mât fixes, Root — solide) :
    plateforme et montures tournent en continu autour du mât."""
    stripe_a = mat("toile_manege_rouge", CANVAS_RED)
    stripe_b = mat("toile_manege_creme", CANVAS_CREAM)
    metal = mat("metal_manege", METAL_DARK)
    dark = mat("bois_sombre_manege", WOOD_DARK)
    cylinder("Root", dark, (0, 0, 0.06), (1.3, 1.3, 0.12), vertices=16)
    cylinder("Root", metal, (0, 0, 1.3), (0.05, 0.05, 2.5))
    for i in range(8):
        a = i * math.tau / 8
        m = stripe_a if i % 2 == 0 else stripe_b
        cube("Platform", m, (1.05 * math.cos(a), 1.05 * math.sin(a), 0.16),
             (0.42, 0.42, 0.06))
    for i in range(4):
        a = i * math.tau / 4
        cube("Platform", dark, (0.8 * math.cos(a), 0.8 * math.sin(a), 0.4),
             (0.06, 0.06, 0.5))
        cube("Platform", stripe_a if i % 2 == 0 else stripe_b,
             (0.8 * math.cos(a), 0.8 * math.sin(a), 0.55), (0.16, 0.30, 0.16))
    cone("Root", metal, (0, 0, 2.7), (1.5, 1.5, 0.7), vertices=16)

    arm = build_rig("MerryGoRound", {
        "Platform": ("Root", (0, 0, 0.16), (0, 0, 1.0)),
    })

    def keys(arm):
        for f, ang in ((1, 0.0), (33, 120.0), (65, 240.0), (97, 360.0)):
            key_rot(arm, "Platform", f, (0, 0, ang))

    bake_idle(arm, 97, keys)
    export("nature_merry_go_round.glb")


def gen_rope_swing():
    """Balançoire à corde ~2 m, portique autoportant (bâti en A, Root —
    solide, tient debout tout seul : `demo_obj` n'offre pas d'offset en Y
    pour un vrai ancrage de branche en hauteur) : assise en bois suspendue
    à la traverse qui se balance en pendule."""
    rope_m = mat("corde_balancoire", ROPE)
    wood = mat("bois_balancoire", WOOD)
    dark = mat("bois_sombre_balancoire", WOOD_DARK)
    for side in (-0.9, 0.9):
        for lean in (-0.35, 0.35):
            post = cylinder("Root", dark, (side, lean, 1.0), (0.05, 0.05, 2.0), vertices=8)
            post.rotation_euler = (0, math.atan2(lean, 2.0) * -1, 0)
    cube("Root", dark, (0, 0, 1.95), (1.9, 0.08, 0.08))
    cylinder("Swing", rope_m, (0.14, 0, 1.6), (0.012, 0.012, 0.7))
    cylinder("Swing", rope_m, (-0.14, 0, 1.6), (0.012, 0.012, 0.7))
    cube("Swing", wood, (0, 0, 1.2), (0.35, 0.16, 0.04))

    arm = build_rig("RopeSwing", {
        "Swing": ("Root", (0, 0, 1.95), (0, 0, 1.2)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (20, 20.0), (40, -20.0), (60, 0.0)):
            key_rot(arm, "Swing", f, (a, 0, 0))

    bake_idle(arm, 60, keys)
    export("nature_rope_swing.glb")


def gen_well_windlass():
    """Puits à manivelle ~1,3 m (margelle de pierre, Root — solide) : manivelle
    qui tourne en continu, enroulant la corde du seau."""
    stone = mat("pierre_puits_manivelle", STONE)
    wood = mat("bois_puits_manivelle", WOOD_DARK)
    metal = mat("metal_puits_manivelle", METAL_DARK)
    rope_m = mat("corde_puits_manivelle", ROPE)
    cylinder("Root", stone, (0, 0, 0.35), (0.55, 0.55, 0.7), vertices=12)
    for x in (-0.5, 0.5):
        cube("Root", wood, (x, 0, 1.0), (0.08, 0.08, 0.7))
    cube("Root", wood, (0, 0, 1.35), (1.1, 0.08, 0.08))
    cylinder("Crank", metal, (0, 0, 1.0), (0.06, 0.06, 1.0),
              rotation=(math.pi / 2, 0, 0))
    cube("Crank", metal, (0, 0.5, 1.0), (0.10, 0.30, 0.03))
    cylinder("Crank", rope_m, (0, 0.5, 0.85), (0.02, 0.02, 0.10))

    arm = build_rig("WellWindlass", {
        "Crank": ("Root", (0, 0, 1.0), (0, 0.5, 1.0)),
    })

    def keys(arm):
        for f, ang in ((1, 0.0), (25, 120.0), (49, 240.0), (73, 360.0)):
            key_rot(arm, "Crank", f, (ang, 0, 0))

    bake_idle(arm, 73, keys)
    export("nature_well_windlass.glb")


def gen_toll_gate():
    """Barrière de péage ~1,8 m (poteau fixe, Root — solide) : bras rayé qui
    se lève puis se rabaisse en boucle."""
    wood = mat("poteau_peage", STONE_DARK)
    stripe_a = mat("rayure_peage_rouge", CANVAS_RED)
    stripe_b = mat("rayure_peage_creme", CANVAS_CREAM)
    cylinder("Root", wood, (0, 0, 0.65), (0.12, 0.12, 1.3), vertices=8)
    for i in range(6):
        m = stripe_a if i % 2 == 0 else stripe_b
        cube("Arm", m, (0.25 + i * 0.30, 0, 1.25), (0.15, 0.10, 0.08))

    arm = build_rig("TollGate", {
        "Arm": ("Root", (0, 0, 1.25), (1.8, 0, 1.25)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (25, 0.0), (45, 75.0), (65, 75.0), (85, 0.0), (110, 0.0)):
            key_rot(arm, "Arm", f, (0, -a, 0))

    bake_idle(arm, 110, keys)
    export("nature_toll_gate.glb")


def gen_bell_tower():
    """Clocheton ~2,2 m (support de bois, Root — solide) : cloche de bronze
    qui se balance et frappe son battant, en boucle."""
    wood = mat("bois_clocheton", WOOD_DARK)
    bronze = mat("bronze_cloche", BRASS)
    metal = mat("metal_cloche", METAL_DARK)
    for x in (-0.45, 0.45):
        cube("Root", wood, (x, 0, 1.0), (0.10, 0.10, 2.0))
    cube("Root", wood, (0, 0, 1.95), (0.9, 0.9, 0.10))
    cone("Bell", bronze, (0, 0, 1.5), (0.35, 0.35, 0.5), vertices=12)
    sphere("Bell", metal, (0, 0, 1.25), (0.05, 0.05, 0.08), segments=6, rings=4)

    arm = build_rig("BellTower", {
        "Bell": ("Root", (0, 0, 1.9), (0, 0, 1.2)),
    })

    def keys(arm):
        for f, a in ((1, 0.0), (13, 18.0), (25, -18.0), (37, 8.0), (49, 0.0)):
            key_rot(arm, "Bell", f, (0, a, 0))

    bake_idle(arm, 49, keys)
    export("nature_bell_tower.glb")


ASSETS = [
    gen_squirrel, gen_deer, gen_owl, gen_bat, gen_snail, gen_crab, gen_duck,
    gen_hedgehog, gen_bee, gen_mole,
    gen_lighthouse_lamp, gen_seesaw, gen_forge_hammer, gen_weaving_loom,
    gen_kite, gen_merry_go_round, gen_rope_swing, gen_well_windlass,
    gen_toll_gate, gen_bell_tower,
]

for gen in ASSETS:
    reset_scene()
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[menagerie2] {len(ASSETS)} assets générés.")
