# Génère 6 nouveaux décors « nature » STATIQUES (baobab, acacia, cactus,
# clochettes bleues, rond de champignons, bois flotté), en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_nature_pack3.py
#
# Sortie : assets/models/nature_{baobab,acacia,cactus,bluebells,
# toadstool_ring,driftwood}.glb (+ un preview PNG par asset).
#
# Même recette que gen_nature_pack.py (référence du style « décor statique
# joint ») : primitives → join → transform_apply → export sans rig/anim.
# Palette reprise à l'identique (mêmes constantes RGB) pour rester cohérent
# avec le reste de la carte — baobab/acacia complètent le pack savane
# (creature63-67) qui n'avait encore aucun arbre dédié à sa lisière.

import math
import os
import random

import bpy

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260719)  # reproductible


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def mat(name, rgb):
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


def cylinder(name, material, location, radius, depth, vertices=10, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=location, rotation=rotation
    )
    o = bpy.context.active_object
    o.name = name
    assign(o, material)
    return o


def cone(name, material, location, radius, depth, vertices=10, radius2=0.0, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices, radius1=radius, radius2=radius2, depth=depth,
        location=location, rotation=rotation,
    )
    o = bpy.context.active_object
    o.name = name
    assign(o, material)
    return o


def blob(name, material, location, radius, squash=1.0, jitter=0.0):
    """Icosphère (option écrasée/irrégulière) — feuillages, buissons, rochers.

    Garde-fou sol (piège documenté dans la charte maison) : le jitter ne
    descend jamais un vertex sous z=0 monde, même pour un blob posé bas.
    """
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=radius, location=location)
    o = bpy.context.active_object
    o.name = name
    o.scale = (1.0, 1.0, squash)
    if jitter > 0.0:
        for v in o.data.vertices:
            v.co.x += rng.uniform(-jitter, jitter)
            v.co.y += rng.uniform(-jitter, jitter)
            dz = rng.uniform(-jitter, jitter)
            world_z = location[2] + v.co.z * squash + dz * squash
            if world_z < 0.02:
                dz += (0.02 - world_z) / max(squash, 1e-3)
            v.co.z += dz
    assign(o, material)
    return o


