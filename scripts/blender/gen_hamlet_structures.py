# Sprint 1 du pack « hameau maison » (sprintcration3delement.md) : structures
# et éléments d'architecture modulaires — 8 assets, complexité faible. Premier
# lot du sprint : valide le patron hamlet_common.py sur des objets simples
# avant d'attaquer le mobilier puis les bâtiments.
#
# Recrée en style maison, sans copier de géométrie tierce, la fonction et la
# silhouette générale de 8 pièces du Medieval Village Pack (Quaternius/CC0,
# déjà retraité en village_*.glb par import_village_pack.py) : Door Round,
# Door Straight, Round Window, Window (x2), Fence, Stairs, Path Straight.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_hamlet_structures.py
#
# Sortie : assets/models/hamlet_*.glb — voir hamlet_common.py pour les
# contraintes moteur (mesh joint, base_color_factor seul, sol z=0, Y-up) et
# la mémoire projet `charte-graphique-assets-maison` pour la charte complète.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    GLASS,
    STONE,
    STONE_DARK,
    WOOD,
    WOOD_DARK,
    cone,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)


def gen_door_round():
    """Porte à linteau arrondi ~2.1 m : jambages de pierre + disque (« lunette »)
    en guise d'arc, vantail de bois surmonté d'un petit disque assorti."""
    stone = mat("pierre", STONE)
    wood = mat("bois_sombre", WOOD_DARK)
    for sx in (-1, 1):
        cube(f"Jambage{sx}", stone, (sx * 0.5, 0, 1.05), (0.18, 0.22, 2.1))
    # Arc en plein cintre approximé par un disque plat (cylindre bas, axe Y) :
    # silhouette ronde lisible sans booléen, cohérent avec le style low-poly.
    cylinder("Arc", stone, (0, 0.02, 2.15), radius=0.62, depth=0.18, vertices=12,
              rotation=(math.pi / 2, 0, 0))
    cube("Vantail", wood, (0, 0.05, 0.95), (0.86, 0.08, 1.7))
    cylinder("VantailArc", wood, (0, 0.05, 1.85), radius=0.44, depth=0.08, vertices=12,
              rotation=(math.pi / 2, 0, 0))
    export("hamlet_door_round.glb")


def gen_door_straight():
    """Porte droite ~2.1 m : jambages + linteau droit, vantail à traverse et
    poignée annulaire (petit cube fin en guise d'anneau, pas de tore natif)."""
    stone = mat("pierre", STONE)
    wood = mat("bois_sombre", WOOD_DARK)
    handle = mat("ferrure", STONE_DARK)
    for sx in (-1, 1):
        cube(f"Jambage{sx}", stone, (sx * 0.5, 0, 1.05), (0.18, 0.22, 2.1))
    cube("Linteau", stone, (0, 0, 2.15), (1.36, 0.22, 0.2))
    cube("Vantail", wood, (0, 0.05, 0.95), (0.86, 0.08, 1.7))
    cube("Traverse", wood, (0, 0.09, 1.35), (0.86, 0.03, 0.1))
    cube("Poignee", handle, (0.32, 0.11, 0.95), (0.05, 0.05, 0.16))
    export("hamlet_door_straight.glb")


def gen_round_window():
    """Fenêtre ronde (oculus) ~0.85 m : cadre de pierre + carreau, deux
    disques concentriques plats. Origine locale à la BASE du cadre (comme les
    portes) : c'est une pièce de kit modulaire, positionnée en hauteur de mur
    par la scène — pas un objet posé au sol, donc pas de contrainte « pas de
    vertex sous z=0 » au sens du décor libre, mais la base de la pièce reste
    quand même calée sur z=0 pour un ancrage cohérent avec les portes."""
    stone = mat("pierre", STONE)
    glass = mat("vitre", GLASS)
    cy = 0.42  # rayon du cadre = hauteur du centre, pour une base flush à z=0
    cylinder("Cadre", stone, (0, 0, cy), radius=0.42, depth=0.14, vertices=14,
              rotation=(math.pi / 2, 0, 0))
    # Le carreau doit dépasser légèrement du cadre côté +Y (face avant) pour
    # rester visible : un disque entièrement encastré dans l'épaisseur du
    # cadre (piège rencontré ici) reste invisible sous toutes les caméras.
    cylinder("Carreau", glass, (0, 0.085, cy), radius=0.32, depth=0.05, vertices=14,
              rotation=(math.pi / 2, 0, 0))
    export("hamlet_round_window.glb")


