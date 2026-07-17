# Génère le pack « objets ramassables » 01-10 en Blender headless — version
# riggée + animée :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_items_pack01_10.py
#
# Sortie : assets/models/item_*.glb + item_*_preview.png (vignette de contrôle).
# 10 pickups : baie, pomme, champignon, épée, potion, pièce, balle, clé,
# gigot, gemme. Conventions (mêmes que les packs créatures 22-41) :
# - rig Root/… par objet, mesh unique skinné (1 os / partie, poids 1.0) ;
# - clip « Idle » 40 fr à 24 fps, bouclable — os verticaux (Y local = Z monde) :
#   key_loc (0,dz,0) = flottement, key_rot (0,a,0) = toupie, key_scale
#   (sx, s_vertical, sz) = squash & stretch (le moteur lit les 3 canaux) ;
# - toupies continues 0→2π en interpolation LINEAR (sinon à-coup au bouclage) ;
# - seule couleur lue par le moteur : base_color_factor (pas de textures) ;
# - shade_smooth sur les parties organiques/rondes (normales exportées),
#   facettes gardées sur cristaux/lames (style voulu) ;
# - sol du jeu = z=0 Blender (glTF Y-up) → AUCUN vertex sous z=0 (assert) ;
# - échelle appliquée AVANT la rotation (piège rotation/scale connu) ;
# - pose remise au neutre avant export ET avant la vignette (piège pose
#   résiduelle de l'exporteur).

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

TAU = 2.0 * math.pi
PARTS = []


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.render.fps = 24
    PARTS.clear()


def mat(name, rgb, roughness=0.8):
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = roughness
    return m


def _finish_part(bone, o, material, smooth):
    o.data.materials.clear()
    o.data.materials.append(material)
    if smooth:
        bpy.ops.object.shade_smooth()
    vg = o.vertex_groups.new(name=bone)
    vg.add(range(len(o.data.vertices)), 1.0, "REPLACE")
    PARTS.append(o)
    return o


def sphere(bone, material, location, scale=(1, 1, 1), radius=1.0,
           segments=20, rings=12, smooth=True):
    bpy.ops.mesh.primitive_uv_sphere_add(
        segments=segments, ring_count=rings, radius=radius, location=location
    )
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return _finish_part(bone, o, material, smooth)


def cube(bone, material, location, scale, rotation=(0, 0, 0), smooth=False):
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    o.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    return _finish_part(bone, o, material, smooth)


def cylinder(bone, material, location, radius, depth, rotation=(0, 0, 0),
             vertices=16, smooth=True):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=location
    )
    o = bpy.context.active_object
    o.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    return _finish_part(bone, o, material, smooth)


def cone(bone, material, location, radius, depth, rotation=(0, 0, 0),
         vertices=16, radius2=0.0, smooth=False):
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=radius, radius2=radius2, depth=depth,
        location=location
    )
    o = bpy.context.active_object
    o.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    return _finish_part(bone, o, material, smooth)


def torus(bone, material, location, major, minor, rotation=(0, 0, 0), smooth=True):
    bpy.ops.mesh.primitive_torus_add(
        major_radius=major, minor_radius=minor, location=location,
        major_segments=20, minor_segments=10,
    )
    o = bpy.context.active_object
    o.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    return _finish_part(bone, o, material, smooth)