def export(filename):
    """Joint tout, applique les transforms et exporte en GLB statique, puis
    rend une vignette (même caméra 3/4 que le reste du pack)."""
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
    print(f"[nature3] exporté {filename}")

    mesh = bpy.context.active_object
    pts = [mesh.matrix_world @ v.co for v in mesh.data.vertices]
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
    import mathutils
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
    print(f"[nature3] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette (identique à gen_nature_pack.py — mêmes valeurs RGB, cohérence
# carte) + quelques teintes propres aux nouveaux décors (baobab, cactus…).
# ---------------------------------------------------------------------------

BROWN = (0.32, 0.22, 0.11)
LEAF_DARK = (0.18, 0.42, 0.16)
LEAF_LIGHT = (0.24, 0.50, 0.18)
STONE = (0.45, 0.44, 0.42)
STONE_DARK = (0.36, 0.35, 0.34)

BAOBAB_BARK = (0.58, 0.42, 0.26)  # écorce grise-rosée caractéristique
BAOBAB_BARK_D = (0.46, 0.32, 0.20)
ACACIA_BARK = (0.34, 0.22, 0.13)
ACACIA_LEAF = (0.30, 0.46, 0.16)  # feuillage plat vert-olive de savane
CACTUS_GREEN = (0.20, 0.44, 0.24)
CACTUS_SPINE = (0.86, 0.80, 0.62)
BLUEBELL_PETAL = (0.26, 0.30, 0.66)
BLUEBELL_STEM = (0.22, 0.42, 0.18)
TOADSTOOL_CAP = (0.78, 0.16, 0.14)
TOADSTOOL_SPOT = (0.94, 0.92, 0.86)
TOADSTOOL_STEM = (0.88, 0.85, 0.76)
DRIFTWOOD = (0.62, 0.56, 0.48)  # bois délavé gris-beige
DRIFTWOOD_D = (0.50, 0.44, 0.37)


# ---------------------------------------------------------------------------
# Assets
# ---------------------------------------------------------------------------


def gen_baobab():
    """Baobab ~6 m : tronc renflé massif (empilement de sphères aplaties) +
    couronne de branches courtes et clairsemée — silhouette iconique de
    savane, pour la lisière du pack creature63-67."""
    bark = mat("ecorce_baobab", BAOBAB_BARK)
    bark_d = mat("ecorce_baobab_sombre", BAOBAB_BARK_D)
    leaf = mat("feuillage_baobab", LEAF_DARK)
    # Tronc : chaîne de sphères aplaties qui se chevauchent largement (piège
    # déjà documenté ailleurs : un simple cylindre est trop régulier/étroit
    # pour lire comme un baobab, mais des sphères mal chevauchées laissent
    # un « collier de perles »).
    for k, (z, r) in enumerate(((0.9, 1.15), (2.0, 1.35), (3.1, 1.15), (4.1, 0.85))):
        blob(f"Tronc{k}", bark if k % 2 == 0 else bark_d, (0, 0, z), radius=r,
             squash=1.3, jitter=0.10)
    # Couronne : branches courtes tordues qui rayonnent depuis le sommet.
    for i in range(7):
        a = i * math.tau / 7 + rng.uniform(-0.2, 0.2)
        r = rng.uniform(0.6, 1.1)
        x, y = r * math.cos(a), r * math.sin(a)
        z = 4.6 + rng.uniform(-0.2, 0.3)
        cylinder(f"Branche{i}", bark_d, (x * 0.5, y * 0.5, z), radius=0.10,
                 depth=1.2, vertices=6, rotation=(math.atan2(r, 1.5), 0, a))
        blob(f"Feuillage{i}", leaf, (x, y, z + 0.3), radius=0.45, squash=0.7, jitter=0.06)
    export("nature_baobab.glb")


def gen_acacia():
    """Acacia parasol ~4,5 m : tronc fin incliné + large couronne plate en
    parasol — deuxième silhouette caractéristique de la savane."""
    bark = mat("ecorce_acacia", ACACIA_BARK)
    leaf = mat("feuillage_acacia", ACACIA_LEAF)
    leaf_d = mat("feuillage_acacia_sombre", (0.22, 0.36, 0.13))
    # z légèrement remonté : un tronc incliné pivote autour de son centre,
    # donc le bord bas de la base descend sous z=0 (piège garde-fou déjà
    # documenté pour les blobs, valable aussi pour un cylindre incliné).
    cylinder("Tronc", bark, (0, 0.1, 1.52), radius=0.16, depth=3.0, vertices=8,
             rotation=(math.radians(6), 0, 0))
    cylinder("TroncHaut", bark, (0.15, 0.25, 3.1), radius=0.11, depth=1.2, vertices=7,
             rotation=(math.radians(18), math.radians(8), 0))
    # Couronne en parasol : large disque plat de blobs aplatis, débordant
    # largement le tronc (silhouette « table » caractéristique).
    for i in range(10):
        a = i * math.tau / 10
        r = rng.uniform(0.9, 1.7)
        x, y = 0.2 + r * math.cos(a), 0.3 + r * math.sin(a)
        m = leaf if i % 2 == 0 else leaf_d
        blob(f"Couronne{i}", m, (x, y, 3.75 + rng.uniform(-0.08, 0.10)),
             radius=rng.uniform(0.45, 0.65), squash=0.35, jitter=0.05)
    blob("CouronneCoeur", leaf, (0.2, 0.3, 3.8), radius=0.9, squash=0.32, jitter=0.06)
    export("nature_acacia.glb")


def gen_cactus():
    """Cactus saguaro ~2,2 m : tige centrale + 2 bras, arêtes épineuses."""
    green = mat("cactus_vert", CACTUS_GREEN)
    spine = mat("cactus_epine", CACTUS_SPINE)
    cylinder("Tige", green, (0, 0, 1.1), radius=0.26, depth=2.2, vertices=10)
    cone("Sommet", green, (0, 0, 2.25), radius=0.26, depth=0.3, vertices=10, radius2=0.05)
    for sx, h in ((-1, 1.3), (1, 1.6)):
        cylinder(f"Bras{sx}", green, (sx * 0.42, 0, h), radius=0.14, depth=0.9,
                  vertices=8, rotation=(0, math.radians(-sx * 75), 0))
        cylinder(f"BrasHaut{sx}", green, (sx * 0.42, 0, h + 0.55), radius=0.14,
                  depth=0.7, vertices=8)
    # Arêtes épineuses : petites rangées de piquants clairs le long de la tige.
    for i in range(8):
        z = 0.3 + i * 0.24
        for a in (0, math.pi / 2, math.pi, 3 * math.pi / 2):
            x, y = 0.27 * math.cos(a), 0.27 * math.sin(a)
            cone(f"Epine{i}_{a:.1f}", spine, (x, y, z), radius=0.015, depth=0.08,
                 vertices=4, rotation=(math.pi / 2, 0, a))
    export("nature_cactus.glb")


def gen_bluebells():
    """Clochettes bleues : tapis de 7 tiges à petites corolles pendantes,
    dans un rayon de 0,6 m — carpette de sous-bois."""
    stem = mat("tige_clochette", BLUEBELL_STEM)
    petal = mat("clochette_bleue", BLUEBELL_PETAL)
    for i in range(7):
        a = i * math.tau / 7 + rng.uniform(-0.3, 0.3)
        r = rng.uniform(0.05, 0.5)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.22, 0.34)
        cylinder(f"Tige{i}", stem, (x, y, h / 2), radius=0.008, depth=h, vertices=4,
                  rotation=(rng.uniform(-0.15, 0.15), rng.uniform(-0.15, 0.15), 0))
        for k in range(3):  # 3 clochettes penchées le long de la tige
            cone(f"Clochette{i}_{k}", petal, (x + k * 0.02, y, h - 0.02 - k * 0.06),
                 radius=0.028, depth=0.05, vertices=6,
                 rotation=(math.radians(140), 0, a))
    export("nature_bluebells.glb")


