# Boîte à outils + palette communes du pack « hameau maison » (gen_hamlet_*.py).
#
# Recrée en style maison, procédural, les 39 assets déjà présents en version
# tierce retraitée (village_*.glb, Medieval Village Pack / Quaternius / CC0,
# import_village_pack.py) — voir la mémoire projet
# `charte-graphique-assets-maison` pour les règles complètes. Aucune géométrie
# tierce n'est réutilisée ici : ce module ne fait qu'imiter les conventions
# déjà en place dans gen_nature_pack.py / gen_stone_pack.py.
#
# Ce module n'est PAS exécutable seul (pas de bloc `ASSETS`/boucle en bas) :
# chaque gen_hamlet_*.py fait `from hamlet_common import *` puis définit ses
# propres fonctions `gen_xxx()` et sa propre liste ASSETS.
#
# Contraintes moteur (src/scene/import.rs, rappelées ici, détaillées dans la
# mémoire projet) :
# - un objet = un seul mesh joint, transform_apply avant export (le loader
#   ignore les transforms de nœuds glTF et concatène les sommets bruts) ;
# - seule couleur lue : base_color_factor par primitive/matériau, alpha
#   ignoré (pas de transparence possible — un objet "fumée" doit rester
#   opaque, ex. un blob stylisé, pas un plan semi-transparent) ;
# - sol du jeu = z=0 Blender / export Y-up ; échelle appliquée avant rotation ;
# - échelle réelle en mètres, échelle scène = 1.0 (décor, pas 0.35 des
#   créatures).

import math  # noqa: F401  (réexporté pour les gen_hamlet_*.py qui en ont besoin)
import os
import random

import bpy
import mathutils

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260718)  # reproductible, même graine que gen_stone_pack.py

# ---------------------------------------------------------------------------
# Palette commune — figée une fois pour tout le pack hameau maison. Reprend
# TELLES QUELLES les teintes bois/pierre/toit/chaume déjà utilisées par
# gen_nature_pack.py (le hameau existant et son décor doivent se répondre,
# règle « une teinte par système » de ANALYSE_DESIGN_VISUEL.md §3) et ajoute
# uniquement les teintes propres au mobilier/aux bâtiments (tissu, métal,
# paille de botte). Règle : ≤ 3 teintes par objet, aucune texture.
# ---------------------------------------------------------------------------

WOOD = (0.45, 0.30, 0.15)  # bois d'œuvre clair (murs à colombage, tabliers, caisses)
WOOD_DARK = (0.28, 0.18, 0.09)  # bois sombre (portes, poutres, rambardes)
STONE = (0.45, 0.44, 0.42)  # pierre claire (socles, margelles, fondations)
STONE_DARK = (0.36, 0.35, 0.34)  # pierre sombre (ombres, fond de puits)
ROOF = (0.50, 0.22, 0.14)  # tuiles/bardeaux des toits en dur
THATCH = (0.62, 0.48, 0.20)  # chaume ocre (toits de chaume, meules)
GLOW_YELLOW = (1.0, 0.78, 0.35)  # verre chaud des fenêtres éclairées / feu

# Teintes propres au mobilier/bâtiments (nouvelles, absentes de gen_nature_pack) :
CLOTH = (0.68, 0.58, 0.40)  # toile/sac de jute clair (sacs, bâches d'étal)
CLOTH_DARK = (0.42, 0.34, 0.22)  # toile/cuir sombre (sacs fermés, harnais)
METAL = (0.35, 0.36, 0.38)  # métal terne (cloche, cerclages, lame de scie)
METAL_DARK = (0.20, 0.21, 0.23)  # métal sombre (chaudron, ferrures)
HAY = (0.68, 0.56, 0.22)  # paille dorée (bottes de foin — plus clair que THATCH)
FIRE = (0.85, 0.35, 0.10)  # flammes (feu de camp, forge)
SMOKE = (0.55, 0.55, 0.55)  # fumée opaque stylisée (pas de transparence : cf. en-tête)
GLASS = (0.55, 0.75, 0.85)  # vitre/carreau — même teinte que gen_nature_pack.gen_cabin


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb, roughness=0.85, emission=0.0):
    """Matériau Principled dont Base Color devient le base_color_factor glTF.
    `emission` (cf. creature_kit.material) réservé aux vrais signaux de jeu
    (feu, fenêtre éclairée) — jamais décoratif, règle n°2 de la charte."""
    m = bpy.data.materials.get(name)
    if m is None:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = roughness
        if emission > 0.0:
            bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
            bsdf.inputs["Emission Strength"].default_value = emission
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


def cone(name, material, location, radius, depth, vertices=10, radius2=0.0):
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=radius, radius2=radius2, depth=depth, location=location
    )
    o = bpy.context.active_object
    o.name = name
    assign(o, material)
    return o


