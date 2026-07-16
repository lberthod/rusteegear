# Génère le pack de décor « nature » de la démo MMORPG (prairie, forêt, rivière,
# cabane…) en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_nature_pack.py
#
# Sortie : assets/models/nature_*.glb (un fichier par type de décor, listés dans
# ASSETS tout en bas). Contraires aux créatures (gen_creature2.py) : pas de rig,
# pas d'animation — meshes statiques uniquement.
#
# Contraintes venues du moteur (src/scene/import.rs) :
# - `load_gltf` concatène les sommets **bruts** des primitives et ignore les
#   transforms de nœuds glTF → chaque asset est joint en un seul mesh puis son
#   transform est appliqué (transform_apply) avant export, pour que la géométrie
#   soit déjà en espace « objet final ».
# - Seule couleur lue : `base_color_factor` du matériau de chaque primitive
#   (pas de textures). Un objet joint garde ses matériaux → une primitive glTF
#   par matériau, donc les couleurs par partie survivent au join.
# - Repère : le sol du jeu est y=0 (glTF Y-up) = plan XY Blender (Z-up), les
#   assets posent leur base à z=0 Blender. Blender +Y devient glTF -Z (la
#   porte de la cabane est côté +Y Blender pour faire face au -Z du jeu).
#
# Tailles réelles en mètres : la scène les place à l'échelle 1.0 (les créatures,
# elles, sont réduites à 0.35 — sans rapport).

import math
import os
import random

import bpy

# `//` (relatif au .blend) ne pointe nulle part de fiable en headless sans
# fichier ouvert : on ancre la sortie sur l'emplacement de ce script.
OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260716)  # reproductible


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb):
    """Matériau Principled dont Base Color devient le base_color_factor glTF."""
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = 0.85
    return m


def assign(obj, material):
    obj.data.materials.clear()
    obj.data.materials.append(material)


def cube(name, material, location, scale):
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
    o = bpy.context.active_object
    o.name = name
    o.scale = scale
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


def cone(name, material, location, radius, depth, vertices=10):
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=radius, radius2=0.0, depth=depth, location=location
    )
    o = bpy.context.active_object
    o.name = name
    assign(o, material)
    return o


def blob(name, material, location, radius, squash=1.0, jitter=0.0):
    """Icosphère (option écrasée/irrégulière) — feuillages, buissons, rochers."""
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=radius, location=location)
    o = bpy.context.active_object
    o.name = name
    o.scale = (1.0, 1.0, squash)
    if jitter > 0.0:
        for v in o.data.vertices:
            v.co.x += rng.uniform(-jitter, jitter)
            v.co.y += rng.uniform(-jitter, jitter)
            v.co.z += rng.uniform(-jitter, jitter)
    assign(o, material)
    return o


def export(filename):
    """Joint tout, applique les transforms et exporte en GLB statique."""
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
    print(f"[nature] exporté {filename}")


# ---------------------------------------------------------------------------
# Palette commune (direction artistique de la carte 72×72) : chaque teinte est
# définie UNE fois et partagée par tous les générateurs — le hameau, la forêt
# et le promontoire se répondent au lieu d'empiler des couleurs ad hoc.
# Règles : silhouettes lisibles à 30 m, ≤ 3 teintes par asset, un seul accent
# rouge par zone (torii/bannière), jaune chaud réservé aux lumières (lanterne).
# ---------------------------------------------------------------------------

BROWN = (0.32, 0.22, 0.11)  # troncs, souches, poteaux
WOOD = (0.45, 0.30, 0.15)  # bois d'œuvre clair (murs, tabliers, caisses)
WOOD_DARK = (0.28, 0.18, 0.09)  # bois sombre (portes, rambardes)
LEAF_DARK = (0.18, 0.42, 0.16)  # feuillage principal
LEAF_LIGHT = (0.24, 0.50, 0.18)  # feuillage clair / buissons
NEEDLE = (0.10, 0.32, 0.14)  # aiguilles de sapin
STONE = (0.45, 0.44, 0.42)  # pierre claire (rochers, socles, margelles)
STONE_DARK = (0.36, 0.35, 0.34)  # pierre sombre
ROOF = (0.50, 0.22, 0.14)  # tuiles/bardeaux des toits en dur
THATCH = (0.62, 0.48, 0.20)  # chaume ocre (hutte, appentis)
ACCENT_RED = (0.72, 0.14, 0.12)  # accent du hameau (torii, bannière)
GLOW_YELLOW = (1.0, 0.78, 0.35)  # verre chaud des lanternes

