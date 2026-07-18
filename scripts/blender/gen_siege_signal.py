# Sprint 5 (lot B) du pack « siège du hameau » (creation3DBlendersuite.md) :
# signalétique et effets 3D lot 1 — 5 assets, complexité faible. Réutilise
# banner() (hamlet_common.py, Sprint 0) pour les deux pièces à panneau de
# tissu (mode/équipe), FIRE/GLOW_YELLOW pour les signaux lumineux.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_siege_signal.py
#
# Sortie : assets/models/siege_*.glb.

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    CLOTH,
    FIRE,
    GLOW_YELLOW,
    METAL_DARK,
    STONE_DARK,
    WOOD_DARK,
    banner,  # noqa: F401  (gardé pour d'éventuels futurs usages non animés)
    blob,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)
from siege_anim_common import (  # noqa: E402
    build_banner_geo,
    build_prop,
    weight,
    weight_remaining,
)


def gen_mode_banner():
    """Bannière de mode (Escorte/Boss/Survie/Vagues, §17.5 et alentours) :
    pièce générique de signalétique de mode, teinte neutre changée par mode
    via le matériau en jeu (hors scope asset). Animée (addendum) : même rig
    que siege_wave_banner (Root + Tissu1 + Tissu2, un seul squelette réutilisé
    pour les 4 teintes), proportions différentes (plus large, moins haute)
    pour rester visuellement distincte du signal de vague. Clip Idle 40f."""
    cloth = mat("tissu", CLOTH)
    pole_mat = mat("bois_sombre", WOOD_DARK)
    pole, segs, bones = build_banner_geo(cloth, pole_mat, width=0.9, height=1.0, n_segments=2)
    for obj, bone in segs:
        weight(obj, bone)
    weight_remaining([pole] + [obj for obj, _ in segs])

    def keyer(key_rot, key_loc, key_scale):
        for f, a in ((1, 0), (14, -9), (28, 11), (40, 0)):
            key_rot("Tissu1", f, (0, 0, math.radians(a)))
        for f, a in ((1, 0), (9, 11), (22, -10), (34, 7), (40, 0)):
            key_rot("Tissu2", f, (0, 0, math.radians(a)))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_mode_banner", parts, bones, "Idle", keyer)


def gen_alert_horn():
    """Corne d'alerte : corne recourbée montée sur une petite console murale
    — approximation low-poly par segments de cône décroissants suivant une
    courbe, pas de géométrie organique lissée."""
    metal_dark = mat("metal_sombre", METAL_DARK)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    cube("Console", wood_dark, (0, 0, 0.55), (0.14, 0.10, 0.10))
    # Courbe approximée par une chaîne de segments cylindriques de rayon
    # décroissant, chacun orienté le long de la tangente de la courbe — pas
    # de géométrie organique lissée (cohérent avec le style low-poly).
    segs = 5
    for i in range(segs):
        t = i / (segs - 1)
        x = 0.05 + t * 0.5
        z = 0.55 + math.sin(t * math.pi * 0.6) * 0.28
        r = 0.03 + 0.05 * (1 - t)
        seg = cylinder(f"Corne{i}", metal_dark, (x, 0, z), radius=r, depth=0.14, vertices=8)
        seg.rotation_euler = (0, math.radians(90 - t * 50), 0)
    export("siege_alert_horn.glb")


def gen_rampart_torch():
    """Torche de rempart : support mural + flamme émissive (FIRE) — même
    convention que hamlet_bonfire (émissif vignette uniquement, la vraie
    lueur en jeu se règle côté scène). Animée (addendum) : Root + Flamme,
    vacillement d'échelle (version réduite du brasero), clip Idle 30f."""
    wood_dark = mat("bois_sombre", WOOD_DARK)
    metal_dark = mat("metal_sombre", METAL_DARK)
    fire = mat("flamme", FIRE, emission=1.8)
    support = cube("Support", metal_dark, (0, 0.06, 0.5), (0.06, 0.12, 0.06))
    hampe = cylinder("Hampe", wood_dark, (0, 0, 0.4), radius=0.03, depth=0.7, vertices=7)
    flamme = blob("Flamme", fire, (0, 0, 0.82), radius=0.12, squash=1.5, jitter=0.04)
    weight(flamme, "Flamme")
    weight_remaining([support, hampe, flamme])
    bones = {"Flamme": ("Root", (0, 0, 0.72), (0, 0, 0.92))}

    def keyer(key_rot, key_loc, key_scale):
        for f, s in ((1, 1.0), (8, 1.1), (15, 0.9), (22, 1.08), (30, 1.0)):
            key_scale("Flamme", f, (s, s * 0.95, s * 1.05))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_rampart_torch", parts, bones, "Idle", keyer, fps=24)


def gen_ground_marker():
    """Marqueur de zone au sol : dalle circulaire + anneau lumineux —
    variante « signalétique » de hamlet_path_straight, mais plate et ronde,
    pour délimiter une zone de jeu (spawn, objectif)."""
    stone_dark = mat("pierre_sombre", STONE_DARK)
    glow = mat("anneau", GLOW_YELLOW, emission=1.4)
    cylinder("Dalle", stone_dark, (0, 0, 0.015), radius=0.55, depth=0.03, vertices=16)
    cylinder("Anneau", glow, (0, 0, 0.032), radius=0.42, depth=0.02, vertices=16)
    export("siege_ground_marker.glb")


def gen_team_pennant():
    """Fanion de couleur d'équipe : le plus petit des assets à panneau, porté
    ou planté, couleur d'équipe appliquée en jeu (matériau), pas ici. Animé
    (addendum) : même rig bannière que siege_wave_banner, un seul segment
    (Root + Tissu), clip Idle 30f."""
    cloth = mat("tissu", CLOTH)
    pole_mat = mat("bois_sombre", WOOD_DARK)
    pole, segs, bones = build_banner_geo(cloth, pole_mat, width=0.32, height=0.5,
                                          n_segments=1, pole_height=0.9, pole_radius=0.025)
    for obj, bone in segs:
        weight(obj, bone)
    weight_remaining([pole] + [obj for obj, _ in segs])

    def keyer(key_rot, key_loc, key_scale):
        for f, a in ((1, 0), (11, 12), (22, -10), (30, 0)):
            key_rot("Tissu", f, (0, 0, math.radians(a)))

    parts = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    build_prop("siege_team_pennant", parts, bones, "Idle", keyer)


ASSETS = [
    gen_mode_banner,
    gen_alert_horn,
    gen_rampart_torch,
    gen_ground_marker,
    gen_team_pennant,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[siege_signal] pack complet : {len(ASSETS)} fichiers")
