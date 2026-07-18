# Sprint 4 (lot B) du pack « siège du hameau » (creation3DBlendersuite.md) :
# lande environnante lot 1 — 6 assets, complexité faible. Décor extérieur au
# hameau (au-delà de nature_*), toujours sans texture ni transparence.
#
# BROWN/LEAF_DARK ci-dessous reprennent TELS QUELS les valeurs déjà utilisées
# par gen_nature_pack.py (pas une nouvelle palette : ce module ne peut pas
# être importé directement, son exécution au niveau module regénère tout le
# pack nature au chargement — les constantes sont donc recopiées ici, comme
# chaque script gen_hamlet_*.py recopie ses propres dimensions locales).
# POND est une teinte réellement nouvelle : aucun ton d'eau stagnante n'existe
# ailleurs dans le projet, justifié une fois ici plutôt que d'improviser un
# gris-vert ad hoc dans le script.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_lande.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    STONE,
    STONE_DARK,
    blob,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)

BROWN = (0.32, 0.22, 0.11)  # = gen_nature_pack.BROWN (troncs, bois mort)
LEAF_DARK = (0.18, 0.42, 0.16)  # = gen_nature_pack.LEAF_DARK (broussaille)
POND = (0.16, 0.20, 0.16)  # nouvelle teinte : eau stagnante, verte et sombre


def gen_moor_rock():
    """Rocher de lande : amas de 3 blocs de pierre irréguliers, plus massif
    et anguleux qu'un simple caillou de nature_pack."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    blob("RocheA", stone, (0, 0, 0.22), radius=0.32, squash=0.8, jitter=0.08)
    blob("RocheB", stone_dark, (0.28, 0.12, 0.14), radius=0.20, squash=0.75, jitter=0.06)
    blob("RocheC", stone_dark, (-0.2, -0.15, 0.10), radius=0.16, squash=0.7, jitter=0.05)
    export("siege_moor_rock.glb")


def gen_dead_tree():
    """Arbre mort tourmenté : tronc noueux en deux segments désaxés + trois
    branches nues (pas de feuillage) — silhouette distincte des arbres de
    nature_pack."""
    brown = mat("bois_mort", BROWN)
    # +0.02 : la base d'un cylindre tourné autour de son centre descend
    # légèrement sous z=0 (même piège que siege_stake_row).
    trunk1 = cylinder("Tronc1", brown, (0, 0, 0.62), radius=0.10, depth=1.2, vertices=7)
    trunk1.rotation_euler = (math.radians(6), math.radians(4), 0)
    trunk2 = cylinder("Tronc2", brown, (0.12, 0.04, 1.5), radius=0.07, depth=0.9, vertices=7)
    trunk2.rotation_euler = (math.radians(-14), math.radians(10), 0)
    branches = [(0.3, 0.0, 1.7, 40, 0), (-0.25, 0.1, 1.55, -35, 60), (0.05, -0.2, 1.9, 30, -80)]
    for i, (bx, by, bz, tilt, yaw) in enumerate(branches):
        b = cylinder(f"Branche{i}", brown, (bx, by, bz), radius=0.035, depth=0.5, vertices=6)
        b.rotation_euler = (math.radians(70 + tilt * 0.3), math.radians(yaw), math.radians(tilt))
    export("siege_dead_tree.glb")


def gen_scattered_bones():
    """Ossements épars : trois os stylisés (cylindres à bouts renflés) posés
    à plat, teinte pierre claire (blanchis par la lande)."""
    bone = mat("os", STONE)
    # z=0.045 : garde-sol pour les renflements (blob radius 0.045, squash 0.8
    # -> min_z = z - 0.036 ; blob() ne borne le jitter que si jitter>0, ici 0).
    positions = [(0.0, 0.0, 0.045, 0), (0.22, 0.1, 0.045, 50), (-0.18, -0.08, 0.045, -30)]
    for i, (x, y, z, yaw) in enumerate(positions):
        cylinder(f"Os{i}", bone, (x, y, z), radius=0.025, depth=0.32, vertices=6,
                  rotation=(math.pi / 2, 0, math.radians(yaw)))
        for s in (-1, 1):
            ex, ey = x + s * 0.16 * math.cos(math.radians(yaw)), y + s * 0.16 * math.sin(math.radians(yaw))
            blob(f"OsBout{i}_{s}", bone, (ex, ey, z), radius=0.045, squash=0.8, jitter=0.0)
    export("siege_scattered_bones.glb")


def gen_menhir():
    """Menhir de lande : monolithe dressé, légèrement penché — silhouette
    verticale isolée, repère visuel de la lande."""
    stone_dark = mat("pierre_sombre", STONE_DARK)
    stone = mat("pierre", STONE)
    m = cube("Monolithe", stone_dark, (0, 0, 0.92), (0.35, 0.22, 1.8))
    m.rotation_euler = (math.radians(4), math.radians(2), math.radians(8))
    cube("Socle", stone, (0, 0, 0.05), (0.55, 0.4, 0.1))
    export("siege_menhir.glb")


def gen_thorny_scrub():
    """Broussaille épineuse : touffe basse (LEAF_DARK) hérissée d'épines
    fines (BROWN) — silhouette hostile, cohérente avec le ton désaturé de la
    charte (décor, pas un enjeu de gameplay)."""
    leaf = mat("feuillage_sombre", LEAF_DARK)
    thorn = mat("epine", BROWN)
    blob("Touffe", leaf, (0, 0, 0.16), radius=0.26, squash=0.7, jitter=0.06)
    n = 8
    for i in range(n):
        a = i * math.tau / n
        x, y = 0.2 * math.cos(a), 0.2 * math.sin(a)
        spike = cylinder(f"Epine{i}", thorn, (x, y, 0.22), radius=0.012, depth=0.16, vertices=5)
        spike.rotation_euler = (math.radians(60) * math.sin(a), math.radians(60) * math.cos(a), 0)
    export("siege_thorny_scrub.glb")


def gen_stagnant_pond():
    """Mare stagnante : flaque opaque (l'alpha est ignoré par le moteur,
    donc pas de vraie transparence) bordée d'un anneau de boue en léger
    relief, pour casser l'aplat du sol comme hamlet_path_straight."""
    pond = mat("eau_stagnante", POND, roughness=0.25)
    mud = mat("boue", STONE_DARK, roughness=0.95)
    cylinder("Berge", mud, (0, 0, 0.015), radius=0.75, depth=0.03, vertices=14)
    cylinder("Flaque", pond, (0, 0, 0.03), radius=0.62, depth=0.03, vertices=14)
    export("siege_stagnant_pond.glb")


ASSETS = [
    gen_moor_rock,
    gen_dead_tree,
    gen_scattered_bones,
    gen_menhir,
    gen_thorny_scrub,
    gen_stagnant_pond,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_lande] pack complet : {len(ASSETS)} fichiers")
