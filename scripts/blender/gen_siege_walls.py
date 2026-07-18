# Sprint 1 du pack « siège du hameau » (creation3DBlendersuite.md) :
# fortifications lot 1 — 5 assets, complexité moyenne à haute. Premier lot de
# génération après le Sprint 0 (helpers crenellations()/banner() ajoutés à
# hamlet_common.py, testés isolément).
#
# Objectif : habiller les remparts du hameau, aujourd'hui des `box_seg` à
# couleur plate (WALL_COLOR = [0.34, 0.33, 0.36], src/scene/demos.rs:6555-6650)
# sans assise de pierre ni créneau. Réutilise intégralement la palette et les
# helpers de hamlet_common.py (aucune nouvelle teinte) : les remparts doivent
# se répondre visuellement avec les bâtiments du hameau (même pierre).
#
# Dimensions calées sur les constantes réelles de hameau_gdd_demo (WALL_H=3.0,
# WALL_T=0.6, GATE_HALF=2.5 -> largeur de porte 5.0) pour que les assets
# s'intègrent sans re-échelle le jour de l'intégration (hors scope ici).
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_walls.py
#
# Sortie : assets/models/siege_*.glb — voir hamlet_common.py pour les
# contraintes moteur (mesh joint, base_color_factor seul, sol z=0, Y-up) et la
# mémoire projet `charte-graphique-assets-maison` pour la charte complète.

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    METAL_DARK,
    STONE,
    STONE_DARK,
    WOOD_DARK,
    banner,  # noqa: F401  (pas utilisé dans ce lot, réutilisé au Sprint 5)
    blob,
    crenellations,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
    stone_coursing,
)
from siege_anim_common import FLAME_PATTERNS, build_prop, weight, weight_remaining  # noqa: E402

WALL_H = 3.0
WALL_T = 0.6
MERLON_N = 5
MERLON_H = 0.4
GATE_W = 5.0


def crenellations_y(prefix, mat_, length, n, base_z, location_xy=(0.0, 0.0),
                     depth=0.3, merlon_h=0.3):
    """Variante de hamlet_common.crenellations pour un mur orienté le long de
    Y (bras du segment d'angle) — même logique (2n-1 unités égales), juste
    l'axe de répartition qui change. Ne justifie pas de généraliser
    crenellations() elle-même (un seul appelant, l'angle)."""
    unit = length / (2 * n - 1)
    x0, y0 = location_xy
    for i in range(n):
        y = y0 - length / 2 + unit * (2 * i + 0.5)
        cube(f"{prefix}Merlon{i}", mat_, (x0, y, base_z + merlon_h / 2),
             (depth, unit * 0.96, merlon_h))


def coursing_arm_y(prefix, base_mat, mortar_mat, location, length, height, depth, rows=4):
    """hamlet_common.stone_coursing tourné de 90° (bras le long de Y) — pour
    le segment d'angle dont un bras part perpendiculairement à l'autre."""
    x0, y0, z0 = location
    cube(f"{prefix}Mur", base_mat, location, (depth, length, height))
    step = height / rows
    for i in range(1, rows):
        z = z0 - height / 2 + i * step
        cube(f"{prefix}Assise{i}", mortar_mat,
             (x0 + depth / 2 * 0.97, y0, z), (depth * 0.06, length * 0.99, 0.025))


