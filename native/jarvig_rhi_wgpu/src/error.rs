use jarvig_rhi::RhiError;

pub fn map_surface_error(error: wgpu::SurfaceError) -> RhiError {
    match error {
        wgpu::SurfaceError::Outdated => RhiError::SurfaceOutdated,
        wgpu::SurfaceError::Lost => RhiError::SurfaceLost,
        wgpu::SurfaceError::Timeout => RhiError::SurfaceTimeout,
        wgpu::SurfaceError::OutOfMemory => RhiError::DeviceLost,
        wgpu::SurfaceError::Other => RhiError::DeviceLost,
    }
}

pub fn map_request(error: wgpu::RequestDeviceError) -> RhiError {
    RhiError::Validation(error.to_string())
}
