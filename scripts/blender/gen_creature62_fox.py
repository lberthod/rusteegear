"""Génère assets/models/creature62.glb : monstre n°62, un renard roux.

Quadrupède svelte — pelage roux, museau et poitrail crème, pattes à
« bas » sombres, oreilles pointues à bout noir, grande queue touffue à
pointe blanche. Conventions et optimisations partagées : voir
`creature_kit.py` (face -Y, rig un os/pièce, clips Idle 40 fr / Walk 24 fr
bouclables, LOD automatique des primitives, export animation optimisé,
aucun vertex sous z=0 + marge 0,02).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature62_fox.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from creature_kit import (  # noqa: E402
    LEGS4, build_creature, cone, cylinder, fresh_scene, material,
    quad_bones, quad_walk_keys, sphere,
)


def renard():
    fresh_scene()
    fur = material("Creature62Fur", (0.82, 0.38, 0.14))  # roux
    cream = material("Creature62Cream", (0.95, 0.90, 0.80))  # poitrail/museau
    white = material("Creature62White", (0.97, 0.97, 0.94))  # bout de queue
    dark = material("Creature62Dark", (0.08, 0.07, 0.09))  # yeux/nez/bas/pattes

    # Corps svelte + poitrail crème.
    sphere("Body", fur, (0, 0.10, 0.62), (0.30, 0.58, 0.32))
    sphere("Body", cream, (0, -0.25, 0.52), (0.24, 0.26, 0.20))
    # Tête + museau effilé + truffe + yeux.
    sphere("Head", fur, (0, -0.78, 0.78), (0.24, 0.26, 0.22))
    sphere("Head", cream, (0, -1.00, 0.72), (0.16, 0.20, 0.14))
    sphere("Head", dark, (0, -1.16, 0.70), (0.05, 0.05, 0.05))  # truffe
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.13, -0.95, 0.85), (0.045, 0.03, 0.045))
    # Oreilles pointues, bout sombre.
    for sx in (-1, 1):
        cone("Head", fur, (sx * 0.13, -0.68, 1.00), (0.09, 0.09, 0.22),
             rotation=(math.radians(-15), 0, math.radians(sx * 12)))
        sphere("Head", dark, (sx * 0.15, -0.72, 1.19), (0.04, 0.04, 0.05))
    # Pattes : fourrure roux au-dessus, « bas » sombres + coussinet.
    for bone, x, y in (("LegFL", -0.20, -0.42), ("LegFR", 0.20, -0.42),
                       ("LegBL", -0.20, 0.42), ("LegBR", 0.20, 0.42)):
        cylinder(bone, fur, (x, y, 0.30), (0.075, 0.075, 0.24))
        cylinder(bone, dark, (x, y, 0.14), (0.07, 0.07, 0.20))
        sphere(bone, dark, (x, y, 0.075), (0.075, 0.08, 0.055))
    # Queue touffue, chaîne de sphères qui s'évase puis pointe blanche.
    sphere("Tail", fur, (0, 0.62, 0.72), (0.14, 0.20, 0.14))
    sphere("Tail", fur, (0, 0.92, 0.80), (0.16, 0.22, 0.16))
    sphere("Tail", fur, (0, 1.20, 0.86), (0.15, 0.20, 0.15))
    sphere("Tail", white, (0, 1.44, 0.90), (0.13, 0.15, 0.13))

    bones = quad_bones(
        x=0.20, yf=-0.42, yb=0.42, top=0.42,
        body=((0, 0.55, 0.60), (0, -0.55, 0.68)),
        extra={
            "Head": ("Body", (0, -0.55, 0.68), (0, -1.05, 0.80)),
            "Tail": ("Body", (0, 0.55, 0.62), (0, 1.55, 0.88)),
        },
    )

    def idle(key_rot, key_loc):
        # Respiration lente, oreilles qui frémissent, queue qui ondule,
        # tête qui guette. Pattes gardées neutres.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, roll in ((1, 0.06), (20, -0.06), (40, 0.06)):
            key_rot("Head", f, (0, roll, 0))
        for f, sway in ((1, 0.30), (20, -0.30), (40, 0.30)):
            key_rot("Tail", f, (0, 0, sway))

    def walk(key_rot, key_loc):
        def extras(key_rot):
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                key_rot("Head", f, (nod, 0, 0))
            for f, sway in ((1, 0.35), (13, -0.35), (24, 0.35)):
                key_rot("Tail", f, (0, 0, sway))

        quad_walk_keys(key_rot, key_loc, swing=25, extras=extras)

    build_creature("creature62", bones, idle, walk, cam=0.8)


renard()
print("PACK DONE")
