# Génère le pack « objets ramassables » 21-30 en Blender headless — riggé +
# animé :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_items_pack21_30.py
#
# Sortie : assets/models/item_*.glb + item_*_preview.png (vignette de contrôle).
# 10 pickups : carotte, fromage, œuf doré, plume, grimoire, marteau, couronne,
# étoile, bombe, poisson. Mêmes conventions que gen_items_pack01_10.py /
# gen_items_pack11_20.py :
# - rig Root/… par objet, mesh unique skinné (1 os / partie, poids 1.0) ;
# - clip « Idle » 40 fr à 24 fps, bouclable — os verticaux (Y local = Z monde) ;
# - toupies continues 0→2π en interpolation LINEAR (sinon à-coup au bouclage) ;
# - seule couleur lue par le moteur : base_color_factor (pas de textures) ;
# - shade_smooth sur les parties organiques/rondes, facettes sur les cristaux ;
# - sol du jeu = z=0 Blender (glTF Y-up) → AUCUN vertex sous z=0 (assert) ;
# - échelle appliquée AVANT la rotation (piège rotation/scale connu) ;
# - pose remise au neutre avant export ET avant la vignette.

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
             vertices=16, smooth=True, scale=(1, 1, 1)):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=location
    )
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    o.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    return _finish_part(bone, o, material, smooth)


def cone(bone, material, location, radius, depth, rotation=(0, 0, 0),
         vertices=16, radius2=0.0, smooth=False, scale=(1, 1, 1)):
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=radius, radius2=radius2, depth=depth,
        location=location
    )
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
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
    """Fusionne PARTS, pose le rig, bake le clip Idle, exporte + vignette."""
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
# Palette (cohérente avec les packs items 01-20).
# ---------------------------------------------------------------------------
CARROT = (0.85, 0.42, 0.10)
LEAF = (0.20, 0.44, 0.17)
CHEESE = (0.92, 0.72, 0.25)
CHEESE_HOLE = (0.70, 0.50, 0.15)
GOLD = (0.85, 0.65, 0.15)
GOLD_DARK = (0.62, 0.45, 0.10)
NEST = (0.42, 0.29, 0.13)
FEATHER = (0.93, 0.94, 0.96)
FEATHER_TIP = (0.55, 0.70, 0.85)
BOOK_RED = (0.48, 0.13, 0.11)
PARCH = (0.88, 0.82, 0.64)
WOOD_DARK = (0.28, 0.18, 0.09)
STEEL = (0.72, 0.75, 0.80)
JEWEL_RED = (0.80, 0.14, 0.16)
GEM_CYAN = (0.20, 0.75, 0.80)
STAR_YELLOW = (1.0, 0.85, 0.30)
BOMB_DARK = (0.12, 0.12, 0.15)
FUSE = (0.50, 0.36, 0.20)
GLOW_YELLOW = (1.0, 0.78, 0.35)
FISH_BLUE = (0.42, 0.58, 0.72)
FISH_BELLY = (0.85, 0.88, 0.90)
DARK = (0.06, 0.06, 0.09)
STONE = (0.40, 0.39, 0.38)


# --- 21 Carotte : toupie pointe en bas + fanes (~0,35 m) ---------------------
def item_carrot():
    reset_scene()
    root_c = mat("carotte_racine", CARROT, roughness=0.55)
    leaf = mat("carotte_fanes", LEAF)
    cone("Item", root_c, (0, 0, 0.17), radius=0.05, depth=0.28,
         rotation=(math.radians(180), 0, 0), smooth=True)            # racine
    for dx, dy, tilt in ((0.0, 0.0, 0.0), (0.03, 0.02, 0.5), (-0.03, 0.01, -0.5)):
        sphere("Item", leaf, (dx, dy, 0.345), (0.22, 0.22, 1.0),
               radius=0.055, segments=10, rings=8,
               )                                                     # fanes
    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.04)

    build_item("item_carrot", {"Item": ("Root", *vbone(0.02, 0.36))}, idle,
               cam_dist=0.8, linear_bones=("Item",))


# --- 22 Fromage : quartier percé qui se dandine (~0,3 m) ---------------------
def item_cheese():
    reset_scene()
    body = mat("fromage_pate", CHEESE, roughness=0.5)
    hole = mat("fromage_trou", CHEESE_HOLE)
    cylinder("Item", body, (0, 0, 0.046), radius=0.15, depth=0.09,
             vertices=3, smooth=False)                               # quartier
    for x, y, z, r in ((0.05, 0.02, 0.092, 0.032), (-0.03, -0.05, 0.09, 0.026),
                       (-0.05, 0.05, 0.05, 0.022), (0.09, -0.04, 0.06, 0.02)):
        sphere("Item", hole, (x, y, z), (1, 1, 0.4), radius=r,
               segments=10, rings=8)                                 # trous

    def idle(key_rot, key_loc, key_scale):
        for f, sw in ((1, 0.06), (20, -0.06), (40, 0.06)):
            key_rot("Item", f, (sw, 0, -sw))
        bob_keys(key_loc, "Item", 0.03)

    build_item("item_cheese", {"Item": ("Root", *vbone(0.01, 0.12))}, idle,
               cam_dist=0.75)


