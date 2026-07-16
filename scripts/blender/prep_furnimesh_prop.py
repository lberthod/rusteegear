# Prépare un asset FurniMesh (glb texturé unique, généré par un outil externe)
# pour le bundle du joueur MMORPG :
#
#   /Applications/Blender.app/Contents/MacOS/Blender --background \
#       --python scripts/blender/prep_furnimesh_prop.py
#
# Le fichier source (33 Mo : 282k triangles + 2 textures 4096² non compressées)
# est bien trop lourd pour un bundle WASM. Le moteur ne lit de toute façon que
# `baseColorTexture` (cf. src/gfx/renderer.rs, un seul groupe de texture par
# objet) — la texture metallic/roughness embarquée est donc jetée, pas
# convertie.
#
# Sortie :
#   assets/bundle/m125_village_prop.glb  — géométrie décimée, UV conservés,
#                                           SANS image embarquée (le moteur lit
#                                           la texture séparément via
#                                           SceneObject.texture, cf. plan)
#   assets/bundle/m125_village_prop.jpg  — baseColorTexture seule, redimensionnée

import os

import bpy

SRC = "/private/tmp/claude-501/-Users-berthod-Desktop-motor3derust/4f63b553-14dc-4a9b-b4e0-6f0de643a39d/scratchpad/furnimesh_src.glb"
OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(__file__), "../../assets/bundle")) + "/"
GLB_OUT = OUT_DIR + "m125_village_chair.glb"
TEX_OUT = OUT_DIR + "m125_village_chair.jpg"
TEX_RES = 1024
TARGET_TRIS = 4000

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=SRC)

meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
assert len(meshes) == 1, f"attendu 1 mesh, trouvé {len(meshes)}"
obj = meshes[0]

tris_before = sum(len(p.vertices) - 2 for p in obj.data.polygons)
print(f"[prep] triangles avant décimation : {tris_before}")

# --- Extraction de la texture base color avant de la détacher du matériau ---
mat = obj.data.materials[0]
base_color_img = None
for node in mat.node_tree.nodes:
    if node.type == "TEX_IMAGE" and node.image is not None:
        # Le nœud branché sur Base Color de Principled BSDF est la baseColorTexture
        for link in mat.node_tree.links:
            if link.from_node == node and link.to_socket.name == "Base Color":
                base_color_img = node.image
                break
    if base_color_img is not None:
        break
assert base_color_img is not None, "baseColorTexture introuvable dans le matériau"

base_color_img.scale(TEX_RES, TEX_RES)
base_color_img.file_format = "JPEG"
base_color_img.filepath_raw = TEX_OUT
base_color_img.save()
print(f"[prep] texture exportée : {TEX_OUT}")

# --- Décimation de la géométrie (préserve le pliage UV existant) ---
bpy.context.view_layer.objects.active = obj
obj.select_set(True)
mod = obj.modifiers.new("Decimate", "DECIMATE")
mod.ratio = min(1.0, TARGET_TRIS / max(tris_before, 1))
bpy.ops.object.modifier_apply(modifier=mod.name)

tris_after = sum(len(p.vertices) - 2 for p in obj.data.polygons)
print(f"[prep] triangles après décimation : {tris_after}")

# --- Retrait des textures du matériau : le moteur ne lit pas les images
# embarquées dans le glb (cf. src/scene/import.rs), seulement base_color_factor
# + les UV. On exporte donc un glb "sans image" pour ne pas embarquer les
# 23 Mo de PNG inutiles ; la couleur vient de la texture externe assignée
# côté scène (SceneObject.texture = bundle://m125_village_prop.jpg).
for node in list(mat.node_tree.nodes):
    if node.type == "TEX_IMAGE":
        mat.node_tree.nodes.remove(node)
bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
bsdf.inputs["Base Color"].default_value = (1.0, 1.0, 1.0, 1.0)

bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
bpy.ops.export_scene.gltf(
    filepath=GLB_OUT,
    export_format="GLB",
    export_animations=False,
    export_skins=False,
    export_apply=True,
    export_yup=True,
    export_image_format="NONE",
)
print(f"[prep] glb exporté : {GLB_OUT}")
