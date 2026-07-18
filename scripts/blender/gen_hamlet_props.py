# Sprint 2 du pack « hameau maison » (sprintcration3delement.md) : mobilier /
# props, lot 1 — 10 assets, complexité faible/moyenne.
#
# Recrée en style maison, sans copier de géométrie tierce, la fonction et la
# silhouette générale de 10 pièces du Medieval Village Pack (Quaternius/CC0,
# déjà retraité en village_*.glb par import_village_pack.py) : Bag Open, Bag,
# Bags, Barrel, Bell, Bench (x2), Cart, Cauldron, Crate.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_hamlet_props.py
#
# Sortie : assets/models/hamlet_*.glb — voir hamlet_common.py pour les
# contraintes moteur et la mémoire projet `charte-graphique-assets-maison`
# pour la charte complète (palette, budget, pièges vignette déjà résolus).

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    CLOTH,
    CLOTH_DARK,
    METAL,
    METAL_DARK,
    WOOD,
    WOOD_DARK,
    blob,
    cone,
    cube,
    cylinder,
    export,
    mat,
    reset_scene,
    rng,
)


def gen_bag_open():
    """Sac ouvert ~0.55 m : corps de toile affaissé + ouverture sombre visible
    en haut (disque plat encastré, comme un puits d'ombre — pas un trou réel,
    juste une teinte plus sombre qui lit comme un intérieur creux)."""
    cloth = mat("toile", CLOTH)
    dark = mat("toile_sombre", CLOTH_DARK)
    blob("Corps", cloth, (0, 0, 0.22), radius=0.28, squash=0.75, jitter=0.05)
    cylinder("Ouverture", dark, (0, 0, 0.42), radius=0.16, depth=0.04, vertices=9)
    export("hamlet_bag_open.glb")


def gen_bag():
    """Sac fermé ~0.6 m : corps de toile + col noué (cylindre resserré) + un
    petit nœud (blob) — silhouette distincte du sac ouvert."""
    cloth = mat("toile", CLOTH)
    dark = mat("toile_sombre", CLOTH_DARK)
    blob("Corps", cloth, (0, 0, 0.22), radius=0.27, squash=0.8, jitter=0.05)
    cylinder("Col", dark, (0, 0, 0.46), radius=0.09, depth=0.14, vertices=8)
    blob("Noeud", dark, (0, 0, 0.56), radius=0.08, squash=0.7)
    export("hamlet_bag.glb")


def gen_bags():
    """Tas de 3 sacs ~0.9 m d'emprise : réutilise la silhouette du sac fermé,
    posés en tas irrégulier — pour l'arrière-boutique et les étals."""
    cloth = mat("toile", CLOTH)
    cloth2 = mat("toile_claire", (0.74, 0.64, 0.46))
    dark = mat("toile_sombre", CLOTH_DARK)
    spots = [(-0.22, -0.08, 0.20, cloth), (0.20, 0.05, 0.20, cloth2), (0.0, 0.24, 0.18, cloth)]
    for i, (x, y, r, m) in enumerate(spots):
        blob(f"Sac{i}", m, (x, y, r * 0.75), radius=r, squash=0.8, jitter=0.04)
        cylinder(f"Col{i}", dark, (x, y, r * 1.55), radius=r * 0.32, depth=r * 0.5, vertices=7)
    export("hamlet_bags.glb")


def gen_barrel():
    """Tonneau ~0.9 m : douves (cylindre) + 3 cerclages métalliques fins."""
    wood = mat("bois", WOOD)
    metal = mat("metal", METAL)
    cylinder("Douves", wood, (0, 0, 0.45), radius=0.34, depth=0.9, vertices=12)
    for z in (0.10, 0.45, 0.80):
        cylinder(f"Cerclage{z}", metal, (0, 0, z), radius=0.345, depth=0.06, vertices=12)
    export("hamlet_barrel.glb")


def gen_bell():
    """Cloche ~0.5 m : robe évasée (cône tronqué, bouche en bas) + couronne de
    suspension — accrochée sous une potence dans la scène (hors asset)."""
    metal = mat("metal_sombre", METAL_DARK)
    metal2 = mat("metal", METAL)
    cone("Robe", metal, (0, 0, 0.28), radius=0.30, depth=0.42, radius2=0.11, vertices=12)
    cylinder("Couronne", metal2, (0, 0, 0.52), radius=0.10, depth=0.08, vertices=8)
    blob("Anneau", metal2, (0, 0, 0.58), radius=0.05, squash=0.6)
    export("hamlet_bell.glb")


