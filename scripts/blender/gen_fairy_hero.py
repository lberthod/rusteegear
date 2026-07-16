"""Génère assets/models/fairy_hero.glb : héros elfe des bois féérique.

Personnage humanoïde bipède (pas une créature de la faune) : silhouette
humaine, oreilles pointues, tunique verte/brune, épée + bouclier + cape en
équipement permanent (façon héros de fantasy classique). Même conventions
que les créatures MMORPG (`gen_creature.py`, `gen_creature2.py`) :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à
  ry=0) ;
- rig articulé Root/Hips/Spine/Chest/Head + bras 3 segments (Shoulder→
  UpperArm→Forearm→Hand) + jambes 3 segments (Thigh→Shin→Foot) + 3 os de
  mouvement secondaire (Weapon.R, Shield.L, Cape, enfants respectivement de
  Hand.R/Forearm.L/Chest) ; mesh unique skinné, un os = un groupe de vertex
  (skinning rigide, poids 1.0, jamais de blend multi-os) ;
- clips « Idle » (40 f), « Walk » (24 f), « Jump » (40 f), « AttackFire »
  (30 f), « AttackShoot » (20 f), « AttackSpell » (40 f) à 24 fps — chaque
  animation (attaque ou saut) engage tout le corps (bassin, torse, jambes,
  tête), pas seulement les bras, avec un peu de mouvement secondaire
  (retard/dépassement) sur l'épée, le bouclier et la cape ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import
  moteur au 16 juillet 2026 — le metallic/roughness est renseigné pour un
  meilleur rendu Blender mais n'est pas garanti d'être exploité par le
  moteur).
"""

import math

import bpy
from mathutils import Vector

OUT = "/Users/berthod/Desktop/motor3derust/assets/models/fairy_hero.glb"

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.fps = 24


def material(name, rgb, emission=0.0, roughness=0.75, metallic=0.0,
             clearcoat=0.0, sheen=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    for key in ("Coat Weight", "Clearcoat"):
        if key in bsdf.inputs:
            bsdf.inputs[key].default_value = clearcoat
            break
    for key in ("Sheen Weight", "Sheen"):
        if key in bsdf.inputs:
            bsdf.inputs[key].default_value = sheen
            break
    if emission > 0.0:
        bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emission
    return m


MAT_SKIN = material("FairySkin", (0.87, 0.70, 0.55), roughness=0.55)
MAT_TUNIC = material("FairyTunic", (0.16, 0.42, 0.18), roughness=0.65, sheen=0.15)  # vert forêt
MAT_LEATHER = material("FairyLeather", (0.34, 0.22, 0.12), roughness=0.55)  # ceinture/gants
MAT_HAIR = material("FairyHair", (0.80, 0.62, 0.28), roughness=0.35, sheen=0.3)  # blond
MAT_HOOD = material("FairyHood", (0.12, 0.33, 0.15), roughness=0.6, sheen=0.15)  # bonnet pointu
MAT_DARK = material("FairyDark", (0.08, 0.06, 0.05), roughness=0.25)  # yeux/sourcils
MAT_FIRE = material("FairyFireOrb", (1.0, 0.45, 0.05), emission=4.5, roughness=0.2)
MAT_GLOVE = material("FairyGlove", (0.32, 0.23, 0.40), roughness=0.45)  # gantelet mauve
MAT_BOOTCUFF = material("FairyBootCuff", (0.83, 0.73, 0.55), roughness=0.6)  # revers de botte
MAT_METAL = material("FairyMetal", (0.75, 0.77, 0.79), roughness=0.22, metallic=1.0)
MAT_SHIELD = material("FairyShieldFace", (0.14, 0.28, 0.55), roughness=0.35, clearcoat=0.4)
MAT_GOLD = material("FairyGold", (0.83, 0.66, 0.20), roughness=0.25, metallic=1.0)
MAT_CAPE = material("FairyCape", (0.10, 0.22, 0.12), roughness=0.7, sheen=0.1)  # cape vert sombre

PARTS = []  # objets déjà assignés à un groupe de vertex / os


def add_part(bone, mat, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    create_op(location=location, rotation=rotation)
    ob = bpy.context.active_object
    ob.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    ob.data.materials.append(mat)
    vg = ob.vertex_groups.new(name=bone)
    vg.add(range(len(ob.data.vertices)), 1.0, "REPLACE")
    PARTS.append(ob)
    return ob


def sphere(bone, mat, location, scale, segments=20, rings=14):
    def op(location, rotation):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=segments, ring_count=rings, radius=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale)


def cone(bone, mat, location, scale, rotation=(0, 0, 0), radius2=0.0):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cone_add(
            vertices=14, radius1=1.0, radius2=radius2, depth=2.0,
            location=location, rotation=rotation,
        )

    return add_part(bone, mat, op, location, scale, rotation)


def cylinder(bone, mat, location, scale, rotation=(0, 0, 0)):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=14, radius=1.0, depth=1.0, location=location, rotation=rotation
        )

    return add_part(bone, mat, op, location, scale, rotation)


# --- Corps (avant = -Y) ------------------------------------------------------
# Bassin / hanches
cylinder("Hips", MAT_TUNIC, (0, 0, 0.95), (0.24, 0.20, 0.34))
# Torse (tunique, jupe qui rejoint les cuisses sans espace)
cone("Spine", MAT_TUNIC, (0, 0, 1.16), (0.26, 0.20, 0.36), radius2=0.85)
cylinder("Spine", MAT_LEATHER, (0, 0, 0.98), (0.27, 0.21, 0.05))  # ceinture
sphere("Spine", MAT_GOLD, (0, -0.205, 0.98), (0.035, 0.02, 0.03))  # boucle de ceinture
# Poitrine/épaules
sphere("Chest", MAT_TUNIC, (0, 0, 1.48), (0.24, 0.19, 0.20))

