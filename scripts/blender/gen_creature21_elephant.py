"""Génère assets/models/creature21.glb : monstre n°21 « style Pokémon », un éléphanteau.

Quadrupède gris pachyderme — corps massif, grandes oreilles plates, trompe
articulée (os dédié), défenses crème, pattes épaisses à ongles crème, petite
queue à touffe sombre. Conventions et optimisations partagées : voir
`creature_kit.py` (face -Y, rig un os/pièce, clips Idle 40 fr / Walk 24 fr
bouclables, LOD automatique des primitives, export animation optimisé).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature21_elephant.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from creature_kit import (  # noqa: E402
    LEGS4, build_creature, cone, cylinder, fresh_scene, material, sphere,
)


def elephanteau():
    fresh_scene()
    hide = material("Creature21Hide", (0.46, 0.47, 0.52))  # gris pachyderme
    ear = material("Creature21Ear", (0.58, 0.46, 0.50))  # intérieur d'oreille rosé
    ivory = material("Creature21Ivory", (0.93, 0.90, 0.78))  # défenses/ongles
    dark = material("Creature21Dark", (0.12, 0.10, 0.12))  # yeux/touffe de queue

    # Corps (avant = -Y).
    sphere("Body", hide, (0, 0.15, 1.20), (0.95, 1.25, 0.88))
    # Tête + yeux.
    sphere("Head", hide, (0, -1.10, 1.62), (0.62, 0.56, 0.58))
    sphere("Head", dark, (-0.30, -1.55, 1.80), (0.085, 0.05, 0.10))  # œil G
    sphere("Head", dark, (0.30, -1.55, 1.80), (0.085, 0.05, 0.10))  # œil D
    # Grandes oreilles plates : disque gris + intérieur rosé légèrement décalé.
    for sx in (-1, 1):
        sphere("Head", hide, (sx * 0.72, -0.92, 1.72), (0.42, 0.10, 0.55))
        sphere("Head", ear, (sx * 0.78, -0.96, 1.70), (0.30, 0.06, 0.40))
    # Trompe : chaîne de sphères qui s'affinent, du mufle vers le sol.
    for loc, r in (
        ((0, -1.62, 1.42), 0.185),
        ((0, -1.74, 1.12), 0.160),
        ((0, -1.82, 0.84), 0.135),
        ((0, -1.88, 0.60), 0.110),
    ):
        sphere("Trunk", hide, loc, (r, r, r * 1.35))
    # Défenses : cônes ivoire pointés vers l'avant-bas.
    for sx in (-1, 1):
        cone("Head", ivory, (sx * 0.32, -1.58, 1.22), (0.11, 0.11, 0.34),
             rotation=(math.radians(125), 0, math.radians(sx * 16)))
    # Pattes épaisses + ongles ivoire.
    for bone, x, y in (("LegFL", -0.52, -0.60), ("LegFR", 0.52, -0.60),
                       ("LegBL", -0.52, 0.78), ("LegBR", 0.52, 0.78)):
        cylinder(bone, hide, (x, y, 0.475), (0.24, 0.24, 0.95))
        sphere(bone, ivory, (x, y - 0.20, 0.10), (0.14, 0.10, 0.09))  # ongle
    # Queue fine à touffe sombre.
    cone("Tail", hide, (0, 1.42, 1.20), (0.09, 0.09, 0.42),
         rotation=(math.radians(-155), 0, 0))
    sphere("Tail", dark, (0, 1.60, 0.88), (0.12, 0.12, 0.15))

    bones = {
        "Body": ("Root", (0, 0.60, 1.15), (0, -0.60, 1.25)),
        "Head": ("Body", (0, -0.90, 1.50), (0, -1.60, 1.75)),
        "Trunk": ("Head", (0, -1.58, 1.50), (0, -1.90, 0.55)),
        "Tail": ("Body", (0, 1.30, 1.30), (0, 1.65, 0.80)),
        "LegFL": ("Body", (-0.52, -0.60, 0.95), (-0.52, -0.60, 0.02)),
        "LegFR": ("Body", (0.52, -0.60, 0.95), (0.52, -0.60, 0.02)),
        "LegBL": ("Body", (-0.52, 0.78, 0.95), (-0.52, 0.78, 0.02)),
        "LegBR": ("Body", (0.52, 0.78, 0.95), (0.52, 0.78, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Respiration lente, trompe qui se balance, oreilles via léger roulis
        # de tête, queue qui chasse les mouches. Pattes keyframées neutres.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.28), (20, -0.28), (40, 0.28)):
            key_rot("Trunk", f, (0.10, 0, sw))
        for f, roll in ((1, 0.05), (20, -0.05), (40, 0.05)):
            key_rot("Head", f, (0, roll, 0))
        for f, sway in ((1, 0.35), (20, -0.35), (40, 0.35)):
            key_rot("Tail", f, (0, 0, sway))

    def walk(key_rot, key_loc):
        # Pas lourd en diagonale, trompe qui balance d'avant en arrière,
        # tête qui dodeline, queue qui fouette.
        swing = math.radians(20)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.30), (13, -0.30), (24, 0.30)):
            key_rot("Trunk", f, (sw, 0, 0))
        for f, nod in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sway in ((1, 0.25), (13, -0.25), (24, 0.25)):
            key_rot("Tail", f, (0, 0, sway))

    build_creature("creature21", bones, idle, walk, cam=1.0)


elephanteau()
print("PACK DONE")
