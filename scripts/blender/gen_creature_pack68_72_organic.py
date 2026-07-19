"""Génère assets/models/creature68.glb … creature72.glb : 5 mammifères ronds
au corps organique lissé (Metaball + Automatic Weights), technique de
`proto_creature62_fox_organic.py` (Option B de
`docs/rapport_qualite_creatures_vs_hyper3d.md`) — validée là comme prototype
de comparaison, appliquée ici pour la première fois à un petit pack complet.

Hippopotame, capybara, loutre de mer, koala, marmotte : cinq mammifères au
corps massif et arrondi, qui se prêtent mieux au lissage metaball que
l'assemblage de primitives à angles vifs des packs précédents.

Technique par créature (cf. docstring de `proto_creature62_fox_organic.py`
pour le détail) :
- **Corps/tête/pattes/queue** : objet Metaball (éléments ellipsoïdes qui
  fusionnent en surface lisse), converti en mesh puis Shade Smooth.
- **Accessoires nets** (oreilles, yeux, truffe, griffes, bas des pattes) :
  primitives rigides classiques de `creature_kit.py`, jointes après coup.
- **Poids** : Automatic Weights (heat diffusion) sur le corps organique SEUL
  avant de joindre les accessoires rigides à poids 1.0 nommé.
- Conventions inchangées par ailleurs : face -Y, clips Idle 40 fr / Walk 24 fr
  bouclables, aucun vertex sous z=0 + marge, QA par `check_creatures.py`.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack68_72_organic.py
"""

import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from creature_kit import (  # noqa: E402
    LEGS4, OUT_DIR, PARTS, cone, cylinder, fresh_scene, material, sphere,
)


def meta_elem(mb_data, co, radius, size=(1.0, 1.0, 1.0), stiffness=2.0):
    e = mb_data.elements.new()
    e.type = "ELLIPSOID"
    e.co = Vector(co)
    e.radius = radius
    e.size_x, e.size_y, e.size_z = size
    e.stiffness = stiffness
    return e


def build_organic_core(name, elements, resolution=0.032, threshold=0.09):
    """`elements` : liste de (co, radius, size, stiffness) — cf. `meta_elem`.

    Chevauchement généreux entre éléments voisins obligatoire (piège déjà
    documenté pour les chaînes de metaballs : sinon « collier de perles »,
    chaque élément visible comme une bosse séparée au lieu d'une surface
    continue). Regarde-fou sol : remonte tout le mesh si un vertex passe
    sous z=0,02 (gel par TriMesh incrusté).
    """
    mb_data = bpy.data.metaballs.new(f"{name}Meta")
    mb_data.resolution = resolution
    mb_data.render_resolution = resolution
    mb_data.threshold = threshold
    mb_obj = bpy.data.objects.new(f"{name}Meta", mb_data)
    bpy.context.collection.objects.link(mb_obj)
    # Facteur de sécurité empirique (cf. session) : deux éléments de rayon r
    # ne fusionnent en surface continue QUE si leur écart de centres reste
    # sous ~0,7×(r1+r2) au seuil/raideur par défaut — un simple chevauchement
    # géométrique (écart < r1+r2) ne suffit PAS et laisse un « collier de
    # perles » disjoint. Grossir tous les rayons d'un facteur fixe, sans
    # toucher aux positions, élargit cette marge partout d'un coup.
    RADIUS_SCALE = 1.6
    for co, radius, size, stiffness in elements:
        meta_elem(mb_data, co, radius * RADIUS_SCALE, size, stiffness)

    bpy.ops.object.select_all(action="DESELECT")
    mb_obj.select_set(True)
    bpy.context.view_layer.objects.active = mb_obj
    bpy.context.view_layer.update()
    bpy.ops.object.convert(target="MESH")
    core = bpy.context.active_object
    core.name = f"{name}OrganicCore"
    bpy.ops.object.shade_smooth()

    core.data.update()
    min_z = min(v.co.z for v in core.data.vertices)
    if min_z < 0.02:
        core.location.z += 0.02 - min_z
        bpy.ops.object.transform_apply(location=True, rotation=False, scale=False)

    return core


