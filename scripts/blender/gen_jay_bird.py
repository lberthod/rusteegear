# Génère assets/models/fauna_jay.glb : geai des chênes (Garrulus glandarius)
# en vol, silhouette et livrée réalistes (vinaceux gris-rosé, calotte striée
# claire, moustache noire, miroir alaire bleu barré de noir et blanc, manteau
# roux, croupion blanc, queue noire). Même recette que gen_menagerie_pack.py
# (armature minuscule, un mesh joint par glb, vertex group plein par partie,
# clip animé en piste NLA) :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_jay_bird.py
#
# Sortie : assets/models/fauna_jay.glb (+ fauna_jay_preview_*.png, planche de
# contrôle à 8 phases du cycle de vol).
#
# Note moteur (cf. [[blender-headless-asset-pipeline]]) : `scene::import::
# load_gltf` ne lit que `base_color_factor` par matériau — aucune texture UV
# image n'est exploitée par le renderer, donc la livrée est portée entièrement
# par des matériaux unis par partie (comme tous les autres assets du pack),
# pas par une texture peinte.

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.fps = 24

# Palette geai des chênes (calibrée sur photos de référence — corps plus
# gris-rosé que rose franc, manteau roux plus sombre, bleu du miroir alaire
# plus saturé/électrique, croupion bien blanc pour le contraste en vol).
BODY_VINACEOUS = (0.64, 0.55, 0.52)        # dos/poitrine gris-rosé
BODY_VINACEOUS_LIGHT = (0.72, 0.62, 0.57)  # ventre, plus clair
MANTLE_RUFOUS = (0.40, 0.28, 0.20)         # manteau/dos roux sombre
CROWN_STREAK = (0.80, 0.75, 0.68)          # calotte striée gris-crème
THROAT_WHITE = (0.88, 0.84, 0.78)
MOUSTACHE_BLACK = (0.05, 0.05, 0.05)
WING_BLUE = (0.16, 0.48, 0.83)              # miroir alaire bleu électrique
WING_BAR_BLACK = (0.05, 0.05, 0.05)         # barrures noires sur le bleu
WING_BAR_WHITE = (0.90, 0.88, 0.85)         # fine barre blanche entre les noires
WING_WHITE = (0.92, 0.90, 0.85)
PRIMARY_BLACK = (0.10, 0.08, 0.07)
SECONDARY_BROWN = (0.38, 0.26, 0.18)
RUMP_WHITE = (0.95, 0.94, 0.90)
TAIL_BLACK = (0.06, 0.05, 0.05)
BEAK_DARK = (0.12, 0.11, 0.10)
LEG = (0.55, 0.44, 0.38)
EYE_BLACK = (0.04, 0.04, 0.04)


def mat(name, rgb):
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.75
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
    bpy.ops.object.shade_smooth()  # normales lissées : silhouette ronde, pas facettée

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
    # Subdivision Catmull-Clark après l'Armature : arrondit chaque primitive
    # (chaque partie du corps est une île topologique séparée après le join,
    # donc chaque bloc devient une forme galbée/organique indépendamment) au
    # lieu de garder les arêtes vives des cubes/cônes d'origine — c'est ce qui
    # sort du rendu « blocs low-poly » vers une silhouette lisse.
    smooth_mod = mesh.modifiers.new("Smooth", "SUBSURF")
    smooth_mod.levels = 2
    smooth_mod.render_levels = 2
    return arm


def key_rot(arm, bone, frame, deg_xyz):
    pb = arm.pose.bones[bone]
    pb.rotation_euler = tuple(math.radians(v) for v in deg_xyz)
    pb.keyframe_insert("rotation_euler", frame=frame)


