# Sprint 6 du pack « siège du hameau » (creation3DBlendersuite.md) :
# signalétique et effets 3D lot 2 — 5 assets, complexité moyenne. Dernier lot
# de contenu avant la QA finale du Sprint 7.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_signal2.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    GLOW_YELLOW,
    METAL,
    METAL_DARK,
    STONE,
    STONE_DARK,
    WOOD,
    WOOD_DARK,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)


def gen_chief_cage():
    """Cage du chef : cage de barreaux métalliques sur socle de bois, silhouette
    sombre à l'intérieur (captif suggéré, pas de figure détaillée) — pièce de
    mise en scène, pas un ennemi jouable."""
    wood_dark = mat("bois_sombre", WOOD_DARK)
    metal_dark = mat("metal_sombre", METAL_DARK)
    captive = mat("silhouette", (0.08, 0.07, 0.07))
    cube("Socle", wood_dark, (0, 0, 0.06), (0.9, 0.9, 0.12))
    cube("Captif", captive, (0, 0, 0.55), (0.22, 0.16, 0.8))
    for sx in (-1, 1):
        for sy in (-1, 1):
            cylinder(f"Poteau{sx}{sy}", metal_dark, (sx * 0.4, sy * 0.4, 0.72),
                      radius=0.03, depth=1.2, vertices=6)
    # Barreaux verticaux sur les 4 faces (bandes de cube, pas des cylindres
    # séparés) — silhouette de cage lisible sans multiplier les petits objets.
    n_bars = 5
    for i in range(n_bars):
        t = -0.32 + i * 0.64 / (n_bars - 1)
        for s in (-1, 1):
            cube(f"BarreauX{i}{s}", metal_dark, (t, s * 0.4, 0.72), (0.025, 0.025, 1.2))
            cube(f"BarreauY{i}{s}", metal_dark, (s * 0.4, t, 0.72), (0.025, 0.025, 1.2))
    cube("Toit", metal_dark, (0, 0, 1.34), (0.9, 0.9, 0.06))
    export("siege_chief_cage.glb")


def gen_memorial_statue():
    """Statue commémorative : figure humanoïde stylisée (blocs géométriques,
    pas de sculpture organique) sur socle de pierre gravé — hommage abstrait,
    pas un portrait détaillé."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    cube("Socle", stone_dark, (0, 0, 0.15), (0.8, 0.8, 0.3))
    cube("SocleHaut", stone, (0, 0, 0.34), (0.6, 0.6, 0.08))
    cube("Jambes", stone, (0, 0, 0.7), (0.34, 0.28, 0.6))
    cube("Torse", stone, (0, 0, 1.25), (0.42, 0.3, 0.5))
    for sx in (-1, 1):
        cube(f"Bras{sx}", stone, (sx * 0.28, 0, 1.2), (0.14, 0.16, 0.5))
    cube("Tete", stone, (0, 0, 1.68), (0.24, 0.24, 0.24))
    cube("Arme", stone_dark, (0, 0.2, 0.9), (0.06, 0.06, 1.5))
    export("siege_memorial_statue.glb")


def gen_round_trophy():
    """Trophée de fin de manche : coupe classique (pied + tige + vasque),
    métal doré-terne pour rester dans la charte (pas de couleur saturée)."""
    metal = mat("metal", METAL)
    metal_dark = mat("metal_sombre", METAL_DARK)
    cylinder("Pied", metal_dark, (0, 0, 0.04), radius=0.14, depth=0.08, vertices=12)
    cylinder("Tige", metal, (0, 0, 0.28), radius=0.035, depth=0.4, vertices=10)
    cylinder("Col", metal_dark, (0, 0, 0.5), radius=0.06, depth=0.06, vertices=10)
    cylinder("Vasque", metal, (0, 0, 0.66), radius=0.18, depth=0.28, vertices=12)
    cylinder("Rebord", metal_dark, (0, 0, 0.81), radius=0.19, depth=0.04, vertices=12)
    for sx in (-1, 1):
        cube(f"Anse{sx}", metal, (sx * 0.2, 0, 0.66), (0.05, 0.05, 0.2))
    export("siege_round_trophy.glb")


def gen_end_portal():
    """Portail de fin stylisé : arche de pierre à double montant + linteau,
    centre émissif (GLOW_YELLOW, vignette uniquement) marquant la fin d'une
    manche/mode."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    glow = mat("seuil", GLOW_YELLOW, emission=1.6)
    opening_w, opening_h = 1.8, 2.6
    jamb_w = 0.35
    for sx in (-1, 1):
        cube(f"Montant{sx}", stone, (sx * (opening_w / 2 + jamb_w / 2), 0, opening_h / 2),
             (jamb_w, 0.4, opening_h))
    cube("Linteau", stone_dark, (0, 0, opening_h + 0.2), (opening_w + jamb_w * 2, 0.42, 0.4))
    cube("Fronton", stone_dark, (0, 0, opening_h + 0.55), (0.5, 0.4, 0.3))
    cube("Seuil", glow, (0, 0.02, 0.03), (opening_w * 0.85, 0.06, 0.06))
    export("siege_end_portal.glb")


def gen_rampart_signpost():
    """Panneau directionnel de rempart : mât + trois planches d'orientation
    à hauteurs et angles différents, silhouette simple mais lisible de loin."""
    wood_dark = mat("bois_sombre", WOOD_DARK)
    wood = mat("bois", WOOD)
    cylinder("Mat", wood_dark, (0, 0, 0.9), radius=0.05, depth=1.8, vertices=8)
    signs = [(1.35, 25, 0.5), (1.15, -35, 0.42), (0.95, 60, 0.46)]
    for i, (z, yaw, length) in enumerate(signs):
        s = cube(f"Panneau{i}", wood, (0, 0, z), (length, 0.03, 0.22))
        s.rotation_euler = (0, 0, math.radians(yaw))
    export("siege_rampart_signpost.glb")


ASSETS = [
    gen_chief_cage,
    gen_memorial_statue,
    gen_round_trophy,
    gen_end_portal,
    gen_rampart_signpost,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_signal2] pack complet : {len(ASSETS)} fichiers")
