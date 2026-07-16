//! Import de modèles glTF / GLB vers un `MeshData` (toutes les primitives fusionnées).

use glam::{Mat4, Quat, Vec3};

use crate::gfx::mesh::{MeshData, Vertex};

/// Charge le document glTF (chemin disque, asset projet `asset://` ou embarqué
/// `bundle://`) et ses buffers, sans encore en extraire de géométrie — partagé par
/// `load_gltf` et `load_gltf_skeleton`.
fn read_document(path: &str) -> Result<(gltf::Document, Vec<gltf::buffer::Data>), String> {
    if crate::assets::is_known_scheme(path) {
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

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            // Teinte de la primitive : `base_color_factor` du matériau glTF (pas de
            // texture — le moteur n'a qu'une couleur par sommet, pas de pipeline de
            // texture pour les meshes importés). `Material::default()` de la crate
            // `gltf` renvoie déjà `[1.0, 1.0, 1.0, 1.0]` (blanc, le défaut de la
            // spec glTF) pour une primitive sans matériau — pas besoin de gérer ce
            // cas à part, `base_color_factor()` le couvre.
            let [r, g, b, _a] = prim.material().pbr_metallic_roughness().base_color_factor();
            let color = [r, g, b];
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

/// Calcule une tangente par sommet quand le glTF n'en fournit pas
/// (notre lecteur n'essaie pas encore de lire l'attribut `TANGENT` — aucun des
/// modèles de test n'en porte un, ce serait un chantier à part). Méthode de Lengyel
/// (la même que la plupart des moteurs implémentent, souvent appelée « à la
/// mikktspace » même si distincte de l'implémentation de référence de Blender, plus
/// complexe) : tangente par triangle à partir des dérivées position/UV, accumulée par
/// sommet, puis orthogonalisée contre la normale (Gram-Schmidt) — le signe de la
/// bitangente (`w`) est déduit du triangle pour rester cohérent avec un UV retourné
/// (miroir), fréquent sur des meshes symétriques.
///
/// `xyz` = tangente normalisée, `w` = ±1 (signe de la bitangente). Un sommet jamais
/// référencé par un triangle valide (dégénéré, UV nuls) retombe sur une tangente
/// arbitraire perpendiculaire à la normale plutôt que `(0,0,0)` — un vecteur nul
/// briserait le repère tangent-espace si jamais échantillonné.
pub fn compute_tangents(vertices: &[Vertex], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut tangent_acc = vec![Vec3::ZERO; vertices.len()];
    let mut bitangent_acc = vec![Vec3::ZERO; vertices.len()];

    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (Some(v0), Some(v1), Some(v2)) = (vertices.get(i0), vertices.get(i1), vertices.get(i2))
        else {
            continue;
        };
        let e1 = Vec3::from_array(v1.position) - Vec3::from_array(v0.position);
        let e2 = Vec3::from_array(v2.position) - Vec3::from_array(v0.position);
        let (du1, dv1) = (v1.uv[0] - v0.uv[0], v1.uv[1] - v0.uv[1]);
        let (du2, dv2) = (v2.uv[0] - v0.uv[0], v2.uv[1] - v0.uv[1]);
        let det = du1 * dv2 - du2 * dv1;
        // UV dégénérés (triangle sans étendue UV réelle) : ce triangle ne contribue
        // aucune direction tangente fiable, plutôt que d'en injecter une via une
        // division par ~0 (explosion numérique qui polluerait tous les sommets
        // partagés, pas seulement ce triangle dégénéré).
        if det.abs() < 1e-8 {
            continue;
        }
        let r = 1.0 / det;
        let tangent = (e1 * dv2 - e2 * dv1) * r;
        let bitangent = (e2 * du1 - e1 * du2) * r;
        for &i in &[i0, i1, i2] {
            tangent_acc[i] += tangent;
            bitangent_acc[i] += bitangent;
        }
    }

    vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let n = Vec3::from_array(v.normal).normalize_or(Vec3::Y);
            let t = tangent_acc[i];
            // Gram-Schmidt : retire la composante de `t` colinéaire à la normale, pour
            // un repère tangent-espace orthogonal même si `t` penchait légèrement hors
            // du plan tangent (accumulation de plusieurs triangles non coplanaires).
            let ortho = t - n * n.dot(t);
            let tangent = if ortho.length_squared() > 1e-12 {
                ortho.normalize()
            } else {
                // Tangente/normale colinéaires (sommet jamais dans un triangle à UV
                // valide, cf. la garde `det` ci-dessus) : n'importe quel vecteur
                // perpendiculaire à `n` fait un repère valide, arbitraire mais stable.
                n.any_orthogonal_vector()
            };
            let handedness = if n.cross(tangent).dot(bitangent_acc[i]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [tangent.x, tangent.y, tangent.z, handedness]
        })
        .collect()
}

