# Sprint 5 du pack « hameau maison » (sprintcration3delement.md) : bâtiments,
# lot 2 — 6 assets, complexité haute. Réutilise les toits de
# gen_hamlet_buildings.py (pitched_roof/hip_roof) et ajoute trois nouveaux
# helpers de détail à hamlet_common.py (plank_wall, stone_coursing,
# shingled_roof) pour des façades qui montrent vraiment planches/assises de
# pierre/tuiles, pas de simples aplats — retour utilisateur après le Sprint 4.
#
# Recrée en style maison, sans copier de géométrie tierce, la fonction et la
# silhouette générale de 6 bâtiments du Medieval Village Pack (Quaternius/CC0,
# déjà retraité en village_*.glb par import_village_pack.py) : Fantasy Inn,
# Fantasy Sawmill, Fantasy Stable, Mill, Gazebo, Well.
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/gen_hamlet_buildings2.py
#
# Portes/fenêtres/enseignes : cubes en saillie (pas de découpe booléenne),
# même convention que gen_hamlet_buildings.py.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hamlet_common import (  # noqa: E402
    GLASS,
    HAY,
    METAL,
    METAL_DARK,
    ROOF,
    STONE,
    STONE_DARK,
    WOOD,
    WOOD_DARK,
    blob,
    cone,
    cube,
    cylinder,
    export,
    mat,
    plank_wall,
    reset_scene,
    rng,
    shingled_roof,
    stone_coursing,
)

ROOF_DARK = (0.40, 0.16, 0.10)  # variante sombre de ROOF pour l'alternance de tuiles


def _shutters(prefix, wood_dark, shutter_wood, glass, x, y, z):
    """Fenêtre à volets ouverts (motif réutilisé sur plusieurs bâtiments) —
    cadre + carreau + deux volets rabattus, cf. gen_hamlet_structures.gen_window_b."""
    cube(f"{prefix}Cadre", wood_dark, (x, y + 0.02, z), (0.7, 0.08, 0.8))
    cube(f"{prefix}Carreau", glass, (x, y + 0.07, z), (0.58, 0.04, 0.68))
    for sx in (-1, 1):
        v = cube(f"{prefix}Volet{sx}", shutter_wood, (x + sx * 0.5, y + 0.12, z),
                  (0.32, 0.04, 0.76))
        v.rotation_euler = (0, 0, sx * math.radians(70))


def gen_inn():
    """Auberge ~5.5×4.0 m, faîte à 4.9 m : soubassement à assises de pierre,
    étage à colombage/planches, deux fenêtres à volets, enseigne suspendue à
    une potence de fer, porte à traverses, cheminée d'angle. Le bâtiment le
    plus riche en détails du lot (auberge = cœur social du hameau)."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    glass = mat("vitre", GLASS)
    roof_m = mat("toit", ROOF)
    roof_d = mat("toit_sombre", ROOF_DARK)
    metal = mat("metal_sombre", METAL_DARK)
    sign_m = mat("enseigne", ROOF)

    stone_coursing("Socle", stone, stone_dark, (0, 0, 0.65), width=5.5, height=1.3,
                   depth=4.0, rows=4)
    plank_wall("Etage", wood, wood_dark, (0, 0, 2.25), width=5.5, height=1.9,
               depth=4.0, n_planks=9)
    cube("Porte", wood_dark, (-1.7, 2.02, 1.55), (0.9, 0.08, 1.7))
    cube("PorteTraverse", wood, (-1.7, 2.08, 1.9), (0.9, 0.03, 0.10))
    _shutters("Fen1", wood_dark, wood, glass, 0.3, 2.0, 2.55)
    _shutters("Fen2", wood_dark, wood, glass, 1.9, 2.0, 2.55)
    shingled_roof("Toit", roof_m, roof_d, span_x=5.5, depth_y=4.0, rise=1.7, base_z=3.2)
    # Cheminée d'angle, dépasse le faîte (base_z + rise ≈ 4.9).
    cube("Cheminee", stone, (2.4, -1.5, 2.6), (0.6, 0.6, 4.2))
    cube("CheminHaut", stone, (2.4, -1.5, 4.75), (0.75, 0.75, 0.2))
    # Potence de fer + enseigne suspendue, côté porte.
    cube("Potence", metal, (-2.6, 1.85, 3.15), (1.0, 0.06, 0.06))
    cube("PotenceMur", metal, (-2.75, 2.02, 3.15), (0.06, 0.35, 0.06))
    cylinder("Chaine", metal, (-2.15, 1.85, 2.85), radius=0.02, depth=0.35, vertices=5)
    cube("Enseigne", sign_m, (-2.15, 1.85, 2.55), (0.55, 0.05, 0.4))
    export("hamlet_inn.glb")


def gen_sawmill():
    """Scierie ~5.0×3.6 m : charpente ouverte côté façade (pas de mur, la
    scie travaille au grand jour), soubassement de pierre, planches empilées
    en désordre à l'extérieur — silhouette industrielle distincte des
    maisons/auberge du hameau."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    roof_d = mat("toit_sombre", ROOF_DARK)

    stone_coursing("Socle", stone, stone_dark, (0, 0, 0.4), width=5.0, height=0.8,
                   depth=3.6, rows=3)
    # Charpente : 6 poteaux d'angle/milieu, pas de mur plein (façade +Y ouverte).
    for x in (-2.3, 0.0, 2.3):
        for y in (-1.6, 1.6):
            cube(f"Poteau{x}_{y}", wood_dark, (x, y, 1.7), (0.16, 0.16, 1.8))
    plank_wall("MurArriere", wood, wood_dark, (0, -1.6, 1.7), width=5.0, height=1.8,
               depth=0.14, n_planks=8)
    for sx in (-1, 1):
        plank_wall(f"MurCote{sx}", wood, wood_dark, (sx * 2.3, 0, 1.7), width=3.2,
                   height=1.8, depth=0.14, n_planks=5)
    shingled_roof("Toit", roof_m, roof_d, span_x=5.0, depth_y=3.6, rise=1.3, base_z=2.6)
    # Pile de planches débitées, en désordre, côté cour.
    for i in range(7):
        z = 0.06 + i * 0.09
        jitter_y = rng.uniform(-0.05, 0.05)
        jitter_rot = rng.uniform(-0.03, 0.03)
        p = cube(f"Planche{i}", wood if i % 2 == 0 else wood_dark,
                  (2.9, -0.6 + jitter_y, z), (1.6, 0.28, 0.07))
        p.rotation_euler = (0, 0, jitter_rot)
    export("hamlet_sawmill.glb")


