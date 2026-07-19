# Génère 10 nouveaux décors « nature » — flore et minéraux (9 statiques + 1
# animée), en Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_nature_pack4.py
#
# Sortie : assets/models/nature_{geode,quartz_spire,obsidian_shard,
# malachite_vein,salt_formation,fern_giant,orchid,bonsai,clover_patch}.glb
# (statiques) + nature_pampas_sway.glb (animée, clip Idle).
#
# Même recette que gen_nature_pack3.py (statiques) / gen_nature_pack3_animated.py
# (animée). Leçons de la session précédente appliquées dès la première passe :
# - chevauchement GÉNÉREUX entre pièces jointes (pas un simple contact tangent) ;
# - un cône Blender a sa base large en -Z et sa pointe en +Z par défaut — tout
#   cône censé pointer vers le BAS (goutte, glaçon, pointe de cristal tombante)
#   doit être tourné de 180°, sinon sa pointe (rayon quasi nul) se retrouve du
#   mauvais côté et laisse un trou visible au raccord ;
# - un cylindre/cône INCLINÉ pivote autour de son centre : sa base descend
#   sous z=0 si on ne remonte pas légèrement son point d'ancrage (garde-fou
#   déjà documenté, cf. nature_acacia) ;
# - une tige/fil trop fin (rayon < 0,015) ne « touche » jamais franchement une
#   grosse pièce voisine au rendu, même quand le calcul de recouvrement est
#   correct sur le papier (cf. nature_dandelion_sway).

import math
import os
import random

import bpy
import mathutils
from mathutils import Vector

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

rng = random.Random(20260720)


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


def cylinder(name, material, location, radius, depth, vertices=10, rotation=(0, 0, 0),
             radius2=None):
    if radius2 is not None:
        bpy.ops.mesh.primitive_cone_add(
            vertices=vertices, radius1=radius, radius2=radius2, depth=depth,
            location=location, rotation=rotation,
        )
    else:
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
    print(f"[nature4] exporté {filename}")

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
    print(f"[nature4] preview {scene.render.filepath}")


# ---------------------------------------------------------------------------
# Palette
# ---------------------------------------------------------------------------

LEAF_DARK = (0.18, 0.42, 0.16)
LEAF_LIGHT = (0.24, 0.50, 0.18)
STONE = (0.45, 0.44, 0.42)
STONE_DARK = (0.36, 0.35, 0.34)
BROWN = (0.32, 0.22, 0.11)

GEODE_ROCK = (0.42, 0.38, 0.34)
GEODE_ROCK_D = (0.30, 0.27, 0.24)
GEODE_CRYSTAL = (0.56, 0.42, 0.82)  # améthyste
GEODE_CRYSTAL_L = (0.72, 0.60, 0.90)
QUARTZ = (0.82, 0.86, 0.88)
QUARTZ_D = (0.68, 0.74, 0.78)
OBSIDIAN = (0.08, 0.08, 0.10)
OBSIDIAN_SHEEN = (0.20, 0.18, 0.24)
MALACHITE = (0.10, 0.42, 0.30)
MALACHITE_D = (0.05, 0.26, 0.19)
MALACHITE_ROCK = (0.40, 0.38, 0.34)
SALT_WHITE = (0.92, 0.90, 0.86)
SALT_SHADOW = (0.80, 0.77, 0.72)
FERN_GREEN = (0.16, 0.40, 0.18)
FERN_GREEN_L = (0.24, 0.50, 0.22)
ORCHID_PETAL = (0.86, 0.30, 0.58)
ORCHID_THROAT = (0.94, 0.82, 0.30)
ORCHID_STEM = (0.22, 0.40, 0.18)
BONSAI_BARK = (0.36, 0.24, 0.14)
BONSAI_LEAF = (0.22, 0.44, 0.20)
BONSAI_POT = (0.52, 0.24, 0.16)
CLOVER_GREEN = (0.20, 0.46, 0.20)
CLOVER_LUCKY = (0.28, 0.56, 0.24)
PAMPAS_STEM = (0.34, 0.42, 0.20)
PAMPAS_PLUME = (0.86, 0.78, 0.62)


