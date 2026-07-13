//! Import de modèles glTF / GLB vers un `MeshData` (toutes les primitives fusionnées).

use glam::{Mat4, Vec3};

use crate::gfx::mesh::{MeshData, Vertex};

/// Charge le document glTF (chemin disque, asset projet `asset://` ou embarqué
/// `bundle://`) et ses buffers, sans encore en extraire de géométrie — partagé par
/// `load_gltf` et `load_gltf_skeleton`.
fn read_document(path: &str) -> Result<(gltf::Document, Vec<gltf::buffer::Data>), String> {
    if path.starts_with(crate::assets::SCHEME) || path.starts_with(crate::assets::ASSET_SCHEME) {
        let bytes =
            crate::assets::read_bytes(path).ok_or_else(|| format!("asset introuvable : {path}"))?;
        let (doc, buffers, _images) =
            gltf::import_slice(&bytes).map_err(|e| format!("glTF illisible : {e}"))?;
        return Ok((doc, buffers));
    }
    let (doc, buffers, _images) = gltf::import(path).map_err(|e| {
        format!(
            "{e} — un .gltf référence des fichiers externes (.bin, textures) qui doivent \
             être dans le même dossier. Préférez un .glb (autonome)."
        )
    })?;
    Ok((doc, buffers))
}

/// Charge un fichier `.gltf`/`.glb` (chemin disque, asset projet `asset://` ou
/// embarqué `bundle://`). Renvoie le mesh fusionné + son AABB local.
pub fn load_gltf(path: &str) -> Result<(MeshData, Vec3, Vec3), String> {
    let (doc, buffers) = read_document(path)?;
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
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|t| t.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            let base = vertices.len() as u32;
            for (i, p) in positions.iter().enumerate() {
                let n = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                vertices.push(Vertex {
                    position: *p,
                    normal: n,
                    color,
                    uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
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

/// Un os (joint) du squelette d'un modèle skinné (Sprint 84), avec sa hiérarchie
/// parent/enfant et sa pose de liaison inverse — nécessaire au skinning GPU (Sprint 86)
/// mais délibérément **sans** dépendance au rendu ici : données pures.
#[derive(Debug, Clone)]
pub struct Joint {
    pub name: String,
    /// Indice du parent dans `Skeleton::joints`, ou `None` pour la racine.
    pub parent: Option<usize>,
    /// Transform du joint **local à son parent**, dans la pose de liaison (bind pose) —
    /// tel que stocké sur le nœud glTF (`node.transform()`).
    pub bind_local: Mat4,
    /// Matrice inverse de la pose de liaison **monde** (`inverse_bind_matrix` du glTF) :
    /// annule la transformation du bind pose avant d'appliquer la pose animée courante.
    /// Identité si le glTF n'en fournit pas (skin sans accessor dédié, rare mais valide).
    pub inverse_bind: Mat4,
}

/// Hiérarchie de joints d'un modèle skinné, dans l'ordre des indices `JOINTS_0` du glTF —
/// l'indice d'un `Joint` dans `joints` EST l'indice utilisé par `VertexSkin::joints`.
#[derive(Debug, Clone, Default)]
pub struct Skeleton {
    pub joints: Vec<Joint>,
}

impl Skeleton {
    /// Indice du joint racine (celui sans parent), s'il existe. `None` seulement pour un
    /// squelette vide — un squelette valide a toujours exactement une racine.
    pub fn root(&self) -> Option<usize> {
        self.joints.iter().position(|j| j.parent.is_none())
    }
}

/// Indices + poids des (jusqu'à 4) os influençant un sommet skinné (convention glTF
/// `JOINTS_0`/`WEIGHTS_0`). Les poids somment à 1.0 dans un glTF bien formé ; un sommet
/// non influencé (poids tous nuls) reste à sa position bind pose au skinning (Sprint 86).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VertexSkin {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

/// Lit le squelette (hiérarchie de joints + poses de liaison) et les poids de peau par
/// sommet du **premier skin** du fichier, s'il en a un (Sprint 84 : données pures, sans
/// rendu — le skinning GPU proprement dit arrive au Sprint 86, l'échantillonnage de clips
/// au Sprint 85).
///
/// `Ok(None)` (pas une erreur) si le glTF n'a pas de skin : un mesh statique n'a
/// simplement rien à squeletter.
pub fn load_gltf_skeleton(path: &str) -> Result<Option<(Skeleton, Vec<VertexSkin>)>, String> {
    let (doc, buffers) = read_document(path)?;
    let Some(skin) = doc.skins().next() else {
        return Ok(None);
    };
    let skeleton = build_skeleton(&doc, &skin, &buffers)?;
    let vertex_skins = read_vertex_skins(&doc, &buffers);
    Ok(Some((skeleton, vertex_skins)))
}

/// Construit la hiérarchie de joints d'un skin. glTF n'expose que les relations
/// parent→enfants (`node.children()`) ; on inverse cette table une fois pour retrouver le
/// parent de chaque joint, puis on ne garde que les parents qui sont eux-mêmes des joints
/// du skin (le nœud « armature » racine, lui, n'en est typiquement pas un → `parent: None`).
fn build_skeleton(
    doc: &gltf::Document,
    skin: &gltf::Skin,
    buffers: &[gltf::buffer::Data],
) -> Result<Skeleton, String> {
    let joint_nodes: Vec<gltf::Node> = skin.joints().collect();
    if joint_nodes.is_empty() {
        return Ok(Skeleton::default());
    }

    let reader = skin.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
    let inverse_binds: Vec<Mat4> = match reader.read_inverse_bind_matrices() {
        Some(iter) => iter.map(|m| Mat4::from_cols_array_2d(&m)).collect(),
        None => vec![Mat4::IDENTITY; joint_nodes.len()],
    };

    let node_to_local: std::collections::HashMap<usize, usize> = joint_nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.index(), i))
        .collect();
    let mut node_parent: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for node in doc.nodes() {
        for child in node.children() {
            node_parent.insert(child.index(), node.index());
        }
    }

    let joints = joint_nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let parent = node_parent
                .get(&node.index())
                .and_then(|p| node_to_local.get(p))
                .copied();
            Joint {
                name: node.name().unwrap_or("joint").to_string(),
                parent,
                bind_local: Mat4::from_cols_array_2d(&node.transform().matrix()),
                inverse_bind: inverse_binds.get(i).copied().unwrap_or(Mat4::IDENTITY),
            }
        })
        .collect();
    Ok(Skeleton { joints })
}

