//! JARVIG graphics-adapter policy. No backend types.
//!
//! A runtime id is not a saved preference. The fingerprint is vendor, device,
//! API, class, and name. Auto on a presenting native host is high performance.
//! Software is a last resort, and only when the config allows it.

use crate::{
    AdapterFingerprint, AdapterKind, GraphicsAdapterDesc, GraphicsApi, GraphicsDeviceConfig,
    GraphicsPreference, RhiError,
};

/// What a presenting device must satisfy today. Not mesh shaders or ray tracing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsRequirements {
    pub require_surface: bool,
    pub min_texture_dimension_2d: u32,
    pub min_storage_buffers_per_shader_stage: u32,
}

impl GraphicsRequirements {
    pub const PRESENTING: Self = Self {
        require_surface: true,
        min_texture_dimension_2d: 2048,
        min_storage_buffers_per_shader_stage: 1,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsSelection {
    pub preference: GraphicsPreference,
    pub adapter: GraphicsAdapterDesc,
    pub reason: String,
    pub fell_back: bool,
    pub software: bool,
}

pub fn select_graphics_adapter(
    adapters: &[GraphicsAdapterDesc],
    config: &GraphicsDeviceConfig,
    requirements: GraphicsRequirements,
) -> Result<GraphicsSelection, RhiError> {
    if let GraphicsPreference::Specific(fingerprint) = &config.preference {
        return select_specific(adapters, fingerprint, requirements);
    }
    let performance = !matches!(config.preference, GraphicsPreference::LowPower);
    let mut hardware = Vec::new();
    let mut software = Vec::new();
    for adapter in adapters {
        if !candidate_ok(adapter, requirements) {
            continue;
        }
        if adapter.software || adapter.kind == AdapterKind::Cpu {
            software.push(adapter);
        } else {
            hardware.push(adapter);
        }
    }
    let (pool, software_pick) = if !hardware.is_empty() {
        (hardware, false)
    } else if config.allow_software_fallback && !software.is_empty() {
        (software, true)
    } else if hardware.is_empty() && !software.is_empty() {
        return Err(RhiError::Validation(
            "no compatible hardware adapter, and software fallback is disabled".into(),
        ));
    } else {
        return Err(RhiError::Validation("no compatible graphics adapter".into()));
    };
    let mut ranked = pool;
    ranked.sort_by(|left, right| rank(right, performance).cmp(&rank(left, performance)));
    let chosen = ranked[0];
    let wanted = if performance { AdapterKind::DiscreteGpu } else { AdapterKind::IntegratedGpu };
    let fell_back = software_pick || chosen.kind != wanted;
    let mut reason = if software_pick {
        "SOFTWARE FALLBACK: no compatible hardware adapter".to_string()
    } else if chosen.kind == wanted {
        if performance {
            "preferred compatible discrete adapter".to_string()
        } else {
            "preferred compatible integrated adapter".to_string()
        }
    } else if performance {
        format!("no compatible discrete adapter; using the best compatible {}", class_name(chosen.kind))
    } else {
        format!("no compatible integrated adapter; using the best compatible {}", class_name(chosen.kind))
    };
    if matches!(config.preference, GraphicsPreference::Auto) {
        reason = format!("auto uses the desktop high-performance policy; {reason}");
    }
    Ok(GraphicsSelection {
        preference: config.preference.clone(),
        adapter: chosen.clone(),
        reason,
        fell_back,
        software: software_pick,
    })
}

fn select_specific(
    adapters: &[GraphicsAdapterDesc],
    fingerprint: &AdapterFingerprint,
    requirements: GraphicsRequirements,
) -> Result<GraphicsSelection, RhiError> {
    let matches: Vec<_> = adapters.iter().filter(|adapter| fingerprint_matches(adapter, fingerprint)).collect();
    let label = format!(
        "{:04x}:{:04x}:{}",
        fingerprint.vendor_id, fingerprint.device_id, fingerprint.api.label()
    );
    if matches.is_empty() {
        return Err(RhiError::Validation(format!("specific adapter {label} was not found")));
    }
    let eligible: Vec<_> = matches.iter().copied().filter(|adapter| candidate_ok(adapter, requirements)).collect();
    if eligible.is_empty() {
        let why = matches[0].rejection.clone().unwrap_or_else(|| "incompatible with the target".into());
        return Err(RhiError::Validation(format!("specific adapter {label} cannot be used: {why}")));
    }
    let mut ranked = eligible;
    ranked.sort_by(|left, right| rank(right, true).cmp(&rank(left, true)));
    let chosen = ranked[0];
    Ok(GraphicsSelection {
        preference: GraphicsPreference::Specific(fingerprint.clone()),
        adapter: chosen.clone(),
        reason: format!("requested adapter {label}"),
        fell_back: false,
        software: chosen.software || chosen.kind == AdapterKind::Cpu,
    })
}

fn candidate_ok(adapter: &GraphicsAdapterDesc, requirements: GraphicsRequirements) -> bool {
    adapter.meets_requirements
        && adapter.max_texture_dimension_2d >= requirements.min_texture_dimension_2d
        && (!requirements.require_surface || adapter.surface_compatible)
}

fn fingerprint_matches(adapter: &GraphicsAdapterDesc, fingerprint: &AdapterFingerprint) -> bool {
    adapter.vendor_id == fingerprint.vendor_id
        && adapter.device_id == fingerprint.device_id
        && adapter.api == fingerprint.api
        && fingerprint.kind.map(|kind| kind == adapter.kind).unwrap_or(true)
        && fingerprint.name.as_ref().map(|name| name == &adapter.name).unwrap_or(true)
}

fn class_name(kind: AdapterKind) -> &'static str {
    match kind {
        AdapterKind::DiscreteGpu => "discrete adapter",
        AdapterKind::IntegratedGpu => "integrated adapter",
        AdapterKind::VirtualGpu => "virtual adapter",
        AdapterKind::Cpu => "software adapter",
        AdapterKind::Unknown => "unknown adapter",
    }
}

fn rank(adapter: &GraphicsAdapterDesc, performance: bool) -> (i32, u32, u32, i32, i32, i32, std::cmp::Reverse<String>) {
    let class = match (performance, adapter.kind) {
        (true, AdapterKind::DiscreteGpu) => 400,
        (true, AdapterKind::IntegratedGpu) => 300,
        (true, AdapterKind::Unknown) => 200,
        (true, AdapterKind::VirtualGpu) => 100,
        (true, AdapterKind::Cpu) => 0,
        (false, AdapterKind::IntegratedGpu) => 400,
        (false, AdapterKind::DiscreteGpu) => 300,
        (false, AdapterKind::Unknown) => 200,
        (false, AdapterKind::VirtualGpu) => 100,
        (false, AdapterKind::Cpu) => 0,
    };
    let api = match adapter.api {
        GraphicsApi::Dx12 => 5,
        GraphicsApi::Vulkan => 4,
        GraphicsApi::Metal => 3,
        GraphicsApi::OpenGl => 2,
        GraphicsApi::BrowserWebGpu => 1,
        GraphicsApi::Null | GraphicsApi::Unknown => 0,
    };
    (
        class,
        adapter.max_texture_dimension_2d,
        adapter.max_storage_buffers_per_shader_stage,
        api,
        -(adapter.vendor_id as i32),
        -(adapter.device_id as i32),
        std::cmp::Reverse(adapter.name.clone()),
    )
}

#[cfg(test)]
fn adapter_template(name: &str, kind: AdapterKind) -> GraphicsAdapterDesc {
    GraphicsAdapterDesc {
        runtime_id: crate::AdapterId(0),
        name: name.into(),
        vendor_id: 0,
        device_id: 0,
        kind,
        backend: crate::BackendKind::Wgpu,
        api: GraphicsApi::Dx12,
        driver: String::new(),
        driver_info: String::new(),
        max_texture_dimension_2d: 8192,
        max_storage_buffers_per_shader_stage: 8,
        timestamp_queries: false,
        surface_compatible: true,
        meets_requirements: true,
        software: kind == AdapterKind::Cpu,
        rejection: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{adapter_template, select_graphics_adapter, GraphicsRequirements};
    use super::*;
    use crate::AdapterId;

    fn gpu(name: &str, kind: AdapterKind, vendor: u32, device: u32) -> GraphicsAdapterDesc {
        let mut adapter = adapter_template(name, kind);
        adapter.vendor_id = vendor;
        adapter.device_id = device;
        adapter.runtime_id = AdapterId(u64::from(vendor) << 16 | u64::from(device));
        adapter
    }

    fn high() -> GraphicsDeviceConfig {
        GraphicsDeviceConfig { preference: GraphicsPreference::HighPerformance, allow_software_fallback: true }
    }

    fn low() -> GraphicsDeviceConfig {
        GraphicsDeviceConfig { preference: GraphicsPreference::LowPower, allow_software_fallback: true }
    }

    #[test]
    fn high_performance_prefers_discrete_and_low_power_prefers_integrated() {
        let adapters = [
            gpu("integrated", AdapterKind::IntegratedGpu, 0x8086, 1),
            gpu("discrete", AdapterKind::DiscreteGpu, 0x1002, 2),
        ];
        let fast = select_graphics_adapter(&adapters, &high(), GraphicsRequirements::PRESENTING).unwrap();
        let quiet = select_graphics_adapter(&adapters, &low(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(fast.adapter.name, "discrete");
        assert!(!fast.fell_back);
        assert_eq!(quiet.adapter.name, "integrated");
        assert!(!quiet.fell_back);
        let auto = GraphicsDeviceConfig::default();
        let automatic = select_graphics_adapter(&adapters, &auto, GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(automatic.adapter.name, "discrete");
        assert!(automatic.reason.starts_with("auto uses the desktop high-performance policy"));
    }

    #[test]
    fn missing_class_falls_back_and_names_the_reason() {
        let only_integrated = [gpu("integrated", AdapterKind::IntegratedGpu, 1, 1)];
        let fast = select_graphics_adapter(&only_integrated, &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(fast.adapter.name, "integrated");
        assert!(fast.fell_back);
        assert!(fast.reason.contains("no compatible discrete"));
        let only_discrete = [gpu("discrete", AdapterKind::DiscreteGpu, 2, 2)];
        let quiet = select_graphics_adapter(&only_discrete, &low(), GraphicsRequirements::PRESENTING).unwrap();
        assert!(quiet.fell_back);
        assert!(quiet.reason.contains("no compatible integrated"));
    }

    #[test]
    fn software_is_last_and_an_incompatible_discrete_is_skipped() {
        let mut software = gpu("software", AdapterKind::Cpu, 3, 3);
        software.software = true;
        let hardware = gpu("integrated", AdapterKind::IntegratedGpu, 1, 1);
        let both = [software.clone(), hardware];
        let fast = select_graphics_adapter(&both, &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(fast.adapter.name, "integrated");
        assert!(!fast.software);
        let only_soft = [software];
        let fallen = select_graphics_adapter(&only_soft, &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert!(fallen.software);
        assert!(fallen.reason.contains("SOFTWARE FALLBACK"));
        let mut blocked = high();
        blocked.allow_software_fallback = false;
        assert!(select_graphics_adapter(&only_soft, &blocked, GraphicsRequirements::PRESENTING).is_err());

        let mut discrete = gpu("discrete", AdapterKind::DiscreteGpu, 9, 9);
        discrete.surface_compatible = false;
        discrete.rejection = Some("cannot present to the window surface".into());
        let mixed = [discrete, gpu("integrated", AdapterKind::IntegratedGpu, 1, 1)];
        let chosen = select_graphics_adapter(&mixed, &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(chosen.adapter.name, "integrated");
        assert!(chosen.fell_back);
    }

    #[test]
    fn specific_selection_fails_closed() {
        let adapters = [
            gpu("integrated", AdapterKind::IntegratedGpu, 0x8086, 1),
            gpu("discrete", AdapterKind::DiscreteGpu, 0x1002, 2),
        ];
        let fingerprint = AdapterFingerprint {
            vendor_id: 0x1002,
            device_id: 2,
            api: GraphicsApi::Dx12,
            kind: None,
            name: None,
        };
        let config = GraphicsDeviceConfig {
            preference: GraphicsPreference::Specific(fingerprint),
            allow_software_fallback: true,
        };
        let chosen = select_graphics_adapter(&adapters, &config, GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(chosen.adapter.name, "discrete");
        let missing = GraphicsDeviceConfig {
            preference: GraphicsPreference::Specific(AdapterFingerprint {
                vendor_id: 0x10de,
                device_id: 9,
                api: GraphicsApi::Dx12,
                kind: None,
                name: None,
            }),
            allow_software_fallback: true,
        };
        let error = select_graphics_adapter(&adapters, &missing, GraphicsRequirements::PRESENTING).unwrap_err();
        assert!(error.to_string().contains("was not found"));
        let mut broken = adapters[1].clone();
        broken.meets_requirements = false;
        broken.rejection = Some("limits are below the engine default".into());
        let bad = GraphicsDeviceConfig {
            preference: GraphicsPreference::Specific(AdapterFingerprint {
                vendor_id: 0x1002,
                device_id: 2,
                api: GraphicsApi::Dx12,
                kind: Some(AdapterKind::DiscreteGpu),
                name: Some("discrete".into()),
            }),
            allow_software_fallback: true,
        };
        let rejected = select_graphics_adapter(&[broken, adapters[0].clone()], &bad, GraphicsRequirements::PRESENTING).unwrap_err();
        let text = rejected.to_string();
        assert!(text.contains("cannot be used"));
        assert!(!text.contains("integrated"));
    }

    #[test]
    fn equal_classes_break_ties_without_using_a_vendor_preference() {
        let mut small = gpu("b-discrete", AdapterKind::DiscreteGpu, 0x1002, 2);
        small.max_texture_dimension_2d = 4096;
        let mut large = gpu("a-discrete", AdapterKind::DiscreteGpu, 0x10de, 1);
        large.max_texture_dimension_2d = 16384;
        let chosen = select_graphics_adapter(&[small, large], &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(chosen.adapter.name, "a-discrete");
        let mut first = gpu("z", AdapterKind::DiscreteGpu, 0x1002, 5);
        let mut second = gpu("a", AdapterKind::DiscreteGpu, 0x10de, 1);
        first.max_texture_dimension_2d = 8192;
        second.max_texture_dimension_2d = 8192;
        first.max_storage_buffers_per_shader_stage = 8;
        second.max_storage_buffers_per_shader_stage = 8;
        let tied = select_graphics_adapter(&[second.clone(), first.clone()], &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(tied.adapter.vendor_id, 0x1002);
        let again = select_graphics_adapter(&[first, second], &high(), GraphicsRequirements::PRESENTING).unwrap();
        assert_eq!(again.adapter.vendor_id, tied.adapter.vendor_id);
        assert_eq!(again.adapter.name, tied.adapter.name);
    }
}