# ---------------------------------------------------------------------------
# Assets statiques
# ---------------------------------------------------------------------------


def gen_geode():
    """Géode ~0,6 m : coquille rocheuse fendue, intérieur tapissé de pointes
    d'améthyste — décor de grotte/promontoire."""
    rock = mat("roche_geode", GEODE_ROCK)
    rock_d = mat("roche_geode_sombre", GEODE_ROCK_D)
    crystal = mat("cristal_geode", GEODE_CRYSTAL)
    crystal_l = mat("cristal_geode_clair", GEODE_CRYSTAL_L)
    # Coquille en coupe basse (pas un dôme fermé) : les cristaux doivent
    # nettement dépasser son profil pour se voir depuis l'extérieur — un
    # dôme complet centré sur les cristaux les avale tout entiers (constaté
    # au rendu, corrigé ici).
    blob("Coquille", rock, (0, 0.10, 0.14), radius=0.30, squash=0.5, jitter=0.05)
    blob("CoquilleD", rock_d, (0.16, 0.20, 0.10), radius=0.18, squash=0.45, jitter=0.04)
    # Pointes de cristal bien dégagées, groupées vers l'avant/le centre de la
    # coupe et nettement plus hautes que le rebord rocheux.
    for i in range(11):
        a = rng.uniform(-0.7, 0.7) - math.pi / 2  # éventail tourné vers -Y (face caméra)
        r = rng.uniform(0.02, 0.14)
        x, y = r * math.cos(a), r * math.sin(a) * 0.6 - 0.02
        h = rng.uniform(0.22, 0.42)
        m = crystal if i % 3 else crystal_l
        cylinder(f"Cristal{i}", m, (x, y, 0.16 + h / 2), radius=0.028, depth=h,
                  vertices=6, radius2=0.006)
    export("nature_geode.glb")


def gen_quartz_spire():
    """Flèche de quartz ~1,3 m : chaîne dense de cristaux hexagonaux qui
    s'élancent, socle rocheux — silhouette verticale de promontoire."""
    quartz = mat("quartz", QUARTZ)
    quartz_d = mat("quartz_sombre", QUARTZ_D)
    stone = mat("socle_quartz", STONE_DARK)
    blob("Socle", stone, (0, 0, 0.10), radius=0.24, squash=0.6, jitter=0.05)
    # Cristal principal + 4 secondaires, chevauchement dense à la base.
    cylinder("Principal", quartz, (0, 0, 0.65), radius=0.13, depth=1.1, vertices=6,
              radius2=0.03)
    for i in range(4):
        a = i * math.tau / 4 + rng.uniform(-0.2, 0.2)
        r = 0.12
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.45, 0.75)
        cylinder(f"Secondaire{i}", quartz_d if i % 2 else quartz,
                  (x, y, 0.12 + h / 2), radius=0.06, depth=h, vertices=6, radius2=0.012,
                  rotation=(rng.uniform(-0.15, 0.15), rng.uniform(-0.15, 0.15), 0))
    export("nature_quartz_spire.glb")


def gen_obsidian_shard():
    """Éclats d'obsidienne ~0,5 m : amas de plaques noires tranchantes au
    reflet violacé, posées en éventail."""
    obs = mat("obsidienne", OBSIDIAN)
    sheen = mat("obsidienne_reflet", OBSIDIAN_SHEEN)
    for i in range(6):
        a = i * math.tau / 6 + rng.uniform(-0.15, 0.15)
        r = rng.uniform(0.02, 0.14)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.22, 0.42)
        m = obs if i % 2 == 0 else sheen
        # z remonté (+0,03) : une plaque inclinée pivote autour de son
        # centre, donc son bord bas descend sous z=0 (garde-fou déjà
        # documenté pour nature_acacia).
        cylinder(f"Eclat{i}", m, (x, y, h / 2 + 0.03), radius=0.09, depth=h, vertices=4,
                  radius2=0.01, rotation=(rng.uniform(-0.25, 0.25), rng.uniform(-0.25, 0.25),
                                          a))
    blob("Base", obs, (0, 0, 0.05), radius=0.16, squash=0.4, jitter=0.03)
    export("nature_obsidian_shard.glb")


