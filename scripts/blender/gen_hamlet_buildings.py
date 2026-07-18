# Sprint 4 du pack « hameau maison » (sprintcration3delement.md) : bâtiments,
# lot 1 — 6 assets, complexité haute/moyenne. Le plus gros morceau du sprint :
# factorise `hamlet_common.pitched_roof`/`hip_roof` (toits à 2 et 4 pans),
# réutilisés ici et par gen_hamlet_buildings2.py (lot 2).
#
# Recrée en style maison, sans copier de géométrie tierce, la fonction et la
# silhouette générale de 6 bâtiments du Medieval Village Pack (Quaternius/CC0,
# déjà retraité en village_*.glb par import_village_pack.py) : Bell Tower,
# Blacksmith, Fantasy Barracks, Fantasy House (x3 variantes).
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_hamlet_buildings.py
#
# Portes/fenêtres : cubes en saillie sur la façade (pas de découpe booléenne),
# même convention que gen_nature_pack.gen_cabin/gen_hut — cohérent avec le
# reste du pack, pas d'ouverture traversante réelle.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    GLASS,
    METAL,
    METAL_DARK,
    ROOF,
    STONE,
    STONE_DARK,
    THATCH,
    WOOD,
    WOOD_DARK,
    cone,
    cube,
    export,
    hip_roof,
    mat,
    pitched_roof,
    reset_scene,
)


def gen_bell_tower():
    """Tour à cloche ~6.6 m : socle et fût de pierre carrés, étage de
    beffroi ajouré (poteaux de bois, cloche visible), flèche octogonale."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    wood = mat("bois_sombre", WOOD_DARK)
    metal = mat("metal_sombre", METAL_DARK)
    roof_m = mat("toit", ROOF)
    cube("Socle", stone, (0, 0, 0.5), (1.7, 1.7, 1.0))
    cube("Fut", stone_dark, (0, 0, 2.15), (1.35, 1.35, 2.3))
    cube("Corniche", stone, (0, 0, 3.35), (1.55, 1.55, 0.14))
    for sx in (-1, 1):
        for sy in (-1, 1):
            cube(f"Poteau{sx}{sy}", wood, (sx * 0.62, sy * 0.62, 3.85),
                 (0.12, 0.12, 1.0))
    cube("Plateau", wood, (0, 0, 4.38), (1.5, 1.5, 0.10))
    cone("Cloche", metal, (0, 0, 4.05), radius=0.22, depth=0.32, vertices=10, radius2=0.08)
    cone("SpireBase", roof_m, (0, 0, 4.9), radius=1.05, depth=0.9, vertices=8)
    cone("Fleche", roof_m, (0, 0, 5.75), radius=0.35, depth=1.1, vertices=8)
    export("hamlet_bell_tower.glb")


def gen_blacksmith():
    """Forge ~4.2 m : soubassement de pierre, mur de bois, cheminée massive,
    enclume et billot devant l'entrée."""
    stone = mat("pierre", STONE)
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    glass = mat("vitre", GLASS)
    metal_dark = mat("metal_sombre", METAL_DARK)
    cube("Fondation", stone, (0, 0, 0.35), (3.4, 3.0, 0.7))
    cube("Mur", wood, (0, 0, 1.55), (3.2, 2.8, 1.7))
    cube("Porte", wood_dark, (0, 1.41, 1.15), (0.9, 0.06, 1.7))
    cube("Fenetre", glass, (-1.1, 1.41, 1.9), (0.6, 0.05, 0.55))
    pitched_roof("Forge", roof_m, span_x=3.2, depth_y=2.8, rise=1.1, base_z=2.4)
    # Cheminée massive côté -X, dépasse le faîte (base_z + rise ≈ 3.5).
    cube("Cheminee", stone, (-1.55, -0.9, 2.1), (0.7, 0.7, 3.8))
    cube("CheminHaut", stone, (-1.55, -0.9, 4.05), (0.85, 0.85, 0.2))
    # Enclume sur billot, devant la porte.
    cube("Billot", wood_dark, (0.9, 1.9, 0.35), (0.4, 0.4, 0.7))
    cube("Enclume", metal_dark, (0.9, 1.9, 0.78), (0.55, 0.22, 0.16))
    export("hamlet_blacksmith.glb")