# ---------------------------------------------------------------------------
# Assets
# ---------------------------------------------------------------------------


def gen_tree():
    """Feuillu ~3.5 m : tronc + 3 boules de feuillage."""
    trunk = mat("tronc", BROWN)
    leaf_a = mat("feuillage_a", LEAF_DARK)
    leaf_b = mat("feuillage_b", LEAF_LIGHT)
    cylinder("Tronc", trunk, (0, 0, 1.1), radius=0.18, depth=2.2)
    blob("Feuillage1", leaf_a, (0.0, 0.0, 2.8), radius=1.05, jitter=0.08)
    blob("Feuillage2", leaf_b, (0.55, 0.25, 2.35), radius=0.7, jitter=0.06)
    blob("Feuillage3", leaf_b, (-0.5, -0.2, 2.45), radius=0.65, jitter=0.06)
    export("nature_tree.glb")


def gen_pine():
    """Sapin ~4 m : tronc + 3 cônes empilés."""
    trunk = mat("tronc", BROWN)
    needles = mat("aiguilles", NEEDLE)
    cylinder("Tronc", trunk, (0, 0, 0.5), radius=0.14, depth=1.0)
    cone("Etage1", needles, (0, 0, 1.55), radius=1.05, depth=1.5)
    cone("Etage2", needles, (0, 0, 2.55), radius=0.8, depth=1.3)
    cone("Etage3", needles, (0, 0, 3.45), radius=0.5, depth=1.1)
    export("nature_pine.glb")


def gen_bush():
    """Buisson bas ~0.8 m."""
    leaf = mat("buisson", (0.20, 0.44, 0.17))
    leaf2 = mat("buisson_clair", (0.28, 0.52, 0.20))
    blob("Buisson1", leaf, (0, 0, 0.35), radius=0.45, squash=0.8, jitter=0.05)
    blob("Buisson2", leaf2, (0.35, 0.15, 0.28), radius=0.32, squash=0.75, jitter=0.04)
    blob("Buisson3", leaf, (-0.3, -0.1, 0.25), radius=0.28, squash=0.7, jitter=0.04)
    export("nature_bush.glb")


def gen_rock():
    """Rocher ~1.1 m, silhouette irrégulière. Assez haut pour rester visible des
    sondes des créatures (raycast horizontal à 0,6 m, cf. creature_wander_script) :
    un rocher qui culmine sous le rayon bloque physiquement sans jamais être
    « vu » — piège à patrouille."""
    stone = mat("pierre", STONE)
    stone2 = mat("pierre_sombre", STONE_DARK)
    blob("Rocher", stone, (0, 0, 0.55), radius=0.75, squash=0.75, jitter=0.12)
    blob("Rocher2", stone2, (0.55, 0.25, 0.25), radius=0.35, squash=0.7, jitter=0.08)
    export("nature_rock.glb")


def gen_flowers():
    """Parterre de fleurs : 6 tiges + corolles colorées dans un rayon de 0.7 m."""
    stem = mat("tige", (0.20, 0.45, 0.15))
    petals = [
        mat("petale_rouge", (0.85, 0.20, 0.18)),
        mat("petale_jaune", (0.95, 0.80, 0.20)),
        mat("petale_blanc", (0.92, 0.92, 0.88)),
        mat("petale_violet", (0.60, 0.30, 0.75)),
    ]
    for i in range(6):
        a = i * math.tau / 6 + rng.uniform(-0.4, 0.4)
        r = rng.uniform(0.15, 0.7)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.22, 0.38)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.015, depth=h, vertices=6)
        blob(f"Fleur{i}", petals[i % len(petals)], (x, y, h + 0.04), radius=0.07, squash=0.7)
    export("nature_flowers.glb")


def gen_rice():
    """Touffe de riz pour rizière : 7 plants fins vert-jaune, ~0.5 m."""
    plant = mat("riz", (0.55, 0.65, 0.25))
    plant2 = mat("riz_clair", (0.65, 0.72, 0.30))
    for i in range(7):
        a = i * math.tau / 7
        r = rng.uniform(0.05, 0.4)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.4, 0.55)
        cone(
            f"Plant{i}",
            plant if i % 2 == 0 else plant2,
            (x, y, h / 2),
            radius=0.06,
            depth=h,
            vertices=5,
        )
    export("nature_rice.glb")