# Cape courte dans le dos (+Y) — skinnée à un os dédié « Cape » (enfant de
# Chest) pour pouvoir lui donner un mouvement secondaire (retard/oscillation)
# distinct du torse dans les animations, plutôt qu'un simple objet rigide.
# Décalée nettement derrière le dos de la tunique (dont la coque arrière
# atteint y≈0.19-0.20 au niveau des épaules) pour ne jamais pénétrer dedans
# — un chevauchement aurait créé un trou visuel vu de dos (le rendu montrait
# alors la face avant de la cape, non éclairée, à travers la tunique).
cone("Cape", MAT_CAPE, (0, 0.33, 1.24), (0.18, 0.11, 0.32),
     rotation=(math.radians(180), 0, 0))

# Tête + oreilles pointues + cheveux blonds + bonnet + yeux
sphere("Head", MAT_SKIN, (0, 0, 1.78), (0.17, 0.16, 0.19), segments=24, rings=18)
sphere("Head", MAT_HAIR, (0, 0.02, 1.885), (0.178, 0.168, 0.115))  # cheveux blonds
cone("Head", MAT_HOOD, (0, 0.05, 1.92), (0.155, 0.145, 0.30),
     rotation=(math.radians(18), 0, 0))  # bonnet pointu vert (façon capuche elfique)
cone("Head", MAT_SKIN, (-0.20, 0.02, 1.82), (0.045, 0.045, 0.14),
     rotation=(math.radians(-15), math.radians(-100), 0))  # oreille pointue G
cone("Head", MAT_SKIN, (0.20, 0.02, 1.82), (0.045, 0.045, 0.14),
     rotation=(math.radians(-15), math.radians(100), 0))  # oreille pointue D
sphere("Head", MAT_DARK, (-0.06, -0.145, 1.80), (0.022, 0.018, 0.018))  # œil G
sphere("Head", MAT_DARK, (0.06, -0.145, 1.80), (0.022, 0.018, 0.018))  # œil D
cone("Head", MAT_SKIN, (0, -0.165, 1.775), (0.02, 0.045, 0.022),
     rotation=(math.radians(80), 0, 0))  # petit nez
cone("Head", MAT_HAIR, (0, -0.155, 1.855), (0.06, 0.02, 0.09),
     rotation=(math.radians(96), 0, 0))  # mèche blonde qui dépasse du bonnet

# Bras (Shoulder → UpperArm → Forearm → Hand), bras au repos le long du corps.
# Manche de tunique sur le haut du bras, avant-bras nu jusqu'au gantelet en
# cuir, comme un héros classique (cf. référence). Chaque segment chevauche
# généreusement le suivant (pas de vide entre pièces).
for side, sx in (("L", -1), ("R", 1)):
    sphere(f"Shoulder.{side}", MAT_TUNIC, (sx * 0.26, 0, 1.50), (0.10, 0.10, 0.10))
    cylinder(f"UpperArm.{side}", MAT_TUNIC, (sx * 0.30, 0, 1.32), (0.06, 0.06, 0.30))  # manche
    cylinder(f"Forearm.{side}", MAT_SKIN, (sx * 0.30, 0, 1.06), (0.048, 0.048, 0.34))
    cylinder(f"Forearm.{side}", MAT_LEATHER, (sx * 0.30, 0, 0.945), (0.052, 0.052, 0.03))  # revers gantelet
    sphere(f"Hand.{side}", MAT_GLOVE, (sx * 0.30, 0, 0.88), (0.055, 0.05, 0.09))

# Épée dans la main droite (hilt / garde / lame / pommeau) — skinnée à un os
# dédié « Weapon.R » (enfant de Hand.R) plutôt qu'à Hand.R directement, pour
# pouvoir lui appliquer un léger mouvement secondaire (retard/fouetté de la
# lame) en plus du mouvement du bras dans les animations d'attaque.
cylinder("Weapon.R", MAT_LEATHER, (0.30, 0, 0.80), (0.022, 0.022, 0.075))  # poignée
cylinder("Weapon.R", MAT_METAL, (0.30, 0, 0.815), (0.014, 0.014, 0.13),
         rotation=(0, math.radians(90), 0))  # garde
sphere("Weapon.R", MAT_GOLD, (0.30, 0, 0.735), (0.02, 0.02, 0.02))  # pommeau
cone("Weapon.R", MAT_METAL, (0.30, 0, 0.53), (0.032, 0.018, 0.33),
     rotation=(math.radians(180), 0, 0))  # lame (pointe vers le bas au repos)

# Bouclier à l'avant-bras gauche — skinné à un os dédié « Shield.L » (enfant
# de Forearm.L) pour un léger retard/oscillation propre au bouclier.
cylinder("Shield.L", MAT_METAL, (-0.30, -0.15, 1.05), (0.19, 0.19, 0.035),
         rotation=(math.radians(90), 0, 0))  # rebord métallique
cylinder("Shield.L", MAT_SHIELD, (-0.30, -0.165, 1.05), (0.16, 0.16, 0.025),
         rotation=(math.radians(90), 0, 0))  # face du bouclier
sphere("Shield.L", MAT_GOLD, (-0.30, -0.19, 1.05), (0.045, 0.045, 0.02))  # emblème central

# Petit orbe de feu, caché dans la main droite (échelle animée à 0 hors AttackFire)
sphere("Prop.R", MAT_FIRE, (0.30, -0.02, 0.88), (0.08, 0.08, 0.08), segments=12, rings=8)

# Jambes (Thigh → Shin → Foot), même principe de chevauchement.
for side, sx in (("L", -1), ("R", 1)):
    cylinder(f"Thigh.{side}", MAT_TUNIC, (sx * 0.12, 0, 0.60), (0.09, 0.09, 0.40))
    cylinder(f"Shin.{side}", MAT_SKIN, (sx * 0.12, 0, 0.25), (0.07, 0.07, 0.38))
    cylinder(f"Shin.{side}", MAT_BOOTCUFF, (sx * 0.12, -0.02, 0.11), (0.078, 0.10, 0.035))  # revers de botte
    cylinder(f"Foot.{side}", MAT_LEATHER, (sx * 0.12, -0.06, 0.05), (0.075, 0.14, 0.09))

