# Sprint 2 du pack « siège du hameau » (creation3DBlendersuite.md) :
# fortifications lot 2 — 5 assets, complexité faible à moyenne. Suite directe
# de gen_siege_walls.py (mêmes constantes de dimension, même palette).
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_walls2.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    METAL_DARK,
    STONE,
    STONE_DARK,
    WOOD,
    WOOD_DARK,
    crenellations,
    cube,
    export,
    mat,
    reset_scene,
)
from siege_anim_common import build_prop, weight, weight_remaining  # noqa: E402

WALL_H = 3.0
WALL_T = 0.6
MERLON_H = 0.4


def gen_crenel_module():
    """Module de créneau autonome (dalle + merlons) — pièce de kit destinée à
    coiffer une longueur de mur arbitraire, indépendamment du corps de mur
    (contrairement à siege_wall_segment qui inclut déjà son propre créneau)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    width = 2.0
    cube("Cren_Dalle", stone, (0, 0, 0.1), (width, WALL_T * 1.05, 0.2))
    crenellations("Cren_", stone_dark, width, 3, base_z=0.2, depth=WALL_T, merlon_h=MERLON_H)
    export("siege_crenel_module.glb")


def gen_rampart_walk():
    """Chemin de ronde : plateau de bois en encorbellement (corbeaux de
    pierre) + garde-corps côté extérieur — longe l'intérieur des remparts à
    hauteur ~2,2 m (cf. box_seg "Chemin de ronde" de hameau_gdd_demo)."""
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    width, depth = 4.0, 1.4
    corbel_h, plank_h = 0.5, 0.12
    for i, x in enumerate((-width / 2 + 0.4, 0.0, width / 2 - 0.4)):
        cube(f"Corbeau{i}", stone_dark, (x, 0, corbel_h / 2), (0.3, depth * 0.5, corbel_h))
    cube("Plateau", wood, (0, 0, corbel_h + plank_h / 2), (width, depth, plank_h))
    n_posts = 5
    rail_y = depth / 2 - 0.05
    rail_z0 = corbel_h + plank_h
    for i in range(n_posts):
        x = -width / 2 + 0.4 + i * (width - 0.8) / (n_posts - 1)
        cube(f"RailPoteau{i}", wood_dark, (x, rail_y, rail_z0 + 0.45), (0.06, 0.06, 0.9))
    cube("RailBarre", wood_dark, (0, rail_y, rail_z0 + 0.85), (width, 0.05, 0.06))
    export("siege_rampart_walk.glb")


def gen_rampart_stairs():
    """Escalier de rempart : version plus large (2,2 m) de hamlet_stairs,
    pour l'accès des défenseurs au chemin de ronde."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    n, step_h, step_d, width = 7, 0.18, 0.34, 2.2
    for i in range(n):
        x = i * step_d
        h = (i + 1) * step_h
        cube(f"Marche{i}", stone if i % 2 == 0 else stone_dark, (x, 0, h / 2), (step_d, width, h))
    export("siege_rampart_stairs.glb")


def gen_postern():
    """Poterne : petite porte secondaire discrète (0,9×1,7 m), vantail
    unique — contraste volontaire avec la porte principale (siege_gate_*).
    Animée (addendum creationAnimation3DBlendersuite.md) : Root + Vantail,
    le battant pivote depuis le jambage gauche (charnière verticale)."""
    stone = mat("pierre", STONE)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    metal_dark = mat("ferrure", METAL_DARK)
    w, h = 0.9, 1.7
    jamb_w = 0.16
    hinge_x = -(w / 2 - jamb_w)
    for sx in (-1, 1):
        cube(f"Jambage{sx}", stone, (sx * (w / 2 - jamb_w / 2), 0, h / 2), (jamb_w, WALL_T, h))
    cube("Linteau", stone, (0, 0, h + 0.08), (w, WALL_T * 1.05, 0.16))
    leaf = cube("Vantail", wood_dark, (0, 0.06, (h - 0.16) / 2), (w - jamb_w * 2 - 0.04, 0.1, h - 0.16))
    weight(leaf, "Vantail")
    handle = cube("Poignee", metal_dark, (w / 2 - jamb_w - 0.1, 0.12, (h - 0.16) / 2), (0.05, 0.05, 0.14))
    weight(handle, "Vantail")
    bones = {"Vantail": ("Root", (hinge_x, 0, 0), (hinge_x, 0, 0.3))}
    weight_remaining([leaf, handle])

    def keyer(key_rot, key_loc, key_scale):
        for f, a in ((1, 0), (15, 80), (30, 0)):
            key_rot("Vantail", f, (0, 0, math.radians(a)))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_postern", parts, bones, "Idle", keyer)


def gen_bastion():
    """Bastion de renfort : contrefort d'angle massif, silhouette étagée
    (blocs décroissants) plaquée contre un pan de mur."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    base_w, base_d, height, steps = 1.8, 1.2, 3.2, 4
    step_h = height / steps
    for i in range(steps):
        w = base_w * (1 - 0.18 * i)
        d = base_d * (1 - 0.15 * i)
        # Base de chaque bloc flush sur le sommet du précédent (pas de
        # chevauchement symétrique : un facteur >1 centré sur step_h*i+step_h/2
        # ferait passer la base du premier bloc sous z=0).
        z_bottom = step_h * i
        block_h = step_h * 1.02
        cube(f"Bastion{i}", stone if i % 2 == 0 else stone_dark,
             (0, 0, z_bottom + block_h / 2), (w, d, block_h))
    export("siege_bastion.glb")


ASSETS = [
    gen_crenel_module,
    gen_rampart_walk,
    gen_rampart_stairs,
    gen_postern,
    gen_bastion,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_walls2] pack complet : {len(ASSETS)} fichiers")