def build_organic_creature(name, fur_color, elements, bones_def, accessories,
                            idle_keys, walk_keys, cam=1.0, roughness=0.85,
                            resolution=0.032, threshold=0.09):
    """Assemble un mammifère organique complet et l'exporte.

    `accessories(fur, cream, dark, white)` : callback qui pose les pièces
    rigides via `sphere`/`cone`/`cylinder` de `creature_kit` (remplit
    `PARTS`, vidé juste avant l'appel). `bones_def` : dict squelette au même
    format que `creature_kit.build_creature`.
    """
    fresh_scene()
    fur = material(f"{name}Fur", fur_color, roughness=roughness)
    cream = material(f"{name}Cream", (0.90, 0.85, 0.72), roughness=roughness)
    dark = material(f"{name}Dark", (0.08, 0.07, 0.07), roughness=roughness)
    white = material(f"{name}White", (0.96, 0.96, 0.93), roughness=roughness)

    core = build_organic_core(name, elements, resolution, threshold)
    core.data.materials.append(fur)

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = f"{name}Rig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.35))
    for bname, (parent, head, tail) in bones_def.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    bpy.ops.object.select_all(action="DESELECT")
    core.select_set(True)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")

    PARTS.clear()
    accessories(fur, cream, dark, white)
    for ob in PARTS:
        for poly in ob.data.polygons:
            poly.use_smooth = True

    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    core.select_set(True)
    bpy.context.view_layer.objects.active = core
    bpy.ops.object.join()
    creature = bpy.context.active_object
    creature.name = name

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

    out = os.path.join(OUT_DIR, f"{name.lower()}.glb")
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


LEG_BONES = {
    "LegFL": ("Body", (-0.24, -0.40, 0.34), (-0.24, -0.40, 0.02)),
    "LegFR": ("Body", (0.24, -0.40, 0.34), (0.24, -0.40, 0.02)),
    "LegBL": ("Body", (-0.24, 0.40, 0.34), (-0.24, 0.40, 0.02)),
    "LegBR": ("Body", (0.24, 0.40, 0.34), (0.24, 0.40, 0.02)),
}