# --- Fusion en un seul mesh ---------------------------------------------------
bpy.ops.object.select_all(action="DESELECT")
for ob in PARTS:
    ob.select_set(True)
bpy.context.view_layer.objects.active = PARTS[0]
bpy.ops.object.join()
hero = bpy.context.active_object
hero.name = "FairyHero"

# Lissage des normales : les surfaces rondes (têtes, membres) paraissent
# lisses au lieu de facettées, sans ajouter de géométrie.
# NB : `shade_auto_smooth` (lissage par seuil d'angle) a été essayé en
# premier mais produit une normale de coin corrompue sur le cône du torse
# (face noire non éclairée visible uniquement de dos, sur la couture
# Spine/Chest) — reproductible à tout angle testé (10°/20°/30°/42°), donc
# pas un simple réglage de seuil. `shade_smooth()` plein (sans seuil
# d'angle) lisse tout, y compris les arêtes franches (lame, bonnet), mais
# reste correct visuellement et n'a pas ce défaut : préféré ici.
bpy.ops.object.shade_smooth()

# --- Armature -----------------------------------------------------------------
bpy.ops.object.armature_add(location=(0, 0, 0))
arm = bpy.context.active_object
arm.name = "FairyHeroRig"
bpy.ops.object.mode_set(mode="EDIT")
eb = arm.data.edit_bones
root = eb[0]
root.name = "Root"
root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.30))

BONES = {
    "Hips": ("Root", (0, 0, 0.95), (0, 0, 1.05)),
    "Spine": ("Hips", (0, 0, 1.05), (0, 0, 1.40)),
    "Chest": ("Spine", (0, 0, 1.40), (0, 0, 1.55)),
    "Head": ("Chest", (0, 0, 1.55), (0, 0, 1.95)),
    "Thigh.L": ("Hips", (-0.12, 0, 0.95), (-0.12, 0, 0.46)),
    "Shin.L": ("Thigh.L", (-0.12, 0, 0.46), (-0.12, 0, 0.10)),
    "Foot.L": ("Shin.L", (-0.12, 0, 0.10), (-0.12, -0.14, 0.02)),
    "Thigh.R": ("Hips", (0.12, 0, 0.95), (0.12, 0, 0.46)),
    "Shin.R": ("Thigh.R", (0.12, 0, 0.46), (0.12, 0, 0.10)),
    "Foot.R": ("Shin.R", (0.12, 0, 0.10), (0.12, -0.14, 0.02)),
    "Shoulder.L": ("Chest", (-0.10, 0, 1.52), (-0.26, 0, 1.52)),
    "UpperArm.L": ("Shoulder.L", (-0.30, 0, 1.44), (-0.30, 0, 1.22)),
    "Forearm.L": ("UpperArm.L", (-0.30, 0, 1.22), (-0.30, 0, 0.95)),
    "Hand.L": ("Forearm.L", (-0.30, 0, 0.95), (-0.30, 0, 0.78)),
    "Shoulder.R": ("Chest", (0.10, 0, 1.52), (0.26, 0, 1.52)),
    "UpperArm.R": ("Shoulder.R", (0.30, 0, 1.44), (0.30, 0, 1.22)),
    "Forearm.R": ("UpperArm.R", (0.30, 0, 1.22), (0.30, 0, 0.95)),
    "Hand.R": ("Forearm.R", (0.30, 0, 0.95), (0.30, 0, 0.78)),
    "Prop.R": ("Hand.R", (0.30, -0.02, 0.86), (0.30, -0.30, 0.86)),
    "Weapon.R": ("Hand.R", (0.30, 0, 0.78), (0.30, 0, 0.50)),
    "Shield.L": ("Forearm.L", (-0.30, 0, 1.06), (-0.30, -0.19, 1.06)),
    "Cape": ("Chest", (0, 0.20, 1.52), (0, 0.38, 0.96)),
}
for name, (parent, head, tail) in BONES.items():
    b = eb.new(name)
    b.head, b.tail = Vector(head), Vector(tail)
    b.parent = eb[parent]
bpy.ops.object.mode_set(mode="OBJECT")

# Skinning : groupes de vertex déjà posés (1 os / partie), l'armature suffit.
hero.parent = arm
mod = hero.modifiers.new("Armature", "ARMATURE")
mod.object = arm

# --- Animations ---------------------------------------------------------------
bpy.ops.object.select_all(action="DESELECT")
arm.select_set(True)
bpy.context.view_layer.objects.active = arm
bpy.ops.object.mode_set(mode="POSE")
for pb in arm.pose.bones:
    pb.rotation_mode = "XYZ"

# Tous les os avec un canal de rotation potentiel — chaque clip doit tous les
# keyframer (au minimum en neutre) pour ne jamais laisser un canal figé au
# changement de clip côté moteur (piège appris sur les créatures MMORPG).
ROT_BONES = [
    "Hips", "Spine", "Chest", "Head",
    "Thigh.L", "Shin.L", "Foot.L", "Thigh.R", "Shin.R", "Foot.R",
    "Shoulder.L", "UpperArm.L", "Forearm.L", "Hand.L",
    "Shoulder.R", "UpperArm.R", "Forearm.R", "Hand.R",
    "Weapon.R", "Shield.L", "Cape",
]
# Os de mouvement secondaire (accessoires) : suivent leur parent rigidement
# via le rig, mais reçoivent en plus une légère rotation locale de retard/
# oscillation ("overlapping action") pour ne pas paraître soudés au bras.
SECONDARY_BONES = ("Weapon.R", "Shield.L", "Cape")


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


def rots(bone, keys):
    """keys : liste de (frame, (x_deg, y_deg, z_deg))."""
    for f, (x, y, z) in keys:
        key_rot(bone, f, (math.radians(x), math.radians(y), math.radians(z)))


def neutral_rot(bones, frames):
    for b in bones:
        for f in frames:
            key_rot(b, f, (0, 0, 0))


def hide_prop(frames):
    for f in frames:
        key_scale("Prop.R", f, (0.001, 0.001, 0.001))
        key_loc("Prop.R", f, (0, 0, 0))


