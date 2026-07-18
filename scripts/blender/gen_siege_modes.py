# Sprint 3 du pack « siège du hameau » (creation3DBlendersuite.md) : props des
# modes de jeu lot 1 — 6 assets directement motivés par le GDD (mode Escorte,
# feu communal, arme « Boulet », signal de vague, boss « Aînée de la lande »,
# mode Survie). Réutilise la palette hamlet_common.py sans nouvelle teinte.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_modes.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    CLOTH,
    METAL,
    METAL_DARK,
    STONE,
    STONE_DARK,
    WOOD,
    WOOD_DARK,
    banner,  # noqa: F401  (gardé pour d'éventuels futurs usages non animés)
    blob,
    cone,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)
from siege_anim_common import (  # noqa: E402
    FLAME_PATTERNS,
    build_banner_geo,
    build_prop,
    weight,
    weight_remaining,
)


def gen_ember_cart():
    """Chariot de braises : convoi du mode Escorte (§4/§9.3) — mêmes
    proportions générales que hamlet_cart (caisse/roues/brancards) mais
    caisse plus haute pour porter un foyer, grille métallique de retenue et
    braises émissives (variante thématique, pas la charrette générique).
    Animé (addendum) : Root + RoueGauche + RoueDroite, les roues tournent en
    continu (vitesse constante), clip Walk 24f (le chariot avance dans le
    mode Escorte)."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    metal = mat("metal", METAL)
    fire = mat("braise", (0.85, 0.35, 0.10), emission=1.6)
    cube("Caisse", wood, (0, 0, 0.75), (1.9, 1.1, 0.9))
    cube("Ridelle", dark, (0, 0, 1.28), (1.9, 1.1, 0.12))
    for i in range(3):
        x = -0.7 + i * 0.7
        cube(f"Grille{i}", metal, (x, 0, 1.30), (0.05, 1.1, 0.05))
    wheel_bones = {}
    wheel_objs = []
    for sy, bone in ((-1, "RoueGauche"), (1, "RoueDroite")):
        w = cylinder(f"Roue{sy}", dark, (0.0, sy * 0.62, 0.42), radius=0.42, depth=0.10,
                      vertices=12, rotation=(math.pi / 2, 0, 0))
        moyeu = cylinder(f"Moyeu{sy}", metal, (0.0, sy * 0.62, 0.42), radius=0.10, depth=0.13,
                          vertices=8, rotation=(math.pi / 2, 0, 0))
        weight(w, bone)
        weight(moyeu, bone)
        wheel_objs += [w, moyeu]
        # Os aligné sur l'axe de la roue (le long de Y, l'essieu) : la
        # rotation locale Y du pose-bone tourne alors la roue autour de son
        # propre axe, pas autour d'un axe arbitraire (cf. prototype testé
        # avant intégration : rotation Y d'un os pointant selon Y).
        wheel_bones[bone] = ("Root", (0.0, sy * 0.62, 0.42), (0.0, sy * 0.62 + (0.1 if sy < 0 else -0.1), 0.42))
        cube(f"Brancard{sy}", dark, (-1.35, sy * 0.38, 0.58), (1.0, 0.08, 0.08))
    for i, (x, y) in enumerate([(-0.4, -0.2), (0.1, 0.25), (0.45, -0.1), (-0.1, 0.05)]):
        blob(f"Braise{i}", fire, (x, y, 1.28), radius=0.14, squash=1.1, jitter=0.03)
    weight_remaining(wheel_objs)

    def keyer(key_rot, key_loc, key_scale):
        for bone in ("RoueGauche", "RoueDroite"):
            key_rot(bone, 1, (0, 0, 0))
            key_rot(bone, 24, (0, math.tau, 0))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_ember_cart", parts, wheel_bones, "Walk", keyer,
                linear_bones=("RoueGauche", "RoueDroite"))


def gen_communal_brazier():
    """Brasero communal : pièce signature de la place (§2.1/§10.1), plus
    ouvragée que hamlet_bonfire (socle de pierre + vasque évasée) — la
    fiction en fait le foyer central du hameau. Animé (addendum) : Root +
    Flamme1..3, vacillement d'échelle déphasé, clip Idle 40f."""
    stone = mat("pierre", STONE)
    metal_dark = mat("metal_sombre", METAL_DARK)
    fire = mat("flamme", (0.85, 0.35, 0.10), emission=1.6)
    cylinder("Socle", stone, (0, 0, 0.35), radius=0.35, depth=0.7, vertices=10)
    cylinder("Col", metal_dark, (0, 0, 0.78), radius=0.16, depth=0.16, vertices=10)
    # Vasque évasée : cône dont le rayon du haut (radius2) dépasse celui du
    # bas — silhouette de coupe qui s'ouvre vers le haut, pas un cône pointu.
    cone("Vasque", metal_dark, (0, 0, 1.05), radius=0.22, depth=0.35, vertices=10, radius2=0.5)
    cylinder("VasqueRebord", metal_dark, (0, 0, 1.22), radius=0.5, depth=0.05, vertices=10)
    flame_objs = []
    flame_bones = {}
    for i, (x, y, r) in enumerate([(0.0, 0.0, 0.26), (0.1, -0.08, 0.16), (-0.12, 0.06, 0.14)]):
        z = 1.35 + r * 0.6
        f = blob(f"Flamme{i + 1}", fire, (x, y, z), radius=r, squash=1.5, jitter=0.05)
        bone = f"Flamme{i + 1}"
        weight(f, bone)
        flame_objs.append(f)
        flame_bones[bone] = ("Root", (x, y, z - r * 0.4), (x, y, z + r * 0.4))
    weight_remaining(flame_objs)

    def keyer(key_rot, key_loc, key_scale):
        for i, bone in enumerate(flame_bones):
            for f, s in FLAME_PATTERNS[i % len(FLAME_PATTERNS)]:
                key_scale(bone, f, (s, s, s))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_communal_brazier", parts, flame_bones, "Idle", keyer)