def gen_stable():
    """Écurie ~5.0×3.2 m, basse (faîte à 2.9 m) : soubassement de pierre bas,
    planches, double porte de grange (un vantail entrouvert), lucarne de
    fenil, botte de foin près de l'entrée."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    roof_d = mat("toit_sombre", ROOF_DARK)
    hay = mat("paille", HAY)

    stone_coursing("Socle", stone, stone_dark, (0, 0, 0.35), width=5.0, height=0.7,
                   depth=3.2, rows=3)
    plank_wall("Mur", wood, wood_dark, (0, 0, 1.55), width=5.0, height=1.7,
               depth=3.2, n_planks=9)
    cube("VantailFixe", wood_dark, (-0.85, 1.62, 1.05), (0.9, 0.08, 1.6))
    v = cube("VantailMobile", wood_dark, (0.30, 1.68, 1.05), (0.9, 0.08, 1.6))
    v.rotation_euler = (0, 0, math.radians(22))
    cube("Lucarne", (mat("bois_sombre", WOOD_DARK)), (0, 1.62, 2.55), (0.7, 0.05, 0.5))
    shingled_roof("Toit", roof_m, roof_d, span_x=5.0, depth_y=3.2, rise=1.2, base_z=2.4)
    cylinder("Botte", hay, (1.9, 1.9, 0.35), radius=0.35, depth=0.7, vertices=12,
              rotation=(0, math.pi / 2, 0))
    export("hamlet_stable.glb")


def gen_mill():
    """Moulin ~3.2 m de diamètre, faîte à 5.4 m : tour ronde à assises de
    pierre (bandes cylindriques), toit conique, grande roue à aubes sur le
    flanc — la pièce la plus caractéristique du lot."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    glass = mat("vitre", GLASS)
    roof_m = mat("toit", ROOF)

    cylinder("Tour", stone, (0, 0, 2.0), radius=1.5, depth=4.0, vertices=14)
    for z in (0.6, 1.6, 2.6, 3.6):
        cylinder(f"Assise{z}", stone_dark, (0, 0, z), radius=1.51, depth=0.08, vertices=14)
    cube("Porte", wood_dark, (0, 1.52, 0.95), (0.8, 0.10, 1.7))
    cube("Fenetre", glass, (-0.9, 1.28, 2.6), (0.5, 0.08, 0.5))
    cone("Toit", roof_m, (0, 0, 4.6), radius=1.75, depth=1.2, vertices=14)
    # Roue à aubes : moyeu + 10 aubes radiales + jante fine. Flanc +X (côté
    # visible de la caméra de vignette par défaut, cf. hamlet_common.
    # render_preview — un flanc -X restait caché derrière la tour sur toutes
    # les vignettes générées avec cette caméra générique).
    hub_x = 1.65
    # hub_z assez haut pour que même l'aube la plus basse (centre - demi-
    # longueur de pale, 1.0+0.45) reste au-dessus du sol (piège rencontré ici :
    # une roue centrée trop bas plonge sous z=0, échec QA vertex-sous-sol).
    hub_z = 1.55
    cylinder("Moyeu", wood_dark, (hub_x, 0, hub_z), radius=0.22, depth=0.25, vertices=10,
              rotation=(0, math.pi / 2, 0))
    for i in range(10):
        a = i * math.tau / 10
        x = hub_x + 0.08
        y, z = 1.0 * math.cos(a), hub_z + 1.0 * math.sin(a)
        p = cube(f"Aube{i}", wood, (x, y, z), (0.16, 0.34, 0.9))
        p.rotation_euler = (a, 0, 0)
    cylinder("Jante", wood_dark, (hub_x, 0, hub_z), radius=1.05, depth=0.12, vertices=16,
              rotation=(0, math.pi / 2, 0))
    export("hamlet_mill.glb")