# =============================================================================
# Créature 68 — Hippopotame : masse énorme, gueule qui bâille.
# =============================================================================
def hippopotame():
    K = 1.4
    elements = [
        ((0, 0.05, 0.62), 0.42, (1.05, 1.75, 0.95), K),
        ((0, -0.45, 0.60), 0.30, (0.95, 0.85, 0.90), K),  # épaule
        ((0, 0.55, 0.58), 0.30, (0.95, 0.85, 0.85), K),  # hanche
        ((0, -0.85, 0.62), 0.30, (0.95, 0.90, 0.85), K),  # tête large
        ((0, -1.15, 0.56), 0.20, (0.90, 0.95, 0.72), K),  # mufle
        ((0, 0.95, 0.55), 0.10, (0.80, 0.90, 0.80), K),  # base queue
    ]
    for x, y in ((-0.26, -0.42), (0.26, -0.42), (-0.26, 0.44), (0.26, 0.44)):
        elements.append(((x, y, 0.32), 0.19, (1.05, 1.05, 1.30), K))
        elements.append(((x, y, 0.12), 0.13, (1.00, 1.00, 1.25), K))
    bones = {
        "Body": ("Root", (0, 0.45, 0.55), (0, -0.60, 0.60)),
        "Head": ("Body", (0, -0.65, 0.60), (0, -1.25, 0.55)),
        "Tail": ("Body", (0, 0.75, 0.55), (0, 1.05, 0.50)),
        **LEG_BONES,
    }

    def accessories(fur, cream, dark, white):
        for sx in (-1, 1):
            # Oreille en 2 pièces (pavillon fourrure + creux interne sombre),
            # comme les autres packs — un disque plat uni se lit mal.
            sphere("Head", fur, (sx * 0.20, -0.72, 0.86), (0.065, 0.06, 0.055))
            sphere("Head", dark, (sx * 0.20, -0.735, 0.855), (0.04, 0.03, 0.035))
            # Œil à 3 pièces (sclère + pupille décalée avant-extérieur +
            # micro-reflet), technique du renard organique — beaucoup plus
            # « vivant » qu'un simple point sombre.
            sphere("Head", white, (sx * 0.16, -0.98, 0.72), (0.05, 0.044, 0.05))
            sphere("Head", dark, (sx * 0.178, -1.005, 0.716), (0.026, 0.02, 0.026))
            sphere("Head", white, (sx * 0.185, -1.012, 0.728), (0.009, 0.007, 0.009))
            sphere("Head", dark, (sx * 0.10, -1.24, 0.60), (0.035, 0.03, 0.03))  # naseau
        sphere("Head", cream, (0, -1.10, 0.42), (0.16, 0.20, 0.10))  # bajoue/gueule claire
        # Bouche : fente sombre + deux incisives émoussées, cohérent avec le
        # bâillement de l'Idle. Remontées dans la moitié haute de la bajoue
        # (pas à son bord bas) : la surface metaball réelle sous un élément
        # isolé s'arrête vers ~75 % du rayon nominal (mesuré en session),
        # donc tout accessoire posé au ras du bord nominal pend dans le vide.
        sphere("Head", dark, (0, -1.14, 0.40), (0.15, 0.05, 0.025))  # fente
        for sx in (-1, 1):
            sphere("Head", white, (sx * 0.05, -1.16, 0.40), (0.025, 0.025, 0.04))  # incisive
        for bone, x, y in (("LegFL", -0.26, -0.42), ("LegFR", 0.26, -0.42),
                           ("LegBL", -0.26, 0.44), ("LegBR", 0.26, 0.44)):
            cylinder(bone, dark, (x, y, 0.055), (0.16, 0.17, 0.045))  # sabot large

    def idle(key_rot, key_loc):
        # Bâille, gueule grande ouverte (tête bascule loin en arrière, tient
        # la pose), puis se replonge lourdement.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.02), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, 0.0), (10, -0.55), (14, -0.65), (26, -0.65), (32, 0.0),
                      (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, sw in ((1, 0.06), (20, -0.06), (40, 0.06)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        s = math.radians(12)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.04), (13, 0.0), (19, 0.04), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.03), (13, -0.03), (24, 0.03)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Tail", f, (0, 0, sw))

    build_organic_creature("Creature68", (0.42, 0.36, 0.36), elements, bones,
                            accessories, idle, walk, cam=1.1)


