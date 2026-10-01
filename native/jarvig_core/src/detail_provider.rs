//! Generic procedural-detail seam.
//!
//! Hosts ask this module for a detail provider. The default provider draws nothing.
//! `ReferenceDetail` is a flat marker with no surface rule. A private build may
//! register another provider behind [`ProceduralMicrogeometry`] without putting
//! that provider's source in a public tree.

use crate::microgeometry::{
    projected_detail_px, MicroBudget, MicroMesh, MicroSpan, SurfaceAnchor, DETAIL_ERROR_THRESHOLD_PX, DETAIL_FEATURE_SIZE_M,
};

/// One procedural-detail implementation. Object-safe so a host can select it by name.
pub trait ProceduralMicrogeometry: Send + Sync {
    fn id(&self) -> &'static str;
    fn budget(&self) -> MicroBudget;
    fn build(
        &self,
        anchors: &[SurfaceAnchor],
        seed: u64,
        enabled: bool,
        viewport_height: f32,
        tan_half_fov: f32,
        cancel: &dyn Fn() -> bool,
    ) -> Result<MicroMesh, String>;
}

/// Draws nothing. This is the public default.
#[derive(Clone, Copy, Debug, Default)]
pub struct DisabledDetail;

/// One flat triangle in the surface tangent plane. Displacement is zero.
/// There is no tiling rule and no seed pattern.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReferenceDetail;

impl ProceduralMicrogeometry for DisabledDetail {
    fn id(&self) -> &'static str {
        "disabled"
    }

    fn budget(&self) -> MicroBudget {
        MicroBudget { max_patches: 0, max_vertices: 0, max_triangles: 0, amplitude_m: 0.0, patch_size_m: 0.0 }
    }

    fn build(
        &self,
        anchors: &[SurfaceAnchor],
        seed: u64,
        _enabled: bool,
        _viewport_height: f32,
        _tan_half_fov: f32,
        _cancel: &dyn Fn() -> bool,
    ) -> Result<MicroMesh, String> {
        Ok(empty_mesh(seed, anchors.len() as u32))
    }
}

impl ProceduralMicrogeometry for ReferenceDetail {
    fn id(&self) -> &'static str {
        "reference"
    }

    fn budget(&self) -> MicroBudget {
        MicroBudget { max_patches: 1, max_vertices: 3, max_triangles: 1, amplitude_m: 0.0, patch_size_m: 0.05 }
    }

    fn build(
        &self,
        anchors: &[SurfaceAnchor],
        seed: u64,
        enabled: bool,
        viewport_height: f32,
        tan_half_fov: f32,
        cancel: &dyn Fn() -> bool,
    ) -> Result<MicroMesh, String> {
        let mut mesh = empty_mesh(seed, 0);
        if !enabled {
            mesh.ordinary = anchors.len() as u32;
            return Ok(mesh);
        }
        let budget = self.budget();
        for anchor in anchors {
            if cancel() {
                return Err("cancelled".into());
            }
            mesh.samples = mesh.samples.saturating_add(1);
            let pixels = projected_detail_px(DETAIL_FEATURE_SIZE_M, anchor.depth_m, viewport_height, tan_half_fov);
            if pixels <= DETAIL_ERROR_THRESHOLD_PX {
                mesh.ordinary = mesh.ordinary.saturating_add(1);
                continue;
            }
            if mesh.patches >= budget.max_patches {
                break;
            }
            let Some(frame) = tangent_frame(anchor.normal, anchor.tangent) else {
                mesh.fallbacks = mesh.fallbacks.saturating_add(1);
                continue;
            };
            let corners = [[-1.0, -1.0], [1.0, -1.0], [0.0, 1.0]];
            let base = mesh.vertex_count;
            for corner in corners {
                let position = [
                    anchor.position[0] + frame.0[0] * corner[0] * budget.patch_size_m + frame.1[0] * corner[1] * budget.patch_size_m,
                    anchor.position[1] + frame.0[1] * corner[0] * budget.patch_size_m + frame.1[1] * corner[1] * budget.patch_size_m,
                    anchor.position[2] + frame.0[2] * corner[0] * budget.patch_size_m + frame.1[2] * corner[1] * budget.patch_size_m,
                ];
                if !position.iter().all(|lane| lane.is_finite()) {
                    mesh.fallbacks = mesh.fallbacks.saturating_add(1);
                    continue;
                }
                push_vertex(&mut mesh.vertices, position, anchor.normal, anchor.uv, anchor.tangent);
                mesh.vertex_count = mesh.vertex_count.saturating_add(1);
            }
            if mesh.vertex_count.saturating_sub(base) != 3 {
                mesh.vertex_count = base;
                mesh.vertices.truncate(base as usize * 60);
                mesh.fallbacks = mesh.fallbacks.saturating_add(1);
                continue;
            }
            let index_start = mesh.indices.len() as u32;
            mesh.indices.extend([base, base + 1, base + 2]);
            mesh.triangle_count = mesh.triangle_count.saturating_add(1);
            mesh.patches = mesh.patches.saturating_add(1);
            mesh.spans.push(MicroSpan { vertex_start: base, vertex_count: 3, index_start, index_count: 3 });
            break;
        }
        Ok(mesh)
    }
}

