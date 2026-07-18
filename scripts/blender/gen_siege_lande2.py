# Sprint 5 (lot A) du pack « siège du hameau » (creation3DBlendersuite.md) :
# lande environnante lot 2 — 4 assets, complexité faible. Suite de
# gen_siege_lande.py (mêmes constantes BROWN/LEAF_DARK recopiées de
# gen_nature_pack.py, même teinte POND introduite au lot précédent — pas
# utilisée ici mais gardée pour référence si besoin futur).
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_lande2.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    SMOKE,
    STONE,
    STONE_DARK,
    blob,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)

BROWN = (0.32, 0.22, 0.11)  # = gen_nature_pack.BROWN / gen_siege_lande.BROWN
CLOTH_TORN = (0.42, 0.34, 0.22)  # = hamlet_common.CLOTH_DARK (toile déchirée)


def gen_ravine():
    """Ravine de terrain : tranchée peu profonde à flancs de terre/pierre,
    fond plat — casse le relief plat de la lande, traversable (pas un mur)."""
    stone_dark = mat("pierre_sombre", STONE_DARK)
    stone = mat("pierre", STONE)
    width, length, depth = 1.6, 3.2, 0.5
    cube("Fond", stone_dark, (0, 0, 0.02), (width * 0.6, length, 0.04))
    angle = math.radians(28)
    half_w, half_h = (width * 0.5) / 2, depth / 2
    # Un flanc incliné (cube tourné autour de son propre centre) descend sous
    # z=0 du montant du coin le plus bas — remonter le centre de cette
    # pénétration (même piège que siege_wall_corner/siege_bastion, ici
    # dérivé géométriquement plutôt que tâtonné).
    z_center = half_w * abs(math.sin(angle)) + half_h * abs(math.cos(angle))
    for sx in (-1, 1):
        slope = cube(f"Flanc{sx}", stone, (sx * width * 0.4, 0, z_center), (width * 0.5, length, depth))
        slope.rotation_euler = (0, sx * angle, 0)
    export("siege_ravine.glb")


def gen_ruined_banner_post():
    """Poteau de bannière en ruine : mât brisé (moignon court) + tissu
    déchiré tombé au sol — variante « détruite » d'un mât de bannière, pour
    marquer un site déjà tombé (avant-poste, ancien camp)."""
    wood_dark = mat("bois_sombre", BROWN)
    cloth = mat("tissu_dechire", CLOTH_TORN)
    cylinder("Moignon", wood_dark, (0, 0, 0.30), radius=0.05, depth=0.56, vertices=8,
              rotation=(0, math.radians(10), 0))
    cube("TissuSol", cloth, (0.35, 0.05, 0.015), (0.55, 0.4, 0.03))
    export("siege_ruined_banner_post.glb")


def gen_war_cairn():
    """Cairn de guerre : monticule de pierres empilées marquant un site de
    bataille — 5 blocs décroissants légèrement décalés (pas un empilement
    parfaitement centré, pour une silhouette organique)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    rocks = [(0.0, 0.0, 0.30, 0.30), (0.08, -0.05, 0.22, 0.24), (-0.06, 0.07, 0.16, 0.19),
             (0.03, 0.02, 0.11, 0.14), (-0.02, -0.03, 0.06, 0.09)]
    z = 0.0
    for i, (x, y, r, _) in enumerate(rocks):
        z_center = z + r * 0.75
        blob(f"Pierre{i}", stone if i % 2 == 0 else stone_dark, (x, y, z_center),
             radius=r, squash=0.65, jitter=0.03)
        z += r * 1.1
    export("siege_war_cairn.glb")


def gen_low_mist():
    """Touffe de brume basse : forme opaque stylisée (l'alpha est ignoré par
    le moteur, cf. charte — jamais un plan semi-transparent), silhouette
    plate et floue par blobs superposés, teinte SMOKE (déjà utilisée pour la
    fumée du feu de camp)."""
    smoke = mat("brume", SMOKE, roughness=1.0)
    puffs = [(0.0, 0.0, 0.10, 0.30), (0.22, 0.08, 0.08, 0.22), (-0.20, -0.05, 0.07, 0.20),
             (0.05, -0.22, 0.06, 0.18)]
    for i, (x, y, z, r) in enumerate(puffs):
        blob(f"Brume{i}", smoke, (x, y, z), radius=r, squash=0.35, jitter=0.05)
    export("siege_low_mist.glb")


ASSETS = [
    gen_ravine,
    gen_ruined_banner_post,
    gen_war_cairn,
    gen_low_mist,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_lande2] pack complet : {len(ASSETS)} fichiers")
