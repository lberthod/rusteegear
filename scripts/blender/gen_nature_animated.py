# Génère le décor nature ANIMÉ de la démo MMORPG (moulins, bannière, feu de
# camp, épouvantail) en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_nature_animated.py
#
# Sortie : assets/models/nature_{watermill,windmill,banner,campfire,scarecrow}.glb.
# Contrairement à gen_nature_pack.py (meshes statiques), chaque asset est rigué
# (armature minuscule, ≤ 4 os utiles) et porte UN clip « Idle » en boucle
# parfaite (première pose = dernière pose) — le moteur le joue via
# `AnimationState { clip: "Idle", .. }` par instance, mesh partagé (même
# mécanique que les créatures). Recette rig/NLA reprise de gen_fairy_hero.py :
# vertex group plein par partie, action nommée poussée en piste NLA,
# `export_animation_mode="NLA_TRACKS"` + `export_force_sampling`.
#
# Contraintes moteur (cf. gen_nature_pack.py pour le détail) :
# - base au sol z=0 Blender (= y=0 jeu), Blender +Y → -Z jeu (face « avant »).
# - la physique du jeu utilise le TriMesh de la POSE DE REPOS : les parties
#   mobiles (roue, pales) sont placées hors de portée du joueur (en hauteur ou
#   côté eau) ; feu et épouvantail sont non solides dans la scène.
# - piège connu du pipeline : purger la pose résiduelle avant export, sinon la
#   pose de liaison embarque la dernière pose keyframée.
# - budget : MAX_SKINNED_INSTANCES du renderer est partagé avec créatures et
#   joueurs réseau → la scène ne place qu'UNE instance de chaque asset animé.

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

