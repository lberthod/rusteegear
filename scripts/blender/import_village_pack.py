# Retraite le « Medieval Village Pack » (Quaternius, CC0, via Poly Pizza) pour
# le rendre exploitable par le loader du moteur. Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/import_village_pack.py
#
# Source : les .glb bruts du pack (export FBX2glTF), attendus dans SRC_DIR.
# Sortie : assets/models/village_*.glb (un fichier par asset source).
#
# Pourquoi ce retraitement (mêmes contraintes que gen_nature_pack.py) :
# - Les .glb du pack portent leurs transforms (rotation -90° X, scale ×100)
#   sur le NŒUD glTF plutôt que sur la géométrie (RootNode → objet, avec
#   rotation/scale, comme le fait FBX2glTF). Or `load_gltf`
#   (src/scene/import.rs) concatène les sommets **bruts** des primitives et
#   ignore les transforms de nœuds → sans ce script, tout apparaîtrait à
#   l'échelle ×100 et couché sur le flanc.
# - On importe donc chaque .glb dans une scène vide (Blender annule alors la
#   rotation -90°/scale ×100 pour retomber sur l'espace Z-up « naturel »),
#   on joint les multi-primitives en un seul objet, on lui applique son
#   transform (désormais identité de toute façon), puis on ré-exporte en GLB
#   Y-up : le nœud racine ressort avec un transform identité, la géométrie
#   déjà en espace « objet final ».
# - Un objet joint garde ses matériaux (une primitive glTF par matériau) :
#   les couleurs par partie (ex. Bag/Bag_Inside) survivent au join. Ces
#   assets n'utilisent que `base_color_factor`, jamais de texture.

import os

import bpy

SRC_DIR = "/Users/berthod/Downloads/Medieval Village Pack-glb"
OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

# (fichier source, nom de sortie normalisé). Les suffixes aléatoires Poly
# Pizza (ex. "-7uSlZo3n9Y") sont remplacés par un suffixe _a/_b lisible quand
# deux fichiers portent le même nom de base.
FILES = [
    ("Bag Open.glb", "village_bag_open.glb"),
    ("Bag.glb", "village_bag.glb"),
    ("Bags.glb", "village_bags.glb"),
    ("Barrel.glb", "village_barrel.glb"),
    ("Bell Tower.glb", "village_bell_tower.glb"),
    ("Bell.glb", "village_bell.glb"),
    ("Bench-7uSlZo3n9Y.glb", "village_bench_a.glb"),
    ("Bench.glb", "village_bench_b.glb"),
    ("Blacksmith.glb", "village_blacksmith.glb"),
    ("Bonfire.glb", "village_bonfire.glb"),
    ("Cart.glb", "village_cart.glb"),
    ("Cauldron.glb", "village_cauldron.glb"),
    ("Crate.glb", "village_crate.glb"),
    ("Door Round.glb", "village_door_round.glb"),
    ("Door Straight.glb", "village_door_straight.glb"),
    ("Fantasy Barracks.glb", "village_barracks.glb"),
    ("Fantasy House-BH2XHWUNmF.glb", "village_house_a.glb"),
    ("Fantasy House-dcPho4SUA3.glb", "village_house_b.glb"),
    ("Fantasy House.glb", "village_house_c.glb"),
    ("Fantasy Inn.glb", "village_inn.glb"),
    ("Fantasy Sawmill.glb", "village_sawmill.glb"),
    ("Fantasy Stable.glb", "village_stable.glb"),
    ("Fence.glb", "village_fence.glb"),
    ("Gazebo.glb", "village_gazebo.glb"),
    ("Hay.glb", "village_hay.glb"),
    ("Market Stand-DGIM5HGISb.glb", "village_market_stand_a.glb"),
    ("Market Stand.glb", "village_market_stand_b.glb"),
    ("Mill.glb", "village_mill.glb"),
    ("Package-kYvD6QCQRd.glb", "village_package_a.glb"),
    ("Package.glb", "village_package_b.glb"),
    ("Path Straight.glb", "village_path_straight.glb"),
    ("Rocks.glb", "village_rocks.glb"),
    ("Round Window.glb", "village_round_window.glb"),
    ("Sawmill Saw.glb", "village_sawmill_saw.glb"),
    ("Smoke.glb", "village_smoke.glb"),
    ("Stairs.glb", "village_stairs.glb"),
    ("Well.glb", "village_well.glb"),
    ("Window-EY1zrFcme9.glb", "village_window_a.glb"),
    ("Window.glb", "village_window_b.glb"),
]


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def process(src_name, out_name):
    reset_scene()
    src_path = os.path.join(SRC_DIR, src_name)
    bpy.ops.import_scene.gltf(filepath=src_path)
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if not meshes:
        print(f"[village] AUCUN MESH dans {src_name}, ignoré")
        return
    for o in bpy.context.scene.objects:
        o.select_set(o in meshes)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.export_scene.gltf(
        filepath=OUT_DIR + out_name,
        export_format="GLB",
        export_animations=False,
        export_skins=False,
        export_apply=True,
        export_yup=True,
    )
    print(f"[village] exporté {out_name}")


for src, out in FILES:
    process(src, out)

print(f"[village] pack complet : {len(FILES)} fichiers dans {OUT_DIR}")