/// Lit `JOINTS_0`/`WEIGHTS_0` de **tous** les sommets, dans le même ordre de parcours
/// (mesh → primitive → sommet) que `build_from` construit `MeshData::vertices` — un
/// `VertexSkin` par vertex fusionné, alignable en parallèle du mesh statique.
fn read_vertex_skins(doc: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Vec<VertexSkin> {
    let mut skins = Vec::new();
    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let n = reader.read_positions().map(|p| p.count()).unwrap_or(0);
            let joints: Vec<[u16; 4]> = reader
                .read_joints(0)
                .map(|j| j.into_u16().collect())
                .unwrap_or_else(|| vec![[0, 0, 0, 0]; n]);
            let weights: Vec<[f32; 4]> = reader
                .read_weights(0)
                .map(|w| w.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0, 0.0, 0.0]; n]);
            for i in 0..n {
                skins.push(VertexSkin {
                    joints: joints.get(i).copied().unwrap_or([0, 0, 0, 0]),
                    weights: weights.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 0.0]),
                });
            }
        }
    }
    skins
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empile un chunk GLB (type + données, alignées à 4 octets — JSON en espaces,
    /// binaire en zéros, cf. spec GLB 2.0 §Binary glTF layout).
    fn push_chunk(out: &mut Vec<u8>, chunk_type: u32, data: &[u8], pad_byte: u8) {
        let padded_len = data.len().div_ceil(4) * 4;
        out.extend_from_slice(&(padded_len as u32).to_le_bytes());
        out.extend_from_slice(&chunk_type.to_le_bytes());
        out.extend_from_slice(data);
        out.resize(out.len() + (padded_len - data.len()), pad_byte);
    }

    /// Construit un `.glb` minimal **à la main** (pas de fixture disque) : deux joints
    /// (« Root » racine, « Child » son enfant) squelettant un unique triangle, avec des
    /// poids de peau non triviaux — de quoi vérifier hiérarchie, noms, poses de liaison
    /// et `VertexSkin` sans dépendre d'un fichier externe (ex. export Mixamo réel).
    fn skinned_triangle_glb() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        // POSITION (VEC3 f32 × 3)
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in p {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        // NORMAL (VEC3 f32 × 3)
        for _ in 0..3 {
            for c in [0.0f32, 0.0, 1.0] {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        // JOINTS_0 (VEC4 u16 × 3)
        for j in [[0u16, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]] {
            for c in j {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        // WEIGHTS_0 (VEC4 f32 × 3)
        for w in [
            [1.0f32, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
        ] {
            for c in w {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        // inverseBindMatrices (MAT4 f32 × 2, identité pour les deux — round-trip simple)
        for _ in 0..2 {
            for c in Mat4::IDENTITY.to_cols_array() {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        assert_eq!(buf.len(), 36 + 36 + 24 + 48 + 128);

        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [2]}],
            "nodes": [
                {"name": "Root", "translation": [0.0, 1.0, 0.0], "children": [1]},
                {"name": "Child", "translation": [0.0, 0.5, 0.0]},
                {"name": "Mesh", "mesh": 0, "skin": 0}
            ],
            "meshes": [{"primitives": [{"attributes": {
                "POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3
            }}]}],
            "skins": [{"joints": [0, 1], "inverseBindMatrices": 4}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                 "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5123, "count": 3, "type": "VEC4"},
                {"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC4"},
                {"bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 36},
                {"buffer": 0, "byteOffset": 72, "byteLength": 24},
                {"buffer": 0, "byteOffset": 96, "byteLength": 48},
                {"buffer": 0, "byteOffset": 144, "byteLength": 128}
            ],
            "buffers": [{"byteLength": buf.len()}]
        });
        let json_bytes = serde_json::to_vec(&json).expect("sérialisation JSON du glTF de test");

        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes()); // version
        let json_padded = json_bytes.len().div_ceil(4) * 4;
        let bin_padded = buf.len().div_ceil(4) * 4;
        let total_len = 12 + 8 + json_padded + 8 + bin_padded;
        glb.extend_from_slice(&(total_len as u32).to_le_bytes());
        push_chunk(&mut glb, 0x4E4F534A, &json_bytes, b' '); // "JSON"
        push_chunk(&mut glb, 0x004E4942, &buf, 0); // "BIN\0"
        glb
    }

    #[test]
    fn skeleton_round_trips_hierarchy_names_and_bind_poses() {
        let glb = skinned_triangle_glb();
        let (doc, buffers, _images) =
            gltf::import_slice(&glb).expect("le glTF de test doit être valide");
        let skin = doc.skins().next().expect("le glTF de test a un skin");
        let skeleton = build_skeleton(&doc, &skin, &buffers).expect("parsing du squelette");

        assert_eq!(skeleton.joints.len(), 2);
        assert_eq!(
            skeleton.root(),
            Some(0),
            "« Root » (nœud 0) doit être la racine"
        );
        assert_eq!(skeleton.joints[0].name, "Root");
        assert_eq!(skeleton.joints[0].parent, None);
        assert_eq!(skeleton.joints[1].name, "Child");
        assert_eq!(
            skeleton.joints[1].parent,
            Some(0),
            "« Child » doit avoir « Root » pour parent (nœud 0 = joint local 0)"
        );

        // Pose de liaison : translation portée par le nœud glTF, retrouvée dans la
        // colonne de translation de `bind_local` (colonne-majeure : col 3 = translation).
        let root_translation = skeleton.joints[0].bind_local.col(3).truncate();
        assert_eq!(root_translation, Vec3::new(0.0, 1.0, 0.0));
        let child_translation = skeleton.joints[1].bind_local.col(3).truncate();
        assert_eq!(child_translation, Vec3::new(0.0, 0.5, 0.0));

        // Matrices inverses de liaison : identité dans cette fixture (round-trip simple).
        assert_eq!(skeleton.joints[0].inverse_bind, Mat4::IDENTITY);
        assert_eq!(skeleton.joints[1].inverse_bind, Mat4::IDENTITY);
    }

    #[test]
    fn vertex_skins_round_trip_joints_and_weights_per_vertex() {
        let glb = skinned_triangle_glb();
        let (doc, buffers, _images) =
            gltf::import_slice(&glb).expect("le glTF de test doit être valide");
        let skins = read_vertex_skins(&doc, &buffers);

        assert_eq!(skins.len(), 3, "un VertexSkin par sommet du triangle");
        assert_eq!(
            skins[0],
            VertexSkin {
                joints: [0, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0]
            }
        );
        assert_eq!(
            skins[1],
            VertexSkin {
                joints: [1, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0]
            }
        );
        assert_eq!(
            skins[2],
            VertexSkin {
                joints: [0, 1, 0, 0],
                weights: [0.5, 0.5, 0.0, 0.0]
            }
        );
    }

    /// Un `.glb` minimal **sans** skin : juste un triangle statique (POSITION/NORMAL),
    /// pour vérifier qu'un mesh statique ne fait pas échouer `load_gltf_skeleton` — il
    /// n'a simplement rien à squeletter.
    fn unskinned_triangle_glb() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in p {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        for _ in 0..3 {
            for c in [0.0f32, 0.0, 1.0] {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        assert_eq!(buf.len(), 36 + 36);

        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Mesh", "mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}}]}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                 "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 36}
            ],
            "buffers": [{"byteLength": buf.len()}]
        });
        let json_bytes = serde_json::to_vec(&json).expect("sérialisation JSON du glTF de test");

        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        let json_padded = json_bytes.len().div_ceil(4) * 4;
        let bin_padded = buf.len().div_ceil(4) * 4;
        let total_len = 12 + 8 + json_padded + 8 + bin_padded;
        glb.extend_from_slice(&(total_len as u32).to_le_bytes());
        push_chunk(&mut glb, 0x4E4F534A, &json_bytes, b' ');
        push_chunk(&mut glb, 0x004E4942, &buf, 0);
        glb
    }

    /// Écrit des octets GLB dans un fichier temporaire unique, pour exercer l'API
    /// publique `load_gltf_skeleton(path)` de bout en bout (pas seulement les fonctions
    /// internes `build_skeleton`/`read_vertex_skins`, déjà testées séparément ci-dessus).
    fn write_temp_glb(bytes: &[u8], name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("rusteegear_test_{name}_{}.glb", std::process::id()));
        std::fs::write(&path, bytes).expect("écriture du glTF de test");
        path
    }

    #[test]
    fn load_gltf_skeleton_returns_none_when_the_file_has_no_skin() {
        let path = write_temp_glb(&unskinned_triangle_glb(), "unskinned");
        let result = load_gltf_skeleton(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(
            result
                .expect("un mesh statique valide ne doit pas être une erreur")
                .is_none(),
            "pas de skin dans le fichier ⇒ pas de squelette, mais pas d'erreur non plus"
        );
    }

    #[test]
    fn load_gltf_skeleton_returns_some_when_the_file_has_a_skin() {
        let path = write_temp_glb(&skinned_triangle_glb(), "skinned");
        let result = load_gltf_skeleton(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let (skeleton, vertex_skins) = result
            .expect("le glTF de test doit être valide")
            .expect("le glTF de test a un skin");
        assert_eq!(skeleton.joints.len(), 2);
        assert_eq!(vertex_skins.len(), 3);
    }
}