# --- 23 Œuf doré : bascule sur son nid comme un culbuto (~0,3 m) -------------
def item_egg():
    reset_scene()
    gold = mat("oeuf_or", GOLD, roughness=0.25)
    nest = mat("oeuf_nid", NEST)
    torus("Root", nest, (0, 0, 0.045), major=0.115, minor=0.042)     # nid figé
    torus("Root", nest, (0, 0, 0.075), major=0.08, minor=0.03)
    sphere("Egg", gold, (0, 0, 0.16), (1, 1, 1.3), radius=0.085)     # œuf

    def idle(key_rot, key_loc, key_scale):
        # Culbuto : l'œuf oscille en rond sur le nid — il va éclore ?
        for f, tx, tz in ((1, 0.12, 0.0), (11, 0.0, 0.12), (21, -0.12, 0.0),
                          (31, 0.0, -0.12), (40, 0.12, 0.0)):
            key_rot("Egg", f, (tx, 0, tz))
        for f in (1, 40):
            key_loc("Egg", f, (0, 0, 0))

    build_item("item_egg", {"Egg": ("Root", *vbone(0.08, 0.3))}, idle,
               cam_dist=0.75)


# --- 24 Plume : se balance comme si elle tombait sans fin (~0,45 m) ----------
def item_feather():
    reset_scene()
    vane = mat("plume_barbes", FEATHER, roughness=0.6)
    tip = mat("plume_pointe", FEATHER_TIP, roughness=0.5)
    quill = mat("plume_tuyau", (0.85, 0.80, 0.70))
    cylinder("Item", quill, (0, 0, 0.21), radius=0.009, depth=0.42)  # tuyau
    # Barbes sur les 2/3 hauts seulement : le tuyau nu reste visible en bas.
    sphere("Item", vane, (0, 0, 0.29), (0.30, 0.09, 0.80), radius=0.15)  # barbes
    sphere("Item", vane, (0.025, 0, 0.24), (0.16, 0.07, 0.45), radius=0.15)
    sphere("Item", tip, (0, 0, 0.40), (0.14, 0.06, 0.22), radius=0.15)  # pointe

    def idle(key_rot, key_loc, key_scale):
        # Pendule de feuille morte : balancement ample + flottement lent.
        for f, sw in ((1, 0.30), (20, -0.30), (40, 0.30)):
            key_rot("Item", f, (sw * 0.4, 0, sw))
        for f, dz in ((1, 0.02), (10, 0.06), (20, 0.02), (30, 0.06), (40, 0.02)):
            key_loc("Item", f, (0, dz, 0))

    build_item("item_feather", {"Item": ("Root", *vbone(0.0, 0.42))}, idle,
               cam_dist=0.85, target_z=0.24)


# --- 25 Grimoire : flotte, sa couverture s'entrouvre (~0,35 m) ---------------
def item_book():
    reset_scene()
    cover = mat("grimoire_couverture", BOOK_RED, roughness=0.5)
    pages = mat("grimoire_pages", PARCH)
    clasp = mat("grimoire_fermoir", GOLD, roughness=0.3)
    # Dos (charnière) le long de X en y=-0,13 : la couverture pivote autour de X.
    cube("Item", cover, (0, 0, 0.03), (0.30, 0.27, 0.025))           # plat verso
    cube("Item", pages, (0.005, 0.005, 0.07), (0.28, 0.25, 0.055), smooth=False)
    cube("Item", cover, (0, -0.125, 0.07), (0.30, 0.02, 0.10))       # dos
    cube("Lid", cover, (0, 0, 0.11), (0.30, 0.27, 0.025))            # plat recto
    cube("Lid", clasp, (0.0, 0.125, 0.10), (0.05, 0.02, 0.03))       # fermoir

    def idle(key_rot, key_loc, key_scale):
        bob_keys(key_loc, "Item", 0.05, base=0.02)
        for f, sw in ((1, 0.04), (20, -0.04), (40, 0.04)):
            key_rot("Item", f, (0, sw, 0))
        # La couverture s'entrouvre (charnière = rotation autour de X au dos),
        # hésite, puis se referme d'un coup — un grimoire qui respire.
        for f, open_ in ((1, 0.0), (12, 0.35), (22, 0.28), (27, 0.0), (40, 0.0)):
            key_rot("Lid", f, (open_, 0, 0))

    build_item("item_book",
               {"Item": ("Root", *vbone(0.005, 0.2)),
                "Lid": ("Item", (0, -0.125, 0.105), (0, -0.125, 0.3))},
               idle, cam_dist=0.85)