def gen_gazebo():
    """Gloriette ~2.6 m de diamètre, faîte à 3.1 m : plancher à lattes, 4
    poteaux, garde-corps à balustres sur 3 côtés (entrée ouverte côté +Y),
    petit toit en tuiles à 4 pans."""
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    roof_d = mat("toit_sombre", ROOF_DARK)

    cube("Plancher", wood, (0, 0, 0.20), (2.6, 2.6, 0.16))
    for i in range(1, 6):
        x = -1.3 + i * (2.6 / 6)
        cube(f"Latte{i}", wood_dark, (x, 0, 0.29), (0.03, 2.55, 0.02))
    posts = [(-1.15, -1.15), (1.15, -1.15), (-1.15, 1.15), (1.15, 1.15)]
    for i, (x, y) in enumerate(posts):
        cube(f"Poteau{i}", wood_dark, (x, y, 1.65), (0.14, 0.14, 3.0))
    # Garde-corps sur 3 côtés (arrière + 2 flancs), balustres rapprochés.
    for rx in (-1.15, 1.15):
        cube(f"Lisse{rx}", wood_dark, (rx, 0, 0.85), (0.10, 2.3, 0.08))
        for j in range(6):
            by = -1.0 + j * 0.4
            cube(f"Balustre{rx}_{j}", wood, (rx, by, 0.55), (0.06, 0.05, 0.7))
    cube("LisseArriere", wood_dark, (0, -1.15, 0.85), (2.3, 0.10, 0.08))
    for j in range(6):
        bx = -1.0 + j * 0.4
        cube(f"BalustreArriere{j}", wood, (bx, -1.15, 0.55), (0.05, 0.06, 0.7))
    shingled_roof("Toit", roof_m, roof_d, span_x=2.6, depth_y=2.6, rise=0.9, base_z=3.15,
                  rows=4, overhang=0.35)
    export("hamlet_gazebo.glb")


def gen_well():
    """Puits ~1.7 m de diamètre : margelle appareillée en 10 blocs de pierre
    disposés en cercle (pas un cylindre lisse), portique de bois, petit toit
    en tuiles, seau suspendu à une corde sur une manivelle."""
    stone = mat("pierre", STONE)
    stone_dark = mat("pierre_sombre", STONE_DARK)
    wood = mat("bois", WOOD)
    wood_dark = mat("bois_sombre", WOOD_DARK)
    roof_m = mat("toit", ROOF)
    roof_d = mat("toit_sombre", ROOF_DARK)
    metal = mat("metal", METAL)

    n_blocks = 10
    for i in range(n_blocks):
        a = i * math.tau / n_blocks
        x, y = 0.75 * math.cos(a), 0.75 * math.sin(a)
        b = cube(f"Bloc{i}", stone if i % 2 == 0 else stone_dark, (x, y, 0.5),
                  (0.36, 0.30, 1.0))
        b.rotation_euler = (0, 0, a)
    cylinder("Fond", stone_dark, (0, 0, 1.01), radius=0.62, depth=0.04, vertices=10)
    for sx in (-1, 1):
        cube(f"Montant{sx}", wood, (sx * 0.72, 0, 1.55), (0.14, 0.14, 1.3))
    cube("Traverse", wood_dark, (0, 0, 2.15), (1.6, 0.12, 0.12))
    cylinder("Manivelle", wood_dark, (0.72, 0.22, 2.15), radius=0.05, depth=0.30,
              vertices=8, rotation=(math.pi / 2, 0, 0))
    shingled_roof("Toit", roof_m, roof_d, span_x=1.6, depth_y=1.5, rise=0.55, base_z=2.2,
                  rows=3, overhang=0.3)
    cylinder("Corde", (mat("corde", (0.55, 0.46, 0.30))), (0, 0, 1.75), radius=0.02,
              depth=0.7, vertices=5)
    cylinder("Seau", metal, (0, 0, 1.35), radius=0.16, depth=0.24, vertices=9)
    export("hamlet_well.glb")


ASSETS = [
    gen_inn,
    gen_sawmill,
    gen_stable,
    gen_mill,
    gen_gazebo,
    gen_well,
]

for gen in ASSETS:
    reset_scene()
    gen()

print(f"[hamlet_buildings2] pack complet : {len(ASSETS)} fichiers")