/// Un os (joint) du squelette d'un modèle skinné, avec sa hiérarchie
/// parent/enfant et sa pose de liaison inverse — nécessaire au skinning GPU
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
/// non influencé (poids tous nuls) reste à sa position bind pose au skinning.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VertexSkin {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

/// Lit le squelette (hiérarchie de joints + poses de liaison) et les poids de peau par
/// sommet du **premier skin** du fichier, s'il en a un — données pures, sans rendu ni
/// échantillonnage de clip (cf. `compute_joint_matrices`/`Clip` pour la suite).
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

/// Interpolation d'un canal d'animation glTF prise en charge. `CubicSpline`
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

/// Interpolation linéaire normalisée (nlerp) de deux quaternions : prend le chemin le
/// plus court (inverse `b` si son produit scalaire avec `a` est négatif), lerp des 4
/// composantes, puis normalise. C'est ce que spécifie glTF pour l'interpolation
/// « Linear » des rotations — pas slerp, moins coûteux et suffisant à fréquence
/// d'échantillonnage normale. Partagé par `TrackQuat::sample` et
/// `compute_joint_matrices_blended` (crossfade entre deux clips).
fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let b = if a.dot(b) < 0.0 { -b } else { b };
    (a * (1.0 - t) + b * t).normalize()
}