# --- 26 Marteau : lévite hors du socle et pivote (~0,6 m) --------------------
def item_hammer():
    reset_scene()
    handle = mat("marteau_manche", WOOD_DARK)
    head = mat("marteau_tete", STEEL, roughness=0.3)
    stone = mat("marteau_socle", STONE)
    sphere("Root", stone, (0, 0, 0.07), (1, 1, 0.5), radius=0.13)    # socle figé
    cylinder("Hammer", handle, (0, 0, 0.30), radius=0.022, depth=0.50)
    cube("Hammer", head, (0, 0, 0.52), (0.24, 0.075, 0.095))         # tête
    for sx in (-1, 1):
        cube("Hammer", head, (sx * 0.125, 0, 0.52), (0.03, 0.09, 0.11))

    def idle(key_rot, key_loc, key_scale):
        for f, dz in ((1, 0.0), (14, 0.09), (26, 0.09), (40, 0.0)):
            key_loc("Hammer", f, (0, dz, 0))
        for f, a in ((1, 0.0), (14, math.pi), (26, math.pi), (40, TAU)):
            key_rot("Hammer", f, (0, a, 0))

    build_item("item_hammer", {"Hammer": ("Root", *vbone(0.03, 0.6))}, idle,
               cam_dist=1.0, target_z=0.3)


# --- 27 Couronne : toupie majestueuse + flottement (~0,3 m) ------------------
def item_crown():
    reset_scene()
    gold = mat("couronne_or", GOLD, roughness=0.3)
    gold_d = mat("couronne_or_d", GOLD_DARK, roughness=0.35)
    ruby = mat("couronne_rubis", JEWEL_RED, roughness=0.15)
    cyan = mat("couronne_gemme", GEM_CYAN, roughness=0.15)
    cylinder("Item", gold, (0, 0, 0.10), radius=0.11, depth=0.09, vertices=20,
             smooth=False)                                           # bandeau
    cylinder("Item", gold_d, (0, 0, 0.055), radius=0.115, depth=0.02,
             vertices=20, smooth=False)                              # jonc bas
    for deg in range(0, 360, 60):
        a = math.radians(deg)
        cone("Item", gold, (0.10 * math.cos(a), 0.10 * math.sin(a), 0.175),
             radius=0.024, depth=0.07, vertices=8)                   # pointes
        sphere("Item", gold, (0.10 * math.cos(a), 0.10 * math.sin(a), 0.215),
               radius=0.012, segments=8, rings=6)                    # perles
    for i, deg in enumerate(range(0, 360, 120)):
        a = math.radians(deg)
        m = ruby if i % 2 == 0 else cyan
        sphere("Item", m, (0.108 * math.cos(a), 0.108 * math.sin(a), 0.10),
               (1, 1, 1.2), radius=0.022, segments=10, rings=8)      # joyaux

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.05)

    build_item("item_crown", {"Item": ("Root", *vbone(0.02, 0.24))}, idle,
               cam_dist=0.8, linear_bones=("Item",))


# --- 28 Étoile : toupie scintillante à 5 branches (~0,35 m) ------------------
def item_star():
    reset_scene()
    star = mat("etoile_or", STAR_YELLOW, roughness=0.25)
    cz = 0.22
    sphere("Item", star, (0, 0, cz), (1, 0.35, 1), radius=0.058, smooth=False)
    # 5 branches dans le plan XZ : cônes aplatis pointés vers l'extérieur
    # (axe +Z tourné autour de Y — le plan vertical face caméra).
    for k in range(5):
        a = math.radians(90 + k * 72)
        dx, dz = math.cos(a), math.sin(a)
        cone("Item", star, (0.10 * dx, 0, cz + 0.10 * dz),
             radius=0.052, depth=0.15, vertices=4,
             rotation=(0, math.atan2(dx, dz), 0), scale=(1, 0.45, 1))

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.05)
        # Scintillement : deux pulsations par cycle.
        for f, s in ((1, 1.0), (10, 1.08), (20, 1.0), (30, 1.08), (40, 1.0)):
            key_scale("Item", f, (s, s, s))

    build_item("item_star", {"Item": ("Root", *vbone(0.05, 0.4))}, idle,
               cam_dist=0.9, target_z=0.22, linear_bones=("Item",))