def gen_barracks():
    """Caserne ~7.4 m : long bâtiment à 2 portes et 4 fenêtres, toit à deux
    pans, fanion sur le faîte — le plus massif du lot."""
    stone = mat("pierre", STONE)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    glass = mat("vitre", GLASS)
    cloth = mat("fanion", (0.62, 0.16, 0.14))
    cube("Mur", stone, (0, 0, 1.3), (7.2, 3.2, 2.6))
    for sx in (-1, 1):
        cube(f"Porte{sx}", wood_dark, (sx * 2.2, 1.61, 0.95), (0.9, 0.06, 1.7))
    for i, x in enumerate((-3.1, -0.7, 0.7, 3.1)):
        cube(f"Fenetre{i}", glass, (x, 1.61, 1.95), (0.55, 0.05, 0.55))
    pitched_roof("Caserne", roof_m, span_x=7.2, depth_y=3.2, rise=1.35, base_z=2.6)
    cube("MatDrapeau", wood_dark, (0, 0, 4.4), (0.06, 0.06, 0.7))
    cube("Fanion", cloth, (0.26, 0, 4.6), (0.42, 0.02, 0.22))
    export("hamlet_barracks.glb")


def gen_house_a():
    """Maison A ~3.6 m : petite chaumière, toit de chaume, façade sobre —
    la plus modeste des trois variantes."""
    wall = mat("torchis", (0.55, 0.45, 0.32))
    thatch_m = mat("chaume", THATCH)
    door_m = mat("porte", WOOD_DARK)
    glass = mat("vitre", GLASS)
    cube("Mur", wall, (0, 0, 1.1), (2.6, 2.4, 2.2))
    cube("Porte", door_m, (0.6, 1.21, 0.75), (0.7, 0.05, 1.5))
    cube("Fenetre", glass, (-0.6, 1.21, 1.35), (0.5, 0.05, 0.5))
    pitched_roof("MaisonA", thatch_m, span_x=2.6, depth_y=2.4, rise=1.15, base_z=2.2,
                 thickness=0.16)
    export("hamlet_house_a.glb")


def gen_house_b():
    """Maison B ~4.2 m : plus haute et étroite, toit à croupe, à colombage —
    variante urbaine (ruelle), silhouette verticale."""
    base = mat("pierre", STONE)
    beam = mat("colombage", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    door_m = mat("porte", WOOD_DARK)
    glass = mat("vitre", GLASS)
    cube("Mur", base, (0, 0, 1.6), (2.2, 2.2, 3.2))
    for sx in (-1, 1):
        cube(f"Colombage{sx}", beam, (sx * 0.75, 1.11, 1.6), (0.10, 0.06, 3.2))
    cube("Porte", door_m, (0, 1.11, 0.85), (0.7, 0.05, 1.7))
    cube("FenetreHaut", glass, (0, 1.11, 2.5), (0.55, 0.05, 0.55))
    hip_roof("MaisonB", roof_m, span_x=2.2, span_y=2.2, rise=1.0, base_z=3.2)
    export("hamlet_house_b.glb")


def gen_house_c():
    """Maison C ~4.6×3.4 m : la plus large, deux fenêtres en façade, toit de
    tuiles à deux pans — variante « maison de famille » du lot."""
    wall = mat("torchis_clair", (0.60, 0.50, 0.36))
    roof_m = mat("toit", ROOF)
    door_m = mat("porte", WOOD_DARK)
    glass = mat("vitre", GLASS)
    cube("Mur", wall, (0, 0, 1.2), (4.4, 3.2, 2.4))
    cube("Porte", door_m, (0, 1.61, 0.85), (0.8, 0.05, 1.7))
    for sx in (-1, 1):
        cube(f"Fenetre{sx}", glass, (sx * 1.5, 1.61, 1.55), (0.55, 0.05, 0.55))
    pitched_roof("MaisonC", roof_m, span_x=4.4, depth_y=3.2, rise=1.25, base_z=2.4)
    export("hamlet_house_c.glb")


ASSETS = [
    gen_bell_tower,
    gen_blacksmith,
    gen_barracks,
    gen_house_a,
    gen_house_b,
    gen_house_c,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[hamlet_buildings] pack complet : {len(ASSETS)} fichiers")
