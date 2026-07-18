# Boîte à outils du pack « grottes & rives » (gen_grotto_*.py / gen_shore_*.py) —
# style organique par métaballes fusionnées, généralisé depuis
# scripts/blender/proto_creature62_fox_organic.py (Option B du rapport
# qualité créatures), adapté au décor STATIQUE (pas de squelette ici : ces
# assets ne bougent pas, contrairement au renard du prototype).
#
# Réutilise telle quelle la boîte à outils du hameau (palette, primitives
# dures, export, vignette) — les accessoires nets (poutres, cristaux
# facettés) et le fond rocheux/boisé doivent rester les mêmes teintes que le
# hameau et le siège, cf. charte-graphique-assets-maison.
#
# Ce module n'est PAS exécutable seul : chaque gen_grotto_*.py/gen_shore_*.py
# fait `from organic_common import *` puis définit ses propres gen_xxx()/ASSETS.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import *  # noqa: F401,F403

import bpy
from mathutils import Vector

# ---------------------------------------------------------------------------
# Teintes propres au pack organique (nouvelles, absentes de hamlet_common) :
# ---------------------------------------------------------------------------

WATER_DARK = (0.10, 0.16, 0.20)  # eau stagnante/flaque souterraine
WATER_SHORE = (0.16, 0.30, 0.34)  # eau de rive, plus claire que l'eau souterraine
WATER_LIGHT = (0.55, 0.70, 0.72)  # eau vive/écume (cascade figée)
GLOW_CAVE = (0.35, 0.85, 0.55)  # bioluminescence des champignons souterrains
MOSS = (0.16, 0.34, 0.18)  # mousse/moisissure/algues sombres
DRIFTWOOD = (0.42, 0.38, 0.32)  # bois flotté grisé par l'eau, distinct de WOOD/WOOD_DARK


def meta_elem(mb_data, co, radius, size=(1.0, 1.0, 1.0), stiffness=2.0):
    """Un élément ellipsoïde de métaballe — cf. proto_creature62_fox_organic.py."""
    e = mb_data.elements.new()
    e.type = "ELLIPSOID"
    e.co = Vector(co)
    e.radius = radius
    e.size_x, e.size_y, e.size_z = size
    e.stiffness = stiffness
    return e


def organic_core(name, elements, material, resolution=0.05, threshold=0.22, ground_guard=0.0):
    """Construit un volume organique à partir d'éléments de métaballe, le
    convertit en mesh lissé, applique le garde-sol (aucun vertex sous
    `ground_guard`, même piège que hamlet_common.blob() avec jitter) et lui
    assigne un matériau.

    `elements` : liste de tuples (co, radius[, size[, stiffness]]).
    """
    mb_data = bpy.data.metaballs.new(name + "Meta")
    mb_data.resolution = resolution
    mb_data.render_resolution = resolution
    mb_data.threshold = threshold
    mb_obj = bpy.data.objects.new(name + "Meta", mb_data)
    bpy.context.collection.objects.link(mb_obj)
    for elem in elements:
        co, radius = elem[0], elem[1]
        size = elem[2] if len(elem) > 2 else (1.0, 1.0, 1.0)
        stiffness = elem[3] if len(elem) > 3 else 2.0
        meta_elem(mb_data, co, radius, size, stiffness)

    bpy.ops.object.select_all(action="DESELECT")
    mb_obj.select_set(True)
    bpy.context.view_layer.objects.active = mb_obj
    bpy.context.view_layer.update()
    bpy.ops.object.convert(target="MESH")
    core = bpy.context.active_object
    core.name = name
    bpy.ops.object.shade_smooth()

    core.data.update()
    if core.data.vertices:
        min_z = min(v.co.z for v in core.data.vertices)
        if min_z < ground_guard:
            core.location.z += ground_guard - min_z
            bpy.ops.object.transform_apply(location=True, rotation=False, scale=False)

    assign(core, material)  # noqa: F405 (hamlet_common.assign)
    return core