def gen_cabin():
    """Cabane en rondins 3×2.5 m, toit à deux pans, porte côté +Y Blender
    (= -Z jeu : posée au nord de la route, sa porte regarde vers elle)."""
    log_m = mat("rondin", WOOD)
    roof_m = mat("toit", ROOF)
    door_m = mat("porte", WOOD_DARK)
    win_m = mat("fenetre", (0.55, 0.75, 0.85))
    # Murs en rondins empilés : 6 rangs de 2.1 m de haut. Rondins le long de X
    # (murs avant/arrière) et le long de Y (pignons), rayon 0.18.
    for i in range(6):
        z = 0.18 + i * 0.35
        # avant (+Y, côté porte) et arrière (-Y)
        cylinder(f"AvR{i}", log_m, (0, 1.25, z), 0.18, 3.2, rotation=(0, math.pi / 2, 0))
        cylinder(f"ArR{i}", log_m, (0, -1.25, z), 0.18, 3.2, rotation=(0, math.pi / 2, 0))
        # côtés (±X)
        cylinder(f"GaR{i}", log_m, (-1.5, 0, z), 0.18, 2.5, rotation=(math.pi / 2, 0, 0))
        cylinder(f"DrR{i}", log_m, (1.5, 0, z), 0.18, 2.5, rotation=(math.pi / 2, 0, 0))
    # Porte et fenêtre encastrées dans le mur avant (+Y), légèrement en saillie.
    cube("Porte", door_m, (0.0, 1.42, 0.85), (0.75, 0.12, 1.7))
    cube("Fenetre", win_m, (-1.0, 1.42, 1.35), (0.55, 0.10, 0.55))
    # Toit : deux pans inclinés qui se rejoignent au faîte (z=3.0).
    half_span = 1.05  # demi-portée horizontale d'un pan (débord compris)
    rise = 0.85
    slope = math.atan2(rise, half_span)
    pan_len = math.hypot(half_span, rise) + 0.25
    for side in (-1, 1):
        cube(
            f"Pan{side}",
            roof_m,
            (side * half_span / 2 * 1.55, 0.0, 2.1 + rise / 2),
            (pan_len, 3.1, 0.12),
        )
        # Rotation Y positive pour side=+1 : l'arête extérieure (+X) descend,
        # l'arête intérieure monte vers le faîte — signe inverse = toit en V.
        bpy.context.active_object.rotation_euler = (0, side * slope, 0)
    # Poutre faîtière : masque l'interstice où les deux pans se rejoignent.
    cube("Faite", roof_m, (0.0, 0.0, 2.1 + rise), (0.30, 3.25, 0.16))
    export("nature_cabin.glb")


def gen_bridge():
    """Pont de bois plat 4.5×1.6 m au-dessus de la rivière : tablier de planches,
    petites rampes aux extrémités, garde-corps. Orienté le long de X (la rivière
    de la démo coule nord-sud) — franchissable (collider trimesh statique)."""
    plank_m = mat("planche", WOOD)
    rail_m = mat("rambarde", WOOD_DARK)
    deck_h = 0.16
    for i in range(9):
        x = -2.0 + i * 0.5
        cube(f"Planche{i}", plank_m, (x, 0, deck_h), (0.46, 1.6, 0.08))
    # Rampes d'accès inclinées (pentes douces, la capsule du joueur les monte).
    for side in (-1, 1):
        r = cube(f"Rampe{side}", plank_m, (side * 2.65, 0, deck_h / 2), (0.9, 1.6, 0.07))
        # Même convention de signe que le toit de la cabane : extrémité
        # extérieure au sol, intérieure au niveau du tablier.
        r.rotation_euler = (0, side * math.atan2(deck_h, 0.9), 0)
    # Garde-corps : lisse haute + 3 poteaux par côté, et un flanc plein du sol à
    # 0,75 m — sans lui, le tablier (0,2 m) passe sous les sondes des créatures
    # (raycast horizontal à 0,6 m) et la lisse (0,85 m) au-dessus : le pont
    # bloquait physiquement en restant invisible aux rayons (patrouille figée à
    # pousser contre le tablier). Les deux extrémités restent ouvertes : joueur
    # et créatures franchissent le pont par les rampes.
    for sy in (-1, 1):
        cube(f"Lisse{sy}", rail_m, (0, sy * 0.75, 0.85), (4.4, 0.07, 0.07))
        cube(f"Flanc{sy}", plank_m, (0, sy * 0.78, 0.375), (4.4, 0.06, 0.75))
        for px in (-1.9, 0.0, 1.9):
            cube(f"Poteau{sy}{px}", rail_m, (px, sy * 0.75, 0.5), (0.09, 0.09, 0.75))
    export("nature_bridge.glb")


