# Retraite le « Ultimate Monsters Bundle » (Quaternius, CC0, via Poly Pizza)
# en décor ANIMÉ pour le moteur. Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/import_monster_pack.py
#
# Source : les .glb du pack (export FBX2glTF, riggés — squelette + jusqu'à 14
# clips par fichier). Sortie : assets/models/monster_*.glb, squelette +
# animations conservés (`MAX_SKINNED_INSTANCES` relevé à 160 dans
# src/gfx/renderer.rs pour leur faire de la place).
#
# Piège (même diagnostic que import_village_pack.py, en pire) : le .glb
# source porte son rotation -90°/scale ×100 (conversion FBX2glTF) sur l'objet
# ARMATURE, pas sur le mesh — glTF réinitialise le TRS d'un nœud de mesh
# SKINNÉ à l'identité (la déformation vient entièrement des joints), et le
# mesh est en plus PARENTÉ (Object parenting) à l'armature. Le squelette lu
# par le moteur (`load_gltf_skeleton`/`build_skeleton`) ne remonte que
# jusqu'au premier joint du **skin** (`skin.joints()`) — l'objet Armature qui
# le porte n'en fait pas partie, donc son transform serait perdu si on le
# laissait au niveau de l'objet : il faut le rapatrier DANS le joint racine
# avant export.
#
# Fix : figer la pose de repos, puis `transform_apply` sur l'objet ARMATURE
# lui-même (pas sur le mesh) — Blender bake alors le rotation/scale dans les
# matrices de repos des os (le joint racine récupère un vrai facteur d'échelle
# réel, exprimé via des translations d'os désormais à taille réelle plutôt que
# via un scale explicite) sans toucher aux animations (les canaux de pose
# sont relatifs à chaque os, donc invariants à ce rebasement). Le mesh reste
# parenté/skinné (on ne le touche pas) ; l'export réévalue tout (position des
# sommets, matrices de bind inverses) de façon cohérente. Les parties
# multiples d'un même personnage (corps + arme séparée) sont jointes en un
# seul mesh, gardant le premier skin/squelette rencontré — cas rare de
# squelettes multiples par fichier (armes montées sur un second squelette) :
# seule la partie liée au premier reste animée, le reste englobé au join.

import os

import bpy

SRC_DIR = "/Users/berthod/Downloads/Ultimate Monsters Bundle-glb 2"
OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/models")) + "/"

# (fichier source, nom de sortie normalisé). Les suffixes aléatoires Poly
# Pizza (ex. "-RRliSQBP7r") sont remplacés par un suffixe _b lisible quand
# deux fichiers portent le même nom de base (variante alternative du pack).
FILES = [
    ("Alien.glb", "monster_alien.glb"),
    ("Alien-RRliSQBP7r.glb", "monster_alien_b.glb"),
    ("Alpaking.glb", "monster_alpaking.glb"),
    ("Alpaking Evolved.glb", "monster_alpaking_evolved.glb"),
    ("Armabee.glb", "monster_armabee.glb"),
    ("Armabee Evolved.glb", "monster_armabee_evolved.glb"),
    ("Birb.glb", "monster_birb.glb"),
    ("Blue Demon.glb", "monster_blue_demon.glb"),
    ("Bunny.glb", "monster_bunny.glb"),
    ("Cactoro.glb", "monster_cactoro.glb"),
    ("Cactoro-IGn9lhdama.glb", "monster_cactoro_b.glb"),
    ("Cat.glb", "monster_cat.glb"),
    ("Chicken.glb", "monster_chicken.glb"),
    ("Demon.glb", "monster_demon.glb"),
    ("Demon-LnfIziKv4o.glb", "monster_demon_b.glb"),
    ("Dino.glb", "monster_dino.glb"),
    ("Dragon.glb", "monster_dragon.glb"),
    ("Dragon Evolved.glb", "monster_dragon_evolved.glb"),
    ("Fish.glb", "monster_fish.glb"),
    ("Fish-ypEYhCImAB.glb", "monster_fish_b.glb"),
    ("Frog.glb", "monster_frog.glb"),
    ("Ghost.glb", "monster_ghost.glb"),
    ("Ghost Skull.glb", "monster_ghost_skull.glb"),
    ("Glub.glb", "monster_glub.glb"),
    ("Glub Evolved.glb", "monster_glub_evolved.glb"),
    ("Goleling.glb", "monster_goleling.glb"),
    ("Goleling Evolved.glb", "monster_goleling_evolved.glb"),
    ("Green Blob.glb", "monster_green_blob.glb"),
    ("Green Spiky Blob.glb", "monster_green_spiky_blob.glb"),
    ("Hywirl.glb", "monster_hywirl.glb"),
    ("Monkroose.glb", "monster_monkroose.glb"),
    ("Mushnub.glb", "monster_mushnub.glb"),
    ("Mushnub Evolved.glb", "monster_mushnub_evolved.glb"),
    ("Mushroom King.glb", "monster_mushroom_king.glb"),
    ("Ninja.glb", "monster_ninja.glb"),
    ("Ninja-xGYmeDpfTu.glb", "monster_ninja_b.glb"),
    ("Orc.glb", "monster_orc.glb"),
    ("Orc Enemy.glb", "monster_orc_enemy.glb"),
    ("Pigeon.glb", "monster_pigeon.glb"),
    ("Pink Blob.glb", "monster_pink_blob.glb"),
    ("Squidle.glb", "monster_squidle.glb"),
    ("Tribal.glb", "monster_tribal.glb"),
    ("Wizard.glb", "monster_wizard.glb"),
    ("Yeti.glb", "monster_yeti.glb"),
    ("Yeti-ceRHrn8HHE.glb", "monster_yeti_b.glb"),
]


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def process(src_name, out_name):
    reset_scene()
    src_path = os.path.join(SRC_DIR, src_name)
    bpy.ops.import_scene.gltf(filepath=src_path)

    all_meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    armatures = [o for o in bpy.context.scene.objects if o.type == "ARMATURE"]
    # Certains fichiers du pack embarquent un mesh générique parasite (ex.
    # "Icosphere") sans parent ni skin, sans rapport avec le personnage — on
    # ne garde que les meshes réellement riggés (parentés à une armature).
    meshes = [m for m in all_meshes if m.parent in armatures] or all_meshes
    if not meshes or not armatures:
        print(f"[monsters] structure inattendue dans {src_name}, ignoré")
        return

    for a in armatures:
        a.data.pose_position = "REST"
        bpy.ops.object.select_all(action="DESELECT")
        a.select_set(True)
        bpy.context.view_layer.objects.active = a
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        # Pas d'action active pointée par erreur au moment de l'export (les 14
        # clips existent déjà comme actions indépendantes, réexportées telles
        # quelles par le mode par défaut de l'exporter).
        if a.animation_data:
            a.animation_data.action = None

    bpy.ops.object.select_all(action="DESELECT")
    for m in meshes:
        m.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()

    bpy.ops.export_scene.gltf(
        filepath=OUT_DIR + out_name,
        export_format="GLB",
        export_animations=True,
        export_skins=True,
        export_apply=True,
        export_yup=True,
    )
    print(f"[monsters] exporté {out_name}")


for src, out in FILES:
    process(src, out)

print(f"[monsters] pack complet : {len(FILES)} fichiers dans {OUT_DIR}")