def gen_wall_segment():
    """Pan de rempart ~4×3 m, assises de pierre + créneau — remplace un
    `box_seg` plat du hameau. Largeur module réutilisable par répétition dans
    la scène (hors scope ici)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    width = 4.0
    stone_coursing("Seg_", stone, stone_dark, (0, 0, WALL_H / 2), width, WALL_H, WALL_T, rows=4)
    crenellations("Seg_", stone_dark, width, MERLON_N, WALL_H, depth=WALL_T, merlon_h=MERLON_H)
    export("siege_wall_segment.glb")


def gen_wall_corner():
    """Coin de rempart : deux bras d'assises perpendiculaires + pilier
    d'angle comblant la jonction, créneau continu sur les deux bras."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    arm = 3.4
    stone_coursing("CoinX_", stone, stone_dark, (arm / 2, 0, WALL_H / 2), arm, WALL_H, WALL_T, rows=4)
    coursing_arm_y("CoinY_", stone, stone_dark, (0, arm / 2, WALL_H / 2), arm, WALL_H, WALL_T, rows=4)
    pillar_h = WALL_H * 1.03  # légèrement plus haut que les bras pour marquer l'angle
    cube("CoinPilier", stone_dark, (0, 0, pillar_h / 2), (WALL_T * 1.15, WALL_T * 1.15, pillar_h))
    crenellations("CoinX_", stone_dark, arm, 4, WALL_H, location_xy=(arm / 2, 0),
                  depth=WALL_T, merlon_h=MERLON_H)
    crenellations_y("CoinY_", stone_dark, arm, 4, WALL_H, location_xy=(0, arm / 2),
                     depth=WALL_T, merlon_h=MERLON_H)
    cube("CoinMerlon", stone_dark, (0, 0, pillar_h + MERLON_H / 2), (WALL_T * 1.15, WALL_T * 1.15, MERLON_H))
    export("siege_wall_corner.glb")


def gen_tower():
    """Tour d'angle ronde + plateforme de tir crénelée — silhouette la plus
    massive du lot, pour marquer les angles du hameau."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    radius, height = 1.8, 4.5
    cylinder("TourCorps", stone, (0, 0, height / 2), radius=radius, depth=height, vertices=12)
    rows = 4
    for i in range(1, rows):
        z = i * height / rows
        cylinder(f"TourAssise{i}", stone_dark, (0, 0, z), radius=radius * 1.02, depth=0.08, vertices=12)
    plat_r = radius * 1.15
    cylinder("TourPlateforme", stone_dark, (0, 0, height + 0.05), radius=plat_r, depth=0.1, vertices=12)
    # Créneau circulaire : un merlon sur deux des 16 secteurs, pour alterner
    # bloc/créneau autour de la plateforme (même principe que crenellations,
    # ici en polaire faute de largeur droite).
    n_slots = 16
    for i in range(n_slots):
        if i % 2 != 0:
            continue
        theta = 2 * math.pi * i / n_slots
        x, y = plat_r * math.cos(theta), plat_r * math.sin(theta)
        m = cube(f"TourMerlon{i}", stone_dark, (x, y, height + 0.1 + MERLON_H / 2),
                  (0.36, 0.36, MERLON_H))
        m.rotation_euler = (0, 0, theta)
    export("siege_tower.glb")


def _gate_body(prefix, door_mat, burning):
    """Corps commun aux deux portes (fermée/embrasée) — même géométrie de
    jambages/linteau/vantaux/créneau, seul le matériau des vantaux (et l'ajout
    de flammes) change selon `burning`. Animé (addendum
    creationAnimation3DBlendersuite.md) : Root + VantailGauche + VantailDroit
    (+ Flamme1/Flamme2 pour la variante embrasée), les vantaux pivotent depuis
    le jambage (charnière verticale)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    metal_dark = mat("ferrure", METAL_DARK)
    opening_w = 3.6
    jamb_w = (GATE_W - opening_w) / 2
    for sx in (-1, 1):
        cube(f"{prefix}Jambage{sx}", stone,
             (sx * (opening_w / 2 + jamb_w / 2), 0, WALL_H / 2), (jamb_w, WALL_T, WALL_H))
    cube(f"{prefix}Linteau", stone_dark, (0, 0, WALL_H - 0.15), (GATE_W, WALL_T * 1.05, 0.3))
    door_objs = {}
    animated_objs = []  # tout objet déjà pondéré à un os non-Root (à exclure de weight_remaining)
    for sx, bone in ((-1, "VantailGauche"), (1, "VantailDroit")):
        leaf = cube(f"{prefix}Vantail{sx}", door_mat,
                     (sx * opening_w / 4, 0.08, (WALL_H - 0.3) / 2), (opening_w / 2 * 0.96, 0.12, WALL_H - 0.3))
        weight(leaf, bone)
        animated_objs.append(leaf)
        for i in range(3):
            z = 0.4 + i * 0.9
            f = cube(f"{prefix}Ferrure{sx}_{i}", metal_dark,
                       (sx * opening_w / 4, 0.15, z), (opening_w / 2 * 0.8, 0.03, 0.08))
            weight(f, bone)
            animated_objs.append(f)
        door_objs[bone] = leaf
    crenellations(prefix, stone_dark, GATE_W, MERLON_N, WALL_H, depth=WALL_T, merlon_h=MERLON_H)
    flame_objs = {}
    if burning:
        fire = mat("flamme", (0.85, 0.35, 0.10), emission=2.2)
        clusters = {"Flamme1": [(-0.9, WALL_H - 0.6, 0.28), (0.0, WALL_H - 0.9, 0.22)],
                    "Flamme2": [(0.7, WALL_H - 0.4, 0.24), (1.3, WALL_H - 0.9, 0.20)]}
        for bone, spots in clusters.items():
            objs = []
            for i, (x, z, r) in enumerate(spots):
                b = blob(f"{prefix}{bone}_{i}", fire, (x, 0.1, z), radius=r, squash=1.6, jitter=0.04)
                weight(b, bone)
                animated_objs.append(b)
                objs.append(b)
            flame_objs[bone] = objs
    bones = {"VantailGauche": ("Root", (-opening_w / 2, 0, 0), (-opening_w / 2, 0, 0.3)),
             "VantailDroit": ("Root", (opening_w / 2, 0, 0), (opening_w / 2, 0, 0.3))}
    if burning:
        bones["Flamme1"] = ("Root", (-0.45, 0.1, WALL_H - 0.7), (-0.45, 0.1, WALL_H - 0.5))
        bones["Flamme2"] = ("Root", (1.0, 0.1, WALL_H - 0.6), (1.0, 0.1, WALL_H - 0.4))
    weight_remaining(animated_objs)
    return bones, door_objs, flame_objs


