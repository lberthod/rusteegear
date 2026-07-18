# Sprint 5 du pack « grottes & rives » (creation3DBlenderOrganicSuite.md) :
# rives, bois et détails organiques — 7 assets, style organique.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_shore_decor.py
#
# Sortie : assets/models/shore_*.glb

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from organic_common import (  # noqa: E402
    DRIFTWOOD,
    STONE,
    WATER_LIGHT,
    boulder_elements,
    chain_elements,
    cube,
    export,
    mat,
    organic_core,
    reset_scene,
    rng,
)

SILVER = (0.68, 0.70, 0.72)  # poisson échoué, ventre pâle


def gen_driftwood():
    """Bois flotté ~1.4 m : tronc échoué tordu, grisé par l'eau."""
    wood = mat("bois_flotte", DRIFTWOOD)
    waypoints = [(-0.7, 0.05, 0.10), (-0.25, -0.08, 0.14), (0.20, 0.10, 0.11),
                 (0.65, -0.05, 0.09)]
    elements = chain_elements(waypoints, radius_start=0.13, radius_end=0.06, density=0.4)
    organic_core("Tronc", elements, wood, resolution=0.02, ground_guard=0.02)
    export("shore_driftwood.glb")


def gen_submerged_root():
    """Racine immergée ~1.0 m : racine de rive tordue, mi-émergée."""
    wood = mat("racine_rive", (0.28, 0.24, 0.18))
    waypoints = [(0, 0, 0.20), (0.15, 0.10, 0.12), (0.35, 0.02, 0.16),
                 (0.55, -0.08, 0.05)]
    elements = chain_elements(waypoints, radius_start=0.07, radius_end=0.02, density=0.4)
    organic_core("RacineImmergee", elements, wood, resolution=0.015, ground_guard=0.0)
    export("shore_submerged_root.glb")


def gen_submerged_stump():
    """Souche à moitié immergée ~0.6 m : base large, sommet cassé irrégulier
    (érosion, pas coupe nette) — distincte de nature_stump."""
    wood = mat("souche_rive", (0.32, 0.27, 0.20))
    wood_dark = mat("souche_rive_sombre", (0.22, 0.18, 0.13))
    elements = boulder_elements(base_radius=0.24, n_bumps=3, squash=0.75)
    organic_core("Souche", elements, wood, resolution=0.02, ground_guard=0.02)
    for i in range(4):
        a = i * math.tau / 4 + rng.uniform(-0.2, 0.2)
        # Rayon plus grand (0.20-0.26, hors du socle de 0.24) : à 0.10 les
        # racines restaient presque entièrement cachées sous la souche,
        # invisibles à la vignette (constaté au premier rendu).
        dist = rng.uniform(0.20, 0.26)
        x, y = dist * math.cos(a), dist * math.sin(a)
        length = rng.uniform(0.22, 0.32)
        thick = rng.uniform(0.045, 0.06)
        b = cube(f"Racine{i}", wood_dark, (x * 0.6, y * 0.6, thick * 0.5 + 0.03),
                  (length, thick, thick))
        b.rotation_euler = (rng.uniform(-0.15, 0.15), rng.uniform(-0.15, 0.15), a)
    export("shore_submerged_stump.glb")


def gen_rooted_bank():
    """Berge à racines apparentes ~1.6 m : érosion qui expose des racines
    entremêlées sur un talus."""
    stone = mat("pierre_rive", STONE)
    wood = mat("racine_rive", (0.28, 0.24, 0.18))
    elements = []
    n_cols = 9
    for i in range(n_cols):
        x = -0.8 + i * 1.6 / (n_cols - 1)
        h = rng.uniform(0.5, 0.9)
        r = 0.26
        elements.append(((x, rng.uniform(-0.05, 0.05), h * 0.5), r, (1.0, 1.0, h / r), 1.3))
    organic_core("Talus", elements, stone, resolution=0.06, ground_guard=0.02)
    for i in range(4):
        x = -0.6 + i * 0.4 + rng.uniform(-0.08, 0.08)
        waypoints = [(x, -0.20, 0.30), (x + rng.uniform(-0.1, 0.1), 0.15, 0.04)]
        elements = chain_elements(waypoints, radius_start=0.035, radius_end=0.015, density=0.4)
        organic_core(f"Racine{i}", elements, wood, resolution=0.015, ground_guard=0.0)
    export("shore_rooted_bank.glb")


def gen_frozen_waterfall():
    """Cascade figée stylisée ~1.8 m : paroi rocheuse + filet d'eau vive
    (opaque, cf. charte) descendant la face."""
    stone = mat("pierre_rive", STONE)
    water = mat("eau_vive", WATER_LIGHT, roughness=0.1)
    elements = []
    n_cols = 7
    for i in range(n_cols):
        x = -0.7 + i * 1.4 / (n_cols - 1)
        h = rng.uniform(1.4, 1.8)
        r = 0.30
        elements.append(((x, rng.uniform(-0.05, 0.05), h * 0.5), r, (1.0, 1.0, h / r), 1.3))
    organic_core("Paroi", elements, stone, resolution=0.08, ground_guard=0.02)
    waypoints = [(0.0, 0.28, 1.75), (0.03, 0.30, 0.9), (-0.02, 0.29, 0.10)]
    water_elems = chain_elements(waypoints, radius_start=0.14, radius_end=0.10, density=0.4)
    organic_core("FiletEau", water_elems, water, resolution=0.02, ground_guard=0.0)
    export("shore_frozen_waterfall.glb")


def gen_drift_line():
    """Laisse de rive ~1.2 m : ligne de débris/algues déposée par l'eau,
    aplatie et allongée."""
    wood = mat("debris_rive", DRIFTWOOD)
    waypoints = [(-0.6, 0.02, 0.03), (-0.2, -0.03, 0.035), (0.2, 0.02, 0.03),
                 (0.6, -0.02, 0.025)]
    elements = chain_elements(waypoints, radius_start=0.06, radius_end=0.04, density=0.4)
    organic_core("Laisse", elements, wood, resolution=0.02, ground_guard=0.0)
    export("shore_drift_line.glb")


def gen_beached_fish():
    """Poisson échoué ~0.3 m : silhouette fusiforme, détail narratif. Chaîne
    dense (même piège de densité que gen_entrance_arch/shore_natural_basin :
    4 éléments espacés à la main rendaient en chapelet de perles)."""
    fish = mat("poisson", SILVER, roughness=0.3)
    waypoints = [(0, -0.13, 0.045), (0, -0.02, 0.055), (0, 0.10, 0.05), (0, 0.20, 0.03)]
    elements = chain_elements(waypoints, radius_start=0.07, radius_end=0.02, density=0.35)
    organic_core("Poisson", elements, fish, resolution=0.008, ground_guard=0.0)
    export("shore_beached_fish.glb")


ASSETS = [
    gen_driftwood,
    gen_submerged_root,
    gen_submerged_stump,
    gen_rooted_bank,
    gen_frozen_waterfall,
    gen_drift_line,
    gen_beached_fish,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[shore_decor] pack complet : {len(ASSETS)} fichiers")
