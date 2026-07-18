# Sprint 3 (partie 2) du pack « hameau maison » (sprintcration3delement.md) :
# décor naturel + effets — 3 assets. Complète le Sprint 3 avec gen_hamlet_props2.py.
#
# Recrée en style maison, sans copier de géométrie tierce, la fonction et la
# silhouette générale de 3 pièces du Medieval Village Pack (Quaternius/CC0,
# déjà retraité en village_*.glb par import_village_pack.py) : Rocks, Bonfire,
# Smoke.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_hamlet_decor.py
#
# Point important (mémoire `charte-graphique-assets-maison`) : le moteur
# ignore le canal alpha (src/scene/import.rs:52) — la Fumée est donc un blob
# opaque stylisé, jamais un plan semi-transparent.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    FIRE,
    SMOKE,
    STONE,
    STONE_DARK,
    WOOD_DARK,
    blob,
    cylinder,
    export,
    mat,
    reset_scene,
    rng,
)


def gen_rocks():
    """Groupe de rochers ~1.0 m d'emprise : cluster de 4 blocs de tailles
    variées — distinct du rocher isolé de gen_nature_pack.gen_rock (celui-ci
    est un amas, pensé pour border une allée ou une carrière du hameau)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    rocks = [
        (-0.22, -0.10, 0.30, stone),
        (0.18, 0.05, 0.24, stone_dark),
        (-0.05, 0.25, 0.20, stone),
        (0.28, -0.20, 0.16, stone_dark),
    ]
    for i, (x, y, r, m) in enumerate(rocks):
        blob(f"Rocher{i}", m, (x, y, r * 0.7), radius=r, squash=0.75, jitter=r * 0.25)
    export("hamlet_rocks.glb")


def gen_bonfire():
    """Feu de camp ~1.0 m : bûches en croisillon (base) + flammes émissives
    (règle n°2 de la charte : l'émissif signale un vrai point d'intérêt/
    danger, pas un simple feu décoratif — cohérent, un foyer est un repère de
    halte). Assez massif pour rester visible des sondes de créature."""
    log = mat("buche", WOOD_DARK)
    # emission=1.3 uniquement pour une vignette de contrôle lisible : le
    # moteur ignore l'émissif glTF (seul base_color_factor est lu, cf.
    # src/scene/import.rs et mémoire charte-graphique-assets-maison) — la
    # vraie lueur en jeu, si besoin, se règle côté scène (obj.emissive),
    # pas dans ce matériau.
    fire = mat("flamme", FIRE, emission=1.3)
    stone = mat("pierre", STONE)
    for i in range(3):
        a = i * math.tau / 3
        x, y = 0.22 * math.cos(a), 0.22 * math.sin(a)
        cylinder(f"CercleFoyer{i}", stone, (x, y, 0.08), radius=0.10, depth=0.16, vertices=7)
    for i in range(4):
        a = i * math.tau / 4 + math.radians(20)
        x, y = 0.14 * math.cos(a), 0.14 * math.sin(a)
        cylinder(f"Buche{i}", log, (x, y, 0.14), radius=0.07, depth=0.55, vertices=6,
                 rotation=(math.pi / 2, 0, a))
    blob("Flamme1", fire, (0, 0, 0.35), radius=0.20, squash=1.4, jitter=0.05)
    blob("Flamme2", fire, (0.05, 0.03, 0.55), radius=0.12, squash=1.3, jitter=0.04)
    export("hamlet_bonfire.glb")


def gen_smoke():
    """Fumée ~1.4 m de haut : 4 blobs opaques empilés, élargis et éclaircis
    vers le haut — silhouette stylisée (le moteur n'a pas de transparence,
    cf. src/scene/import.rs:52 : aucun plan semi-transparent possible ici)."""
    puff1 = mat("fumee1", SMOKE)
    puff2 = mat("fumee2", (0.62, 0.62, 0.62))
    puff3 = mat("fumee3", (0.70, 0.70, 0.70))
    puff4 = mat("fumee4", (0.78, 0.78, 0.78))
    blob("Volute1", puff1, (0, 0, 0.22), radius=0.20, squash=0.9, jitter=0.04)
    blob("Volute2", puff2, (0.05, -0.03, 0.55), radius=0.28, squash=0.85, jitter=0.06)
    blob("Volute3", puff3, (-0.08, 0.05, 0.95), radius=0.34, squash=0.8, jitter=0.07)
    blob("Volute4", puff4, (0.10, -0.02, 1.35), radius=0.38, squash=0.75, jitter=0.08)
    export("hamlet_smoke.glb")


ASSETS = [
    gen_rocks,
    gen_bonfire,
    gen_smoke,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[hamlet_decor] pack complet : {len(ASSETS)} fichiers")