impl TrackQuat {
    fn sample(&self, time: f32) -> Quat {
        sample_keyed(&self.times, time, self.interp, |i| self.values[i], nlerp)
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
/// s'applique telle quelle sur toute la durée du clip.
#[derive(Debug, Clone, Default)]
struct JointTracks {
    translation: Option<TrackVec3>,
    rotation: Option<TrackQuat>,
    scale: Option<TrackVec3>,
}

/// Transform locale échantillonnée d'un joint à un instant donné : chaque
/// composante est `Some` seulement si le clip anime effectivement cette propriété — les
/// composantes `None` doivent retomber sur la pose de liaison du joint (`Joint::bind_local`
/// décomposée), pas sur une valeur neutre arbitraire. Cette fusion pose de liaison / pose
/// animée est le travail de l'appelant (skinning), délibérément hors de ce module.
#[derive(Debug, Clone, Copy, Default)]
pub struct JointPose {
    pub translation: Option<Vec3>,
    pub rotation: Option<Quat>,
    pub scale: Option<Vec3>,
}

/// Un clip d'animation : plusieurs canaux (typiquement 3 par joint animé : T/R/S),
/// échantillonnable à un temps quelconque, en boucle — CPU pur, sans lien au rendu
/// (le skinning GPU consomme le résultat séparément).
#[derive(Debug, Clone, Default)]
pub struct Clip {
    pub name: String,
    /// Durée du clip (secondes) : le dernier temps de clé, tous canaux confondus. `0.0`
    /// pour un clip sans canal exploitable (ex. tous `CubicSpline`, ignorés).
    pub duration: f32,
    tracks: std::collections::HashMap<usize, JointTracks>,
}

impl Clip {
    /// Clip sans piste de joint : pour les tests d'échange
    /// notifies/événements, qui n'ont besoin que de `name`/`duration` — `tracks` reste
    /// privé (jamais construit à la main hors de ce module), d'où ce constructeur
    /// plutôt qu'un accès direct au champ depuis `app::mod` (tests de `notifies_crossed`).
    pub fn without_tracks(name: impl Into<String>, duration: f32) -> Self {
        Self {
            name: name.into(),
            duration,
            tracks: std::collections::HashMap::new(),
        }
    }

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

/// Lit les clips d'animation du **premier skin** du glTF : chaque canal
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

/// Transform locale (T, R, S) d'un joint à un instant donné : pose de liaison, avec les
/// composantes que le clip anime réellement écrasées (`JointPose` — chaque
/// champ `Option`, jamais de valeur neutre inventée pour une composante non animée).
/// `clip = None` ⇒ pose de liaison pure. Partagé par `compute_joint_matrices` (un clip)
/// et `compute_joint_matrices_blended` (deux clips mélangés).
fn local_pose(
    joint: &Joint,
    joint_index: usize,
    clip: Option<&Clip>,
    time: f32,
) -> (Vec3, Quat, Vec3) {
    let (bind_t, bind_r, bind_s) = decompose(joint.bind_local);
    match clip {
        Some(clip) => {
            let pose = clip.sample_joint(joint_index, time);
            (
                pose.translation.unwrap_or(bind_t),
                pose.rotation.unwrap_or(bind_r),
                pose.scale.unwrap_or(bind_s),
            )
        }
        None => (bind_t, bind_r, bind_s),
    }
}

/// Résout la hiérarchie monde d'un squelette à partir d'une transform locale par joint —
/// partagé par `compute_joint_matrices` et `compute_joint_matrices_blended`,
/// qui ne diffèrent que par la façon dont `local_of` calcule cette transform (un clip,
/// ou un mélange de deux). Renvoie `monde_du_joint * inverse_bind` : la partie
/// `inverse_bind` annule la pose de liaison pour ne laisser que le **déplacement** depuis
/// cette pose, ce qui est ce qu'un sommet en espace de liaison doit subir.
///
/// Robuste à un ordre de `Skeleton::joints` où un parent n'est **pas** garanti apparaître
/// avant ses enfants (le glTF ne l'impose pas, même si c'est l'usage courant des
/// exportateurs) : résolution par vagues plutôt que par simple parcours linéaire.
fn resolve_world_matrices(
    skeleton: &Skeleton,
    local_of: impl Fn(usize, &Joint) -> Mat4,
) -> Vec<Mat4> {
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
            let local = local_of(i, joint);
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

/// Calcule, pour chaque joint d'un `Skeleton`, la matrice à envoyer au shader de skinning.
/// `clip = None` ⇒ pose de liaison pure (équivalent à un modèle statique :
/// chaque matrice résultante est proche de l'identité, à l'erreur de précision flottante
/// près — cf. test).
pub fn compute_joint_matrices(skeleton: &Skeleton, clip: Option<&Clip>, time: f32) -> Vec<Mat4> {
    resolve_world_matrices(skeleton, |i, joint| {
        let (t, r, s) = local_pose(joint, i, clip, time);
        Mat4::from_scale_rotation_translation(s, r, t)
    })
}

/// Comme `compute_joint_matrices`, mais mélange (crossfade) deux clips — transitions
/// douces entre états d'animation, ex. idle→run. Le mélange se fait au
/// niveau de la pose **locale** de chaque joint — translation/échelle en lerp, rotation
/// en nlerp — **avant** de composer la hiérarchie une seule fois avec le résultat.
/// Mélanger des matrices **monde** directement serait faux pour la rotation (une matrice
/// n'interpole pas linéairement comme un quaternion) ; mélanger au niveau local est la
/// pratique standard de blending d'animation squelettale.
///
/// `blend` : 0.0 = `clip_a` pur, 1.0 = `clip_b` pur, clampé entre les deux.
pub fn compute_joint_matrices_blended(
    skeleton: &Skeleton,
    clip_a: Option<&Clip>,
    time_a: f32,
    clip_b: Option<&Clip>,
    time_b: f32,
    blend: f32,
) -> Vec<Mat4> {
    let blend = blend.clamp(0.0, 1.0);
    resolve_world_matrices(skeleton, |i, joint| {
        let (ta, ra, sa) = local_pose(joint, i, clip_a, time_a);
        let (tb, rb, sb) = local_pose(joint, i, clip_b, time_b);
        Mat4::from_scale_rotation_translation(
            sa.lerp(sb, blend),
            nlerp(ra, rb, blend),
            ta.lerp(tb, blend),
        )
    })
}

#[cfg(test)]
pub(crate) mod tests {
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
    pub(crate) fn skinned_triangle_glb() -> Vec<u8> {
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
    pub(crate) fn unskinned_triangle_glb() -> Vec<u8> {
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
    /// Compteur atomique en plus du PID : `cargo test` exécute les tests d'un même
    /// binaire sur plusieurs **threads** du même processus, donc le PID seul ne suffit
    /// pas à distinguer deux tests utilisant le même `name` (cf. docs/audits/scene-import.md
    /// pour l'échec intermittent que ça causait).
    static TEMP_GLB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub(crate) fn write_temp_glb(bytes: &[u8], name: &str) -> std::path::PathBuf {
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
    pub(crate) fn animated_skinned_glb() -> Vec<u8> {
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
        // doit retomber sur la pose de liaison, pas sur une valeur inventée.
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

    #[test]
    fn blended_joint_matrices_at_the_extremes_match_the_unblended_result() {
        let path = write_temp_glb(&animated_skinned_glb(), "blend_extremes");
        let (skeleton, _) = load_gltf_skeleton(path.to_str().unwrap()).unwrap().unwrap();
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        let pure_a = compute_joint_matrices(&skeleton, Some(clip), 0.0);
        let blended_at_0 =
            compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, 0.0);
        for (p, b) in pure_a.iter().zip(&blended_at_0) {
            assert!(
                p.abs_diff_eq(*b, 1e-5),
                "blend=0.0 doit être identique au clip A pur : {p:?} vs {b:?}"
            );
        }

        let pure_b = compute_joint_matrices(&skeleton, Some(clip), 1.0);
        let blended_at_1 =
            compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, 1.0);
        for (p, b) in pure_b.iter().zip(&blended_at_1) {
            assert!(
                p.abs_diff_eq(*b, 1e-5),
                "blend=1.0 doit être identique au clip B pur : {p:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn blended_joint_matrices_at_midpoint_interpolate_translation() {
        let path = write_temp_glb(&animated_skinned_glb(), "blend_midpoint");
        let (skeleton, _) = load_gltf_skeleton(path.to_str().unwrap()).unwrap().unwrap();
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        // Joint 0 : translation linéaire de (0,0,0) à t=0 vers (10,0,0) à t=1. Mélanger
        // clip A (t=0, translation (0,0,0)) et clip B (t=1, translation (10,0,0)) à
        // blend=0.5 doit donner (5,0,0) — le lerp attendu.
        let matrices =
            compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, 0.5);
        let translation = matrices[0].col(3).truncate();
        assert!(
            (translation - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-4,
            "blend=0.5 doit donner le milieu de la translation, obtenu {translation:?}"
        );
    }

    #[test]
    fn blended_joint_matrices_clamp_out_of_range_blend_values() {
        let path = write_temp_glb(&animated_skinned_glb(), "blend_clamp");
        let (skeleton, _) = load_gltf_skeleton(path.to_str().unwrap()).unwrap().unwrap();
        let clips = load_gltf_clips(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        let clip = &clips[0];

        let over = compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, 5.0);
        let at_1 = compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, 1.0);
        for (o, a) in over.iter().zip(&at_1) {
            assert!(
                o.abs_diff_eq(*a, 1e-5),
                "blend > 1.0 doit être clampé à 1.0"
            );
        }
        let under =
            compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, -5.0);
        let at_0 = compute_joint_matrices_blended(&skeleton, Some(clip), 0.0, Some(clip), 1.0, 0.0);
        for (u, a) in under.iter().zip(&at_0) {
            assert!(
                u.abs_diff_eq(*a, 1e-5),
                "blend < 0.0 doit être clampé à 0.0"
            );
        }
    }

    /// Triangle dans le plan XY (normale +Z), UV alignés sur les axes monde (u ~ x,
    /// v ~ y) — la tangente attendue est donc +X, la bitangente +Y.
    fn axis_aligned_uv_triangle(mirrored: bool) -> (Vec<Vertex>, Vec<u32>) {
        let v_sign = if mirrored { -1.0 } else { 1.0 };
        let vertices = vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [1.0, 1.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, v_sign],
            },
        ];
        (vertices, vec![0, 1, 2])
    }

    #[test]
    fn compute_tangents_matches_world_x_for_axis_aligned_uvs() {
        let (vertices, indices) = axis_aligned_uv_triangle(false);
        let tangents = compute_tangents(&vertices, &indices);
        assert_eq!(tangents.len(), 3);
        for t in &tangents {
            let tangent = Vec3::new(t[0], t[1], t[2]);
            assert!(
                tangent.abs_diff_eq(Vec3::X, 1e-4),
                "tangente attendue ~+X pour un UV aligné sur les axes monde : {tangent:?}"
            );
            assert_eq!(t[3], 1.0, "bitangente droite (pas d'UV en miroir)");
        }
    }

    #[test]
    fn compute_tangents_is_orthogonal_to_the_normal() {
        // Sur un triangle non trivial (normale inclinée), la tangente calculée doit
        // rester dans le plan tangent — sinon Gram-Schmidt aurait un bug.
        let vertices = vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.3, 0.2, 0.9],
                color: [1.0; 3],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, 0.2, -0.1],
                normal: [0.3, 0.2, 0.9],
                color: [1.0; 3],
                uv: [1.0, 0.3],
            },
            Vertex {
                position: [0.1, 1.0, 0.05],
                normal: [0.3, 0.2, 0.9],
                color: [1.0; 3],
                uv: [0.2, 1.0],
            },
        ];
        let tangents = compute_tangents(&vertices, &[0, 1, 2]);
        for (t, v) in tangents.iter().zip(&vertices) {
            let n = Vec3::from_array(v.normal).normalize();
            let tangent = Vec3::new(t[0], t[1], t[2]);
            assert!(
                tangent.dot(n).abs() < 1e-4,
                "la tangente doit être orthogonale à la normale : dot={}",
                tangent.dot(n)
            );
            assert!(
                (tangent.length() - 1.0).abs() < 1e-4,
                "la tangente doit être normalisée : longueur={}",
                tangent.length()
            );
        }
    }