# =============================================================================
# Créature 69 — Capybara : posé, mâchouille, très détendu.
# =============================================================================
def capybara():
    K = 1.3
    elements = [
        ((0, 0.0, 0.44), 0.28, (1.00, 1.60, 0.72), K),
        ((0, -0.40, 0.44), 0.20, (0.90, 0.80, 0.75), K),
        ((0, 0.42, 0.42), 0.20, (0.90, 0.80, 0.72), K),
        ((0, -0.70, 0.46), 0.20, (0.95, 0.95, 0.72), K),  # tête carrée
        ((0, -0.92, 0.40), 0.13, (0.90, 0.95, 0.60), K),  # museau tronqué
    ]
    for x, y in ((-0.18, -0.32), (0.18, -0.32), (-0.18, 0.34), (0.18, 0.34)):
        elements.append(((x, y, 0.24), 0.13, (0.95, 0.95, 1.15), K))
        elements.append(((x, y, 0.08), 0.09, (0.90, 0.90, 1.10), K))
    bones = {
        "Body": ("Root", (0, 0.38, 0.42), (0, -0.42, 0.46)),
        "Head": ("Body", (0, -0.50, 0.46), (0, -0.98, 0.42)),
        "LegFL": ("Body", (-0.18, -0.32, 0.28), (-0.18, -0.32, 0.02)),
        "LegFR": ("Body", (0.18, -0.32, 0.28), (0.18, -0.32, 0.02)),
        "LegBL": ("Body", (-0.18, 0.34, 0.28), (-0.18, 0.34, 0.02)),
        "LegBR": ("Body", (0.18, 0.34, 0.28), (0.18, 0.34, 0.02)),
    }

    def accessories(fur, cream, dark, white):
        for sx in (-1, 1):
            sphere("Head", fur, (sx * 0.14, -0.62, 0.62), (0.038, 0.032, 0.038))  # petite oreille
            sphere("Head", dark, (sx * 0.14, -0.635, 0.62), (0.024, 0.018, 0.024))  # creux
            # Œil à 3 pièces (sclère + pupille + micro-reflet).
            sphere("Head", white, (sx * 0.11, -0.85, 0.52), (0.036, 0.032, 0.036))
            sphere("Head", dark, (sx * 0.122, -0.865, 0.516), (0.02, 0.016, 0.02))
            sphere("Head", white, (sx * 0.127, -0.872, 0.524), (0.007, 0.006, 0.007))
        sphere("Head", dark, (0, -0.96, 0.40), (0.04, 0.035, 0.03))  # naseau
        sphere("Head", cream, (0, -0.85, 0.37), (0.06, 0.06, 0.05))  # museau clair
        # Grandes incisives orange, signature du rongeur. Le cœur organique
        # d'un élément isolé a sa vraie surface bien EN DEÇÀ du rayon nominal
        # (~75 %, mesuré en session) : tout accessoire visé au bord nominal
        # pend dans le vide. Assemblage remonté dans la moitié haute du
        # museau (marge large, pas juste « posé au ras du bord »).
        sphere("Head", dark, (0, -0.86, 0.35), (0.05, 0.025, 0.016))  # fente de bouche
        teeth = material("Capybara69Teeth", (0.82, 0.62, 0.22))
        for sx in (-1, 1):
            sphere("Head", teeth, (sx * 0.02, -0.87, 0.35), (0.016, 0.016, 0.03))
        for bone, x, y in (("LegFL", -0.18, -0.32), ("LegFR", 0.18, -0.32),
                           ("LegBL", -0.18, 0.34), ("LegBR", 0.18, 0.34)):
            sphere(bone, dark, (x, y, 0.03), (0.09, 0.09, 0.03))  # patte palmée

    def idle(key_rot, key_loc):
        # Mâchouille placidement : petite mastication de la tête, quasi
        # immobile — le calme légendaire du capybara.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.015), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.0), (6, 0.10), (11, 0.0), (16, 0.10), (21, 0.0),
                       (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        s = math.radians(16)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.03), (13, -0.03), (24, 0.03)):
            key_rot("Head", f, (nod, 0, 0))

    build_organic_creature("Creature69", (0.52, 0.42, 0.28), elements, bones,
                            accessories, idle, walk, cam=0.85)


