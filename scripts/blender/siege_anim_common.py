"""Squelette + bake minimal pour les 12 assets animés du pack « siège du
hameau » (addendum `creationAnimation3DBlendersuite.md`, à la racine du
dépôt) — porte de rempart, poterne, herse, chariot, brasero, bannières,
torche, balise de spawn, cage du chef.

Contrainte moteur confirmée dans `src/scene/import.rs` : aucune animation
n'est lue sans squelette+skin (`load_gltf_skeleton`/`load_gltf_clips`
renvoient vide sans `skin`) — même un battant de porte a besoin d'un
mini-squelette. Recette calquée sur `creature_kit.build_creature`/
`bake_clip`, généralisée à un squelette sur-mesure par pièce (Root + un os
par partie mobile, jamais un rig quadrupède générique) plutôt qu'un rig
standard réutilisé partout.

Usage : construire les pièces avec les helpers de `hamlet_common` (cube,
cylinder, cone, blob), appeler `weight(obj, bone)` sur chacune (y compris les
parties statiques, pondérées à "Root"), puis `build_prop(...)`.
"""

import math
import os

import bpy
from mathutils import Vector

from hamlet_common import OUT_DIR


FLAME_PATTERNS = [
    [(1, 1.0), (9, 1.12), (18, 0.92), (27, 1.08), (36, 0.95), (40, 1.0)],
    [(1, 1.0), (7, 0.93), (16, 1.15), (24, 0.90), (33, 1.1), (40, 1.0)],
    [(1, 1.0), (11, 1.08), (21, 0.94), (30, 1.10), (40, 1.0)],
]
"""Motifs de vacillement d'échelle (flammes/torches) — toujours à l'échelle
1.0 aux frames 1 et 40 pour fermer la boucle NLA, déphasés entre eux pour un
effet moins synchronisé quand plusieurs foyers d'un même asset les utilisent
en rotation (`i % len(FLAME_PATTERNS)`)."""


def weight(obj, bone):
    """Groupe de vertex à poids plein (100 %) sur `bone` — pas de skinning
    mélangé (méthode de l'addendum, point 2)."""
    vg = obj.vertex_groups.new(name=bone)
    vg.add(range(len(obj.data.vertices)), 1.0, "REPLACE")
    return obj


def build_banner_geo(cloth_mat, pole_mat, width, height, n_segments=2,
                      pole_height=None, pole_radius=0.05, panel_thickness=0.02,
                      location=(0.0, 0.0, 0.0)):
    """Poteau + N segments de tissu empilés verticalement, chacun articulé
    sur le bord côté poteau (charnière verticale, même principe que les
    vantaux de porte) — version animable de `hamlet_common.banner()` (celle-ci
    reste un panneau plein, pour les usages non animés). `n_segments=1` donne
    un unique os "Tissu" (fanion), `n_segments>1` donne "Tissu1".."TissuN".
    Retourne (pole_obj, [(obj, bone_name), ...], bones) — `pole_obj` doit
    encore être pondéré à Root par l'appelant (weight_remaining)."""
    from hamlet_common import cube, cylinder

    x, y, z = location
    pole_h = pole_height if pole_height is not None else height * 1.4
    pole = cylinder("Poteau", pole_mat, (x, y, z + pole_h / 2), pole_radius, pole_h, vertices=8)
    panel_top = pole_h * 0.92
    panel_x = x + width / 2 - pole_radius * 0.5
    hinge_x = panel_x - width / 2
    seg_h = height / n_segments
    segs = []
    bones = {}
    for i in range(n_segments):
        seg_z = z + panel_top - height + seg_h * (i + 0.5)
        name = f"Tissu{i + 1}" if n_segments > 1 else "Tissu"
        obj = cube(name, cloth_mat, (panel_x, y, seg_z), (width, panel_thickness, seg_h))
        segs.append((obj, name))
        bones[name] = ("Root", (hinge_x, y, seg_z), (hinge_x, y, seg_z + 0.15))
    return pole, segs, bones


def weight_remaining(exclude, bone="Root"):
    """Pondère à `bone` (Root par défaut) tous les mesh de la scène pas déjà
    dans `exclude` — pratique pour les pièces statiques créées via des
    helpers qui ne renvoient pas leurs objets (crenellations(), stone_coursing()...) :
    plutôt que de les traquer un par un, on les rattrape en fin de scène."""
    for o in bpy.context.scene.objects:
        if o.type == "MESH" and o not in exclude:
            weight(o, bone)


