# Sprint 6 du pack « grottes & rives » (creation3DBlenderOrganicSuite.md) :
# rives, petits props et ambiance — 7 assets, style organique (+ 2 pièces
# dures en contraste : ponton, nid).
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_shore_props.py
#
# Sortie : assets/models/shore_*.glb

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from organic_common import (  # noqa: E402
    MOSS,
    WATER_SHORE,
    WOOD,
    WOOD_DARK,
    boulder_elements,
    cube,
    cylinder,
    export,
    mat,
    organic_core,
    reset_scene,
    rng,
)

SHELL = (0.88, 0.84, 0.76)  # coquillages, pâle nacré
FOG_LIGHT = (0.80, 0.83, 0.82)  # brume basse, désaturée claire (opaque)


def gen_bank_moss():
    """Mousse de berge ~0.3 m : touffe humide, distincte de nature_moss_boulder
    (plus plate, plus verte, pas un rocher moussu)."""
    moss = mat("mousse_berge", MOSS, roughness=0.95)
    elements = [((0, 0, 0.03), 0.16, (1.2, 1.0, 0.22), 1.3)]
    for _ in range(4):
        x, y = rng.uniform(-0.10, 0.10), rng.uniform(-0.08, 0.08)
        elements.append(((x, y, 0.025), 0.07, (1.0, 1.0, 0.2), 1.3))
    organic_core("Mousse", elements, moss, resolution=0.01, ground_guard=0.0)
    export("shore_bank_moss.glb")


def gen_water_ripple():
    """Ondulation d'eau figée ~0.4 m : détail de surface stylisé, plat et
    lustré."""
    water = mat("ondulation", WATER_SHORE, roughness=0.05)
    # Épaisseur réelle (radius * size_z) doit rester nettement plus grande
    # que `resolution`, sinon la métaballe ne polygonise à rien (0 vertex,
    # export vide) — piège rencontré ici : size_z=0.03 sur radius=0.20 donne
    # une épaisseur de 0.006 pour une résolution de 0.012, trop fine.
    elements = [((0, 0, 0.025), 0.20, (1.0, 1.0, 0.09), 1.3),
                ((0.10, 0.03, 0.02), 0.09, (1.0, 1.0, 0.08), 1.3)]
    organic_core("Ondulation", elements, water, resolution=0.008, ground_guard=0.0)
    export("shore_water_ripple.glb")


def gen_short_pier():
    """Ponton rustique court ~1.6 m : jetée en bois — primitives dures,
    contraste avec la roche organique du lot."""
    wood = mat("bois_ponton", WOOD)
    wood_dark = mat("bois_ponton_sombre", WOOD_DARK)
    cube("Tablier", wood, (0, 0, 0.32), (1.6, 0.6, 0.06))
    for i, x in enumerate((-1.4, -0.9, -0.4, 0.1, 0.6, 1.1)):
        for sy in (-1, 1):
            cylinder(f"Pilotis{i}{sy}", wood_dark, (x, sy * 0.25, 0.14),
                      radius=0.05, depth=0.28, vertices=7)
    # Poteaux de garde à l'entrée du ponton (côté terre) : dimensions
    # corrigées — (0.4, 0.04, 0.5) donnait une planche plate flottante à
    # l'horizontale au lieu d'un poteau vertical (bug constaté au rendu).
    for sy in (-1, 1):
        cube(f"Poteau{sy}", wood_dark, (-1.4, sy * 0.27, 0.55), (0.06, 0.06, 0.5))
    export("shore_short_pier.glb")


def gen_shell_cluster():
    """Amas de coquillages ~0.25 m : petites coquilles groupées, pâles."""
    shell = mat("coquillage", SHELL, roughness=0.4)
    for i in range(5):
        x = rng.uniform(-0.10, 0.10)
        y = rng.uniform(-0.10, 0.10)
        r = rng.uniform(0.035, 0.06)
        elements = boulder_elements(base_radius=r, n_bumps=1, squash=0.5)
        elements = [((ex + x, ey + y, ez), er, es, est) for (ex, ey, ez), er, es, est in elements]
        organic_core(f"Coquille{i}", elements, shell, resolution=0.006, ground_guard=0.0)
    export("shore_shell_cluster.glb")


def gen_shore_nest():
    """Nid de rive ~0.3 m : brindilles tressées, vide (pas d'oiseau —
    fauna_* s'en charge) — primitives dures, pas organique."""
    twig = mat("brindille", (0.42, 0.34, 0.20))
    for i in range(10):
        a = i * math.tau / 10 + rng.uniform(-0.15, 0.15)
        r = 0.11 + rng.uniform(-0.015, 0.015)
        x, y = r * math.cos(a), r * math.sin(a)
        c = cylinder(f"Brin{i}", twig, (x, y, 0.03), radius=0.008, depth=0.10, vertices=5,
                      rotation=(math.pi / 2, 0, a))
        c.rotation_euler = (math.radians(80), 0, a)
    export("shore_nest.glb")


def gen_beached_algae():
    """Amas d'algues échouées ~0.5 m : tas organique sombre, aplati."""
    moss = mat("algues", MOSS, roughness=0.85)
    elements = boulder_elements(base_radius=0.22, n_bumps=4, squash=0.35)
    organic_core("Algues", elements, moss, resolution=0.015, ground_guard=0.0)
    export("shore_beached_algae.glb")


def gen_low_fog():
    """Brume basse de rive ~1.6 m : nappe stylisée au ras de l'eau, opaque
    (le moteur ignore l'alpha, cf. charte — même technique que hamlet_smoke)."""
    fog = mat("brume_rive", FOG_LIGHT, roughness=0.95)
    elements = [((0, 0, 0.05), 0.55, (1.3, 1.0, 0.10), 1.3),
                ((0.35, 0.10, 0.04), 0.35, (1.0, 1.0, 0.09), 1.3),
                ((-0.30, -0.08, 0.045), 0.32, (1.0, 1.0, 0.09), 1.3)]
    organic_core("Brume", elements, fog, resolution=0.03, ground_guard=0.0)
    export("shore_low_fog.glb")


ASSETS = [
    gen_bank_moss,
    gen_water_ripple,
    gen_short_pier,
    gen_shell_cluster,
    gen_shore_nest,
    gen_beached_algae,
    gen_low_fog,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[shore_props] pack complet : {len(ASSETS)} fichiers")
