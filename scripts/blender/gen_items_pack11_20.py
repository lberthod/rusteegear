# Génère le pack « objets ramassables » 11-20 en Blender headless — version
# riggée + animée :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_items_pack11_20.py
#
# Sortie : assets/models/item_*.glb + item_*_preview.png (vignette de contrôle).
# 10 pickups : bouclier, hache, arc, pain, bourse, parchemin, anneau, cœur,
# fiole de mana, lanterne. Mêmes conventions que gen_items_pack01_10.py :
# - rig Root/… par objet, mesh unique skinné (1 os / partie, poids 1.0) ;
# - clip « Idle » 40 fr à 24 fps, bouclable — os verticaux (Y local = Z monde) ;
# - toupies continues 0→2π en interpolation LINEAR (sinon à-coup au bouclage) ;
# - seule couleur lue par le moteur : base_color_factor (pas de textures) ;
# - shade_smooth sur les parties organiques/rondes, facettes sur les lames ;
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
# Palette (cohérente avec les packs nature/items 01-10).
# ---------------------------------------------------------------------------
WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
STEEL = (0.72, 0.75, 0.80)
IRON_DARK = (0.25, 0.26, 0.30)
GOLD = (0.85, 0.65, 0.15)
GOLD_DARK = (0.62, 0.45, 0.10)
ACCENT_RED = (0.72, 0.14, 0.12)
HEART_RED = (0.85, 0.12, 0.18)
CRUST = (0.55, 0.35, 0.14)
CRUMB = (0.85, 0.72, 0.45)
LEATHER = (0.42, 0.27, 0.14)
PARCH = (0.88, 0.82, 0.64)
PARCH_DARK = (0.70, 0.62, 0.44)
GLASS = (0.55, 0.70, 0.80)
MANA_BLUE = (0.18, 0.35, 0.85)
CORK = (0.50, 0.36, 0.20)
GLOW_YELLOW = (1.0, 0.78, 0.35)
GEM_CYAN = (0.20, 0.75, 0.80)
STONE = (0.40, 0.39, 0.38)


# --- 11 Bouclier : balancement fier + flottement (~0,55 m) -------------------
def item_shield():
    reset_scene()
    wood = mat("bouclier_bois", WOOD)
    iron = mat("bouclier_fer", IRON_DARK, roughness=0.4)
    boss = mat("bouclier_umbo", GOLD, roughness=0.35)
    face = (math.radians(90), 0, 0)  # plateau dressé, face vers -Y
    cylinder("Item", wood, (0, 0.01, 0.28), radius=0.25, depth=0.035,
             rotation=face, vertices=24, smooth=False)
    cylinder("Item", iron, (0, 0.03, 0.28), radius=0.27, depth=0.025,
             rotation=face, vertices=24, smooth=False)
    sphere("Item", boss, (0, -0.02, 0.28), radius=0.07)

    def idle(key_rot, key_loc, key_scale):
        for f, sw in ((1, 0.10), (20, -0.10), (40, 0.10)):
            key_rot("Item", f, (0, sw, 0))
        bob_keys(key_loc, "Item", 0.03)

    build_item("item_shield", {"Item": ("Root", *vbone(0.01, 0.55))}, idle,
               cam_dist=1.0, target_z=0.28)


# --- 12 Hache : lévite hors du socle avec un demi-tour (~0,6 m) --------------
def item_axe():
    reset_scene()
    handle = mat("hache_manche", WOOD_DARK)
    head = mat("hache_tete", STEEL, roughness=0.3)
    edge = mat("hache_taillant", (0.88, 0.90, 0.94), roughness=0.2)
    stone = mat("hache_socle", STONE)
    sphere("Root", stone, (0, 0, 0.07), (1, 1, 0.5), radius=0.13)     # socle figé
    cylinder("Axe", handle, (0, 0, 0.32), radius=0.024, depth=0.55)   # manche
    cube("Axe", head, (0.09, 0, 0.52), (0.16, 0.045, 0.12))           # tête
    cube("Axe", edge, (0.185, 0, 0.52), (0.035, 0.04, 0.16))          # taillant
    cube("Axe", head, (-0.045, 0, 0.52), (0.06, 0.05, 0.06))          # contrepoids

    def idle(key_rot, key_loc, key_scale):
        for f, dz in ((1, 0.0), (14, 0.09), (26, 0.09), (40, 0.0)):
            key_loc("Axe", f, (0, dz, 0))
        for f, a in ((1, 0.0), (14, math.pi), (26, math.pi), (40, TAU)):
            key_rot("Axe", f, (0, a, 0))

    build_item("item_axe", {"Axe": ("Root", *vbone(0.03, 0.6))}, idle,
               cam_dist=1.0, target_z=0.32)