def smooth_all_fcurves(arm):
    """Passe toutes les keyframes de l'action courante en interpolation
    Bezier + easing IN/OUT (au lieu du LINEAR par défaut) pour un battement
    d'aile qui accélère/décélère naturellement plutôt que de tourner à
    vitesse constante entre chaque pose.

    Blender 5.x a remplacé l'ancien `Action.fcurves` par le système en
    couches : `action.layers[*].strips[*].channelbags[*].fcurves`."""
    act = arm.animation_data.action
    for layer in act.layers:
        for strip in layer.strips:
            for channelbag in strip.channelbags:
                for fc in channelbag.fcurves:
                    for kp in fc.keyframe_points:
                        kp.interpolation = "BEZIER"
                        kp.easing = "EASE_IN_OUT"
                    fc.update()


def bake_fly(arm, length, keyer):
    """Crée le clip « Fly » (keyframes via `keyer`) en piste NLA, boucle
    parfaite (première pose = dernière pose), puis purge la pose résiduelle
    (piège connu : la pose de liaison embarquerait sinon la dernière pose
    keyframée — cf. [[blender-headless-asset-pipeline]])."""
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
    act.name = "Fly"
    smooth_all_fcurves(arm)
    track = ad.nla_tracks.new()
    track.name = "Fly"
    track.strips.new("Fly", 1, act)
    ad.action = None
    bpy.context.scene.frame_end = length

    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)
    bpy.ops.object.mode_set(mode="OBJECT")


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
        export_apply=True,  # bake le modificateur Subdivision Surface dans le mesh exporté
    )
    print(f"[jay] exporté {filename}")


# ---------------------------------------------------------------------------
# Geai des chênes — corps (Root) + Head + WingL/WingR + Tail.
# Pose de vol : ailes déployées, pattes repliées sous le ventre.
# ---------------------------------------------------------------------------

body_m = mat("geai_corps", BODY_VINACEOUS)
belly_m = mat("geai_ventre", BODY_VINACEOUS_LIGHT)
mantle_m = mat("geai_manteau", MANTLE_RUFOUS)
crown_m = mat("geai_calotte", CROWN_STREAK)
throat_m = mat("geai_gorge", THROAT_WHITE)
moustache_m = mat("geai_moustache", MOUSTACHE_BLACK)
beak_m = mat("geai_bec", BEAK_DARK)
eye_m = mat("geai_oeil", EYE_BLACK)
leg_m = mat("geai_patte", LEG)
rump_m = mat("geai_croupion", RUMP_WHITE)
tail_m = mat("geai_queue", TAIL_BLACK)
wing_blue_m = mat("geai_miroir_bleu", WING_BLUE)
wing_bar_m = mat("geai_barrure", WING_BAR_BLACK)
wing_bar_white_m = mat("geai_barrure_blanche", WING_BAR_WHITE)
wing_white_m = mat("geai_tache_blanche", WING_WHITE)
wing_secondary_m = mat("geai_secondaires", SECONDARY_BROWN)
primary_m = mat("geai_primaires", PRIMARY_BLACK)

# --- Corps (Root) ---
sphere("Root", body_m, (0, 0, 0.20), (0.115, 0.165, 0.115))
sphere("Root", belly_m, (0, -0.03, 0.145), (0.095, 0.11, 0.09))
sphere("Root", mantle_m, (0, 0.05, 0.26), (0.095, 0.11, 0.07))
sphere("Root", rump_m, (0, -0.21, 0.175), (0.06, 0.075, 0.06))
cube("Root", leg_m, (0.035, -0.02, 0.095), (0.018, 0.03, 0.05))
cube("Root", leg_m, (-0.035, -0.02, 0.095), (0.018, 0.03, 0.05))