def gen_bench_a():
    """Banc simple ~1.4 m : assise + 2 pieds en A — silhouette la plus sobre
    du duo (place de marché, sans dossier)."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cube("Assise", wood, (0, 0, 0.42), (1.4, 0.34, 0.06))
    for sx in (-1, 1):
        cube(f"Pied{sx}", dark, (sx * 0.58, 0, 0.20), (0.06, 0.30, 0.40))
    export("hamlet_bench_a.glb")


def gen_bench_b():
    """Banc à dossier ~1.4 m : variante B du duo, avec dossier bas — casse la
    répétition quand plusieurs bancs se répondent sur la place."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cube("Assise", wood, (0, 0, 0.42), (1.4, 0.34, 0.06))
    for sx in (-1, 1):
        cube(f"Pied{sx}", dark, (sx * 0.58, 0, 0.20), (0.06, 0.30, 0.40))
    cube("Dossier", dark, (0, -0.15, 0.68), (1.4, 0.06, 0.5))
    export("hamlet_bench_b.glb")


def gen_chair():
    """Chaise simple ~0.85 m : assise + 4 pieds + dossier — ajoutée après
    coup (Sprint 7, remplacement du pack village) : la seule pièce du hameau
    encore utilisée par les scènes (`village_chair.glb`, demos.rs) sans
    équivalent maison lors des Sprints 0-6."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cube("Assise", wood, (0, 0, 0.45), (0.42, 0.42, 0.05))
    for sx in (-1, 1):
        for sy in (-1, 1):
            cube(f"Pied{sx}{sy}", dark, (sx * 0.17, sy * 0.17, 0.22), (0.05, 0.05, 0.44))
    cube("Dossier", dark, (0, -0.18, 0.75), (0.42, 0.05, 0.6))
    export("hamlet_chair.glb")


def gen_cart():
    """Charrette à bras ~2.2 m : caisse pleine (flancs visibles des sondes) +
    deux roues + brancards — variante hameau du chariot (caisse plus haute,
    planches visibles) distincte de nature_cart de gen_nature_pack.py."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    metal = mat("metal", METAL)
    cube("Caisse", wood, (0, 0, 0.75), (1.9, 1.1, 0.75))
    cube("Ridelle", dark, (0, 0, 1.18), (1.9, 1.1, 0.12))
    for sy in (-1, 1):
        cylinder(
            f"Roue{sy}", dark, (0.0, sy * 0.62, 0.42), radius=0.42, depth=0.10,
            vertices=12, rotation=(math.pi / 2, 0, 0),
        )
        cylinder(
            f"Moyeu{sy}", metal, (0.0, sy * 0.62, 0.42), radius=0.10, depth=0.13,
            vertices=8, rotation=(math.pi / 2, 0, 0),
        )
        cube(f"Brancard{sy}", dark, (-1.35, sy * 0.38, 0.58), (1.0, 0.08, 0.08))
    export("hamlet_cart.glb")


def gen_cauldron():
    """Chaudron ~0.55 m : panse arrondie de fonte + 3 pieds courts + 2 anses —
    posé au-dessus d'un feu de camp dans la scène (hors asset)."""
    metal_dark = mat("metal_sombre", METAL_DARK)
    metal = mat("metal", METAL)
    blob("Panse", metal_dark, (0, 0, 0.30), radius=0.30, squash=0.85, jitter=0.02)
    for i in range(3):
        a = i * math.tau / 3 + rng.uniform(-0.1, 0.1)
        x, y = 0.20 * math.cos(a), 0.20 * math.sin(a)
        cylinder(f"Pied{i}", metal_dark, (x, y, 0.08), radius=0.03, depth=0.16, vertices=6)
    for sx in (-1, 1):
        # x=0.24 (pas 0.30, le rayon à l'équateur) : à z=0.42 la panse s'est
        # déjà resserrée (icosphère écrasée), une anse à rayon plein flotterait
        # à côté de la panse au lieu d'y toucher.
        cube(f"Anse{sx}", metal, (sx * 0.24, 0, 0.42), (0.06, 0.06, 0.10))
    export("hamlet_cauldron.glb")


def gen_crate():
    """Caisse à claire-voie ~0.6 m : bloc de bois + croisillons de planches en
    relief — silhouette simple, réutilisée en pile ou isolée."""
    wood = mat("bois", WOOD)
    dark = mat("bois_sombre", WOOD_DARK)
    cube("Bloc", wood, (0, 0, 0.3), (0.6, 0.6, 0.6))
    for sx in (-1, 1):
        cube(f"CroixX{sx}", dark, (sx * 0.15, 0, 0.3), (0.05, 0.6, 0.6))
    for sy in (-1, 1):
        cube(f"CroixY{sy}", dark, (0, sy * 0.15, 0.3), (0.6, 0.05, 0.6))
    export("hamlet_crate.glb")


ASSETS = [
    gen_bag_open,
    gen_bag,
    gen_bags,
    gen_barrel,
    gen_bell,
    gen_bench_a,
    gen_bench_b,
    gen_chair,
    gen_cart,
    gen_cauldron,
    gen_crate,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[hamlet_props] pack complet : {len(ASSETS)} fichiers")
