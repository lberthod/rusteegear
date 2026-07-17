"""Génère assets/models/creature27.glb … creature31.glb : 5 monstres animés.

Pack « insectes & arachnides » — araignée géante, mante religieuse, scarabée
rhinocéros, frelon géant, mille-pattes. Mêmes conventions que le pack 22-26 :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/… par créature, mesh unique skinné (1 os / partie, poids 1.0) ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables, chaque clip
  keyframe tous les os animés par l'autre (piège glTF : canaux absents = os figé) ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur) ;
- échelle appliquée AVANT la rotation dans add_part (piège rotation/scale des
  cônes : sinon l'étirement se fait dans les axes monde et déforme la pièce) ;
- pose remise au neutre avant export ET avant la vignette (l'exporteur glTF
  laisse l'armature posée au dernier frame évalué).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack27_31.py
"""

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

PARTS = []


def material(name, rgb, roughness=0.85):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    return m


def add_part(bone, mat, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    create_op(location=location)
    ob = bpy.context.active_object
    ob.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    ob.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    ob.data.materials.append(mat)
    vg = ob.vertex_groups.new(name=bone)
    vg.add(range(len(ob.data.vertices)), 1.0, "REPLACE")
    PARTS.append(ob)
    return ob


def sphere(bone, mat, location, scale, segments=24, rings=16):
    def op(location):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=segments, ring_count=rings, radius=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale)


def cone(bone, mat, location, scale, rotation=(0, 0, 0)):
    def op(location):
        bpy.ops.mesh.primitive_cone_add(
            vertices=16, radius1=1.0, radius2=0.0, depth=2.0, location=location
        )

    return add_part(bone, mat, op, location, scale, rotation)


def cylinder(bone, mat, location, scale, rotation=(0, 0, 0)):
    def op(location):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=16, radius=1.0, depth=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale, rotation)


def build_creature(name, bones, idle_keys, walk_keys, cam=1.0):
    """Fusionne PARTS, pose l'armature `bones`, bake Idle/Walk, exporte + vignette."""
    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = PARTS[0]
    bpy.ops.object.join()
    creature = bpy.context.active_object
    creature.name = name.capitalize()

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = f"{creature.name}Rig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.35))
    for bname, (parent, head, tail) in bones.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    creature.parent = arm
    creature.modifiers.new("Armature", "ARMATURE").object = arm

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

    def bake_clip(clip, keyer):
        ad = arm.animation_data_create()
        ad.action = None
        keyer(key_rot, key_loc)
        act = ad.action
        act.name = clip
        track = ad.nla_tracks.new()
        track.name = clip
        track.strips.new(clip, 1, act)
        ad.action = None

    bake_clip("Idle", idle_keys)
    bake_clip("Walk", walk_keys)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
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

    # Vignette de contrôle : pistes NLA purgées + pose neutre (piège connu).
    ad = arm.animation_data
    ad.action = None
    for t in list(ad.nla_tracks):
        ad.nla_tracks.remove(t)
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    scene = bpy.context.scene
    scene.frame_set(1)
    bpy.context.view_layer.update()
    # `cam` : facteur d'éloignement/hauteur pour cadrer les grands gabarits.
    bpy.ops.object.camera_add(
        location=(5.2 * cam, -7.0 * cam, 3.6 * cam),
        rotation=(math.radians(74), 0, math.radians(37)),
    )
    scene.camera = bpy.context.active_object
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


def fresh_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.render.fps = 24
    PARTS.clear()


LEGS4 = ("LegFL", "LegFR", "LegBL", "LegBR")