# Palette commune — mêmes valeurs que gen_nature_pack.py (direction artistique
# de la carte : le décor animé ne détonne pas du décor statique).
BROWN = (0.32, 0.22, 0.11)
WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
STONE = (0.45, 0.44, 0.42)
STONE_DARK = (0.36, 0.35, 0.34)
ROOF = (0.50, 0.22, 0.14)
THATCH = (0.62, 0.48, 0.20)
ACCENT_RED = (0.72, 0.14, 0.12)
GLOW_YELLOW = (1.0, 0.78, 0.35)
FLAME_ORANGE = (0.95, 0.45, 0.10)


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
    pose résiduelle (piège connu : la pose de liaison embarquerait la dernière
    pose keyframée)."""
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
    print(f"[nature-anim] exporté {filename}")


# ---------------------------------------------------------------------------
# Assets
# ---------------------------------------------------------------------------


def gen_watermill():
    """Moulin à eau ~3.5 m : bâtiment de pierre (Root, solide dans la scène) +
    roue à aubes sur l'os « Wheel », rotation continue 360°/boucle. La roue est
    côté +X (posé berge ouest, roue côté rivière — hors de portée du joueur)."""
    stone = mat("pierre", STONE)
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    # Bâtiment (Root) : socle plein → flancs visibles des sondes à 0,6 m.
    cube("Root", stone, (0, 0, 1.1), (2.6, 2.2, 2.2))
    for side in (-1, 1):
        pan = cube("Root", roof_m, (side * 0.72, 0, 2.45), (1.7, 2.4, 0.12))
        pan.rotation_euler = (0, side * math.radians(38), 0)
    cube("Root", roof_m, (0, 0, 2.98), (0.3, 2.5, 0.14))
    cube("Root", dark, (0.0, 1.12, 0.8), (0.7, 0.12, 1.6))  # porte côté +Y
    # Axe et roue (Wheel) : axle à x=1.65, z=1.0 ; l'os tourne autour de X monde.
    cylinder("Wheel", dark, (1.65, 0, 1.0), (0.09, 0.09, 0.7),
             rotation=(0, math.pi / 2, 0))
    cylinder("Wheel", wood, (1.85, 0, 1.0), (1.0, 1.0, 0.12),
             rotation=(0, math.pi / 2, 0), vertices=12)
    for i in range(8):
        a = i * math.tau / 8
        pale = cube("Wheel", dark,
                    (1.85, 1.05 * math.cos(a), 1.0 + 1.05 * math.sin(a)),
                    (0.14, 0.5, 0.30))
        pale.rotation_euler = (a, 0, 0)

    arm = build_rig("Watermill", {
        # Os horizontal le long de +X : sa rotation locale Y fait tourner la
        # roue dans le plan YZ monde (comme un axe de moulin réel).
        "Wheel": ("Root", (1.65, 0, 1.0), (2.1, 0, 1.0)),
    })

    def keys(arm):
        # Boucle parfaite : 0° → 360° sur 96 frames (rotation continue, la
        # frame 97 rejoindrait la frame 1). Interpolation linéaire (cf.
        # `keyframe_new_interpolation_type` global) pour une vitesse constante.
        for f, ang in ((1, 0.0), (33, 120.0), (65, 240.0), (97, 360.0)):
            key_rot(arm, "Wheel", f, (0, ang, 0))

    bake_idle(arm, 97, keys)
    export("nature_watermill.glb")


def gen_windmill():
    """Moulin à vent des rizières ~4.5 m : tour de torchis (Root, socle solide)
    + croix de 4 pales sur l'os « Blades » à 3,4 m (hors de portée du joueur),
    rotation continue face +Y (= -Z jeu : posé au nord des rizières, pales vers
    elles)."""
    wall_m = mat("torchis", (0.55, 0.45, 0.32))
    thatch_m = mat("chaume", THATCH)
    dark = mat("bois_sombre", WOOD_DARK)
    wood = mat("bois", WOOD)
    cone("Root", wall_m, (0, 0, 1.6), (1.5, 1.5, 3.2), vertices=12)
    cone("Root", thatch_m, (0, 0, 3.6), (1.15, 1.15, 1.2), vertices=12)
    cube("Root", dark, (0.0, 1.02, 0.8), (0.65, 0.14, 1.6))  # porte
    # Moyeu + 4 pales sur l'avant (+Y), axe de rotation = Y monde.
    cylinder("Blades", dark, (0, 1.05, 3.4), (0.14, 0.14, 0.5),
             rotation=(math.pi / 2, 0, 0))
    for i in range(4):
        a = i * math.tau / 4
        pale = cube("Blades", wood,
                    (1.15 * math.cos(a), 1.25, 3.4 + 1.15 * math.sin(a)),
                    (0.32, 0.06, 1.9))
        pale.rotation_euler = (0, -a, 0)

    arm = build_rig("Windmill", {
        # Os vers +Y (l'axe des pales) : rotation locale Y = rotation des pales
        # dans le plan XZ monde.
        "Blades": ("Root", (0, 1.05, 3.4), (0, 1.5, 3.4)),
    })

    def keys(arm):
        for f, ang in ((1, 0.0), (41, 120.0), (81, 240.0), (121, 360.0)):
            key_rot(arm, "Blades", f, (0, ang, 0))

    bake_idle(arm, 121, keys)
    export("nature_windmill.glb")


def gen_banner():
    """Bannière du hameau ~2.6 m (accent rouge) : mât sur socle de pierre plein
    (Root — solide, visible des sondes) + drap en 3 segments (chaîne Cloth1-3)
    qui ondule. Drap vers +X."""
    stone = mat("pierre", STONE)
    dark = mat("bois_sombre", WOOD_DARK)
    red = mat("laque_rouge", ACCENT_RED)
    cylinder("Root", stone, (0, 0, 0.4), (0.38, 0.38, 0.8), vertices=8)
    cube("Root", dark, (0, 0, 1.55), (0.12, 0.12, 2.3))
    cube("Root", dark, (0.35, 0, 2.62), (0.7, 0.08, 0.08))  # potence
    # Drap : 3 segments de haut en bas, chacun 100 % sur son os (la chaîne
    # d'os fait l'ondulation ; un skinning dégradé serait plus doux mais les
    # 3 segments suffisent au style low-poly).
    cube("Cloth1", red, (0.42, 0, 2.30), (0.55, 0.05, 0.5))
    cube("Cloth2", red, (0.42, 0, 1.85), (0.55, 0.05, 0.44))
    cube("Cloth3", red, (0.42, 0, 1.45), (0.55, 0.05, 0.38))

    arm = build_rig("Banner", {
        "Cloth1": ("Root", (0.42, 0, 2.56), (0.42, 0, 2.10)),
        "Cloth2": ("Cloth1", (0.42, 0, 2.10), (0.42, 0, 1.66)),
        "Cloth3": ("Cloth2", (0.42, 0, 1.66), (0.42, 0, 1.26)),
    })

    def keys(arm):
        # Ondulation : chaque segment oscille en Y (torsion autour du mât) avec
        # un déphasage croissant — le bas « suit » le haut. Boucle parfaite :
        # frame 1 = frame 73.
        wave = ((1, 0.0), (19, 1.0), (37, 0.0), (55, -1.0), (73, 0.0))
        for seg, (amp, lag) in (("Cloth1", (8.0, 0)), ("Cloth2", (12.0, 6)), ("Cloth3", (16.0, 12))):
            for f, s in wave:
                key_rot(arm, seg, f + lag, (s * amp * 0.35, s * amp, 0))
            # Ferme la boucle du segment déphasé : rejoue les premières clés
            # décalées d'un cycle pour que 1..73 reste continu.
            if lag:
                for f, s in wave[:2]:
                    if f + lag - 72 >= 1:
                        key_rot(arm, seg, f + lag - 72, (s * amp * 0.35, s * amp, 0))

    bake_idle(arm, 73, keys)
    export("nature_banner.glb")


def gen_campfire():
    """Feu de camp (non solide dans la scène) : cercle de pierres + bûches
    (Root) et 3 flammes (Flame1-3) qui pulsent en boucle."""
    stone = mat("pierre_sombre", STONE_DARK)
    trunk = mat("tronc", BROWN)
    flame = mat("flamme", FLAME_ORANGE)
    glow = mat("verre_chaud", GLOW_YELLOW)
    for i in range(7):
        a = i * math.tau / 7
        cube("Root", stone, (0.55 * math.cos(a), 0.55 * math.sin(a), 0.12),
             (0.28, 0.22, 0.24))
    for rot in (0.3, 1.6, 2.7):
        buche = cylinder("Root", trunk, (0, 0, 0.16), (0.09, 0.09, 0.8),
                         rotation=(math.pi / 2, 0, rot), vertices=7)
        buche.rotation_euler = (math.pi / 2 - 0.25, 0, rot)
    cone("Flame1", flame, (0, 0, 0.55), (0.30, 0.30, 0.8), vertices=8)
    cone("Flame2", glow, (0.14, 0.08, 0.42), (0.18, 0.18, 0.5), vertices=7)
    cone("Flame3", flame, (-0.13, -0.07, 0.45), (0.15, 0.15, 0.45), vertices=7)

    arm = build_rig("Campfire", {
        "Flame1": ("Root", (0, 0, 0.2), (0, 0, 0.9)),
        "Flame2": ("Root", (0.14, 0.08, 0.2), (0.14, 0.08, 0.7)),
        "Flame3": ("Root", (-0.13, -0.07, 0.2), (-0.13, -0.07, 0.65)),
    })

    def keys(arm):
        # Pulsation déphasée des 3 flammes (scale + petit balancement), boucle
        # parfaite frame 1 = frame 49.
        pulses = {
            "Flame1": ((1, 1.0), (13, 1.25), (25, 0.9), (37, 1.15), (49, 1.0)),
            "Flame2": ((1, 1.1), (13, 0.85), (25, 1.2), (37, 0.95), (49, 1.1)),
            "Flame3": ((1, 0.9), (13, 1.15), (25, 1.0), (37, 1.25), (49, 0.9)),
        }
        for bone, ks in pulses.items():
            for f, s in ks:
                key_scale(arm, bone, f, (max(0.75, 2.0 - s), max(0.75, 2.0 - s), s))
                key_rot(arm, bone, f, ((s - 1.0) * 12.0, (1.0 - s) * 9.0, 0))

    bake_idle(arm, 49, keys)
    export("nature_campfire.glb")


def gen_scarecrow():
    """Épouvantail des rizières ~2 m (non solide : poteau trop fin pour les
    sondes) : piquet (Root) + corps/bras/tête (Spine) qui se balance doucement."""
    dark = mat("bois_sombre", WOOD_DARK)
    straw = mat("chaume", THATCH)
    tunic = mat("tunique", (0.35, 0.30, 0.45))
    cube("Root", dark, (0, 0, 0.55), (0.10, 0.10, 1.1))
    cube("Spine", tunic, (0, 0, 1.35), (0.5, 0.3, 0.7))
    cube("Spine", tunic, (0, 0, 1.55), (1.5, 0.16, 0.16))  # bras en croix
    cone("Spine", straw, (0, 0, 1.98), (0.28, 0.28, 0.55), vertices=8)  # tête/chapeau
    cube("Spine", straw, (0.78, 0, 1.55), (0.24, 0.1, 0.1))
    cube("Spine", straw, (-0.78, 0, 1.55), (0.24, 0.1, 0.1))

    arm = build_rig("Scarecrow", {
        "Spine": ("Root", (0, 0, 1.05), (0, 0, 1.9)),
    })

    def keys(arm):
        # Balancement lent au vent, boucle parfaite frame 1 = frame 97.
        for f, (x, y) in ((1, (0.0, 0.0)), (25, (3.5, 2.0)), (49, (0.0, -1.5)),
                          (73, (-3.5, 1.0)), (97, (0.0, 0.0))):
            key_rot(arm, "Spine", f, (x, y, 0))

    bake_idle(arm, 97, keys)
    export("nature_scarecrow.glb")


ASSETS = [gen_watermill, gen_windmill, gen_banner, gen_campfire, gen_scarecrow]

for gen in ASSETS:
    reset_scene()
    # Interpolation LINEAR par défaut pour toutes les clés insérées : rotation
    # continue des roues/pales à vitesse constante (le easing Bézier par défaut
    # ferait « respirer » la rotation à chaque clé). Re-posé après chaque
    # reset_scene (read_factory_settings réinitialise les préférences).
    bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
    PARTS.clear()
    gen()

print(f"[nature-anim] pack animé complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
