"""Génère assets/models/creature52.glb … creature61.glb : 10 animaux du grand froid.

Pack « grand froid » — ours polaire, manchot empereur, renne, loup gris,
harfang des neiges, morse, phoque, bouquetin, lièvre arctique, yack. Que des
animaux réels, chacun avec une animation signature (flair de l'ours, dandine
du manchot, hurlement du loup, tête pivotante du harfang, bonds du lièvre…).
Conventions et optimisations partagées : voir `creature_kit.py` (face -Y,
un os par pièce à poids 1.0, clips Idle 40 fr / Walk 24 fr bouclables et
couvrants, LOD automatique des primitives, aucun vertex sous z=0 + marge,
QA par `check_creatures.py` après génération).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack52_61.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from creature_kit import (  # noqa: E402
    LEGS4, build_creature, cone, cylinder, fresh_scene, material,
    quad_bones, quad_walk_keys, sphere,
)


# =============================================================================
# Créature 52 — Ours polaire : massif crème, flaire le vent en Idle.
# =============================================================================
def ours_polaire():
    fresh_scene()
    fur = material("Ours52Fur", (0.92, 0.90, 0.84))
    fur_d = material("Ours52FurD", (0.78, 0.75, 0.68))
    dark = material("Ours52Dark", (0.08, 0.07, 0.07))

    sphere("Body", fur, (0, 0.10, 1.05), (0.72, 1.00, 0.68))
    sphere("Body", fur_d, (0, 0.55, 1.30), (0.42, 0.40, 0.35))  # croupe haute
    sphere("Head", fur, (0, -0.95, 1.30), (0.40, 0.42, 0.36))
    sphere("Head", fur, (0, -1.28, 1.20), (0.20, 0.20, 0.16))  # museau allongé
    sphere("Head", dark, (0, -1.44, 1.22), (0.06, 0.05, 0.05))  # truffe
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.17, -1.22, 1.40), (0.05, 0.04, 0.05))
        sphere("Head", fur_d, (sx * 0.26, -0.80, 1.58), (0.09, 0.06, 0.09))  # oreille
    for bone, x, y in (("LegFL", -0.42, -0.50), ("LegFR", 0.42, -0.50),
                       ("LegBL", -0.42, 0.65), ("LegBR", 0.42, 0.65)):
        cylinder(bone, fur, (x, y, 0.47), (0.19, 0.19, 0.88))
        sphere(bone, fur_d, (x, y - 0.08, 0.10), (0.20, 0.24, 0.09))  # large patte
    sphere("Tail", fur_d, (0, 1.05, 1.10), (0.11, 0.10, 0.10))

    bones = quad_bones(0.42, -0.50, 0.65, 0.90, ((0, 0.55, 1.00), (0, -0.55, 1.10)), {
        "Head": ("Body", (0, -0.75, 1.20), (0, -1.45, 1.25)),
        "Tail": ("Body", (0, 0.98, 1.10), (0, 1.20, 1.10)),
    })

    def idle(key_rot, key_loc):
        # Flaire le vent : le museau se lève haut et balaie lentement, comme
        # pour chercher une odeur de phoque à des kilomètres.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up, yaw in ((1, 0.0, 0.0), (12, -0.35, 0.15), (22, -0.35, -0.15),
                           (32, 0.0, 0.0), (40, 0.0, 0.0)):
            key_rot("Head", f, (up, 0, yaw))
        for f, sw in ((1, 0.12), (20, -0.12), (40, 0.12)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.06), (13, -0.04), (24, 0.06)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.1), (13, -0.1), (24, 0.1)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 18, extras)

    build_creature("creature52", bones, idle, walk, cam=1.05)


# =============================================================================
# Créature 53 — Manchot empereur : dandine, ailerons battants, col doré.
# =============================================================================
def manchot():
    fresh_scene()
    black = material("Manchot53Black", (0.10, 0.11, 0.13))
    white = material("Manchot53White", (0.92, 0.92, 0.90))
    gold = material("Manchot53Gold", (0.88, 0.68, 0.18))
    beak = material("Manchot53Beak", (0.20, 0.18, 0.18))

    sphere("Body", black, (0, 0.05, 0.85), (0.48, 0.44, 0.72))
    sphere("Body", white, (0, -0.16, 0.78), (0.38, 0.32, 0.58))  # plastron
    sphere("Head", black, (0, -0.05, 1.62), (0.26, 0.26, 0.24))
    for sx in (-1, 1):
        sphere("Head", gold, (sx * 0.17, -0.12, 1.52), (0.08, 0.06, 0.12))  # col
        sphere("Head", white, (sx * 0.09, -0.24, 1.66), (0.05, 0.035, 0.06))
        sphere("Head", beak, (sx * 0.09, -0.26, 1.66), (0.028, 0.02, 0.032))  # œil
    cone("Head", beak, (0, -0.30, 1.56), (0.035, 0.035, 0.16),
         rotation=(math.radians(105), 0, 0))
    for bone, sx in (("WingL", -1), ("WingR", 1)):  # ailerons plats
        sphere(bone, black, (sx * 0.50, 0.02, 0.92), (0.09, 0.22, 0.34))
    for bone, sx in (("LegL", -1), ("LegR", 1)):  # pattes courtes palmées
        cylinder(bone, beak, (sx * 0.16, 0.05, 0.14), (0.06, 0.06, 0.24))
        sphere(bone, beak, (sx * 0.16, -0.08, 0.05), (0.10, 0.14, 0.035))

    bones = {
        "Body": ("Root", (0, 0.30, 0.85), (0, -0.20, 0.95)),
        "Head": ("Body", (0, 0.0, 1.40), (0, -0.15, 1.75)),
        "WingL": ("Body", (-0.42, 0.02, 1.15), (-0.55, 0.02, 0.60)),
        "WingR": ("Body", (0.42, 0.02, 1.15), (0.55, 0.02, 0.60)),
        "LegL": ("Body", (-0.16, 0.05, 0.28), (-0.16, 0.05, 0.02)),
        "LegR": ("Body", (0.16, 0.05, 0.28), (0.16, 0.05, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Parade : les ailerons battent en arrière, la tête se lisse le
        # plastron puis remonte fièrement.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, a in ((1, 0.0), (10, 0.5), (16, 0.1), (22, 0.5), (30, 0.0), (40, 0.0)):
            key_rot("WingL", f, (a, 0, 0.2 if a else 0.0))
            key_rot("WingR", f, (a, 0, -0.2 if a else 0.0))
        for f, nod in ((1, 0.0), (14, 0.45), (24, 0.45), (32, -0.10), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        # Dandine : tout le corps roule d'un pied sur l'autre, petits pas,
        # les ailerons s'écartent pour l'équilibre.
        s = math.radians(18)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, roll in ((1, 0.16), (13, -0.16), (24, 0.16)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.15), (13, 0.25), (24, 0.15)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, roll in ((1, -0.10), (13, 0.10), (24, -0.10)):
            key_rot("Head", f, (0, roll, 0))

    build_creature("creature53", bones, idle, walk, cam=0.9)


# =============================================================================
# Créature 54 — Renne : bois ramifiés en chaînes de sphères, trot de toundra.
# =============================================================================
def renne():
    fresh_scene()
    brown = material("Renne54Brown", (0.42, 0.30, 0.20))
    cream = material("Renne54Cream", (0.85, 0.78, 0.65))
    antler = material("Renne54Antler", (0.70, 0.62, 0.48))
    dark = material("Renne54Dark", (0.08, 0.07, 0.06))

    sphere("Body", brown, (0, 0.10, 1.00), (0.52, 0.85, 0.50))
    sphere("Body", cream, (0, -0.45, 0.90), (0.40, 0.35, 0.42))  # poitrail clair
    sphere("Head", brown, (0, -0.85, 1.35), (0.30, 0.34, 0.28))
    sphere("Head", dark, (0, -1.14, 1.28), (0.07, 0.06, 0.06))  # mufle
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.14, -1.02, 1.46), (0.045, 0.035, 0.05))
        sphere("Head", brown, (sx * 0.24, -0.72, 1.56), (0.09, 0.05, 0.11))  # oreille
        # Bois : tige principale dense + deux rameaux — les sphères adjacentes
        # DOIVENT se chevaucher (pas > 2 rayons), sinon perles flottantes.
        for k in range(7):
            t = k / 6.0
            sphere("Head", antler,
                   (sx * (0.14 + 0.20 * t), -0.70 + 0.26 * t, 1.62 + 0.52 * t),
                   (0.065, 0.065, 0.065))
        for k in range(3):  # rameau avant depuis le tiers bas
            sphere("Head", antler,
                   (sx * 0.20, -0.80 - 0.09 * k, 1.80 + 0.07 * k),
                   (0.05, 0.05, 0.05))
        for k in range(3):  # rameau externe depuis le milieu
            sphere("Head", antler,
                   (sx * (0.26 + 0.08 * k), -0.58, 1.88 + 0.06 * k),
                   (0.045, 0.045, 0.045))
    for bone, x, y in (("LegFL", -0.30, -0.50), ("LegFR", 0.30, -0.50),
                       ("LegBL", -0.30, 0.58), ("LegBR", 0.30, 0.58)):
        cylinder(bone, brown, (x, y, 0.40), (0.11, 0.11, 0.78))
        cylinder(bone, dark, (x, y, 0.07), (0.12, 0.12, 0.12))  # sabot
    sphere("Tail", cream, (0, 0.90, 1.05), (0.11, 0.10, 0.11))

    bones = quad_bones(0.30, -0.50, 0.58, 0.78, ((0, 0.50, 0.98), (0, -0.50, 1.05)), {
        "Head": ("Body", (0, -0.65, 1.20), (0, -1.20, 1.40)),
        "Tail": ("Body", (0, 0.82, 1.05), (0, 1.05, 1.05)),
    })

    def idle(key_rot, key_loc):
        # Broute le lichen : la tête plonge vers le sol, remonte mâchouiller,
        # l'oreille chasse un moustique (roulis de tête).
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod, roll in ((1, 0.0, 0.0), (12, 0.55, 0.0), (22, 0.10, 0.0),
                             (28, 0.18, 0.0), (32, 0.05, 0.15), (36, 0.02, -0.05),
                             (40, 0.0, 0.0)):
            key_rot("Head", f, (nod, roll, 0))
        for f, sw in ((1, 0.2), (20, -0.2), (40, 0.2)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 24, extras)

    build_creature("creature54", bones, idle, walk, cam=1.0)


# =============================================================================
# Créature 55 — Loup gris : hurle à la lune en Idle, trot infatigable.
# =============================================================================
def loup():
    fresh_scene()
    grey = material("Loup55Grey", (0.44, 0.44, 0.46))
    grey_l = material("Loup55GreyL", (0.72, 0.72, 0.70))
    dark = material("Loup55Dark", (0.08, 0.08, 0.09))

    sphere("Body", grey, (0, 0.10, 0.92), (0.44, 0.85, 0.44))
    sphere("Body", grey_l, (0, -0.30, 0.78), (0.34, 0.45, 0.34))  # poitrail
    sphere("Head", grey, (0, -0.85, 1.20), (0.30, 0.32, 0.28))
    sphere("Head", grey_l, (0, -1.12, 1.10), (0.14, 0.18, 0.12))  # museau
    sphere("Head", dark, (0, -1.28, 1.12), (0.05, 0.045, 0.045))  # truffe
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.13, -1.00, 1.30), (0.045, 0.035, 0.05))
        cone("Head", grey, (sx * 0.17, -0.72, 1.48), (0.07, 0.05, 0.13),
             rotation=(math.radians(-8), 0, math.radians(sx * 10)))  # oreille pointue
    for bone, x, y in (("LegFL", -0.26, -0.48), ("LegFR", 0.26, -0.48),
                       ("LegBL", -0.26, 0.55), ("LegBR", 0.26, 0.55)):
        cylinder(bone, grey, (x, y, 0.38), (0.10, 0.10, 0.74))
        sphere(bone, grey_l, (x, y - 0.05, 0.08), (0.11, 0.14, 0.07))
    # Queue touffue mi-basse.
    for y, z, r in ((0.82, 0.88, 0.11), (1.05, 0.78, 0.10), (1.24, 0.66, 0.09)):
        sphere("Tail", grey, (0, y, z), (r, r * 1.3, r))
    sphere("Tail", grey_l, (0, 1.38, 0.58), (0.08, 0.10, 0.08))

    bones = quad_bones(0.26, -0.48, 0.55, 0.74, ((0, 0.45, 0.90), (0, -0.50, 0.98)), {
        "Head": ("Body", (0, -0.65, 1.08), (0, -1.30, 1.18)),
        "Tail": ("Body", (0, 0.75, 0.90), (0, 1.42, 0.55)),
    })

    def idle(key_rot, key_loc):
        # Hurlement : le museau pointe au ciel et TIENT la note (frames 12-26),
        # la queue se dresse — le cri du clan sur la banquise.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, 0.0), (8, -0.75), (12, -0.85), (26, -0.85), (32, 0.0),
                      (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, up in ((1, 0.1), (12, -0.35), (26, -0.35), (34, 0.1), (40, 0.1)):
            key_rot("Tail", f, (up, 0, 0))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.18), (13, -0.18), (24, 0.18)):
                kr("Tail", f, (0.05, 0, sw))
        quad_walk_keys(key_rot, key_loc, 26, extras)

    build_creature("creature55", bones, idle, walk, cam=0.9)


# =============================================================================
# Créature 56 — Harfang des neiges : tête pivotante, yeux d'or.
# =============================================================================
def harfang():
    fresh_scene()
    white = material("Harfang56White", (0.94, 0.93, 0.90))
    fleck = material("Harfang56Fleck", (0.55, 0.52, 0.48))
    gold = material("Harfang56Gold", (0.92, 0.75, 0.15))
    dark = material("Harfang56Dark", (0.06, 0.06, 0.06))

    sphere("Body", white, (0, 0.05, 0.70), (0.40, 0.42, 0.55))
    for sx, y, z in ((-0.18, 0.15, 0.95), (0.22, 0.05, 0.85), (-0.10, 0.25, 0.70),
                     (0.15, 0.20, 0.60)):  # mouchetures
        sphere("Body", fleck, (sx, y, z), (0.06, 0.05, 0.04))
    for bone, sx in (("WingL", -1), ("WingR", 1)):  # ailes pliées
        sphere(bone, white, (sx * 0.36, 0.12, 0.75), (0.10, 0.30, 0.42))
        sphere(bone, fleck, (sx * 0.42, 0.25, 0.72), (0.05, 0.10, 0.12))
    sphere("Head", white, (0, -0.02, 1.32), (0.30, 0.30, 0.27))
    for sx in (-1, 1):
        sphere("Head", gold, (sx * 0.12, -0.26, 1.38), (0.07, 0.04, 0.08))  # œil d'or
        sphere("Head", dark, (sx * 0.12, -0.29, 1.38), (0.035, 0.02, 0.04))
    cone("Head", dark, (0, -0.30, 1.26), (0.035, 0.035, 0.07),
         rotation=(math.radians(115), 0, 0))  # bec crochu
    for bone, sx in (("LegL", -1), ("LegR", 1)):  # pattes emplumées
        cylinder(bone, white, (sx * 0.14, 0.08, 0.18), (0.07, 0.07, 0.30))
        sphere(bone, fleck, (sx * 0.14, -0.02, 0.05), (0.09, 0.12, 0.035))

    bones = {
        "Body": ("Root", (0, 0.30, 0.70), (0, -0.20, 0.80)),
        "Head": ("Body", (0, 0.0, 1.10), (0, -0.10, 1.50)),
        "WingL": ("Body", (-0.30, 0.10, 0.95), (-0.44, 0.20, 0.40)),
        "WingR": ("Body", (0.30, 0.10, 0.95), (0.44, 0.20, 0.40)),
        "LegL": ("Body", (-0.14, 0.08, 0.35), (-0.14, 0.08, 0.02)),
        "LegR": ("Body", (0.14, 0.08, 0.35), (0.14, 0.08, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Vigie : la tête pivote très loin d'un côté, se fige, puis balaie de
        # l'autre — le radar silencieux du harfang.
        for f in (1, 40):
            for b in ("LegL", "LegR", "WingL", "WingR"):
                key_rot(b, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, yaw in ((1, 0.0), (8, 1.1), (16, 1.1), (22, -1.1), (32, -1.1),
                       (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        # Trottine en se dandinant, ailes entrouvertes pour l'équilibre,
        # la tête reste stable (les rapaces stabilisent le regard).
        s = math.radians(20)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, roll in ((1, 0.10), (13, -0.10), (24, 0.10)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.15), (13, 0.22), (24, 0.15)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, c in ((1, -0.08), (13, 0.08), (24, -0.08)):
            key_rot("Head", f, (0, c, 0))

    build_creature("creature56", bones, idle, walk, cam=0.8)


# =============================================================================
# Créature 57 — Morse : défenses d'ivoire, se hisse sur ses nageoires.
# =============================================================================
def morse():
    fresh_scene()
    hide = material("Morse57Hide", (0.48, 0.34, 0.26))
    hide_d = material("Morse57HideD", (0.36, 0.24, 0.18))
    ivory = material("Morse57Ivory", (0.93, 0.90, 0.80))
    dark = material("Morse57Dark", (0.07, 0.06, 0.05))

    # Corps fuselé qui s'affine vers l'arrière, posé au sol.
    sphere("Body", hide, (0, 0.10, 0.60), (0.62, 0.90, 0.55))
    sphere("Body", hide_d, (0, 0.85, 0.45), (0.40, 0.45, 0.32))
    sphere("Tail", hide, (0, 1.35, 0.35), (0.26, 0.30, 0.20))
    for sx in (-1, 1):  # nageoires caudales en éventail
        sphere("Tail", hide_d, (sx * 0.18, 1.58, 0.30), (0.12, 0.20, 0.06))
    sphere("Head", hide, (0, -0.72, 0.85), (0.34, 0.32, 0.30))
    sphere("Head", hide_d, (0, -0.95, 0.72), (0.24, 0.16, 0.16))  # mufle épais
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.15, -0.90, 0.98), (0.04, 0.035, 0.045))
        # Longues défenses vers le bas.
        cone("Head", ivory, (sx * 0.10, -1.02, 0.48), (0.035, 0.035, 0.22),
             rotation=(math.radians(178), 0, math.radians(sx * 4)))
        # Moustaches : rangée de points clairs.
        for dx in (0.06, 0.14, 0.22):
            sphere("Head", ivory, (sx * dx, -1.06, 0.70), (0.022, 0.02, 0.02))
        # Nageoires avant sur lesquelles il se hisse.
        sphere(f"Flip{'L' if sx < 0 else 'R'}", hide_d,
               (sx * 0.55, -0.35, 0.25), (0.16, 0.30, 0.08))

    bones = {
        "Body": ("Root", (0, 0.50, 0.60), (0, -0.40, 0.70)),
        "Head": ("Body", (0, -0.50, 0.75), (0, -1.10, 0.75)),
        "Tail": ("Body", (0, 1.10, 0.45), (0, 1.70, 0.30)),
        "FlipL": ("Body", (-0.45, -0.35, 0.40), (-0.70, -0.35, 0.05)),
        "FlipR": ("Body", (0.45, -0.35, 0.40), (0.70, -0.35, 0.05)),
    }

    def idle(key_rot, key_loc):
        # Se hisse : le torse se redresse sur les nageoires, la tête se lève
        # défenses en avant, puis tout retombe lourdement.
        for f, up in ((1, 0.0), (12, -0.22), (24, -0.22), (34, 0.0), (40, 0.0)):
            key_rot("Body", f, (up, 0, 0))
        for f, up in ((1, 0.0), (12, -0.25), (24, -0.25), (34, 0.0), (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, a in ((1, 0.0), (12, 0.30), (24, 0.30), (34, 0.0), (40, 0.0)):
            key_rot("FlipL", f, (a, 0, 0))
            key_rot("FlipR", f, (a, 0, 0))
        for f, sw in ((1, 0.15), (20, -0.15), (40, 0.15)):
            key_rot("Tail", f, (0, 0, sw))
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))

    def walk(key_rot, key_loc):
        # Reptation de plage : les nageoires avant rament ensemble, le corps
        # se soulève et retombe par vagues, la queue pousse.
        for f, a in ((1, 0.45), (9, -0.25), (17, 0.15), (24, 0.45)):
            key_rot("FlipL", f, (a, 0, 0))
            key_rot("FlipR", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (9, 0.10), (17, 0.0), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, pitch in ((1, 0.06), (9, -0.08), (17, 0.03), (24, 0.06)):
            key_rot("Body", f, (pitch, 0, 0))
        for f, sw in ((1, 0.25), (13, -0.25), (24, 0.25)):
            key_rot("Tail", f, (0, 0, sw))
        for f, nod in ((1, -0.05), (13, 0.08), (24, -0.05)):
            key_rot("Head", f, (nod, 0, 0))

    build_creature("creature57", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 58 — Phoque : moucheté, applaudit de ses nageoires.
# =============================================================================
def phoque():
    fresh_scene()
    grey = material("Phoque58Grey", (0.62, 0.62, 0.64))
    grey_d = material("Phoque58GreyD", (0.42, 0.42, 0.45))
    dark = material("Phoque58Dark", (0.07, 0.07, 0.08))

    sphere("Body", grey, (0, 0.05, 0.50), (0.44, 0.72, 0.42))
    for sx, y, z in ((-0.20, -0.10, 0.72), (0.24, 0.15, 0.65), (-0.12, 0.35, 0.58),
                     (0.10, -0.30, 0.68)):  # mouchetures
        sphere("Body", grey_d, (sx, y, z), (0.05, 0.06, 0.035))
    sphere("Tail", grey, (0, 0.70, 0.40), (0.24, 0.30, 0.22))
    for sx in (-1, 1):
        sphere("Tail", grey_d, (sx * 0.16, 0.98, 0.35), (0.10, 0.18, 0.05))
        sphere(f"Flip{'L' if sx < 0 else 'R'}", grey_d,
               (sx * 0.42, -0.25, 0.20), (0.12, 0.22, 0.06))
    sphere("Head", grey, (0, -0.60, 0.70), (0.26, 0.26, 0.24))
    sphere("Head", grey, (0, -0.80, 0.62), (0.13, 0.12, 0.10))  # museau rond
    sphere("Head", dark, (0, -0.90, 0.66), (0.045, 0.04, 0.04))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.11, -0.78, 0.80), (0.045, 0.04, 0.05))

    bones = {
        "Body": ("Root", (0, 0.35, 0.50), (0, -0.30, 0.58)),
        "Head": ("Body", (0, -0.40, 0.62), (0, -0.90, 0.68)),
        "Tail": ("Body", (0, 0.55, 0.45), (0, 1.05, 0.32)),
        "FlipL": ("Body", (-0.34, -0.25, 0.32), (-0.55, -0.25, 0.05)),
        "FlipR": ("Body", (0.34, -0.25, 0.32), (0.55, -0.25, 0.05)),
    }

    def idle(key_rot, key_loc):
        # Applaudit : cambré museau au ciel, les nageoires avant claquent
        # l'une vers l'autre trois fois — le numéro du phoque heureux.
        for f, up in ((1, -0.10), (10, -0.30), (30, -0.30), (40, -0.10)):
            key_rot("Body", f, (up, 0, 0))
        for f, up in ((1, 0.0), (10, -0.30), (30, -0.30), (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, a in ((1, 0.0), (10, -0.5), (14, -0.9), (18, -0.5), (22, -0.9),
                     (26, -0.5), (30, -0.9), (36, 0.0), (40, 0.0)):
            key_rot("FlipL", f, (0, 0, a))
            key_rot("FlipR", f, (0, 0, -a))
        for f, sw in ((1, 0.2), (20, -0.2), (40, 0.2)):
            key_rot("Tail", f, (0, 0, sw))
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))

    def walk(key_rot, key_loc):
        # Ondulation : le corps avance par vagues (tangage), les nageoires
        # rament en alternance, la queue bat la mesure.
        for f, pitch in ((1, 0.10), (7, -0.10), (13, 0.10), (19, -0.10), (24, 0.10)):
            key_rot("Body", f, (pitch, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.06), (13, 0.0), (19, 0.06), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.35), (13, -0.35), (24, 0.35)):
            key_rot("FlipL", f, (a, 0, 0))
            key_rot("FlipR", f, (-a, 0, 0))
        for f, sw in ((1, 0.3), (13, -0.3), (24, 0.3)):
            key_rot("Tail", f, (0, 0, sw))
        for f, nod in ((1, -0.06), (13, 0.06), (24, -0.06)):
            key_rot("Head", f, (nod, 0, 0))

    build_creature("creature58", bones, idle, walk, cam=0.75)


# =============================================================================
# Créature 59 — Bouquetin : cornes en arc de cercle, toise du haut du pic.
# =============================================================================
def bouquetin():
    fresh_scene()
    tan = material("Bouquetin59Tan", (0.58, 0.44, 0.28))
    tan_d = material("Bouquetin59TanD", (0.40, 0.30, 0.18))
    horn = material("Bouquetin59Horn", (0.52, 0.44, 0.32))
    dark = material("Bouquetin59Dark", (0.08, 0.07, 0.06))

    sphere("Body", tan, (0, 0.10, 0.95), (0.46, 0.78, 0.46))
    sphere("Body", tan_d, (0, 0.55, 1.05), (0.30, 0.30, 0.28))  # arrière-train
    sphere("Head", tan, (0, -0.80, 1.25), (0.26, 0.30, 0.25))
    sphere("Head", tan_d, (0, -1.05, 1.15), (0.12, 0.14, 0.10))  # museau
    sphere("Head", dark, (0, -1.18, 1.16), (0.045, 0.04, 0.04))
    sphere("Head", tan_d, (0, -0.95, 0.98), (0.06, 0.05, 0.10))  # barbiche
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.12, -0.95, 1.36), (0.04, 0.035, 0.045))
        sphere("Head", tan, (sx * 0.22, -0.68, 1.42), (0.08, 0.05, 0.09))  # oreille
        # Cornes : grand arc de cercle vers l'arrière, chaîne DENSE de sphères
        # (les sphères adjacentes doivent se chevaucher, sinon perles).
        for k in range(12):
            t = k / 11.0
            a = t * 2.1  # angle le long de l'arc
            hy = -0.70 + 0.40 * math.sin(a)
            hz = 1.46 + 0.34 * (1 - math.cos(a))
            hr = 0.062 - t * 0.025
            sphere("Head", horn, (sx * (0.10 + t * 0.10), hy, hz), (hr, hr, hr))
    for bone, x, y in (("LegFL", -0.26, -0.45), ("LegFR", 0.26, -0.45),
                       ("LegBL", -0.26, 0.55), ("LegBR", 0.26, 0.55)):
        cylinder(bone, tan, (x, y, 0.38), (0.10, 0.10, 0.72))
        cylinder(bone, dark, (x, y, 0.06), (0.11, 0.11, 0.10))  # sabot sûr
    sphere("Tail", tan_d, (0, 0.85, 1.05), (0.08, 0.08, 0.09))

    bones = quad_bones(0.26, -0.45, 0.55, 0.72, ((0, 0.45, 0.92), (0, -0.45, 1.00)), {
        "Head": ("Body", (0, -0.60, 1.10), (0, -1.20, 1.25)),
        "Tail": ("Body", (0, 0.78, 1.05), (0, 1.00, 1.05)),
    })

    def idle(key_rot, key_loc):
        # Toise la vallée : port de tête altier, coup de menton par moments,
        # parfaitement immobile sur ses sabots — l'aplomb du sommet.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
            key_loc("Body", f, (0, 0, 0))
        for f, up in ((1, -0.10), (14, -0.10), (18, -0.30), (24, -0.10),
                      (40, -0.10)):
            key_rot("Head", f, (up, 0, 0))
        for f, sw in ((1, 0.15), (20, -0.15), (40, 0.15)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.12), (13, -0.12), (24, 0.12)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 22, extras)

    build_creature("creature59", bones, idle, walk, cam=0.9)


# =============================================================================
# Créature 60 — Lièvre arctique : assis en vigie, déplacement par bonds.
# =============================================================================
def lievre():
    fresh_scene()
    white = material("Lievre60White", (0.93, 0.92, 0.88))
    grey = material("Lievre60Grey", (0.70, 0.70, 0.68))
    dark = material("Lievre60Dark", (0.07, 0.07, 0.08))

    sphere("Body", white, (0, 0.08, 0.48), (0.32, 0.45, 0.38))
    sphere("Head", white, (0, -0.30, 0.80), (0.22, 0.24, 0.21))
    sphere("Head", grey, (0, -0.50, 0.74), (0.09, 0.08, 0.07))  # museau
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -0.46, 0.86), (0.04, 0.03, 0.045))
        # Longues oreilles à bout sombre.
        sphere("Head", white, (sx * 0.10, -0.18, 1.14), (0.055, 0.04, 0.20))
        sphere("Head", dark, (sx * 0.10, -0.16, 1.32), (0.04, 0.03, 0.05))
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        # Grandes pattes arrière repliées + petites pattes avant.
        sphere(bone, white, (sx * 0.26, 0.28, 0.28), (0.14, 0.24, 0.18))
        sphere(bone, grey, (sx * 0.24, 0.05, 0.08), (0.09, 0.18, 0.06))
        cylinder(bone, white, (sx * 0.13, -0.22, 0.18), (0.055, 0.055, 0.30))
    sphere("Tail", grey, (0, 0.52, 0.55), (0.09, 0.08, 0.09))

    bones = {
        "Body": ("Root", (0, 0.28, 0.48), (0, -0.20, 0.55)),
        "Head": ("Body", (0, -0.15, 0.68), (0, -0.42, 1.00)),
        "LegL": ("Body", (-0.22, 0.15, 0.35), (-0.22, 0.15, 0.02)),
        "LegR": ("Body", (0.22, 0.15, 0.35), (0.22, 0.15, 0.02)),
        "Tail": ("Body", (0, 0.45, 0.55), (0, 0.65, 0.58)),
    }

    def idle(key_rot, key_loc):
        # Vigie : se dresse sur son séant, les oreilles balaient en radar,
        # deux frémissements de museau, puis retombe.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, up, dz in ((1, 0.0, 0.0), (10, -0.35, 0.10), (28, -0.35, 0.10),
                          (36, 0.0, 0.0), (40, 0.0, 0.0)):
            key_rot("Body", f, (up, 0, 0))
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (12, 0.35), (20, -0.35), (28, 0.0), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))
        for f, sw in ((1, 0.15), (20, -0.15), (40, 0.15)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Bonds : accroupi, détente (le corps décolle, pattes tendues),
        # réception — deux bonds par cycle de 24 frames.
        for f, dz in ((1, -0.04), (4, 0.16), (8, 0.02), (12, -0.04), (16, 0.16),
                      (20, 0.02), (24, -0.04)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.35), (4, -0.45), (8, 0.15), (12, 0.35), (16, -0.45),
                     (20, 0.15), (24, 0.35)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (a, 0, 0))
        for f, pitch in ((1, 0.10), (4, -0.12), (8, 0.04), (12, 0.10), (16, -0.12),
                         (20, 0.04), (24, 0.10)):
            key_rot("Body", f, (pitch, 0, 0))
        for f, nod in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Head", f, (nod, 0, 0))
        for f in (1, 24):
            key_rot("Tail", f, (0, 0, 0))

    build_creature("creature60", bones, idle, walk, cam=0.65)


# =============================================================================
# Créature 61 — Yack : toison-jupe hirsute, secoue sa crinière.
# =============================================================================
def yack():
    fresh_scene()
    shag = material("Yack61Shag", (0.26, 0.18, 0.14))
    shag_d = material("Yack61ShagD", (0.16, 0.11, 0.08))
    muzzle = material("Yack61Muzzle", (0.72, 0.66, 0.58))
    horn = material("Yack61Horn", (0.75, 0.68, 0.55))
    dark = material("Yack61Dark", (0.06, 0.05, 0.05))

    sphere("Body", shag, (0, 0.10, 1.05), (0.72, 1.00, 0.62))
    # Jupe de toison : rangée de mèches qui pendent sous les flancs.
    for sx in (-1, 1):
        for y in (-0.45, -0.10, 0.25, 0.60):
            sphere("Body", shag_d, (sx * 0.62, y, 0.62), (0.14, 0.16, 0.26))
    sphere("Body", shag_d, (0, 0.35, 1.48), (0.34, 0.40, 0.20))  # bosse d'épaules
    sphere("Head", shag, (0, -0.95, 1.10), (0.36, 0.38, 0.32))
    sphere("Head", muzzle, (0, -1.25, 0.98), (0.19, 0.16, 0.14))  # mufle clair
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.17, -1.18, 1.22), (0.045, 0.04, 0.05))
        sphere("Head", shag_d, (sx * 0.34, -0.85, 1.28), (0.10, 0.07, 0.09))
        # Cornes : chaîne dense de sphères dehors puis pointe cône vers le haut
        # (chevauchement obligatoire, sinon perles flottantes).
        for k in range(6):
            t = k / 5.0
            sphere("Head", horn,
                   (sx * (0.40 + 0.32 * t), -0.90 + 0.10 * t, 1.34 + 0.22 * t),
                   (0.062 - t * 0.012,) * 3)
        cone("Head", horn, (sx * 0.76, -0.74, 1.66), (0.04, 0.04, 0.12),
             rotation=(math.radians(-30), 0, math.radians(sx * 12)))
    for bone, x, y in (("LegFL", -0.42, -0.52), ("LegFR", 0.42, -0.52),
                       ("LegBL", -0.42, 0.66), ("LegBR", 0.42, 0.66)):
        cylinder(bone, shag_d, (x, y, 0.42), (0.16, 0.16, 0.78))
        cylinder(bone, dark, (x, y, 0.08), (0.17, 0.17, 0.12))
    # Queue-plumeau de cheval.
    for y, z, r in ((1.15, 1.10, 0.09), (1.32, 0.92, 0.11), (1.45, 0.72, 0.12)):
        sphere("Tail", shag_d, (0, y, z), (r, r, r * 1.3))

    bones = quad_bones(0.42, -0.52, 0.66, 0.80, ((0, 0.55, 1.00), (0, -0.55, 1.10)), {
        "Head": ("Body", (0, -0.72, 1.05), (0, -1.35, 1.00)),
        "Tail": ("Body", (0, 1.05, 1.10), (0, 1.50, 0.65)),
    })

    def idle(key_rot, key_loc):
        # Secoue sa toison : la tête roule d'une épaule à l'autre puis
        # s'ébroue (petites secousses rapides), la queue-plumeau fouette.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, roll in ((1, 0.0), (10, 0.30), (18, -0.30), (23, 0.15), (27, -0.15),
                        (31, 0.0), (40, 0.0)):
            key_rot("Head", f, (0.08, roll, 0))
        for f, sw in ((1, 0.4), (14, -0.4), (27, 0.4), (40, 0.4)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, roll in ((1, 0.06), (13, -0.06), (24, 0.06)):
                kr("Body", f, (0, roll, 0))
            for f, nod in ((1, 0.06), (13, -0.04), (24, 0.06)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.25), (13, -0.25), (24, 0.25)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 16, extras)

    build_creature("creature61", bones, idle, walk, cam=1.05)


ours_polaire()
manchot()
renne()
loup()
harfang()
morse()
phoque()
bouquetin()
lievre()
yack()
print("PACK DONE")
