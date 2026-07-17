# Quatrième pack « flore » : 10 espèces de plus, suite de gen_flora_pack3.py
# (mêmes contraintes moteur, même palette, helpers améliorés) :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_flora_pack4.py
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

rng = random.Random(20260721)  # reproductible


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


def blob(name, material, location, radius, squash=1.0, jitter=0.0, subdiv=1, smooth=False):
    """Icosphère. subdiv=2 densifie les facettes (canopées) ; smooth lisse les
    normales (fruits, chapeaux) — le moteur lit les normales exportées."""
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


def taper(name, material, location, r_bottom, r_top, depth, vertices=12):
    """Tronc conique lissé : plus organique qu'un cylindre droit."""
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=r_bottom, radius2=r_top, depth=depth, location=location
    )
    o = bpy.context.active_object
    o.name = name
    bpy.ops.object.shade_smooth()
    assign(o, material)
    return o


def branch(name, material, base, tip, radius):
    """Cylindre lissé orienté du point `base` vers `tip` (troncs, branches)."""
    bx, by, bz = base
    tx, ty, tz = tip
    dx, dy, dz = tx - bx, ty - by, tz - bz
    length = math.sqrt(dx * dx + dy * dy + dz * dz)
    mid = ((bx + tx) / 2, (by + ty) / 2, (bz + tz) / 2)
    rot_y = math.acos(dz / length)
    rot_z = math.atan2(dy, dx)
    o = cylinder(
        name, material, mid, radius=radius, depth=length, vertices=7,
        rotation=(0, rot_y, rot_z),
    )
    bpy.ops.object.shade_smooth()
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
    print(f"[flore4] exporté {filename}")

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
    # Lumière d'appoint rasante SANS ombres : débouche les dessous (grappes
    # pendantes…) qui, avec le seul soleil zénithal, disparaissent dans le
    # fond noir. Preview uniquement — le jeu a son propre terme ambiant.
    bpy.ops.object.light_add(type="SUN", location=(-4, 3, 2))
    fill = bpy.context.active_object
    fill.data.energy = 1.5
    fill.data.use_shadow = False
    fill.rotation_euler = (math.radians(78), 0, math.radians(230))
    # Ambiance monde : petit terme constant qui débouche les zones à l'ombre
    # (l'éclairage direct seul rend les dessous noirs sur fond noir).
    world = bpy.data.worlds.new("PreviewWorld")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.28, 0.28, 0.30, 1.0)
    bg.inputs["Strength"].default_value = 1.0
    scene.world = world
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 640
    scene.render.resolution_y = 480
    scene.render.filepath = OUT_DIR + filename.replace(".glb", "_preview.png")
    bpy.ops.render.render(write_still=True)
    print(f"[flore4] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette : teintes partagées + nouvelles teintes de ce pack.
# ---------------------------------------------------------------------------

BROWN = (0.32, 0.22, 0.11)
WOOD = (0.45, 0.30, 0.15)
WOOD_DARK = (0.28, 0.18, 0.09)
LEAF_DARK = (0.18, 0.42, 0.16)
LEAF_LIGHT = (0.24, 0.50, 0.18)
STONE = (0.45, 0.44, 0.42)
BERRY_RED = (0.68, 0.16, 0.20)
LAVENDER = (0.55, 0.42, 0.78)
MOSS = (0.30, 0.48, 0.20)
SEQUOIA_BARK = (0.42, 0.24, 0.14)  # écorce rousse du séquoia
PALM_LEAF = (0.22, 0.48, 0.24)  # palmes
HOLLY_LEAF = (0.10, 0.28, 0.12)  # houx vernissé sombre
WISTERIA = (0.62, 0.50, 0.82)  # glycine (violet clair, famille lavande)
GRAPE = (0.35, 0.18, 0.42)  # grappes de raisin noir
CORN_LEAF = (0.34, 0.52, 0.20)  # feuilles de maïs
CORN_EAR = (0.85, 0.70, 0.28)  # épi doré (≠ jaune chaud des lanternes)
CARROT_TOP = (0.26, 0.50, 0.18)  # fanes
CARROT = (0.85, 0.42, 0.10)  # racine orange
IRIS_PETAL = (0.42, 0.30, 0.72)  # iris d'eau violet
MUSHROOM_CAP = (0.68, 0.16, 0.20)  # chapeau du champignon géant

# ---------------------------------------------------------------------------
# Grands sujets
# ---------------------------------------------------------------------------


def gen_sequoia():
    """Séquoia ~7 m : fût roux massif + étages de feuillage sombre — le géant
    de la forêt profonde, plus haut que la tour de guet. Tronc plein très
    visible des sondes."""
    bark = mat("ecorce_sequoia", SEQUOIA_BARK)
    needle = mat("aiguilles", (0.10, 0.32, 0.14))
    needle_d = mat("aiguilles_sombres", (0.08, 0.26, 0.12))
    taper("Fut", bark, (0, 0, 2.2), r_bottom=0.55, r_top=0.30, depth=4.4)
    blob("Contrefort", bark, (0, 0, 0.3), radius=0.7, squash=0.5, jitter=0.08)
    for i, (z, r) in enumerate(((3.2, 1.6), (4.3, 1.3), (5.3, 1.0), (6.1, 0.7))):
        m = needle if i % 2 == 0 else needle_d
        blob(f"Etage{i}", m, (0.05 * (i % 2), -0.05 * (i % 2), z), radius=r,
             squash=0.55, jitter=0.10, subdiv=2)
    cone("Cime", needle, (0, 0, 6.8), radius=0.45, depth=0.9, vertices=9)
    export_and_preview("nature_sequoia.glb")


def gen_palm():
    """Palmier ~4,2 m : stipe incliné annelé + couronne de palmes arquées —
    la berge sableuse de la rivière. Stipe plein pour les sondes."""
    trunk = mat("stipe", (0.48, 0.36, 0.22))
    ring = mat("anneau_stipe", (0.38, 0.28, 0.16))
    palm_m = mat("palme", PALM_LEAF)
    coco = mat("noix", (0.35, 0.24, 0.12))
    top = (0.55, 0.3, 3.4)
    branch("Stipe", trunk, (0, 0, 0), top, radius=0.16)
    for k in range(1, 6):
        t = k / 6
        cylinder(f"Anneau{k}", ring, (top[0] * t, top[1] * t, 3.4 * t),
                 radius=0.175, depth=0.08, vertices=9)
    # 7 palmes : cônes très aplatis arqués autour de la couronne.
    for i in range(7):
        a = i * math.tau / 7
        dx, dy = math.cos(a), math.sin(a)
        p = cone(
            f"Palme{i}", palm_m,
            (top[0] + dx * 0.85, top[1] + dy * 0.85, top[2] + 0.12),
            radius=0.42, depth=1.9, vertices=4,
            rotation=(0, math.radians(105), a),
        )
        bpy.ops.object.shade_smooth()
    for i in range(3):
        a = i * math.tau / 3 + 0.5
        blob(f"Noix{i}", coco,
             (top[0] + 0.22 * math.cos(a), top[1] + 0.22 * math.sin(a), top[2] - 0.12),
             radius=0.1, smooth=True)
    export_and_preview("nature_palm.glb")


def gen_holly():
    """Houx ~2,1 m : masse vernissée sombre piquée de baies rouges — le buisson
    d'hiver, silhouette pyramidale dense. Solide (masse pleine dès le sol)."""
    leaf = mat("houx", HOLLY_LEAF)
    leaf2 = mat("houx_clair", (0.14, 0.34, 0.15))
    berry = mat("baie_houx", BERRY_RED)
    blob("Masse1", leaf, (0, 0, 0.6), radius=0.7, squash=1.0, jitter=0.07, subdiv=2)
    blob("Masse2", leaf2, (0.1, 0.05, 1.4), radius=0.5, squash=1.0, jitter=0.06, subdiv=2)
    cone("Pointe", leaf, (0.1, 0.05, 1.95), radius=0.3, depth=0.5, vertices=8)
    for i in range(7):
        a = i * math.tau / 7 + 0.3
        r = rng.uniform(0.5, 0.68)
        z = 0.7 + rng.uniform(-0.2, 0.5)
        blob(f"Baie{i}", berry, (r * math.cos(a), r * math.sin(a), z), radius=0.06,
             smooth=True)
    export_and_preview("nature_holly.glb")


def gen_wisteria_arch():
    """Arche de glycine ~2,6 m : portique de bois sous cascade violette — le
    passage fleuri du jardin (piliers pleins vus des sondes, on passe dessous
    comme sous le torii)."""
    wood = mat("bois_sombre", WOOD_DARK)
    bloom = mat("glycine", WISTERIA)
    bloom_d = mat("glycine_sombre", (0.50, 0.38, 0.70))
    leaf = mat("feuillage_b", LEAF_LIGHT)
    for sx in (-1, 1):
        cylinder(f"Pilier{sx}", wood, (sx * 1.05, 0, 1.1), radius=0.14, depth=2.2, vertices=9)
    cube("Linteau", wood, (0, 0, 2.28), (2.6, 0.18, 0.16))
    # Feuillage vautré SUR le linteau : chapelet de touffes modestes — une
    # grosse boule (1,8 m de profondeur) surplomberait les grappes et les
    # cacherait de toute vue en plongée.
    for i, (x, r) in enumerate(((-0.85, 0.42), (-0.3, 0.5), (0.25, 0.48), (0.8, 0.42))):
        blob(f"Feuillage{i}", leaf, (x, rng.uniform(-0.06, 0.06), 2.42),
             radius=r, squash=0.5, jitter=0.05, subdiv=2)
    # Cascade : grosses grappes pendantes sous le linteau, longueurs variées.
    for i in range(7):
        x = -0.95 + i * 0.32
        h = rng.uniform(0.55, 0.95)
        m = bloom if i % 2 == 0 else bloom_d
        o = cone(f"Grappe{i}", m, (x, rng.uniform(-0.12, 0.12), 2.24 - h / 2),
                 radius=0.17, depth=h, vertices=7, rotation=(math.pi, 0, 0))
        bpy.ops.object.shade_smooth()
    export_and_preview("nature_wisteria_arch.glb")


def gen_vine_trellis():
    """Treille de vigne 2×2 m : poteaux + traverses, rideau de feuilles PLEIN
    de 0,3 à 1,9 m (visible des sondes — une treille ajourée serait un piège à
    patrouille) et grappes noires."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    leaf = mat("feuille_vigne", LEAF_DARK)
    leaf2 = mat("feuille_vigne_claire", LEAF_LIGHT)
    grape = mat("raisin", GRAPE)
    for sx in (-1, 1):
        cube(f"Poteau{sx}", dark, (sx * 0.95, 0, 0.95), (0.12, 0.12, 1.9))
    for z in (0.75, 1.35, 1.9):
        cube(f"Traverse{z}", wood, (0, 0, z), (2.1, 0.08, 0.08))
    # Cœur plein mince : garantit le flanc continu vu des sondes (une treille
    # ajourée serait un piège à patrouille)…
    cube("Coeur", leaf, (0, 0, 1.1), (1.85, 0.10, 1.5))
    # …habillé de masses feuillues qui débordent des deux côtés : c'est la
    # végétation qu'on lit, pas le panneau.
    for i in range(9):
        x = -0.8 + (i % 5) * 0.4 + rng.uniform(-0.08, 0.08)
        z = 0.55 + (i // 5) * 0.75 + rng.uniform(-0.12, 0.12)
        m = leaf if i % 2 == 0 else leaf2
        blob(f"Masse{i}", m, (x, rng.uniform(-0.05, 0.05), z),
             radius=rng.uniform(0.32, 0.44), squash=0.9, jitter=0.05, subdiv=2)
    for i in range(5):
        x = rng.uniform(-0.75, 0.75)
        z = rng.uniform(0.55, 1.35)
        o = cone(f"Grappe{i}", grape, (x, rng.choice((-0.3, 0.3)), z),
                 radius=0.10, depth=0.30, vertices=7, rotation=(math.pi, 0, 0))
        bpy.ops.object.shade_smooth()
    export_and_preview("nature_vine_trellis.glb")


# ---------------------------------------------------------------------------
# Potager, berge, sous-bois
# ---------------------------------------------------------------------------


def gen_corn():
    """Maïs ~1,9 m : 3 pieds à longues feuilles arquées + épis dorés — le fond
    du potager, haute silhouette agricole (non solide)."""
    stalk = mat("tige_mais", CORN_LEAF)
    leaf = mat("feuille_mais", (0.28, 0.46, 0.17))
    ear = mat("epi_mais", CORN_EAR)
    spots = [(0.0, 0.0), (0.5, 0.2), (-0.45, -0.15)]
    for i, (x, y) in enumerate(spots):
        h = rng.uniform(1.6, 1.9)
        cylinder(f"Tige{i}", stalk, (x, y, h / 2), radius=0.03, depth=h, vertices=6)
        for k in range(4):
            a = k * math.tau / 4 + i * 0.7
            z = 0.5 + k * 0.35
            o = cone(f"Feuille{i}_{k}", leaf,
                     (x + 0.3 * math.cos(a), y + 0.3 * math.sin(a), z),
                     radius=0.09, depth=0.8, vertices=4,
                     rotation=(0, math.radians(115), a))
            bpy.ops.object.shade_smooth()
        o = cylinder(f"Epi{i}", ear, (x + 0.09, y + 0.05, h * 0.62),
                     radius=0.055, depth=0.3, vertices=8,
                     rotation=(math.radians(20), 0, 0.5))
        bpy.ops.object.shade_smooth()
    export_and_preview("nature_corn.glb")


def gen_carrots():
    """Rang de carottes ~0,3 m : 5 touffes de fanes + une carotte arrachée
    posée à côté — le potager qui raconte la récolte (non solide)."""
    top = mat("fane", CARROT_TOP)
    root = mat("carotte", CARROT)
    dirt = mat("terre", (0.30, 0.22, 0.13))
    for i in range(5):
        x = -0.7 + i * 0.35
        blob(f"Butte{i}", dirt, (x, 0, 0.03), radius=0.12, squash=0.35, jitter=0.02)
        for k in range(3):
            a = k * math.tau / 3 + i
            cone(f"Fane{i}_{k}", top,
                 (x + 0.05 * math.cos(a), 0.05 * math.sin(a), 0.16),
                 radius=0.045, depth=0.26, vertices=5)
        cone(f"Collet{i}", root, (x, 0, 0.045), radius=0.035, depth=0.05, vertices=7)
    # La carotte arrachée, couchée devant le rang.
    o = cone("Arrachee", root, (0.15, -0.4, 0.045), radius=0.05, depth=0.32, vertices=8,
             rotation=(0, math.radians(88), 0.4))
    bpy.ops.object.shade_smooth()
    cone("FaneArrachee", top, (-0.02, -0.46, 0.06), radius=0.04, depth=0.2, vertices=5,
         rotation=(0, math.radians(75), 0.9))
    export_and_preview("nature_carrots.glb")


def gen_irises():
    """Iris d'eau ~0,8 m : lames dressées + fleurs violettes à cœur blanc —
    la berge fleurie, entre roseaux et nénuphars (non solide)."""
    blade = mat("lame_iris", (0.20, 0.44, 0.22))
    petal = mat("petale_iris", IRIS_PETAL)
    heart = mat("coeur_blanc", (0.90, 0.90, 0.86))
    for i in range(6):
        a = i * math.tau / 6 + rng.uniform(-0.25, 0.25)
        r = rng.uniform(0.05, 0.3)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.5, 0.75)
        cone(f"Lame{i}", blade, (x, y, h / 2), radius=0.045, depth=h, vertices=4)
        if i % 2 == 0:
            blob(f"Fleur{i}", petal, (x, y, h + 0.06), radius=0.085, squash=0.75,
                 smooth=True)
            blob(f"Coeur{i}", heart, (x, y, h + 0.11), radius=0.03, smooth=True)
    export_and_preview("nature_irises.glb")


def gen_moss_boulder():
    """Rocher moussu ~1,3 m : bloc de pierre coiffé de coussins de mousse — la
    variante sous-bois du rocher nu. Solide, culmine bien au-dessus des sondes
    à 0,6 m."""
    stone = mat("pierre", STONE)
    stone_d = mat("pierre_sombre", (0.36, 0.35, 0.34))
    moss = mat("mousse", MOSS)
    blob("Bloc", stone, (0, 0, 0.65), radius=0.85, squash=0.8, jitter=0.12, subdiv=2)
    blob("Bloc2", stone_d, (0.6, 0.3, 0.35), radius=0.42, squash=0.75, jitter=0.08)
    blob("Coussin1", moss, (0.05, -0.1, 1.15), radius=0.5, squash=0.4, jitter=0.06, subdiv=2)
    blob("Coussin2", moss, (-0.45, 0.25, 0.9), radius=0.3, squash=0.4, jitter=0.05)
    blob("CoussinSol", moss, (0.75, -0.3, 0.07), radius=0.25, squash=0.3, jitter=0.04)
    export_and_preview("nature_moss_boulder.glb")


def gen_giant_mushroom():
    """Champignon géant ~1,7 m : pied massif + chapeau rouge à points blancs —
    le coin fantastique de la forêt, côté monstres. Solide (pied plein r 0,28)."""
    stem = mat("pied_geant", (0.85, 0.80, 0.70))
    gill = mat("lamelles", (0.72, 0.66, 0.55))
    cap = mat("chapeau_geant", MUSHROOM_CAP)
    dot = mat("point_blanc", (0.92, 0.92, 0.88))
    taper("Pied", stem, (0, 0, 0.55), r_bottom=0.34, r_top=0.22, depth=1.1)
    cylinder("Lamelles", gill, (0, 0, 1.12), radius=0.55, depth=0.1, vertices=12)
    blob("Chapeau", cap, (0, 0, 1.3), radius=0.75, squash=0.55, jitter=0.05,
         subdiv=2, smooth=True)
    for i in range(6):
        a = i * math.tau / 6 + 0.4
        r = rng.uniform(0.25, 0.55)
        z = 1.42 + (0.55 - r) * 0.35
        blob(f"Point{i}", dot, (r * math.cos(a), r * math.sin(a), z), radius=0.07,
             squash=0.5, smooth=True)
    # Deux petits au pied du grand.
    for sx, sy in ((0.55, 0.25), (-0.45, -0.3)):
        cylinder(f"PetitPied{sx}", stem, (sx, sy, 0.14), radius=0.06, depth=0.28, vertices=7)
        blob(f"PetitChapeau{sx}", cap, (sx, sy, 0.32), radius=0.15, squash=0.6,
             subdiv=2, smooth=True)
    export_and_preview("nature_giant_mushroom.glb")


ASSETS = [
    gen_sequoia,
    gen_palm,
    gen_holly,
    gen_wisteria_arch,
    gen_vine_trellis,
    gen_corn,
    gen_carrots,
    gen_irises,
    gen_moss_boulder,
    gen_giant_mushroom,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[flore4] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