def _gate_door_keyer(key_rot, key_loc, key_scale, flame_bones):
    """Idle 40f : vantaux fermés (0°) -> ouverts (±75°) -> refermés, boucle
    parfaite (pose identique aux frames 1 et 40). Flammes (porte embrasée) :
    tremblement d'échelle déphasé entre les deux foyers (motifs distincts,
    tous deux à l'échelle 1.0 aux frames 1 et 40 pour fermer la boucle)."""
    for f, a in ((1, 0), (20, 75), (40, 0)):
        key_rot("VantailGauche", f, (0, 0, math.radians(a)))
        key_rot("VantailDroit", f, (0, 0, math.radians(-a)))
    for i, bone in enumerate(flame_bones):
        for f, s in FLAME_PATTERNS[i % len(FLAME_PATTERNS)]:
            key_scale(bone, f, (s, s, s))


def gen_gate_closed():
    """Porte de rempart fermée : vantaux de bois clos, ferrures — entrée
    principale du hameau. Animée (addendum) : les 2 vantaux pivotent depuis
    leur jambage (charnière verticale)."""
    bones, door_objs, _ = _gate_body("PorteF_", mat("bois_sombre", WOOD_DARK), burning=False)
    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_gate_closed", parts, bones, "Idle",
                lambda kr, kl, ks: _gate_door_keyer(kr, kl, ks, []))


def gen_gate_burning():
    """Variante « signal de vague » (§17.5) : mêmes vantaux, matériau
    assombri + flammes émissives (vignette uniquement, cf. charte — l'émissif
    glTF est ignoré par le moteur, un vrai feu en jeu se règle côté scène).
    Animée : vantaux + tremblement des flammes (Flamme1/Flamme2)."""
    charred = mat("bois_calcine", (0.12, 0.09, 0.07))
    bones, door_objs, flame_objs = _gate_body("PorteB_", charred, burning=True)
    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_gate_burning", parts, bones, "Idle",
                lambda kr, kl, ks: _gate_door_keyer(kr, kl, ks, list(flame_objs.keys())))


ASSETS = [
    gen_wall_segment,
    gen_wall_corner,
    gen_tower,
    gen_gate_closed,
    gen_gate_burning,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_walls] pack complet : {len(ASSETS)} fichiers")