def gen_window_a():
    """Fenêtre à croisillon ~0.9×1.1 m : cadre de bois, croix centrale, quatre
    carreaux — variante « fermée » du duo de fenêtres. Origine locale à la
    base du cadre (pièce de kit modulaire, cf. gen_round_window)."""
    wood = mat("bois_sombre", WOOD_DARK)
    glass = mat("vitre", GLASS)
    cy = 0.55  # demi-hauteur du cadre, pour une base flush à z=0
    cube("Cadre", wood, (0, 0.02, cy), (0.9, 0.10, 1.1))
    cube("Carreau", glass, (0, 0.08, cy), (0.78, 0.04, 0.98))
    cube("CroixV", wood, (0, 0.11, cy), (0.05, 0.05, 1.0))
    cube("CroixH", wood, (0, 0.11, cy), (0.82, 0.05, 0.05))
    export("hamlet_window_a.glb")


def gen_window_b():
    """Fenêtre à volets ouverts ~0.9×1.1 m : cadre + carreau simple, deux
    volets de bois clair rabattus de part et d'autre — variante « ouverte ».
    Origine locale à la base du cadre (pièce de kit modulaire)."""
    wood = mat("bois_sombre", WOOD_DARK)
    shutter_m = mat("bois", WOOD)
    glass = mat("vitre", GLASS)
    cy = 0.55  # demi-hauteur du cadre, pour une base flush à z=0
    cube("Cadre", wood, (0, 0.02, cy), (0.9, 0.10, 1.1))
    cube("Carreau", glass, (0, 0.08, cy), (0.78, 0.04, 0.98))
    for sx in (-1, 1):
        v = cube(f"Volet{sx}", shutter_m, (sx * 0.66, 0.16, cy), (0.42, 0.05, 1.02))
        v.rotation_euler = (0, 0, sx * math.radians(65))
    export("hamlet_window_b.glb")


def gen_fence():
    """Clôture à piquets ~2 m : deux poteaux + lisse basse PLEINE (visible des
    sondes de créature à 0,6 m, même piège documenté que nature_fence) +
    piquets verticaux rapprochés — silhouette distincte de la clôture à
    lisses de gen_nature_pack (village vs. campagne)."""
    post = mat("bois_sombre", WOOD_DARK)
    picket_m = mat("bois", WOOD)
    for sx in (-1, 1):
        cube(f"Poteau{sx}", post, (sx * 0.95, 0, 0.55), (0.12, 0.12, 1.1))
    cube("LisseBasse", post, (0, 0, 0.35), (2.0, 0.06, 0.5))
    n = 9
    for i in range(n):
        x = -0.85 + i * (1.7 / (n - 1))
        cube(f"Piquet{i}", picket_m, (x, 0, 0.5), (0.06, 0.05, 1.0))
    export("hamlet_fence.glb")


def gen_stairs():
    """Escalier de pierre ~1.6 m de long, 5 marches montantes — assez massif
    pour rester visible des sondes de créature (flancs pleins)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    n = 5
    step_h, step_d = 0.18, 0.32
    for i in range(n):
        x = i * step_d
        h = (i + 1) * step_h
        cube(f"Marche{i}", stone if i % 2 == 0 else stone_dark, (x, 0, h / 2), (step_d, 1.1, h))
    export("hamlet_stairs.glb")


def gen_path_straight():
    """Dalle de chemin ~1.2×1.2 m : socle plat + pavés irréguliers en relief
    léger, pour casser l'aplat du sol comme gen_grass_tuft/gen_flowers le font
    en prairie — ici pour les allées du hameau."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    cube("Socle", stone_dark, (0, 0, 0.02), (1.2, 1.2, 0.04))
    cobbles = [
        (-0.35, -0.35, 0.42, 0.42),
        (0.30, -0.30, 0.5, 0.36),
        (-0.30, 0.32, 0.36, 0.46),
        (0.32, 0.34, 0.44, 0.4),
        (0.0, 0.0, 0.4, 0.4),
    ]
    for i, (x, y, sx, sy) in enumerate(cobbles):
        cube(f"Pave{i}", stone, (x, y, 0.05), (sx, sy, 0.06))
    export("hamlet_path_straight.glb")


ASSETS = [
    gen_door_round,
    gen_door_straight,
    gen_round_window,
    gen_window_a,
    gen_window_b,
    gen_fence,
    gen_stairs,
    gen_path_straight,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[hamlet_structures] pack complet : {len(ASSETS)} fichiers")