def gen_malachite_vein():
    """Veine de malachite ~0,5 m : rocher gris veiné de bandes vert émeraude
    concentriques — minerai affleurant."""
    rock = mat("roche_malachite", MALACHITE_ROCK)
    green = mat("malachite", MALACHITE)
    green_d = mat("malachite_sombre", MALACHITE_D)
    blob("Roche", rock, (0, 0, 0.24), radius=0.30, squash=0.75, jitter=0.08)
    # Bandes concentriques de malachite qui affleurent sur une face — rayons
    # resserrés pour rester DANS le volume du rocher (0,30 de rayon) : des
    # bandes trop larges dépassent largement la roche et se lisent comme un
    # anneau posé à côté plutôt qu'une veine affleurante (constaté au rendu).
    for k in range(5):
        r = 0.025 + k * 0.020
        m = green if k % 2 == 0 else green_d
        cylinder(f"Bande{k}", m, (0.20, 0.02, 0.26), radius=r, depth=0.10,
                  vertices=12, rotation=(0, math.radians(90), 0))
    export("nature_malachite_vein.glb")


def gen_salt_formation():
    """Formation de sel ~0,7 m : monticule cristallisé blanc, facettes
    anguleuses empilées — décor de rive salée/désert."""
    white = mat("sel_blanc", SALT_WHITE)
    shadow = mat("sel_ombre", SALT_SHADOW)
    blob("MonticuleBas", white, (0, 0, 0.16), radius=0.30, squash=0.6, jitter=0.06)
    # Centré presque à la verticale du premier monticule et bien plus bas
    # (chevauchement généreux) : un décalage XY + un centre trop haut laisse
    # une selle visible entre les deux blobs (piège déjà rencontré ailleurs).
    blob("MonticuleHaut", shadow, (0.0, 0.0, 0.16), radius=0.24, squash=0.85, jitter=0.05)
    for i in range(6):
        a = i * math.tau / 6
        r = rng.uniform(0.15, 0.26)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.10, 0.22)
        cylinder(f"Pointe{i}", white if i % 2 else shadow, (x, y, 0.10 + h / 2),
                  radius=0.06, depth=h, vertices=5, radius2=0.01)
    export("nature_salt_formation.glb")


def gen_fern_giant():
    """Fougère géante ~1,4 m : 6 grandes frondes courbes (rachis visible +
    paires de folioles serrées le long de la tige) qui rayonnent depuis une
    souche centrale — sous-bois dense, plus imposante que nature_fern.

    Piège corrigé en session : des folioles dispersées loin de tout rachis
    (rayon ~0,55×hauteur) se lisent comme un nuage de confettis plutôt
    qu'une plante structurée — ici chaque foliole reste collée à son rachis
    (décalage perpendiculaire court), qui lui-même est une chaîne DENSE de
    segments qui se chevauchent (même piège que les chaînes de cornes)."""
    stem = mat("tige_fougere_geante", (0.24, 0.30, 0.14))
    leaf = mat("fronde_fougere_geante", FERN_GREEN)
    leaf_l = mat("fronde_fougere_geante_claire", FERN_GREEN_L)
    blob("Souche", (mat("souche_fougere", BROWN)), (0, 0, 0.08), radius=0.13,
         squash=0.7, jitter=0.03)
    for i in range(6):
        a = i * math.tau / 6 + rng.uniform(-0.08, 0.08)
        tilt = rng.uniform(0.30, 0.45)  # penché vers l'extérieur, pas la verticale
        h = rng.uniform(1.0, 1.35)
        bone = f"Fronde{i}"
        dx, dy = math.sin(tilt) * math.cos(a), math.sin(tilt) * math.sin(a)
        dz = math.cos(tilt)
        # Direction exacte via quaternion (pas de composition d'angles Euler
        # X/Y approximative, qui dérive pour un azimut `a` quelconque).
        rach_euler = Vector((dx, dy, dz)).to_track_quat("Z", "Y").to_euler()
        # Rachis : chaîne de 7 segments COURTS le long d'une seule direction
        # de penché (pas de courbure composée qui exigerait un alignement
        # de rotation par segment, source d'erreur) — un seul axe tilt/az
        # suffit pour une silhouette arquée lisible et fiable.
        n = 7
        for k in range(n):
            t0, t1 = k / n, (k + 1) / n
            mt = (t0 + t1) / 2
            mx, my, mz = dx * h * mt, dy * h * mt, 0.10 + dz * h * mt
            seg_len = h / n * 1.4  # chevauchement généreux entre segments
            cylinder(bone, stem, (mx, my, mz), radius=0.022 - 0.0018 * k,
                      depth=seg_len, vertices=5, rotation=tuple(rach_euler))
        # Paire de folioles serrée de part et d'autre du rachis, décalage
        # court (jamais loin du rachis visible) — collée à chaque segment.
        perp = a + math.pi / 2
        for k in range(1, n + 1):
            t = k / n
            mx, my, mz = dx * h * t, dy * h * t, 0.10 + dz * h * t
            off = 0.07 + 0.05 * t
            m = leaf if k % 2 == 0 else leaf_l
            for sx in (-1, 1):
                blob(f"{bone}_{k}_{sx}", m,
                     (mx + sx * off * math.cos(perp), my + sx * off * math.sin(perp), mz),
                     radius=0.10 - 0.006 * k, squash=0.3, jitter=0.015)
    export("nature_fern_giant.glb")