# =============================================================================
# Créature 27 — Araignée géante : 8 pattes arquées, gros abdomen, crochets.
# =============================================================================
def araignee():
    fresh_scene()
    body = material("Araignee27Body", (0.13, 0.10, 0.09))
    joint = material("Araignee27Joint", (0.30, 0.20, 0.12))
    mark = material("Araignee27Mark", (0.72, 0.12, 0.10))
    fang = material("Araignee27Fang", (0.85, 0.82, 0.72))
    eye = material("Araignee27Eye", (0.55, 0.05, 0.05), roughness=0.3)

    # Abdomen bombé à l'arrière (os Tail pour qu'il tressaute), céphalothorax
    # devant (os Body), marque rouge dorsale.
    sphere("Tail", body, (0, 0.55, 0.72), (0.55, 0.62, 0.52))
    sphere("Tail", mark, (0, 0.62, 1.18), (0.16, 0.26, 0.10))  # marque dorsale
    sphere("Body", body, (0, -0.35, 0.55), (0.40, 0.45, 0.32))
    # 8 yeux : deux rangées de 4 sur l'avant du céphalothorax.
    for sx, dz, s in ((-0.16, 0.12, 0.055), (-0.06, 0.16, 0.07),
                      (0.06, 0.16, 0.07), (0.16, 0.12, 0.055),
                      (-0.11, 0.02, 0.045), (-0.04, 0.04, 0.05),
                      (0.04, 0.04, 0.05), (0.11, 0.02, 0.045)):
        sphere("Body", eye, (sx, -0.76, 0.62 + dz), (s, s * 0.8, s))
    # Chélicères + crochets recourbés vers le bas (os Head : ils s'écartent).
    for sx in (-1, 1):
        sphere("Head", body, (sx * 0.10, -0.80, 0.42), (0.09, 0.12, 0.11))
        cone("Head", fang, (sx * 0.10, -0.88, 0.26), (0.045, 0.045, 0.10),
             rotation=(math.radians(190), 0, 0))
        # Pédipalpes : petits bras sensoriels sur les côtés.
        cylinder("Head", joint, (sx * 0.28, -0.78, 0.48), (0.045, 0.045, 0.30),
                 rotation=(math.radians(60), 0, math.radians(-sx * 25)))
    # 8 pattes arquées : 2 pattes par os, fémur qui monte + tibia qui redescend.
    for bone, y in (("LegFL", -0.48), ("LegFR", -0.48),
                    ("LegBL", 0.10), ("LegBR", 0.10)):
        sx = 1 if bone.endswith("R") else -1
        for dy in (0.0, 0.26):
            # Fémur : part du corps vers le haut-dehors.
            cylinder(bone, body, (sx * 0.58, y + dy, 0.72), (0.055, 0.055, 0.55),
                     rotation=(0, math.radians(sx * 62), 0))
            sphere(bone, joint, (sx * 0.82, y + dy, 0.85), (0.075, 0.075, 0.075))
            # Tibia : part de l'articulation (0.82, 0.85) et redescend au sol
            # en s'écartant — tilt inversé (-sx) pour que le haut du cylindre
            # reste collé au genou et le bas touche terre vers l'extérieur.
            cylinder(bone, body, (sx * 0.96, y + dy, 0.43), (0.045, 0.045, 0.88),
                     rotation=(0, math.radians(-sx * 19), 0))

    bones = {
        "Body": ("Root", (0, 0.15, 0.55), (0, -0.60, 0.55)),
        "Head": ("Body", (0, -0.62, 0.45), (0, -0.95, 0.25)),
        "Tail": ("Body", (0, 0.15, 0.60), (0, 0.85, 0.75)),
        "LegFL": ("Body", (-0.35, -0.40, 0.60), (-1.10, -0.35, 0.02)),
        "LegFR": ("Body", (0.35, -0.40, 0.60), (1.10, -0.35, 0.02)),
        "LegBL": ("Body", (-0.35, 0.20, 0.60), (-1.10, 0.30, 0.02)),
        "LegBR": ("Body", (0.35, 0.20, 0.60), (1.10, 0.30, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Tapie : l'abdomen palpite, les crochets s'écartent, deux pattes avant
        # tâtent le sol en alternance.
        for f in (1, 40):
            key_rot("LegBL", f, (0, 0, 0))
            key_rot("LegBR", f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, dz in ((1, 0.0), (12, 0.05), (20, 0.0), (32, 0.05), (40, 0.0)):
            key_loc("Tail", f, (0, dz, 0))
        for f, puls in ((1, 0.0), (12, 0.10), (20, 0.0), (32, 0.10), (40, 0.0)):
            key_rot("Tail", f, (puls, 0, 0))
        for f, open_ in ((1, 0.0), (10, 0.30), (18, 0.05), (26, 0.30), (40, 0.0)):
            key_rot("Head", f, (open_ * 0.4, 0, 0))
        for f, tap in ((1, 0.0), (8, 0.35), (16, 0.0), (40, 0.0)):
            key_rot("LegFL", f, (tap, 0, 0))
        for f, tap in ((1, 0.0), (20, 0.0), (28, 0.35), (36, 0.0), (40, 0.0)):
            key_rot("LegFR", f, (tap, 0, 0))
        for f, sw in ((1, 0.03), (20, -0.03), (40, 0.03)):
            key_rot("Body", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Trottinement arachnide : diagonales opposées, abdomen qui tangue.
        swing = math.radians(30)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.10), (13, -0.10), (24, 0.10)):
            key_rot("Tail", f, (0, 0, sw))
        for f in (1, 24):
            key_rot("Head", f, (0, 0, 0))

    build_creature("creature27", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 28 — Mante religieuse : buste dressé, bras ravisseurs repliés.
# =============================================================================
def mante():
    fresh_scene()
    green = material("Mante28Green", (0.28, 0.55, 0.16))
    green_d = material("Mante28GreenD", (0.16, 0.36, 0.10))
    belly = material("Mante28Belly", (0.62, 0.74, 0.38))
    eye = material("Mante28Eye", (0.80, 0.70, 0.20), roughness=0.35)
    dark = material("Mante28Dark", (0.06, 0.08, 0.04))

    # Abdomen long et bas (os Tail), ailes plaquées dessus, thorax dressé (Body).
    sphere("Tail", green, (0, 0.55, 0.72), (0.24, 0.72, 0.22))
    sphere("Tail", belly, (0, 0.55, 0.62), (0.19, 0.62, 0.14))
    sphere("Tail", green_d, (0, 0.62, 0.88), (0.20, 0.60, 0.07))  # ailes pliées
    cylinder("Body", green, (0, -0.10, 1.05), (0.13, 0.13, 0.85),
             rotation=(math.radians(35), 0, 0))  # thorax incliné
    # Tête triangulaire mobile + gros yeux globuleux + antennes.
    sphere("Head", green, (0, -0.38, 1.55), (0.20, 0.16, 0.15))
    cone("Head", belly, (0, -0.42, 1.42), (0.09, 0.06, 0.10),
         rotation=(math.radians(170), 0, 0))  # pointe du menton
    for sx in (-1, 1):
        sphere("Head", eye, (sx * 0.19, -0.42, 1.60), (0.09, 0.09, 0.11))
        sphere("Head", dark, (sx * 0.21, -0.49, 1.62), (0.035, 0.03, 0.04))
        cylinder("Head", green_d, (sx * 0.08, -0.42, 1.82), (0.02, 0.02, 0.40),
                 rotation=(math.radians(-25), 0, math.radians(sx * 18)))  # antenne
    # Bras ravisseurs repliés en garde (os ArmL/ArmR) : fémur épineux + tibia.
    for bone, sx in (("ArmL", -1), ("ArmR", 1)):
        sphere(bone, green, (sx * 0.20, -0.22, 1.15), (0.10, 0.10, 0.10))
        cylinder(bone, green, (sx * 0.26, -0.42, 1.00), (0.07, 0.07, 0.52),
                 rotation=(math.radians(48), 0, 0))  # fémur vers l'avant-bas
        for i in range(3):  # épines du fémur
            cone(bone, dark, (sx * 0.26, -0.50 - i * 0.09, 0.90 + i * 0.07),
                 (0.02, 0.02, 0.05), rotation=(math.radians(210), 0, 0))
        cylinder(bone, green_d, (sx * 0.26, -0.50, 1.16), (0.05, 0.05, 0.45),
                 rotation=(math.radians(-55), 0, 0))  # tibia replié vers le haut
        cone(bone, dark, (sx * 0.26, -0.34, 1.34), (0.03, 0.03, 0.08),
             rotation=(math.radians(-40), 0, 0))  # griffe
    # 4 pattes marcheuses fines, ancrées sous le thorax et l'abdomen.
    for bone, x, y in (("LegFL", -0.20, 0.05), ("LegFR", 0.20, 0.05),
                       ("LegBL", -0.22, 0.60), ("LegBR", 0.22, 0.60)):
        sx = 1 if x > 0 else -1
        cylinder(bone, green, (x + sx * 0.14, y, 0.45), (0.04, 0.04, 0.70),
                 rotation=(0, math.radians(sx * 22), 0))

    bones = {
        "Body": ("Root", (0, 0.20, 0.75), (0, -0.30, 1.40)),
        "Head": ("Body", (0, -0.30, 1.42), (0, -0.50, 1.70)),
        "Tail": ("Body", (0, 0.15, 0.72), (0, 1.10, 0.70)),
        "ArmL": ("Body", (-0.20, -0.20, 1.18), (-0.28, -0.55, 0.80)),
        "ArmR": ("Body", (0.20, -0.20, 1.18), (0.28, -0.55, 0.80)),
        "LegFL": ("Body", (-0.20, 0.05, 0.70), (-0.38, 0.05, 0.02)),
        "LegFR": ("Body", (0.20, 0.05, 0.70), (0.38, 0.05, 0.02)),
        "LegBL": ("Tail", (-0.22, 0.60, 0.70), (-0.40, 0.60, 0.02)),
        "LegBR": ("Tail", (0.22, 0.60, 0.70), (0.40, 0.60, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Prière puis coup éclair : les bras se déplient d'un coup (fr 22-25)
        # avant de se replier lentement. La tête pivote, très mante.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
            key_rot("Tail", f, (0, 0, 0))
        for f, sway in ((1, 0.0), (12, 0.06), (30, -0.04), (40, 0.0)):
            key_rot("Body", f, (sway, 0, 0))
        for f, yaw in ((1, 0.0), (10, 0.45), (18, 0.45), (22, 0.0), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))
        strike = ((1, 0.0), (20, 0.10), (22, -0.85), (25, -0.85), (34, 0.0), (40, 0.0))
        for f, a in strike:
            key_rot("ArmL", f, (a, 0, 0))
            key_rot("ArmR", f, (a, 0, 0))

    def walk(key_rot, key_loc):
        # Démarche saccadée, buste qui oscille, bras en garde serrée.
        swing = math.radians(26)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.04), (13, 0.0), (19, 0.04), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, roll in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Body", f, (0, roll, 0))
        for f, guard in ((1, 0.10), (13, 0.18), (24, 0.10)):
            key_rot("ArmL", f, (guard, 0, 0))
            key_rot("ArmR", f, (guard, 0, 0))
        for f, yaw in ((1, -0.08), (13, 0.08), (24, -0.08)):
            key_rot("Head", f, (0, 0, yaw))
        for f, sw in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature("creature28", bones, idle, walk)


# =============================================================================
# Créature 29 — Scarabée rhinocéros : carapace bombée, grande corne frontale.
# =============================================================================
def scarabee():
    fresh_scene()
    shell = material("Scarabee29Shell", (0.16, 0.09, 0.05), roughness=0.35)
    shell_l = material("Scarabee29ShellL", (0.30, 0.16, 0.08), roughness=0.4)
    horn = material("Scarabee29Horn", (0.10, 0.06, 0.04), roughness=0.5)
    belly = material("Scarabee29Belly", (0.42, 0.28, 0.14))
    eye = material("Scarabee29Eye", (0.85, 0.75, 0.45), roughness=0.3)

    # Élytres bombés (os Body) avec sillon central, ventre clair dessous.
    sphere("Body", shell, (0, 0.25, 0.62), (0.55, 0.75, 0.42))
    cylinder("Body", shell_l, (0, 0.30, 0.98), (0.03, 0.03, 1.10),
             rotation=(math.radians(90), 0, 0))  # sillon des élytres
    sphere("Body", belly, (0, 0.20, 0.42), (0.44, 0.65, 0.22))
    # Pronotum : bouclier du thorax, avec une petite corne secondaire.
    sphere("Body", shell_l, (0, -0.48, 0.70), (0.38, 0.30, 0.32))
    cone("Body", horn, (0, -0.55, 1.02), (0.06, 0.06, 0.16),
         rotation=(math.radians(-20), 0, 0))
    # Tête basse (os Head) + grande corne en Y recourbée vers le haut.
    sphere("Head", shell, (0, -0.85, 0.48), (0.22, 0.22, 0.18))
    for sx in (-1, 1):
        sphere("Head", eye, (sx * 0.15, -0.95, 0.55), (0.05, 0.045, 0.05))
    # Fût de la corne : part de la tête (0, -0.95, 0.50) et se dresse vers
    # l'avant-haut (65° : +Z bascule vers -Y en gardant une forte composante
    # verticale — à 125° elle piquait vers le sol, vu sur la 1re vignette).
    cylinder("Head", horn, (0, -1.27, 0.65), (0.06, 0.06, 0.70),
             rotation=(math.radians(65), 0, 0))
    for sx in (-1, 1):  # fourche du bout de corne, pointes vers l'avant-haut
        cone("Head", horn, (sx * 0.06, -1.60, 0.82), (0.04, 0.04, 0.14),
             rotation=(math.radians(45), 0, math.radians(sx * 15)))
    # 6 pattes crochues : avant sur un os chacune, milieu+arrière appariées.
    for bone, x, y in (("LegFL", -0.42, -0.55), ("LegFR", 0.42, -0.55),
                       ("LegBL", -0.48, 0.15), ("LegBR", 0.48, 0.15)):
        sx = 1 if x > 0 else -1
        dys = (0.0,) if y < 0 else (0.0, 0.45)
        for dy in dys:
            # Tilt -sx : le haut du cylindre s'ancre sous la carapace et le bas
            # s'écarte vers l'extérieur (le bout tombe pile sur la griffe).
            cylinder(bone, shell, (x + sx * 0.10, y + dy, 0.32),
                     (0.06, 0.06, 0.55), rotation=(0, math.radians(-sx * 30), 0))
            cone(bone, horn, (x + sx * 0.26, y + dy, 0.05), (0.035, 0.035, 0.09),
                 rotation=(0, math.radians(sx * 160), 0))  # griffe

    bones = {
        "Body": ("Root", (0, 0.60, 0.60), (0, -0.60, 0.60)),
        "Head": ("Body", (0, -0.68, 0.50), (0, -1.45, 0.95)),
        "LegFL": ("Body", (-0.38, -0.55, 0.40), (-0.62, -0.55, 0.02)),
        "LegFR": ("Body", (0.38, -0.55, 0.40), (0.62, -0.55, 0.02)),
        "LegBL": ("Body", (-0.42, 0.35, 0.40), (-0.70, 0.40, 0.02)),
        "LegBR": ("Body", (0.42, 0.35, 0.40), (0.70, 0.40, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Coup de corne : la tête se baisse, charge, puis toss vers le haut.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (18, -0.03), (24, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, pitch in ((1, 0.0), (14, 0.35), (20, 0.35), (24, -0.40), (32, 0.0), (40, 0.0)):
            key_rot("Head", f, (pitch, 0, 0))
        for f, lean in ((1, 0.0), (14, 0.08), (24, -0.10), (32, 0.0), (40, 0.0)):
            key_rot("Body", f, (lean, 0, 0))

    def walk(key_rot, key_loc):
        # Pas lourd et régulier, corne qui laboure légèrement.
        swing = math.radians(22)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, pitch in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Head", f, (pitch, 0, 0))
        for f, roll in ((1, 0.04), (13, -0.04), (24, 0.04)):
            key_rot("Body", f, (0, roll, 0))

    build_creature("creature29", bones, idle, walk, cam=0.9)


# =============================================================================
# Créature 30 — Frelon géant : abdomen rayé, ailes vrombissantes, dard.
# =============================================================================
def frelon():
    fresh_scene()
    amber = material("Frelon30Amber", (0.85, 0.55, 0.10))
    stripe = material("Frelon30Stripe", (0.12, 0.09, 0.05))
    wing = material("Frelon30Wing", (0.80, 0.84, 0.88), roughness=0.25)
    eye = material("Frelon30Eye", (0.30, 0.12, 0.04), roughness=0.3)
    dark = material("Frelon30Dark", (0.08, 0.06, 0.04))

    # Vol stationnaire : tout le corps est porté haut, pattes pendantes.
    # Abdomen rayé qui pointe vers le bas-arrière (os Tail) + dard.
    for i, (y, z, s) in enumerate(((0.42, 1.05, 0.30), (0.62, 0.92, 0.26),
                                   (0.78, 0.80, 0.20))):
        mat = amber if i % 2 == 0 else stripe
        sphere("Tail", mat, (0, y, z), (s, s * 0.9, s * 0.95))
    cone("Tail", dark, (0, 0.94, 0.66), (0.05, 0.05, 0.14),
         rotation=(math.radians(35), 0, 0))  # dard
    # Thorax duveteux (os Body), tête large (os Head) : yeux énormes, antennes,
    # mandibules.
    sphere("Body", stripe, (0, 0.02, 1.18), (0.30, 0.34, 0.30))
    sphere("Head", amber, (0, -0.38, 1.20), (0.24, 0.20, 0.22))
    for sx in (-1, 1):
        sphere("Head", eye, (sx * 0.16, -0.46, 1.24), (0.11, 0.10, 0.14))
        cylinder("Head", dark, (sx * 0.08, -0.52, 1.42), (0.02, 0.02, 0.30),
                 rotation=(math.radians(-40), 0, math.radians(sx * 15)))  # antenne
        cone("Head", dark, (sx * 0.08, -0.58, 1.08), (0.035, 0.035, 0.09),
             rotation=(math.radians(160), 0, math.radians(-sx * 15)))  # mandibule
    # Deux paires d'ailes translucides (os WingL/WingR), à plat vers l'extérieur.
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        sphere(bone, wing, (sx * 0.62, 0.10, 1.34), (0.48, 0.16, 0.03))
        sphere(bone, wing, (sx * 0.50, 0.32, 1.32), (0.34, 0.12, 0.03))
    # 6 pattes fines repliées sous le thorax, pendantes.
    for bone, y in (("LegL", -0.10), ("LegR", -0.10)):
        sx = 1 if bone == "LegR" else -1
        for dy in (0.0, 0.14, 0.28):
            cylinder(bone, dark, (sx * 0.26, y + dy, 0.92), (0.025, 0.025, 0.38),
                     rotation=(math.radians(15), math.radians(sx * 18), 0))

    bones = {
        "Body": ("Root", (0, 0.25, 1.18), (0, -0.25, 1.18)),
        "Head": ("Body", (0, -0.25, 1.18), (0, -0.60, 1.20)),
        "Tail": ("Body", (0, 0.28, 1.10), (0, 0.95, 0.65)),
        "WingL": ("Body", (-0.15, 0.10, 1.32), (-1.05, 0.10, 1.36)),
        "WingR": ("Body", (0.15, 0.10, 1.32), (1.05, 0.10, 1.36)),
        "LegL": ("Body", (-0.24, 0.00, 1.00), (-0.30, 0.10, 0.70)),
        "LegR": ("Body", (0.24, 0.00, 1.00), (0.30, 0.10, 0.70)),
    }

    def buzz(key_rot, frames_end):
        # Vrombissement : les ailes battent en opposition toutes les 2 frames.
        up = math.radians(35)
        for i, f in enumerate(range(1, frames_end + 1, 2)):
            a = up if i % 2 == 0 else -up
            key_rot("WingL", f, (0, a, 0))
            key_rot("WingR", f, (0, -a, 0))
        key_rot("WingL", frames_end, (0, up, 0))
        key_rot("WingR", frames_end, (0, -up, 0))

    def idle(key_rot, key_loc):
        # Stationnaire : bourdonnement continu, corps qui flotte, dard qui arme.
        buzz(key_rot, 40)
        for f in (1, 40):
            key_rot("LegL", f, (0, 0, 0))
            key_rot("LegR", f, (0, 0, 0))
        for f, dz in ((1, 0.0), (10, 0.08), (20, 0.0), (30, 0.08), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, curl in ((1, 0.0), (16, 0.30), (24, 0.30), (32, 0.0), (40, 0.0)):
            key_rot("Tail", f, (-curl, 0, 0))
        for f, yaw in ((1, 0.0), (12, 0.20), (28, -0.20), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        # Vol de croisière : penché vers l'avant, pattes ramenées, cap stable.
        buzz(key_rot, 24)
        for f, pitch in ((1, 0.14), (13, 0.18), (24, 0.14)):
            key_rot("Body", f, (pitch, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, tuck in ((1, 0.25), (13, 0.30), (24, 0.25)):
            key_rot("LegL", f, (tuck, 0, 0))
            key_rot("LegR", f, (tuck, 0, 0))
        for f, tr in ((1, 0.10), (13, -0.10), (24, 0.10)):
            key_rot("Tail", f, (tr * 0.3, 0, tr))
        for f in (1, 24):
            key_rot("Head", f, (0, 0, 0))

    build_creature("creature30", bones, idle, walk)


# =============================================================================
# Créature 31 — Mille-pattes : corps segmenté qui ondule, forêt de pattes.
# =============================================================================
def millepattes():
    fresh_scene()
    seg_m = material("Millepattes31Seg", (0.45, 0.20, 0.08))
    seg_d = material("Millepattes31SegD", (0.28, 0.12, 0.05))
    head_m = material("Millepattes31Head", (0.60, 0.16, 0.08))
    leg_m = material("Millepattes31Leg", (0.75, 0.50, 0.20))
    dark = material("Millepattes31Dark", (0.08, 0.05, 0.04))

    # Tête (os Head) : plaque ronde, antennes longues, forcipules (crochets à
    # venin caractéristiques).
    sphere("Head", head_m, (0, -1.05, 0.30), (0.26, 0.26, 0.20))
    sphere("Head", dark, (-0.13, -1.25, 0.36), (0.045, 0.04, 0.045))
    sphere("Head", dark, (0.13, -1.25, 0.36), (0.045, 0.04, 0.045))
    for sx in (-1, 1):
        cylinder("Head", leg_m, (sx * 0.14, -1.35, 0.46), (0.025, 0.025, 0.55),
                 rotation=(math.radians(65), 0, math.radians(-sx * 28)))  # antenne
        cone("Head", dark, (sx * 0.10, -1.30, 0.20), (0.04, 0.04, 0.11),
             rotation=(math.radians(150), 0, math.radians(sx * 18)))  # forcipule
    # Corps en 3 tronçons de 2 segments chacun (os Body/Mid/Tail) : l'ondulation
    # vient du chaînage des os. Segments alternés clair/sombre + pattes par paire.
    troncons = (("Body", (-0.70, -0.30)), ("Mid", (0.10, 0.50)), ("Tail", (0.90, 1.30)))
    for bi, (bone, ys) in enumerate(troncons):
        for si, y in enumerate(ys):
            mat = seg_m if (bi * 2 + si) % 2 == 0 else seg_d
            s = 0.26 - bi * 0.025
            sphere(bone, mat, (0, y, 0.28), (s, 0.26, s * 0.85))
            for sx in (-1, 1):  # une paire de pattes par segment
                cylinder(bone, leg_m, (sx * (s + 0.10), y, 0.16),
                         (0.03, 0.03, 0.30), rotation=(0, math.radians(sx * 42), 0))
    # Deux cerques arrière (fausses antennes de queue).
    for sx in (-1, 1):
        cylinder("Tail", leg_m, (sx * 0.10, 1.55, 0.30), (0.025, 0.025, 0.40),
                 rotation=(math.radians(-70), 0, math.radians(sx * 20)))

    bones = {
        "Body": ("Root", (0, -0.05, 0.28), (0, -0.85, 0.28)),
        "Head": ("Body", (0, -0.85, 0.28), (0, -1.35, 0.28)),
        "Mid": ("Body", (0, -0.05, 0.28), (0, 0.70, 0.28)),
        "Tail": ("Mid", (0, 0.70, 0.28), (0, 1.60, 0.28)),
    }

    def idle(key_rot, key_loc):
        # Repos vigilant : antennes qui fouettent (tête), légère respiration,
        # bout de queue qui se recourbe.
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))
        for f, yaw in ((1, 0.0), (8, 0.30), (16, -0.25), (24, 0.30), (32, -0.15), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))
        for f, breathe in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_rot("Body", f, (breathe, 0, 0))
        for f, sw in ((1, 0.0), (14, 0.12), (28, -0.12), (40, 0.0)):
            key_rot("Mid", f, (0, 0, sw))
        for f, curl in ((1, 0.0), (14, -0.20), (28, 0.15), (40, 0.0)):
            key_rot("Tail", f, (curl * 0.4, 0, curl))

    def walk(key_rot, key_loc):
        # Reptation ondulante : vague latérale déphasée le long de la chaîne
        # Body→Mid→Tail, tête qui balaie en sens inverse pour garder le cap.
        wave = 0.28
        for f, s in ((1, wave), (13, -wave), (24, wave)):
            key_rot("Body", f, (0, 0, s * 0.4))
            key_rot("Mid", f, (0, 0, -s))
            key_rot("Tail", f, (0, 0, s))
            key_rot("Head", f, (0, 0, -s * 0.5))
        for f, dz in ((1, 0.0), (7, 0.02), (13, 0.0), (19, 0.02), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature("creature31", bones, idle, walk, cam=0.85)


araignee()
mante()
scarabee()
frelon()
millepattes()
print("PACK DONE")
