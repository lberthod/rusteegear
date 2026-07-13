//! Import de modèles glTF / GLB vers un `MeshData` (toutes les primitives fusionnées).

use glam::{Mat4, Quat, Vec3};

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

/// Interpolation d'un canal d'animation glTF prise en charge (Sprint 85). `CubicSpline`
/// n'est **pas** géré : rare en pratique (Mixamo/Blender exportent en Linear/Step) et
/// nécessiterait de porter les tangentes entrée/sortie — un canal `CubicSpline` est
/// ignoré (le joint garde sa pose de liaison sur cette propriété) plutôt que
/// silencieusement mal interpolé comme s'il était linéaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interp {
    Step,
    Linear,
}

/// Piste de valeurs `Vec3` clé en temps (translation ou scale) d'un canal d'animation.
#[derive(Debug, Clone)]
struct TrackVec3 {
    times: Vec<f32>,
    values: Vec<Vec3>,
    interp: Interp,
}

impl TrackVec3 {
    /// `time` est déjà rebouclé par l'appelant (`Clip::sample_joint`) — cette fonction ne
    /// gère que l'interpolation entre deux clés, jamais le bouclage.
    fn sample(&self, time: f32) -> Vec3 {
        sample_keyed(
            &self.times,
            time,
            self.interp,
            |i| self.values[i],
            Vec3::lerp,
        )
    }
}

/// Piste de rotations (quaternions) clées en temps d'un canal d'animation.
#[derive(Debug, Clone)]
struct TrackQuat {
    times: Vec<f32>,
    values: Vec<Quat>,
    interp: Interp,
}

impl TrackQuat {
    fn sample(&self, time: f32) -> Quat {
        // nlerp (interpolation linéaire normalisée), pas slerp : c'est ce que spécifie
        // glTF pour l'interpolation « Linear » des rotations (lerp des 4 composantes,
        // en prenant le chemin le plus court, puis normalisation) — moins coûteux que
        // slerp et suffisant pour des clips à fréquence d'échantillonnage normale.
        sample_keyed(
            &self.times,
            time,
            self.interp,
            |i| self.values[i],
            |a, b, t| {
                let b = if a.dot(b) < 0.0 { -b } else { b };
                (a * (1.0 - t) + b * t).normalize()
            },
        )
    }
}

/// Logique d'échantillonnage partagée par `TrackVec3`/`TrackQuat` : trouve l'intervalle
/// `[times[i], times[i+1]]` contenant `time` (déjà dans `[times[0], times[last]]` — le
/// bouclage est fait en amont par l'appelant) et interpole avec `lerp`, ou tient la valeur
/// du dernier keyframe passé si `interp == Step`.
///
/// `time` en dehors de l'intervalle des clés (avant la première, après la dernière) est
/// bloqué (« clamped ») sur la clé la plus proche plutôt que d'extrapoler.
fn sample_keyed<V: Copy>(
    times: &[f32],
    time: f32,
    interp: Interp,
    value: impl Fn(usize) -> V,
    lerp: impl Fn(V, V, f32) -> V,
) -> V {
    debug_assert!(!times.is_empty(), "un canal a toujours au moins une clé");
    if time <= times[0] {
        return value(0);
    }
    let last = times.len() - 1;
    if time >= times[last] {
        return value(last);
    }
    // `times` est trié croissant (garanti par le glTF) : la première clé strictement
    // après `time` donne l'intervalle [i-1, i].
    let i = times.partition_point(|&t| t <= time).max(1);
    if interp == Interp::Step {
        return value(i - 1);
    }
    let (t0, t1) = (times[i - 1], times[i]);
    let t = if t1 > t0 {
        (time - t0) / (t1 - t0)
    } else {
        0.0
    };
    lerp(value(i - 1), value(i), t)
}

/// Les canaux animés d'**un** joint (jusqu'à 3 : translation, rotation, scale). Un joint
/// non mentionné dans un clip n'a pas d'entrée dans `Clip::tracks` — sa pose de liaison
/// (Sprint 84) s'applique telle quelle sur toute la durée du clip.
#[derive(Debug, Clone, Default)]
struct JointTracks {
    translation: Option<TrackVec3>,
    rotation: Option<TrackQuat>,
    scale: Option<TrackVec3>,
}

