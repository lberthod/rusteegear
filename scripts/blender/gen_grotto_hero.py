# Sprint 1 du pack « grottes & rives » (creation3DBlenderOrganicSuite.md) :
# grottes, pièces hero — 4 assets, style organique (métaballes fusionnées,
# cf. organic_common.py). Ces 4 pièces posent le vocabulaire visuel du lot :
# masses rocheuses lisses + accessoires durs en contraste (gravats, arêtes
# cassées) là où le script le précise.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_grotto_hero.py
#
# Sortie : assets/models/grotto_*.glb

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from organic_common import (  # noqa: E402
    STONE,
    STONE_DARK,
    boulder_elements,
    chain_elements,
    cube,
    export,
    mat,
    organic_core,
    reset_scene,
    rng,
    spire_elements,
)


def gen_entrance_arch():
    """Arche d'entrée ~2.6 m de large, 2.1 m de haut : deux piliers (peu
    effilés, contrairement à une stalagmite) reliés par un arc de blocs
    fusionnés — pièce hero du lot, doit se reconnaître de loin."""
    stone = mat("pierre_grotte", STONE)
    stone_dark = mat("pierre_grotte_sombre", STONE_DARK)

    elements = []
    nodule_anchors = []  # (x, y, z, rayon local) — points où accrocher des nodules
    for side in (-1, 1):
        pillar = spire_elements(base_radius=0.34, height=1.85, taper=0.85)
        for i, ((dx, dy, z), r, size, stiff) in enumerate(pillar):
            # Rayon légèrement irrégulier le long du pilier : sans ça, le
            # rendu lit comme un tube en plastique uniforme, pas une masse
            # rocheuse (constaté au premier rendu de cette pièce).
            r_var = r * rng.uniform(0.92, 1.12)
            x = side * 1.05 + dx
            elements.append(((x, dy, z), r_var, size, stiff))
            if i in (2, 4):
                nodule_anchors.append((x, dy, z, r_var))
    # Arc au sommet : chaîne dense de blocs suivant une courbe entre les deux
    # piliers, légèrement aplatie (pas un demi-cercle parfait — arche
    # rocheuse, pas une porte géométrique). `chain_elements` calcule le
    # nombre de blocs nécessaire pour fusionner (piège résolu : une première
    # version à 5 blocs fixes rendait en boules détachées, cf. docstring de
    # spire_elements).
    waypoints = []
    for i in range(9):
        t = i / 8
        angle = math.pi * (1 - t)
        x = 1.05 * math.cos(angle)
        z = 1.85 + 1.05 * math.sin(angle) * 0.55
        waypoints.append((x, 0.0, z))
    arch_elems = chain_elements(waypoints, radius_start=0.30, radius_end=0.30, density=0.4)
    for i, ((x, y, z), r, size, stiff) in enumerate(arch_elems):
        r_var = r * rng.uniform(0.90, 1.15)
        elements.append(((x, y, z), r_var, size, stiff))
        if i in (2, 4, 6):
            nodule_anchors.append((x, y, z, r_var))
    # Nodules rocheux en saillie sur la face visible, à quelques points le
    # long du pilier et de l'arc — casse le profil « tube lisse » constaté au
    # premier rendu.
    for bx, by, bz, base_r in nodule_anchors:
        nx = bx + rng.uniform(-0.15, 0.15)
        ny = by + rng.uniform(0.10, 0.22)
        nz = bz + rng.uniform(-0.15, 0.15)
        elements.append(((nx, ny, nz), base_r * rng.uniform(0.35, 0.55), (1.0, 1.0, 0.9), 1.4))
    organic_core("Arche", elements, stone, resolution=0.06, ground_guard=0.02)

    # Gravats durs au pied des piliers (contraste facetté).
    for side in (-1, 1):
        for i in range(2):
            x = side * (1.05 + rng.uniform(-0.25, 0.35))
            y = rng.uniform(-0.3, 0.3)
            s = rng.uniform(0.10, 0.18)
            # z = s/2 + marge : un cube tourné sur X/Y peut faire dépasser un
            # coin sous z=0 même si son centre est à la demi-hauteur pile
            # (piège rencontré ici, min z=-0.033 avant correction).
            b = cube(f"Gravat{side}_{i}", stone_dark, (x, y, s / 2 + 0.05), (s, s, s))
            b.rotation_euler = (rng.uniform(-0.3, 0.3), rng.uniform(-0.3, 0.3), rng.uniform(0, 6.28))
    export("grotto_entrance_arch.glb")


