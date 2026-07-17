# Variantes ANIMÉES du pack flore : saule, bambou, blé et tournesols au vent,
# en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_flora_pack_animated.py
#
# Sortie : assets/models/nature_{willow,bamboo,wheat,sunflowers}_sway.glb
# (+ un preview PNG par asset, pose de repos).
#
# Recette rig/NLA identique à gen_nature_animated.py (elle-même reprise de
# gen_fairy_hero.py) : chaque partie est skinnée à 100 % sur un os (vertex
# group plein), UN clip « Idle » en boucle parfaite (première pose = dernière)
# poussé en piste NLA, export `export_animation_mode="NLA_TRACKS"` +
# `export_force_sampling`. Le moteur joue le clip via
# `AnimationState { clip: "Idle", .. }`, mesh partagé.
#
# Contraintes moteur :
# - base au sol z=0 Blender, Blender +Y → -Z jeu ;
# - la physique utilise le TriMesh de la POSE DE REPOS : seuls saule et bambou
#   sont pensés solides (tronc/cannes au centre, sondes à 0,6 m servies) ; blé
#   et tournesols restent non solides ;
# - budget MAX_SKINNED_INSTANCES partagé avec créatures et joueurs réseau →
#   la scène ne place qu'UNE instance de chaque asset animé (les versions
#   statiques du pack flore couvrent la masse) ;
# - piège connu : purger la pose résiduelle avant export (bake_idle le fait),
#   sinon la pose de liaison embarque la dernière pose keyframée.

import math
import os
import random

import bpy
import mathutils
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260719)  # reproductible

# Palette : mêmes teintes que gen_flora_pack.py / gen_flora_pack2.py.
BROWN = (0.32, 0.22, 0.11)
LEAF_LIGHT = (0.24, 0.50, 0.18)
WILLOW_LEAF = (0.35, 0.52, 0.28)
WILLOW_DARK = (0.26, 0.42, 0.22)
BAMBOO = (0.45, 0.58, 0.22)
BAMBOO_NODE = (0.35, 0.45, 0.18)
WHEAT = (0.78, 0.62, 0.25)
WHEAT_STALK = (0.62, 0.52, 0.22)
SUN_GOLD = (0.90, 0.60, 0.12)
SUN_HEART = (0.30, 0.20, 0.10)
SUN_STEM = (0.22, 0.42, 0.14)


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


def cube(bone, material, location, scale):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=location, rotation=rotation)

    return add_part(bone, material, op, location, scale)


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


def sphere(bone, material, location, scale, subdiv=2, jitter=0.0, smooth=False):
    def op(location, rotation):
        bpy.ops.mesh.primitive_ico_sphere_add(
            subdivisions=subdiv, radius=1.0, location=location
        )

    ob = add_part(bone, material, op, location, scale)
    if jitter > 0.0:
        for v in ob.data.vertices:
            v.co.x += rng.uniform(-jitter, jitter)
            v.co.y += rng.uniform(-jitter, jitter)
            v.co.z += rng.uniform(-jitter, jitter)
    if smooth:
        bpy.ops.object.shade_smooth()
    return ob


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
    """Clip « Idle » en piste NLA + purge de la pose résiduelle (piège connu :
    la pose de liaison embarquerait la dernière pose keyframée)."""
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


