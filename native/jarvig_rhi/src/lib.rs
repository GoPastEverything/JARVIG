//! JARVIG's public rendering boundary.
//!
//! Code in this crate must not name a GPU library. A backend implements [`Instance`]
//! in its own crate and keeps that library's types private. The first GPU backend
//! is wgpu, and it lives in `jarvig_rhi_wgpu`. It is not this API.

mod graphics_device;
mod null;
mod registry;
mod types;

pub use registry::Registry;

pub use graphics_device::{select_graphics_adapter, GraphicsRequirements, GraphicsSelection};
pub use null::create_null_instance;
pub use types::*;

#[cfg(test)]
mod boundary_tests {
    #[test]
    fn this_crate_does_not_depend_on_a_gpu_library() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("wgpu"));
        assert!(!manifest.contains("ash"));
        assert!(!manifest.contains("d3d12"));
    }
}