def gen_orchid():
    """Orchidée ~0,4 m : tige arquée, 4 fleurs roses à gorge dorée — touche
    exotique pour la lisière tropicale."""
    stem = mat("tige_orchidee", ORCHID_STEM)
    petal = mat("petale_orchidee", ORCHID_PETAL)
    throat = mat("gorge_orchidee", ORCHID_THROAT)
    # Tige arquée : chaîne de segments courbes, chevauchement généreux.
    for k in range(7):
        t = k / 6.0
        x = 0.20 * math.sin(t * 1.3)
        z = 0.06 + t * 0.34
        cylinder(f"Tige{k}", stem, (x, 0, z), radius=0.014, depth=0.09,
                  vertices=5, rotation=(0, -t * 0.7, 0))
    # 3 fleurs bien espacées (pas 4 trop rapprochées, qui se lisaient comme
    # une seule pile de pétales confuse) ; rotation par direction exacte
    # (quaternion, pas une composition d'angles Euler X puis Z qui donnait
    # des pétales inclinés n'importe comment — même correctif que la
    # fougère géante).
    for i in range(3):
        t = 0.55 + i * 0.18
        x = 0.20 * math.sin(t * 1.3)
        z = 0.06 + t * 0.34
        for k in range(5):  # 5 pétales par fleur, en rosace face à -Y
            a = k * math.tau / 5
            half = math.radians(48)
            direction = Vector((math.sin(half) * math.cos(a), -math.cos(half),
                                 math.sin(half) * math.sin(a)))
            euler = direction.to_track_quat("Z", "Y").to_euler()
            cone(f"Fleur{i}_{k}", petal,
                 (x + 0.045 * math.cos(a), -0.02 + 0.045 * math.sin(a) * 0.3, z),
                 radius=0.032, depth=0.07, vertices=6, radius2=0.008,
                 rotation=tuple(euler))
        blob(f"Coeur{i}", throat, (x, -0.01, z), radius=0.020, squash=0.8)
    export("nature_orchid.glb")


