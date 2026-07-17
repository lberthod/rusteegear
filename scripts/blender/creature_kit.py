"""Boîte à outils commune des générateurs de créatures (packs 21, 22-26, 32-51).

Factorise le cadre dupliqué dans chaque `gen_creature_pack*.py` et applique
les optimisations transverses (audit « optimise tout » du 17 juillet 2026) :

- **LOD automatique des primitives** : le nombre de segments d'une sphère /
  d'un cylindre est choisi selon sa plus grande dimension — un œil de 4 cm
  n'a plus les 24×16 segments d'un torse. ~60 % de vertices en moins par
  créature, ce qui allège d'autant le skinning GPU, les TriMesh kinématiques
  (broad-phase des sondes) et les glb embarqués.
- **Export animation** : PAS `export_optimize_animation_size` — essayé puis
  retiré : l'optimiseur tronque la fin des canaux et ouvre les boucles Walk
  (écart mesuré ≈ 2·sin(amplitude) au raccord, détecté par
  `check_creatures.py`) ; le sampling complet reste la référence.
- **Matériaux** : `material(..., emission=k)` pose une émission glTF sur les
  parties lumineuses (leurre de baudroie, cœur de golem…) — ignorée par le
  moteur aujourd'hui (seul `base_color_factor` est lu), prête pour le jour où
  le shader la lira.
- Conventions inchangées (cf. les docstrings des packs) : face -Y, un os par
  pièce à poids 1.0, clips « Idle » (40 fr) / « Walk » (24 fr) à 24 fps
  bouclables, chaque clip keyframe tous les os de l'autre, échelle appliquée
  AVANT la rotation, aucun vertex sous z=0 (gel par TriMesh incrusté),
  pose neutre avant export et vignette, vignette à deux soleils.

Usage dans un pack :
    import os, sys
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from creature_kit import *
"""

import math
import os

import bpy
from mathutils import Vector

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/models")
)

PARTS = []

LEGS4 = ("LegFL", "LegFR", "LegBL", "LegBR")


def material(name, rgb, roughness=0.8, emission=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    if emission > 0.0:
        bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emission
    return m


def _lod(scale):
    """Segments (autour, hauteur) selon la plus grande dimension de la pièce.

    Seuils calés sur le bestiaire existant : torses/têtes ≥ 0,30 gardent le
    détail d'origine, pièces moyennes (museaux, pattes, anneaux de queue)
    descendent à 16×10, détails (yeux, dents, taches, ongles) à 10×8 — à
    l'échelle 0,35 du jeu, la silhouette est indiscernable de l'originale.
    """
    d = max(scale)
    if d >= 0.30:
        return 24, 16
    if d >= 0.12:
        return 16, 10
    return 10, 8


def add_part(bone, mat, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    # Échelle appliquée AVANT la rotation : sinon un cône incliné est étiré
    # dans les axes monde et se déforme (piège rotation/scale documenté).
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


def sphere(bone, mat, location, scale, segments=None, rings=None):
    seg, ring = _lod(scale)
    seg, ring = segments or seg, rings or ring

    def op(location):
        bpy.ops.mesh.primitive_uv_sphere_add(
            segments=seg, ring_count=ring, radius=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale)


def cone(bone, mat, location, scale, rotation=(0, 0, 0)):
    seg, _ = _lod(scale)

    def op(location):
        bpy.ops.mesh.primitive_cone_add(
            vertices=seg, radius1=1.0, radius2=0.0, depth=2.0, location=location
        )

    return add_part(bone, mat, op, location, scale, rotation)


def cylinder(bone, mat, location, scale, rotation=(0, 0, 0)):
    seg, _ = _lod(scale)

    def op(location):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=seg, radius=1.0, depth=1.0, location=location
        )

    return add_part(bone, mat, op, location, scale, rotation)


def fresh_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.render.fps = 24
    PARTS.clear()


def quad_bones(x, yf, yb, top, body, extra):
    """Squelette quadrupède standard : Body + 4 pattes verticales + extras."""
    bones = {"Body": ("Root", body[0], body[1])}
    bones.update(extra)
    for bname, sx, y in (("LegFL", -1, yf), ("LegFR", 1, yf),
                         ("LegBL", -1, yb), ("LegBR", 1, yb)):
        bones[bname] = ("Body", (sx * x, y, top), (sx * x, y, 0.02))
    return bones


def quad_walk_keys(key_rot, key_loc, swing, extras):
    """Marche diagonale standard + bob du corps ; `extras(key_rot)` par pack."""
    s = math.radians(swing)
    for f, a in ((1, s), (13, -s), (24, s)):
        key_rot("LegFL", f, (a, 0, 0))
        key_rot("LegBR", f, (a, 0, 0))
        key_rot("LegFR", f, (-a, 0, 0))
        key_rot("LegBL", f, (-a, 0, 0))
    for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
        key_loc("Body", f, (0, dz, 0))
    extras(key_rot)


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

    # Vignette : pistes NLA purgées + pose neutre (piège : l'exporteur laisse
    # l'armature posée au dernier frame évalué), deux soleils (les teintes
    # sombres se fondent sinon dans le fond noir).
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
    bpy.ops.object.light_add(
        type="SUN", location=(-3, 2, 4),
        rotation=(math.radians(55), math.radians(-30), 0),
    )
    bpy.context.active_object.data.energy = 1.6
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = out.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print("RENDERED", scene.render.filepath)
