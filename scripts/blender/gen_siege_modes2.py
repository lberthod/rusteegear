# Sprint 4 (lot A) du pack « siège du hameau » (creation3DBlendersuite.md) :
# props des modes de jeu lot 2 — 4 assets, complexité faible à moyenne.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_modes2.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    GLOW_YELLOW,
    METAL_DARK,
    STONE,
    WOOD,
    WOOD_DARK,
    cone,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)
from siege_anim_common import build_prop, weight, weight_remaining  # noqa: E402

WALL_H = 3.0


def gen_spawn_beacon():
    """Balise de spawn : marqueur visuel de point d'apparition de vague —
    socle de pierre, mât de bois, orbe émissive (GLOW_YELLOW, même teinte que
    les fenêtres éclairées) au sommet pour rester visible de loin. Animée
    (addendum) : Root + Rune, l'orbe tourne en continu (vitesse constante,
    linear_bones) et pulse d'échelle, clip Idle 48f (boucle plus longue,
    moins répétitive)."""
    stone = mat("pierre", STONE)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    glow = mat("orbe", GLOW_YELLOW, emission=1.8)
    socle = cylinder("Socle", stone, (0, 0, 0.12), radius=0.28, depth=0.24, vertices=10)
    mat_ = cylinder("Mat", wood_dark, (0, 0, 1.1), radius=0.06, depth=1.75, vertices=8)
    orbe = cylinder("Orbe", glow, (0, 0, 2.05), radius=0.16, depth=0.28, vertices=10)
    weight(orbe, "Rune")
    weight_remaining([orbe])
    bones = {"Rune": ("Root", (0, 0, 2.05), (0, 0, 2.33))}

    def keyer(key_rot, key_loc, key_scale):
        key_rot("Rune", 1, (0, 0, 0))
        key_rot("Rune", 48, (0, 0, math.tau))
        for f, s in ((1, 1.0), (12, 1.08), (24, 1.0), (36, 1.12), (48, 1.0)):
            key_scale("Rune", f, (s, s, s))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_spawn_beacon", parts, bones, "Idle", keyer, linear_bones=("Rune",))


def gen_reserve_crate():
    """Caisse de réserve : munitions/ressources du mode défense — même
    silhouette de base que hamlet_crate (bloc + croisillons) mais cerclée de
    métal plutôt que de bois, pour se distinguer visuellement de la caisse
    civile du hameau."""
    wood = mat("bois", WOOD)
    metal_dark = mat("metal_sombre", METAL_DARK)
    cube("Bloc", wood, (0, 0, 0.3), (0.6, 0.6, 0.6))
    for sx in (-1, 1):
        cube(f"CercleX{sx}", metal_dark, (sx * 0.28, 0, 0.3), (0.04, 0.62, 0.6))
    for i, z in enumerate((0.08, 0.52)):
        cube(f"CercleZ{i}", metal_dark, (0, 0, z), (0.62, 0.62, 0.04))
    export("siege_reserve_crate.glb")


def gen_portcullis():
    """Herse : grille de porte relevable, calée sur l'ouverture de
    siege_gate_closed/burning (largeur 3,6 m, hauteur WALL_H) pour une
    intégration future sans re-échelle. Animée (addendum) : Root + Grille,
    toute la grille (une seule pièce rigide) glisse verticalement — pas de
    partie statique dans cet asset, tout est pondéré à "Grille"."""
    metal_dark = mat("metal_sombre", METAL_DARK)
    opening_w, opening_h = 3.6, WALL_H
    n_vert, n_horiz = 8, 5
    grille_objs = []
    for i in range(n_vert):
        x = -opening_w / 2 + 0.2 + i * (opening_w - 0.4) / (n_vert - 1)
        grille_objs.append(cube(f"Barreau{i}", metal_dark, (x, 0, opening_h / 2), (0.06, 0.10, opening_h)))
    for i in range(n_horiz):
        z = 0.2 + i * (opening_h - 0.4) / (n_horiz - 1)
        grille_objs.append(cube(f"Traverse{i}", metal_dark, (0, 0, z), (opening_w - 0.3, 0.10, 0.06)))
    for sx in (-1, 1):
        pointe = cone(f"Pointe{sx}", metal_dark, (sx * (opening_w / 2 - 0.2), 0, 0.12),
                       radius=0.08, depth=0.24, vertices=6)
        pointe.rotation_euler = (math.pi, 0, 0)
        grille_objs.append(pointe)
    for o in grille_objs:
        weight(o, "Grille")
    bones = {"Grille": ("Root", (0, 0, 0), (0, 0, 0.3))}

    def keyer(key_rot, key_loc, key_scale):
        for f, dz in ((1, 0.0), (20, opening_h - 0.5), (40, 0.0)):
            key_loc("Grille", f, (0, 0, dz))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_portcullis", parts, bones, "Idle", keyer)


def gen_stake_row():
    """Rangée de pieux : défense anti-monstre, ligne de piquets aiguisés
    plantés en quinconce, inclinés vers l'extérieur (silhouette de chevaux de
    frise stylisée). Chaque piquet est un unique cône effilé (radius2=0)
    tourné en bloc autour de son propre centre — pas de pointe assemblée
    séparément, pour éviter le défaut de pivot rencontré sur siege_trophy_pile
    (lame/hampe désalignées quand deux objets distincts partagent la même
    rotation sans pivot commun)."""
    wood_dark = mat("bois_sombre", WOOD_DARK)
    n = 7
    width = 2.6
    for i in range(n):
        x = -width / 2 + i * width / (n - 1)
        h = 0.55 if i % 2 == 0 else 0.42
        # +0.02 : la base d'un cône tourné autour de son centre descend
        # légèrement sous z=0 (rayon * sin(angle)) — marge couvrant le pire
        # cas (16°, rayon 0.05 -> ~0.014).
        stake = cone(f"Pieu{i}", wood_dark, (x, 0, h / 2 + 0.02), radius=0.05, depth=h, vertices=6)
        stake.rotation_euler = (math.radians(-16 if i % 2 == 0 else -8), 0, 0)
    export("siege_stake_row.glb")


ASSETS = [
    gen_spawn_beacon,
    gen_reserve_crate,
    gen_portcullis,
    gen_stake_row,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_modes2] pack complet : {len(ASSETS)} fichiers")
