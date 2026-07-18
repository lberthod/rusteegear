# Sprint 2 du pack « grottes & rives » (creation3DBlenderOrganicSuite.md) :
# grottes, formations rocheuses — 8 assets, style organique.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_grotto_rocks.py
#
# Sortie : assets/models/grotto_*.glb

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from organic_common import (  # noqa: E402
    METAL,
    STONE,
    STONE_DARK,
    boulder_elements,
    chain_elements,
    cone,
    export,
    mat,
    organic_core,
    reset_scene,
    rng,
    spire_elements,
)


def _hanging_spire(name, base_radius, height, ceiling_z, taper, material, resolution=0.02):
    """Stalactite : reprend spire_elements (large à z=0, fin à z=height) puis
    la retourne (large en haut, fine pointe vers le bas) — cf. docstring de
    spire_elements."""
    raw = spire_elements(base_radius=base_radius, height=height, taper=taper)
    elements = [((dx, dy, ceiling_z - z), r, size, stiff) for (dx, dy, z), r, size, stiff in raw]
    organic_core(name, elements, material, resolution=resolution, ground_guard=0.02)


def gen_stalactite_small():
    """Stalactite ~0.6 m, suspendue (ne touche pas le sol)."""
    stone = mat("pierre_grotte", STONE)
    _hanging_spire("StalactitePetite", base_radius=0.11, height=0.55, ceiling_z=1.05,
                   taper=0.20, material=stone, resolution=0.015)
    export("grotto_stalactite_small.glb")


def gen_stalactite_large():
    """Stalactite ~1.1 m, landmark vertical suspendu."""
    stone = mat("pierre_grotte", STONE)
    _hanging_spire("StalactiteGrande", base_radius=0.19, height=1.1, ceiling_z=1.9,
                   taper=0.16, material=stone, resolution=0.02)
    export("grotto_stalactite_large.glb")


def gen_stalagmite_small():
    """Stalagmite ~0.5 m, au sol."""
    stone = mat("pierre_grotte", STONE)
    elements = spire_elements(base_radius=0.15, height=0.5, taper=0.22)
    organic_core("StalagmitePetite", elements, stone, resolution=0.015, ground_guard=0.02)
    export("grotto_stalagmite_small.glb")


def gen_stalagmite_large():
    """Stalagmite ~1.0 m, landmark de couloir."""
    stone = mat("pierre_grotte", STONE)
    elements = spire_elements(base_radius=0.26, height=1.0, taper=0.20)
    organic_core("StalagmiteGrande", elements, stone, resolution=0.02, ground_guard=0.02)
    export("grotto_stalagmite_large.glb")


def gen_bumpy_floor():
    """Sol rocheux bosselé ~2×2 m : dalle pleine et basse (pas un aplat
    parfait) + quelques bosses éparses par-dessus. Une première version en
    grille dense de petites bosses (sans base pleine) rendait en « bulles de
    plastique » séparées, jamais un sol continu — corrigé en revenant à une
    seule masse large et plate, sur laquelle quelques bosses viennent juste
    ajouter de l'irrégularité de surface (même logique que les nodules de
    gen_entrance_arch)."""
    stone = mat("pierre_grotte", STONE)
    elements = [((0, 0, 0.09), 1.5, (1.0, 1.0, 0.12), 1.3)]
    for _ in range(14):
        x = rng.uniform(-0.85, 0.85)
        y = rng.uniform(-0.85, 0.85)
        h = rng.uniform(0.08, 0.20)
        r = rng.uniform(0.20, 0.32)
        elements.append(((x, y, h * 0.4), r, (1.0, 1.0, h / r), 1.3))
    organic_core("SolBossele", elements, stone, resolution=0.05, ground_guard=0.0)
    export("grotto_bumpy_floor.glb")


def gen_rubble():
    """Éboulis ~1 m d'emprise : cluster de 5 petits rochers organiques,
    tailles variées — version organique du tas de pierres."""
    stone = mat("pierre_grotte", STONE)
    stone_dark = mat("pierre_grotte_sombre", STONE_DARK)
    positions = [(-0.30, -0.15, 0.35), (0.20, 0.10, 0.30), (-0.05, 0.28, 0.22),
                 (0.30, -0.22, 0.20), (0.0, 0.0, 0.15)]
    for i, (x, y, r) in enumerate(positions):
        elements = boulder_elements(base_radius=r, n_bumps=3, squash=0.65)
        elements = [((ex + x, ey + y, ez), er, esize, est) for (ex, ey, ez), er, esize, est in elements]
        m = stone if i % 2 == 0 else stone_dark
        organic_core(f"Rocher{i}", elements, m, resolution=0.035, ground_guard=0.02)
    export("grotto_rubble.glb")


def gen_low_passage():
    """Passage bas ~1.6 m de large, 1.1 m de haut à la clé : arche basse et
    trapue à franchir courbé — variante réduite de grotto_entrance_arch."""
    stone = mat("pierre_grotte", STONE)
    elements = []
    for side in (-1, 1):
        pillar = spire_elements(base_radius=0.26, height=0.75, taper=0.9)
        for (dx, dy, z), r, size, stiff in pillar:
            elements.append(((side * 0.65 + dx, dy, z), r * rng.uniform(0.9, 1.1), size, stiff))
    waypoints = []
    for i in range(7):
        t = i / 6
        angle = math.pi * (1 - t)
        x = 0.65 * math.cos(angle)
        z = 0.75 + 0.65 * math.sin(angle) * 0.5
        waypoints.append((x, 0.0, z))
    elements.extend(chain_elements(waypoints, radius_start=0.24, radius_end=0.24, density=0.4))
    organic_core("PassageBas", elements, stone, resolution=0.045, ground_guard=0.02)
    export("grotto_low_passage.glb")


def gen_crystal():
    """Cristal de grotte ~0.6 m : cœur organique + facettes dures plantées
    (accessoires nets, non lissés — contraste voulu avec la roche)."""
    stone = mat("pierre_grotte", STONE)
    crystal_m = mat("cristal", METAL, roughness=0.15)
    elements = boulder_elements(base_radius=0.22, n_bumps=3, squash=0.7)
    organic_core("SocleCristal", elements, stone, resolution=0.02, ground_guard=0.02)
    for i in range(5):
        a = i * math.tau / 5 + rng.uniform(-0.2, 0.2)
        d = rng.uniform(0.05, 0.14)
        x, y = d * math.cos(a), d * math.sin(a)
        h = rng.uniform(0.25, 0.5)
        tilt = rng.uniform(-0.25, 0.25)
        c = cone(f"Facette{i}", crystal_m, (x, y, 0.10 + h / 2), radius=h * 0.16, depth=h, vertices=6)
        c.rotation_euler = (tilt, tilt, a)
    export("grotto_crystal.glb")


ASSETS = [
    gen_stalactite_small,
    gen_stalactite_large,
    gen_stalagmite_small,
    gen_stalagmite_large,
    gen_bumpy_floor,
    gen_rubble,
    gen_low_passage,
    gen_crystal,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[grotto_rocks] pack complet : {len(ASSETS)} fichiers")
