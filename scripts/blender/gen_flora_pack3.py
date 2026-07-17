# Troisième pack « flore » : 10 espèces de plus, suite de gen_flora_pack2.py
# (mêmes contraintes moteur, même palette, helpers améliorés : troncs coniques
# lissés, canopées subdiv 2, fruits en normales lisses) :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_flora_pack3.py
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

rng = random.Random(20260720)  # reproductible


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
    print(f"[flore3] exporté {filename}")

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
    print(f"[flore3] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette : teintes partagées + nouvelles teintes de ce pack.
# ---------------------------------------------------------------------------

BROWN = (0.32, 0.22, 0.11)
LEAF_DARK = (0.18, 0.42, 0.16)
LEAF_LIGHT = (0.24, 0.50, 0.18)
BERRY_RED = (0.68, 0.16, 0.20)
LAVENDER = (0.55, 0.42, 0.78)
POPLAR_LEAF = (0.30, 0.52, 0.22)  # vert frais du peuplier
GINKGO_GOLD = (0.80, 0.68, 0.20)  # éventails dorés (≠ jaune chaud des lanternes)
MAGNOLIA = (0.94, 0.88, 0.90)  # fleurs blanc rosé
MAGNOLIA_PINK = (0.85, 0.60, 0.70)
HAZEL_LEAF = (0.22, 0.46, 0.18)
PLUM = (0.42, 0.22, 0.48)  # prunes violettes (répond à la lavande)
TOPIARY = (0.16, 0.40, 0.18)  # buis dense
CATTAIL_HEAD = (0.36, 0.22, 0.12)  # massette brune
THISTLE = (0.58, 0.44, 0.72)  # fleur de chardon
TOMATO = (0.75, 0.22, 0.14)  # tomates mûres (rouge discret, pas l'accent)
CABBAGE = (0.42, 0.58, 0.32)  # chou pommé

# ---------------------------------------------------------------------------
# Arbres
# ---------------------------------------------------------------------------


def gen_poplar():
    """Peuplier ~5,2 m : colonne feuillue élancée sur tronc fin — l'alignement
    de bord de route, plus doux que le cyprès sombre."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuille_peuplier", POPLAR_LEAF)
    leaf_d = mat("feuillage_a", LEAF_DARK)
    taper("Tronc", trunk, (0, 0, 0.8), r_bottom=0.16, r_top=0.10, depth=1.6)
    blob("Colonne1", leaf, (0, 0, 2.1), radius=0.75, squash=1.5, jitter=0.08, subdiv=2)
    blob("Colonne2", leaf, (0, 0, 3.5), radius=0.62, squash=1.6, jitter=0.07, subdiv=2)
    blob("Colonne3", leaf_d, (0.1, 0.05, 4.6), radius=0.42, squash=1.5, jitter=0.05, subdiv=2)
    export_and_preview("nature_poplar.glb")


def gen_ginkgo():
    """Ginkgo ~3,6 m : couronne dorée en éventails superposés — l'arbre doré
    près du torii (or feuille, distinct du jaune chaud des lanternes)."""
    trunk = mat("tronc", BROWN)
    gold = mat("feuille_ginkgo", GINKGO_GOLD)
    gold_d = mat("feuille_ginkgo_sombre", (0.68, 0.55, 0.16))
    taper("Tronc", trunk, (0, 0, 1.1), r_bottom=0.18, r_top=0.11, depth=2.2)
    branch("Branche1", trunk, (0.05, 0, 1.9), (0.7, 0.3, 2.5), radius=0.07)
    blob("Eventail1", gold, (0, 0, 3.0), radius=1.0, squash=0.55, jitter=0.08, subdiv=2)
    blob("Eventail2", gold_d, (0.6, 0.3, 2.55), radius=0.6, squash=0.5, jitter=0.06, subdiv=2)
    blob("Eventail3", gold, (-0.5, -0.25, 2.7), radius=0.55, squash=0.5, jitter=0.06, subdiv=2)
    blob("Cime", gold_d, (0.05, 0.05, 3.45), radius=0.5, squash=0.6, jitter=0.05, subdiv=2)
    export_and_preview("nature_ginkgo.glb")


def gen_magnolia():
    """Magnolia ~2,6 m : couronne basse constellée de grosses fleurs blanc rosé
    — l'arbre d'apparat des cours, en fleur toute l'année."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuillage_a", LEAF_DARK)
    bloom = mat("fleur_magnolia", MAGNOLIA)
    bloom_p = mat("fleur_magnolia_rose", MAGNOLIA_PINK)
    branch("Tronc", trunk, (0, 0, 0), (0.2, 0.1, 1.2), radius=0.15)
    branch("Branche1", trunk, (0.15, 0.08, 1.0), (0.75, 0.4, 1.7), radius=0.07)
    branch("Branche2", trunk, (0.15, 0.08, 1.1), (-0.55, -0.35, 1.75), radius=0.07)
    blob("Couronne", leaf, (0.1, 0.05, 1.9), radius=1.0, squash=0.7, jitter=0.08, subdiv=2)
    for i in range(8):
        a = i * math.tau / 8 + 0.3
        # En surface de la couronne (rayon 1.0 centrée z=1.9) : posées dessus,
        # pas noyées dedans.
        r = rng.uniform(0.92, 1.05)
        z = 1.9 + rng.uniform(-0.2, 0.45)
        m = bloom if i % 2 == 0 else bloom_p
        blob(f"Fleur{i}", m, (r * math.cos(a), r * math.sin(a), z), radius=0.15,
             squash=0.8, smooth=True)
    export_and_preview("nature_magnolia.glb")


def gen_hazel():
    """Noisetier ~2,2 m : cépée de 4 brins arqués + feuillage en touffes — le
    buisson haut des lisières, silhouette évasée sans tronc unique."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuille_noisetier", HAZEL_LEAF)
    leaf2 = mat("feuillage_b", LEAF_LIGHT)
    tips = []
    for i in range(4):
        a = i * math.tau / 4 + 0.4
        tip = (0.75 * math.cos(a), 0.75 * math.sin(a), rng.uniform(1.5, 1.9))
        branch(f"Brin{i}", trunk, (0.12 * math.cos(a), 0.12 * math.sin(a), 0), tip, radius=0.06)
        tips.append(tip)
    for i, (x, y, z) in enumerate(tips):
        m = leaf if i % 2 == 0 else leaf2
        blob(f"Touffe{i}", m, (x, y, z + 0.25), radius=0.55, squash=0.8, jitter=0.06, subdiv=2)
    blob("Coeur", leaf, (0, 0, 1.5), radius=0.6, squash=0.85, jitter=0.07, subdiv=2)
    export_and_preview("nature_hazel.glb")


def gen_plum():
    """Prunier ~2,8 m : couronne compacte piquée de prunes violettes — le
    verger côté couleur froide (répond à la lavande du hameau)."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuillage_a", LEAF_DARK)
    leaf2 = mat("feuillage_b", LEAF_LIGHT)
    plum_m = mat("prune", PLUM)
    taper("Tronc", trunk, (0, 0, 0.7), r_bottom=0.17, r_top=0.11, depth=1.4)
    blob("Couronne", leaf, (0, 0, 2.0), radius=1.0, squash=0.85, jitter=0.08, subdiv=2)
    blob("Couronne2", leaf2, (-0.45, 0.3, 1.75), radius=0.55, squash=0.75, jitter=0.06, subdiv=2)
    for i in range(6):
        a = i * math.tau / 6 + 0.5
        # En surface de la couronne (rayon 1.0 centrée z=2.0), légèrement sous
        # l'équateur : les fruits pendent, visibles du sol.
        r = rng.uniform(0.92, 1.02)
        z = 1.85 + rng.uniform(-0.25, 0.2)
        blob(f"Prune{i}", plum_m, (r * math.cos(a), r * math.sin(a), z), radius=0.085,
             subdiv=2, smooth=True)
    export_and_preview("nature_plum.glb")