    #[test]
    fn compute_tangents_flips_handedness_on_mirrored_uvs() {
        // UV en miroir (v inversé) : cas fréquent sur un mesh symétrique (les deux
        // moitiés partagent une texture retournée) — le signe de la bitangente doit
        // suivre, sinon le normal mapping serait incohérent d'un côté à l'autre.
        let (vertices, indices) = axis_aligned_uv_triangle(true);
        let tangents = compute_tangents(&vertices, &indices);
        for t in &tangents {
            assert_eq!(t[3], -1.0, "bitangente inversée pour un UV en miroir");
        }
    }

    #[test]
    fn compute_tangents_returns_one_entry_per_vertex_even_with_degenerate_triangles() {
        // Triangle dégénéré (UV identiques partout, `det` ~ 0) : ne doit ni paniquer
        // ni produire une tangente NaN/explosée — juste une valeur arbitraire stable.
        let vertices = vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                color: [1.0; 3],
                uv: [0.5, 0.5],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                color: [1.0; 3],
                uv: [0.5, 0.5],
            },
            Vertex {
                position: [0.0, 0.0, 1.0],
                normal: [0.0, 1.0, 0.0],
                color: [1.0; 3],
                uv: [0.5, 0.5],
            },
        ];
        let tangents = compute_tangents(&vertices, &[0, 1, 2]);
        assert_eq!(tangents.len(), 3);
        for t in &tangents {
            assert!(
                t[0].is_finite() && t[1].is_finite() && t[2].is_finite(),
                "tangente non finie sur triangle dégénéré : {t:?}"
            );
        }
    }

    /// Preuve sur le **vrai** asset (audit du bug « bras/tête déformés en virage »,
    /// finalement causé par le round-trip Euler des scripts, cf.
    /// `app::scripting::canonical_euler_xyz`) : les données de peau de
    /// `creature.glb` sont saines — poids normalisés, indices dans la palette — et
    /// le skinning CPU reste borné à travers une grille de fondus Walk↔Idle : aucune
    /// combinaison (temps × temps × blend) ne projette un sommet loin de la bbox de
    /// liaison, ce qui exclut toute déformation « qui part en couille » d'origine
    /// données/blending, quel que soit l'enchaînement de transitions en jeu.
    #[test]
    fn creature_glb_skin_weights_and_walk_idle_blends_stay_bounded() {
        assert_creature_glb_sane(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/models/creature.glb"
        ));
    }

    /// Même preuve pour la créature n°2 (quadrupède `creature2.glb`, généré sous
    /// Blender comme le n°1 mais avec un rig différent — pattes/queue au lieu de
    /// bras/jambes) : les données de skinning et les fondus Walk↔Idle doivent être
    /// tout aussi sains, le pipeline d'import ne fait aucun cas particulier.
    #[test]
    fn creature2_glb_skin_weights_and_walk_idle_blends_stay_bounded() {
        assert_creature_glb_sane(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/models/creature2.glb"
        ));
    }

    /// Même preuve pour la créature n°3 (bipède trapu `creature3.glb`, troisième
    /// style — rig Root/Body/Head/Crest/ArmL/ArmR/LegL/LegR).
    #[test]
    fn creature3_glb_skin_weights_and_walk_idle_blends_stay_bounded() {
        assert_creature_glb_sane(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/models/creature3.glb"
        ));
    }

    /// Même preuve pour la créature n°4 (quadrupède tortue/roche `creature4.glb`,
    /// quatrième style — rig Root/Body/Head/Shell/LegFL/LegFR/LegBL/LegBR).
    #[test]
    fn creature4_glb_skin_weights_and_walk_idle_blends_stay_bounded() {
        assert_creature_glb_sane(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/models/creature4.glb"
        ));
    }

    /// Même preuve pour la créature n°5 (oisillon `creature5.glb`, cinquième
    /// style — rig Root/Body/Head/WingL/WingR/LegL/LegR/Tail).
    #[test]
    fn creature5_glb_skin_weights_and_walk_idle_blends_stay_bounded() {
        assert_creature_glb_sane(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/models/creature5.glb"
        ));
    }

    /// Même preuve pour les créatures n°6 à 10 (chauve-souris, crabe,
    /// salamandre, souris électrique, limace-champignon) — une seule boucle
    /// plutôt que cinq tests copiés-collés, le chemin en échec est dans le
    /// message d'assertion de `assert_creature_glb_sane`.
    #[test]
    fn creatures_6_to_10_glb_skin_weights_and_walk_idle_blends_stay_bounded() {
        for n in 6..=20 {
            assert_creature_glb_sane(&format!(
                "{}/assets/models/creature{n}.glb",
                env!("CARGO_MANIFEST_DIR")
            ));
        }
    }

    fn assert_creature_glb_sane(path: &str) {
        let (data, aabb_min, aabb_max) = load_gltf(path).expect("glb de créature lisible");
        let mut m = crate::scene::ImportedMesh {
            path: path.to_string(),
            data,
            aabb_min,
            aabb_max,
            ..Default::default()
        };
        m.load_skinning();
        let skeleton = m.skeleton.as_ref().expect("creature.glb skinné");
        let n_joints = skeleton.joints.len();
        let walk = m
            .clips
            .iter()
            .find(|c| c.name == "Walk")
            .expect("clip Walk");
        let idle = m
            .clips
            .iter()
            .find(|c| c.name == "Idle")
            .expect("clip Idle");

        for (i, s) in m.vertex_skins.iter().enumerate() {
            let sum: f32 = s.weights.iter().sum();
            assert!(
                (0.99..=1.01).contains(&sum),
                "sommet {i} : somme des poids {sum} (attendu ≈ 1.0, {:?})",
                s.weights
            );
            for (&j, &w) in s.joints.iter().zip(&s.weights) {
                assert!(
                    w <= 0.0 || (j as usize) < n_joints,
                    "sommet {i} : joint {j} hors palette ({n_joints} joints, poids {w})"
                );
            }
        }

        let center = (aabb_min + aabb_max) * 0.5;
        let half_diag = (aabb_max - aabb_min).length() * 0.5;
        for wi in 0..=4 {
            for ii in 0..=2 {
                for bi in 0..=5 {
                    let tw = walk.duration * wi as f32 / 4.0;
                    let ti = idle.duration * ii as f32 / 2.0;
                    let b = bi as f32 / 5.0;
                    let mats =
                        compute_joint_matrices_blended(skeleton, Some(walk), tw, Some(idle), ti, b);
                    for (i, (v, s)) in m.data.vertices.iter().zip(&m.vertex_skins).enumerate() {
                        let p = Vec3::from(v.position);
                        let mut out = Vec3::ZERO;
                        for (&j, &w) in s.joints.iter().zip(&s.weights) {
                            out += (mats[j as usize] * p.extend(1.0)).truncate() * w;
                        }
                        let d = out.distance(center);
                        assert!(
                            d <= half_diag * 2.0,
                            "blend Walk@{tw:.2}/Idle@{ti:.2} b={b:.1} : sommet {i} projeté \
                             à {d:.2} du centre (demi-diag bbox {half_diag:.2}) — skinning aberrant"
                        );
                    }
                }
            }
        }
    }
}