def gen_tree2():
    """Feuillu large ~3 m (variante de gen_tree, canopée aplatie plus étalée) :
    casse la répétition en forêt dense."""
    trunk = mat("tronc", BROWN)
    leaf_a = mat("feuillage_a", LEAF_DARK)
    leaf_b = mat("feuillage_b", LEAF_LIGHT)
    cylinder("Tronc", trunk, (0, 0, 0.9), radius=0.22, depth=1.8)
    blob("Canopee", leaf_b, (0.0, 0.0, 2.3), radius=1.35, squash=0.65, jitter=0.10)
    blob("Canopee2", leaf_a, (0.7, -0.4, 2.0), radius=0.8, squash=0.7, jitter=0.07)
    blob("Canopee3", leaf_a, (-0.65, 0.45, 2.1), radius=0.75, squash=0.7, jitter=0.07)
    export("nature_tree2.glb")


def gen_pine2():
    """Sapin élancé ~5 m (variante de gen_pine, plus haut et plus étroit)."""
    trunk = mat("tronc", BROWN)
    needles = mat("aiguilles", NEEDLE)
    cylinder("Tronc", trunk, (0, 0, 0.6), radius=0.13, depth=1.2)
    cone("Etage1", needles, (0, 0, 1.75), radius=0.85, depth=1.6)
    cone("Etage2", needles, (0, 0, 2.85), radius=0.65, depth=1.4)
    cone("Etage3", needles, (0, 0, 3.85), radius=0.45, depth=1.2)
    cone("Etage4", needles, (0, 0, 4.65), radius=0.28, depth=0.9)
    export("nature_pine2.glb")


def gen_stump():
    """Souche ~0.85 m, tronc plein : assez haute pour les sondes à 0,6 m."""
    trunk = mat("tronc", BROWN)
    top = mat("coeur", (0.62, 0.48, 0.30))
    cylinder("Souche", trunk, (0, 0, 0.42), radius=0.38, depth=0.85, vertices=9)
    cylinder("Coeur", top, (0, 0, 0.86), radius=0.32, depth=0.06, vertices=9)
    blob("Racine", trunk, (0.38, 0.1, 0.12), radius=0.16, squash=0.6, jitter=0.04)
    export("nature_stump.glb")


def gen_hut():
    """Hutte ronde au toit de chaume ~3 m — silhouette distincte de la cabane
    en rondins (murs cylindriques + grand cône ocre)."""
    wall_m = mat("torchis", (0.55, 0.45, 0.32))
    thatch_m = mat("chaume", THATCH)
    door_m = mat("porte", WOOD_DARK)
    cylinder("Murs", wall_m, (0, 0, 0.9), radius=1.5, depth=1.8, vertices=12)
    cone("Toit", thatch_m, (0, 0, 2.4), radius=2.1, depth=1.6, vertices=12)
    # Porte côté +Y Blender (= -Z jeu, face à la route au sud du hameau).
    cube("Porte", door_m, (0.0, 1.48, 0.75), (0.7, 0.14, 1.5))
    export("nature_hut.glb")


def gen_tower():
    """Tour de guet ~4.2 m sur socle de pierre plein (1.6 m de côté) : le
    landmark du promontoire, lisible de l'autre bout de la carte. Le socle
    plein garantit des flancs visibles aux sondes à 0,6 m."""
    stone = mat("pierre", STONE)
    wood = mat("bois", WOOD)
    roof_m = mat("toit", ROOF)
    cube("Socle", stone, (0, 0, 0.5), (1.6, 1.6, 1.0))
    cube("Fut", wood, (0, 0, 1.9), (1.15, 1.15, 1.8))
    cube("Plateforme", wood, (0, 0, 2.9), (1.7, 1.7, 0.16))
    for sx in (-1, 1):
        for sy in (-1, 1):
            cube(f"Pilier{sx}{sy}", wood, (sx * 0.72, sy * 0.72, 3.35), (0.12, 0.12, 0.9))
    cube("Parapet1", wood, (0, 0.8, 3.15), (1.7, 0.08, 0.35))
    cube("Parapet2", wood, (0, -0.8, 3.15), (1.7, 0.08, 0.35))
    cube("Parapet3", wood, (0.8, 0, 3.15), (0.08, 1.7, 0.35))
    cube("Parapet4", wood, (-0.8, 0, 3.15), (0.08, 1.7, 0.35))
    cone("Toit", roof_m, (0, 0, 4.15), radius=1.35, depth=0.9, vertices=8)
    export("nature_tower.glb")


