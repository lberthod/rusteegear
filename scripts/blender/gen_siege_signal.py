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

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    CLOTH,
    FIRE,
    GLOW_YELLOW,
    METAL_DARK,
    STONE_DARK,
    WOOD_DARK,
    banner,
    blob,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
)


def gen_mode_banner():
    """Bannière de mode (Escorte/Boss/Survie/Vagues, §17.5 et alentours) :
    pièce générique de signalétique de mode — geometrie double pan pour se
    distinguer de siege_wave_banner (spécifique au signal de vague), teinte
    neutre changée par mode via le matériau en jeu (hors scope asset)."""
    cloth = mat("tissu", CLOTH)
    pole_mat = mat("bois_sombre", WOOD_DARK)
    banner("ModeA_", cloth, pole_mat, location=(0, 0, 0), width=0.55, height=1.1)
    banner("ModeB_", cloth, pole_mat, location=(0, 0.35, 0), width=0.55, height=0.9,
           pole_height=1.1)
    export("siege_mode_banner.glb")


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
    lueur en jeu se règle côté scène)."""
    wood_dark = mat("bois_sombre", WOOD_DARK)
    metal_dark = mat("metal_sombre", METAL_DARK)
    fire = mat("flamme", FIRE, emission=1.8)
    cube("Support", metal_dark, (0, 0.06, 0.5), (0.06, 0.12, 0.06))
    cylinder("Hampe", wood_dark, (0, 0, 0.4), radius=0.03, depth=0.7, vertices=7)
    blob("Flamme", fire, (0, 0, 0.82), radius=0.12, squash=1.5, jitter=0.04)
    export("siege_rampart_torch.glb")


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
    """Fanion de couleur d'équipe : réutilise directement le helper banner()
    (Sprint 0) — le plus petit des quatre assets à panneau, porté ou planté,
    couleur d'équipe appliquée en jeu (matériau), pas ici."""
    cloth = mat("tissu", CLOTH)
    pole_mat = mat("bois_sombre", WOOD_DARK)
    banner("Fanion_", cloth, pole_mat, location=(0, 0, 0), width=0.32, height=0.5,
           pole_height=0.9, pole_radius=0.025)
    export("siege_team_pennant.glb")


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
