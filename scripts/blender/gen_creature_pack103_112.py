"""Génère assets/models/creature103.glb … creature112.glb : 10 animaux, thème
« faune cavernicole ». Le pack grotte (`grotto_*.glb` — colonnes, stalactites,
champignons luminescents, ossements, cf. `docs/creation3DBlenderOrganicSuite.md`)
est entièrement construit en décor mais n'a encore AUCUNE créature vivante :
ce pack peuple enfin ces grottes.

Chauve-souris, salamandre aveugle, rat-taupe, grillon cavernicole, opilion des
cavernes, crabe des stalactites, scarabée luisant, ver des cavernes géant,
troll des cavernes, serpent aveugle. Deux clips par créature (Idle, Walk),
technique `creature_kit.py` (primitives, un os/pièce, LOD auto, aucun vertex
sous z=0). QA par `check_creatures.py` après génération.

Leçons des packs 73-102 (session) appliquées dès la première passe :
- tête/corps : chevauchement GÉNÉREUX (agrandir la sphère plutôt qu'un pont
  tangent trop juste, qui laisse un trou vu depuis la caméra en plongée) ;
- chaque frame ne reçoit qu'UN SEUL appel `key_rot(bone, ...)` par os
  (sinon la boucle Idle→Walk se rouvre silencieusement) ;
- un cône penché/incliné pivote autour de son centre : remonter légèrement
  son ancrage pour ne pas percer z=0.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack103_112.py
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
# Créature 103 — Chauve-souris : ailes membraneuses, vol suspendu.
# =============================================================================
def chauve_souris():
    fresh_scene()
    fur = material("Chauve103Fur", (0.18, 0.15, 0.16))
    wing = material("Chauve103Wing", (0.24, 0.18, 0.22), roughness=0.6)
    dark = material("Chauve103Dark", (0.05, 0.04, 0.05))

    sphere("Body", fur, (0, 0.05, 0.40), (0.13, 0.20, 0.15))
    sphere("Head", fur, (0, -0.20, 0.46), (0.11, 0.12, 0.10))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.06, -0.28, 0.44), (0.022, 0.02, 0.022))
        cone("Head", fur, (sx * 0.08, -0.12, 0.58), (0.045, 0.03, 0.10),
             rotation=(math.radians(-10), 0, math.radians(sx * 15)))  # oreille
        cone("Head", dark, (sx * 0.03, -0.30, 0.40), (0.018, 0.018, 0.05),
             rotation=(math.radians(100), 0, 0))  # museau écrasé
    # Ailes membraneuses : chaîne de segments qui s'évasent depuis le corps.
    for sx in (-1, 1):
        for k in range(4):
            t = k / 3.0
            cone(f"Wing{'L' if sx < 0 else 'R'}", wing,
                 (sx * (0.16 + 0.30 * t), -0.02 + 0.05 * t, 0.42 - 0.08 * t),
                 (0.09, 0.16 - 0.03 * t, 0.012),
                 rotation=(0, 0, math.radians(sx * (25 + 20 * t))))
    for sx in (-1, 1):
        cylinder("Body", dark, (sx * 0.05, 0.20, 0.28), (0.012, 0.012, 0.16))
        sphere("Body", dark, (sx * 0.05, 0.20, 0.19), (0.02, 0.03, 0.012))  # pied crochu

    bones = {
        "Body": ("Root", (0, 0.15, 0.42), (0, -0.05, 0.44)),
        "Head": ("Body", (0, -0.10, 0.44), (0, -0.30, 0.44)),
        "WingL": ("Body", (-0.14, -0.02, 0.44), (-0.55, 0.10, 0.30)),
        "WingR": ("Body", (0.14, -0.02, 0.44), (0.55, 0.10, 0.30)),
    }

    def idle(key_rot, key_loc):
        # Suspendue tête en bas dans l'esprit, mais posée ici comme les
        # autres (contrainte moteur : sol) — ailes repliées qui frémissent.
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))
        for f, a in ((1, 0.05), (20, 0.15), (40, 0.05)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, yaw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, yaw))

    def walk(key_rot, key_loc):
        # Vol battu, corps qui monte et descend au rythme des ailes.
        for f, a in ((1, 0.9), (7, -0.6), (13, 0.9)):
            key_rot("WingL", f, (0, 0, a))
            key_rot("WingR", f, (0, 0, -a))
        for f, dz in ((1, 0.0), (4, 0.10), (7, 0.0), (10, 0.10), (13, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature("creature103", bones, idle, walk, cam=0.4)


# =============================================================================
# Créature 104 — Salamandre aveugle : peau rose translucide, branchies.
# =============================================================================
def salamandre_aveugle():
    fresh_scene()
    pink = material("Salamandre104Pink", (0.86, 0.68, 0.66))
    pink_l = material("Salamandre104PinkL", (0.92, 0.80, 0.78))
    gill = material("Salamandre104Gill", (0.72, 0.30, 0.34))

    sphere("Body", pink, (0, 0.05, 0.16), (0.09, 0.34, 0.10))
    sphere("Head", pink, (0, -0.36, 0.17), (0.075, 0.10, 0.075))
    sphere("Head", pink_l, (0, -0.44, 0.15), (0.04, 0.05, 0.035))  # museau, pas d'yeux
    for sx in (-1, 1):
        for k in range(3):  # branchies externes en plumeau
            cone("Head", gill, (sx * (0.08 + 0.02 * k), -0.30 + 0.03 * k, 0.19),
                 (0.02, 0.05, 0.02),
                 rotation=(math.radians(90), 0, math.radians(sx * (30 + 20 * k))))
    for bone, x, y in (("LegFL", -0.07, -0.16), ("LegFR", 0.07, -0.16),
                       ("LegBL", -0.07, 0.20), ("LegBR", 0.07, 0.20)):
        cylinder(bone, pink, (x, y, 0.07), (0.018, 0.018, 0.09))
    for k in range(4):  # queue plate effilée, nage anguille
        t = k / 3.0
        sphere("Tail", pink, (0, 0.30 + 0.16 * t, 0.15 - 0.02 * t),
               (0.06 - 0.035 * t, 0.10, 0.05 - 0.028 * t))

    bones = quad_bones(0.07, -0.16, 0.20, 0.10, ((0, 0.20, 0.15), (0, -0.20, 0.16)), {
        "Head": ("Body", (0, -0.24, 0.16), (0, -0.44, 0.17)),
        "Tail": ("Body", (0, 0.28, 0.15), (0, 0.62, 0.10)),
    })

    def idle(key_rot, key_loc):
        # Immobile dans l'eau souterraine, seules les branchies ondulent.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, sw in ((1, 0.0), (20, 0.4), (40, 0.0)):
            key_rot("Head", f, (0, sw, 0))
        for f, sw in ((1, 0.15), (20, -0.15), (40, 0.15)):
            key_rot("Tail", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Ondulation anguille : le corps entier serpente, pattes secondaires.
        for f, yaw in ((1, 0.25), (13, -0.25), (24, 0.25)):
            key_rot("Body", f, (0, 0, yaw))
        for f, sw in ((1, 0.4), (13, -0.4), (24, 0.4)):
            key_rot("Tail", f, (0, 0, sw))
        for f in (1, 24):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))

    build_creature("creature104", bones, idle, walk, cam=0.35)


# =============================================================================
# Créature 105 — Rat-taupe : creuse, aveugle, grandes incisives.
# =============================================================================
def rat_taupe():
    fresh_scene()
    skin = material("RatTaupe105Skin", (0.78, 0.68, 0.62))
    skin_d = material("RatTaupe105SkinD", (0.62, 0.52, 0.48))
    tooth = material("RatTaupe105Tooth", (0.85, 0.72, 0.32))
    dark = material("RatTaupe105Dark", (0.08, 0.07, 0.07))

    sphere("Body", skin, (0, 0.06, 0.16), (0.14, 0.28, 0.13))
    sphere("Head", skin, (0, -0.28, 0.16), (0.11, 0.12, 0.10))
    sphere("Head", skin_d, (0, -0.42, 0.13), (0.055, 0.06, 0.045))  # museau, pas d'yeux
    for sx in (-1, 1):
        cylinder("Head", tooth, (sx * 0.025, -0.47, 0.09), (0.012, 0.012, 0.045),
                  rotation=(math.radians(15), 0, 0))
        sphere("Head", skin_d, (sx * 0.10, -0.18, 0.22), (0.025, 0.02, 0.02))  # oreille minuscule
    for bone, x, y in (("LegFL", -0.10, -0.14), ("LegFR", 0.10, -0.14),
                       ("LegBL", -0.10, 0.18), ("LegBR", 0.10, 0.18)):
        cylinder(bone, skin, (x, y, 0.07), (0.03, 0.03, 0.08))
        sphere(bone, dark, (x, y, 0.02), (0.035, 0.045, 0.015))
    sphere("Tail", skin_d, (0, 0.32, 0.13), (0.02, 0.02, 0.018))

    bones = quad_bones(0.10, -0.14, 0.18, 0.10, ((0, 0.16, 0.14), (0, -0.16, 0.15)), {
        "Head": ("Body", (0, -0.20, 0.15), (0, -0.42, 0.14)),
        "Tail": ("Body", (0, 0.22, 0.13), (0, 0.38, 0.12)),
    })

    def idle(key_rot, key_loc):
        # Renifle l'air, tête qui balaie bas, incisives bien visibles.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, sw in ((1, 0.0), (12, 0.35), (24, -0.35), (40, 0.0)):
            key_rot("Head", f, (0.15, 0, sw))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 16, lambda kr: None)

    build_creature("creature105", bones, idle, walk, cam=0.3)


# =============================================================================
# Créature 106 — Grillon cavernicole : antennes démesurées, dos bossu.
# =============================================================================
def grillon_cavernicole():
    fresh_scene()
    shell = material("Grillon106Shell", (0.62, 0.52, 0.38))
    shell_d = material("Grillon106ShellD", (0.44, 0.36, 0.26))
    dark = material("Grillon106Dark", (0.10, 0.09, 0.08))

    sphere("Body", shell, (0, 0.05, 0.20), (0.13, 0.22, 0.20))  # dos bossu haut
    sphere("Body", shell_d, (0, -0.05, 0.14), (0.11, 0.16, 0.10))
    sphere("Head", shell, (0, -0.26, 0.18), (0.08, 0.08, 0.08))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.04, -0.30, 0.20), (0.014, 0.012, 0.014))
        for k in range(5):  # antenne démesurée, chaîne dense
            t = k / 4.0
            cylinder("Head", dark, (sx * (0.04 + 0.16 * t), -0.30 - 0.30 * t, 0.20 + 0.10 * t),
                      (0.006, 0.006, 0.09),
                      rotation=(0, math.radians(sx * (20 + 15 * t)), 0))
    # Grandes pattes arrière repliées (bond), pattes avant fines.
    for bone, sx in (("LegFL", -1), ("LegFR", 1)):
        cylinder(bone, shell_d, (sx * 0.09, -0.05, 0.09), (0.02, 0.02, 0.14))
    for bone, sx in (("LegBL", -1), ("LegBR", 1)):
        sphere(bone, shell, (sx * 0.13, 0.10, 0.11), (0.05, 0.10, 0.06))
        # z remonté (0.03→0.045) : un cylindre court centré à 0.03 avec
        # demi-hauteur 0.04 perçait sous z=0 (garde-fou déjà documenté).
        cylinder(bone, shell_d, (sx * 0.15, 0.18, 0.045), (0.014, 0.014, 0.08))

    bones = {
        "Body": ("Root", (0, 0.18, 0.14), (0, -0.10, 0.18)),
        "Head": ("Body", (0, -0.10, 0.18), (0, -0.30, 0.18)),
        "LegFL": ("Body", (-0.09, -0.05, 0.13), (-0.09, -0.05, 0.02)),
        "LegFR": ("Body", (0.09, -0.05, 0.13), (0.09, -0.05, 0.02)),
        "LegBL": ("Body", (-0.13, 0.10, 0.14), (-0.13, 0.20, 0.02)),
        "LegBR": ("Body", (0.13, 0.10, 0.14), (0.13, 0.20, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Antennes qui balaient sans cesse (seul sens dans le noir total).
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, sw in ((1, 0.0), (10, 0.5), (20, -0.5), (30, 0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Petits bonds saccadés sur les grandes pattes arrière.
        for f, dz in ((1, -0.02), (6, 0.10), (12, -0.02), (18, 0.10), (24, -0.02)):
            key_loc("Body", f, (0, dz, 0))
        for bone in ("LegBL", "LegBR"):
            for f, a in ((1, 0.2), (6, -0.6), (12, 0.1), (18, -0.6), (24, 0.2)):
                key_rot(bone, f, (a, 0, 0))
        for f in (1, 24):
            key_rot("LegFL", f, (0, 0, 0))
            key_rot("LegFR", f, (0, 0, 0))

    build_creature("creature106", bones, idle, walk, cam=0.3)


# =============================================================================
# Créature 107 — Opilion des cavernes : pattes filiformes démesurées.
# =============================================================================
def opilion_cavernes():
    fresh_scene()
    pale = material("Opilion107Pale", (0.72, 0.64, 0.56))
    dark = material("Opilion107Dark", (0.10, 0.09, 0.08))

    sphere("Body", pale, (0, 0, 0.42), (0.06, 0.07, 0.055))
    sphere("Body", dark, (0, -0.05, 0.40), (0.02, 0.02, 0.02))  # œil résiduel unique

    bones = {"Body": ("Root", (0, 0, 0.35), (0, 0, 0.48))}
    for i in range(4):
        sx = -1 if i < 2 else 1
        sy = -1 if i % 2 == 0 else 1
        bone = f"Leg{i}"
        hip = (sx * 0.05, sy * 0.05, 0.40)
        knee = (sx * 0.30, sy * 0.30, 0.24)
        foot = (sx * 0.42, sy * 0.42, 0.02)
        cylinder(bone, pale, ((hip[0] + knee[0]) / 2, (hip[1] + knee[1]) / 2,
                              (hip[2] + knee[2]) / 2), (0.010, 0.010, 0.32),
                  rotation=(math.radians(sy * 42), math.radians(-sx * 42), 0))
        cylinder(bone, dark, ((knee[0] + foot[0]) / 2, (knee[1] + foot[1]) / 2,
                              (knee[2] + foot[2]) / 2), (0.007, 0.007, 0.28),
                  rotation=(math.radians(sy * 55), math.radians(-sx * 55), 0))
        bones[bone] = ("Body", hip, foot)

    def idle(key_rot, key_loc):
        # Immobile, seules les pattes tâtonnent très légèrement au sol.
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))
        for i in range(4):
            for f, a in ((1, 0.0), (20, 0.08 if i % 2 else -0.08), (40, 0.0)):
                key_rot(f"Leg{i}", f, (0, 0, a))

    def walk(key_rot, key_loc):
        # Démarche vacillante : chaque patte reçoit EXACTEMENT les mêmes
        # frames (1/13/24), en boucle parfaite — la version précédente
        # décalait les frames par patte avec un modulo qui ne revenait
        # jamais à la valeur de départ à la frame 24, rouvrant la boucle.
        for f, dz in ((1, 0.0), (13, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, 0, dz))
        for i in range(4):
            sign = 1 if i % 2 else -1
            for f, a in ((1, 0.0), (13, 0.25 * sign), (24, 0.0)):
                key_rot(f"Leg{i}", f, (0, 0, a))

    build_creature("creature107", bones, idle, walk, cam=0.35)


# =============================================================================
# Créature 108 — Crabe des stalactites : agrippé au plafond, pinces prêtes.
# =============================================================================
def crabe_stalactites():
    fresh_scene()
    shell = material("Crabe108Shell", (0.82, 0.80, 0.78))
    shell_d = material("Crabe108ShellD", (0.66, 0.64, 0.60))
    dark = material("Crabe108Dark", (0.08, 0.07, 0.07))

    sphere("Body", shell, (0, 0, 0.20), (0.16, 0.20, 0.10))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.10, -0.14, 0.24), (0.018, 0.016, 0.018))
        cylinder(f"Claw{'L' if sx < 0 else 'R'}", shell_d, (sx * 0.22, -0.06, 0.20),
                  (0.03, 0.03, 0.16), rotation=(0, math.radians(sx * 60), 0))
        sphere(f"Claw{'L' if sx < 0 else 'R'}", shell, (sx * 0.32, -0.10, 0.20),
               (0.06, 0.05, 0.04))  # pince
    for i in range(4):
        sx = -1 if i < 2 else 1
        sy = -1 if i % 2 == 0 else 1
        bone = f"Leg{i}"
        cylinder(bone, shell_d, (sx * 0.20, sy * 0.14, 0.18), (0.014, 0.014, 0.20),
                  rotation=(math.radians(sy * 50), math.radians(-sx * 30), 0))

    bones = {
        "Body": ("Root", (0, 0, 0.14), (0, 0, 0.26)),
        "Head": ("Body", (0, -0.05, 0.22), (0, -0.16, 0.24)),
        "ClawL": ("Body", (-0.16, -0.04, 0.20), (-0.36, -0.12, 0.20)),
        "ClawR": ("Body", (0.16, -0.04, 0.20), (0.36, -0.12, 0.20)),
    }
    for i in range(4):
        sx = -1 if i < 2 else 1
        sy = -1 if i % 2 == 0 else 1
        bones[f"Leg{i}"] = ("Body", (sx * 0.14, sy * 0.06, 0.18),
                            (sx * 0.34, sy * 0.28, 0.10))

    def idle(key_rot, key_loc):
        # Pinces qui claquent doucement, pattes agrippées immobiles.
        for f in (1, 40):
            for i in range(4):
                key_rot(f"Leg{i}", f, (0, 0, 0))
        for f, a in ((1, 0.0), (10, 0.3), (20, 0.0), (30, 0.3), (40, 0.0)):
            key_rot("ClawL", f, (0, 0, a))
            key_rot("ClawR", f, (0, 0, -a))

    def walk(key_rot, key_loc):
        # Marche latérale caractéristique, pinces qui se balancent.
        for f, roll in ((1, 0.15), (13, -0.15), (24, 0.15)):
            key_rot("Body", f, (0, roll, 0))
        for i in range(4):
            for f, a in ((1, 0.2 if i % 2 else -0.2), (13, -0.2 if i % 2 else 0.2),
                         (24, 0.2 if i % 2 else -0.2)):
                key_rot(f"Leg{i}", f, (0, 0, a))
        for f, a in ((1, 0.2), (13, -0.2), (24, 0.2)):
            key_rot("ClawL", f, (0, 0, a))
            key_rot("ClawR", f, (0, 0, -a))

    build_creature("creature108", bones, idle, walk, cam=0.3)


# =============================================================================
# Créature 109 — Scarabée luisant : carapace bioluminescente par taches.
# =============================================================================
def scarabee_luisant():
    fresh_scene()
    shell = material("Scarabee109Shell", (0.10, 0.09, 0.12))
    glow = material("Scarabee109Glow", (0.30, 0.90, 0.55), roughness=0.4, emission=1.6)
    dark = material("Scarabee109Dark", (0.06, 0.05, 0.06))

    sphere("Body", shell, (0, 0.02, 0.13), (0.13, 0.20, 0.11))
    for sx, y in ((-0.07, -0.05), (0.07, -0.05), (-0.06, 0.10), (0.06, 0.10), (0.0, 0.02)):
        sphere("Body", glow, (sx, y, 0.19), (0.022, 0.022, 0.012))
    sphere("Head", shell, (0, -0.22, 0.13), (0.06, 0.06, 0.06))
    cone("Head", dark, (0, -0.30, 0.11), (0.012, 0.012, 0.06),
         rotation=(math.radians(95), 0, 0))  # corne
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.035, -0.24, 0.15), (0.012, 0.01, 0.012))
    for bone, x, y in (("LegFL", -0.10, -0.10), ("LegFR", 0.10, -0.10),
                       ("LegBL", -0.10, 0.14), ("LegBR", 0.10, 0.14)):
        cylinder(bone, dark, (x, y, 0.07), (0.014, 0.014, 0.10))

    bones = quad_bones(0.10, -0.10, 0.14, 0.10, ((0, 0.14, 0.13), (0, -0.14, 0.14)), {
        "Head": ("Body", (0, -0.16, 0.14), (0, -0.30, 0.13)),
    })

    def idle(key_rot, key_loc):
        # Les taches luisent, corps immobile, corne qui tâte le terrain.
        for f in (1, 40):
            for leg in LEGS4:
                key_rot(leg, f, (0, 0, 0))
        for f, down in ((1, 0.0), (16, 0.2), (32, 0.0), (40, 0.0)):
            key_rot("Head", f, (down, 0, 0))

    def walk(key_rot, key_loc):
        quad_walk_keys(key_rot, key_loc, 20, lambda kr: None)

    build_creature("creature109", bones, idle, walk, cam=0.3)


# =============================================================================
# Créature 110 — Ver des cavernes : géant, segmenté, queue luminescente.
# =============================================================================
def ver_cavernes():
    fresh_scene()
    skin = material("Ver110Skin", (0.56, 0.42, 0.44))
    skin_d = material("Ver110SkinD", (0.42, 0.30, 0.32))
    glow = material("Ver110Glow", (0.55, 0.85, 0.40), roughness=0.4, emission=1.4)
    dark = material("Ver110Dark", (0.10, 0.08, 0.08))

    for k in range(8):  # corps segmenté, chaîne dense qui se chevauche
        t = k / 7.0
        m = skin if k % 2 == 0 else skin_d
        r = 0.13 - 0.03 * t
        sphere("Body", m, (0, -0.50 + 1.10 * t, 0.16), (r, 0.14, r))
    sphere("Head", skin, (0, -0.62, 0.16), (0.11, 0.11, 0.10))
    for sx in (-1, 1):
        cylinder("Head", dark, (sx * 0.05, -0.72, 0.16), (0.012, 0.012, 0.08),
                  rotation=(math.radians(90), 0, math.radians(sx * 20)))  # palpe
    sphere("Tail", glow, (0, 0.62, 0.14), (0.06, 0.09, 0.06))

    bones = {
        "Body": ("Root", (0, -0.20, 0.16), (0, 0.30, 0.16)),
        "Head": ("Body", (0, -0.40, 0.16), (0, -0.72, 0.16)),
        "Tail": ("Body", (0, 0.45, 0.15), (0, 0.68, 0.14)),
    }

    def idle(key_rot, key_loc):
        # Corps qui ondule lentement sur place, la queue luit et pulse.
        for f, sw in ((1, 0.0), (14, 0.3), (28, -0.3), (40, 0.0)):
            key_rot("Body", f, (0, 0, sw))
        for f, up in ((1, 0.0), (14, 0.2), (28, -0.1), (40, 0.0)):
            key_rot("Head", f, (up, 0, 0))
        for f in (1, 40):
            key_rot("Tail", f, (0, 0, 0))  # queue fixe, seul le matériau émissif « pulse » à l'œil

    def walk(key_rot, key_loc):
        # Reptation : ondulation en S qui progresse le long du corps.
        for f, sw in ((1, 0.3), (13, -0.3), (24, 0.3)):
            key_rot("Body", f, (0, 0, sw))
        for f, sw in ((1, -0.3), (13, 0.3), (24, -0.3)):
            key_rot("Head", f, (0, 0, sw))
        for f, sw in ((1, 0.4), (13, -0.4), (24, 0.4)):
            key_rot("Tail", f, (0, 0, sw))

    build_creature("creature110", bones, idle, walk, cam=0.6)


# =============================================================================
# Créature 111 — Troll des cavernes : massif, peau grise, gourdin naturel.
# =============================================================================
def troll_cavernes():
    fresh_scene()
    skin = material("Troll111Skin", (0.42, 0.44, 0.40))
    skin_d = material("Troll111SkinD", (0.32, 0.34, 0.30))
    tusk = material("Troll111Tusk", (0.80, 0.78, 0.68))
    dark = material("Troll111Dark", (0.07, 0.06, 0.06))

    sphere("Body", skin, (0, 0.05, 1.05), (0.42, 0.55, 0.55))
    sphere("Body", skin_d, (0, -0.15, 0.75), (0.34, 0.30, 0.32))  # bedaine basse
    sphere("Head", skin, (0, -0.25, 1.55), (0.26, 0.28, 0.26))
    sphere("Head", skin_d, (0, -0.48, 1.44), (0.13, 0.14, 0.11))  # mâchoire lourde
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.12, -0.30, 1.62), (0.03, 0.026, 0.026))
        cone("Head", tusk, (sx * 0.11, -0.50, 1.34), (0.025, 0.025, 0.16),
             rotation=(math.radians(155), 0, math.radians(sx * 15)))  # croc
        sphere("Head", skin_d, (sx * 0.24, -0.10, 1.60), (0.06, 0.05, 0.07))  # oreille
    # Bras massifs, un poing traînant au sol.
    for sx, y in ((-1, 0.0), (1, 0.0)):
        cylinder(f"Arm{'L' if sx < 0 else 'R'}", skin, (sx * 0.44, y, 0.85),
                  (0.13, 0.13, 0.65), rotation=(math.radians(15), 0, math.radians(-sx * 12)))
        sphere(f"Arm{'L' if sx < 0 else 'R'}", skin_d, (sx * 0.52, y + 0.05, 0.30),
               (0.16, 0.16, 0.16))  # poing
    for bone, x in (("LegL", -0.20), ("LegR", 0.20)):
        cylinder(bone, skin_d, (x, 0.05, 0.36), (0.17, 0.17, 0.62))
        # z remonté (0.05→0.065) : la sphère de pied (rayon 0.06) centrée à
        # 0.05 perçait sous z=0 (garde-fou déjà documenté).
        sphere(bone, dark, (x, 0.10, 0.065), (0.19, 0.24, 0.06))  # pied large

    bones = {
        "Body": ("Root", (0, 0.30, 1.00), (0, -0.20, 1.15)),
        "Head": ("Body", (0, -0.35, 1.35), (0, -0.45, 1.65)),
        "ArmL": ("Body", (-0.40, 0.0, 1.30), (-0.55, 0.02, 0.25)),
        "ArmR": ("Body", (0.40, 0.0, 1.30), (0.55, 0.02, 0.25)),
        "LegL": ("Body", (-0.20, 0.05, 0.65), (-0.20, 0.05, 0.02)),
        "LegR": ("Body", (0.20, 0.05, 0.65), (0.20, 0.05, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Respire lourdement, tête qui dodeline, poings qui pendent.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, nod in ((1, 0.0), (20, 0.12), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))
        for f, sw in ((1, 0.0), (20, 0.08), (40, 0.0)):
            key_rot("ArmL", f, (sw, 0, 0))
            key_rot("ArmR", f, (-sw, 0, 0))

    def walk(key_rot, key_loc):
        # Démarche lourde et lente, bras qui se balancent en contretemps.
        s = math.radians(16)
        for f, a in ((1, s), (13, -s), (24, s)):
            key_rot("LegL", f, (a, 0, 0))
            key_rot("LegR", f, (-a, 0, 0))
        for f, a in ((1, -0.25), (13, 0.25), (24, -0.25)):
            key_rot("ArmL", f, (a, 0, 0))
            key_rot("ArmR", f, (-a, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.08), (13, 0.0), (19, 0.08), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature("creature111", bones, idle, walk, cam=1.15)


# =============================================================================
# Créature 112 — Serpent aveugle : corps pâle et lisse, langue tâtonnante.
# =============================================================================
def serpent_aveugle():
    fresh_scene()
    pale = material("Serpent112Pale", (0.82, 0.78, 0.70))
    pale_d = material("Serpent112PaleD", (0.70, 0.66, 0.58))
    tongue = material("Serpent112Tongue", (0.62, 0.20, 0.24))

    # z remonté (0.09→0.105) : le point le plus large du corps (rayon
    # jusqu'à 0.095) centré à 0.09 perçait sous z=0 (garde-fou déjà
    # documenté).
    for k in range(9):  # corps effilé, chaîne dense qui se chevauche
        t = k / 8.0
        m = pale if k % 2 == 0 else pale_d
        r = 0.075 * math.sin(math.pi * min(t * 1.3, 1.0)) + 0.02
        sphere("Body", m, (0, -0.55 + 1.15 * t, 0.105), (max(r, 0.02), 0.14, max(r, 0.02)))
    for sx in (-1, 1):
        cylinder("Head", tongue, (sx * 0.015, -0.72, 0.105), (0.006, 0.006, 0.05),
                  rotation=(math.radians(90), 0, math.radians(sx * 10)))  # langue fourchue

    bones = {
        "Body": ("Root", (0, -0.20, 0.105), (0, 0.30, 0.105)),
        "Head": ("Body", (0, -0.40, 0.105), (0, -0.68, 0.105)),
    }

    def idle(key_rot, key_loc):
        # Immobile, la langue tâtonne l'air (seul sens fiable sous terre).
        for f, sw in ((1, 0.0), (12, 0.3), (24, -0.3), (40, 0.0)):
            key_rot("Head", f, (0, 0, sw))

    def walk(key_rot, key_loc):
        # Reptation sinusoïdale classique.
        for f, sw in ((1, 0.35), (13, -0.35), (24, 0.35)):
            key_rot("Body", f, (0, 0, sw))
        for f, sw in ((1, -0.35), (13, 0.35), (24, -0.35)):
            key_rot("Head", f, (0, 0, sw))

    build_creature("creature112", bones, idle, walk, cam=0.45)


chauve_souris()
salamandre_aveugle()
rat_taupe()
grillon_cavernicole()
opilion_cavernes()
crabe_stalactites()
scarabee_luisant()
ver_cavernes()
troll_cavernes()
serpent_aveugle()
print("PACK DONE")