# =============================================================================
# Créature 70 — Loutre de mer : flotte sur le dos, se tortille en nageant.
# =============================================================================
def loutre():
    K = 1.2
    elements = [
        ((0, 0.0, 0.28), 0.20, (0.72, 1.65, 0.68), K),  # corps long et fuselé
        ((0, -0.55, 0.30), 0.15, (0.68, 0.85, 0.60), K),  # cou
        ((0, -0.78, 0.32), 0.16, (0.85, 0.85, 0.78), K),  # tête ronde
        ((0, -0.98, 0.28), 0.09, (0.75, 0.85, 0.60), K),  # museau
    ]
    for k in range(4):  # queue étirée, effilée
        t = k / 3.0
        elements.append(((0, 0.62 + 0.24 * t, 0.30 - 0.04 * t),
                          0.13 - 0.06 * t, (0.75, 1.35, 0.70), 1.1))
    for x, y in ((-0.13, -0.22), (0.13, -0.22), (-0.13, 0.30), (0.13, 0.30)):
        elements.append(((x, y, 0.16), 0.09, (0.90, 0.90, 1.00), K))
    bones = {
        "Body": ("Root", (0, -0.30, 0.28), (0, 0.35, 0.28)),
        "Head": ("Body", (0, -0.45, 0.30), (0, -1.02, 0.28)),
        "Tail": ("Body", (0, 0.45, 0.28), (0, 0.95, 0.24)),
        "LegFL": ("Body", (-0.13, -0.22, 0.18), (-0.13, -0.22, 0.02)),
        "LegFR": ("Body", (0.13, -0.22, 0.18), (0.13, -0.22, 0.02)),
        "LegBL": ("Body", (-0.13, 0.30, 0.18), (-0.13, 0.30, 0.02)),
        "LegBR": ("Body", (0.13, 0.30, 0.18), (0.13, 0.30, 0.02)),
    }

    def accessories(fur, cream, dark, white):
        for sx in (-1, 1):
            sphere("Head", fur, (sx * 0.10, -0.68, 0.42), (0.032, 0.026, 0.032))  # petite oreille
            sphere("Head", dark, (sx * 0.10, -0.695, 0.42), (0.02, 0.015, 0.02))  # creux
            # Œil à 3 pièces, un peu plus grand/rond (regard curieux de loutre).
            sphere("Head", white, (sx * 0.09, -0.86, 0.36), (0.034, 0.03, 0.034))
            sphere("Head", dark, (sx * 0.10, -0.875, 0.356), (0.019, 0.015, 0.019))
            sphere("Head", white, (sx * 0.105, -0.882, 0.363), (0.007, 0.006, 0.007))
            # (Moustaches retirées : en petits points isolés à ce gabarit,
            # elles ne touchent jamais le maillage et se lisent comme des
            # débris flottants plutôt que des vibrisses — cf. captures.)
        # Truffe/bouche/dents remontées et reculées vers le centre du museau
        # (marge large) : posées au bord nominal du cœur organique, elles
        # pendaient dans le vide — la vraie surface d'un élément isolé
        # s'arrête bien avant ce bord (~75 % du rayon nominal, mesuré).
        sphere("Head", dark, (0, -1.00, 0.29), (0.03, 0.026, 0.025))  # truffe
        sphere("Head", cream, (0, -0.88, 0.26), (0.08, 0.08, 0.06))  # museau clair
        # Bouche entrouverte + petites dents pointues (loutre croqueuse de
        # coquillages) : fente sombre courte, deux crocs fins qui dépassent.
        sphere("Head", dark, (0, -0.93, 0.27), (0.045, 0.02, 0.016))
        for sx in (-1, 1):
            sphere("Head", white, (sx * 0.03, -0.935, 0.26), (0.012, 0.012, 0.022))
        for bone, x, y in (("LegFL", -0.13, -0.22), ("LegFR", 0.13, -0.22),
                           ("LegBL", -0.13, 0.30), ("LegBR", 0.13, 0.30)):
            sphere(bone, dark, (x, y, 0.035), (0.075, 0.075, 0.025))  # patte palmée

    def idle(key_rot, key_loc):
        # Flotte sur le dos : le corps roule doucement d'un côté à l'autre,
        # les pattes remuent en berçant, la tête se tourne pour observer.
        for f in (1, 40):
            key_rot("Body", f, (0, math.radians(150), 0))
        for f, roll in ((1, math.radians(150) - 0.10), (20, math.radians(150) + 0.10),
                        (40, math.radians(150) - 0.10)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (15, 0.25), (28, -0.25), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))
        for f, s in ((1, 0.15), (10, -0.15), (20, 0.15), (30, -0.15), (40, 0.15)):
            for leg in LEGS4:
                key_rot(leg, f, (s, 0, 0))

    def walk(key_rot, key_loc):
        # Se tortille pour nager : ondulation du corps (lacet en S) plutôt
        # qu'une marche à quatre pattes classique.
        for f, yaw in ((1, 0.25), (13, -0.25), (24, 0.25)):
            key_rot("Body", f, (0, 0, yaw))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, s in ((1, 0.35), (13, -0.35), (24, 0.35)):
            for leg in LEGS4:
                key_rot(leg, f, (s, 0, 0))
        for f, sw in ((1, 0.3), (13, -0.3), (24, 0.3)):
            key_rot("Tail", f, (0, 0, sw))

    build_organic_creature("Creature70", (0.34, 0.24, 0.16), elements, bones,
                            accessories, idle, walk, cam=0.7)


