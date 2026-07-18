# Sprint 3 du pack « grottes & rives » (creation3DBlenderOrganicSuite.md) :
# grottes, flore/détails — 8 assets, style organique.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_grotto_decor.py
#
# Sortie : assets/models/grotto_*.glb

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from organic_common import (  # noqa: E402
    GLOW_CAVE,
    MOSS,
    WATER_DARK,
    WOOD_DARK,
    boulder_elements,
    chain_elements,
    cube,
    cylinder,
    export,
    mat,
    organic_core,
    reset_scene,
    rng,
    spire_elements,
)

CREAM = (0.90, 0.86, 0.78)  # os fossilisés, pâle


def gen_glow_mushroom_small():
    """Champignon lumineux ~0.20 m : chapeau bombé + pied fin — flore
    souterraine bioluminescente."""
    glow = mat("champignon_lueur", GLOW_CAVE, emission=1.0)
    stem = mat("pied_champignon", (0.55, 0.50, 0.42))
    cap = boulder_elements(base_radius=0.09, n_bumps=3, squash=0.55, base_z=0.16)
    organic_core("Chapeau", cap, glow, resolution=0.012, ground_guard=0.02)
    cylinder("Pied", stem, (0, 0, 0.06), radius=0.025, depth=0.12, vertices=7)
    export("grotto_glow_mushroom_small.glb")


def gen_glow_mushroom_cluster():
    """Grappe de 4 champignons lumineux de tailles variées ~0.35 m d'emprise."""
    glow = mat("champignon_lueur", GLOW_CAVE, emission=1.0)
    stem = mat("pied_champignon", (0.55, 0.50, 0.42))
    spots = [(-0.12, -0.06, 0.16), (0.10, 0.05, 0.20), (-0.03, 0.13, 0.12), (0.08, -0.10, 0.10)]
    for i, (x, y, h) in enumerate(spots):
        r = h * 0.55
        cap = boulder_elements(base_radius=r, n_bumps=2, squash=0.55, base_z=h)
        cap = [((ex + x, ey + y, ez), er, es, est) for (ex, ey, ez), er, es, est in cap]
        organic_core(f"Chapeau{i}", cap, glow, resolution=0.012, ground_guard=0.02)
        cylinder(f"Pied{i}", stem, (x, y, h * 0.4), radius=r * 0.3, depth=h * 0.8, vertices=6)
    export("grotto_glow_mushroom_cluster.glb")


def gen_hanging_root():
    """Racine pendante ~1.2 m : tendron sinueux qui perce le plafond,
    fine et tordue — dérive latérale marquée (racine, pas stalactite)."""
    stone_root = mat("racine", (0.30, 0.22, 0.14))
    waypoints = []
    x, y = 0.0, 0.0
    for i in range(6):
        t = i / 5
        x += rng.uniform(-0.10, 0.10)
        y += rng.uniform(-0.10, 0.10)
        z = 1.15 - t * 1.15
        waypoints.append((x, y, z))
    elements = chain_elements(waypoints, radius_start=0.055, radius_end=0.015, density=0.4)
    organic_core("RacinePendante", elements, stone_root, resolution=0.012, ground_guard=0.0)
    export("grotto_hanging_root.glb")


def gen_underground_puddle():
    """Flaque souterraine ~0.9 m : eau stagnante, sol plat et sombre."""
    water = mat("eau_stagnante", WATER_DARK, roughness=0.15)
    elements = [((0, 0, 0.015), 0.45, (1.0, 1.0, 0.035), 1.3)]
    for _ in range(4):
        x, y = rng.uniform(-0.2, 0.2), rng.uniform(-0.2, 0.2)
        elements.append(((x, y, 0.012), 0.18, (1.0, 1.0, 0.03), 1.3))
    organic_core("Flaque", elements, water, resolution=0.02, ground_guard=0.0)
    export("grotto_underground_puddle.glb")


def gen_support_beam():
    """Poutre de soutènement ~1.8 m : structure humaine dans le tunnel —
    primitives dures classiques, contraste voulu avec le reste organique."""
    wood = mat("bois_etai", WOOD_DARK)
    for sx in (-1, 1):
        cube(f"Montant{sx}", wood, (sx * 0.75, 0, 0.55), (0.12, 0.12, 1.1))
    cube("Traverse", wood, (0, 0, 1.1), (1.7, 0.14, 0.12))
    for sx in (-1, 1):
        d = cube(f"Diagonale{sx}", wood, (sx * 0.5, 0, 0.85), (0.08, 0.08, 0.65))
        d.rotation_euler = (0, sx * math.radians(28), 0)
    export("grotto_support_beam.glb")


def gen_bones():
    """Ossements de créature ~0.7 m : restes fossilisés au sol (côtes +
    crâne stylisé), teinte pâle."""
    bone = mat("os", CREAM)
    skull = boulder_elements(base_radius=0.13, n_bumps=2, squash=0.6, base_z=0.10)
    organic_core("Crane", skull, bone, resolution=0.02, ground_guard=0.02)
    for i in range(5):
        x = -0.25 + i * 0.11
        waypoints = [(x, -0.18, 0.05), (x + rng.uniform(-0.03, 0.03), 0.20, 0.06)]
        elements = chain_elements(waypoints, radius_start=0.025, radius_end=0.018, density=0.4)
        organic_core(f"Cote{i}", elements, bone, resolution=0.018, ground_guard=0.0)
    export("grotto_bones.glb")


def gen_mold_veil():
    """Voile de moisissure ~0.5 m : patch sombre en surplomb, aplati et
    irrégulier."""
    moss = mat("moisissure", MOSS, roughness=0.9)
    elements = [((0, 0, 0.02), 0.28, (1.2, 0.9, 0.06), 1.3)]
    for _ in range(5):
        x, y = rng.uniform(-0.18, 0.18), rng.uniform(-0.14, 0.14)
        elements.append(((x, y, 0.018), 0.10, (1.0, 1.0, 0.05), 1.3))
    organic_core("Moisissure", elements, moss, resolution=0.015, ground_guard=0.0)
    export("grotto_mold_veil.glb")


def gen_hanging_drop():
    """Goutte suspendue ~0.18 m : détail miniature, mini-stalactite + reflet
    d'eau à la pointe."""
    stone = mat("pierre_grotte_detail", (0.42, 0.41, 0.40))
    water = mat("goutte_eau", WATER_DARK, roughness=0.05)
    raw = spire_elements(base_radius=0.035, height=0.14, taper=0.25)
    elements = [((dx, dy, 0.16 - z), r, size, stiff) for (dx, dy, z), r, size, stiff in raw]
    organic_core("MiniStalactite", elements, stone, resolution=0.006, ground_guard=0.0)
    cube("Goutte", water, (0, 0, 0.016), (0.02, 0.02, 0.02))
    export("grotto_hanging_drop.glb")


ASSETS = [
    gen_glow_mushroom_small,
    gen_glow_mushroom_cluster,
    gen_hanging_root,
    gen_underground_puddle,
    gen_support_beam,
    gen_bones,
    gen_mold_veil,
    gen_hanging_drop,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[grotto_decor] pack complet : {len(ASSETS)} fichiers")