def blob(name, material, location, radius, squash=1.0, jitter=0.0):
    """Icosphère (option écrasée/irrégulière) — sacs, tas, fumée, feuillage."""
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=radius, location=location)
    o = bpy.context.active_object
    o.name = name
    o.scale = (1.0, 1.0, squash)
    if jitter > 0.0:
        # Garde-sol : un blob proche du sol (sac, tas, souche...) dont le
        # jitter tire un sommet trop bas percerait le plancher — piège
        # rencontré sur hamlet_bag/hamlet_bags. On borne le z LOCAL (avant le
        # o.scale appliqué ensuite) pour que le z MONDE (location[2] +
        # local_z * squash) ne descende jamais sous 0.
        min_local_z = -location[2] / squash if squash > 0 else -location[2]
        for v in o.data.vertices:
            v.co.x += rng.uniform(-jitter, jitter)
            v.co.y += rng.uniform(-jitter, jitter)
            v.co.z = max(v.co.z + rng.uniform(-jitter, jitter), min_local_z)
    assign(o, material)
    return o


def render_preview(obj, filename):
    """Vignette EEVEE 640×480, deux soleils (convention transverse des scripts
    gen_*, cf. creature_kit.build_creature). La distance de caméra est mise à
    l'échelle de l'objet (span de sa bounding box) pour cadrer correctement
    aussi bien un petit prop (~0.3 m) qu'un bâtiment (~5 m). Caméra côté +X/+Y
    (visée calculée, pas d'angle fixe) : les pièces directionnelles (portes,
    fenêtres…) ont leur face avant en +Y, cf. convention gen_nature_pack —
    une caméra fixe côté -Y montrerait leur dos."""
    scene = bpy.context.scene
    dims = obj.dimensions
    span = max(dims.x, dims.y, dims.z, 0.3)
    cam_dist = span * 1.5 + 0.5
    center = mathutils.Vector((0, 0, dims.z * 0.5))
    cam_loc = mathutils.Vector((cam_dist, cam_dist * 1.15, cam_dist * 0.85 + dims.z * 0.3))
    bpy.ops.object.camera_add(location=cam_loc)
    cam = bpy.context.active_object
    scene.camera = cam
    bpy.ops.object.empty_add(type="PLAIN_AXES", location=center)
    target = bpy.context.active_object
    tc = cam.constraints.new("TRACK_TO")
    tc.target = target
    tc.track_axis = "TRACK_NEGATIVE_Z"
    tc.up_axis = "UP_Y"
    bpy.context.view_layer.update()
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
    # Fond ambiant : contrairement aux créatures (silhouette vue sous un seul
    # angle canonique), nos pièces ont une face « avant » (+Y) qui peut se
    # retrouver dos aux deux soleils selon l'orientation de l'objet — sans
    # remplissage ambiant, cette face rend en noir pur (piège rencontré sur
    # gen_window_b : porte/fenêtre invisibles).
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
        scene.render.engine = "BLENDER_EEVEE_NEXT"  # Blender 4.2+
    except TypeError:
        scene.render.engine = "BLENDER_EEVEE"  # Blender < 4.2
    # Vue « Standard » (pas AgX/Filmic) : le jeu n'applique pas de tone
    # mapping cinéma, un émissif saturé (flamme) rendrait sinon délavé/pâle à
    # la vignette alors qu'il reste vif en jeu — la vignette doit refléter la
    # vraie base_color_factor, pas une réinterprétation colorimétrique.
    scene.view_settings.view_transform = "Standard"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = OUT_DIR + filename.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print(f"[hamlet] vignette {filename.replace('.glb', '_preview.png')}")


def pitched_roof(prefix, roof_mat, span_x, depth_y, rise, base_z, ridge_mat=None,
                  overhang=0.25, thickness=0.12):
    """Toit à deux pans (faîte parallèle à Y, pente le long de X) — généralisé
    depuis gen_nature_pack.gen_cabin pour être réutilisé par tous les
    bâtiments du hameau (gen_hamlet_buildings*.py). `span_x`/`depth_y` sont
    les dimensions du bâtiment SOUS le toit ; `base_z` le sommet des murs."""
    ridge_mat = ridge_mat or roof_mat
    half_span = span_x / 2 + overhang
    slope = math.atan2(rise, half_span)
    pan_len = math.hypot(half_span, rise) + overhang
    # Centre du pan = milieu du segment faîte (x=0) → égout (x=side*half_span) :
    # un facteur autre que /2 ici désaligne les deux pans dès que le rapport
    # portée/hauteur s'écarte de celui d'origine (piège rencontré sur
    # gen_blacksmith, toit visiblement disjoint/en porte-à-faux).
    for side in (-1, 1):
        p = cube(
            f"{prefix}Pan{side}", roof_mat,
            (side * half_span / 2, 0, base_z + rise / 2),
            (pan_len, depth_y + overhang, thickness),
        )
        p.rotation_euler = (0, side * slope, 0)
    cube(f"{prefix}Faite", ridge_mat, (0, 0, base_z + rise),
         (0.30, depth_y + overhang * 1.2, 0.16))