# --- 13 Arc : flottement + léger balancement de visée (~0,7 m) ---------------
def item_bow():
    reset_scene()
    wood = mat("arc_bois", WOOD)
    grip = mat("arc_poignee", LEATHER)
    string = mat("arc_corde", (0.85, 0.83, 0.75))
    # Arc dans le plan XZ : arc de cercle rayon 0,32 centré à (x=0,26, z=0,38).
    cx, cz, r = 0.26, 0.38, 0.32
    # Pas de 5° pour que les sphères se chevauchent : branche continue.
    for deg in range(115, 246, 5):
        a = math.radians(deg)
        sphere("Item", wood, (cx + r * math.cos(a), 0, cz + r * math.sin(a)),
               (1, 1, 1.2), radius=0.026, segments=10, rings=8)
    sphere("Item", grip, (cx - r, 0, cz), (1, 1, 1.8), radius=0.032)  # poignée
    top_a, bot_a = math.radians(115), math.radians(245)
    top = (cx + r * math.cos(top_a), cz + r * math.sin(top_a))
    bot = (cx + r * math.cos(bot_a), cz + r * math.sin(bot_a))
    mid = ((top[0] + bot[0]) / 2, (top[1] + bot[1]) / 2)
    length = math.hypot(top[0] - bot[0], top[1] - bot[1])
    tilt = math.atan2(top[0] - bot[0], top[1] - bot[1])
    cylinder("Item", string, (mid[0], 0, mid[1]), radius=0.008, depth=length,
             rotation=(0, tilt, 0), vertices=8)

    def idle(key_rot, key_loc, key_scale):
        for f, sw in ((1, 0.08), (20, -0.08), (40, 0.08)):
            key_rot("Item", f, (0, sw, 0))
        bob_keys(key_loc, "Item", 0.04)

    build_item("item_bow", {"Item": ("Root", *vbone(0.02, 0.7))}, idle,
               cam_dist=1.05, target_z=0.38)


# --- 14 Pain : flottement gourmand + inclinaison (~0,35 m) -------------------
def item_bread():
    reset_scene()
    crust = mat("pain_croute", CRUST, roughness=0.6)
    crumb = mat("pain_mie", CRUMB)
    sphere("Item", crust, (0, 0, 0.10), (1.6, 1.0, 0.85), radius=0.11)
    for dx in (-0.08, 0.0, 0.08):
        cube("Item", crumb, (dx, 0, 0.172), (0.016, 0.12, 0.014),
             rotation=(0, 0, math.radians(18)))                       # grignes

    def idle(key_rot, key_loc, key_scale):
        for f, sw in ((1, 0.05), (20, -0.05), (40, 0.05)):
            key_rot("Item", f, (sw, 0, sw))
        bob_keys(key_loc, "Item", 0.03)

    build_item("item_bread", {"Item": ("Root", *vbone(0.01, 0.22))}, idle,
               cam_dist=0.75)


