# Pack « pierre & mystique » : 11 éléments de décor minéral et sacré pour
# habiller les collines, les ruines et les abords du torii (aucun doublon avec
# les nature_* existants) :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_stone_pack.py
#
# Sortie : assets/models/nature_*.glb + nature_*_preview.png.
#
# Contraintes moteur (détail dans l'en-tête de gen_nature_pack.py) :
# meshes statiques joints, transform_apply, base_color_factor seulement
# (normales lues → shade_smooth utile), base à z=0 ; les assets solides
# présentent un flanc plein visible du raycast des créatures à 0,6 m.

import math
import os
import random

import bpy
import mathutils

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260718)  # reproductible


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb, roughness=0.85):
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = roughness
    return m


def assign(obj, material):
    obj.data.materials.clear()
    obj.data.materials.append(material)


def cube(name, material, location, scale, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
    o = bpy.context.active_object
    o.name = name
    o.scale = scale
    o.rotation_euler = rotation
    assign(o, material)
    return o


def cylinder(name, material, location, radius, depth, vertices=10, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=location, rotation=rotation
    )
    o = bpy.context.active_object
    o.name = name
    assign(o, material)
    return o


def cone(name, material, location, radius, depth, vertices=10, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=radius, radius2=0.0, depth=depth, location=location,
        rotation=rotation,
    )
    o = bpy.context.active_object
    o.name = name
    assign(o, material)
    return o


def taper(name, material, location, r_bottom, r_top, depth, vertices=12, rotation=(0, 0, 0)):
    """Cône tronqué lissé (fûts de colonne, troncs, lanternes)."""
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=r_bottom, radius2=r_top, depth=depth, location=location,
        rotation=rotation,
    )
    o = bpy.context.active_object
    o.name = name
    bpy.ops.object.shade_smooth()
    assign(o, material)
    return o


def blob(name, material, location, radius, squash=1.0, jitter=0.0, subdiv=1, smooth=False):
    """Icosphère bosselée : la brique de base des rochers."""
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=subdiv, radius=radius, location=location)
    o = bpy.context.active_object
    o.name = name
    o.scale = (1.0, 1.0, squash)
    if jitter > 0.0:
        for v in o.data.vertices:
            v.co.x += rng.uniform(-jitter, jitter)
            v.co.y += rng.uniform(-jitter, jitter)
            v.co.z += rng.uniform(-jitter, jitter)
    if smooth:
        bpy.ops.object.shade_smooth()
    assign(o, material)
    return o


def rock(name, material, location, radius, squash=0.7, stretch=(1.0, 1.0)):
    """Rocher : icosphère aplatie, étirée en x/y et bosselée."""
    o = blob(name, material, location, radius, squash=squash, jitter=radius * 0.18)
    o.scale = (stretch[0], stretch[1], squash)
    return o