def gen_back_wall():
    """Paroi de fond ~3.8 m de large, 2 m de haut : rangée dense de petites
    colonnes rocheuses de hauteur irrégulière, assez rapprochées pour
    fusionner latéralement (piège résolu : une première version à 7 bosses
    espacées à la main rendait en chapelet séparé, cf. docstring de
    spire_elements — même cause que gen_entrance_arch)."""
    stone = mat("pierre_grotte", STONE)
    elements = []
    n_cols = 21
    for i in range(n_cols):
        x = -1.8 + i * 3.6 / (n_cols - 1)
        h = rng.uniform(1.3, 2.0)
        r = 0.45 + rng.uniform(-0.05, 0.05)
        col = spire_elements(base_radius=r, height=h, taper=0.9)
        for (dx, dy, z), rr, size, stiff in col:
            elements.append(((x + dx, dy, z), rr, size, stiff))
    organic_core("ParoiFond", elements, stone, resolution=0.11, ground_guard=0.02)
    export("grotto_back_wall.glb")


def gen_collapsed_block():
    """Bloc effondré ~1.3 m : masse organique massive (plafond écroulé) +
    arêtes dures cassées en périphérie — contraste lisse/facetté volontaire,
    contrairement aux autres pièces du lot entièrement organiques."""
    stone = mat("pierre_grotte", STONE)
    stone_dark = mat("pierre_grotte_sombre", STONE_DARK)
    elements = boulder_elements(base_radius=0.55, n_bumps=5, squash=0.62)
    organic_core("Bloc", elements, stone, resolution=0.035, ground_guard=0.02)
    for i in range(4):
        a = i * math.tau / 4 + rng.uniform(-0.2, 0.2)
        d = rng.uniform(0.35, 0.55)
        x, y = d * math.cos(a), d * math.sin(a)
        s = rng.uniform(0.14, 0.22)
        # z relevé + marge (0.05) : à demi-hauteur pile, l'inclinaison sur
        # X/Y (jusqu'à 0.4 rad) fait dépasser un coin sous z=0 (piège
        # rencontré ici, min z=-0.018 avant correction).
        b = cube(f"Arete{i}", stone_dark, (x, y, s * 0.4 + 0.05), (s, s * 0.8, s * 0.5))
        b.rotation_euler = (rng.uniform(-0.4, 0.4), rng.uniform(-0.4, 0.4), a)
    export("grotto_collapsed_block.glb")


def gen_column():
    """Colonne ~1.9 m : stalactite et stalagmite fusionnées en une seule
    colonne continue — large aux deux bouts (plafond/sol), fine au milieu."""
    stone = mat("pierre_grotte", STONE)
    total_h = 1.9
    mid = total_h * 0.52
    elements = []
    bottom = spire_elements(base_radius=0.24, height=mid, taper=0.42)
    elements.extend(bottom)
    top = spire_elements(base_radius=0.22, height=total_h - mid, taper=0.45)
    for (dx, dy, z), r, size, stiff in top:
        # Miroir en z (fine en bas, large en haut) + décalage pour repartir
        # du point de jonction au milieu de la colonne.
        z_mirror = (total_h - mid) - z
        elements.append(((dx, dy, mid + z_mirror), r, size, stiff))
    organic_core("Colonne", elements, stone, resolution=0.03, ground_guard=0.02)
    export("grotto_column.glb")


ASSETS = [
    gen_entrance_arch,
    gen_back_wall,
    gen_collapsed_block,
    gen_column,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[grotto_hero] pack complet : {len(ASSETS)} fichiers")