def gen_fence():
    """Segment de clôture 2 m : 2 poteaux + 2 lisses, dont une lisse PLEINE
    couvrant la bande 0,35–0,85 m — vue par les sondes à 0,6 m (des barreaux
    ajourés laisseraient passer les rayons : piège à patrouille)."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    for sx in (-1, 1):
        cube(f"Poteau{sx}", dark, (sx * 0.95, 0, 0.5), (0.12, 0.12, 1.0))
    cube("LissePleine", wood, (0, 0, 0.6), (2.0, 0.07, 0.5))
    cube("LisseHaute", dark, (0, 0, 0.95), (2.0, 0.06, 0.08))
    export("nature_fence.glb")


def gen_lantern():
    """Lanterne sur poteau ~1.9 m : socle de pierre PLEIN (r 0.4, h 1.0) pour
    rester visible des sondes — un poteau nu passerait entre les 3 rayons."""
    stone = mat("pierre", STONE)
    dark = mat("bois_sombre", WOOD_DARK)
    glow = mat("verre_chaud", GLOW_YELLOW)
    cylinder("Socle", stone, (0, 0, 0.5), radius=0.4, depth=1.0, vertices=8)
    cube("Poteau", dark, (0, 0, 1.25), (0.14, 0.14, 0.5))
    cube("Cage", dark, (0, 0, 1.62), (0.4, 0.4, 0.45))
    cube("Verre", glow, (0, 0, 1.62), (0.32, 0.32, 0.34))
    cone("Chapeau", dark, (0, 0, 1.95), radius=0.34, depth=0.25, vertices=8)
    export("nature_lantern.glb")


def gen_well():
    """Puits du hameau : margelle de pierre pleine (r 0.8, h 1.0) + portique et
    toit. La margelle fait le flanc plein exigé par les sondes."""
    stone = mat("pierre", STONE)
    wood = mat("bois", WOOD)
    roof_m = mat("toit", ROOF)
    cylinder("Margelle", stone, (0, 0, 0.5), radius=0.8, depth=1.0, vertices=10)
    cylinder("Fond", mat("pierre_sombre", STONE_DARK), (0, 0, 1.01), radius=0.62, depth=0.04, vertices=10)
    for sx in (-1, 1):
        cube(f"Montant{sx}", wood, (sx * 0.7, 0, 1.5), (0.12, 0.12, 1.2))
    cube("Traverse", wood, (0, 0, 2.05), (1.55, 0.10, 0.10))
    cone("Toit", roof_m, (0, 0, 2.35), radius=1.0, depth=0.55, vertices=8)
    export("nature_well.glb")


def gen_cart():
    """Charrette à bras : caisse pleine (flancs de 0,35 à 1,05 m — visibles des
    sondes) + deux roues + brancards."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cube("Caisse", wood, (0, 0, 0.7), (1.9, 1.1, 0.7))
    for sy in (-1, 1):
        cylinder(
            f"Roue{sy}",
            dark,
            (0.0, sy * 0.62, 0.42),
            radius=0.42,
            depth=0.10,
            vertices=10,
            rotation=(math.pi / 2, 0, 0),
        )
        cube(f"Brancard{sy}", dark, (-1.35, sy * 0.38, 0.55), (1.0, 0.08, 0.08))
    export("nature_cart.glb")


def gen_signpost():
    """Panneau de direction (non solide : trop fin pour les sondes) : poteau +
    deux flèches. Posé aux croisements de routes."""
    dark = mat("bois_sombre", WOOD_DARK)
    wood = mat("bois", WOOD)
    cube("Poteau", dark, (0, 0, 1.0), (0.12, 0.12, 2.0))
    cube("Fleche1", wood, (0.35, 0, 1.75), (0.85, 0.06, 0.24))
    f2 = cube("Fleche2", wood, (-0.3, 0, 1.45), (0.7, 0.06, 0.22))
    f2.rotation_euler = (0, 0, math.radians(35))
    export("nature_signpost.glb")