# --- Tête (Head bone) ---
sphere("Head", body_m, (0, 0.24, 0.285), (0.085, 0.09, 0.085))
sphere("Head", crown_m, (0, 0.225, 0.325), (0.068, 0.075, 0.035))
sphere("Head", throat_m, (0, 0.305, 0.245), (0.045, 0.05, 0.04))
cube("Head", moustache_m, (0.052, 0.295, 0.255), (0.012, 0.05, 0.02))
cube("Head", moustache_m, (-0.052, 0.295, 0.255), (0.012, 0.05, 0.02))
sphere("Head", eye_m, (0.062, 0.275, 0.30), (0.016, 0.016, 0.016))
sphere("Head", eye_m, (-0.062, 0.275, 0.30), (0.016, 0.016, 0.016))
cone("Head", beak_m, (0, 0.35, 0.275), (0.028, 0.028, 0.11), rotation=(math.pi / 2, 0, 0))

# --- Aile droite (WingR bone), déployée vers +X : miroir bleu barré
# noir/blanc/noir (plus proche de la vraie livrée qu'un simple bleu uni). ---
cube("WingR", wing_blue_m, (0.185, 0.02, 0.205), (0.095, 0.085, 0.02))
cube("WingR", wing_bar_m, (0.15, 0.02, 0.205), (0.010, 0.085, 0.022))
cube("WingR", wing_bar_white_m, (0.175, 0.02, 0.206), (0.008, 0.085, 0.023))
cube("WingR", wing_bar_m, (0.20, 0.02, 0.205), (0.010, 0.085, 0.022))
cube("WingR", wing_secondary_m, (0.30, 0.00, 0.195), (0.09, 0.095, 0.018))
cube("WingR", wing_white_m, (0.375, -0.03, 0.19), (0.04, 0.085, 0.016))
cube("WingR", primary_m, (0.45, -0.06, 0.185), (0.065, 0.09, 0.014))
cube("WingR", primary_m, (0.51, -0.11, 0.178), (0.05, 0.075, 0.012), rotation=(0, 0, -0.12))

# --- Aile gauche (WingL bone), miroir vers -X ---
cube("WingL", wing_blue_m, (-0.185, 0.02, 0.205), (0.095, 0.085, 0.02))
cube("WingL", wing_bar_m, (-0.15, 0.02, 0.205), (0.010, 0.085, 0.022))
cube("WingL", wing_bar_white_m, (-0.175, 0.02, 0.206), (0.008, 0.085, 0.023))
cube("WingL", wing_bar_m, (-0.20, 0.02, 0.205), (0.010, 0.085, 0.022))
cube("WingL", wing_secondary_m, (-0.30, 0.00, 0.195), (0.09, 0.095, 0.018))
cube("WingL", wing_white_m, (-0.375, -0.03, 0.19), (0.04, 0.085, 0.016))
cube("WingL", primary_m, (-0.45, -0.06, 0.185), (0.065, 0.09, 0.014))
cube("WingL", primary_m, (-0.51, -0.11, 0.178), (0.05, 0.075, 0.012), rotation=(0, 0, 0.12))

# --- Queue (Tail bone), noire, en éventail plat trainant derrière le corps.
# Convention cone (vérifiée empiriquement) pour rotation=(pi/2, ry, 0) :
# scale.x -> largeur (X monde), scale.y -> épaisseur (Z monde), scale.z ->
# longueur (-Y monde, direction de pointe du cône). Un ry léger évase les
# rectrices latérales dans le plan horizontal sans les faire basculer à la
# verticale (piège du pi/4 initial : il faisait passer la longueur en Z).
cone("Tail", tail_m, (0, -0.27, 0.16), (0.05, 0.012, 0.16), rotation=(math.pi / 2, 0, 0))
cone("Tail", tail_m, (0.028, -0.26, 0.158), (0.032, 0.010, 0.13),
     rotation=(math.pi / 2, 0.20, 0))
cone("Tail", tail_m, (-0.028, -0.26, 0.158), (0.032, 0.010, 0.13),
     rotation=(math.pi / 2, -0.20, 0))

arm = build_rig("Jay", {
    "Head": ("Root", (0, 0.20, 0.24), (0, 0.36, 0.27)),
    "WingR": ("Root", (0.11, 0.03, 0.21), (0.34, -0.05, 0.19)),
    "WingL": ("Root", (-0.11, 0.03, 0.21), (-0.34, -0.05, 0.19)),
    "Tail": ("Root", (0, -0.20, 0.175), (0, -0.42, 0.15)),
})