def bake_clip(name, length, keyer):
    """Crée l'action `name` (keyframes via `keyer`) et la pousse en piste NLA."""
    ad = arm.animation_data_create()
    ad.action = None
    keyer()
    act = ad.action
    act.name = name
    track = ad.nla_tracks.new()
    track.name = name
    strip = track.strips.new(name, 1, act)
    strip.name = name
    ad.action = None
    return act


LEG_BONES = ("Thigh.L", "Shin.L", "Foot.L", "Thigh.R", "Shin.R", "Foot.R")
ARM_ONLY_BONES = ("Shoulder.L", "Hand.L", "Shoulder.R", "Hand.R")  # peu animés


def idle_keys():
    f = (1, 20, 40)
    key_loc("Hips", 1, (0, 0, 0))
    key_loc("Hips", 20, (0.006, 0, 0.01))  # léger transfert de poids latéral
    key_loc("Hips", 40, (0, 0, 0))
    rots("Hips", [(1, (0, 0, 0)), (20, (0, 0, 1.5)), (40, (0, 0, 0))])
    neutral_rot(LEG_BONES, f)
    for fr, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
        key_loc("Spine", fr, (0, 0, dz))
    for fr, sway in ((1, 0.05), (20, -0.03), (40, 0.05)):
        key_rot("UpperArm.L", fr, (0, 0, -sway))
        key_rot("UpperArm.R", fr, (0, 0, sway))
    neutral_rot(["Forearm.L", "Forearm.R", "Hand.L", "Hand.R",
                 "Shoulder.L", "Shoulder.R"], f)
    for fr, nod in ((1, 0.0), (20, 0.04), (40, 0.0)):
        key_rot("Head", fr, (nod, 0, 0))
    neutral_rot(["Chest", "Spine"], f)
    # Mouvement secondaire subtil : la cape et le bouclier suivent le buste
    # avec un léger décalage, l'épée reste quasi immobile (tenue fermement).
    rots("Cape", [(1, (2, 0, 0)), (20, (-3, 0, 1)), (40, (2, 0, 0))])
    rots("Shield.L", [(1, (0, 0, 0)), (20, (0, 0, -2)), (40, (0, 0, 0))])
    neutral_rot(["Weapon.R"], f)
    hide_prop(f)


def walk_keys():
    f3 = (1, 13, 24)
    f5 = (1, 7, 13, 19, 24)
    swing = math.radians(30)
    arm_swing = math.radians(26)
    for fr, s in ((1, swing), (13, -swing), (24, swing)):
        key_rot("Thigh.L", fr, (s, 0, 0))
        key_rot("Thigh.R", fr, (-s, 0, 0))
    for fr, s in ((1, -swing * 0.6), (13, swing * 0.6), (24, -swing * 0.6)):
        key_rot("Shin.L", fr, (s if s > 0 else 0, 0, 0))
        key_rot("Shin.R", fr, (-s if s < 0 else 0, 0, 0))
    # Déroulé du pied (talon → pointe) : le pied avant pointe un peu vers le
    # haut à l'attaque du talon, celui qui repousse s'incline vers le bas.
    for fr, foot_l, foot_r in (
        (1, 10, -14), (7, -8, 4), (13, -14, 10), (19, 4, -8), (24, 10, -14),
    ):
        key_rot("Foot.L", fr, (math.radians(foot_l), 0, 0))
        key_rot("Foot.R", fr, (math.radians(foot_r), 0, 0))
    for fr, s in ((1, -arm_swing), (13, arm_swing), (24, -arm_swing)):
        key_rot("UpperArm.L", fr, (s, 0, 0))
        key_rot("UpperArm.R", fr, (-s, 0, 0))
    # Léger contre-mouvement des épaules à l'opposé du bassin (balancier
    # naturel du torse pendant la marche).
    for fr, tw in ((1, -3), (13, 3), (24, -3)):
        key_rot("Shoulder.L", fr, (0, 0, math.radians(tw)))
        key_rot("Shoulder.R", fr, (0, 0, math.radians(tw)))
    neutral_rot(["Forearm.L", "Forearm.R", "Hand.L", "Hand.R"], f3)
    for fr, dz in ((1, 0.0), (7, 0.035), (13, 0.0), (19, 0.035), (24, 0.0)):
        key_loc("Spine", fr, (0, 0, dz))
    for fr, twist in ((1, 5), (13, -5), (24, 5)):
        key_rot("Hips", fr, (0, 0, math.radians(twist)))
    for fr, dz in ((1, 0.0), (7, -0.012), (13, 0.0), (19, -0.012), (24, 0.0)):
        key_loc("Hips", fr, (0, 0, dz))
    for fr, nod in ((1, 0.02), (13, -0.02), (24, 0.02)):
        key_rot("Head", fr, (nod, 0, 0))
    neutral_rot(["Chest"], f3)
    # La cape et le bouclier accusent un léger retard sur le balancier du
    # bassin/torse ; l'épée, tenue fermement, ne fait que suivre la main.
    for fr, sway in ((1, -6), (7, 3), (13, 6), (19, -3), (24, -6)):
        key_rot("Cape", fr, (0, 0, math.radians(sway)))
    for fr, sway in ((1, 2), (13, -2), (24, 2)):
        key_rot("Shield.L", fr, (0, 0, math.radians(sway)))
    neutral_rot(["Weapon.R"], f3)
    hide_prop(f3)