# --- 15 Bourse : le sac respire, les pièces restent au sol (~0,25 m) ---------
def item_pouch():
    reset_scene()
    leather = mat("bourse_cuir", LEATHER)
    tie = mat("bourse_lien", WOOD_DARK)
    gold = mat("bourse_or", GOLD, roughness=0.3)
    sphere("Bag", leather, (0, 0, 0.116), (1, 1, 1.05), radius=0.11)  # panse
    cylinder("Bag", tie, (0, 0, 0.215), radius=0.035, depth=0.03)     # lien noué
    sphere("Bag", leather, (0, 0, 0.245), (1, 1, 0.75), radius=0.045)  # goulot
    for x, y, rot in ((0.13, -0.06, 0.4), (0.16, 0.04, -0.3), (-0.14, -0.03, 0.8)):
        cylinder("Root", gold, (x, y, 0.02), radius=0.035, depth=0.012,
                 rotation=(0, rot * 0.3, rot), vertices=16, smooth=False)

    def idle(key_rot, key_loc, key_scale):
        # Le sac frémit — quelque chose bouge à l'intérieur…
        for f, s in ((1, 1.0), (8, 1.05), (14, 0.97), (20, 1.02), (40, 1.0)):
            key_scale("Bag", f, (s, 1.0 / s, s))
        for f, sw in ((1, 0.0), (8, 0.05), (14, -0.05), (20, 0.0), (40, 0.0)):
            key_rot("Bag", f, (0, 0, sw))

    build_item("item_pouch", {"Bag": ("Root", *vbone(0.01, 0.28))}, idle,
               cam_dist=0.7)


# --- 16 Parchemin : tangue doucement comme posé sur l'eau (~0,3 m) -----------
def item_scroll():
    reset_scene()
    parch = mat("parchemin_papier", PARCH, roughness=0.7)
    parch_d = mat("parchemin_tranche", PARCH_DARK)
    ribbon = mat("parchemin_ruban", ACCENT_RED, roughness=0.5)
    lay = (0, math.radians(90), 0)  # couché le long de X
    cylinder("Item", parch, (0, 0, 0.056), radius=0.05, depth=0.30, rotation=lay)
    for sx in (-1, 1):
        cylinder("Item", parch_d, (sx * 0.145, 0, 0.056), radius=0.035,
                 depth=0.02, rotation=lay)
    cylinder("Item", ribbon, (0.02, 0, 0.056), radius=0.054, depth=0.035,
             rotation=lay)
    cube("Item", ribbon, (0.02, 0.045, 0.028), (0.03, 0.055, 0.012),
         rotation=(math.radians(-30), 0, 0))                          # nœud

    def idle(key_rot, key_loc, key_scale):
        # Roulis autour de l'axe long (X) + petit flottement.
        for f, roll in ((1, 0.10), (20, -0.10), (40, 0.10)):
            key_rot("Item", f, (roll, 0, 0))
        bob_keys(key_loc, "Item", 0.03)

    build_item("item_scroll", {"Item": ("Root", *vbone(0.01, 0.2))}, idle,
               cam_dist=0.7)


# --- 17 Anneau : toupie précieuse + flottement (~0,25 m) ---------------------
def item_ring():
    reset_scene()
    gold = mat("anneau_or", GOLD, roughness=0.25)
    gold_d = mat("anneau_or_d", GOLD_DARK, roughness=0.3)
    gem = mat("anneau_gemme", GEM_CYAN, roughness=0.1)
    torus("Item", gold, (0, 0, 0.116), major=0.09, minor=0.024,
          rotation=(math.radians(90), 0, 0))                          # jonc
    cube("Item", gold_d, (0, 0, 0.205), (0.05, 0.05, 0.03))           # chaton
    sphere("Item", gem, (0, 0, 0.235), (1, 1, 0.8), radius=0.035,
           smooth=False)                                              # gemme

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.05)

    build_item("item_ring", {"Item": ("Root", *vbone(0.005, 0.27))}, idle,
               cam_dist=0.65, linear_bones=("Item",))