def gen_bonsai():
    """Bonsaï ~0,5 m : tronc tors dans une coupelle, feuillage en coussins
    étagés — décor de jardin soigné."""
    bark = mat("ecorce_bonsai", BONSAI_BARK)
    leaf = mat("feuillage_bonsai", BONSAI_LEAF)
    leaf_d = mat("feuillage_bonsai_sombre", (0.16, 0.34, 0.15))
    pot = mat("coupelle_bonsai", BONSAI_POT)
    cylinder("Coupelle", pot, (0, 0, 0.06), radius=0.22, depth=0.12, vertices=12)
    # Tronc tors : chaîne de segments légèrement décalés, chevauchement
    # généreux (piège chaîne déjà documenté).
    for k in range(6):
        t = k / 5.0
        x = 0.05 * math.sin(t * 3.0)
        y = 0.04 * math.cos(t * 2.0)
        z = 0.12 + t * 0.32
        r = 0.045 - 0.02 * t
        cylinder(f"Tronc{k}", bark, (x, y, z), radius=r, depth=0.10, vertices=8)
    # Bloc de jonction tronc-feuillage : les coussins plats (squash=0.35)
    # sont trop aplatis pour toucher franchement le sommet fin du tronc au
    # rendu, même quand leur intervalle Z nominal recouvre le tronc sur le
    # papier — un volume rond et plus épais à la jonction règle ça de façon
    # fiable (même piège que nature_dandelion_sway/nature_windchime_sway).
    blob("Jonction", leaf, (0, 0, 0.42), radius=0.11, squash=0.75, jitter=0.03)
    # Coussins de feuillage étagés, chevauchant largement le tronc haut.
    for i, (r, z, m) in enumerate(((0.16, 0.44, leaf), (0.12, 0.53, leaf_d),
                                    (0.09, 0.60, leaf))):
        blob(f"Coussin{i}", m, (0.02 * i, 0.01 * i, z), radius=r, squash=0.4, jitter=0.04)
    export("nature_bonsai.glb")


def gen_clover_patch():
    """Carré de trèfles ~0,5 m : tapis dense de trèfles à 3 feuilles, un
    trèfle à 4 feuilles porte-bonheur caché dedans."""
    green = mat("trefle_vert", CLOVER_GREEN)
    lucky = mat("trefle_chance", CLOVER_LUCKY)
    for i in range(14):
        a = rng.uniform(0, math.tau)
        r = rng.uniform(0.02, 0.45)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.05, 0.09)
        cylinder(f"Tige{i}", green, (x, y, h / 2), radius=0.006, depth=h, vertices=4)
        n_leaves = 4 if i == 7 else 3  # le trèfle porte-bonheur, discret dans le tapis
        for k in range(n_leaves):
            la = k * math.tau / n_leaves
            blob(f"Feuille{i}_{k}", lucky if i == 7 else green,
                 (x + 0.028 * math.cos(la), y + 0.028 * math.sin(la), h + 0.01),
                 radius=0.022, squash=0.35, jitter=0.005)
    export("nature_clover_patch.glb")


ASSETS_STATIC = [gen_geode, gen_quartz_spire, gen_obsidian_shard, gen_malachite_vein,
                 gen_salt_formation, gen_fern_giant, gen_orchid, gen_bonsai, gen_clover_patch]

for gen in ASSETS_STATIC:
    reset_scene()
    gen()

print(f"[nature4] pack statique complet : {len(ASSETS_STATIC)} fichiers dans {OUT_DIR}")


# ---------------------------------------------------------------------------
# Asset animé : herbe de la pampa qui ondule
# ---------------------------------------------------------------------------

PARTS = []


def add_part(bone, material, create_op, location, scale=(1, 1, 1), rotation=(0, 0, 0)):
    create_op(location=location, rotation=rotation)
    ob = bpy.context.active_object
    ob.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    ob.data.materials.append(material)
    vg = ob.vertex_groups.new(name=bone)
    vg.add(range(len(ob.data.vertices)), 1.0, "REPLACE")
    PARTS.append(ob)
    return ob


def a_cylinder(bone, material, location, scale, rotation=(0, 0, 0), vertices=10):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cylinder_add(
            vertices=vertices, radius=1.0, depth=1.0, location=location, rotation=rotation
        )
    return add_part(bone, material, op, location, scale, rotation)


def a_cone(bone, material, location, scale, rotation=(0, 0, 0), vertices=10, radius2=0.0):
    def op(location, rotation):
        bpy.ops.mesh.primitive_cone_add(
            vertices=vertices, radius1=1.0, radius2=radius2, depth=1.0,
            location=location, rotation=rotation,
        )
    return add_part(bone, material, op, location, scale, rotation)


def build_rig(name, bones):
    bpy.ops.object.select_all(action="DESELECT")
    for ob in PARTS:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = PARTS[0]
    if len(PARTS) > 1:
        bpy.ops.object.join()
    mesh = bpy.context.active_object
    mesh.name = name

    bpy.ops.object.armature_add(location=(0, 0, 0))
    arm = bpy.context.active_object
    arm.name = name + "Rig"
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    root = eb[0]
    root.name = "Root"
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.3))
    for bname, (parent, head, tail) in bones.items():
        b = eb.new(bname)
        b.head, b.tail = Vector(head), Vector(tail)
        b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")

    mesh.parent = arm
    mod = mesh.modifiers.new("Armature", "ARMATURE")
    mod.object = arm
    return arm


