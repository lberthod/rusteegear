//! Import de modèles glTF / GLB vers un `MeshData` (toutes les primitives fusionnées).

use glam::Vec3;

use crate::gfx::mesh::{MeshData, Vertex};

/// Charge un fichier `.gltf`/`.glb` (chemin disque **ou** asset embarqué `bundle://`).
/// Renvoie le mesh fusionné + son AABB local.
pub fn load_gltf(path: &str) -> Result<(MeshData, Vec3, Vec3), String> {
    // Asset embarqué dans le binaire (player exporté).
    if let Some(key) = crate::assets::strip_scheme(path) {
        let bytes = crate::assets::bundle_bytes(key)
            .ok_or_else(|| format!("asset embarqué introuvable : {key}"))?;
        let (doc, buffers, _images) =
            gltf::import_slice(bytes).map_err(|e| format!("glTF embarqué illisible : {e}"))?;
        return build_from(doc, buffers);
    }

    let (doc, buffers, _images) = gltf::import(path).map_err(|e| {
        format!(
            "{e} — un .gltf référence des fichiers externes (.bin, textures) qui doivent \
             être dans le même dossier. Préférez un .glb (autonome)."
        )
    })?;
    build_from(doc, buffers)
}

/// Construit le `MeshData` fusionné (+ AABB local) à partir d'un document glTF chargé.
fn build_from(
    doc: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
) -> Result<(MeshData, Vec3, Vec3), String> {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let color = [0.8, 0.8, 0.82];

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(p) => p.collect(),
                None => continue,
            };
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            let base = vertices.len() as u32;
            for (i, p) in positions.iter().enumerate() {
                let n = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                vertices.push(Vertex {
                    position: *p,
                    normal: n,
                    color,
                });
                min = min.min(Vec3::from_array(*p));
                max = max.max(Vec3::from_array(*p));
            }

            match reader.read_indices() {
                Some(read) => indices.extend(read.into_u32().map(|i| base + i)),
                // pas d'indices : on suppose des triangles séquentiels
                None => indices.extend((0..positions.len() as u32).map(|i| base + i)),
            }
        }
    }

    if vertices.is_empty() {
        return Err("Aucune géométrie trouvée dans le fichier glTF".into());
    }
    Ok((MeshData { vertices, indices }, min, max))
}