def attack_fire_keys():
    # Séquence pleine implication du corps : appel (recul + flexion), charge
    # (bassin vrillé, genoux fléchis), lâcher (vrille inverse + extension des
    # jambes qui pousse le tir en avant), léger dépassement (follow-through)
    # puis retour au calme.
    f0, f_wind, f_peak, f_throw, f_over, f_end = 1, 8, 14, 20, 25, 30
    FR = (f0, f_wind, f_peak, f_throw, f_end)
    FR_OVER = (f0, f_wind, f_peak, f_throw, f_over, f_end)

    rots("Hips", [
        (f0, (0, 0, 0)), (f_wind, (5, 0, 11)), (f_peak, (7, 0, 15)),
        (f_throw, (-9, 0, -17)), (f_end, (0, 0, 0)),
    ])
    for fr, dz in ((f0, 0.0), (f_wind, -0.03), (f_peak, -0.025), (f_throw, 0.02), (f_end, 0.0)):
        key_loc("Hips", fr, (0, 0, dz))

    rots("Spine", [
        (f0, (0, 0, 0)), (f_wind, (4, 0, -6)), (f_peak, (-6, 0, -9)),
        (f_throw, (14, 0, 11)), (f_over, (10, 0, 13)), (f_end, (0, 0, 0)),
    ])
    rots("Chest", [
        (f0, (0, 0, 0)), (f_wind, (2, 0, -4)), (f_peak, (-4, 0, -6)),
        (f_throw, (8, 0, 7)), (f_over, (5, 0, 9)), (f_end, (0, 0, 0)),
    ])
    rots("Head", [
        (f0, (0, 0, 0)), (f_wind, (6, 0, 0)), (f_peak, (4, 0, 0)),
        (f_throw, (-10, 0, 0)), (f_end, (0, 0, 0)),
    ])

    for side in ("L", "R"):
        rots(f"Thigh.{side}", [
            (f0, (0, 0, 0)), (f_wind, (20, 0, 0)), (f_peak, (17, 0, 0)),
            (f_throw, (-12, 0, 0)), (f_end, (0, 0, 0)),
        ])
        rots(f"Shin.{side}", [
            (f0, (0, 0, 0)), (f_wind, (-36, 0, 0)), (f_peak, (-32, 0, 0)),
            (f_throw, (16, 0, 0)), (f_end, (0, 0, 0)),
        ])
        rots(f"Foot.{side}", [
            (f0, (0, 0, 0)), (f_wind, (7, 0, 0)), (f_peak, (7, 0, 0)),
            (f_throw, (-9, 0, 0)), (f_end, (0, 0, 0)),
        ])

    rots("UpperArm.R", [
        (f0, (-30, 0, 10)), (f_wind, (-12, 0, 32)), (f_peak, (-70, 0, 20)),
        (f_throw, (-18, 0, -42)), (f_over, (-24, 0, -50)), (f_end, (-30, 0, 10)),
    ])
    rots("Forearm.R", [
        (f0, (-20, 0, 0)), (f_wind, (-70, 0, 0)), (f_peak, (-80, 0, 0)),
        (f_throw, (-8, 0, 0)), (f_over, (-4, 0, 0)), (f_end, (-20, 0, 0)),
    ])
    rots("UpperArm.L", [
        (f0, (-30, 0, -10)), (f_wind, (-12, 0, -30)), (f_peak, (-65, 0, -15)),
        (f_throw, (-35, 0, 0)), (f_over, (-40, 0, 4)), (f_end, (-30, 0, -10)),
    ])
    rots("Forearm.L", [
        (f0, (-20, 0, 0)), (f_wind, (-65, 0, 0)), (f_peak, (-75, 0, 0)),
        (f_throw, (-25, 0, 0)), (f_over, (-20, 0, 0)), (f_end, (-20, 0, 0)),
    ])
    neutral_rot(["Hand.L", "Hand.R", "Shoulder.L", "Shoulder.R"], FR)

    # Épée/bouclier/cape : léger retard puis dépassement (overlapping action)
    # par rapport au mouvement du bras/torse qui les porte.
    rots("Weapon.R", [
        (f0, (0, 0, 0)), (f_wind, (3, 0, -6)), (f_peak, (-4, 0, -4)),
        (f_throw, (10, 0, 14)), (f_over, (-8, 0, -6)), (f_end, (0, 0, 0)),
    ])
    rots("Shield.L", [
        (f0, (0, 0, 0)), (f_wind, (-3, 0, 5)), (f_peak, (2, 0, 4)),
        (f_throw, (-6, 0, -8)), (f_over, (5, 0, 6)), (f_end, (0, 0, 0)),
    ])
    rots("Cape", [
        (f0, (0, 0, 0)), (f_wind, (5, 0, -4)), (f_peak, (-8, 0, -6)),
        (f_throw, (12, 0, 10)), (f_over, (-6, 0, -8)), (f_end, (0, 0, 0)),
    ])

    # Orbe : caché, grossit dans la main pendant la charge, part en avant au tir.
    key_scale("Prop.R", f0, (0.001, 0.001, 0.001))
    key_loc("Prop.R", f0, (0, 0, 0))
    key_scale("Prop.R", f_wind, (0.3, 0.3, 0.3))
    key_loc("Prop.R", f_wind, (0, 0, 0))
    key_scale("Prop.R", f_peak, (1.0, 1.0, 1.0))
    key_loc("Prop.R", f_peak, (0, 0, 0))
    key_scale("Prop.R", f_throw, (1.0, 1.0, 1.0))
    key_loc("Prop.R", f_throw, (0, -1.4, 0.05))
    key_scale("Prop.R", f_end, (0.001, 0.001, 0.001))
    key_loc("Prop.R", f_end, (0, 0, 0))


