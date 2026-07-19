//! Couche **rendu pur** (wgpu + egui). Ne contient aucun état métier : la scène,
//! la caméra et la sélection vivent dans `AppState` et sont passées à `render`.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use super::lod::foliage_lod_mesh;
use super::mesh::GpuMesh;
use super::passes::{
    aabb_visible, culling_radius_for, distance_visible, frustum_planes, is_skinned, mesh_key,
    render_input_hash,
};
#[cfg(test)]
use super::pipelines::mip_count_for;
use super::pipelines::{
    self, PipelineBundle, create_bloom_mip_views, create_depth_view, create_hdr_view,
    create_models_buffer, create_skinned_models_bind_group, load_rgba, make_texture,
};
use crate::app::{AppState, GIZMO_LEN, GizmoMode, RING_SEGMENTS, axis_basis, axis_dir};
use crate::editor::Editor;
use crate::scene::{MeshKind, Scene};
use crate::time_compat::Instant;

mod types;
pub use types::Renderer;
pub(crate) use types::*;

mod resources;

mod shadows;

mod sync;

mod post_process;

mod frame;

mod headless;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