def export_and_preview(filename):
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
    print(f"[flore-anim] exporté {filename}")

    # Preview de la pose de repos : caméra 3/4 cadrée sur le mesh joint.
    mesh = next(o for o in bpy.context.scene.objects if o.type == "MESH")
    bpy.context.view_layer.update()
    pts = [(mesh.matrix_world @ v.co) for v in mesh.data.vertices]
    min_z = min(p.z for p in pts)
    max_z = max(p.z for p in pts)
    span = max(
        max(p.x for p in pts) - min(p.x for p in pts),
        max(p.y for p in pts) - min(p.y for p in pts),
        max_z - min_z,
    )
    target = (0.0, 0.0, (min_z + max_z) / 2)
    dist = span * 1.9 + 1.0
    cam_loc = (dist * 0.72, -dist * 0.72, target[2] + dist * 0.45)
    bpy.ops.object.camera_add(location=cam_loc)
    cam = bpy.context.active_object
    direction = mathutils.Vector(target) - mathutils.Vector(cam_loc)
    cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    scene = bpy.context.scene
    scene.camera = cam
    bpy.ops.object.light_add(type="SUN", location=(4, -3, 8))
    bpy.context.active_object.data.energy = 3.0
    bpy.context.active_object.rotation_euler = (math.radians(35), 0, math.radians(40))
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = OUT_DIR + filename.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print(f"[flore-anim] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Assets
# ---------------------------------------------------------------------------


def gen_willow_sway():
    """Saule pleureur ~3,8 m : tronc sur Root (solide), dôme sur Canopy et
    mèches en 3 groupes (Fringe1-3) qui ondulent en vagues déphasées — la
    variante vivante du nature_willow statique, posée près de l'eau."""
    trunk = mat("tronc_saule", BROWN)
    leaf = mat("feuille_saule", WILLOW_LEAF)
    leaf_d = mat("feuille_saule_sombre", WILLOW_DARK)
    cylinder("Root", trunk, (0, 0, 1.0), (0.26, 0.26, 2.0), vertices=12)
    sphere("Canopy", leaf, (0, 0, 2.9), (1.35, 1.35, 0.95), jitter=0.09)
    for i in range(9):
        a = i * math.tau / 9 + rng.uniform(-0.15, 0.15)
        r = rng.uniform(1.05, 1.3)
        x, y = r * math.cos(a), r * math.sin(a)
        top = rng.uniform(2.4, 2.7)
        bot = rng.uniform(0.8, 1.3)
        m = leaf if i % 2 == 0 else leaf_d
        cylinder(
            f"Fringe{i % 3 + 1}", m, (x, y, (top + bot) / 2),
            (rng.uniform(0.10, 0.15), rng.uniform(0.10, 0.15), top - bot), vertices=6,
        )

    bones = {"Canopy": ("Root", (0, 0, 2.0), (0, 0, 3.2))}
    for k in range(1, 4):
        bones[f"Fringe{k}"] = ("Canopy", (0, 0, 2.6), (0, 0, 0.9))
    arm = build_rig("WillowSway", bones)

    def keys(arm):
        # Houle lente : la canopée respire à peine, les mèches balancent en 3
        # vagues déphasées. Boucle 1 = 97.
        wave = ((1, 0.0), (25, 1.0), (49, 0.0), (73, -1.0), (97, 0.0))
        for f, s in wave:
            key_rot(arm, "Canopy", f, (s * 1.5, s * 2.5, 0))
        amps = [6.0, 8.0, 5.0]
        lags = [0, 32, 64]
        for k in range(1, 4):
            for f, s in wave:
                ff = ((f - 1 + lags[k - 1]) % 96) + 1
                key_rot(arm, f"Fringe{k}", ff, (s * amps[k - 1] * 0.5, s * amps[k - 1], 0))

    bake_idle(arm, 97, keys)
    export_and_preview("nature_willow_sway.glb")


def gen_bamboo_sway():
    """Touffe de bambous ~3,2 m : 6 cannes en 3 groupes (Cane1-3) qui plient
    doucement — variante vivante du nature_bamboo statique."""
    cane_m = mat("canne_bambou", BAMBOO)
    node_m = mat("noeud_bambou", BAMBOO_NODE)
    leaf = mat("feuille_bambou", LEAF_LIGHT)
    for i in range(6):
        a = i * math.tau / 6
        r = rng.uniform(0.10, 0.30)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(2.4, 3.2)
        bone = f"Cane{i % 3 + 1}"
        cylinder(bone, cane_m, (x, y, h / 2), (0.055, 0.055, h), vertices=7)
        for k in range(1, 4):
            cylinder(bone, node_m, (x, y, h * k / 4), (0.065, 0.065, 0.05), vertices=7)
        cone(bone, leaf, (x, y, h + 0.15), (0.22, 0.22, 0.55), vertices=5)

    bones = {f"Cane{k}": ("Root", (0, 0, 0.3), (0, 0, 3.0)) for k in range(1, 4)}
    arm = build_rig("BambooSway", bones)

    def keys(arm):
        # Flexion souple déphasée par groupe de cannes. Boucle 1 = 73.
        wave = ((1, 0.0), (19, 1.0), (37, 0.0), (55, -1.0), (73, 0.0))
        amps = [4.0, 5.5, 3.5]
        lags = [0, 24, 48]
        for k in range(1, 4):
            for f, s in wave:
                ff = ((f - 1 + lags[k - 1]) % 72) + 1
                key_rot(arm, f"Cane{k}", ff, (s * amps[k - 1] * 0.6, s * amps[k - 1], 0))

    bake_idle(arm, 73, keys)
    export_and_preview("nature_bamboo_sway.glb")


def gen_wheat_sway():
    """Touffe de blé ~0,8 m : 9 tiges en 3 groupes (Stalk1-3) couchées par le
    vent en vague — le champ qui ondule (non solide)."""
    stalk_m = mat("tige_ble", WHEAT_STALK)
    ear_m = mat("epi_ble", WHEAT)
    for i in range(9):
        a = i * math.tau / 9 + rng.uniform(-0.2, 0.2)
        r = rng.uniform(0.05, 0.32)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.55, 0.8)
        bone = f"Stalk{i % 3 + 1}"
        cylinder(bone, stalk_m, (x, y, h / 2), (0.018, 0.018, h), vertices=5)
        cone(bone, ear_m, (x, y, h + 0.10), (0.045, 0.045, 0.22), vertices=6)

    bones = {f"Stalk{k}": ("Root", (0, 0, 0.05), (0, 0, 0.85)) for k in range(1, 4)}
    arm = build_rig("WheatSway", bones)

    def keys(arm):
        # Vague appuyée dans un sens dominant (+Y) avec retour souple : le vent
        # couche le blé plus qu'il ne le redresse. Boucle 1 = 65.
        wave = ((1, 0.0), (17, 1.0), (33, 0.2), (49, -0.5), (65, 0.0))
        amps = [14.0, 18.0, 12.0]
        lags = [0, 8, 16]
        for k in range(1, 4):
            for f, s in wave:
                ff = ((f - 1 + lags[k - 1]) % 64) + 1
                key_rot(arm, f"Stalk{k}", ff, (s * amps[k - 1], s * amps[k - 1] * 0.3, 0))

    bake_idle(arm, 65, keys)
    export_and_preview("nature_wheat_sway.glb")