def keys(arm):
    # Cycle de battement (24 fr @ 24 fps = 1 s/cycle) construit sur une onde
    # sinusoïdale à 2 harmoniques (fondamentale + 2e harmonique en
    # contre-phase) : monte vite, redescend plus lentement, comme un vrai
    # battement propulsif, au lieu de segments linéaires équirépartis.
    # Interpolation Bezier + easing (bake_fly -> smooth_all_fcurves) lisse
    # encore le mouvement entre les clés.
    n = 8
    length = 24
    for i in range(n + 1):  # dernière clé = première (boucle parfaite)
        f = 1 + round(i * (length - 1) / n)
        t = (i % n) / n * 2 * math.pi
        wing = 42.0 * math.sin(t) - 12.0 * math.sin(2 * t)
        key_rot(arm, "WingR", f, (wing * 0.18, 0, wing))
        key_rot(arm, "WingL", f, (wing * 0.18, 0, -wing))
        key_rot(arm, "Tail", f, (wing * 0.22, 0, 0))
        key_rot(arm, "Root", f, (-wing * 0.09, 0, 0))
        key_rot(arm, "Head", f, (wing * 0.05, 0, 0))


bake_fly(arm, 24, keys)
export("fauna_jay.glb")

# --- Planche de contrôle : 8 phases du cycle de vol ---
# Vue 3/4 arrière-dessus reculée pour éviter le raccourci de perspective sur
# la queue/tête (pointées vers -Y) : caméra loin sur +Y, légèrement en X/Z.
cam_loc = Vector((1.3, -1.3, 0.95))
bpy.ops.object.camera_add(location=cam_loc)
cam = bpy.context.active_object
target = Vector((0, -0.05, 0.19))
direction = target - cam_loc
cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
cam.data.lens = 55
scene.camera = cam
bpy.ops.object.light_add(type="SUN", location=(1.5, -1.0, 2.0), rotation=(math.radians(35), 0, math.radians(35)))
bpy.context.active_object.data.energy = 1.4
bpy.ops.object.light_add(type="SUN", location=(-1.0, 1.5, 1.0), rotation=(math.radians(-35), 0, math.radians(-145)))
bpy.context.active_object.data.energy = 0.5

# Fond neutre (le lissage/subsurf du mesh est déjà fait dans build_rig) au
# lieu du vide noir, pour un rendu de contrôle qui se lit mieux qu'un aplat de
# blocs sur fond nul.
world = bpy.data.worlds.new("JayPreviewWorld")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs[0].default_value = (0.10, 0.10, 0.12, 1.0)
world.node_tree.nodes["Background"].inputs[1].default_value = 0.6
scene.world = world

scene.render.engine = "BLENDER_EEVEE"
scene.view_settings.view_transform = "Standard"  # AgX désature trop pour un contrôle couleur fidèle
scene.render.resolution_x = 1080
scene.render.resolution_y = 1080
scene.render.filter_size = 1.2  # anti-aliasing du filtre de reconstruction

# Échantillonnage : les noms de propriétés diffèrent entre EEVEE et
# EEVEE Next (5.x) — on tente les deux plutôt que de figer une version.
eevee = scene.eevee
for attr, value in (
    ("taa_render_samples", 128),
    ("use_taa_reprojection", True),
    ("use_gtao", True),
    ("gtao_distance", 0.25),
    ("use_soft_shadows", True),
    ("shadow_ray_count", 4),
    ("shadow_step_count", 8),
):
    if hasattr(eevee, attr):
        setattr(eevee, attr, value)

for i, f in enumerate((1, 4, 7, 10, 13, 16, 19, 22)):
    scene.frame_set(f)
    scene.render.filepath = OUT_DIR + f"fauna_jay_preview_{i}.png"
    bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)
