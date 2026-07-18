# Sprint 3 (partie 1) du pack « hameau maison » (sprintcration3delement.md) :
# mobilier / props, lot 2 — 6 assets, complexité faible/moyenne.
#
# Recrée en style maison, sans copier de géométrie tierce, la fonction et la
# silhouette générale de 6 pièces du Medieval Village Pack (Quaternius/CC0,
# déjà retraité en village_*.glb par import_village_pack.py) : Hay, Market
# Stand (x2), Package (x2), Sawmill Saw.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_hamlet_props2.py
#
# Sortie : assets/models/hamlet_*.glb — voir hamlet_common.py pour les
# contraintes moteur et la mémoire projet `charte-graphique-assets-maison`
# pour la charte complète (palette, budget, pièges déjà résolus : blob+jitter
# près du sol, rayon réel d'une sphère écrasée pour les éléments accrochés).

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    CLOTH,
    CLOTH_DARK,
    HAY,
    METAL,
    METAL_DARK,
    WOOD,
    WOOD_DARK,
    blob,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)


def gen_hay():
    """Botte de foin ~0.9 m : cylindre couché (grosse balle ronde) + brins de
    paille en bout (blobs irréguliers) — plus clair que THATCH (toit)."""
    hay = mat("paille", HAY)
    cylinder("Balle", hay, (0, 0, 0.45), radius=0.45, depth=0.9, vertices=12,
              rotation=(0, math.pi / 2, 0))
    for sx in (-1, 1):
        blob(f"Brins{sx}", hay, (sx * 0.46, 0, 0.45), radius=0.16, squash=0.6, jitter=0.06)
    export("hamlet_hay.glb")


def gen_market_stand_a():
    """Étal de marché ~1.8×1.2 m : table + 2 montants + auvent de toile plat —
    variante A, auvent uni couleur toile claire."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cloth = mat("toile_auvent", CLOTH)
    cube("Table", wood, (0, 0, 0.5), (1.8, 0.9, 0.08))
    for sx in (-1, 1):
        cube(f"Pied{sx}", dark, (sx * 0.8, 0, 0.25), (0.10, 0.7, 0.5))
    for sx in (-1, 1):
        cube(f"Montant{sx}", dark, (sx * 0.8, -0.3, 1.15), (0.08, 0.08, 1.3))
    cube("Auvent", cloth, (0, -0.3, 1.85), (2.0, 1.1, 0.06))
    export("hamlet_market_stand_a.glb")


def gen_market_stand_b():
    """Étal de marché ~1.8×1.2 m : variante B, auvent incliné deux pans (plus
    couvrant) + petite étagère de marchandises (caisses miniatures)."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cloth = mat("toile_auvent_sombre", CLOTH_DARK)
    cube("Table", wood, (0, 0, 0.5), (1.8, 0.9, 0.08))
    for sx in (-1, 1):
        cube(f"Pied{sx}", dark, (sx * 0.8, 0, 0.25), (0.10, 0.7, 0.5))
        cube(f"Montant{sx}", dark, (sx * 0.8, -0.3, 1.25), (0.08, 0.08, 1.5))
    for side in (-1, 1):
        p = cube(f"Pan{side}", cloth, (side * 0.5, -0.3, 1.95), (1.1, 1.0, 0.06))
        p.rotation_euler = (0, side * math.radians(-12), 0)
    for i, (x, z) in enumerate([(-0.55, 0.62), (-0.15, 0.62), (0.35, 0.68)]):
        cube(f"Marchandise{i}", wood if i % 2 == 0 else dark, (x, 0.15, z), (0.22, 0.22, 0.16))
    export("hamlet_market_stand_b.glb")


def gen_package_a():
    """Paquet ficelé ~0.4 m : caisse de toile sombre + croix de corde en
    relief sur le dessus — variante A, forme carrée."""
    cloth = mat("toile_sombre", CLOTH_DARK)
    rope = mat("corde", (0.55, 0.46, 0.30))
    cube("Paquet", cloth, (0, 0, 0.2), (0.4, 0.4, 0.4))
    cube("CordeX", rope, (0, 0, 0.41), (0.42, 0.05, 0.02))
    cube("CordeY", rope, (0, 0, 0.41), (0.05, 0.42, 0.02))
    export("hamlet_package_a.glb")


def gen_package_b():
    """Baluchon noué ~0.35 m : bourse de toile arrondie (blob) + col noué —
    variante B, forme souple distincte du paquet carré."""
    cloth = mat("toile", CLOTH)
    rope = mat("corde", (0.55, 0.46, 0.30))
    blob("Bourse", cloth, (0, 0, 0.16), radius=0.20, squash=0.75, jitter=0.04)
    cylinder("Col", rope, (0, 0, 0.30), radius=0.05, depth=0.08, vertices=7)
    export("hamlet_package_b.glb")


def gen_sawmill_saw():
    """Scie circulaire de scierie ~1.2 m : lame métallique plate sur un
    portique de bois en A, moyeu central sombre — pièce d'accent industriel
    du hameau (posée devant la Scierie dans la scène, hors asset)."""
    wood = mat("bois_sombre", WOOD_DARK)
    metal = mat("metal", METAL)
    metal_dark = mat("metal_sombre", METAL_DARK)
    for sx in (-1, 1):
        p = cube(f"PiedA{sx}", wood, (sx * 0.28, 0, 0.55), (0.12, 0.4, 1.1))
        p.rotation_euler = (0, 0, sx * math.radians(12))
    cube("Traverse", wood, (0, 0, 1.05), (0.75, 0.35, 0.10))
    cylinder("Lame", metal, (0, 0, 1.05), radius=0.55, depth=0.06, vertices=16,
              rotation=(math.pi / 2, 0, 0))
    cylinder("Moyeu", metal_dark, (0, 0.04, 1.05), radius=0.10, depth=0.10, vertices=10,
              rotation=(math.pi / 2, 0, 0))
    export("hamlet_sawmill_saw.glb")


ASSETS = [
    gen_hay,
    gen_market_stand_a,
    gen_market_stand_b,
    gen_package_a,
    gen_package_b,
    gen_sawmill_saw,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[hamlet_props2] pack complet : {len(ASSETS)} fichiers")