/// Transform locale échantillonnée d'un joint à un instant donné (Sprint 85) : chaque
/// composante est `Some` seulement si le clip anime effectivement cette propriété — les
/// composantes `None` doivent retomber sur la pose de liaison du joint (`Joint::bind_local`
/// décomposée), pas sur une valeur neutre arbitraire. Cette fusion pose de liaison / pose
/// animée est le travail de l'appelant (Sprint 86, skinning), délibérément hors de ce sprint.
#[derive(Debug, Clone, Copy, Default)]
pub struct JointPose {
    pub translation: Option<Vec3>,
    pub rotation: Option<Quat>,
    pub scale: Option<Vec3>,
}

/// Un clip d'animation : plusieurs canaux (typiquement 3 par joint animé : T/R/S),
/// échantillonnable à un temps quelconque, en boucle (Sprint 85 — CPU pur, sans lien au
/// rendu ; le skinning GPU proprement dit arrive au Sprint 86).
#[derive(Debug, Clone, Default)]
pub struct Clip {
    pub name: String,
    /// Durée du clip (secondes) : le dernier temps de clé, tous canaux confondus. `0.0`
    /// pour un clip sans canal exploitable (ex. tous `CubicSpline`, ignorés).
    pub duration: f32,
    tracks: std::collections::HashMap<usize, JointTracks>,
}

impl Clip {
    /// Échantillonne le clip à `time` (secondes, rebouclé automatiquement sur `duration` —
    /// négatif ou au-delà de la durée sont tous les deux ramenés dans `[0, duration)` par
    /// `rem_euclid`, jamais d'extrapolation ni de panique sur une valeur hors bornes).
    pub fn sample_joint(&self, joint: usize, time: f32) -> JointPose {
        let Some(tracks) = self.tracks.get(&joint) else {
            return JointPose::default();
        };
        let t = if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        };
        JointPose {
            translation: tracks.translation.as_ref().map(|tr| tr.sample(t)),
            rotation: tracks.rotation.as_ref().map(|tr| tr.sample(t)),
            scale: tracks.scale.as_ref().map(|tr| tr.sample(t)),
        }
    }
}

/// Lit les clips d'animation du **premier skin** du glTF (Sprint 85) : chaque canal
/// (`doc.animations()[..].channels()`) qui cible un nœud faisant partie des joints du skin
/// devient une piste de `Clip`. Les canaux ciblant un nœud hors du skin (caméra, lumière,
/// morph targets…) sont ignorés — hors périmètre de l'animation squelettale.
///
/// `Ok(vec![])` (pas une erreur) si le fichier n'a ni skin ni animation.
pub fn load_gltf_clips(path: &str) -> Result<Vec<Clip>, String> {
    let (doc, buffers) = read_document(path)?;
    let Some(skin) = doc.skins().next() else {
        return Ok(Vec::new());
    };
    let node_to_joint: std::collections::HashMap<usize, usize> = skin
        .joints()
        .enumerate()
        .map(|(i, n)| (n.index(), i))
        .collect();

    let clips = doc
        .animations()
        .map(|anim| build_clip(&anim, &node_to_joint, &buffers))
        .collect();
    Ok(clips)
}

fn build_clip(
    anim: &gltf::Animation,
    node_to_joint: &std::collections::HashMap<usize, usize>,
    buffers: &[gltf::buffer::Data],
) -> Clip {
    let mut tracks: std::collections::HashMap<usize, JointTracks> =
        std::collections::HashMap::new();
    let mut duration = 0.0f32;

    for channel in anim.channels() {
        let Some(&joint) = node_to_joint.get(&channel.target().node().index()) else {
            continue; // cible hors du skin (caméra, lumière…) : hors périmètre
        };
        let interp = match channel.sampler().interpolation() {
            gltf::animation::Interpolation::Step => Interp::Step,
            gltf::animation::Interpolation::Linear => Interp::Linear,
            gltf::animation::Interpolation::CubicSpline => continue, // non géré, cf. doc `Interp`
        };
        let reader = channel.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
        let Some(times): Option<Vec<f32>> = reader.read_inputs().map(Iterator::collect) else {
            continue;
        };
        if times.is_empty() {
            continue;
        }
        duration = duration.max(times[times.len() - 1]);

        let Some(outputs) = reader.read_outputs() else {
            continue;
        };
        let entry = tracks.entry(joint).or_default();
        match outputs {
            gltf::animation::util::ReadOutputs::Translations(v) => {
                entry.translation = Some(TrackVec3 {
                    times,
                    values: v.map(Vec3::from_array).collect(),
                    interp,
                });
            }
            gltf::animation::util::ReadOutputs::Rotations(r) => {
                entry.rotation = Some(TrackQuat {
                    times,
                    values: r
                        .into_f32()
                        .map(|[x, y, z, w]| Quat::from_xyzw(x, y, z, w))
                        .collect(),
                    interp,
                });
            }
            gltf::animation::util::ReadOutputs::Scales(v) => {
                entry.scale = Some(TrackVec3 {
                    times,
                    values: v.map(Vec3::from_array).collect(),
                    interp,
                });
            }
            gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => {} // hors périmètre
        }
    }

    Clip {
        name: anim.name().unwrap_or("clip").to_string(),
        duration,
        tracks,
    }
}