def gen_cannonball():
    """Boulet : projectile visuel de l'arme nommée « Boulet » (§5.1),
    distinct des icônes item_* — sphère de fonte simple, complexité faible."""
    metal_dark = mat("metal_sombre", METAL_DARK)
    blob("Boulet", metal_dark, (0, 0, 0.18), radius=0.18, squash=1.0, jitter=0.015)
    export("siege_cannonball.glb")


def gen_wave_banner():
    """Bannière de vague : change d'état visuel selon la progression (§17.5)
    — la variation d'état se règle côté matériau en jeu (hors scope asset).
    Animée (addendum) : Root + Tissu1 + Tissu2 (deux segments empilés,
    charnière verticale côté poteau — reprise de la technique nature_banner),
    ondulation déphasée entre les deux segments, clip Idle 40f."""
    cloth = mat("tissu", CLOTH)
    pole_mat = mat("bois_sombre", WOOD_DARK)
    pole, segs, bones = build_banner_geo(cloth, pole_mat, width=0.7, height=1.4, n_segments=2)
    for obj, bone in segs:
        weight(obj, bone)
    weight_remaining([pole] + [obj for obj, _ in segs])

    def keyer(key_rot, key_loc, key_scale):
        for f, a in ((1, 0), (13, 10), (27, -14), (40, 0)):
            key_rot("Tissu1", f, (0, 0, math.radians(a)))
        for f, a in ((1, 0), (10, -8), (24, 12), (33, -6), (40, 0)):
            key_rot("Tissu2", f, (0, 0, math.radians(a)))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_wave_banner", parts, bones, "Idle", keyer)


def gen_elder_altar():
    """Autel de l'Aînée : socle de mise en scène du boss (§4) — trois
    plateformes de pierre décroissantes surmontées d'un monolithe sombre,
    silhouette imposante mais complexité moyenne (pas de détail fin, l'objet
    se voit de loin)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    metal_dark = mat("metal_sombre", METAL_DARK)
    base_w, base_d, step_h, steps = 2.4, 2.4, 0.22, 3
    for i in range(steps):
        w = base_w * (1 - 0.22 * i)
        d = base_d * (1 - 0.22 * i)
        z_bottom = step_h * i
        cube(f"Plateforme{i}", stone if i % 2 == 0 else stone_dark,
             (0, 0, z_bottom + step_h / 2), (w, d, step_h))
    monolith_h = 1.6
    monolith_z0 = steps * step_h
    cube("Monolithe", stone_dark, (0, 0, monolith_z0 + monolith_h / 2), (0.5, 0.35, monolith_h))
    cylinder("Coupelle", metal_dark, (0, 0, monolith_z0 + monolith_h + 0.04),
              radius=0.28, depth=0.08, vertices=10)
    export("siege_elder_altar.glb")


def gen_trophy_pile():
    """Tas de trophées : repère de progression du mode Survie — monticule de
    pierre sombre hérissé d'armes plantées à la verticale (hampes + lames) et
    d'un bouclier posé, complexité faible. Hampes verticales (pas de tilt) :
    une inclinaison à deux axes désalignait visuellement lame/hampe, chacune
    tournant autour de son propre centre plutôt que d'un pivot commun à la
    base — plus simple et plus lisible à la verticale."""
    mound = mat("pierre_sombre", STONE_DARK)
    blade_m = mat("metal", METAL)
    haft_m = mat("bois_sombre", WOOD_DARK)
    blob("Monticule", mound, (0, 0, 0.16), radius=0.34, squash=0.75, jitter=0.05)
    stakes = [(-0.16, 0.06, 0.38), (0.14, -0.08, 0.46), (0.0, 0.16, 0.32)]
    for i, (x, y, h) in enumerate(stakes):
        cylinder(f"Hampe{i}", haft_m, (x, y, h / 2 + 0.2), radius=0.025, depth=h, vertices=6)
        cube(f"Lame{i}", blade_m, (x, y, h + 0.2 + 0.12), (0.05, 0.14, 0.24))
    cylinder("Bouclier", blade_m, (0.18, 0.12, 0.32), radius=0.16, depth=0.03, vertices=8,
              rotation=(math.radians(70), 0, 0.4))
    export("siege_trophy_pile.glb")


ASSETS = [
    gen_ember_cart,
    gen_communal_brazier,
    gen_cannonball,
    gen_wave_banner,
    gen_elder_altar,
    gen_trophy_pile,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_modes] pack complet : {len(ASSETS)} fichiers")