/// Public resolver. Private builds replace the meaning of `surface` in their own module.
pub fn public_detail(surface: bool) -> Box<dyn ProceduralMicrogeometry> {
    if surface {
        Box::new(ReferenceDetail)
    } else {
        Box::new(DisabledDetail)
    }
}

fn empty_mesh(seed: u64, ordinary: u32) -> MicroMesh {
    MicroMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        spans: Vec::new(),
        patches: 0,
        samples: 0,
        ordinary,
        triangle_count: 0,
        vertex_count: 0,
        fallbacks: 0,
        max_displacement_m: 0.0,
        fingerprint: 0xcbf29ce484222325u64 ^ seed,
        generation_us: 0,
        invalid: 0,
    }
}

fn tangent_frame(normal: [f32; 3], tangent: [f32; 4]) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    let normal = normalize3(normal)?;
    let tangent_xyz = normalize3([tangent[0], tangent[1], tangent[2]])?;
    let sign = if tangent[3] < 0.0 { -1.0 } else { 1.0 };
    let bitangent = scale3(normalize3(cross(normal, tangent_xyz))?, sign);
    Some((tangent_xyz, bitangent, normal))
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[1] * right[2] - left[2] * right[1], left[2] * right[0] - left[0] * right[2], left[0] * right[1] - left[1] * right[0]]
}

fn scale3(value: [f32; 3], scale: f32) -> [f32; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn normalize3(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length < 1.0e-6 || !length.is_finite() {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3], uv: [f32; 2], tangent: [f32; 4]) {
    for value in position.into_iter().chain([1.0, 1.0, 1.0]).chain(uv).chain(normal).chain(tangent) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor() -> SurfaceAnchor {
        SurfaceAnchor {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.2, 0.4],
            depth_m: 0.4,
        }
    }

    #[test]
    fn disabled_detail_draws_nothing_and_reference_detail_stays_flat() {
        let off = DisabledDetail.build(&[anchor()], 1, true, 600.0, 0.4, &|| false).unwrap();
        assert_eq!(off.triangle_count, 0);
        assert_eq!(off.max_displacement_m, 0.0);
        let demo = ReferenceDetail.build(&[anchor()], 1, true, 600.0, 0.4, &|| false).unwrap();
        assert_eq!(demo.triangle_count, 1);
        assert_eq!(demo.max_displacement_m, 0.0);
        assert_eq!(public_detail(false).id(), "disabled");
        assert_eq!(public_detail(true).id(), "reference");
    }
}