# ---------------------------------------------------------------------------
# Plantes basses et potager
# ---------------------------------------------------------------------------


def gen_topiary():
    """Buis taillé ~1,3 m : boule dense sur pied court, dans un bac de bois —
    le jardin soigné devant l'auberge. Solide : bac plein + boule à hauteur
    des sondes (0,6 m dans la boule)."""
    wood_dark = mat("bois_sombre", (0.28, 0.18, 0.09))
    leaf = mat("buis", TOPIARY)
    cube("Bac", wood_dark, (0, 0, 0.14), (0.55, 0.55, 0.28))
    cylinder("Pied", wood_dark, (0, 0, 0.36), radius=0.06, depth=0.2, vertices=8)
    blob("Boule", leaf, (0, 0, 0.85), radius=0.45, squash=1.0, jitter=0.03,
         subdiv=2, smooth=True)
    export_and_preview("nature_topiary.glb")


def gen_cattails():
    """Quenouilles ~1,3 m : tiges fines à massettes brunes + feuilles-lames —
    la berge d'étang, distinct des roseaux à épis (non solide)."""
    stem = mat("tige_roseau", (0.30, 0.48, 0.20))
    head = mat("massette", CATTAIL_HEAD)
    blade = mat("lame", (0.26, 0.44, 0.18))
    for i in range(5):
        a = i * math.tau / 5 + rng.uniform(-0.25, 0.25)
        r = rng.uniform(0.06, 0.28)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.9, 1.25)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.018, depth=h, vertices=6)
        o = cylinder(f"Massette{i}", head, (x, y, h + 0.14), radius=0.045, depth=0.3, vertices=8)
        bpy.ops.object.shade_smooth()
        cone(f"Pointe{i}", stem, (x, y, h + 0.36), radius=0.012, depth=0.14, vertices=5)
    for i in range(4):
        a = i * math.tau / 4 + 0.6
        r = rng.uniform(0.12, 0.3)
        cone(
            f"Lame{i}", blade, (r * math.cos(a), r * math.sin(a), 0.45),
            radius=0.035, depth=0.9, vertices=4,
        )
    export_and_preview("nature_cattails.glb")


