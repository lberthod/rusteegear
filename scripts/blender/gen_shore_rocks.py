# Sprint 4 du pack « grottes & rives » (creation3DBlenderOrganicSuite.md) :
# rives, pièces hero et rochers — 6 assets, style organique.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_shore_rocks.py
#
# Sortie : assets/models/shore_*.glb

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from organic_common import (  # noqa: E402
    STONE,
    STONE_DARK,
    WATER_DARK,
    boulder_elements,
    export,
    mat,
    organic_core,
    reset_scene,
    rng,
)


def gen_smooth_shore_rock():
    """Rocher de rive lissé ~0.8 m : galet géant arrondi par l'eau, pièce
    hero du lot — recette validée manuellement (test_shore_rock)."""
    stone = mat("pierre_rive", STONE)
    elements = boulder_elements(base_radius=0.32, n_bumps=4, squash=0.62)
    organic_core("RocherLisse", elements, stone, resolution=0.02, ground_guard=0.02)
    export("shore_smooth_rock.glb")


def gen_pebble_group():
    """Groupe de galets ~0.6 m d'emprise : 4 galets de tailles variées,
    bien arrondis."""
    stone = mat("pierre_rive", STONE)
    stone_dark = mat("pierre_rive_sombre", STONE_DARK)
    positions = [(-0.18, -0.10, 0.14), (0.14, 0.06, 0.11), (-0.02, 0.16, 0.09),
                 (0.16, -0.14, 0.08)]
    for i, (x, y, r) in enumerate(positions):
        elements = boulder_elements(base_radius=r, n_bumps=2, squash=0.7)
        elements = [((ex + x, ey + y, ez), er, es, est) for (ex, ey, ez), er, es, est in elements]
        m = stone if i % 2 == 0 else stone_dark
        organic_core(f"Galet{i}", elements, m, resolution=0.012, ground_guard=0.02)
    export("shore_pebble_group.glb")


def gen_rock_island():
    """Îlot rocheux ~1.3 m : petit rocher émergent, posable au milieu de
    l'eau — plus massif que le rocher de rive isolé."""
    stone = mat("pierre_rive", STONE)
    elements = boulder_elements(base_radius=0.5, n_bumps=5, squash=0.55)
    organic_core("Ilot", elements, stone, resolution=0.03, ground_guard=0.02)
    export("shore_rock_island.glb")


def gen_gentle_bank():
    """Berge en pente douce ~2.4 m : talus végétalisé bas et large, suit un
    relief naturel (Sprint 26 de sprintreflecion.md)."""
    stone = mat("pierre_rive", STONE)
    elements = []
    n_cols = 13
    for i in range(n_cols):
        x = -1.2 + i * 2.4 / (n_cols - 1)
        h = 0.15 + (i / (n_cols - 1)) * 0.55 + rng.uniform(-0.03, 0.03)
        r = 0.30
        elements.append(((x, rng.uniform(-0.05, 0.05), h * 0.5), r, (1.0, 1.0, h / r), 1.3))
    organic_core("BergeDouce", elements, stone, resolution=0.06, ground_guard=0.02)
    export("shore_gentle_bank.glb")


def gen_steep_bank():
    """Berge abrupte rocheuse ~1.6 m de haut : variante escarpée de la
    berge douce, silhouette verticale."""
    stone = mat("pierre_rive", STONE)
    elements = []
    n_cols = 9
    for i in range(n_cols):
        x = -1.0 + i * 2.0 / (n_cols - 1)
        h = rng.uniform(1.2, 1.7)
        r = 0.32
        elements.append(((x, rng.uniform(-0.06, 0.06), h * 0.5), r, (1.0, 1.0, h / r), 1.3))
    organic_core("BergeAbrupte", elements, stone, resolution=0.075, ground_guard=0.02)
    export("shore_steep_bank.glb")


def gen_natural_basin():
    """Vasque naturelle ~0.9 m : bassin creusé par l'érosion — anneau de
    pierre + eau sombre insérée au centre (pas de vraie dépression
    métaball : plus simple et plus sûr qu'une soustraction, cf. charte)."""
    stone = mat("pierre_rive", STONE)
    water = mat("eau_vasque", WATER_DARK, roughness=0.1)
    # Anneau dense (même piège que gen_entrance_arch/gen_back_wall : un
    # premier essai à 9 blocs espacés à la main rendait en collier de perles
    # séparées, pas un rebord fusionné) : rayon de l'anneau 0.38, blocs de
    # rayon ~0.17 → circonférence/espacement calculé pour un ratio ~0.4.
    ring_radius = 0.38
    block_r = 0.17
    circumference = math.tau * ring_radius
    n = max(int(circumference / (block_r * 0.4)) + 1, 8)
    elements = []
    for i in range(n):
        a = i * math.tau / n
        x, y = ring_radius * math.cos(a), ring_radius * math.sin(a)
        r = block_r + rng.uniform(-0.015, 0.015)
        elements.append(((x, y, 0.10), r, (1.0, 1.0, 0.9), 1.3))
    organic_core("Rebord", elements, stone, resolution=0.032, ground_guard=0.02)
    water_elems = [((0, 0, 0.05), 0.30, (1.0, 1.0, 0.06), 1.3)]
    organic_core("Eau", water_elems, water, resolution=0.02, ground_guard=0.0)
    export("shore_natural_basin.glb")


ASSETS = [
    gen_smooth_shore_rock,
    gen_pebble_group,
    gen_rock_island,
    gen_gentle_bank,
    gen_steep_bank,
    gen_natural_basin,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[shore_rocks] pack complet : {len(ASSETS)} fichiers")
