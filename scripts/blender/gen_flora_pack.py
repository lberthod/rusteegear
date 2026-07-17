# Génère le pack « flore » : nouvelles espèces d'arbres et de plantes qui
# complètent gen_nature_pack.py (mêmes contraintes moteur, même palette) :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_flora_pack.py
#
# Sortie : assets/models/nature_*.glb + nature_*_preview.png (rendu EEVEE
# 640×480 par asset, pour choisir sans lancer le jeu).
#
# Contraintes moteur (voir l'en-tête de gen_nature_pack.py pour le détail) :
# - meshes statiques joints en un seul objet, transform_apply avant export ;
# - couleurs uniquement via base_color_factor (pas de textures) ;
# - base des assets à z=0 Blender (= sol du jeu, glTF Y-up) ;
# - tout asset destiné à être solide doit présenter un flanc plein visible du
#   raycast horizontal des créatures à 0,6 m (les troncs au centre suffisent
#   pour les arbres ; les petites plantes sont non solides).
#
# Direction artistique : ≤ 3 teintes par asset, silhouettes lisibles à 30 m.
# Le jaune chaud reste réservé aux lumières — les tournesols prennent un or
# plus orangé pour ne pas se confondre avec les lanternes.

import math
import os
import random

import bpy
import mathutils

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260717)  # reproductible


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
    """Branche : cylindre orienté du point `base` vers `tip` (troncs penchés,
    branches nues de l'arbre mort, cannes de bambou inclinées)."""
    bx, by, bz = base
    tx, ty, tz = tip
    dx, dy, dz = tx - bx, ty - by, tz - bz
    length = math.sqrt(dx * dx + dy * dy + dz * dz)
    mid = ((bx + tx) / 2, (by + ty) / 2, (bz + tz) / 2)
    # Rotation qui amène l'axe Z du cylindre sur la direction base→tip.
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
    print(f"[flore] exporté {filename}")

    # Preview : caméra 3/4 cadrée sur les bornes de l'objet joint, soleil doux.
    obj = bpy.context.active_object
    bpy.context.view_layer.update()
    xs = [(obj.matrix_world @ v.co) for v in obj.data.vertices]
    min_z = min(p.z for p in xs)
    max_z = max(p.z for p in xs)
    span = max(
        max(p.x for p in xs) - min(p.x for p in xs),
        max(p.y for p in xs) - min(p.y for p in xs),
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
    print(f"[flore] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette : teintes partagées avec gen_nature_pack + nouvelles teintes flore.
# ---------------------------------------------------------------------------

BROWN = (0.32, 0.22, 0.11)
LEAF_DARK = (0.18, 0.42, 0.16)
LEAF_LIGHT = (0.24, 0.50, 0.18)
BIRCH_BARK = (0.88, 0.87, 0.82)  # écorce blanche du bouleau
BIRCH_MARK = (0.15, 0.14, 0.12)  # taches noires de l'écorce
BIRCH_LEAF = (0.42, 0.60, 0.22)  # feuillage doré-vert clair
WILLOW_LEAF = (0.35, 0.52, 0.28)  # vert cendré du saule
BLOSSOM = (0.92, 0.62, 0.72)  # fleurs de cerisier
BLOSSOM_DEEP = (0.82, 0.45, 0.58)
DEAD_WOOD = (0.38, 0.33, 0.28)  # bois mort grisâtre
BAMBOO = (0.45, 0.58, 0.22)  # canne de bambou
SUN_GOLD = (0.90, 0.60, 0.12)  # or orangé (≠ jaune des lanternes)
LAVENDER = (0.55, 0.42, 0.78)
BERRY_RED = (0.68, 0.16, 0.20)  # baies discrètes (pas l'accent rouge de zone)

# ---------------------------------------------------------------------------
# Arbres
# ---------------------------------------------------------------------------


def gen_birch():
    """Bouleau ~4.2 m : tronc blanc élancé taché de noir, feuillage clair et
    léger en haut — silhouette verticale qui tranche avec les feuillus ronds."""
    bark = mat("ecorce_bouleau", BIRCH_BARK)
    marks = mat("taches_bouleau", BIRCH_MARK)
    leaf = mat("feuille_bouleau", BIRCH_LEAF)
    taper("Tronc", bark, (0, 0, 1.5), r_bottom=0.16, r_top=0.09, depth=3.0)
    for i in range(5):
        a = i * math.tau / 5 + 0.5
        z = 0.5 + i * 0.55
        cube(
            f"Tache{i}", marks,
            (0.13 * math.cos(a), 0.13 * math.sin(a), z),
            (0.06, 0.06, 0.14), rotation=(0, 0, a),
        )
    blob("Feuillage1", leaf, (0.0, 0.0, 3.5), radius=0.85, squash=0.9, jitter=0.07, subdiv=2)
    blob("Feuillage2", leaf, (0.4, 0.2, 2.9), radius=0.5, squash=0.8, jitter=0.05, subdiv=2)
    blob("Feuillage3", leaf, (-0.35, -0.25, 3.05), radius=0.45, squash=0.8, jitter=0.05, subdiv=2)
    export_and_preview("nature_birch.glb")


def gen_willow():
    """Saule pleureur ~3.8 m : dôme de feuillage + mèches retombantes tout
    autour, jusqu'à ~0.8 m du sol — posé près de l'eau."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuille_saule", WILLOW_LEAF)
    leaf_d = mat("feuille_saule_sombre", (0.26, 0.42, 0.22))
    taper("Tronc", trunk, (0, 0, 1.0), r_bottom=0.28, r_top=0.18, depth=2.0)
    blob("Dome", leaf, (0.0, 0.0, 2.9), radius=1.35, squash=0.7, jitter=0.09, subdiv=2)
    for i in range(9):
        a = i * math.tau / 9 + rng.uniform(-0.15, 0.15)
        r = rng.uniform(1.05, 1.3)
        x, y = r * math.cos(a), r * math.sin(a)
        top = rng.uniform(2.4, 2.7)
        bot = rng.uniform(0.8, 1.3)
        m = leaf if i % 2 == 0 else leaf_d
        cylinder(
            f"Meche{i}", m, (x, y, (top + bot) / 2),
            radius=rng.uniform(0.10, 0.15), depth=top - bot, vertices=6,
        )
    export_and_preview("nature_willow.glb")


def gen_cherry_blossom():
    """Cerisier en fleurs ~3.2 m : tronc penché noueux + nuages roses — l'arbre
    remarquable près du torii (même famille d'accent, plus doux)."""
    trunk = mat("tronc", BROWN)
    pink = mat("fleurs_cerisier", BLOSSOM)
    pink_d = mat("fleurs_cerisier_fonce", BLOSSOM_DEEP)
    branch("Tronc", trunk, (0, 0, 0), (0.35, 0.15, 1.9), radius=0.19)
    branch("Branche1", trunk, (0.25, 0.1, 1.5), (1.0, 0.5, 2.3), radius=0.09)
    branch("Branche2", trunk, (0.3, 0.12, 1.7), (-0.6, -0.4, 2.5), radius=0.09)
    blob("Nuage1", pink, (0.5, 0.2, 2.55), radius=1.0, squash=0.75, jitter=0.08, subdiv=2)
    blob("Nuage2", pink_d, (1.05, 0.55, 2.25), radius=0.6, squash=0.7, jitter=0.06, subdiv=2)
    blob("Nuage3", pink, (-0.65, -0.4, 2.5), radius=0.65, squash=0.7, jitter=0.06, subdiv=2)
    export_and_preview("nature_cherry_blossom.glb")


def gen_dead_tree():
    """Arbre mort ~3.5 m : tronc gris + branches nues tordues, zéro feuillage —
    ponctue les lisières et le promontoire. Tronc plein pour les sondes."""
    wood = mat("bois_mort", DEAD_WOOD)
    dark = mat("bois_mort_sombre", (0.28, 0.24, 0.20))
    branch("Tronc", wood, (0, 0, 0), (0.15, -0.1, 2.4), radius=0.22)
    branch("Branche1", wood, (0.1, -0.05, 1.9), (1.1, 0.3, 2.9), radius=0.08)
    branch("Branche2", dark, (0.12, -0.07, 2.1), (-0.9, -0.5, 3.1), radius=0.08)
    branch("Branche3", wood, (0.15, -0.1, 2.35), (0.5, 0.6, 3.5), radius=0.07)
    branch("Rameau1", dark, (0.8, 0.2, 2.7), (1.35, 0.45, 3.3), radius=0.04)
    branch("Rameau2", wood, (-0.6, -0.35, 2.8), (-1.15, -0.4, 3.35), radius=0.04)
    export_and_preview("nature_dead_tree.glb")


def gen_apple_tree():
    """Pommier ~3 m : couronne ronde basse piquée de pommes — le verger du
    hameau. Rouge des baies (discret), pas l'accent rouge de zone."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuillage_a", LEAF_DARK)
    leaf2 = mat("feuillage_b", LEAF_LIGHT)
    apple = mat("pomme", BERRY_RED)
    taper("Tronc", trunk, (0, 0, 0.75), r_bottom=0.20, r_top=0.13, depth=1.5)
    blob("Couronne", leaf, (0.0, 0.0, 2.1), radius=1.1, squash=0.8, jitter=0.08, subdiv=2)
    blob("Couronne2", leaf2, (0.5, 0.3, 1.85), radius=0.6, squash=0.75, jitter=0.06, subdiv=2)
    for i in range(7):
        a = i * math.tau / 7 + 0.3
        r = rng.uniform(0.85, 1.05)
        z = 2.0 + rng.uniform(-0.45, 0.35)
        blob(f"Pomme{i}", apple, (r * math.cos(a), r * math.sin(a), z), radius=0.09,
             subdiv=2, smooth=True)
    export_and_preview("nature_apple_tree.glb")


def gen_bamboo():
    """Touffe de bambous ~3.2 m : 6 cannes segmentées légèrement inclinées +
    petites feuilles en haut. Les cannes groupées font une masse centrale
    suffisante pour les sondes si l'asset est posé solide."""
    cane = mat("canne_bambou", BAMBOO)
    node_m = mat("noeud_bambou", (0.35, 0.45, 0.18))
    leaf = mat("feuille_bambou", LEAF_LIGHT)
    for i in range(6):
        a = i * math.tau / 6
        r = rng.uniform(0.10, 0.30)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(2.4, 3.2)
        lean = rng.uniform(0.05, 0.18)
        tip = (x + lean * math.cos(a), y + lean * math.sin(a), h)
        branch(f"Canne{i}", cane, (x, y, 0), tip, radius=0.055)
        for k in range(1, 4):
            t = k / 4
            nx, ny, nz = x + (tip[0] - x) * t, y + (tip[1] - y) * t, h * t
            cylinder(f"Noeud{i}_{k}", node_m, (nx, ny, nz), radius=0.065, depth=0.05, vertices=7)
        cone(
            f"Feuilles{i}", leaf, (tip[0], tip[1], h + 0.15),
            radius=0.22, depth=0.55, vertices=5,
        )
    export_and_preview("nature_bamboo.glb")


# ---------------------------------------------------------------------------
# Plantes basses (non solides — sous les sondes à 0,6 m, décor pur)
# ---------------------------------------------------------------------------


def gen_mushrooms():
    """Rond de champignons ~0.35 m : 5 pieds crème à chapeaux bruns/rouges —
    sous-bois, pieds des arbres morts."""
    stem = mat("pied_champignon", (0.85, 0.80, 0.70))
    cap_b = mat("chapeau_brun", (0.48, 0.30, 0.16))
    cap_r = mat("chapeau_rouge", BERRY_RED)
    dot = mat("point_blanc", (0.92, 0.92, 0.88))
    for i in range(5):
        a = i * math.tau / 5 + rng.uniform(-0.3, 0.3)
        r = rng.uniform(0.08, 0.32)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.10, 0.22)
        cylinder(f"Pied{i}", stem, (x, y, h / 2), radius=0.035, depth=h, vertices=6)
        cap = cap_r if i % 3 == 0 else cap_b
        blob(f"Chapeau{i}", cap, (x, y, h + 0.02), radius=0.10, squash=0.55, subdiv=2, smooth=True)
        if i % 3 == 0:
            blob(f"Point{i}", dot, (x + 0.04, y + 0.02, h + 0.055), radius=0.022, squash=0.5,
                 smooth=True)
    export_and_preview("nature_mushrooms.glb")


def gen_sunflowers():
    """Tournesols ~1.5 m : 3 tiges hautes, cœur brun + couronne or orangé
    (volontairement ≠ du jaune chaud des lanternes) — potagers du hameau."""
    stem = mat("tige_tournesol", (0.22, 0.42, 0.14))
    heart = mat("coeur_tournesol", (0.30, 0.20, 0.10))
    petal = mat("petale_or", SUN_GOLD)
    for i in range(3):
        a = i * math.tau / 3
        r = 0.28 if i > 0 else 0.0
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(1.15, 1.5)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.03, depth=h, vertices=6)
        blob(f"FeuilleT{i}", stem, (x + 0.10, y, h * 0.55), radius=0.12, squash=0.35)
        # Fleur face au sud (-Y Blender) : couronne aplatie + cœur en avant.
        cylinder(
            f"Couronne{i}", petal, (x, y - 0.03, h + 0.16),
            radius=0.20, depth=0.05, vertices=12, rotation=(math.pi / 2, 0, 0),
        )
        cylinder(
            f"Coeur{i}", heart, (x, y - 0.06, h + 0.16),
            radius=0.11, depth=0.05, vertices=10, rotation=(math.pi / 2, 0, 0),
        )
    export_and_preview("nature_sunflowers.glb")


def gen_lavender():
    """Massif de lavande ~0.6 m : 8 épis violets sur tiges gris-vert — bordures
    des chemins du hameau, tache de couleur froide."""
    stem = mat("tige_lavande", (0.42, 0.50, 0.38))
    bloom = mat("epi_lavande", LAVENDER)
    bloom_d = mat("epi_lavande_fonce", (0.45, 0.32, 0.66))
    for i in range(8):
        a = i * math.tau / 8 + rng.uniform(-0.2, 0.2)
        r = rng.uniform(0.06, 0.30)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.35, 0.55)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.02, depth=h, vertices=5)
        cylinder(
            f"Epi{i}", bloom if i % 2 == 0 else bloom_d,
            (x, y, h + 0.09), radius=0.045, depth=0.2, vertices=6,
        )
    export_and_preview("nature_lavender.glb")


