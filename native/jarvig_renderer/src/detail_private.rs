//! Public detail draw path.
//!
//! Procedural detail is off unless the host asks for the reference provider.
//! This file does not upload a private patch set and does not shade a private rule.
use super::*;

impl super::Renderer {
    pub(super) fn sync_microtriangles(&mut self, _pass: &Prepared, _snapshot: &RenderSceneSnapshot, _meshes: &MeshLibrary) -> Result<(), RenderError> {
        if !self.micro_enabled {
            return Ok(());
        }
        let provider = jarvig_core::public_detail(self.micro_surface);
        let mesh = provider.build(&[], self.micro_seed, false, 0.0, 0.0, &|| false).unwrap_or_else(|_| jarvig_core::MicroMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            spans: Vec::new(),
            patches: 0,
            samples: 0,
            ordinary: 0,
            triangle_count: 0,
            vertex_count: 0,
            fallbacks: 0,
            max_displacement_m: 0.0,
            fingerprint: 0,
            generation_us: 0,
            invalid: 0,
        });
        self.gpu_scene.stats.micro_triangles = mesh.triangle_count;
        self.gpu_scene.stats.micro_patches = mesh.patches;
        self.micro_stage = "Ready";
        Ok(())
    }

    pub(super) fn publish_micro_absence(&mut self) {
        self.micro_request = None;
        self.micro_pending = None;
        self.micro_live.clear();
        self.micro_index_count = 0;
        self.micro_input_key = 0;
        self.micro_stage = "Ready";
    }

    pub(super) fn clear_drawn_micro_stats(&mut self) {
        self.gpu_scene.stats.micro_patches = 0;
        self.gpu_scene.stats.micro_samples = 0;
        self.gpu_scene.stats.micro_vertices = 0;
        self.gpu_scene.stats.micro_triangles = 0;
        self.gpu_scene.stats.micro_generation_us = 0;
        self.gpu_scene.stats.micro_upload_us = 0;
        self.gpu_scene.stats.micro_fallbacks = 0;
        self.gpu_scene.stats.micro_ordinary = 0;
        self.gpu_scene.stats.micro_invalid = 0;
        self.gpu_scene.stats.micro_reused = 0;
        self.gpu_scene.stats.micro_fingerprint = 0;
        self.gpu_scene.stats.micro_max_displacement_um = 0;
        self.gpu_scene.stats.micro_partial = 0;
    }

    pub(super) fn note_micro_stale(&mut self) {
        self.micro_stale_discarded = self.micro_stale_discarded.saturating_add(1);
        self.gpu_scene.stats.micro_stale_discarded = self.micro_stale_discarded;
    }

    pub(super) fn write_published_stats(&mut self, mesh: &jarvig_core::MicroMesh, upload_us: u32) {
        self.gpu_scene.stats.micro_patches = mesh.patches;
        self.gpu_scene.stats.micro_triangles = mesh.triangle_count;
        self.gpu_scene.stats.micro_vertices = mesh.vertex_count;
        self.gpu_scene.stats.micro_upload_us = upload_us;
        self.gpu_scene.stats.micro_fingerprint = mesh.fingerprint;
    }

    pub(super) fn discard_pending(&mut self, stale: bool) -> Result<(), RenderError> {
        if self.micro_pending.take().is_some() && stale {
            self.note_micro_stale();
        }
        Ok(())
    }

    pub(super) fn destroy_live(&mut self) -> Result<(), RenderError> {
        let live = std::mem::take(&mut self.micro_live);
        self.micro_index_count = 0;
        for chunk in live {
            self.device.destroy(ResourceKind::Buffer, chunk.vertices.raw()).map_err(RenderError::Rhi)?;
            self.device.destroy(ResourceKind::Buffer, chunk.indices.raw()).map_err(RenderError::Rhi)?;
        }
        Ok(())
    }

    pub(super) fn draw_microtriangles(&mut self, _encoder: &mut dyn jarvig_rhi::CommandEncoder, _pass: &Prepared) -> Result<(), RenderError> {
        Ok(())
    }

    pub(super) fn draw_einstein_debug(&mut self, _encoder: &mut dyn jarvig_rhi::CommandEncoder, _pass: &Prepared) -> Result<(), RenderError> {
        Ok(())
    }

    pub fn accept_micro_build(&mut self, _epoch: u64, _key: u64, _mesh: jarvig_core::MicroMesh, _queue_wait_us: u32, _skipped: u32) -> bool {
        false
    }
}