def gen_sunflowers_sway():
    """Tournesols ~1,5 m : 3 tiges sur Flower1-3, dodelinement lent et déphasé
    des têtes — le coin de potager qui vit (non solide)."""
    stem_m = mat("tige_tournesol", SUN_STEM)
    heart_m = mat("coeur_tournesol", SUN_HEART)
    petal_m = mat("petale_or", SUN_GOLD)
    for i in range(3):
        a = i * math.tau / 3
        r = 0.28 if i > 0 else 0.0
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(1.15, 1.5)
        bone = f"Flower{i + 1}"
        cylinder(bone, stem_m, (x, y, h / 2), (0.03, 0.03, h), vertices=6)
        sphere(bone, stem_m, (x + 0.10, y, h * 0.55), (0.12, 0.12, 0.042), subdiv=1)
        cylinder(
            bone, petal_m, (x, y - 0.03, h + 0.16), (0.20, 0.20, 0.05),
            rotation=(math.pi / 2, 0, 0), vertices=12,
        )
        cylinder(
            bone, heart_m, (x, y - 0.06, h + 0.16), (0.11, 0.11, 0.05),
            rotation=(math.pi / 2, 0, 0), vertices=10,
        )

    bones = {f"Flower{k}": ("Root", (0, 0, 0.05), (0, 0, 1.5)) for k in range(1, 4)}
    arm = build_rig("SunflowersSway", bones)

    def keys(arm):
        # Dodelinement lent, chaque fleur à son rythme. Boucle 1 = 97.
        wave = ((1, 0.0), (25, 1.0), (49, 0.0), (73, -1.0), (97, 0.0))
        amps = [4.0, 5.0, 3.5]
        lags = [0, 32, 64]
        for k in range(1, 4):
            for f, s in wave:
                ff = ((f - 1 + lags[k - 1]) % 96) + 1
                key_rot(arm, f"Flower{k}", ff, (s * amps[k - 1] * 0.7, s * amps[k - 1], 0))

    bake_idle(arm, 97, keys)
    export_and_preview("nature_sunflowers_sway.glb")


ASSETS = [gen_willow_sway, gen_bamboo_sway, gen_wheat_sway, gen_sunflowers_sway]

for gen in ASSETS:
    reset_scene()
    # Interpolation LINEAR pour des boucles à vitesse constante (re-posée après
    # chaque reset_scene : read_factory_settings réinitialise les préférences).
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[flore-anim] pack animé complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