def build_item(name, bones, idle_keys, cam_dist, cam_height=None, target_z=None,
               linear_bones=()):
    """Fusionne PARTS, pose le rig, bake le clip Idle, exporte + vignette.

    `bones` : {nom: (parent, head, tail)} — Root est créé d'office. Les parties
    statiques (socles…) se skinnent sur "Root" (jamais keyé = figé, un seul clip).
    `linear_bones` : os dont toutes les clés passent en LINEAR (toupies 0→2π).
    """
    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = PARTS[0]
    bpy.ops.object.join()
    item = bpy.context.active_object
    item.name = name
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

    # Garde-fou : aucun vertex sous z=0 (gel par TriMesh incrusté côté moteur).
    min_z = min(v.co.z for v in item.data.vertices)
    assert min_z >= -1e-4, f"{name}: vertex sous z=0 ({min_z:.4f})"

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = f"{name}Rig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.2))
    for bname, (parent, head, tail) in bones.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    item.parent = arm
    item.modifiers.new("Armature", "ARMATURE").object = arm

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

    def key_scale(bone, frame, xyz):
        pb = arm.pose.bones[bone]
        pb.scale = xyz
        pb.keyframe_insert("scale", frame=frame)

    ad = arm.animation_data_create()
    ad.action = None
    idle_keys(key_rot, key_loc, key_scale)
    act = ad.action
    act.name = "Idle"
    # Blender 5.x : les fcurves vivent dans les channelbags des actions slottées.
    for layer in act.layers:
        for strip in layer.strips:
            for bag in strip.channelbags:
                for fc in bag.fcurves:
                    if any(f'"{b}"' in fc.data_path for b in linear_bones):
                        for kp in fc.keyframe_points:
                            kp.interpolation = "LINEAR"
    track = ad.nla_tracks.new()
    track.name = "Idle"
    track.strips.new("Idle", 1, act)
    ad.action = None

    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)
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

    # Vignette : pistes NLA purgées + pose neutre (piège pose résiduelle).
    ad = arm.animation_data
    ad.action = None
    for t in list(ad.nla_tracks):
        ad.nla_tracks.remove(t)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)
    scene = bpy.context.scene
    scene.frame_set(1)
    bpy.context.view_layer.update()
    top_z = max(v.co.z for v in item.data.vertices)
    tz = target_z if target_z is not None else top_z * 0.5
    loc = Vector((cam_dist * 0.75, -cam_dist, cam_height if cam_height else cam_dist * 0.7))
    bpy.ops.object.camera_add(location=loc)
    cam = bpy.context.active_object
    cam.rotation_euler = (Vector((0, 0, tz)) - loc).to_track_quat("-Z", "Y").to_euler()
    scene.camera = cam
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


def vbone(z0, z1):
    """Os vertical : Y local = Z monde (flottement/toupie dans les axes simples)."""
    return ((0, 0, z0), (0, 0, z1))


def spin_keys(key_rot, bone):
    """Toupie continue 0→2π sur 40 fr (à passer en LINEAR via linear_bones)."""
    for f, a in ((1, 0.0), (14, TAU / 3), (27, 2 * TAU / 3), (40, TAU)):
        key_rot(bone, f, (0, a, 0))


def bob_keys(key_loc, bone, amp, base=0.0):
    """Flottement sinusoïdal doux (vers le haut uniquement : jamais sous z=0)."""
    for f, dz in ((1, base), (20, base + amp), (40, base)):
        key_loc(bone, f, (0, dz, 0))


# ---------------------------------------------------------------------------
# Palette (cohérente avec la DA nature/hameau) — accent doré pour les trésors.
# ---------------------------------------------------------------------------
BERRY_RED = (0.78, 0.10, 0.12)
LEAF = (0.20, 0.44, 0.17)
LEAF_DARK = (0.14, 0.34, 0.12)
APPLE_RED = (0.72, 0.12, 0.10)
STEM_BROWN = (0.32, 0.22, 0.11)
CAP_RED = (0.75, 0.16, 0.12)
CREAM = (0.90, 0.86, 0.76)
STEEL = (0.72, 0.75, 0.80)
GOLD = (0.85, 0.65, 0.15)
GOLD_DARK = (0.62, 0.45, 0.10)
GLASS = (0.55, 0.70, 0.80)
POTION_MAGENTA = (0.72, 0.15, 0.55)
CORK = (0.50, 0.36, 0.20)
BALL_RED = (0.80, 0.20, 0.15)
BALL_WHITE = (0.92, 0.90, 0.85)
MEAT = (0.60, 0.26, 0.18)
BONE = (0.92, 0.89, 0.80)
GEM_CYAN = (0.20, 0.75, 0.80)
STONE = (0.40, 0.39, 0.38)
GRIP = (0.28, 0.18, 0.09)