# --- 18 Cœur : battement cardiaque (double pulsation) + flottement (~0,3 m) --
def item_heart():
    reset_scene()
    red = mat("coeur_rouge", HEART_RED, roughness=0.3)
    shine = mat("coeur_reflet", (1.0, 0.65, 0.70), roughness=0.25)
    cone("Item", red, (0, 0, 0.13), radius=0.115, depth=0.18,
         rotation=(math.radians(180), 0, 0), vertices=16)
    for sx in (-1, 1):
        sphere("Item", red, (sx * 0.065, 0, 0.235), radius=0.078)     # lobes
    sphere("Item", shine, (-0.075, -0.045, 0.27), (1, 1, 1.3),
           radius=0.022, segments=10, rings=8)                        # reflet

    def idle(key_rot, key_loc, key_scale):
        # Toum-toum : deux pulsations rapprochées puis repos — un vrai rythme
        # cardiaque, pas un simple gonflement.
        for f, s in ((1, 1.0), (5, 1.14), (9, 1.0), (13, 1.10), (17, 1.0),
                     (40, 1.0)):
            key_scale("Item", f, (s, s, s))
        bob_keys(key_loc, "Item", 0.04)
        for f in (1, 40):
            key_rot("Item", f, (0, 0, 0))

    build_item("item_heart", {"Item": ("Root", *vbone(0.03, 0.33))}, idle,
               cam_dist=0.75)


# --- 19 Fiole de mana : toupie + flottement (~0,3 m) -------------------------
def item_mana():
    reset_scene()
    liquid = mat("mana_liquide", MANA_BLUE, roughness=0.2)
    glass = mat("mana_verre", GLASS, roughness=0.15)
    cork = mat("mana_bouchon", CORK)
    # Verre opaque : le corps porte la couleur du liquide (cf. item_potion).
    cone("Item", liquid, (0, 0, 0.10), radius=0.11, depth=0.20,
         radius2=0.04, smooth=True)                                   # erlenmeyer
    sphere("Item", glass, (-0.05, -0.055, 0.09), (1, 1, 1.5),
           radius=0.02, segments=10, rings=8)                         # reflet
    cylinder("Item", glass, (0, 0, 0.235), radius=0.035, depth=0.09)  # col
    cylinder("Item", cork, (0, 0, 0.29), radius=0.04, depth=0.045)    # bouchon

    def idle(key_rot, key_loc, key_scale):
        spin_keys(key_rot, "Item")
        bob_keys(key_loc, "Item", 0.04)

    build_item("item_mana", {"Item": ("Root", *vbone(0.005, 0.31))}, idle,
               cam_dist=0.75, linear_bones=("Item",))


# --- 20 Lanterne : oscille doucement, le verre palpite (~0,4 m) --------------
def item_lantern():
    reset_scene()
    iron = mat("lanterne_fer", IRON_DARK, roughness=0.45)
    glow = mat("lanterne_verre", GLOW_YELLOW, roughness=0.3)
    cylinder("Item", iron, (0, 0, 0.02), radius=0.095, depth=0.04,
             smooth=False)                                            # socle
    cylinder("Glass", glow, (0, 0, 0.15), radius=0.075, depth=0.22,
             vertices=12)                                             # verre
    for deg in (0, 90, 180, 270):
        a = math.radians(deg)
        cylinder("Item", iron, (0.078 * math.cos(a), 0.078 * math.sin(a), 0.15),
                 radius=0.012, depth=0.24, vertices=8, smooth=False)  # montants
    cone("Item", iron, (0, 0, 0.30), radius=0.11, depth=0.08, radius2=0.02)
    torus("Item", iron, (0, 0, 0.375), major=0.045, minor=0.012,
          rotation=(math.radians(90), 0, 0))                          # anse

    def idle(key_rot, key_loc, key_scale):
        # La lanterne oscille à peine ; la flamme palpite dans le verre.
        for f, sw in ((1, 0.04), (20, -0.04), (40, 0.04)):
            key_rot("Item", f, (sw, 0, 0))
        for f, s in ((1, 1.0), (7, 1.06), (13, 0.98), (22, 1.05), (30, 1.0),
                     (40, 1.0)):
            key_scale("Glass", f, (s, 1.0, s))

    build_item("item_lantern",
               {"Item": ("Root", *vbone(0.005, 0.4)),
                "Glass": ("Item", *vbone(0.04, 0.28))},
               idle, cam_dist=0.95, target_z=0.2)


item_shield()
item_axe()
item_bow()
item_bread()
item_pouch()
item_scroll()
item_ring()
item_heart()
item_mana()
item_lantern()
print("PACK ITEMS 11-20 ANIM DONE")
