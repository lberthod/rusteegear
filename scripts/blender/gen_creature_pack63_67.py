"""Génère assets/models/creature63.glb … creature67.glb : 5 animaux de savane.

Pack « savane africaine » — lion, zèbre, guépard, rhinocéros, hyène tachetée.
Complète la ménagerie savane déjà présente (girafe/autruche/crocodile/gorille
du pack22-26) avec le trio prédateur/proie manquant. Chacun a une animation
signature (rugissement figé du lion, oreilles nerveuses du zèbre, guet bas du
guépard, broutage massif du rhinocéros, ricanement tacheté de la hyène).
Conventions et optimisations partagées : voir `creature_kit.py` (face -Y,
un os par pièce à poids 1.0, clips Idle 40 fr / Walk 24 fr bouclables et
couvrants, LOD automatique des primitives, aucun vertex sous z=0 + marge,
QA par `check_creatures.py` après génération).

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack63_67.py
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
# Créature 63 — Lion : crinière épaisse, rugit tête haute en Idle.
# =============================================================================
def lion():
    fresh_scene()
    tawny = material("Lion63Tawny", (0.72, 0.55, 0.28))
    cream = material("Lion63Cream", (0.86, 0.76, 0.56))
    mane = material("Lion63Mane", (0.36, 0.21, 0.10))
    dark = material("Lion63Dark", (0.08, 0.07, 0.06))
    white = material("Lion63White", (0.97, 0.96, 0.92))
    amber = material("Lion63Amber", (0.62, 0.42, 0.10))

    sphere("Body", tawny, (0, 0.10, 0.98), (0.48, 0.85, 0.48))
    sphere("Body", cream, (0, -0.25, 0.82), (0.34, 0.34, 0.30))  # poitrail clair
    sphere("Head", tawny, (0, -0.90, 1.28), (0.32, 0.34, 0.30))
    sphere("Head", cream, (0, -1.16, 1.18), (0.17, 0.18, 0.14))  # museau
    sphere("Head", dark, (0, -1.32, 1.16), (0.055, 0.05, 0.05))  # truffe
    for sx in (-1, 1):
        # Œil à 3 pièces (sclère + iris ambré + pupille + micro-reflet) —
        # regard félin plus vif qu'un simple point sombre.
        sphere("Head", white, (sx * 0.13, -1.06, 1.34), (0.052, 0.044, 0.052))
        sphere("Head", amber, (sx * 0.148, -1.075, 1.336), (0.03, 0.024, 0.03))
        sphere("Head", dark, (sx * 0.155, -1.082, 1.334), (0.015, 0.012, 0.015))
        sphere("Head", white, (sx * 0.16, -1.088, 1.344), (0.009, 0.007, 0.009))
        # Oreille en 2 pièces (pavillon fourrure + creux interne sombre).
        sphere("Head", tawny, (sx * 0.24, -0.76, 1.52), (0.09, 0.05, 0.09))
        sphere("Head", dark, (sx * 0.24, -0.775, 1.52), (0.06, 0.03, 0.06))
        # Crinière : collier DENSE de sphères qui encercle tête et cou (les
        # sphères adjacentes DOIVENT se chevaucher, sinon un trou s'ouvre sur
        # le corps vu depuis la caméra en plongée — piège déjà documenté pour
        # les cornes/bois des packs 54/59/61).
        for k in range(8):
            a = math.radians(-165 + k * 40)
            my = -0.66 + 0.34 * math.sin(a)
            mz = 1.20 + 0.36 * math.cos(a)
            sphere("Head", mane, (sx * 0.15, my, mz), (0.15, 0.15, 0.15))
    # Gueule + crocs, bien visibles dans l'Idle (rugissement, tête levée).
    sphere("Head", dark, (0, -1.20, 1.06), (0.13, 0.05, 0.03))  # fente de bouche
    for sx in (-1, 1):
        cone("Head", white, (sx * 0.09, -1.16, 1.00), (0.022, 0.022, 0.07),
             rotation=(math.radians(170), 0, 0))  # croc supérieur
    for bone, x, y in (("LegFL", -0.28, -0.48), ("LegFR", 0.28, -0.48),
                       ("LegBL", -0.28, 0.58), ("LegBR", 0.28, 0.58)):
        cylinder(bone, tawny, (x, y, 0.40), (0.12, 0.12, 0.78))
        sphere(bone, cream, (x, y - 0.06, 0.09), (0.13, 0.16, 0.08))
    # Queue avec touffe sombre en pointe.
    for y, z, r in ((0.90, 0.90, 0.11), (1.20, 0.72, 0.10), (1.45, 0.52, 0.09)):
        sphere("Tail", tawny, (0, y, z), (r, r * 1.2, r))
    sphere("Tail", mane, (0, 1.62, 0.42), (0.09, 0.10, 0.09))

    bones = quad_bones(0.28, -0.48, 0.58, 0.80, ((0, 0.55, 0.95), (0, -0.55, 1.05)), {
        "Head": ("Body", (0, -0.70, 1.15), (0, -1.35, 1.25)),
        "Tail": ("Body", (0, 0.85, 0.92), (0, 1.60, 0.42)),
    })

    def idle(key_rot, key_loc):
        # Rugissement : la tête se lève et TIENT la note (12-26), la crinière
        # (portée par la tête) suit, la queue fouette au sol.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, 0.0), (8, -0.55), (12, -0.65), (26, -0.65), (32, 0.0),
                      (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, sw in ((1, 0.25), (12, -0.45), (26, 0.45), (40, 0.25)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.04), (13, -0.04), (24, 0.04)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.15), (13, -0.15), (24, 0.15)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 22, extras)

    build_creature("creature63", bones, idle, walk, cam=1.0)


# =============================================================================
# Créature 64 — Zèbre : rayures nettes, oreilles en radar nerveux.
# =============================================================================
def zebre():
    fresh_scene()
    white = material("Zebre64White", (0.90, 0.88, 0.84))
    stripe = material("Zebre64Stripe", (0.10, 0.09, 0.10))
    dark_eye = material("Zebre64Eye", (0.05, 0.04, 0.04))

    sphere("Body", white, (0, 0.10, 0.92), (0.42, 0.78, 0.42))
    for k, y in enumerate((-0.35, -0.15, 0.05, 0.25, 0.45, 0.65)):
        w = 0.05 if k % 2 else 0.06
        sphere("Body", stripe, (0, y, 0.95 + 0.02 * (k % 2)), (0.44, w, 0.40))
    sphere("Head", white, (0, -0.80, 1.22), (0.24, 0.28, 0.22))
    sphere("Head", white, (0, -1.05, 1.14), (0.13, 0.16, 0.11))  # museau
    sphere("Head", stripe, (0, -1.20, 1.14), (0.06, 0.05, 0.05))  # naseau sombre
    for sx in (-1, 1):
        # Œil à 3 pièces (sclère + pupille sombre + micro-reflet) — la
        # rayure sombre qui traversait l'œil devient son contour naturel.
        sphere("Head", white, (sx * 0.12, -0.96, 1.32), (0.044, 0.038, 0.044))
        sphere("Head", dark_eye, (sx * 0.133, -0.974, 1.316), (0.024, 0.019, 0.024))
        sphere("Head", white, (sx * 0.14, -0.982, 1.324), (0.008, 0.006, 0.008))
        sphere("Head", white, (sx * 0.20, -0.66, 1.46), (0.075, 0.06, 0.10))  # oreille
        sphere("Head", stripe, (sx * 0.20, -0.675, 1.46), (0.05, 0.035, 0.07))  # creux
        for k, mz in enumerate((0.98, 1.14, 1.30)):  # rayures du museau
            sphere("Head", stripe, (sx * (0.08 + 0.02 * k), -0.95 + 0.05 * k, mz),
                   (0.035, 0.06, 0.035))
    # Bouche : fente sombre + petites dents plates herbivores.
    sphere("Head", stripe, (0, -1.14, 1.08), (0.08, 0.03, 0.02))
    for sx in (-1, 1):
        sphere("Head", white, (sx * 0.03, -1.16, 1.05), (0.018, 0.014, 0.012))
    # Crinière courte en piquants dressés.
    for k, y in enumerate((-0.55, -0.35, -0.15, 0.05)):
        cone("Body", stripe, (0, y, 1.30 - 0.02 * k), (0.05, 0.10, 0.10),
             rotation=(math.radians(-90), 0, 0))
    for bone, x, y in (("LegFL", -0.24, -0.45), ("LegFR", 0.24, -0.45),
                       ("LegBL", -0.24, 0.52), ("LegBR", 0.24, 0.52)):
        cylinder(bone, white, (x, y, 0.36), (0.095, 0.095, 0.72))
        for k, dz in enumerate((0.55, 0.30)):
            sphere(bone, stripe, (x, y, dz), (0.10, 0.10, 0.045))
        cylinder(bone, stripe, (x, y, 0.05), (0.10, 0.10, 0.08))  # sabot
    sphere("Tail", white, (0, 0.85, 0.98), (0.08, 0.08, 0.09))
    sphere("Tail", stripe, (0, 1.05, 0.86), (0.055, 0.09, 0.055))

    bones = quad_bones(0.24, -0.45, 0.52, 0.70, ((0, 0.45, 0.90), (0, -0.45, 0.98)), {
        "Head": ("Body", (0, -0.58, 1.10), (0, -1.10, 1.25)),
        "Tail": ("Body", (0, 0.78, 0.98), (0, 1.15, 0.80)),
    })

    def idle(key_rot, key_loc):
        # Nerveux : les oreilles balaient le radar vite, la tête guette,
        # la queue chasse les mouches par à-coups secs.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, 0.0), (6, 0.30), (12, -0.30), (18, 0.15), (24, 0.0),
                       (32, 0.20), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))
        for f, sw in ((1, 0.0), (5, 0.5), (9, 0.0), (22, 0.5), (26, 0.0), (40, 0.0)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.06), (13, -0.06), (24, 0.06)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.2), (13, -0.2), (24, 0.2)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 28, extras)

    build_creature("creature64", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 65 — Guépard : tacheté, larme faciale, guet accroupi.
# =============================================================================
def guepard():
    fresh_scene()
    tan = material("Guepard65Tan", (0.80, 0.63, 0.36))
    cream = material("Guepard65Cream", (0.92, 0.87, 0.74))
    spot = material("Guepard65Spot", (0.15, 0.12, 0.10))
    white = material("Guepard65White", (0.97, 0.96, 0.93))
    amber = material("Guepard65Amber", (0.68, 0.48, 0.14))

    sphere("Body", tan, (0, 0.10, 0.66), (0.34, 0.82, 0.32))
    sphere("Body", cream, (0, -0.30, 0.56), (0.24, 0.28, 0.20))  # poitrail clair
    for sx, y, z in ((-0.16, -0.20, 0.82), (0.19, 0.05, 0.75), (-0.12, 0.30, 0.68),
                     (0.14, 0.45, 0.78), (-0.20, 0.55, 0.70), (0.10, -0.05, 0.60)):
        sphere("Body", spot, (sx, y, z), (0.045, 0.045, 0.04))
    sphere("Head", tan, (0, -0.62, 0.86), (0.22, 0.28, 0.20))
    # Cou : un seul bloc massif qui recouvre largement tête ET corps (pas un
    # pont tangent) — à ce petit gabarit le maillage basse résolution
    # (LOD 16x10) facette assez fort pour qu'un chevauchement mathématique
    # correct laisse quand même un trou visible depuis la caméra en plongée ;
    # seule une interpénétration généreuse referme la selle de façon fiable.
    sphere("Head", tan, (0, -0.30, 0.90), (0.30, 0.42, 0.24))
    sphere("Head", cream, (0, -0.88, 0.80), (0.11, 0.12, 0.09))  # museau
    sphere("Head", spot, (0, -0.98, 0.82), (0.04, 0.035, 0.035))  # truffe
    sphere("Head", spot, (0, -1.00, 0.76), (0.05, 0.02, 0.012))  # fine bouche fermée
    for sx in (-1, 1):
        # Œil à 3 pièces (sclère + iris ambré + pupille + micro-reflet).
        sphere("Head", white, (sx * 0.10, -0.80, 0.94), (0.044, 0.036, 0.05))
        sphere("Head", amber, (sx * 0.113, -0.815, 0.936), (0.026, 0.02, 0.03))
        sphere("Head", spot, (sx * 0.12, -0.822, 0.934), (0.013, 0.01, 0.015))
        sphere("Head", white, (sx * 0.125, -0.828, 0.944), (0.007, 0.005, 0.007))
        sphere("Head", tan, (sx * 0.13, -0.58, 0.94), (0.06, 0.05, 0.09))  # oreille, enfoncée dans le crâne
        sphere("Head", spot, (sx * 0.13, -0.595, 0.94), (0.038, 0.03, 0.06))  # creux
        # Larme faciale : traînée sombre de l'œil à la commissure.
        sphere("Head", spot, (sx * 0.10, -0.90, 0.90), (0.018, 0.05, 0.018))
    for bone, x, y in (("LegFL", -0.17, -0.36), ("LegFR", 0.17, -0.36),
                       ("LegBL", -0.17, 0.42), ("LegBR", 0.17, 0.42)):
        cylinder(bone, tan, (x, y, 0.30), (0.065, 0.065, 0.60))
        for sy, sz in ((0.24, 0.22), (0.10, 0.10)):
            sphere(bone, spot, (x, y * (sy / 0.36) if y else 0, sz), (0.04, 0.04, 0.035))
    # Longue queue tachetée, extrémité annelée.
    for y, z, r in ((0.60, 0.70, 0.075), (0.90, 0.62, 0.07), (1.18, 0.52, 0.065),
                    (1.42, 0.42, 0.06)):
        sphere("Tail", tan, (0, y, z), (r, r * 1.4, r))
    sphere("Tail", spot, (0, 1.58, 0.35), (0.06, 0.08, 0.06))

    bones = quad_bones(0.17, -0.36, 0.42, 0.56, ((0, 0.38, 0.64), (0, -0.38, 0.72)), {
        "Head": ("Body", (0, -0.48, 0.80), (0, -0.95, 0.90)),
        "Tail": ("Body", (0, 0.68, 0.64), (0, 1.55, 0.30)),
    })

    def idle(key_rot, key_loc):
        # Guet accroupi : le corps se tasse, la tête balaie bas et lentement
        # la savane, seule la pointe de queue frémit — l'affût silencieux.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz, up in ((1, -0.05, 0.10), (20, -0.03, 0.06), (40, -0.05, 0.10)):
            key_loc("Body", f, (0, dz, 0))
            key_rot("Body", f, (up, 0, 0))
        for f, yaw in ((1, 0.0), (10, 0.28), (20, -0.28), (30, 0.10), (40, 0.0)):
            key_rot("Head", f, (0.10, 0, yaw))
        for f, sw in ((1, 0.0), (10, 0.6), (14, -0.6), (18, 0.6), (24, 0.0),
                      (40, 0.0)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.10), (13, -0.10), (24, 0.10)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 32, extras)

    build_creature("creature65", bones, idle, walk, cam=0.75)


# =============================================================================
# Créature 66 — Rhinocéros : corne massive, marche pesante, broute bas.
# =============================================================================
def rhinoceros():
    fresh_scene()
    hide = material("Rhino66Hide", (0.56, 0.55, 0.52))
    hide_d = material("Rhino66HideD", (0.42, 0.41, 0.38))
    horn = material("Rhino66Horn", (0.62, 0.58, 0.48))
    dark = material("Rhino66Dark", (0.07, 0.06, 0.06))
    white = material("Rhino66White", (0.94, 0.93, 0.90))

    sphere("Body", hide, (0, 0.10, 1.00), (0.66, 1.05, 0.62))
    sphere("Body", hide_d, (0, 0.60, 1.15), (0.42, 0.40, 0.34))  # croupe massive
    # Plis épais de la peau au niveau des épaules.
    for y in (-0.55, -0.30):
        sphere("Body", hide_d, (0, y, 1.30), (0.40, 0.10, 0.16))
    sphere("Head", hide, (0, -0.95, 1.15), (0.34, 0.36, 0.30))
    sphere("Head", hide_d, (0, -1.28, 1.05), (0.20, 0.18, 0.16))  # mufle épais
    for sx in (-1, 1):
        # Œil à 3 pièces — petit et enfoncé, plissé, comme le vrai animal.
        sphere("Head", white, (sx * 0.16, -1.10, 1.24), (0.04, 0.034, 0.04))
        sphere("Head", dark, (sx * 0.175, -1.113, 1.236), (0.022, 0.017, 0.022))
        sphere("Head", white, (sx * 0.18, -1.12, 1.244), (0.007, 0.006, 0.007))
        sphere("Head", hide_d, (sx * 0.26, -0.80, 1.42), (0.08, 0.05, 0.10))  # oreille
        sphere("Head", dark, (sx * 0.26, -0.815, 1.42), (0.05, 0.03, 0.07))  # creux
    # Corne principale + petite corne arrière, chaîne conique dense.
    for k in range(6):
        t = k / 5.0
        r = 0.13 - t * 0.10
        sphere("Head", horn, (0, -1.30 + 0.10 * t, 1.28 + 0.30 * t), (r, r, r))
    sphere("Head", horn, (0, -1.10, 1.52), (0.06, 0.06, 0.10))  # petite corne
    sphere("Head", dark, (0, -1.32, 1.00), (0.13, 0.05, 0.035))  # large bouche carrée
    for bone, x, y in (("LegFL", -0.40, -0.52), ("LegFR", 0.40, -0.52),
                       ("LegBL", -0.40, 0.62), ("LegBR", 0.40, 0.62)):
        cylinder(bone, hide, (x, y, 0.42), (0.19, 0.19, 0.80))
        cylinder(bone, dark, (x, y, 0.06), (0.20, 0.20, 0.10))  # sabot large
    sphere("Tail", hide_d, (0, 1.15, 1.00), (0.07, 0.07, 0.09))

    bones = quad_bones(0.40, -0.52, 0.62, 0.84, ((0, 0.55, 0.95), (0, -0.55, 1.05)), {
        "Head": ("Body", (0, -0.75, 1.10), (0, -1.35, 1.15)),
        "Tail": ("Body", (0, 1.08, 1.00), (0, 1.25, 0.95)),
    })

    def idle(key_rot, key_loc):
        # Broute bas : la corne balaie le sol de gauche à droite comme pour
        # arracher les buissons, la masse retombe lourdement entre deux passes.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, down, yaw in ((1, 0.20, 0.0), (10, 0.35, 0.20), (20, 0.35, -0.20),
                             (30, 0.20, 0.0), (40, 0.20, 0.0)):
            key_rot("Head", f, (down, 0, yaw))
        for f, sw in ((1, 0.08), (20, -0.08), (40, 0.08)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        def extras(kr):
            for f, nod in ((1, 0.03), (13, -0.03), (24, 0.03)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.06), (13, -0.06), (24, 0.06)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 14, extras)

    build_creature("creature66", bones, idle, walk, cam=1.15)


# =============================================================================
# Créature 67 — Hyène tachetée : dos incliné, ricanement moqueur.
# =============================================================================
def hyene():
    fresh_scene()
    tan = material("Hyene67Tan", (0.62, 0.50, 0.30))
    cream = material("Hyene67Cream", (0.80, 0.72, 0.54))
    spot = material("Hyene67Spot", (0.20, 0.15, 0.10))
    dark = material("Hyene67Dark", (0.08, 0.07, 0.07))
    white = material("Hyene67White", (0.96, 0.95, 0.92))
    amber = material("Hyene67Amber", (0.55, 0.40, 0.14))

    # Corps incliné : épaules hautes (avant) et croupe basse (arrière), la
    # silhouette caractéristique en pente de la hyène.
    sphere("Body", tan, (0, -0.20, 1.00), (0.38, 0.50, 0.38))  # épaules hautes
    sphere("Body", tan, (0, 0.35, 0.78), (0.34, 0.46, 0.30))  # croupe basse
    # Pont entre les deux bosses : sans lui, la selle entre épaules et croupe
    # ouvre un trou vu depuis la caméra en plongée (piège chevauchement).
    sphere("Body", tan, (0, 0.10, 1.02), (0.38, 0.42, 0.40))
    for sx, y, z in ((-0.18, -0.30, 1.05), (0.20, 0.05, 0.95), (-0.14, 0.30, 0.85),
                     (0.16, 0.50, 0.75), (-0.10, -0.05, 0.80)):
        sphere("Body", spot, (sx, y, z), (0.05, 0.05, 0.04))
    sphere("Head", tan, (0, -0.72, 1.10), (0.24, 0.26, 0.22))
    sphere("Head", cream, (0, -0.96, 1.02), (0.12, 0.13, 0.10))  # museau puissant
    sphere("Head", dark, (0, -1.10, 1.03), (0.045, 0.04, 0.04))  # truffe
    # Gueule grande ouverte + dents pointues, le ricanement caractéristique.
    sphere("Head", dark, (0, -1.08, 0.90), (0.13, 0.06, 0.04))  # fente de bouche
    for sx in (-1, 1):
        cone("Head", white, (sx * 0.075, -1.03, 0.85), (0.02, 0.02, 0.06),
             rotation=(math.radians(170), 0, 0))  # croc supérieur
        cone("Head", white, (sx * 0.06, -1.05, 0.83), (0.016, 0.016, 0.045),
             rotation=(math.radians(-8), 0, 0))  # croc inférieur
    for sx in (-1, 1):
        # Œil à 3 pièces (sclère + iris ambré + pupille + micro-reflet).
        sphere("Head", white, (sx * 0.12, -0.86, 1.18), (0.046, 0.038, 0.052))
        sphere("Head", amber, (sx * 0.133, -0.875, 1.176), (0.026, 0.02, 0.03))
        sphere("Head", dark, (sx * 0.14, -0.882, 1.174), (0.013, 0.01, 0.015))
        sphere("Head", white, (sx * 0.145, -0.888, 1.184), (0.007, 0.005, 0.007))
        sphere("Head", tan, (sx * 0.18, -0.50, 1.22), (0.09, 0.06, 0.13))  # grande oreille ronde, enfoncée dans le crâne
        sphere("Head", dark, (sx * 0.18, -0.515, 1.22), (0.06, 0.036, 0.09))  # creux
    for bone, x, y, top in (("LegFL", -0.22, -0.38, 0.72), ("LegFR", 0.22, -0.38, 0.72),
                            ("LegBL", -0.20, 0.55, 0.48), ("LegBR", 0.20, 0.55, 0.48)):
        cylinder(bone, tan, (x, y, top * 0.55), (0.09, 0.09, top))
        sphere(bone, spot, (x, y, 0.06), (0.09, 0.11, 0.05))
    # Crinière hérissée le long de l'échine, du cou à la croupe.
    for k, y in enumerate((-0.55, -0.30, -0.05, 0.20, 0.45)):
        z = 1.38 - 0.10 * k
        cone("Body", spot, (0, y, z), (0.04, 0.09, 0.09),
             rotation=(math.radians(-90), 0, 0))
    sphere("Tail", tan, (0, 0.85, 0.80), (0.08, 0.08, 0.09))
    sphere("Tail", spot, (0, 1.02, 0.68), (0.06, 0.09, 0.06))

    bones = {
        "Body": ("Root", (0, 0.10, 0.85), (0, -0.30, 0.98)),
        "Head": ("Body", (0, -0.45, 1.05), (0, -1.05, 1.15)),
        "LegFL": ("Body", (-0.22, -0.38, 0.72), (-0.22, -0.38, 0.02)),
        "LegFR": ("Body", (0.22, -0.38, 0.72), (0.22, -0.38, 0.02)),
        "LegBL": ("Body", (-0.20, 0.55, 0.48), (-0.20, 0.55, 0.02)),
        "LegBR": ("Body", (0.20, 0.55, 0.48), (0.20, 0.55, 0.02)),
        "Tail": ("Body", (0, 0.75, 0.80), (0, 1.10, 0.62)),
    }

    def idle(key_rot, key_loc):
        # Ricanement : la tête part en arrière puis retombe par petites
        # secousses saccadées (le rire de la hyène), la queue reste basse.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.03), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, up in ((1, 0.0), (8, -0.30), (11, -0.10), (14, -0.30), (17, -0.05),
                      (20, -0.30), (28, 0.0), (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f, sw in ((1, 0.10), (20, -0.10), (40, 0.10)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Trot bas de charognard, oscillant : les pattes avant hautes et
        # arrière basses accentuent le déhanché en pente.
        def extras(kr):
            for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
                kr("Head", f, (nod, 0, 0))
            for f, sw in ((1, 0.14), (13, -0.14), (24, 0.14)):
                kr("Tail", f, (0, 0, sw))
        quad_walk_keys(key_rot, key_loc, 20, extras)

    build_creature("creature67", bones, idle, walk, cam=0.85)


lion()
zebre()
guepard()
rhinoceros()
hyene()
print("PACK DONE")