def gen_berry_bush():
    """Buisson à baies ~0.8 m : masse feuillue piquée de baies rouges sombres —
    variante gourmande du buisson standard, lisière de forêt."""
    leaf = mat("buisson", (0.20, 0.44, 0.17))
    leaf2 = mat("buisson_clair", (0.28, 0.52, 0.20))
    berry = mat("baie", BERRY_RED)
    blob("Masse1", leaf, (0, 0, 0.38), radius=0.48, squash=0.8, jitter=0.05, subdiv=2)
    blob("Masse2", leaf2, (0.32, 0.18, 0.30), radius=0.30, squash=0.75, jitter=0.04, subdiv=2)
    blob("Masse3", leaf, (-0.30, -0.12, 0.28), radius=0.28, squash=0.7, jitter=0.04, subdiv=2)
    for i in range(8):
        a = i * math.tau / 8 + 0.2
        r = rng.uniform(0.30, 0.48)
        z = 0.38 + rng.uniform(-0.15, 0.18)
        blob(f"Baie{i}", berry, (r * math.cos(a), r * math.sin(a), z), radius=0.045, smooth=True)
    export_and_preview("nature_berry_bush.glb")


ASSETS = [
    gen_birch,
    gen_willow,
    gen_cherry_blossom,
    gen_dead_tree,
    gen_apple_tree,
    gen_bamboo,
    gen_mushrooms,
    gen_sunflowers,
    gen_lavender,
    gen_berry_bush,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[flore] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