def attack_shoot_keys():
    # Geste bref mais engageant tout le corps : petite torsion du bassin et
    # avancée du poids au moment du tir, retour rapide à neutre.
    f_wind, f_shoot, f_end = 1, 10, 20
    FR = (f_wind, f_shoot, f_end)

    rots("Hips", [(f_wind, (3, 0, 7)), (f_shoot, (-4, 0, -9)), (f_end, (0, 0, 0))])
    for fr, dz in ((f_wind, -0.015), (f_shoot, 0.01), (f_end, 0.0)):
        key_loc("Hips", fr, (0, 0, dz))
    for side in ("L", "R"):
        rots(f"Thigh.{side}", [(f_wind, (8, 0, 0)), (f_shoot, (-6, 0, 0)), (f_end, (0, 0, 0))])
        rots(f"Shin.{side}", [(f_wind, (-14, 0, 0)), (f_shoot, (8, 0, 0)), (f_end, (0, 0, 0))])
        rots(f"Foot.{side}", [(f_wind, (0, 0, 0)), (f_shoot, (0, 0, 0)), (f_end, (0, 0, 0))])

    rots("UpperArm.R", [(f_wind, (-30, 0, 15)), (f_shoot, (-85, 0, 0)), (f_end, (0, 0, 0))])
    rots("Forearm.R", [(f_wind, (-60, 0, 0)), (f_shoot, (-5, 0, 0)), (f_end, (0, 0, 0))])
    rots("UpperArm.L", [(f_wind, (-15, 0, -10)), (f_shoot, (-25, 0, -5)), (f_end, (0, 0, 0))])
    rots("Forearm.L", [(f_wind, (0, 0, 0)), (f_shoot, (0, 0, 0)), (f_end, (0, 0, 0))])
    neutral_rot(["Hand.L", "Hand.R", "Shoulder.L", "Shoulder.R"], FR)

    rots("Spine", [(f_wind, (-4, 0, 6)), (f_shoot, (6, 0, -4)), (f_end, (0, 0, 0))])
    rots("Chest", [(f_wind, (-2, 0, 3)), (f_shoot, (4, 0, -3)), (f_end, (0, 0, 0))])
    rots("Head", [(f_wind, (2, 0, 0)), (f_shoot, (-6, 0, 0)), (f_end, (0, 0, 0))])

    # Léger dépassement de la lame/du bouclier/de la cape au moment du tir.
    rots("Weapon.R", [(f_wind, (2, 0, -8)), (f_shoot, (-10, 0, 16)), (f_end, (0, 0, 0))])
    rots("Shield.L", [(f_wind, (-2, 0, 3)), (f_shoot, (4, 0, -6)), (f_end, (0, 0, 0))])
    rots("Cape", [(f_wind, (3, 0, -5)), (f_shoot, (-6, 0, 9)), (f_end, (0, 0, 0))])
    hide_prop(FR)


def attack_spell_keys():
    # Incantation à deux mains : montée sur l'énergie (bassin qui s'élève,
    # buste qui s'arque), tenue au-dessus de la tête, puis retombée en un
    # léger impact (flexion des genoux) qui vend le poids du sort.
    f_start, f_rise, f_hold, f_fall = 1, 14, 26, 40
    FR = (f_start, f_rise, f_hold, f_fall)

    rots("Hips", [(f_start, (0, 0, 0)), (f_rise, (-6, 0, 0)),
                  (f_hold, (-8, 0, 0)), (f_fall, (10, 0, 0))])
    for fr, dz in ((f_start, 0.0), (f_rise, 0.03), (f_hold, 0.02), (f_fall, -0.035)):
        key_loc("Hips", fr, (0, 0, dz))
    for side in ("L", "R"):
        rots(f"Thigh.{side}", [(f_start, (0, 0, 0)), (f_rise, (-6, 0, 0)),
                                (f_hold, (-6, 0, 0)), (f_fall, (22, 0, 0))])
        rots(f"Shin.{side}", [(f_start, (0, 0, 0)), (f_rise, (4, 0, 0)),
                               (f_hold, (4, 0, 0)), (f_fall, (-40, 0, 0))])
        rots(f"Foot.{side}", [(f_start, (0, 0, 0)), (f_rise, (10, 0, 0)),
                               (f_hold, (10, 0, 0)), (f_fall, (-4, 0, 0))])

    rots("UpperArm.L", [(f_start, (0, 0, 0)), (f_rise, (-150, 0, -20)),
                         (f_hold, (-160, 0, -20)), (f_fall, (0, 0, 0))])
    rots("UpperArm.R", [(f_start, (0, 0, 0)), (f_rise, (-150, 0, 20)),
                         (f_hold, (-160, 0, 20)), (f_fall, (0, 0, 0))])
    rots("Forearm.L", [(f_start, (0, 0, 0)), (f_rise, (-20, 0, 0)),
                        (f_hold, (-30, 0, 0)), (f_fall, (0, 0, 0))])
    rots("Forearm.R", [(f_start, (0, 0, 0)), (f_rise, (-20, 0, 0)),
                        (f_hold, (-30, 0, 0)), (f_fall, (0, 0, 0))])
    neutral_rot(["Hand.L", "Hand.R", "Shoulder.L", "Shoulder.R"], FR)

    rots("Chest", [(f_start, (0, 0, 0)), (f_rise, (-10, 0, 0)),
                   (f_hold, (-12, 0, 0)), (f_fall, (14, 0, 0))])
    rots("Spine", [(f_start, (0, 0, 0)), (f_rise, (-6, 0, 0)),
                   (f_hold, (-8, 0, 0)), (f_fall, (8, 0, 0))])
    rots("Head", [(f_start, (0, 0, 0)), (f_rise, (-18, 0, 0)),
                  (f_hold, (-20, 0, 0)), (f_fall, (12, 0, 0))])

    # La cape flotte derrière l'énergie qui monte puis retombe avec le corps ;
    # bouclier et épée, bras tendus au-dessus de la tête, bougent à peine.
    rots("Cape", [(f_start, (0, 0, 0)), (f_rise, (-14, 0, 0)),
                  (f_hold, (-10, 0, 0)), (f_fall, (16, 0, 0))])
    rots("Weapon.R", [(f_start, (0, 0, 0)), (f_rise, (6, 0, -4)),
                       (f_hold, (3, 0, -2)), (f_fall, (-8, 0, 3))])
    rots("Shield.L", [(f_start, (0, 0, 0)), (f_rise, (4, 0, 4)),
                       (f_hold, (2, 0, 2)), (f_fall, (-6, 0, -3))])
    hide_prop(FR)


