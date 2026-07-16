# Retraite le « Ultimate Monsters Bundle » (Quaternius, CC0, via Poly Pizza)
# en décor STATIQUE pour le moteur. Blender headless :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/import_monster_pack.py
#
# Source : les .glb du pack (export FBX2glTF, riggés — squelette + jusqu'à 14
# clips par fichier). Sortie : assets/models/monster_*.glb, un mesh STATIQUE
# joint par asset, SANS squelette ni animation.
#
# Pourquoi statique et pas animé (même moteur que import_village_pack.py) :
# - `MAX_SKINNED_INSTANCES` (src/gfx/renderer.rs) borne à 96 le nombre total
#   d'objets SKINNÉS visibles à la fois (créatures MMORPG + décor nature
#   animé + joueurs réseau) — un mesh est skinné dès qu'il a un squelette,
#   même sans `AnimationState` (pose de liaison figée, mais toujours un
#   créneau consommé). Le budget est déjà à ~66/96 avant ce pack ; 45
#   nouveaux monstres skinnés le ferait exploser (au-delà de 96, l'excédent
#   est simplement invisible, en silence — cf. commentaire de
#   `write_joint_matrices`). On exporte donc en statique : aucun coût sur ce
#   budget, tous les 45 assets restent posables sans limite.
# - Comme pour le village : le .glb source porte transform (rotation -90° X,
#   scale ×100) sur le NŒUD du mesh (`load_gltf` du moteur ignore les
#   transforms de nœuds et concatène les sommets bruts) → on joint les
#   parties (corps + arme séparée sur certains monstres), on applique le
#   transform, on supprime l'armature (inutile sans skin), on ré-exporte
#   Y-up. La géométrie exportée est la pose de repos (= pose de liaison),
#   identique au premier repère de l'animation « Idle » du pack d'origine.

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
    # ne garde que les meshes réellement riggés (parentés à l'armature).
    meshes = [m for m in all_meshes if m.parent in armatures] or all_meshes
    if not meshes:
        print(f"[monsters] AUCUN MESH dans {src_name}, ignoré")
        return

    # Piège : un mesh skinné de ce pack est PARENTÉ (Object parenting, pas
    # seulement modifier Armature) à l'objet Armature, qui seul porte le
    # rotation -90°/scale ×100 du pipeline FBX2glTF (le nœud du mesh, lui,
    # est réinitialisé à l'identité par l'import glTF, comme pour tout mesh
    # skinné). Supprimer l'armature ferait donc perdre ce facteur ×100 avec
    # elle. On le récupère en détachant chaque mesh AVANT suppression avec
    # « Clear Parent Keep Transform » (bake le parent dans matrix_basis),
    # après avoir figé la pose de repos et appliqué le modifier Armature
    # (déformation osseuse propre).
    for a in armatures:
        a.data.pose_position = "REST"
    for m in meshes:
        bpy.context.view_layer.objects.active = m
        for mod in list(m.modifiers):
            if mod.type == "ARMATURE":
                bpy.ops.object.modifier_apply(modifier=mod.name)
        if m.parent is not None:
            bpy.ops.object.select_all(action="DESELECT")
            m.select_set(True)
            bpy.context.view_layer.objects.active = m
            bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")

    bpy.ops.object.select_all(action="DESELECT")
    for a in armatures:
        a.select_set(True)
    if armatures:
        bpy.ops.object.delete()

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
    print(f"[monsters] exporté {out_name}")


for src, out in FILES:
    process(src, out)

print(f"[monsters] pack complet : {len(FILES)} fichiers dans {OUT_DIR}")