def spire_elements(base_radius, height, n=None, taper=0.22, density=0.35):
    """Éléments empilés qui s'amincissent en cône (stalagmite/stalactite/
    colonne) — recette validée manuellement (test_stalagmite3) : rapprochés
    pour fusionner en cône continu (pas un « collier de perles »), légère
    dérive latérale déterministe pour casser la symétrie parfaite d'un cône
    lisse.

    `n` : si omis, calculé automatiquement pour que l'espacement entre deux
    éléments consécutifs ne dépasse jamais `density` × le rayon moyen — un
    espacement trop grand par rapport au rayon fait fusionner les éléments en
    boules détachées au lieu d'un volume continu (piège rencontré sur
    gen_entrance_arch : n trop petit pour une grande hauteur → chapelet de
    perles, pas un pilier). Ne jamais fixer `n` à la main sans vérifier ce
    ratio.

    Pour une stalactite (suspendue, base large en haut), construire
    normalement puis miroir explicite des z dans l'appelant (un scale z=-1
    n'est pas fiable sur les normales avant join), cf. gen_grotto_rocks.
    """
    end_radius = base_radius * taper
    if n is None:
        avg_radius = (base_radius + end_radius) / 2
        n = max(int(height / max(avg_radius * density, 0.01)) + 1, 4)
    elements = []
    dx, dy = 0.0, 0.0
    step = height / max(n - 1, 1)
    for i in range(n):
        t = i / max(n - 1, 1)
        z = t * height
        r = base_radius + (end_radius - base_radius) * t
        dx += rng.uniform(-0.012, 0.012) * step
        dy += rng.uniform(-0.012, 0.012) * step
        stiffness = 1.5 if i == 0 else 1.3
        elements.append(((dx, dy, z), r, (1.0, 1.0, 1.0), stiffness))
    return elements


def boulder_elements(base_radius, n_bumps=4, squash=0.7, base_z=None):
    """Masse principale + bosses fusionnées en périphérie, aplatie — recette
    validée manuellement (test_shore_rock) : casse la sphère parfaite d'un
    rocher isolé sans le faire paraître composite (bosses très chevauchantes,
    jamais des sphères distinctes accolées)."""
    base_z = base_radius * squash if base_z is None else base_z
    elements = [((0, 0, base_z), base_radius, (1.3, 1.1, squash + 0.05), 1.4)]
    for i in range(n_bumps):
        a = i * math.tau / n_bumps + rng.uniform(-0.3, 0.3)
        r = base_radius * rng.uniform(0.55, 0.75)
        dist = base_radius * rng.uniform(0.4, 0.6)
        x, y = dist * math.cos(a), dist * math.sin(a)
        z = base_z * rng.uniform(0.6, 0.85)
        elements.append(((x, y, z), r, (1.0, 1.0, 0.9), 1.3))
    return elements


def chain_elements(waypoints, radius_start, radius_end=None, density=0.35, stiffness=1.3):
    """Chaîne d'éléments le long d'une polyligne (arche, racine, bois flotté)
    — même principe de densité que spire_elements : l'espacement entre
    éléments consécutifs ne dépasse jamais `density` × le rayon local, calculé
    automatiquement à partir de la longueur totale du chemin. Sans ça, une
    arche ou une racine sinueuse rendent en boules détachées (même piège que
    spire_elements sans son calcul auto de `n`).

    `waypoints` : liste de (x, y, z), au moins 2 points.
    `radius_start`/`radius_end` : rayon au premier/dernier point (interpolé
    linéairement le long du chemin) ; `radius_end` par défaut = `radius_start`.
    """
    radius_end = radius_start if radius_end is None else radius_end
    seg_lengths = []
    total = 0.0
    for i in range(len(waypoints) - 1):
        ax, ay, az = waypoints[i]
        bx, by, bz = waypoints[i + 1]
        d = math.sqrt((bx - ax) ** 2 + (by - ay) ** 2 + (bz - az) ** 2)
        seg_lengths.append(d)
        total += d
    avg_radius = (radius_start + radius_end) / 2
    n = max(int(total / max(avg_radius * density, 0.01)) + 1, len(waypoints))
    elements = []
    for i in range(n):
        t = i / max(n - 1, 1)
        target = t * total
        acc = 0.0
        x = y = z = 0.0
        for seg_i, seg_len in enumerate(seg_lengths):
            if target <= acc + seg_len or seg_i == len(seg_lengths) - 1:
                u = 0.0 if seg_len <= 0 else max(0.0, min(1.0, (target - acc) / seg_len))
                ax, ay, az = waypoints[seg_i]
                bx, by, bz = waypoints[seg_i + 1]
                x, y, z = ax + (bx - ax) * u, ay + (by - ay) * u, az + (bz - az) * u
                break
            acc += seg_len
        r = radius_start + (radius_end - radius_start) * t
        elements.append(((x, y, z), r, (1.0, 1.0, 1.0), stiffness))
    return elements
