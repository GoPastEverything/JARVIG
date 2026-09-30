//! Hardware-independent renderer budgets. Not a second renderer and not a vendor name.
//!
//! Baseline is the default on every adapter until a measured frame time chooses otherwise.
//! An integrated GPU is not automatically a lower budget. ADR-0046.

/// One quality selection for the whole renderer. The authored scene does not change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderQuality {
    /// The numbers the Lighting Lab was built and checked with.
    #[default]
    Baseline,
    /// Higher capture budget. Same materials, same lights.
    Enhanced,
    /// Highest capture budget this model names. Still not a different renderer.
    High,
}

/// What a quality level is allowed to spend. Placeholders stay zero until that system exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderBudget {
    pub shadow_resolution: u32,
    pub point_shadow_resolution: u32,
    pub shadow_filter_taps: u32,
    pub probe_resolution: u32,
    pub contact_steps: u32,
    /// Not read. Dynamic GI is not started.
    pub gi_updates_per_frame: u32,
    /// Not read. A reflection does not require hardware ray tracing.
    pub reflection_ray_budget: u32,
}

impl RenderQuality {
    pub fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Enhanced => "enhanced",
            Self::High => "high",
        }
    }

    pub fn budget(self) -> RenderBudget {
        let common = RenderBudget {
            // The atlas is still created at the baseline size. Naming a larger map here
            // would hide the floor by spending texels, which this pass does not do.
            shadow_resolution: 1024,
            point_shadow_resolution: 256,
            shadow_filter_taps: 16,
            probe_resolution: 64,
            contact_steps: 6,
            gi_updates_per_frame: 0,
            reflection_ray_budget: 0,
        };
        match self {
            Self::Baseline => common,
            Self::Enhanced => RenderBudget { probe_resolution: 128, ..common },
            Self::High => RenderBudget { probe_resolution: 256, ..common },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_share_the_lighting_model_and_only_scale_capture() {
        let baseline = RenderQuality::Baseline.budget();
        let enhanced = RenderQuality::Enhanced.budget();
        let high = RenderQuality::High.budget();
        assert_eq!(baseline.probe_resolution, 64);
        assert_eq!(enhanced.probe_resolution, 128);
        assert_eq!(high.probe_resolution, 256);
        assert_eq!(baseline.shadow_resolution, enhanced.shadow_resolution);
        assert_eq!(baseline.shadow_filter_taps, 16);
        assert_eq!(baseline.contact_steps, high.contact_steps);
        assert_eq!(baseline.gi_updates_per_frame, 0);
        assert_eq!(high.reflection_ray_budget, 0);
        assert_eq!(RenderQuality::default(), RenderQuality::Baseline);
    }
}