# --- 01 Baie : touffe de feuillage, ondulation de brise (~0,25 m) ------------
def item_berry():
    reset_scene()
    leaf = mat("baie_feuille", LEAF)
    leaf_d = mat("baie_feuille_d", LEAF_DARK)
    berry = mat("baie_fruit", BERRY_RED, roughness=0.35)
    sphere("Item", leaf_d, (0, 0, 0.095), (1, 1, 0.65), radius=0.14)
    sphere("Item", leaf, (0.09, 0.05, 0.10), (1, 1, 0.7), radius=0.09)
    sphere("Item", leaf, (-0.08, -0.05, 0.10), (1, 1, 0.7), radius=0.08)
    for x, y, z in ((0.0, -0.02, 0.20), (0.07, 0.04, 0.17), (-0.06, 0.05, 0.18),
                    (-0.03, -0.08, 0.15), (0.05, -0.06, 0.14), (-0.09, -0.02, 0.13)):
        sphere("Item", berry, (x, y, z), radius=0.045, segments=12, rings=8)

    def idle(key_rot, key_loc, key_scale):
        # Brise : le buisson ondule et frissonne légèrement.
        for f, sw in ((1, 0.05), (13, -0.04), (27, 0.05), (40, 0.05)):
            key_rot("Item", f, (sw, 0, sw * 0.6))
        bob_keys(key_loc, "Item", 0.015)

    build_item("item_berry", {"Item": ("Root", *vbone(0.01, 0.25))}, idle,
               cam_dist=0.7)


# --- 02 Pomme : toupie de trésor + flottement (~0,25 m) ----------------------
def item_apple():
    reset_scene()
    apple = mat("pomme_fruit", APPLE_RED, roughness=0.35)
    stem = mat("pomme_queue", STEM_BROWN)
    leaf = mat("pomme_feuille", LEAF)
    sphere("Item", apple, (0, 0, 0.115), (1, 1, 0.92), radius=0.12)
    cylinder("Item", stem, (0, 0, 0.235), radius=0.012, depth=0.06,
             rotation=(math.radians(8), 0, 0), vertices=8)
    sphere("Item", leaf, (0.05, 0.02, 0.25), (1.6, 0.8, 0.25), radius=0.045)

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.04)

    build_item("item_apple", {"Item": ("Root", *vbone(0.01, 0.26))}, idle,
               cam_dist=0.65, linear_bones=("Item",))


# --- 03 Champignon : chapeau qui dodeline sur son pied (~0,35 m) -------------
def item_mushroom():
    reset_scene()
    stem = mat("champi_pied", CREAM)
    cap = mat("champi_chapeau", CAP_RED, roughness=0.5)
    dot = mat("champi_pois", (0.95, 0.93, 0.88))
    cylinder("Item", stem, (0, 0, 0.11), radius=0.055, depth=0.22)
    sphere("Cap", cap, (0, 0, 0.27), (1, 1, 0.62), radius=0.16)
    # Pois posés SUR la calotte : z suit la surface de la demi-sphère écrasée.
    for dx, dy in ((0.08, 0.05), (-0.07, 0.07), (0.0, -0.10), (-0.09, -0.04), (0.10, -0.03)):
        z = 0.27 + 0.099 * math.sqrt(max(0.0, 1 - (dx * dx + dy * dy) / 0.0256))
        sphere("Cap", dot, (dx, dy, z), (1, 1, 0.35), radius=0.032,
               segments=10, rings=6)

    def idle(key_rot, key_loc, key_scale):
        # Le chapeau dessine un petit cercle (dodeline), le pied respire.
        for f, tx, tz in ((1, 0.10, 0.0), (11, 0.0, 0.10), (21, -0.10, 0.0),
                          (31, 0.0, -0.10), (40, 0.10, 0.0)):
            key_rot("Cap", f, (tx, 0, tz))
        for f, s in ((1, 1.0), (20, 1.03), (40, 1.0)):
            key_scale("Item", f, (s, 1.0 / s, s))

    build_item("item_mushroom",
               {"Item": ("Root", *vbone(0.01, 0.22)),
                "Cap": ("Item", *vbone(0.22, 0.42))},
               idle, cam_dist=0.75)


