"""Génère assets/models/creature32.glb … creature36.glb : 5 monstres marins.

Pack « lagune & abysses » — poulpe, requin, tortue de mer, crabe géant,
baudroie-lanterne. Animations pensées « eau » : ondulation de tentacules,
battement de nageoire caudale, rame des palettes natatoires, leurre qui
oscille. Numérotés 32-36 : 27-31 sont pris par le pack « insectes &
arachnides » (gen_creature_pack27_31.py). Mêmes conventions que les packs
précédents (creature21/22-26) :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/… par créature, mesh unique skinné (1 os / partie, poids 1.0) ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables, chaque clip
  keyframe tous les os animés par l'autre (piège glTF : canaux absents = os figé) ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur) ;
- échelle appliquée AVANT la rotation (piège rotation/scale des cônes) ;
- AUCUN vertex sous z=0 + marge 0,02 : un mesh qui perce le sol laisse le
  TriMesh kinématique en pénétration permanente et peut figer la créature
  (bug élucidé sur le gorille, cf. commentaire Créature 24 dans scene/demos.rs) ;
- pose remise au neutre avant export ET avant la vignette.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack32_36.py
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
# Créature 32 — Poulpe : dôme mauve, 8 tentacules qui ondulent (4 os, 2 chacun).
# =============================================================================
def poulpe():
    fresh_scene()
    mauve = material("Poulpe32Mauve", (0.52, 0.30, 0.55))
    mauve_d = material("Poulpe32MauveD", (0.36, 0.18, 0.40))
    cream = material("Poulpe32Cream", (0.88, 0.82, 0.75))
    dark = material("Poulpe32Dark", (0.08, 0.06, 0.10))

    # Dôme (manteau) + front bombé.
    sphere("Body", mauve, (0, 0.05, 1.05), (0.58, 0.60, 0.65))
    sphere("Body", mauve, (0, -0.30, 0.85), (0.48, 0.42, 0.45))
    # Grands yeux (blanc + pupille) de part et d'autre du front.
    for sx in (-1, 1):
        sphere("Body", cream, (sx * 0.34, -0.52, 1.05), (0.14, 0.10, 0.15))
        sphere("Body", dark, (sx * 0.36, -0.60, 1.05), (0.07, 0.05, 0.08))
    # 8 tentacules rayonnants : un bras tous les ~45° autour du manteau (angle
    # mesuré depuis l'avant -Y), chaîne de 4 sphères qui se chevauchent et
    # s'affinent jusqu'au ras du sol (z ≥ 0,04). 4 os, 2 bras par quadrant.
    for deg in (-25, -65, -115, -155, 25, 65, 115, 155):
        rad = math.radians(deg)
        ux, uy = math.sin(rad), -math.cos(rad)
        bone = ("TenF" if uy < 0 else "TenB") + ("L" if ux < 0 else "R")
        for d, z, r in ((0.30, 0.50, 0.17), (0.55, 0.30, 0.14),
                        (0.78, 0.16, 0.11), (0.98, 0.09, 0.085)):
            sphere(bone, mauve_d, (ux * d, 0.02 + uy * d, z), (r, r, r * 0.9))

    bones = {
        "Body": ("Root", (0, 0.35, 1.00), (0, -0.40, 1.10)),
        "TenFL": ("Body", (-0.25, -0.25, 0.60), (-0.85, -0.45, 0.08)),
        "TenFR": ("Body", (0.25, -0.25, 0.60), (0.85, -0.45, 0.08)),
        "TenBL": ("Body", (-0.25, 0.30, 0.60), (-0.85, 0.60, 0.08)),
        "TenBR": ("Body", (0.25, 0.30, 0.60), (0.85, 0.60, 0.08)),
    }

    def idle(key_rot, key_loc):
        # Houle : le manteau respire, les tentacules ondulent en deux vagues
        # déphasées — la signature « sous-marine » du poulpe.
        for f, dz in ((1, 0.0), (20, 0.06), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.10), (20, -0.10), (40, 0.10)):
            key_rot("Body", f, (0, 0, sw))
        for f, a in ((1, 0.25), (11, -0.20), (21, 0.25), (31, -0.20), (40, 0.25)):
            key_rot("TenFL", f, (a, 0, 0.10))
            key_rot("TenBR", f, (a, 0, -0.10))
        for f, a in ((1, -0.20), (11, 0.25), (21, -0.20), (31, 0.25), (40, -0.20)):
            key_rot("TenFR", f, (a, 0, -0.10))
            key_rot("TenBL", f, (a, 0, 0.10))

    def walk(key_rot, key_loc):
        # Propulsion par jet : le manteau pompe, les huit tentacules balaient
        # ensemble vers l'arrière puis se détendent — nage par à-coups.
        for f, dz in ((1, 0.0), (9, 0.14), (15, -0.02), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.10), (9, 0.55), (15, -0.15), (24, 0.10)):
            key_rot("TenFL", f, (a, 0, 0))
            key_rot("TenFR", f, (a, 0, 0))
        for f, a in ((1, 0.05), (9, 0.45), (15, -0.20), (24, 0.05)):
            key_rot("TenBL", f, (a, 0, 0))
            key_rot("TenBR", f, (a, 0, 0))

    build_creature("creature32", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 33 — Requin : fuselage gris-bleu, nage par battement de caudale.
# =============================================================================
def requin():
    fresh_scene()
    grey = material("Requin33Grey", (0.35, 0.44, 0.55))
    belly = material("Requin33Belly", (0.86, 0.88, 0.90))
    fin = material("Requin33Fin", (0.26, 0.34, 0.44))
    dark = material("Requin33Dark", (0.06, 0.06, 0.08))
    ivory = material("Requin33Ivory", (0.94, 0.92, 0.85))

    # Fuselage : nage « entre deux eaux » (z ~0,78), c'est un habitant du lac.
    sphere("Body", grey, (0, 0.10, 0.78), (0.42, 0.95, 0.45))
    sphere("Body", belly, (0, 0.05, 0.62), (0.36, 0.82, 0.32))  # ventre
    # Museau fondu dans le fuselage + gueule à dents.
    sphere("Head", grey, (0, -0.68, 0.78), (0.40, 0.62, 0.42))
    sphere("Head", belly, (0, -0.98, 0.64), (0.28, 0.36, 0.22))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.22, -1.05, 0.92), (0.055, 0.045, 0.06))
        cone("Head", ivory, (sx * 0.10, -1.26, 0.64), (0.035, 0.035, 0.06),
             rotation=(math.radians(180), 0, 0))
    # Aileron dorsal + pectorales pointées vers l'avant-bas-dehors
    # (échelle AVANT rotation, piège cônes).
    cone("Body", fin, (0, 0.12, 1.30), (0.06, 0.26, 0.34))
    for sx in (-1, 1):
        cone("Body", fin, (sx * 0.44, -0.32, 0.52), (0.05, 0.10, 0.30),
             rotation=(math.radians(115), 0, math.radians(sx * 35)))
    # Queue (os Tail) : pédoncule + caudale en croissant (deux lobes).
    sphere("Tail", grey, (0, 0.95, 0.75), (0.24, 0.40, 0.26))
    cone("Tail", fin, (0, 1.35, 0.95), (0.055, 0.26, 0.38),
         rotation=(math.radians(-35), 0, 0))
    cone("Tail", fin, (0, 1.30, 0.55), (0.05, 0.22, 0.28),
         rotation=(math.radians(-155), 0, 0))

    bones = {
        "Body": ("Root", (0, 0.55, 0.78), (0, -0.55, 0.80)),
        "Head": ("Body", (0, -0.70, 0.78), (0, -1.35, 0.72)),
        "Tail": ("Body", (0, 0.70, 0.76), (0, 1.45, 0.80)),
    }

    def idle(key_rot, key_loc):
        # Nage sur place : caudale qui godille doucement, corps qui tangue.
        for f, sw in ((1, 0.30), (20, -0.30), (40, 0.30)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.08), (20, 0.08), (40, -0.08)):
            key_rot("Head", f, (0, 0, yaw))
        for f, roll in ((1, 0.05), (20, -0.05), (40, 0.05)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (20, 0.05), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    def walk(key_rot, key_loc):
        # Croisière : grands battements de caudale, la tête contre-braque —
        # cinématique de nage carangiforme simplifiée.
        for f, sw in ((1, 0.55), (13, -0.55), (24, 0.55)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.15), (13, 0.15), (24, -0.15)):
            key_rot("Head", f, (0, 0, yaw))
        for f, yaw in ((1, 0.08), (13, -0.08), (24, 0.08)):
            key_rot("Body", f, (0, 0, yaw))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))

    build_creature("creature33", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 34 — Tortue de mer : carapace bombée, rame des palettes natatoires.
# =============================================================================
def tortue():
    fresh_scene()
    shell = material("Tortue34Shell", (0.30, 0.42, 0.24))
    shell_d = material("Tortue34ShellD", (0.20, 0.30, 0.16))
    skin = material("Tortue34Skin", (0.55, 0.60, 0.38))
    belly = material("Tortue34Belly", (0.85, 0.80, 0.62))
    dark = material("Tortue34Dark", (0.08, 0.08, 0.06))

    # Carapace + écailles + plastron (bas du plastron à z 0,12 : marge sol).
    sphere("Body", shell, (0, 0.05, 0.52), (0.62, 0.75, 0.34))
    for x, y in ((0.0, -0.20), (0.0, 0.28), (-0.30, 0.05), (0.30, 0.05)):
        sphere("Body", shell_d, (x, y, 0.76), (0.16, 0.18, 0.08))
    sphere("Body", belly, (0, 0.05, 0.38), (0.54, 0.66, 0.26))
    # Tête sur cou tendu (os Neck) + yeux.
    sphere("Neck", skin, (0, -0.70, 0.55), (0.16, 0.22, 0.15))
    sphere("Head", skin, (0, -0.98, 0.58), (0.20, 0.24, 0.18))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.12, -1.10, 0.66), (0.05, 0.04, 0.05))
    # Palettes natatoires : grandes lentilles plates. Avant = rameuses (os
    # FlipL/FlipR), arrière = gouvernails (os RearL/RearR).
    for bone, sx, y, s in (
        ("FlipL", -1, -0.30, 0.42), ("FlipR", 1, -0.30, 0.42),
        ("RearL", -1, 0.55, 0.28), ("RearR", 1, 0.55, 0.28),
    ):
        sphere(bone, skin, (sx * (0.60 + s * 0.35), y + s * 0.3, 0.34),
               (s, s * 0.55, 0.07))

    bones = {
        "Body": ("Root", (0, 0.45, 0.50), (0, -0.40, 0.55)),
        "Neck": ("Body", (0, -0.55, 0.52), (0, -0.85, 0.58)),
        "Head": ("Neck", (0, -0.85, 0.58), (0, -1.15, 0.60)),
        "FlipL": ("Body", (-0.50, -0.30, 0.40), (-1.05, -0.12, 0.30)),
        "FlipR": ("Body", (0.50, -0.30, 0.40), (1.05, -0.12, 0.30)),
        "RearL": ("Body", (-0.45, 0.55, 0.38), (-0.90, 0.70, 0.32)),
        "RearR": ("Body", (0.45, 0.55, 0.38), (0.90, 0.70, 0.32)),
    }

    def idle(key_rot, key_loc):
        # Flottaison : palettes qui frémissent, tête qui pointe puis rentre.
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.10), (20, -0.10), (40, 0.10)):
            key_rot("FlipL", f, (a, 0, 0))
            key_rot("FlipR", f, (-a, 0, 0))
        for f, a in ((1, -0.06), (20, 0.06), (40, -0.06)):
            key_rot("RearL", f, (a, 0, 0))
            key_rot("RearR", f, (-a, 0, 0))
        for f, ext in ((1, 0.0), (14, 0.18), (26, -0.10), (40, 0.0)):
            key_rot("Neck", f, (ext, 0, 0))
        for f, nod in ((1, 0.0), (14, -0.12), (26, 0.06), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        # Rame : les deux palettes avant tirent ENSEMBLE (brasse de tortue
        # marine), les arrière contre-battent, le corps surfe sur la poussée.
        for f, a in ((1, 0.55), (9, -0.40), (17, 0.20), (24, 0.55)):
            key_rot("FlipL", f, (a, 0, 0.15))
            key_rot("FlipR", f, (a, 0, -0.15))
        for f, a in ((1, -0.25), (9, 0.30), (17, -0.10), (24, -0.25)):
            key_rot("RearL", f, (a, 0, 0))
            key_rot("RearR", f, (a, 0, 0))
        for f, dz in ((1, 0.0), (9, 0.08), (17, 0.0), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, ext in ((1, 0.06), (13, -0.06), (24, 0.06)):
            key_rot("Neck", f, (ext, 0, 0))
        for f in (1, 24):
            key_rot("Head", f, (0, 0, 0))

    build_creature("creature34", bones, idle, walk, cam=0.9)


# =============================================================================
# Créature 35 — Crabe géant : carapace large, pinces qui claquent, scuttle vif.
# =============================================================================
def crabe():
    fresh_scene()
    red = material("Crabe35Red", (0.72, 0.24, 0.14))
    red_d = material("Crabe35RedD", (0.48, 0.14, 0.08))
    cream = material("Crabe35Cream", (0.90, 0.80, 0.65))
    dark = material("Crabe35Dark", (0.07, 0.05, 0.05))

    # Carapace large et plate + ventre clair (bas à z 0,18).
    sphere("Body", red, (0, 0.05, 0.50), (0.80, 0.55, 0.30))
    sphere("Body", cream, (0, 0.05, 0.38), (0.68, 0.46, 0.20))
    # Yeux sur pédoncules.
    for sx in (-1, 1):
        cylinder("Body", red_d, (sx * 0.22, -0.42, 0.82), (0.045, 0.045, 0.28))
        sphere("Body", dark, (sx * 0.22, -0.44, 0.98), (0.09, 0.08, 0.09))
    # Pinces massives : bras + pince + doigts (os ClawL/ClawR).
    for bone, sx in (("ClawL", -1), ("ClawR", 1)):
        cylinder(bone, red_d, (sx * 0.78, -0.42, 0.44), (0.11, 0.11, 0.40),
                 rotation=(math.radians(90), 0, math.radians(-sx * 35)))
        sphere(bone, red, (sx * 0.95, -0.72, 0.46), (0.28, 0.34, 0.20))
        cone(bone, red_d, (sx * 0.85, -1.04, 0.54), (0.09, 0.09, 0.18),
             rotation=(math.radians(105), 0, math.radians(sx * 10)))
        cone(bone, red_d, (sx * 1.05, -1.02, 0.40), (0.08, 0.08, 0.16),
             rotation=(math.radians(100), 0, math.radians(-sx * 12)))
    # 8 pattes arquées (4 os, 2 pattes par os), pointes à z ≥ 0,04.
    # Pattes en cônes pointés dehors-bas (même recette que le scorpion 26) :
    # base ancrée sous la carapace, pointe au ras du sol.
    for bone, x, y in (("LegFL", -0.62, -0.15), ("LegFR", 0.62, -0.15),
                       ("LegBL", -0.62, 0.28), ("LegBR", 0.62, 0.28)):
        sx = 1 if x > 0 else -1
        for dy in (0.0, 0.24):
            cone(bone, red_d, (x + sx * 0.16, y + dy, 0.26),
                 (0.06, 0.06, 0.28), rotation=(0, math.radians(sx * 128), 0))

    bones = {
        "Body": ("Root", (0, 0.40, 0.50), (0, -0.45, 0.52)),
        "ClawL": ("Body", (-0.60, -0.40, 0.46), (-1.00, -1.10, 0.44)),
        "ClawR": ("Body", (0.60, -0.40, 0.46), (1.00, -1.10, 0.44)),
        "LegFL": ("Body", (-0.55, -0.15, 0.45), (-0.95, -0.10, 0.04)),
        "LegFR": ("Body", (0.55, -0.15, 0.45), (0.95, -0.10, 0.04)),
        "LegBL": ("Body", (-0.55, 0.28, 0.45), (-0.95, 0.35, 0.04)),
        "LegBR": ("Body", (0.55, 0.28, 0.45), (0.95, 0.35, 0.04)),
    }
    legs = ("LegFL", "LegFR", "LegBL", "LegBR")

    def idle(key_rot, key_loc):
        # Menace de marée basse : pinces qui claquent en alternance, carapace
        # qui se soulève comme portée par la houle.
        for f in (1, 40):
            for leg in legs:
                key_rot(leg, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.06), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.0), (10, 0.40), (16, 0.05), (28, 0.40), (40, 0.0)):
            key_rot("ClawL", f, (0, 0, a))
        for f, a in ((1, 0.40), (10, 0.05), (22, 0.40), (34, 0.05), (40, 0.40)):
            key_rot("ClawR", f, (0, 0, -a))

    def walk(key_rot, key_loc):
        # Scuttle : deux vagues de pattes en quinconce, pinces relevées en
        # garde, carapace qui roule d'un bord à l'autre.
        swing = math.radians(24)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegFL", f, (s, 0, 0))
            key_rot("LegBR", f, (s, 0, 0))
            key_rot("LegFR", f, (-s, 0, 0))
            key_rot("LegBL", f, (-s, 0, 0))
        for f, roll in ((1, 0.08), (13, -0.08), (24, 0.08)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (7, 0.03), (13, 0.0), (19, 0.03), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, a in ((1, 0.20), (13, 0.30), (24, 0.20)):
            key_rot("ClawL", f, (-0.15, 0, a))
            key_rot("ClawR", f, (-0.15, 0, -a))

    build_creature("creature35", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 36 — Baudroie-lanterne : gueule béante, leurre lumineux qui oscille.
# =============================================================================
def baudroie():
    fresh_scene()
    brown = material("Baudroie36Brown", (0.30, 0.22, 0.20))
    brown_d = material("Baudroie36BrownD", (0.20, 0.14, 0.13))
    glow = material("Baudroie36Glow", (0.98, 0.85, 0.30), roughness=0.3, emission=3.0)
    ivory = material("Baudroie36Ivory", (0.92, 0.90, 0.82))
    dark = material("Baudroie36Dark", (0.05, 0.04, 0.04))

    # Corps globuleux flottant (z ~0,85 : elle « nage » au-dessus du fond).
    sphere("Body", brown, (0, 0.10, 0.85), (0.52, 0.60, 0.50))
    # Front proéminent + petits yeux globuleux.
    sphere("Head", brown, (0, -0.48, 0.95), (0.42, 0.30, 0.34))
    for sx in (-1, 1):
        sphere("Head", ivory, (sx * 0.26, -0.60, 1.10), (0.09, 0.07, 0.09))
        sphere("Head", dark, (sx * 0.27, -0.66, 1.10), (0.05, 0.04, 0.05))
        for dx in (0.10, 0.26):  # dents du haut
            cone("Head", ivory, (sx * dx, -0.72, 0.82), (0.035, 0.035, 0.09),
                 rotation=(math.radians(180), 0, 0))
    # Mâchoire inférieure (os Jaw) : lippe + dents vers le haut.
    sphere("Jaw", brown_d, (0, -0.50, 0.62), (0.40, 0.30, 0.16))
    for sx in (-1, 1):
        for dx in (0.12, 0.28):
            cone("Jaw", ivory, (sx * dx, -0.68, 0.70), (0.035, 0.035, 0.09))
    # Leurre (os Lure) : tige arquée depuis le front + lanterne lumineuse.
    cylinder("Lure", brown_d, (0, -0.42, 1.52), (0.035, 0.035, 0.55),
             rotation=(math.radians(38), 0, 0))
    sphere("Lure", glow, (0, -0.62, 1.72), (0.11, 0.11, 0.11))
    # Nageoires : pectorales rondes + caudale en éventail (os Tail).
    for sx in (-1, 1):
        sphere("Body", brown_d, (sx * 0.52, 0.15, 0.80), (0.10, 0.26, 0.18))
    sphere("Tail", brown, (0, 0.72, 0.85), (0.18, 0.24, 0.20))
    cone("Tail", brown_d, (0, 1.05, 0.85), (0.06, 0.30, 0.34),
         rotation=(math.radians(-90), 0, 0))

    bones = {
        "Body": ("Root", (0, 0.40, 0.85), (0, -0.35, 0.88)),
        "Head": ("Body", (0, -0.25, 0.92), (0, -0.80, 0.95)),
        "Jaw": ("Head", (0, -0.25, 0.70), (0, -0.80, 0.58)),
        "Lure": ("Head", (0, -0.30, 1.35), (0, -0.65, 1.75)),
        "Tail": ("Body", (0, 0.55, 0.85), (0, 1.15, 0.85)),
    }

    def idle(key_rot, key_loc):
        # Affût des abysses : le leurre oscille pour appâter, la gueule bâille
        # lentement, la caudale godille à peine — flottaison stationnaire.
        for f, sw in ((1, 0.30), (11, -0.30), (21, 0.30), (31, -0.30), (40, 0.30)):
            key_rot("Lure", f, (0, sw * 0.4, sw))
        for f, open_ in ((1, 0.0), (18, 0.35), (26, 0.35), (32, 0.0), (40, 0.0)):
            key_rot("Jaw", f, (-open_, 0, 0))
        for f, sw in ((1, 0.12), (20, -0.12), (40, 0.12)):
            key_rot("Tail", f, (0, 0, sw))
        for f, dz in ((1, 0.0), (20, 0.07), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f in (1, 40):
            key_rot("Head", f, (0, 0, 0))

    def walk(key_rot, key_loc):
        # Nage : caudale ample, le leurre traîne dans le courant, gueule close.
        for f, sw in ((1, 0.40), (13, -0.40), (24, 0.40)):
            key_rot("Tail", f, (0, 0, sw))
        for f, yaw in ((1, -0.10), (13, 0.10), (24, -0.10)):
            key_rot("Head", f, (0, 0, yaw))
        for f, drag in ((1, 0.25), (13, 0.45), (24, 0.25)):
            key_rot("Lure", f, (drag, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.05), (13, 0.0), (19, 0.05), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f in (1, 24):
            key_rot("Jaw", f, (0, 0, 0))

    build_creature("creature36", bones, idle, walk, cam=0.9)


poulpe()
requin()
tortue()
crabe()
baudroie()
print("PACK DONE")