def bake_idle(arm, length, keyer):
    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    for pb in arm.pose.bones:
        pb.rotation_mode = "XYZ"

    ad = arm.animation_data_create()
    ad.action = None
    keyer(arm)
    act = ad.action
    act.name = "Idle"
    track = ad.nla_tracks.new()
    track.name = "Idle"
    track.strips.new("Idle", 1, act)
    ad.action = None
    bpy.context.scene.frame_end = length

    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)
    bpy.ops.object.mode_set(mode="OBJECT")


def a_key_rot(arm, bone, frame, deg_xyz):
    pb = arm.pose.bones[bone]
    pb.rotation_euler = tuple(math.radians(v) for v in deg_xyz)
    pb.keyframe_insert("rotation_euler", frame=frame)


def export_and_preview(filename):
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=OUT_DIR + filename,
        export_format="GLB",
        export_skins=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_force_sampling=True,
        export_yup=True,
    )
    print(f"[nature4-anim] exporté {filename}")

    mesh = next(o for o in bpy.context.scene.objects if o.type == "MESH")
    bpy.context.view_layer.update()
    pts = [(mesh.matrix_world @ v.co) for v in mesh.data.vertices]
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
    print(f"[nature4-anim] preview {scene.render.filepath}")


def gen_pampas_sway():
    """Herbe de la pampa ~1,1 m : touffe de tiges à plumeaux qui ondulent
    en 3 groupes déphasés — variante vivante de haute prairie."""
    stem_m = mat("tige_pampa", PAMPAS_STEM)
    plume_m = mat("plumeau_pampa", PAMPAS_PLUME)
    plume_d_m = mat("plumeau_pampa_sombre", (0.74, 0.66, 0.50))
    for i in range(9):
        a = i * math.tau / 9 + rng.uniform(-0.15, 0.15)
        r = rng.uniform(0.04, 0.22)
        x, y = r * math.cos(a), r * math.sin(a)
        h = rng.uniform(0.75, 1.05)
        bone = f"Stalk{i % 3 + 1}"
        a_cylinder(bone, stem_m, (x, y, h / 2), (0.016, 0.016, h), vertices=5)
        # Plumeau : chaîne dense de petits cônes qui se chevauchent (piège
        # « collier de perles » déjà documenté pour les chaînes coniques).
        for k in range(5):
            t = k / 4.0
            a_cone(bone, plume_m if k % 2 == 0 else plume_d_m,
                   (x, y, h + 0.03 + t * 0.16), (0.05 - 0.025 * t, 0.05 - 0.025 * t, 0.10),
                   vertices=6, rotation=(rng.uniform(-0.2, 0.2), rng.uniform(-0.2, 0.2), 0))

    bones = {f"Stalk{k}": ("Root", (0, 0, 0.05), (0, 0, 1.05)) for k in range(1, 4)}
    arm = build_rig("PampasSway", bones)

    def keys(arm):
        # Vague ample dans un sens dominant, déphasée par groupe de tiges.
        # Boucle 1 = 89.
        wave = ((1, 0.0), (23, 1.0), (45, 0.1), (67, -0.8), (89, 0.0))
        amps = [10.0, 13.0, 8.0]
        lags = [0, 30, 60]
        for k in range(1, 4):
            for f, s in wave:
                ff = ((f - 1 + lags[k - 1]) % 88) + 1
                a_key_rot(arm, f"Stalk{k}", ff, (s * amps[k - 1], s * amps[k - 1] * 0.35, 0))

    bake_idle(arm, 89, keys)
    export_and_preview("nature_pampas_sway.glb")


reset_scene()
bpy.context.preferences.edit.keyframe_new_interpolation_type = "LINEAR"
PARTS.clear()
gen_pampas_sway()

print("[nature4-anim] pack animé complet : 1 fichier dans " + OUT_DIR)
