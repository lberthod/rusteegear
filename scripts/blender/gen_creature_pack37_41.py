"""Génère assets/models/creature37.glb … creature41.glb : 5 monstres fantastiques.

Pack « féerie & maléfices » — dragonnet, golem de pierre, fantôme, plante
carnivore, slime. Animations signature : battement d'ailes, démarche tellurique,
lévitation ondulante, tige hypnotique qui claque des mâchoires, rebond
gélatineux. Mêmes conventions que les packs 21/22-26/32-36 :
- face vers -Y Blender (= +Z glTF, direction d'avance du script wander à ry=0) ;
- rig Root/Body/… par créature, mesh unique skinné (1 os / partie, poids 1.0) ;
- clips « Idle » (40 fr) et « Walk » (24 fr) à 24 fps, bouclables, chaque clip
  keyframe tous les os animés par l'autre (piège glTF : canaux absents = os figé) ;
- couleurs par matériau (base_color_factor, seul canal lu par l'import moteur) ;
- échelle appliquée AVANT la rotation (piège rotation/scale des cônes) ;
- AUCUN vertex sous z=0 + marge 0,02 (gel par TriMesh incrusté, cf. mémoire
  et commentaire Créature 24 dans scene/demos.rs) ;
- pose remise au neutre avant export ET avant la vignette.

Exécution : /Applications/Blender.app/Contents/MacOS/Blender --background \
    --python scripts/blender/gen_creature_pack37_41.py
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
# Créature 37 — Dragonnet : ailes membraneuses battantes, queue en fer de lance.
# =============================================================================
def dragonnet():
    fresh_scene()
    scale_g = material("Dragonnet37Scale", (0.16, 0.52, 0.38))
    belly = material("Dragonnet37Belly", (0.90, 0.85, 0.62))
    wing = material("Dragonnet37Wing", (0.42, 0.24, 0.45))
    horn = material("Dragonnet37Horn", (0.90, 0.88, 0.78))
    dark = material("Dragonnet37Dark", (0.07, 0.06, 0.08))

    sphere("Body", scale_g, (0, 0.05, 0.95), (0.52, 0.72, 0.50))
    sphere("Body", belly, (0, -0.20, 0.78), (0.40, 0.50, 0.34))  # plastron
    # Tête + museau + cornes en arrière + yeux.
    sphere("Head", scale_g, (0, -0.80, 1.30), (0.34, 0.38, 0.30))
    sphere("Head", scale_g, (0, -1.10, 1.20), (0.18, 0.22, 0.14))  # museau
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.17, -1.02, 1.42), (0.055, 0.045, 0.06))
        cone("Head", horn, (sx * 0.15, -0.60, 1.52), (0.055, 0.055, 0.18),
             rotation=(math.radians(-50), 0, math.radians(sx * 10)))
    # Ailes : bras horizontal ancré dans le dos + membrane qui le chevauche
    # (os WingL/WingR pointés vers l'extérieur).
    for bone, sx in (("WingL", -1), ("WingR", 1)):
        cylinder(bone, scale_g, (sx * 0.45, 0.15, 1.26), (0.07, 0.07, 0.60),
                 rotation=(0, math.radians(sx * 90), 0))
        sphere(bone, wing, (sx * 0.72, 0.22, 1.26), (0.34, 0.26, 0.05))
        sphere(bone, wing, (sx * 0.50, 0.36, 1.24), (0.22, 0.20, 0.045))
    # Pattes ancrées dans le ventre + pieds griffus.
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, scale_g, (sx * 0.22, 0.08, 0.45), (0.13, 0.13, 0.75))
        sphere(bone, dark, (sx * 0.22, -0.04, 0.12), (0.16, 0.22, 0.09))
    # Queue en fer de lance : chaîne serrée jusqu'à la pointe.
    for y, z, s in ((0.62, 0.90, 0.18), (0.92, 0.94, 0.15), (1.18, 1.00, 0.12)):
        sphere("Tail", scale_g, (0, y, z), (s, s * 1.2, s))
    cone("Tail", wing, (0, 1.40, 1.05), (0.14, 0.05, 0.16),
         rotation=(math.radians(-95), 0, 0))

    bones = {
        "Body": ("Root", (0, 0.45, 0.95), (0, -0.40, 1.00)),
        "Head": ("Body", (0, -0.55, 1.15), (0, -1.20, 1.35)),
        "WingL": ("Body", (-0.35, 0.12, 1.25), (-1.15, 0.25, 1.25)),
        "WingR": ("Body", (0.35, 0.12, 1.25), (1.15, 0.25, 1.25)),
        "LegL": ("Body", (-0.22, 0.08, 0.70), (-0.22, 0.08, 0.02)),
        "LegR": ("Body", (0.22, 0.08, 0.70), (0.22, 0.08, 0.02)),
        "Tail": ("Body", (0, 0.55, 0.90), (0, 1.65, 1.10)),
    }

    def idle(key_rot, key_loc):
        # Battement lent : les ailes se soulèvent et retombent, la queue
        # fouette, le corps se soulève à chaque coup d'aile.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, a in ((1, 0.35), (11, -0.25), (21, 0.35), (31, -0.25), (40, 0.35)):
            key_rot("WingL", f, (0, -a, 0))
            key_rot("WingR", f, (0, a, 0))
        for f, dz in ((1, 0.02), (11, 0.08), (21, 0.02), (31, 0.08), (40, 0.02)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.25), (20, -0.25), (40, 0.25)):
            key_rot("Tail", f, (0, 0, sw))
        for f, nod in ((1, 0.0), (20, 0.10), (40, 0.0)):
            key_rot("Head", f, (nod, 0, 0))

    def walk(key_rot, key_loc):
        # Trottinement ailé : pattes alternées, ailes qui battent vite en appui.
        swing = math.radians(24)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegL", f, (s, 0, 0))
            key_rot("LegR", f, (-s, 0, 0))
        for f, a in ((1, 0.45), (7, -0.30), (13, 0.45), (19, -0.30), (24, 0.45)):
            key_rot("WingL", f, (0, -a, 0))
            key_rot("WingR", f, (0, a, 0))
        for f, dz in ((1, 0.0), (7, 0.06), (13, 0.0), (19, 0.06), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.18), (13, -0.18), (24, 0.18)):
            key_rot("Tail", f, (0, 0, sw))
        for f, nod in ((1, 0.05), (13, -0.05), (24, 0.05)):
            key_rot("Head", f, (nod, 0, 0))

    build_creature("creature37", bones, idle, walk, cam=1.05)


# =============================================================================
# Créature 38 — Golem de pierre : blocs moussus, cœur incandescent, pas lourd.
# =============================================================================
def golem():
    fresh_scene()
    stone = material("Golem38Stone", (0.42, 0.41, 0.40))
    stone_d = material("Golem38StoneD", (0.28, 0.27, 0.27))
    moss = material("Golem38Moss", (0.28, 0.44, 0.20))
    core = material("Golem38Core", (0.95, 0.55, 0.12), roughness=0.3, emission=2.5)

    # Torse massif + cœur incandescent dans une cavité.
    sphere("Body", stone, (0, 0.05, 1.20), (0.72, 0.55, 0.68))
    sphere("Body", core, (0, -0.42, 1.15), (0.18, 0.16, 0.20))
    sphere("Body", stone_d, (0, -0.30, 1.52), (0.34, 0.30, 0.22))  # épaule de roc
    for sx, y, z in ((-1, 0.25, 1.55), (1, 0.10, 1.62), (-1, -0.10, 0.95)):
        sphere("Body", moss, (sx * 0.42, y, z), (0.20, 0.22, 0.10))
    # Tête : bloc court aux yeux de braise.
    sphere("Head", stone_d, (0, -0.12, 1.90), (0.30, 0.30, 0.26))
    for sx in (-1, 1):
        sphere("Head", core, (sx * 0.13, -0.38, 1.92), (0.06, 0.04, 0.05))
    # Bras : blocs enchaînés jusqu'à des poings-rochers au sol.
    for bone, sx in (("ArmL", -1), ("ArmR", 1)):
        sphere(bone, stone, (sx * 0.72, 0.0, 1.42), (0.28, 0.28, 0.30))
        sphere(bone, stone_d, (sx * 0.85, -0.08, 0.90), (0.22, 0.24, 0.30))
        sphere(bone, stone, (sx * 0.92, -0.12, 0.35), (0.28, 0.32, 0.30))
    # Jambes trapues + pieds-socles.
    for bone, sx in (("LegL", -1), ("LegR", 1)):
        cylinder(bone, stone_d, (sx * 0.32, 0.15, 0.48), (0.22, 0.22, 0.60))
        sphere(bone, stone, (sx * 0.34, 0.10, 0.16), (0.28, 0.34, 0.13))

    bones = {
        "Body": ("Root", (0, 0.40, 1.15), (0, -0.35, 1.30)),
        "Head": ("Body", (0, -0.05, 1.70), (0, -0.30, 2.05)),
        "ArmL": ("Body", (-0.68, 0.0, 1.50), (-0.95, -0.15, 0.20)),
        "ArmR": ("Body", (0.68, 0.0, 1.50), (0.95, -0.15, 0.20)),
        "LegL": ("Body", (-0.32, 0.15, 0.80), (-0.32, 0.15, 0.02)),
        "LegR": ("Body", (0.32, 0.15, 0.80), (0.32, 0.15, 0.02)),
    }

    def idle(key_rot, key_loc):
        # Sommeil minéral : respiration lente, la tête balaie l'horizon comme
        # une sentinelle, les poings raclent à peine le sol.
        for f in (1, 40):
            for b in ("LegL", "LegR"):
                key_rot(b, f, (0, 0, 0))
        for f, dz in ((1, 0.0), (20, 0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, yaw in ((1, -0.30), (20, 0.30), (40, -0.30)):
            key_rot("Head", f, (0, 0, yaw))
        for f, a in ((1, 0.06), (20, -0.04), (40, 0.06)):
            key_rot("ArmL", f, (a, 0, 0))
            key_rot("ArmR", f, (-a, 0, 0))

    def walk(key_rot, key_loc):
        # Démarche tellurique : jambes raides, bras en balancier massif,
        # le torse tangue à chaque impact.
        swing = math.radians(16)
        for f, s in ((1, swing), (13, -swing), (24, swing)):
            key_rot("LegL", f, (s, 0, 0))
            key_rot("LegR", f, (-s, 0, 0))
            key_rot("ArmL", f, (-s * 1.4, 0, 0))
            key_rot("ArmR", f, (s * 1.4, 0, 0))
        for f, roll in ((1, 0.09), (13, -0.09), (24, 0.09)):
            key_rot("Body", f, (0, roll, 0))
        for f, dz in ((1, 0.0), (7, 0.06), (13, 0.0), (19, 0.06), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f in (1, 24):
            key_rot("Head", f, (0, 0, 0))

    build_creature("creature38", bones, idle, walk, cam=1.05)


# =============================================================================
# Créature 39 — Fantôme : drap flottant, traîne effilochée, jamais au sol.
# =============================================================================
def fantome():
    fresh_scene()
    sheet = material("Fantome39Sheet", (0.85, 0.88, 0.95), roughness=0.6)
    sheet_d = material("Fantome39SheetD", (0.62, 0.68, 0.82))
    dark = material("Fantome39Dark", (0.05, 0.05, 0.09))

    # Tête-drap en goutte inversée, lévite (rien sous z=0,30 : il flotte).
    sphere("Body", sheet, (0, 0.0, 1.30), (0.48, 0.50, 0.55))
    sphere("Body", sheet, (0, 0.05, 0.95), (0.42, 0.44, 0.40))
    # Yeux creux + bouche en O.
    for sx in (-1, 1):
        sphere("Body", dark, (sx * 0.18, -0.42, 1.42), (0.09, 0.05, 0.12))
    sphere("Body", dark, (0, -0.46, 1.16), (0.07, 0.04, 0.09))
    # Petits bras-moignons (os ArmL/ArmR).
    for bone, sx in (("ArmL", -1), ("ArmR", 1)):
        sphere(bone, sheet, (sx * 0.52, -0.05, 1.15), (0.16, 0.14, 0.12))
        sphere(bone, sheet_d, (sx * 0.68, -0.10, 1.05), (0.11, 0.10, 0.09))
    # Traîne effilochée : trois pointes qui pendent (os Tail).
    sphere("Tail", sheet_d, (0, 0.10, 0.62), (0.34, 0.36, 0.28))
    for sx, y in ((-0.20, 0.0), (0.05, 0.18), (0.22, -0.08)):
        cone("Tail", sheet_d, (sx, 0.10 + y, 0.42), (0.10, 0.10, 0.16),
             rotation=(math.radians(180), 0, 0))

    bones = {
        "Body": ("Root", (0, 0.30, 1.25), (0, -0.30, 1.35)),
        "ArmL": ("Body", (-0.40, -0.05, 1.20), (-0.75, -0.12, 1.00)),
        "ArmR": ("Body", (0.40, -0.05, 1.20), (0.75, -0.12, 1.00)),
        "Tail": ("Body", (0, 0.08, 0.90), (0, 0.12, 0.30)),
    }

    def idle(key_rot, key_loc):
        # Lévitation : houle verticale ample, la traîne ondule à contretemps,
        # les bras flottent comme dans un courant.
        for f, dz in ((1, 0.0), (13, 0.12), (27, -0.04), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, 0.20), (13, -0.15), (27, 0.20), (40, 0.20)):
            key_rot("Tail", f, (sw, 0, sw * 0.5))
        for f, a in ((1, 0.15), (20, -0.20), (40, 0.15)):
            key_rot("ArmL", f, (0, 0, a))
            key_rot("ArmR", f, (0, 0, -a))
        for f, roll in ((1, 0.05), (20, -0.05), (40, 0.05)):
            key_rot("Body", f, (0, roll, 0))

    def walk(key_rot, key_loc):
        # Glisse hantée : penché vers l'avant, la traîne fouette derrière,
        # houle plus rapide — il « nage » dans l'air.
        for f, lean in ((1, 0.18), (13, 0.24), (24, 0.18)):
            key_rot("Body", f, (lean, 0, 0))
        for f, dz in ((1, 0.0), (7, 0.08), (13, 0.0), (19, 0.08), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, sw in ((1, -0.30), (13, 0.30), (24, -0.30)):
            key_rot("Tail", f, (0.15, 0, sw))
        for f, a in ((1, -0.20), (13, 0.20), (24, -0.20)):
            key_rot("ArmL", f, (a, 0, 0.10))
            key_rot("ArmR", f, (-a, 0, -0.10))

    build_creature("creature39", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 40 — Plante carnivore : tige hypnotique, mâchoire qui claque.
# =============================================================================
def plante():
    fresh_scene()
    leaf = material("Plante40Leaf", (0.22, 0.48, 0.18))
    leaf_d = material("Plante40LeafD", (0.14, 0.34, 0.12))
    maw = material("Plante40Maw", (0.75, 0.22, 0.42))
    ivory = material("Plante40Ivory", (0.93, 0.91, 0.80))
    dark = material("Plante40Dark", (0.06, 0.07, 0.05))

    # Rosette de feuilles au sol (base à z 0,06) + bulbe.
    for deg in range(0, 360, 60):
        rad = math.radians(deg)
        ux, uy = math.sin(rad), math.cos(rad)
        sphere("Body", leaf_d, (ux * 0.42, uy * 0.42, 0.14), (0.26, 0.26, 0.09))
    sphere("Body", leaf, (0, 0, 0.30), (0.34, 0.34, 0.26))  # bulbe
    # Tige (os Neck) : chaîne de sphères qui se chevauchent — plus fiable
    # visuellement qu'un cylindre incliné (constaté : tige « invisible »).
    for y, z, r in ((-0.02, 0.55, 0.15), (-0.06, 0.80, 0.13), (-0.11, 1.05, 0.12),
                    (-0.16, 1.28, 0.11), (-0.22, 1.48, 0.11)):
        sphere("Neck", leaf, (0, y, z), (r, r, r * 1.4))
    # Gueule : lèvre haute (os Head) + mâchoire basse (os Jaw), dents, lobes.
    sphere("Head", maw, (0, -0.42, 1.68), (0.30, 0.36, 0.22))
    sphere("Head", leaf, (0, -0.28, 1.80), (0.28, 0.34, 0.16))  # dos du capuchon
    sphere("Jaw", maw, (0, -0.48, 1.42), (0.26, 0.32, 0.14))
    sphere("Jaw", leaf, (0, -0.36, 1.32), (0.24, 0.30, 0.10))
    for sx in (-1, 1):
        sphere("Head", dark, (sx * 0.14, -0.60, 1.76), (0.045, 0.035, 0.05))
        for dx in (0.06, 0.18):
            cone("Head", ivory, (sx * dx, -0.70, 1.60), (0.03, 0.03, 0.07),
                 rotation=(math.radians(180), 0, 0))
            cone("Jaw", ivory, (sx * dx, -0.72, 1.50), (0.03, 0.03, 0.07))

    bones = {
        "Body": ("Root", (0, 0.30, 0.25), (0, -0.20, 0.35)),
        "Neck": ("Body", (0, 0.0, 0.45), (0, -0.30, 1.55)),
        "Head": ("Neck", (0, -0.30, 1.55), (0, -0.60, 1.85)),
        "Jaw": ("Neck", (0, -0.30, 1.50), (0, -0.60, 1.30)),
    }

    def idle(key_rot, key_loc):
        # Danse hypnotique : la tige dessine des cercles lents, la gueule
        # s'entrouvre puis CLAQUE d'un coup sec (frames serrées).
        for f in (1, 40):
            key_loc("Body", f, (0, 0, 0))
        for f, sx, sy in ((1, 0.22, 0.0), (11, 0.0, 0.18), (21, -0.22, 0.0),
                          (31, 0.0, -0.14), (40, 0.22, 0.0)):
            key_rot("Neck", f, (sy, 0, sx))
        for f, open_ in ((1, 0.10), (18, 0.45), (24, 0.45), (26, 0.0),
                         (32, 0.10), (40, 0.10)):
            key_rot("Jaw", f, (-open_, 0, 0))
        for f, tilt in ((1, 0.0), (18, -0.15), (26, 0.05), (40, 0.0)):
            key_rot("Head", f, (tilt, 0, 0))

    def walk(key_rot, key_loc):
        # « Marche » de plante : le pied bulbeux sautille (bonds courts), la
        # tige fouette d'avant en arrière, la gueule claque au rythme.
        for f, dz in ((1, 0.0), (7, 0.12), (13, 0.0), (19, 0.12), (24, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f, whip in ((1, 0.20), (13, -0.20), (24, 0.20)):
            key_rot("Neck", f, (whip, 0, 0.08))
        for f, open_ in ((1, 0.30), (7, 0.0), (13, 0.30), (19, 0.0), (24, 0.30)):
            key_rot("Jaw", f, (-open_, 0, 0))
        for f, tilt in ((1, -0.08), (13, 0.08), (24, -0.08)):
            key_rot("Head", f, (tilt, 0, 0))

    build_creature("creature40", bones, idle, walk, cam=0.95)


# =============================================================================
# Créature 41 — Slime : goutte gélatineuse, rebond avec écrasement-étirement.
# =============================================================================
def slime():
    fresh_scene()
    gel = material("Slime41Gel", (0.25, 0.55, 0.80), roughness=0.25)
    gel_d = material("Slime41GelD", (0.15, 0.38, 0.62), roughness=0.3)
    shine = material("Slime41Shine", (0.85, 0.93, 1.0), roughness=0.2)
    dark = material("Slime41Dark", (0.05, 0.06, 0.10))

    # Goutte : base large (os Body) + calotte (os Cap) qui s'écrase dessus.
    sphere("Body", gel, (0, 0, 0.42), (0.62, 0.62, 0.40))
    sphere("Cap", gel, (0, 0, 0.72), (0.48, 0.48, 0.38))
    sphere("Cap", gel_d, (0, 0.08, 0.66), (0.30, 0.30, 0.24))  # noyau visible
    sphere("Cap", shine, (-0.18, -0.28, 0.92), (0.10, 0.07, 0.07))  # reflet
    # Yeux + bouche sur la calotte (ils suivent la déformation du rebond).
    for sx in (-1, 1):
        sphere("Cap", dark, (sx * 0.16, -0.42, 0.78), (0.07, 0.04, 0.09))
    sphere("Cap", dark, (0, -0.46, 0.58), (0.10, 0.05, 0.04))
    # Gouttelettes satellites au sol.
    for sx, y in ((-0.62, -0.25), (0.58, 0.30), (0.15, 0.62)):
        sphere("Body", gel_d, (sx, y, 0.10), (0.10, 0.10, 0.08))

    bones = {
        "Body": ("Root", (0, 0.35, 0.35), (0, -0.30, 0.40)),
        "Cap": ("Body", (0, 0.0, 0.55), (0, 0.0, 1.05)),
    }

    def idle(key_rot, key_loc):
        # Tremblote : la calotte s'affaisse et rebondit mollement, penche d'un
        # côté puis de l'autre — de la gélatine posée sur une assiette.
        for f, dz in ((1, 0.0), (10, -0.10), (20, 0.06), (30, -0.04), (40, 0.0)):
            key_loc("Cap", f, (0, dz, 0))
        for f, tilt in ((1, 0.08), (20, -0.08), (40, 0.08)):
            key_rot("Cap", f, (tilt * 0.5, 0, tilt))
        for f, dz in ((1, 0.0), (20, 0.02), (40, 0.0)):
            key_loc("Body", f, (0, dz, 0))
        for f in (1, 40):
            key_rot("Body", f, (0, 0, 0))

    def walk(key_rot, key_loc):
        # Rebond : accroupi (squash), détente (stretch, tout le corps saute),
        # atterrissage écrasé — le cycle de saut du slime en 24 frames.
        for f, dz in ((1, -0.06), (6, 0.02), (11, 0.22), (17, 0.06), (21, -0.10),
                      (24, -0.06)):
            key_loc("Body", f, (0, dz, 0))
        for f, dz in ((1, -0.16), (6, 0.06), (11, 0.14), (17, 0.02), (21, -0.20),
                      (24, -0.16)):
            key_loc("Cap", f, (0, dz, 0))
        for f, lean in ((1, 0.06), (11, -0.10), (21, 0.10), (24, 0.06)):
            key_rot("Cap", f, (lean, 0, 0))
        for f, lean in ((1, 0.02), (11, -0.05), (21, 0.05), (24, 0.02)):
            key_rot("Body", f, (lean, 0, 0))

    build_creature("creature41", bones, idle, walk, cam=0.85)


dragonnet()
golem()
fantome()
plante()
slime()
print("PACK DONE")