def gen_torii():
    """Arche d'entrée du hameau (accent rouge de la zone) : deux piliers pleins
    (r 0.2 — vus des sondes), linteau à 2,45 m (le joueur passe dessous)."""
    red = mat("laque_rouge", ACCENT_RED)
    dark = mat("bois_sombre", WOOD_DARK)
    for sx in (-1, 1):
        cylinder(f"Pilier{sx}", red, (sx * 1.15, 0, 1.25), radius=0.2, depth=2.5, vertices=10)
        cube(f"Socle{sx}", dark, (sx * 1.15, 0, 0.14), (0.55, 0.55, 0.28))
    cube("Linteau", red, (0, 0, 2.62), (3.3, 0.24, 0.24))
    cube("LinteauHaut", dark, (0, 0, 2.88), (3.7, 0.3, 0.22))
    export("nature_torii.glb")


def gen_woodpile():
    """Tas de bois 0,95 m : rondins empilés en bloc plein (flancs visibles)."""
    trunk = mat("tronc", BROWN)
    heart = mat("coeur", (0.62, 0.48, 0.30))
    n_per_row = [4, 3, 2]
    for row, n in enumerate(n_per_row):
        z = 0.22 + row * 0.34
        for i in range(n):
            y = (i - (n - 1) / 2) * 0.4
            cylinder(
                f"Rondin{row}_{i}",
                trunk if (row + i) % 2 == 0 else heart,
                (0, y, z),
                radius=0.19,
                depth=1.5,
                vertices=8,
                rotation=(0, math.pi / 2, 0),
            )
    export("nature_woodpile.glb")


def gen_boat():
    """Barque échouée ~2.6 m : coque à flancs pleins (0 → 0,7 m) + banc."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cube("Fond", dark, (0, 0, 0.1), (2.4, 0.9, 0.2))
    for sy in (-1, 1):
        f = cube(f"Flanc{sy}", wood, (0, sy * 0.48, 0.38), (2.6, 0.1, 0.6))
        f.rotation_euler = (sy * math.radians(-8), 0, 0)
    for sx in (-1, 1):
        p = cube(f"Etrave{sx}", wood, (sx * 1.28, 0, 0.38), (0.12, 0.95, 0.6))
        p.rotation_euler = (0, sx * math.radians(18), 0)
    cube("Banc", dark, (0, 0, 0.5), (0.35, 0.9, 0.08))
    export("nature_boat.glb")


def gen_lily():
    """Nénuphars (non solides, posés sur l'eau) : 4 feuilles plates + 1 fleur."""
    leaf = mat("feuille_eau", (0.16, 0.45, 0.22))
    petal = mat("petale_blanc", (0.92, 0.92, 0.88))
    for i in range(4):
        a = i * math.tau / 4 + 0.4
        r = 0.55 if i % 2 == 0 else 0.3
        cylinder(
            f"Feuille{i}",
            leaf,
            (r * math.cos(a), r * math.sin(a), 0.02),
            radius=0.3 - 0.05 * (i % 2),
            depth=0.04,
            vertices=9,
        )
    blob("Fleur", petal, (0.1, 0.05, 0.1), radius=0.12, squash=0.7)
    export("nature_lily.glb")


def gen_reeds():
    """Roseaux de berge ~1.2 m (non solides) : tiges fines + épis bruns."""
    stem = mat("tige_roseau", (0.30, 0.48, 0.20))
    head = mat("epi", (0.42, 0.28, 0.14))
    for i in range(6):
        a = i * math.tau / 6
        r = rng.uniform(0.08, 0.35)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.8, 1.2)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.025, depth=h, vertices=5)
        cylinder(f"Epi{i}", head, (x, y, h + 0.12), radius=0.05, depth=0.24, vertices=5)
    export("nature_reeds.glb")


ASSETS = [
    gen_tree,
    gen_pine,
    gen_bush,
    gen_rock,
    gen_flowers,
    gen_rice,
    gen_cabin,
    gen_bridge,
    gen_tree2,
    gen_pine2,
    gen_stump,
    gen_hut,
    gen_tower,
    gen_fence,
    gen_lantern,
    gen_well,
    gen_cart,
    gen_signpost,
    gen_torii,
    gen_woodpile,
    gen_boat,
    gen_lily,
    gen_reeds,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[nature] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