/// Décompose une matrice de liaison en (translation, rotation, échelle) — pour fusionner
/// avec les composantes qu'un `Clip` anime réellement (`JointPose`, dont chaque champ peut
/// être `None`) sans jamais perdre les composantes **non** animées de la pose de liaison.
fn decompose(m: Mat4) -> (Vec3, Quat, Vec3) {
    let (scale, rotation, translation) = m.to_scale_rotation_translation();
    (translation, rotation, scale)
}

/// Calcule, pour chaque joint d'un `Skeleton`, la matrice à envoyer au shader de skinning
/// (Sprint 86) : `monde_du_joint(pose animée ou de liaison) * inverse_bind` — la partie
/// `inverse_bind` annule la pose de liaison pour ne laisser que le **déplacement** depuis
/// cette pose, ce qui est ce qu'un sommet en espace de liaison doit subir.
///
/// `clip = None` ⇒ pose de liaison pure (équivalent à un modèle statique : chaque matrice
/// résultante est proche de l'identité, à l'erreur de précision flottante près — cf. test).
///
/// Robuste à un ordre de `Skeleton::joints` où un parent n'est **pas** garanti apparaître
/// avant ses enfants (le glTF ne l'impose pas, même si c'est l'usage courant des
/// exportateurs) : résolution par vagues plutôt que par simple parcours linéaire.
pub fn compute_joint_matrices(skeleton: &Skeleton, clip: Option<&Clip>, time: f32) -> Vec<Mat4> {
    let n = skeleton.joints.len();
    let mut world: Vec<Option<Mat4>> = vec![None; n];
    let mut remaining: Vec<usize> = (0..n).collect();

    while !remaining.is_empty() {
        let mut progressed = false;
        remaining.retain(|&i| {
            let joint = &skeleton.joints[i];
            let parent_ready = match joint.parent {
                None => true,
                Some(p) => world[p].is_some(),
            };
            if !parent_ready {
                return true; // pas encore résolvable : on retente à la vague suivante
            }
            let (bind_t, bind_r, bind_s) = decompose(joint.bind_local);
            let local = match clip {
                Some(clip) => {
                    let pose = clip.sample_joint(i, time);
                    Mat4::from_scale_rotation_translation(
                        pose.scale.unwrap_or(bind_s),
                        pose.rotation.unwrap_or(bind_r),
                        pose.translation.unwrap_or(bind_t),
                    )
                }
                None => joint.bind_local,
            };
            let parent_world = joint
                .parent
                .and_then(|p| world[p])
                .unwrap_or(Mat4::IDENTITY);
            world[i] = Some(parent_world * local);
            progressed = true;
            false // résolu : sorti de `remaining`
        });
        if !progressed {
            // Squelette invalide (cycle parent/enfant, ou parent hors bornes) : n'arrive
            // pas avec un glTF valide, mais on n'y boucle jamais indéfiniment pour autant.
            break;
        }
    }

    (0..n)
        .map(|i| world[i].unwrap_or(Mat4::IDENTITY) * skeleton.joints[i].inverse_bind)
        .collect()
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
    /// Un compteur atomique, pas seulement `std::process::id()` : `cargo test` exécute les
    /// tests d'un même binaire sur plusieurs **threads** du même processus. Deux tests
    /// utilisant le même `name` avec seulement le PID en suffixe écrivaient donc le même
    /// chemin en parallèle — l'un tronquait le fichier pendant que l'autre le lisait
    /// (« failed to fill whole buffer », intermittent selon l'ordonnancement des threads).
    static TEMP_GLB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn write_temp_glb(bytes: &[u8], name: &str) -> std::path::PathBuf {
        let n = TEMP_GLB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusteegear_test_{name}_{}_{n}.glb",
            std::process::id()
        ));
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

    /// Étend `skinned_triangle_glb` avec un clip « Test » : translation (linéaire) sur le
    /// joint 0, keyframes t=0→(0,0,0), t=1→(10,0,0), t=2→(10,0,0) ; scale (step) sur le
    /// joint 1, keyframes t=0→(1,1,1), t=1→(2,2,2). Assez pour exercer interpolation
    /// linéaire, palier (step), bouclage et durée multi-canaux dans un seul clip.
    fn animated_skinned_glb() -> Vec<u8> {
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
        for j in [[0u16, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]] {
            for c in j {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        for w in [
            [1.0f32, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
        ] {
            for c in w {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        for _ in 0..2 {
            for c in Mat4::IDENTITY.to_cols_array() {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        assert_eq!(
            buf.len(),
            272,
            "layout de base identique à skinned_triangle_glb"
        );

        // Canal 0 : translation du joint 0 (Root), LINEAR, 3 clés.
        for t in [0.0f32, 1.0, 2.0] {
            buf.extend_from_slice(&t.to_le_bytes());
        }
        for v in [[0.0f32, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]] {
            for c in v {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        // Canal 1 : scale du joint 1 (Child), STEP, 2 clés.
        for t in [0.0f32, 1.0] {
            buf.extend_from_slice(&t.to_le_bytes());
        }
        for v in [[1.0f32, 1.0, 1.0], [2.0, 2.0, 2.0]] {
            for c in v {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        assert_eq!(buf.len(), 272 + 12 + 36 + 8 + 24);

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
            "animations": [{
                "name": "Test",
                "channels": [
                    {"sampler": 0, "target": {"node": 0, "path": "translation"}},
                    {"sampler": 1, "target": {"node": 1, "path": "scale"}}
                ],
                "samplers": [
                    {"input": 5, "interpolation": "LINEAR", "output": 6},
                    {"input": 7, "interpolation": "STEP", "output": 8}
                ]
            }],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                 "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5123, "count": 3, "type": "VEC4"},
                {"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC4"},
                {"bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4"},
                {"bufferView": 5, "componentType": 5126, "count": 3, "type": "SCALAR",
                 "min": [0.0], "max": [2.0]},
                {"bufferView": 6, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 7, "componentType": 5126, "count": 2, "type": "SCALAR",
                 "min": [0.0], "max": [1.0]},
                {"bufferView": 8, "componentType": 5126, "count": 2, "type": "VEC3"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 36},
                {"buffer": 0, "byteOffset": 72, "byteLength": 24},
                {"buffer": 0, "byteOffset": 96, "byteLength": 48},
                {"buffer": 0, "byteOffset": 144, "byteLength": 128},
                {"buffer": 0, "byteOffset": 272, "byteLength": 12},
                {"buffer": 0, "byteOffset": 284, "byteLength": 36},
                {"buffer": 0, "byteOffset": 320, "byteLength": 8},
                {"buffer": 0, "byteOffset": 328, "byteLength": 24}
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

    #[test]
    fn clip_duration_is_the_last_keyframe_across_all_channels() {
        let path = write_temp_glb(&animated_skinned_glb(), "animated");
        let clips = load_gltf_clips(path.to_str().unwrap()).expect("parsing des clips");
        let _ = std::fs::remove_file(&path);
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].name, "Test");
        assert_eq!(
            clips[0].duration, 2.0,
            "dernière clé du canal de translation"
        );
    }

    #[test]
    fn clip_linear_translation_interpolates_between_keyframes_at_the_right_speed() {
        let path = write_temp_glb(&animated_skinned_glb(), "animated");
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        assert_eq!(
            clip.sample_joint(0, 0.0).translation,
            Some(Vec3::new(0.0, 0.0, 0.0))
        );
        assert_eq!(
            clip.sample_joint(0, 0.5).translation,
            Some(Vec3::new(5.0, 0.0, 0.0)),
            "à mi-chemin entre t=0 (0,0,0) et t=1 (10,0,0)"
        );
        assert_eq!(
            clip.sample_joint(0, 1.0).translation,
            Some(Vec3::new(10.0, 0.0, 0.0))
        );
        assert_eq!(
            clip.sample_joint(0, 1.5).translation,
            Some(Vec3::new(10.0, 0.0, 0.0)),
            "clé t=1 et t=2 identiques ⇒ plat entre les deux"
        );
    }

    #[test]
    fn clip_loops_past_its_duration() {
        let path = write_temp_glb(&animated_skinned_glb(), "animated");
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        // t=2.5 rebouclé sur une durée de 2.0 ⇒ équivalent à t=0.5.
        assert_eq!(
            clip.sample_joint(0, 2.5).translation,
            clip.sample_joint(0, 0.5).translation,
            "un temps au-delà de la durée doit reboucler, pas s'arrêter ni extrapoler"
        );
        // Un temps négatif reboucle aussi correctement (rem_euclid, pas de panique).
        assert_eq!(
            clip.sample_joint(0, -1.5).translation,
            clip.sample_joint(0, 0.5).translation
        );
    }

    #[test]
    fn clip_step_interpolation_holds_the_value_until_the_next_keyframe() {
        let path = write_temp_glb(&animated_skinned_glb(), "animated");
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        assert_eq!(clip.sample_joint(1, 0.0).scale, Some(Vec3::splat(1.0)));
        assert_eq!(
            clip.sample_joint(1, 0.9).scale,
            Some(Vec3::splat(1.0)),
            "step : tient la valeur jusqu'à la clé suivante, pas d'interpolation"
        );
        assert_eq!(clip.sample_joint(1, 1.0).scale, Some(Vec3::splat(2.0)));
        assert_eq!(
            clip.sample_joint(1, 1.9).scale,
            Some(Vec3::splat(2.0)),
            "tenu jusqu'à la fin du clip (pas de clé suivante)"
        );
    }

    #[test]
    fn clip_sample_joint_of_an_unanimated_joint_returns_no_channel_values() {
        let path = write_temp_glb(&animated_skinned_glb(), "animated");
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        // Le joint 0 n'a pas de canal de rotation/scale dans cette fixture : l'appelant
        // (Sprint 86) doit retomber sur la pose de liaison, pas sur une valeur inventée.
        let pose = clip.sample_joint(0, 0.5);
        assert!(pose.rotation.is_none());
        assert!(pose.scale.is_none());
        // Le joint 1 n'a pas de canal de translation.
        assert!(clip.sample_joint(1, 0.5).translation.is_none());
    }

    #[test]
    fn load_gltf_clips_returns_empty_when_the_file_has_no_skin() {
        let path = write_temp_glb(&unskinned_triangle_glb(), "clips_unskinned");
        let clips = load_gltf_clips(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(clips.unwrap().is_empty());
    }

    #[test]
    fn joint_matrices_in_bind_pose_equal_the_bind_hierarchy_when_inverse_bind_is_identity() {
        let path = write_temp_glb(&skinned_triangle_glb(), "joint_matrices_bind");
        let (skeleton, _) = load_gltf_skeleton(path.to_str().unwrap()).unwrap().unwrap();
        let _ = std::fs::remove_file(&path);

        let matrices = compute_joint_matrices(&skeleton, None, 0.0);
        assert_eq!(matrices.len(), 2);
        // inverse_bind = identité dans cette fixture ⇒ le résultat EST la hiérarchie de
        // liaison monde : joint 0 (Root) à (0,1,0), joint 1 (Child) composé par-dessus,
        // à (0, 1+0.5, 0).
        assert_eq!(matrices[0].col(3).truncate(), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(matrices[1].col(3).truncate(), Vec3::new(0.0, 1.5, 0.0));
    }

    #[test]
    fn joint_matrices_with_a_clip_override_only_the_animated_components() {
        let path = write_temp_glb(&animated_skinned_glb(), "joint_matrices_animated");
        let (skeleton, _) = load_gltf_skeleton(path.to_str().unwrap()).unwrap().unwrap();
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        let matrices = compute_joint_matrices(&skeleton, Some(clip), 0.5);
        // Joint 0 : translation animée (linéaire, →(5,0,0) à t=0.5) REMPLACE la
        // translation de liaison (0,1,0) — c'est la propriété que le clip anime.
        assert_eq!(matrices[0].col(3).truncate(), Vec3::new(5.0, 0.0, 0.0));
        // Joint 1 : pas de canal de translation ⇒ garde sa translation de liaison
        // (0,0.5,0) composée par-dessus le monde du joint 0 ; son canal de scale
        // (step) est tenu à (1,1,1) à t=0.5, donc sans effet sur la position.
        assert_eq!(matrices[1].col(3).truncate(), Vec3::new(5.0, 0.5, 0.0));
    }

    #[test]
    fn joint_matrices_never_panics_or_infinite_loops_on_an_empty_skeleton() {
        let matrices = compute_joint_matrices(&Skeleton::default(), None, 0.0);
        assert!(matrices.is_empty());
    }
}
