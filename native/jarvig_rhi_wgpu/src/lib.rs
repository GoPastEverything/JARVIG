//! Private wgpu backend. Nothing outside this crate may name `wgpu`.

mod error;
mod graphics;

pub use graphics::{attach_window, describe_adapters, AdapterSurvey};

#[cfg(test)]
mod tests {
    use super::error::map_surface_error;
    use jarvig_rhi::RhiError;

    #[test]
    fn surface_errors_become_jarvig_errors() {
        assert_eq!(map_surface_error(wgpu::SurfaceError::Outdated), RhiError::SurfaceOutdated);
        assert_eq!(map_surface_error(wgpu::SurfaceError::Lost), RhiError::SurfaceLost);
        assert_eq!(map_surface_error(wgpu::SurfaceError::Timeout), RhiError::SurfaceTimeout);
        assert_eq!(map_surface_error(wgpu::SurfaceError::OutOfMemory), RhiError::DeviceLost);
    }
}