# =============================================================================
# Créature 71 — Koala : rond et pelucheux, oreilles duveteuses géantes.
# =============================================================================
def koala():
    K = 1.4
    elements = [
        ((0, 0.05, 0.42), 0.26, (1.00, 1.10, 0.95), K),
        ((0, -0.35, 0.52), 0.24, (1.00, 1.00, 0.95), K),  # tête très ronde
    ]
    for x, y in ((-0.16, -0.28), (0.16, -0.28), (-0.16, 0.30), (0.16, 0.30)):
        elements.append(((x, y, 0.24), 0.13, (0.95, 0.95, 1.10), K))
        elements.append(((x, y, 0.09), 0.09, (0.90, 0.90, 1.05), K))
    bones = {
        "Body": ("Root", (0, 0.30, 0.40), (0, -0.20, 0.44)),
        "Head": ("Body", (0, -0.20, 0.48), (0, -0.55, 0.56)),
        "LegFL": ("Body", (-0.16, -0.28, 0.30), (-0.16, -0.28, 0.02)),
        "LegFR": ("Body", (0.16, -0.28, 0.30), (0.16, -0.28, 0.02)),
        "LegBL": ("Body", (-0.16, 0.30, 0.30), (-0.16, 0.30, 0.02)),
        "LegBR": ("Body", (0.16, 0.30, 0.30), (0.16, 0.30, 0.02)),
    }

    def accessories(fur, cream, dark, white):
        for sx in (-1, 1):
            # Grande oreille duveteuse ronde, signature du koala.
            sphere("Head", fur, (sx * 0.28, -0.30, 0.68), (0.13, 0.06, 0.14))
            sphere("Head", cream, (sx * 0.28, -0.33, 0.68), (0.08, 0.03, 0.09))
            # Œil à 3 pièces (sclère + pupille + micro-reflet) — regard
            # somnolent, pupille un peu plus basse pour l'air endormi.
            sphere("Head", white, (sx * 0.11, -0.56, 0.58), (0.04, 0.035, 0.04))
            sphere("Head", dark, (sx * 0.12, -0.575, 0.572), (0.022, 0.018, 0.022))
            sphere("Head", white, (sx * 0.125, -0.582, 0.58), (0.008, 0.006, 0.008))
        # Nez/bouche remontés vers le centre de la tête (marge large) : posés
        # au bord nominal du cœur organique, ils pendaient dans le vide — la
        # vraie surface metaball d'un élément isolé s'arrête bien avant ce
        # bord (~75 % du rayon nominal, mesuré en session).
        sphere("Head", dark, (0, -0.54, 0.42), (0.06, 0.05, 0.05))  # grand nez noir
        # Bouche : fine fente incurvée sous le nez, expression placide.
        sphere("Head", dark, (0, -0.56, 0.35), (0.06, 0.022, 0.014))
        for bone, x, y in (("LegFL", -0.16, -0.28), ("LegFR", 0.16, -0.28),
                           ("LegBL", -0.16, 0.30), ("LegBR", 0.16, 0.30)):
            sphere(bone, dark, (x, y, 0.035), (0.10, 0.10, 0.025))  # coussinet+griffes

    def idle(key_rot, key_loc):
        # Somnole : la tête dodeline lentement puis pique du nez, sursaut
        # léger, retour — la torpeur légendaire du koala.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.015), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.0), (16, 0.30), (24, 0.45), (28, 0.10), (34, 0.0),
                       (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        s = math.radians(20)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.04), (13, 0.0), (19, 0.04), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Head", f, (nod, 0, 0))

    build_organic_creature("Creature71", (0.62, 0.60, 0.58), elements, bones,
                            accessories, idle, walk, cam=0.7)