# --- 04 Épée : lame qui lévite hors du socle et retombe (~0,9 m) -------------
def item_sword():
    reset_scene()
    blade = mat("epee_lame", STEEL, roughness=0.25)
    guard = mat("epee_garde", GOLD, roughness=0.4)
    grip = mat("epee_poignee", GRIP)
    stone = mat("epee_socle", STONE)
    sphere("Root", stone, (0, 0, 0.08), (1, 1, 0.55), radius=0.14)   # socle figé
    cube("Sword", blade, (0, 0, 0.45), (0.075, 0.022, 0.56))         # lame
    cone("Sword", blade, (0, 0, 0.775), radius=0.0375, depth=0.09,   # pointe
         vertices=4, rotation=(0, 0, math.radians(45)))
    cube("Sword", guard, (0, 0, 0.17), (0.20, 0.045, 0.035))         # garde
    cylinder("Sword", grip, (0, 0, 0.10), radius=0.025, depth=0.13,  # poignée
             smooth=False)
    sphere("Sword", guard, (0, 0, 0.05), radius=0.04)                # pommeau

    def idle(key_rot, key_loc, key_scale):
        # Épée légendaire : elle s'élève lentement du socle, pivote d'un
        # quart de tour, puis se repose — cycle solennel.
        for f, dz in ((1, 0.0), (14, 0.10), (26, 0.10), (40, 0.0)):
            key_loc("Sword", f, (0, dz, 0))
        for f, a in ((1, 0.0), (14, math.pi / 2), (26, math.pi / 2), (40, 0.0)):
            key_rot("Sword", f, (0, a, 0))

    build_item("item_sword", {"Sword": ("Root", *vbone(0.03, 0.8))}, idle,
               cam_dist=1.3, cam_height=0.7, target_z=0.4)


# --- 05 Potion : toupie + flottement (~0,35 m) -------------------------------
def item_potion():
    reset_scene()
    liquid = mat("potion_liquide", POTION_MAGENTA, roughness=0.2)
    glass = mat("potion_verre", GLASS, roughness=0.15)
    cork = mat("potion_bouchon", CORK)
    # Verre opaque (pas de transparence lue par le moteur) : la panse porte
    # directement la couleur du liquide, seul le col reste « verre ».
    sphere("Item", liquid, (0, 0, 0.12), radius=0.115)                # panse
    sphere("Item", glass, (-0.045, -0.07, 0.17), (1, 1, 1.4),
           radius=0.028, segments=12, rings=8)                        # reflet
    cylinder("Item", glass, (0, 0, 0.25), radius=0.038, depth=0.09)   # col
    cylinder("Item", cork, (0, 0, 0.305), radius=0.042, depth=0.05)   # bouchon

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.04)

    build_item("item_potion", {"Item": ("Root", *vbone(0.01, 0.33))}, idle,
               cam_dist=0.75, linear_bones=("Item",))


# --- 06 Pièce d'or : la toupie classique du trésor (~0,25 m) -----------------
def item_coin():
    reset_scene()
    gold = mat("piece_or", GOLD, roughness=0.3)
    gold_d = mat("piece_or_d", GOLD_DARK, roughness=0.35)
    tilt = (math.radians(90), math.radians(12), 0)
    cylinder("Item", gold, (0, 0, 0.13), radius=0.12, depth=0.03,
             rotation=tilt, vertices=24, smooth=False)
    cylinder("Item", gold_d, (0, 0, 0.13), radius=0.075, depth=0.034,
             rotation=tilt, vertices=24, smooth=False)

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.03)

    build_item("item_coin", {"Item": ("Root", *vbone(0.005, 0.26))}, idle,
               cam_dist=0.7, linear_bones=("Item",))


# --- 07 Balle : rebond avec squash & stretch (~0,3 m) ------------------------
def item_ball():
    reset_scene()
    body = mat("balle_corps", BALL_RED, roughness=0.4)
    band = mat("balle_bande", BALL_WHITE, roughness=0.4)
    sphere("Item", body, (0, 0, 0.15), radius=0.15)
    cylinder("Item", band, (0, 0, 0.15), radius=0.152, depth=0.055, vertices=24)
    sphere("Item", band, (0, 0, 0.15), (1, 1, 0.18), radius=0.152)

    def idle(key_rot, key_loc, key_scale):
        # Un rebond par cycle : écrasée au sol, étirée à l'envol, ronde à
        # l'apogée, ré-écrasée à l'atterrissage (os vertical : Y local = haut).
        for f, dz in ((1, 0.0), (8, 0.16), (15, 0.24), (23, 0.16), (30, 0.0),
                      (40, 0.0)):
            key_loc("Item", f, (0, dz, 0))
        for f, sxz, sy in ((1, 1.07, 0.86), (6, 0.96, 1.08), (15, 1.0, 1.0),
                           (26, 0.97, 1.06), (31, 1.09, 0.82), (36, 1.0, 1.0),
                           (40, 1.07, 0.86)):
            key_scale("Item", f, (sxz, sy, sxz))

    build_item("item_ball", {"Item": ("Root", *vbone(0.0, 0.3))}, idle,
               cam_dist=0.8)