def jump_keys():
    # Saut sur place (le déplacement vertical réel est géré par le moteur/la
    # physique, comme pour Walk) : anticipation accroupie, poussée des
    # jambes, envol (genoux repliés), préparation de la réception, atterri
    # -ssage amorti, puis retour debout. Bassin, torse, jambes, bras et tête
    # participent tous, avec la cape qui traîne derrière en l'air.
    f0, f_crouch, f_launch, f_apex, f_fall, f_land, f_end = 1, 6, 12, 20, 28, 34, 40
    FR = (f0, f_crouch, f_launch, f_apex, f_fall, f_land, f_end)

    rots("Hips", [
        (f0, (0, 0, 0)), (f_crouch, (6, 0, 0)), (f_launch, (-4, 0, 0)),
        (f_apex, (0, 0, 0)), (f_fall, (2, 0, 0)), (f_land, (8, 0, 0)), (f_end, (0, 0, 0)),
    ])
    for fr, dz in (
        (f0, 0.0), (f_crouch, -0.05), (f_launch, 0.025), (f_apex, 0.04),
        (f_fall, 0.0), (f_land, -0.06), (f_end, 0.0),
    ):
        key_loc("Hips", fr, (0, 0, dz))

    rots("Spine", [
        (f0, (0, 0, 0)), (f_crouch, (5, 0, 0)), (f_launch, (-6, 0, 0)),
        (f_apex, (-3, 0, 0)), (f_fall, (2, 0, 0)), (f_land, (10, 0, 0)), (f_end, (0, 0, 0)),
    ])
    rots("Chest", [
        (f0, (0, 0, 0)), (f_crouch, (3, 0, 0)), (f_launch, (-5, 0, 0)),
        (f_apex, (-2, 0, 0)), (f_fall, (1, 0, 0)), (f_land, (6, 0, 0)), (f_end, (0, 0, 0)),
    ])
    rots("Head", [
        (f0, (0, 0, 0)), (f_crouch, (8, 0, 0)), (f_launch, (-6, 0, 0)),
        (f_apex, (-10, 0, 0)), (f_fall, (2, 0, 0)), (f_land, (10, 0, 0)), (f_end, (0, 0, 0)),
    ])

    for side in ("L", "R"):
        rots(f"Thigh.{side}", [
            (f0, (0, 0, 0)), (f_crouch, (45, 0, 0)), (f_launch, (-15, 0, 0)),
            (f_apex, (30, 0, 0)), (f_fall, (10, 0, 0)), (f_land, (50, 0, 0)), (f_end, (0, 0, 0)),
        ])
        rots(f"Shin.{side}", [
            (f0, (0, 0, 0)), (f_crouch, (-70, 0, 0)), (f_launch, (20, 0, 0)),
            (f_apex, (-55, 0, 0)), (f_fall, (-20, 0, 0)), (f_land, (-75, 0, 0)), (f_end, (0, 0, 0)),
        ])
        rots(f"Foot.{side}", [
            (f0, (0, 0, 0)), (f_crouch, (10, 0, 0)), (f_launch, (-20, 0, 0)),
            (f_apex, (15, 0, 0)), (f_fall, (-5, 0, 0)), (f_land, (12, 0, 0)), (f_end, (0, 0, 0)),
        ])

    rots("UpperArm.L", [
        (f0, (0, 0, -8)), (f_crouch, (20, 0, -12)), (f_launch, (-70, 0, -18)),
        (f_apex, (-50, 0, -32)), (f_fall, (-20, 0, -18)), (f_land, (15, 0, -22)), (f_end, (0, 0, -8)),
    ])
    rots("UpperArm.R", [
        (f0, (0, 0, 8)), (f_crouch, (20, 0, 12)), (f_launch, (-70, 0, 18)),
        (f_apex, (-50, 0, 32)), (f_fall, (-20, 0, 18)), (f_land, (15, 0, 22)), (f_end, (0, 0, 8)),
    ])
    rots("Forearm.L", [
        (f0, (0, 0, 0)), (f_crouch, (-25, 0, 0)), (f_launch, (-10, 0, 0)),
        (f_apex, (-30, 0, 0)), (f_fall, (-15, 0, 0)), (f_land, (-35, 0, 0)), (f_end, (0, 0, 0)),
    ])
    rots("Forearm.R", [
        (f0, (0, 0, 0)), (f_crouch, (-25, 0, 0)), (f_launch, (-10, 0, 0)),
        (f_apex, (-30, 0, 0)), (f_fall, (-15, 0, 0)), (f_land, (-35, 0, 0)), (f_end, (0, 0, 0)),
    ])
    neutral_rot(["Hand.L", "Hand.R", "Shoulder.L", "Shoulder.R"], FR)

    # Mouvement secondaire : la cape traîne en l'air (retard) puis rebondit
    # légèrement à la réception ; épée et bouclier accusent un léger flou.
    rots("Cape", [
        (f0, (0, 0, 0)), (f_crouch, (4, 0, 0)), (f_launch, (-10, 0, 0)),
        (f_apex, (-18, 0, 0)), (f_fall, (-8, 0, 0)), (f_land, (10, 0, 0)), (f_end, (0, 0, 0)),
    ])
    rots("Weapon.R", [
        (f0, (0, 0, 0)), (f_crouch, (3, 0, -3)), (f_launch, (-8, 0, 5)),
        (f_apex, (-4, 0, 3)), (f_fall, (2, 0, -2)), (f_land, (-6, 0, 4)), (f_end, (0, 0, 0)),
    ])
    rots("Shield.L", [
        (f0, (0, 0, 0)), (f_crouch, (-3, 0, 3)), (f_launch, (6, 0, -4)),
        (f_apex, (3, 0, -2)), (f_fall, (-2, 0, 2)), (f_land, (5, 0, -3)), (f_end, (0, 0, 0)),
    ])
    hide_prop(FR)


bake_clip("Idle", 40, idle_keys)
bake_clip("Walk", 24, walk_keys)
bake_clip("Jump", 40, jump_keys)
bake_clip("AttackFire", 30, attack_fire_keys)
bake_clip("AttackShoot", 20, attack_shoot_keys)
bake_clip("AttackSpell", 40, attack_spell_keys)

for pb in arm.pose.bones:
    pb.location = (0, 0, 0)
    pb.rotation_euler = (0, 0, 0)
    pb.scale = (1, 1, 1)