def gen_thistle():
    """Chardons ~0,7 m : rosettes épineuses + capitules violets — la friche
    au pied des ruines et de l'arbre mort (non solide)."""
    stem = mat("tige_chardon", (0.32, 0.42, 0.28))
    bloom = mat("fleur_chardon", THISTLE)
    spike = mat("bractee", (0.28, 0.36, 0.22))
    for i in range(4):
        a = i * math.tau / 4 + rng.uniform(-0.3, 0.3)
        r = rng.uniform(0.05, 0.25)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.45, 0.68)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.02, depth=h, vertices=6)
        cone(f"Bractee{i}", spike, (x, y, h - 0.02), radius=0.07, depth=0.14, vertices=8)
        blob(f"Capitule{i}", bloom, (x, y, h + 0.07), radius=0.055, squash=1.2, smooth=True)
        blob(f"Rosette{i}", spike, (x, y, 0.06), radius=0.14, squash=0.3, jitter=0.03)
    export_and_preview("nature_thistle.glb")


def gen_tomatoes():
    """Plants de tomates tuteurés ~1,2 m : 3 tuteurs de bois, feuillage grimpant
    et tomates mûres — le potager d'été (non solide, rouge discret)."""
    stake = mat("tuteur", (0.45, 0.30, 0.15))
    leaf = mat("feuille_tomate", LEAF_LIGHT)
    leaf_d = mat("feuille_tomate_sombre", LEAF_DARK)
    fruit = mat("tomate", TOMATO)
    spots = [(0.0, 0.0), (0.55, 0.15), (-0.5, -0.1)]
    for i, (x, y) in enumerate(spots):
        h = rng.uniform(1.0, 1.25)
        cylinder(f"Tuteur{i}", stake, (x, y, h / 2), radius=0.025, depth=h, vertices=6)
        for k in range(3):
            z = 0.3 + k * (h - 0.4) / 2
            m = leaf if (i + k) % 2 == 0 else leaf_d
            blob(f"Feuillage{i}_{k}", m, (x + rng.uniform(-0.08, 0.08),
                                          y + rng.uniform(-0.08, 0.08), z),
                 radius=0.20 - 0.03 * k, squash=0.8, jitter=0.04, subdiv=2)
        for k in range(3):
            a = k * math.tau / 3 + i
            blob(f"Tomate{i}_{k}", fruit,
                 (x + 0.16 * math.cos(a), y + 0.16 * math.sin(a),
                  0.35 + k * 0.28 + rng.uniform(-0.05, 0.05)),
                 radius=0.055, subdiv=2, smooth=True)
    export_and_preview("nature_tomatoes.glb")


def gen_cabbages():
    """Rangée de choux ~0,3 m : 4 pommes feuillues au sol — le potager
    d'hiver, à aligner en rangs (non solide)."""
    heart = mat("chou", CABBAGE)
    outer = mat("feuille_chou", (0.32, 0.50, 0.26))
    spots = [(0.0, 0.0), (0.6, 0.1), (-0.6, -0.05), (0.05, 0.6)]
    for i, (x, y) in enumerate(spots):
        r = rng.uniform(0.16, 0.22)
        blob(f"Pomme{i}", heart, (x, y, r * 0.75), radius=r, squash=0.85,
             subdiv=2, smooth=True)
        for k in range(4):
            a = k * math.tau / 4 + i * 0.5
            blob(f"FeuilleExt{i}_{k}", outer,
                 (x + r * 0.9 * math.cos(a), y + r * 0.9 * math.sin(a), r * 0.45),
                 radius=r * 0.55, squash=0.5, jitter=0.02)
    export_and_preview("nature_cabbages.glb")


ASSETS = [
    gen_poplar,
    gen_ginkgo,
    gen_magnolia,
    gen_hazel,
    gen_plum,
    gen_topiary,
    gen_cattails,
    gen_thistle,
    gen_tomatoes,
    gen_cabbages,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[flore3] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