# --- 08 Clé : toupie + flottement (~0,45 m) ----------------------------------
def item_key():
    reset_scene()
    gold = mat("cle_or", GOLD, roughness=0.3)
    gold_d = mat("cle_or_d", GOLD_DARK, roughness=0.35)
    torus("Item", gold, (0, 0, 0.36), major=0.075, minor=0.022,
          rotation=(math.radians(90), 0, 0))                          # anneau
    cylinder("Item", gold, (0, 0, 0.17), radius=0.025, depth=0.26)    # tige
    cube("Item", gold_d, (0.045, 0, 0.06), (0.07, 0.04, 0.04))        # dent 1
    cube("Item", gold_d, (0.04, 0, 0.115), (0.06, 0.04, 0.035))       # dent 2
    sphere("Item", gold, (0, 0, 0.035), radius=0.032)                 # bout

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.05)

    build_item("item_key", {"Item": ("Root", *vbone(0.005, 0.42))}, idle,
               cam_dist=0.95, target_z=0.22, linear_bones=("Item",))


# --- 09 Gigot : petit balancement appétissant (~0,4 m) -----------------------
def item_meat():
    reset_scene()
    meat = mat("gigot_viande", MEAT, roughness=0.6)
    meat_d = mat("gigot_croute", (0.45, 0.17, 0.12), roughness=0.55)
    bone = mat("gigot_os", BONE)
    rot = (0, math.radians(65), math.radians(20))
    sphere("Item", meat, (0, 0, 0.12), (1.5, 1.0, 1.0), radius=0.11)
    sphere("Item", meat_d, (-0.07, 0.0, 0.16), (1.2, 0.85, 0.8), radius=0.09)
    cylinder("Item", bone, (0.17, 0.06, 0.20), radius=0.022, depth=0.22,
             rotation=rot)
    sphere("Item", bone, (0.26, 0.09, 0.28), radius=0.035)
    sphere("Item", bone, (0.30, 0.11, 0.26), radius=0.03)

    def idle(key_rot, key_loc, key_scale):
        for f, sw in ((1, 0.06), (20, -0.06), (40, 0.06)):
            key_rot("Item", f, (sw, 0, -sw))
        bob_keys(key_loc, "Item", 0.025)

    build_item("item_meat", {"Item": ("Root", *vbone(0.01, 0.3))}, idle,
               cam_dist=0.85)


# --- 10 Gemme : toupie scintillante au-dessus du socle (~0,45 m) -------------
def item_gem():
    reset_scene()
    gem = mat("gemme_cristal", GEM_CYAN, roughness=0.1)
    gem_d = mat("gemme_coeur", (0.10, 0.50, 0.58), roughness=0.15)
    stone = mat("gemme_socle", STONE)
    sphere("Root", stone, (0, 0, 0.06), (1, 1, 0.45), radius=0.13)    # socle figé
    cone("Gem", gem, (0, 0, 0.145), radius=0.11, depth=0.14,          # pointe basse
         rotation=(math.radians(180), 0, 0), vertices=6)
    cone("Gem", gem, (0, 0, 0.315), radius=0.11, depth=0.20, vertices=6)
    sphere("Gem", gem_d, (0, 0, 0.215), (1, 1, 0.55), radius=0.075, smooth=False)

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Gem")
        bob_keys(key_loc, "Gem", 0.06, base=0.02)
        # Pulsation subtile : la gemme « respire » sa lumière.
        for f, s in ((1, 1.0), (10, 1.05), (20, 1.0), (30, 1.05), (40, 1.0)):
            key_scale("Gem", f, (s, s, s))

    build_item("item_gem", {"Gem": ("Root", *vbone(0.08, 0.42))}, idle,
               cam_dist=0.95, target_z=0.22, linear_bones=("Gem",))


item_berry()
item_apple()
item_mushroom()
item_sword()
item_potion()
item_coin()
item_ball()
item_key()
item_meat()
item_gem()
print("PACK ITEMS 01-10 ANIM DONE")