def shingled_roof(prefix, roof_mat, roof_mat_dark, span_x, depth_y, rise, base_z,
                   overhang=0.25, rows=5, thickness=0.05, ridge_mat=None):
    """Toit à deux pans « en tuiles » : remplace le pan plein de pitched_roof
    par des rangées qui suivent la pente, teintées en alternance — lisible
    comme des tuiles/bardeaux réels, pas un aplat. Même géométrie de pente
    que pitched_roof (mêmes span_x/depth_y/rise/base_z), donc interchangeable
    dans un bâtiment donné."""
    ridge_mat = ridge_mat or roof_mat_dark
    half_span = span_x / 2 + overhang
    slope = math.atan2(rise, half_span)
    full_len = math.hypot(half_span, rise) + overhang
    row_len = full_len / rows
    # Direction unitaire le long du pan, de la crête vers l'égout : dérivée
    # directement de la pente (half_span = H·cos(slope), rise = H·sin(slope)
    # avec H = hypot(half_span, rise)), donc pas de calcul d'angle séparé.
    for side in (-1, 1):
        dir_x, dir_z = side * math.cos(slope), -math.sin(slope)
        for r in range(rows):
            d = row_len * (r + 0.5)
            center = (dir_x * d, 0, base_z + rise + dir_z * d)
            m = roof_mat if r % 2 == 0 else roof_mat_dark
            p = cube(f"{prefix}Tuile{side}_{r}", m, center,
                      (row_len * 1.08, depth_y + overhang, thickness))
            p.rotation_euler = (0, side * slope, 0)
    cube(f"{prefix}Faite", ridge_mat, (0, 0, base_z + rise),
         (0.30, depth_y + overhang * 1.2, 0.16))


def plank_wall(prefix, wall_mat, groove_mat, location, width, height, depth,
               n_planks=6, groove_w=0.02):
    """Mur cube + rainures verticales en légère saillie (planches individuelles
    visibles) — `location` = centre, `width` le long de X, `height` le long
    de Z, `depth` l'épaisseur (Y)."""
    cube(f"{prefix}Mur", wall_mat, location, (width, depth, height))
    step = width / n_planks
    for i in range(1, n_planks):
        x = location[0] - width / 2 + i * step
        cube(f"{prefix}Rainure{i}", groove_mat,
             (x, location[1] + depth / 2 * 0.97, location[2]),
             (groove_w, depth * 0.06, height * 0.98))


def stone_coursing(prefix, base_mat, mortar_mat, location, width, height, depth, rows=4):
    """Mur cube + lignes d'assises horizontales en légère saillie (pierre ou
    brique appareillée) — mêmes conventions de paramètres que plank_wall."""
    cube(f"{prefix}Mur", base_mat, location, (width, depth, height))
    step = height / rows
    for i in range(1, rows):
        z = location[2] - height / 2 + i * step
        cube(f"{prefix}Assise{i}", mortar_mat,
             (location[0], location[1] + depth / 2 * 0.97, z),
             (width * 0.99, depth * 0.06, 0.025))


def hip_roof(prefix, roof_mat, span_x, span_y, rise, base_z, overhang=0.2):
    """Toit à 4 pans (croupe) — pyramide à base carrée (cône à 4 sommets),
    pour les bâtiments plus massifs (tour, caserne) où un pignon deux-pans
    lirait mal. Le cône est tourné de 45° pour aligner ses arêtes sur les
    murs (sans rotation, un cône à 4 sommets pointe en losange, pas en
    carré droit)."""
    span_x, span_y = span_x + overhang * 2, span_y + overhang * 2
    apothem = (span_x + span_y) / 4  # moyenne des demi-portées X/Y
    radius = apothem / math.cos(math.pi / 4)
    o = cone(f"{prefix}Toit", roof_mat, (0, 0, base_z + rise / 2),
             radius=radius, depth=rise, vertices=4)
    o.rotation_euler = (0, 0, math.pi / 4)


def export(filename, preview=True):
    """Joint tout, applique les transforms et exporte en GLB statique, puis
    rend une vignette de contrôle (sauf preview=False)."""
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    for o in bpy.context.scene.objects:
        o.select_set(o in meshes)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    joined = bpy.context.view_layer.objects.active
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.export_scene.gltf(
        filepath=OUT_DIR + filename,
        export_format="GLB",
        export_animations=False,
        export_skins=False,
        export_apply=True,
        export_yup=True,
    )
    print(f"[hamlet] exporté {filename}")
    if preview:
        render_preview(joined, filename)