arm.pose.bones["Prop.R"].scale = (0.001, 0.001, 0.001)
bpy.ops.object.mode_set(mode="OBJECT")

# --- Export -------------------------------------------------------------------
bpy.ops.object.select_all(action="SELECT")
bpy.ops.export_scene.gltf(
    filepath=OUT,
    export_format="GLB",
    export_skins=True,
    export_animations=True,
    export_animation_mode="NLA_TRACKS",
    export_force_sampling=True,
    export_yup=True,
)
print("EXPORTED", OUT)

# --- Caméra / éclairage trois points + sol de contact --------------------------
bpy.ops.object.camera_add(location=(3.4, -4.4, 2.1),
                          rotation=(math.radians(76), 0, math.radians(37)))
scene.camera = bpy.context.active_object
scene.camera.data.lens = 60  # focale plus longue = moins de distorsion grand-angle

# Clé (soleil, ombres dures nettes façon héros de jeu stylisé)
bpy.ops.object.light_add(type="SUN", location=(2, -3, 6),
                         rotation=(math.radians(35), math.radians(20), 0))
key_light = bpy.context.active_object
key_light.data.energy = 3.2
key_light.data.angle = math.radians(1.5)  # pénombre fine

# Remplissage (adoucit les ombres côté ombre, sans les effacer)
bpy.ops.object.light_add(type="AREA", location=(-3.2, 2.2, 2.6))
fill_light = bpy.context.active_object
fill_light.data.energy = 55.0
fill_light.data.size = 3.5
fill_light.data.color = (0.85, 0.90, 1.0)  # légèrement froid, contraste avec le soleil chaud

# Contre-jour (détache la silhouette du fond, fait briller les bords/l'épée)
bpy.ops.object.light_add(type="AREA", location=(0.5, 2.6, 3.0),
                         rotation=(math.radians(-60), 0, 0))
rim_light = bpy.context.active_object
rim_light.data.energy = 60.0
rim_light.data.size = 2.0
rim_light.data.color = (0.75, 0.85, 1.0)

# Petit sol pour ancrer le personnage (ombre de contact), matériau neutre mat.
bpy.ops.mesh.primitive_plane_add(size=6, location=(0, 0, 0))
ground = bpy.context.active_object
ground.name = "GroundPlane"
MAT_GROUND = material("FairyGround", (0.5, 0.5, 0.52), roughness=0.9)
ground.data.materials.append(MAT_GROUND)

# Fond dégradé neutre (au lieu du noir pur) pour une présentation plus soignée.
world = scene.world or bpy.data.worlds.new("World")
scene.world = world
world.use_nodes = True
bg = world.node_tree.nodes.get("Background")
if bg:
    bg.inputs[0].default_value = (0.045, 0.05, 0.07, 1.0)
    bg.inputs[1].default_value = 0.9

scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = 960
scene.render.resolution_y = 720
scene.render.film_transparent = False
scene.render.filter_size = 1.2  # anti-aliasing plus doux (bords lissés)
try:
    scene.eevee.taa_render_samples = 128
except AttributeError:
    pass
for attr, val in (
    ("use_gtao", True), ("gtao_distance", 0.3), ("gtao_factor", 1.0),
    ("use_bloom", True), ("bloom_threshold", 1.0), ("bloom_intensity", 0.06),
    ("use_soft_shadows", True), ("use_ssr", True),
):
    try:
        setattr(scene.eevee, attr, val)
    except AttributeError:
        pass

# --- Vignettes de contrôle des poses d'attaque (frame médiane de chaque clip) --
# On assigne directement l'action baked (hors NLA) pour évaluer la pose de
# façon fiable en headless, afin de vérifier visuellement l'implication du
# corps entier (bassin/torse/jambes), pas seulement les bras.
ad = arm.animation_data
POSE_CHECKS = {"Idle": 20, "Walk": 13, "Jump": 20,
               "AttackFire": 14, "AttackShoot": 10, "AttackSpell": 26}
for clip_name, frame in POSE_CHECKS.items():
    ad.action = bpy.data.actions[clip_name]
    scene.frame_set(frame)
    bpy.context.view_layer.update()
    scene.render.filepath = OUT.replace(".glb", f"_preview_{clip_name}.png")
    bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)
ad.action = None

# --- Rendu de contrôle neutre (vue 3/4 avant) -----------------------------------
ad.action = None
for t in list(ad.nla_tracks):
    ad.nla_tracks.remove(t)
bpy.ops.object.select_all(action="DESELECT")
arm.select_set(True)
bpy.context.view_layer.objects.active = arm
bpy.ops.object.mode_set(mode="POSE")
for pb in arm.pose.bones:
    pb.location = (0, 0, 0)
    pb.rotation_euler = (0, 0, 0)
    pb.scale = (1, 1, 1)
arm.pose.bones["Prop.R"].scale = (0.001, 0.001, 0.001)
bpy.ops.object.mode_set(mode="OBJECT")
scene.frame_set(1)
bpy.context.view_layer.update()
scene.render.filepath = OUT.replace(".glb", "_preview.png")
bpy.ops.render.render(write_still=True)
print("RENDERED", scene.render.filepath)

# --- Vignettes de contrôle multi-angles (silhouette, cape dans le dos, ------
# --- épée/bouclier de profil) — pose neutre déjà en place ci-dessus. -------
cam = scene.camera
ANGLE_SHOTS = {
    "back": ((0, 4.6, 1.35), (math.radians(82), 0, math.radians(180))),
    "side": ((4.6, 0, 1.35), (math.radians(82), 0, math.radians(90))),
}
orig_loc, orig_rot = tuple(cam.location), tuple(cam.rotation_euler)
for shot_name, (loc, rot) in ANGLE_SHOTS.items():
    cam.location = loc
    cam.rotation_euler = rot
    bpy.context.view_layer.update()
    scene.render.filepath = OUT.replace(".glb", f"_preview_{shot_name}.png")
    bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)
cam.location = orig_loc
cam.rotation_euler = orig_rot
