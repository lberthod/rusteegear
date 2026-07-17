# Deuxième pack « flore » : encore d'autres espèces d'arbres et de plantes,
# suite de gen_flora_pack.py (mêmes contraintes moteur, même palette) :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_flora_pack2.py
#
# Sortie : assets/models/nature_*.glb + nature_*_preview.png.
#
# Contraintes moteur (détail dans l'en-tête de gen_nature_pack.py) :
# meshes statiques joints, transform_apply, base_color_factor seulement,
# base à z=0 ; les assets solides présentent un flanc plein visible du
# raycast des créatures à 0,6 m. L'érable d'automne reste dans les ambres
# et orangés — le rouge franc demeure l'accent de zone (torii/bannière).

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
    """Cylindre orienté du point `base` vers `tip` (troncs noueux, branches)."""
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
    print(f"[flore2] exporté {filename}")

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
    print(f"[flore2] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette : teintes partagées + nouvelles teintes de ce pack.
# ---------------------------------------------------------------------------

BROWN = (0.32, 0.22, 0.11)
LEAF_DARK = (0.18, 0.42, 0.16)
LEAF_LIGHT = (0.24, 0.50, 0.18)
BERRY_RED = (0.68, 0.16, 0.20)
OAK_LEAF = (0.16, 0.38, 0.14)  # vert profond du chêne
AUTUMN_ORANGE = (0.82, 0.42, 0.10)  # érable d'automne (ambre, pas l'accent rouge)
AUTUMN_AMBER = (0.72, 0.30, 0.08)
CYPRESS = (0.12, 0.30, 0.16)  # vert bleuté du cyprès
OLIVE_LEAF = (0.45, 0.52, 0.35)  # argenté de l'olivier
OLIVE_TRUNK = (0.42, 0.36, 0.28)  # bois pâle noueux
PARASOL = (0.20, 0.40, 0.18)  # ombrelle du pin parasol
PUMPKIN = (0.85, 0.45, 0.10)  # citrouilles
WHEAT = (0.78, 0.62, 0.25)  # blé mûr
MOSS = (0.30, 0.48, 0.20)  # mousse du tronc couché

# ---------------------------------------------------------------------------
# Arbres
# ---------------------------------------------------------------------------


def gen_oak():
    """Grand chêne ~5,5 m : tronc large + canopée massive en 4 boules — le
    doyen de la forêt, repère aussi lisible que la tour de guet."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuille_chene", OAK_LEAF)
    leaf2 = mat("feuillage_b", LEAF_LIGHT)
    taper("Tronc", trunk, (0, 0, 1.3), r_bottom=0.40, r_top=0.26, depth=2.6)
    branch("Depart1", trunk, (0.2, 0.1, 2.2), (1.0, 0.5, 3.2), radius=0.14)
    branch("Depart2", trunk, (-0.15, -0.1, 2.3), (-1.0, -0.4, 3.3), radius=0.14)
    blob("Canopee1", leaf, (0.0, 0.0, 4.0), radius=1.7, squash=0.75, jitter=0.12, subdiv=2)
    blob("Canopee2", leaf, (1.15, 0.55, 3.4), radius=1.0, squash=0.75, jitter=0.09, subdiv=2)
    blob("Canopee3", leaf, (-1.1, -0.5, 3.5), radius=0.95, squash=0.75, jitter=0.09, subdiv=2)
    blob("Canopee4", leaf2, (0.15, -0.75, 3.65), radius=0.8, squash=0.7, jitter=0.07, subdiv=2)
    export_and_preview("nature_oak.glb")


def gen_maple_autumn():
    """Érable d'automne ~3,6 m : canopée orange/ambre — tache chaude unique de
    la lisière (reste dans les ambres, le rouge franc est l'accent de zone)."""
    trunk = mat("tronc_sombre", (0.26, 0.17, 0.09))
    fol_a = mat("automne_orange", AUTUMN_ORANGE)
    fol_b = mat("automne_ambre", AUTUMN_AMBER)
    taper("Tronc", trunk, (0, 0, 1.0), r_bottom=0.22, r_top=0.14, depth=2.0)
    blob("Canopee1", fol_a, (0.0, 0.0, 2.85), radius=1.15, squash=0.85, jitter=0.09, subdiv=2)
    blob("Canopee2", fol_b, (0.6, 0.3, 2.35), radius=0.65, squash=0.75, jitter=0.06, subdiv=2)
    blob("Canopee3", fol_b, (-0.55, -0.3, 2.5), radius=0.6, squash=0.75, jitter=0.06, subdiv=2)
    # Quelques feuilles tombées au pied — raccroche l'arbre au sol.
    for i in range(5):
        a = i * math.tau / 5 + 0.4
        r = rng.uniform(0.5, 1.1)
        blob(
            f"FeuilleSol{i}", fol_a if i % 2 == 0 else fol_b,
            (r * math.cos(a), r * math.sin(a), 0.03), radius=0.09, squash=0.3,
        )
    export_and_preview("nature_maple_autumn.glb")


def gen_cypress():
    """Cyprès ~4,5 m : colonne effilée vert sombre — ponctuation verticale des
    entrées et allées, silhouette très différente des sapins étagés."""
    trunk = mat("tronc", BROWN)
    leaf = mat("feuille_cypres", CYPRESS)
    cylinder("Tronc", trunk, (0, 0, 0.25), radius=0.10, depth=0.5)
    cone("Colonne", leaf, (0, 0, 2.45), radius=0.62, depth=4.0, vertices=9)
    blob("Base", leaf, (0, 0, 0.6), radius=0.55, squash=0.7, jitter=0.05)
    export_and_preview("nature_cypress.glb")


def gen_olive():
    """Olivier ~2,8 m : tronc pâle noueux en deux temps + couronne argentée
    basse — l'arbre des cours et des terrasses du hameau."""
    trunk = mat("tronc_olivier", OLIVE_TRUNK)
    leaf = mat("feuille_olivier", OLIVE_LEAF)
    leaf_d = mat("feuille_olivier_sombre", (0.36, 0.44, 0.28))
    branch("Tronc1", trunk, (0, 0, 0), (0.3, -0.15, 1.1), radius=0.20)
    branch("Tronc2", trunk, (0.28, -0.14, 1.0), (0.05, 0.15, 2.1), radius=0.15)
    blob("Couronne1", leaf, (0.1, 0.1, 1.95), radius=0.95, squash=0.65, jitter=0.09, subdiv=2)
    blob("Couronne2", leaf_d, (0.6, -0.2, 1.7), radius=0.55, squash=0.65, jitter=0.06, subdiv=2)
    blob("Couronne3", leaf_d, (-0.45, 0.3, 1.75), radius=0.5, squash=0.6, jitter=0.06, subdiv=2)
    export_and_preview("nature_olive.glb")


def gen_pine_parasol():
    """Pin parasol ~4 m : long tronc nu incliné + ombrelle plate tout en haut —
    contrepoint méditerranéen des sapins pointus."""
    trunk = mat("tronc", BROWN)
    leaf = mat("ombrelle", PARASOL)
    branch("Tronc", trunk, (0, 0, 0), (0.35, 0.2, 2.9), radius=0.17)
    branch("Fourche", trunk, (0.3, 0.17, 2.6), (0.9, 0.5, 3.2), radius=0.09)
    blob("Ombrelle1", leaf, (0.4, 0.25, 3.45), radius=1.5, squash=0.4, jitter=0.10, subdiv=2)
    blob("Ombrelle2", leaf, (1.0, 0.55, 3.3), radius=0.8, squash=0.45, jitter=0.07, subdiv=2)
    export_and_preview("nature_pine_parasol.glb")


# ---------------------------------------------------------------------------
# Plantes basses et sol
# ---------------------------------------------------------------------------


def gen_pumpkins():
    """Carré de citrouilles ~0,45 m : 4 citrouilles côtelées + tiges — le
    potager d'automne du hameau (non solide)."""
    pumpkin = mat("citrouille", PUMPKIN)
    pumpkin_d = mat("citrouille_sombre", (0.70, 0.34, 0.07))
    stem = mat("tige_citrouille", (0.30, 0.42, 0.14))
    spots = [(0.0, 0.0, 0.42), (0.72, 0.25, 0.30), (-0.55, 0.4, 0.26), (0.15, -0.6, 0.24)]
    for i, (x, y, r) in enumerate(spots):
        m = pumpkin if i % 2 == 0 else pumpkin_d
        # Côtes : 5 sphères écrasées qui se chevauchent autour de l'axe.
        for k in range(5):
            a = k * math.tau / 5
            blob(
                f"Cote{i}_{k}", m,
                (x + 0.18 * r * math.cos(a), y + 0.18 * r * math.sin(a), r * 0.55),
                radius=r * 0.85, squash=0.62, subdiv=2, smooth=True,
            )
        cylinder(f"Queue{i}", stem, (x, y, r * 0.95 + 0.06), radius=0.035, depth=0.14, vertices=6)
    export_and_preview("nature_pumpkins.glb")


def gen_wheat():
    """Touffe de blé mûr ~0,8 m : 9 tiges dorées à épis — champs autour du
    hameau, répond au chaume des toits (non solide)."""
    stalk = mat("tige_ble", (0.62, 0.52, 0.22))
    ear = mat("epi_ble", WHEAT)
    for i in range(9):
        a = i * math.tau / 9 + rng.uniform(-0.2, 0.2)
        r = rng.uniform(0.05, 0.32)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.55, 0.8)
        cylinder(f"Tige{i}", stalk, (x, y, h / 2), radius=0.018, depth=h, vertices=5)
        cone(f"Epi{i}", ear, (x, y, h + 0.10), radius=0.045, depth=0.22, vertices=6)
    export_and_preview("nature_wheat.glb")


def gen_bramble():
    """Fourré de ronces ~0,9 m : masses épineuses sombres piquées de mûres.
    Prévu solide (haie naturelle) : les masses montent à 0,9 m, flanc plein
    visible des sondes à 0,6 m."""
    thorn = mat("ronce", (0.16, 0.30, 0.12))
    thorn_d = mat("ronce_sombre", (0.12, 0.22, 0.10))
    berry = mat("mure", (0.22, 0.10, 0.26))
    blob("Masse1", thorn, (0, 0, 0.45), radius=0.55, squash=0.85, jitter=0.09, subdiv=2)
    blob("Masse2", thorn_d, (0.5, 0.25, 0.35), radius=0.4, squash=0.8, jitter=0.07, subdiv=2)
    blob("Masse3", thorn_d, (-0.45, -0.2, 0.38), radius=0.42, squash=0.8, jitter=0.07, subdiv=2)
    for i in range(6):
        a = i * math.tau / 6 + 0.5
        r = rng.uniform(0.35, 0.55)
        z = 0.45 + rng.uniform(-0.1, 0.25)
        blob(f"Mure{i}", berry, (r * math.cos(a), r * math.sin(a), z), radius=0.05, smooth=True)
    # Tiges arquées qui dépassent — la silhouette « ronce ».
    for i in range(4):
        a = i * math.tau / 4 + 0.2
        branch(
            f"Tige{i}", thorn_d,
            (0.3 * math.cos(a), 0.3 * math.sin(a), 0.5),
            (0.85 * math.cos(a), 0.85 * math.sin(a), 0.85 + 0.1 * (i % 2)),
            radius=0.025,
        )
    export_and_preview("nature_bramble.glb")


def gen_daisies():
    """Coin de pâquerettes ~0,25 m : 7 fleurs blanches à cœur doré dans
    l'herbe — prairie, plus discret que le parterre de fleurs (non solide)."""
    grass = mat("herbe_b", LEAF_LIGHT)
    petal = mat("petale_blanc", (0.92, 0.92, 0.88))
    heart = mat("coeur_dore", (0.88, 0.68, 0.18))
    for i in range(7):
        a = i * math.tau / 7 + rng.uniform(-0.3, 0.3)
        r = rng.uniform(0.06, 0.4)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.12, 0.22)
        cylinder(f"Tige{i}", grass, (x, y, h / 2), radius=0.012, depth=h, vertices=5)
        cylinder(
            f"Corolle{i}", petal, (x, y, h + 0.02), radius=0.06, depth=0.025, vertices=8,
        )
        blob(f"Coeur{i}", heart, (x, y, h + 0.045), radius=0.025, squash=0.6)
    for i in range(4):
        a = i * math.tau / 4 + 0.6
        r = rng.uniform(0.15, 0.45)
        cone(
            f"Brin{i}", grass,
            (r * math.cos(a), r * math.sin(a), 0.09), radius=0.02, depth=0.18, vertices=4,
        )
    export_and_preview("nature_daisies.glb")