def build_prop(name, parts, bones, clip_name, keyer, fps=24, cam=1.0, linear_bones=()):
    """Joint `parts` (déjà pondérées via weight()), construit le squelette
    `bones` ({nom_os: (parent, head, tail)} — "Root" existe toujours, pas
    besoin de le lister), bake un unique clip NLA `clip_name` via `keyer(key_rot,
    key_loc, key_scale)`, exporte en GLB skinné + vignette (pose neutre,
    frame 1, même convention deux-soleils que hamlet_common.render_preview)."""
    bpy.context.scene.render.fps = fps
    bpy.ops.object.select_all(action="DESELECT")
    for ob in parts:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    if len(parts) > 1:
        bpy.ops.object.join()
    prop = bpy.context.view_layer.objects.active
    prop.name = name.capitalize()

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = f"{prop.name}Rig"
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

    prop.parent = arm
    prop.modifiers.new("Armature", "ARMATURE").object = arm

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

    def reset_pose():
        for pb in arm.pose.bones:
            pb.location = (0, 0, 0)
            pb.rotation_euler = (0, 0, 0)
            pb.scale = (1, 1, 1)

    ad = arm.animation_data_create()
    ad.action = None
    keyer(key_rot, key_loc, key_scale)
    act = ad.action
    act.name = clip_name
    if linear_bones:
        # Rotation continue (roue qui tourne, rune qui pivote) : l'interpolation
        # Bezier par défaut ease in/out à chaque keyframe, ce qui produit un
        # à-coup de vitesse au raccord de boucle NLA (piège documenté sur les
        # toupies des packs item_*, mémoire blender-headless-asset-pipeline).
        # Blender 5.x : les fcurves vivent dans les channelbags des actions
        # slottées, pas directement sur act.fcurves.
        for layer in act.layers:
            for strip in layer.strips:
                for bag in strip.channelbags:
                    for fc in bag.fcurves:
                        if any(f'"{b}"' in fc.data_path for b in linear_bones):
                            for kp in fc.keyframe_points:
                                kp.interpolation = "LINEAR"
    track = ad.nla_tracks.new()
    track.name = clip_name
    track.strips.new(clip_name, 1, act)
    ad.action = None
    reset_pose()
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
    print(f"[siege_anim] exporté {name}.glb")

    # Vignette : pistes NLA purgées + pose neutre au frame 1 (piège documenté
    # dans creature_kit/mémoire blender-headless-asset-pipeline : l'exporteur
    # laisse l'armature posée au dernier frame évalué).
    ad = arm.animation_data
    ad.action = None
    for t in list(ad.nla_tracks):
        ad.nla_tracks.remove(t)
    reset_pose()
    scene = bpy.context.scene
    scene.frame_set(1)
    bpy.context.view_layer.update()
    dims = prop.dimensions
    span = max(dims.x, dims.y, dims.z, 0.3)
    cam_dist = span * 1.5 + 0.5
    center = Vector((0, 0, dims.z * 0.5))
    cam_loc = Vector((cam_dist, cam_dist * 1.15, cam_dist * 0.85 + dims.z * 0.3))
    bpy.ops.object.camera_add(location=cam_loc)
    cam_obj = bpy.context.active_object
    scene.camera = cam_obj
    bpy.ops.object.empty_add(type="PLAIN_AXES", location=center)
    target = bpy.context.active_object
    tc = cam_obj.constraints.new("TRACK_TO")
    tc.target = target
    tc.track_axis = "TRACK_NEGATIVE_Z"
    tc.up_axis = "UP_Y"
    bpy.context.view_layer.update()
    bpy.ops.object.light_add(type="SUN", location=(2, -3, 6),
                              rotation=(math.radians(35), math.radians(20), 0))
    bpy.context.active_object.data.energy = 3.0
    bpy.ops.object.light_add(type="SUN", location=(-3, 2, 4),
                              rotation=(math.radians(55), math.radians(-30), 0))
    bpy.context.active_object.data.energy = 1.6
    world = scene.world
    if world is None:
        world = bpy.data.worlds.new("World")
        scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    if bg is not None:
        bg.inputs[0].default_value = (1.0, 1.0, 1.0, 1.0)
        bg.inputs[1].default_value = 0.35
    try:
        scene.render.engine = "BLENDER_EEVEE_NEXT"
    except TypeError:
        scene.render.engine = "BLENDER_EEVEE"
    scene.view_settings.view_transform = "Standard"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = out.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print(f"[siege_anim] vignette {name}_preview.png")