def gen_toadstool_ring():
    """Rond de sorcières ~1,4 m de diamètre : 6 champignons rouges à pois
    disposés en cercle autour d'un centre d'herbe rase."""
    cap = mat("chapeau_toadstool", TOADSTOOL_CAP)
    spot = mat("pois_toadstool", TOADSTOOL_SPOT)
    stem = mat("pied_toadstool", TOADSTOOL_STEM)
    for i in range(6):
        a = i * math.tau / 6
        r = 0.55 + rng.uniform(-0.05, 0.05)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.12, 0.18)
        cylinder(f"Pied{i}", stem, (x, y, h / 2), radius=0.025, depth=h, vertices=6)
        cone(f"Chapeau{i}", cap, (x, y, h + 0.04), radius=0.09, depth=0.09,
             vertices=8, radius2=0.02)
        for k in range(3):
            aa = rng.uniform(0, math.tau)
            rr = rng.uniform(0.02, 0.06)
            blob(f"Pois{i}_{k}", spot, (x + rr * math.cos(aa), y + rr * math.sin(aa),
                                        h + 0.07), radius=0.014, squash=0.6)
    export("nature_toadstool_ring.glb")


def gen_driftwood():
    """Bois flotté ~1,8 m : tronc échoué, écorce partiellement arrachée,
    posé au sol — décor de rive/plage."""
    wood = mat("bois_flotte", DRIFTWOOD)
    wood_d = mat("bois_flotte_sombre", DRIFTWOOD_D)
    cylinder("Tronc", wood, (0, 0, 0.16), radius=0.16, depth=1.8, vertices=9,
             rotation=(0, math.radians(90), math.radians(12)))
    cylinder("Branche", wood_d, (0.55, 0.12, 0.22), radius=0.08, depth=0.6, vertices=7,
             rotation=(0, math.radians(70), math.radians(30)))
    for k, (x, y) in enumerate(((-0.6, 0.05), (-0.1, -0.08), (0.4, 0.10))):
        blob(f"Ecorce{k}", wood_d, (x, y, 0.22), radius=0.10, squash=0.5, jitter=0.03)
    export("nature_driftwood.glb")


ASSETS = [gen_baobab, gen_acacia, gen_cactus, gen_bluebells, gen_toadstool_ring,
          gen_driftwood]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[nature3] pack statique complet : {len(ASSETS)} fichiers dans {OUT_DIR}")