def export_and_preview(filename):
    """Joint tout, applique les transforms, exporte en GLB puis rend un preview."""
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    for o in bpy.context.scene.objects:
        o.select_set(o in meshes)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.export_scene.gltf(
        filepath=OUT_DIR + filename,
        export_format="GLB",
        export_animations=False,
        export_skins=False,
        export_apply=True,
        export_yup=True,
    )
    print(f"[pierre] exporté {filename}")

    obj = bpy.context.active_object
    bpy.context.view_layer.update()
    pts = [(obj.matrix_world @ v.co) for v in obj.data.vertices]
    min_z = min(p.z for p in pts)
    max_z = max(p.z for p in pts)
    span = max(
        max(p.x for p in pts) - min(p.x for p in pts),
        max(p.y for p in pts) - min(p.y for p in pts),
        max_z - min_z,
    )
    target = (0.0, 0.0, (min_z + max_z) / 2)
    dist = span * 1.9 + 1.0
    cam_loc = (dist * 0.72, -dist * 0.72, target[2] + dist * 0.45)
    bpy.ops.object.camera_add(location=cam_loc)
    cam = bpy.context.active_object
    direction = mathutils.Vector(target) - mathutils.Vector(cam_loc)
    cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    scene = bpy.context.scene
    scene.camera = cam
    bpy.ops.object.light_add(type="SUN", location=(4, -3, 8))
    bpy.context.active_object.data.energy = 3.0
    bpy.context.active_object.rotation_euler = (math.radians(35), 0, math.radians(40))
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = OUT_DIR + filename.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print(f"[pierre] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette minérale + accents.
# ---------------------------------------------------------------------------

STONE = (0.46, 0.45, 0.42)  # granit clair
STONE_DARK = (0.34, 0.33, 0.31)  # granit ombré
STONE_OLD = (0.52, 0.50, 0.42)  # pierre ancienne blondie (ruines)
MOSS = (0.24, 0.42, 0.18)  # mousse des pierres (vert feuillage partagé)
CRYSTAL = (0.35, 0.62, 0.85)  # cristal bleu glacier (accent froid)
CRYSTAL_DEEP = (0.22, 0.42, 0.72)
WOOD = (0.32, 0.22, 0.11)  # bois brun partagé
WOOD_PALE = (0.55, 0.42, 0.24)  # bois clair sculpté (totem, ruche)
STRAW = (0.72, 0.60, 0.30)  # paille de la ruche
ROOF_RED = (0.55, 0.20, 0.16)  # toit du kiosque (tuile sombre)
OCHRE = (0.68, 0.42, 0.16)  # motifs peints du totem
TEAL = (0.16, 0.44, 0.42)  # motifs peints du totem (froid)


# ---------------------------------------------------------------------------
# Pierres levées
# ---------------------------------------------------------------------------


def gen_menhir():
    """Menhir ~2,4 m : pierre levée massive légèrement inclinée, mousse au
    pied — le repère solitaire des collines. Solide : flanc plein à 0,6 m."""
    stone = mat("pierre", STONE)
    stone_d = mat("pierre_sombre", STONE_DARK)
    moss = mat("mousse", MOSS)
    o = cube("Menhir", stone, (0, 0, 1.15), (0.7, 0.5, 2.35), rotation=(0.05, 0.04, 0.2))
    for v in o.data.vertices:
        v.co.x += rng.uniform(-0.05, 0.05)
        v.co.y += rng.uniform(-0.05, 0.05)
        if v.co.z > 0:  # sommet rétréci : silhouette de dent de pierre
            v.co.x *= 0.6
            v.co.y *= 0.7
    rock("Pied1", stone_d, (0.45, 0.2, 0.14), radius=0.24)
    rock("Pied2", stone_d, (-0.4, -0.25, 0.11), radius=0.19)
    blob("Mousse", moss, (0.1, 0.28, 0.35), radius=0.28, squash=0.5, jitter=0.05, subdiv=2)
    export_and_preview("nature_menhir.glb")


def gen_cairn():
    """Cairn ~1,1 m : pierres empilées en pyramide libre — le balisage des
    sentiers. Solide : la base large couvre la hauteur des sondes."""
    stone = mat("pierre", STONE)
    stone_d = mat("pierre_sombre", STONE_DARK)
    layers = [(0.0, 0.42, 3), (0.28, 0.32, 3), (0.55, 0.24, 2), (0.78, 0.17, 1), (0.95, 0.11, 1)]
    for li, (z, r, count) in enumerate(layers):
        for k in range(count):
            a = k * math.tau / max(count, 1) + li * 0.9
            off = 0.16 if count > 1 else 0.0
            m = stone if (li + k) % 2 == 0 else stone_d
            rock(
                f"Pierre{li}_{k}", m,
                (off * math.cos(a), off * math.sin(a), z + r * 0.55),
                radius=r, squash=0.75, stretch=(rng.uniform(0.9, 1.2), rng.uniform(0.85, 1.1)),
            )
    export_and_preview("nature_cairn.glb")


def gen_stone_circle():
    """Cromlech ~0,9 m : cercle de 7 pierres dressées autour d'une dalle —
    le lieu de pouvoir des landes. Solide pierre par pierre (les sondes
    passent entre deux pierres, comme entre deux troncs)."""
    stone = mat("pierre", STONE)
    stone_d = mat("pierre_sombre", STONE_DARK)
    moss = mat("mousse", MOSS)
    for i in range(7):
        a = i * math.tau / 7
        h = rng.uniform(0.7, 1.0)
        m = stone if i % 2 == 0 else stone_d
        o = cube(
            f"Pierre{i}", m,
            (1.6 * math.cos(a), 1.6 * math.sin(a), h / 2),
            (0.34, 0.24, h), rotation=(0, rng.uniform(-0.08, 0.08), a + rng.uniform(-0.2, 0.2)),
        )
        for v in o.data.vertices:
            v.co.x += rng.uniform(-0.03, 0.03)
            if v.co.z > 0:
                v.co.x *= 0.75
                v.co.y *= 0.8
    cylinder("Dalle", stone_d, (0, 0, 0.09), radius=0.55, depth=0.18, vertices=9)
    blob("Mousse", moss, (0.2, -0.15, 0.18), radius=0.2, squash=0.4, jitter=0.03, subdiv=2)
    export_and_preview("nature_stone_circle.glb")


# ---------------------------------------------------------------------------
# Ruines
# ---------------------------------------------------------------------------


def gen_ruin_arch():
    """Arche en ruine ~2,8 m : deux piédroits de blocs et un linteau rompu —
    la porte d'un domaine disparu. Solide : chaque piédroit est plein
    du sol au sommet."""
    stone = mat("pierre_ancienne", STONE_OLD)
    stone_d = mat("pierre_sombre", STONE_DARK)
    moss = mat("mousse", MOSS)
    for side, sx in (("G", -1.1), ("D", 1.1)):
        for k in range(4):
            w = 0.62 - 0.04 * k
            m = stone if k % 2 == 0 else stone_d
            # Blocs plus hauts que leur pas d'empilement : ils se chevauchent,
            # aucun jour ne s'ouvre malgré le léger désaxage.
            cube(
                f"Bloc{side}{k}", m,
                (sx + rng.uniform(-0.03, 0.03), rng.uniform(-0.03, 0.03), 0.33 + k * 0.6),
                (w, 0.55, 0.68), rotation=(0, 0, rng.uniform(-0.03, 0.03)),
            )
    # Linteau rompu : un tronçon encore en place côté gauche, l'autre au sol.
    cube("LinteauHaut", stone, (-0.55, 0, 2.5), (1.3, 0.5, 0.42), rotation=(0, 0.08, 0))
    cube("LinteauTombe", stone_d, (0.9, 0.9, 0.2), (1.15, 0.45, 0.4),
         rotation=(0, 0.12, 0.7))
    rock("Gravat1", stone_d, (0.2, -0.7, 0.12), radius=0.2)
    rock("Gravat2", stone, (-0.5, 0.75, 0.1), radius=0.16)
    blob("Mousse", moss, (-1.1, 0.1, 1.9), radius=0.26, squash=0.5, jitter=0.04, subdiv=2)
    export_and_preview("nature_ruin_arch.glb")


def gen_ruin_column():
    """Colonne brisée ~1,8 m : fût cannelé rompu sur socle carré, un tambour
    renversé à côté — le vestige à semer autour de l'arche. Solide."""
    stone = mat("pierre_ancienne", STONE_OLD)
    stone_d = mat("pierre_sombre", STONE_DARK)
    moss = mat("mousse", MOSS)
    cube("Socle", stone_d, (0, 0, 0.14), (0.95, 0.95, 0.28))
    taper("Fut", stone, (0, 0, 1.05), r_bottom=0.32, r_top=0.27, depth=1.55, vertices=12)
    # Cassure : sommet biseauté irrégulier.
    o = bpy.context.active_object
    for v in o.data.vertices:
        if v.co.z > 0.5:
            v.co.z += rng.uniform(-0.16, 0.05) + 0.12 * v.co.x
    # Tambour tombé, couché sur le flanc.
    taper("Tambour", stone, (1.15, -0.5, 0.26), r_bottom=0.27, r_top=0.25, depth=0.7,
          vertices=12, rotation=(math.pi / 2, 0, 0.6))
    rock("Gravat", stone_d, (-0.75, 0.55, 0.1), radius=0.16)
    blob("Mousse", moss, (0.05, -0.28, 0.32), radius=0.22, squash=0.5, jitter=0.04, subdiv=2)
    export_and_preview("nature_ruin_column.glb")


# ---------------------------------------------------------------------------
# Mystique
# ---------------------------------------------------------------------------


def gen_crystal_cluster():
    """Cristaux ~1,3 m : bouquet de prismes bleu glacier sur socle rocheux —
    l'accent froid des grottes et clairières de nuit. Solide : le socle et
    le grand prisme couvrent 0,6 m."""
    stone_d = mat("pierre_sombre", STONE_DARK)
    crystal = mat("cristal", CRYSTAL, roughness=0.25)
    deep = mat("cristal_profond", CRYSTAL_DEEP, roughness=0.3)
    rock("Socle", stone_d, (0, 0, 0.16), radius=0.5, squash=0.5, stretch=(1.25, 1.05))
    shards = [
        (0.0, 0.0, 0.13, 1.25, 0.0, 0.0, crystal),
        (0.3, 0.15, 0.09, 0.8, 0.35, 0.5, deep),
        (-0.28, 0.1, 0.08, 0.7, -0.3, 2.2, crystal),
        (0.05, -0.3, 0.07, 0.55, 0.3, 4.0, deep),
        (-0.12, -0.14, 0.05, 0.4, -0.25, 5.2, crystal),
    ]
    for i, (x, y, r, h, tilt, spin, m) in enumerate(shards):
        # Un seul prisme effilé par cristal : la pointe fait corps avec le fût,
        # rien ne peut se détacher quand le prisme est incliné.
        bpy.ops.mesh.primitive_cone_add(
            vertices=6, radius1=r, radius2=r * 0.25, depth=h,
            location=(x, y, 0.2 + h / 2), rotation=(tilt, 0, spin),
        )
        o = bpy.context.active_object
        o.name = f"Prisme{i}"
        assign(o, m)
        # Pointe posée au sommet réel du prisme incliné (euler XYZ : offset du
        # sommet = Rz(spin)·Rx(tilt)·(0,0,h/2) depuis le centre).
        tip = mathutils.Euler((tilt, 0, spin), "XYZ").to_matrix() @ mathutils.Vector(
            (0, 0, h * 0.42)
        )
        cone(f"Pointe{i}", m, (x + tip.x, y + tip.y, 0.2 + h / 2 + tip.z),
             radius=r * 0.27, depth=0.3, vertices=6, rotation=(tilt, 0, spin))
    export_and_preview("nature_crystal_cluster.glb")


def gen_shrine():
    """Autel ~1,4 m : table de pierre sur deux montants, offrande de fruits et
    petite stèle gravée — le sanctuaire de chemin. Solide : montants pleins."""
    stone = mat("pierre", STONE)
    stone_d = mat("pierre_sombre", STONE_DARK)
    moss = mat("mousse", MOSS)
    fruit = mat("offrande", (0.68, 0.16, 0.20))
    cube("MontantG", stone_d, (-0.5, 0, 0.42), (0.3, 0.6, 0.84))
    cube("MontantD", stone_d, (0.5, 0, 0.42), (0.3, 0.6, 0.84))
    cube("Table", stone, (0, 0, 0.94), (1.5, 0.8, 0.16))
    cube("Stele", stone, (0, -0.15, 1.4), (0.4, 0.12, 0.8), rotation=(0.06, 0, 0))
    cone("Chapeau", stone_d, (0, -0.17, 1.86), radius=0.3, depth=0.24, vertices=4,
         rotation=(0, 0, math.pi / 4))
    for i in range(3):
        blob(f"Offrande{i}", fruit, (-0.3 + i * 0.3, 0.22, 1.08), radius=0.07,
             subdiv=2, smooth=True)
    blob("Mousse", moss, (-0.55, 0.2, 0.86), radius=0.18, squash=0.45, jitter=0.03, subdiv=2)
    export_and_preview("nature_shrine.glb")


def gen_stone_lantern():
    """Lanterne de pierre ~1,6 m : fût, loge à feu ajourée et chapeau courbe —
    l'allée du torii (pierre, distincte des lanternes de bois). Solide."""
    stone = mat("pierre", STONE)
    stone_d = mat("pierre_sombre", STONE_DARK)
    glow = mat("feu_lanterne", (0.95, 0.72, 0.30), roughness=0.5)
    cube("Base", stone_d, (0, 0, 0.09), (0.62, 0.62, 0.18))
    # Fût cylindrique en normales plates : les facettes nettes évitent l'effet
    # « cire fondue » du shade_smooth sur un si petit rayon.
    cylinder("Fut", stone, (0, 0, 0.55), radius=0.12, depth=0.8, vertices=8)
    cube("Plateau", stone_d, (0, 0, 0.9), (0.5, 0.5, 0.12))
    # Loge à feu : quatre piliers d'angle autour du coeur lumineux, tous ancrés
    # dans le plateau et montant jusque sous le chapeau.
    blob("Feu", glow, (0, 0, 1.14), radius=0.14, smooth=True)
    for i in range(4):
        a = i * math.tau / 4 + math.pi / 4
        cube(f"Pilier{i}", stone, (0.19 * math.cos(a), 0.19 * math.sin(a), 1.13),
             (0.09, 0.09, 0.5))
    cone("Chapeau", stone_d, (0, 0, 1.46), radius=0.42, depth=0.3, vertices=4,
         rotation=(0, 0, math.pi / 4))
    blob("Perle", stone, (0, 0, 1.6), radius=0.08, smooth=True)
    export_and_preview("nature_stone_lantern.glb")


# ---------------------------------------------------------------------------
# Village
# ---------------------------------------------------------------------------


def gen_beehive():
    """Ruche paillée ~1,2 m : dôme de paille torsadée sur table de bois —
    le rucher près du verger (répond aux abeilles de la faune). Solide :
    la table et le dôme couvrent 0,6 m."""
    wood = mat("bois", WOOD)
    straw = mat("paille", STRAW)
    straw_d = mat("paille_sombre", (0.60, 0.48, 0.22))
    for sx, sy in ((-0.3, -0.22), (0.3, -0.22), (-0.3, 0.22), (0.3, 0.22)):
        cube(f"Pied{sx}_{sy}", wood, (sx, sy, 0.22), (0.09, 0.09, 0.44))
    cube("Table", wood, (0, 0, 0.49), (0.9, 0.7, 0.1))
    # Dôme : anneaux de paille empilés, rétrécis vers le haut.
    rings = [(0.60, 0.36), (0.74, 0.33), (0.88, 0.29), (1.00, 0.23), (1.10, 0.15)]
    for i, (z, r) in enumerate(rings):
        m = straw if i % 2 == 0 else straw_d
        o = cylinder(f"Anneau{i}", m, (0, 0, z), radius=r, depth=0.15, vertices=12)
        bpy.ops.object.shade_smooth()
    blob("Calotte", straw, (0, 0, 1.19), radius=0.13, squash=0.7, smooth=True)
    # Trou d'envol : petite arche sombre au bas du dôme.
    cube("Entree", mat("entree_ruche", (0.12, 0.08, 0.05)), (0, -0.34, 0.62),
         (0.12, 0.06, 0.09))
    export_and_preview("nature_beehive.glb")


def gen_totem():
    """Totem ~2,6 m : trois figures de bois sculpté empilées, ailes déployées,
    motifs ocre et sarcelle — le gardien peint des clairières. Solide."""
    wood = mat("bois_clair", WOOD_PALE)
    wood_d = mat("bois", WOOD)
    ochre = mat("motif_ocre", OCHRE)
    teal = mat("motif_sarcelle", TEAL)
    cylinder("Socle", wood_d, (0, 0, 0.15), radius=0.42, depth=0.3, vertices=10)
    # Trois tambours-figures.
    for i, (z, r, m) in enumerate([(0.75, 0.34, wood), (1.45, 0.30, wood), (2.1, 0.26, wood)]):
        o = cylinder(f"Figure{i}", m, (0, 0, z), radius=r, depth=0.66, vertices=10)
        bpy.ops.object.shade_smooth()
        # Bec saillant et yeux peints, face -y (face caméra du preview).
        # Rx(+90°) envoie l'axe +z du cône vers -y : la pointe regarde la
        # face avant, la base reste noyée dans le tambour.
        cone(f"Bec{i}", ochre, (0, -r + 0.04, z - 0.05), radius=0.1, depth=0.34,
             vertices=6, rotation=(math.pi / 2, 0, 0))
        for sx in (-0.12, 0.12):
            blob(f"Oeil{i}{sx}", teal, (sx, -r + 0.02, z + 0.18), radius=0.06, smooth=True)
    # Ailes déployées au sommet.
    cube("AileG", teal, (-0.62, 0, 2.38), (0.85, 0.1, 0.3), rotation=(0, -0.35, 0))
    cube("AileD", teal, (0.62, 0, 2.38), (0.85, 0.1, 0.3), rotation=(0, 0.35, 0))
    cone("Coiffe", ochre, (0, 0, 2.62), radius=0.24, depth=0.4, vertices=8)
    export_and_preview("nature_totem.glb")


def gen_gazebo():
    """Kiosque ~2,9 m : plancher octogonal, six poteaux, toit de tuiles à
    lanterneau — l'abri du parc du village. Solide poteau par poteau (les
    sondes passent entre deux poteaux, l'intérieur reste traversable)."""
    wood = mat("bois", WOOD)
    wood_p = mat("bois_clair", WOOD_PALE)
    roof = mat("tuile", ROOF_RED)
    cylinder("Plancher", wood_p, (0, 0, 0.1), radius=1.6, depth=0.2, vertices=8)
    cylinder("Marche", wood, (0, -1.65, 0.06), radius=0.35, depth=0.12, vertices=8)
    for i in range(6):
        a = i * math.tau / 6 + math.pi / 6
        x, y = 1.35 * math.cos(a), 1.35 * math.sin(a)
        cylinder(f"Poteau{i}", wood, (x, y, 1.2), radius=0.09, depth=2.4, vertices=8)
        # Garde-corps bas entre poteaux (sauf face d'entrée -y).
        a2 = (i + 1) * math.tau / 6 + math.pi / 6
        x2, y2 = 1.35 * math.cos(a2), 1.35 * math.sin(a2)
        mid = ((x + x2) / 2, (y + y2) / 2, 0.55)
        if mid[1] > -1.0:  # l'entrée au sud reste ouverte
            length = math.dist((x, y), (x2, y2))
            cube(f"Lisse{i}", wood_p, mid, (length * 0.92, 0.07, 0.1),
                 rotation=(0, 0, math.atan2(y2 - y, x2 - x)))
    # Ceinture octogonale reliant les têtes de poteaux, sous le toit relevé :
    # aucun jour vu du sol, aucun poteau ne perce le pan du cône (le pan passe
    # à z≈2,54 au rayon des poteaux, têtes à 2,40 ; ceinture haute à 2,45
    # sous le pan à son rayon, z≈2,47).
    cylinder("Ceinture", wood, (0, 0, 2.36), radius=1.5, depth=0.18, vertices=8)
    cone("Toit", roof, (0, 0, 2.7), radius=2.0, depth=0.9, vertices=8)
    cylinder("Lanterneau", wood_p, (0, 0, 3.2), radius=0.22, depth=0.3, vertices=8)
    cone("Chapeau", roof, (0, 0, 3.5), radius=0.34, depth=0.3, vertices=8)
    export_and_preview("nature_gazebo.glb")


ASSETS = [
    gen_menhir,
    gen_cairn,
    gen_stone_circle,
    gen_ruin_arch,
    gen_ruin_column,
    gen_crystal_cluster,
    gen_shrine,
    gen_stone_lantern,
    gen_beehive,
    gen_totem,
    gen_gazebo,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[pierre] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