def gen_mossy_log():
    """Tronc couché moussu ~2,2 m : gros fût au sol + coussins de mousse qui
    culminent à ~0,9 m — les coussins donnent le flanc visible des sondes à
    0,6 m (le fût seul, à 0,7 m de haut, frôlerait le rayon)."""
    wood = mat("bois_mort", (0.38, 0.33, 0.28))
    heart = mat("coeur", (0.62, 0.48, 0.30))
    moss = mat("mousse", MOSS)
    fut = cylinder(
        "Fut", wood, (0, 0, 0.35), radius=0.35, depth=2.2, vertices=9,
        rotation=(0, math.pi / 2, 0),
    )
    bpy.ops.object.shade_smooth()
    for sx in (-1, 1):
        cylinder(
            f"Coupe{sx}", heart, (sx * 1.101, 0, 0.35), radius=0.30, depth=0.02, vertices=9,
            rotation=(0, math.pi / 2, 0),
        )
    blob("Mousse1", moss, (0.2, 0.05, 0.75), radius=0.38, squash=0.6, jitter=0.06)
    blob("Mousse2", moss, (-0.6, -0.08, 0.68), radius=0.28, squash=0.6, jitter=0.05)
    blob("Mousse3", moss, (0.75, 0.1, 0.62), radius=0.22, squash=0.55, jitter=0.04)
    blob("MousseSol", moss, (-0.3, 0.35, 0.08), radius=0.25, squash=0.35, jitter=0.04)
    export_and_preview("nature_mossy_log.glb")


ASSETS = [
    gen_oak,
    gen_maple_autumn,
    gen_cypress,
    gen_olive,
    gen_pine_parasol,
    gen_pumpkins,
    gen_wheat,
    gen_bramble,
    gen_daisies,
    gen_mossy_log,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[flore2] pack complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