# =============================================================================
# Créature 72 — Marmotte : dressée sur les pattes arrière, siffle l'alerte.
# =============================================================================
def marmotte():
    K = 1.3
    elements = [
        ((0, 0.02, 0.28), 0.19, (0.90, 1.30, 0.85), K),
        ((0, -0.38, 0.34), 0.16, (0.90, 0.90, 0.85), K),  # tête
        ((0, -0.56, 0.32), 0.10, (0.85, 0.90, 0.72), K),  # museau
    ]
    for x, y in ((-0.13, -0.22), (0.13, -0.22), (-0.13, 0.26), (0.13, 0.26)):
        elements.append(((x, y, 0.18), 0.10, (0.95, 0.95, 1.10), K))
    elements.append(((0, 0.42, 0.24), 0.11, (0.90, 1.10, 0.85), 1.2))  # queue courte

    bones = {
        "Body": ("Root", (0, 0.24, 0.26), (0, -0.14, 0.30)),
        "Head": ("Body", (0, -0.14, 0.32), (0, -0.62, 0.34)),
        "Tail": ("Body", (0, 0.32, 0.24), (0, 0.55, 0.22)),
        "LegFL": ("Body", (-0.13, -0.22, 0.20), (-0.13, -0.22, 0.02)),
        "LegFR": ("Body", (0.13, -0.22, 0.20), (0.13, -0.22, 0.02)),
        "LegBL": ("Body", (-0.13, 0.26, 0.20), (-0.13, 0.26, 0.02)),
        "LegBR": ("Body", (0.13, 0.26, 0.20), (0.13, 0.26, 0.02)),
    }

    def accessories(fur, cream, dark, white):
        for sx in (-1, 1):
            sphere("Head", fur, (sx * 0.10, -0.42, 0.46), (0.032, 0.026, 0.032))  # oreille
            sphere("Head", dark, (sx * 0.10, -0.435, 0.46), (0.02, 0.015, 0.02))  # creux
            # Œil à 3 pièces (sclère + pupille + micro-reflet).
            sphere("Head", white, (sx * 0.08, -0.52, 0.38), (0.03, 0.026, 0.03))
            sphere("Head", dark, (sx * 0.09, -0.533, 0.376), (0.017, 0.014, 0.017))
            sphere("Head", white, (sx * 0.094, -0.538, 0.382), (0.006, 0.005, 0.006))
        # Truffe/bouche/dents remontées et reculées vers le centre du museau
        # (marge large) : posées au bord nominal du cœur organique, elles
        # pendaient dans le vide — la vraie surface d'un élément isolé
        # s'arrête bien avant ce bord (~75 % du rayon nominal, mesuré).
        sphere("Head", dark, (0, -0.62, 0.30), (0.028, 0.024, 0.024))  # truffe
        sphere("Head", cream, (0, -0.53, 0.26), (0.05, 0.05, 0.04))  # museau clair
        # Deux incisives de rongeur qui dépassent sous la lèvre — bien
        # visibles quand elle se dresse pour siffler l'alerte.
        sphere("Head", dark, (0, -0.58, 0.24), (0.04, 0.018, 0.012))  # fente
        teeth = material("Marmotte72Teeth", (0.85, 0.72, 0.30))
        for sx in (-1, 1):
            sphere("Head", teeth, (sx * 0.015, -0.585, 0.225), (0.011, 0.011, 0.02))
        for bone, x, y in (("LegFL", -0.13, -0.22), ("LegFR", 0.13, -0.22),
                           ("LegBL", -0.13, 0.26), ("LegBR", 0.13, 0.26)):
            sphere(bone, dark, (x, y, 0.02), (0.06, 0.06, 0.02))  # patte

    def idle(key_rot, key_loc):
        # Se dresse sur son séant façon vigie et siffle l'alerte : le corps
        # se redresse à la verticale, tient la pose, puis retombe.
        for f in (1, 40):
            for b in ("LegFL", "LegFR"):
                key_rot(b, f, (0, 0, 0))
        for f, up, dz in ((1, 0.0, 0.0), (10, -0.85, 0.10), (28, -0.85, 0.10),
                          (36, 0.0, 0.0), (40, 0.0, 0.0)):
            key_rot("Body", f, (up, 0, 0))
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, 0.0), (10, 0.75), (28, 0.75), (36, 0.0), (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, sw in ((1, 0.15), (20, -0.15), (40, 0.15)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        s = math.radians(22)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegFL", f, (a, 0, 0))
            key_rot("LegBR", f, (a, 0, 0))
            key_rot("LegFR", f, (-a, 0, 0))
            key_rot("LegBL", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.2), (13, -0.2), (24, 0.2)):
            key_rot("Tail", f, (0, 0, sw))

    build_organic_creature("Creature72", (0.48, 0.38, 0.24), elements, bones,
                            accessories, idle, walk, cam=0.6)


hippopotame()
capybara()
loutre()
koala()
marmotte()
print("PACK DONE")