# --- 29 Bombe : tremble nerveusement, l'étincelle crépite (~0,35 m) ----------
def item_bomb():
    reset_scene()
    body = mat("bombe_corps", BOMB_DARK, roughness=0.35)
    cap = mat("bombe_bouchon", (0.35, 0.36, 0.40), roughness=0.4)
    fuse = mat("bombe_meche", FUSE)
    spark = mat("bombe_etincelle", GLOW_YELLOW, roughness=0.2)
    sphere("Item", body, (0, 0, 0.14), radius=0.135)                 # corps
    cylinder("Item", cap, (0, 0, 0.285), radius=0.045, depth=0.04)   # bouchon
    cylinder("Item", fuse, (0.015, 0, 0.325), radius=0.012, depth=0.05,
             rotation=(0, math.radians(25), 0))                      # mèche
    cylinder("Item", fuse, (0.045, 0, 0.35), radius=0.012, depth=0.05,
             rotation=(0, math.radians(60), 0))
    sphere("Spark", spark, (0.075, 0, 0.365), radius=0.028,
           segments=10, rings=8)                                     # étincelle

    def idle(key_rot, key_loc, key_scale):
        # Tremblement nerveux (petites secousses rapides) + étincelle qui
        # crépite en changeant de taille à chaque battement.
        for f, sw in ((1, 0.0), (5, 0.05), (10, -0.05), (15, 0.04), (20, -0.04),
                      (25, 0.05), (30, -0.05), (35, 0.03), (40, 0.0)):
            key_rot("Item", f, (sw, 0, -sw))
        for f, s in ((1, 1.0), (6, 1.5), (12, 0.7), (18, 1.4), (24, 0.8),
                     (30, 1.5), (36, 0.9), (40, 1.0)):
            key_scale("Spark", f, (s, s, s))

    build_item("item_bomb",
               {"Item": ("Root", *vbone(0.005, 0.3)),
                "Spark": ("Item", (0.075, 0, 0.34), (0.075, 0, 0.42))},
               idle, cam_dist=0.85)


# --- 30 Poisson : frétille et saute — tout frais pêché (~0,4 m) --------------
def item_fish():
    reset_scene()
    body = mat("poisson_dos", FISH_BLUE, roughness=0.35)
    belly = mat("poisson_ventre", FISH_BELLY, roughness=0.4)
    fin = mat("poisson_nageoire", (0.32, 0.46, 0.60), roughness=0.5)
    dark = mat("poisson_oeil", DARK)
    sphere("Item", body, (0, 0, 0.105), (1.7, 0.55, 0.95), radius=0.105)  # corps
    sphere("Item", belly, (-0.02, 0, 0.075), (1.4, 0.5, 0.6), radius=0.10)
    cone("Item", fin, (0.02, 0, 0.21), radius=0.05, depth=0.09,
         rotation=(0, math.radians(-15), 0), scale=(1, 0.3, 1))      # dorsale
    for sy in (-1, 1):
        sphere("Item", dark, (-0.135, sy * 0.042, 0.135), radius=0.017,
               segments=8, rings=6)                                  # yeux
    # Queue (os Tail) : deux lobes évasés vers +X.
    for tilt in (35, -35):
        cone("Tail", fin, (0.24, 0, 0.105 + 0.035 * (1 if tilt > 0 else -0.6)),
             radius=0.045, depth=0.11, vertices=10,
             rotation=(0, math.radians(90 + tilt), 0), scale=(1, 0.3, 1))

    def idle(key_rot, key_loc, key_scale):
        # Frétillement : la queue bat, le corps se cambre en opposition,
        # puis un petit bond — un poisson qui refuse de finir en gigot.
        for f, wag in ((1, 0.35), (6, -0.35), (11, 0.35), (16, -0.35),
                       (21, 0.30), (30, -0.20), (40, 0.35)):
            key_rot("Tail", f, (0, wag, 0))
        for f, arc in ((1, -0.10), (6, 0.10), (11, -0.10), (16, 0.10),
                       (21, -0.08), (30, 0.05), (40, -0.10)):
            key_rot("Item", f, (arc * 0.5, arc, 0))
        for f, dz in ((1, 0.0), (18, 0.0), (24, 0.10), (30, 0.0), (40, 0.0)):
            key_loc("Item", f, (0, dz, 0))

    build_item("item_fish",
               {"Item": ("Root", *vbone(0.005, 0.25)),
                "Tail": ("Item", (0.16, 0, 0.105), (0.32, 0, 0.105))},
               idle, cam_dist=0.85)


item_carrot()
item_cheese()
item_egg()
item_feather()
item_book()
item_hammer()
item_crown()
item_star()
item_bomb()
item_fish()
print("PACK ITEMS 21-30 ANIM DONE")
