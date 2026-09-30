//! Motion and runtime gate for the RFC-0001 hierarchy that already passed the still-camera check.
//!
//! The dolly is one continuous distance curve. A frame that drops a leaf, pops backward
//! while the camera moves smoothly, or spends the old per-leaf allocation cost fails here.

#[derive(Clone, Debug)]
pub struct RuntimeFrame {
    pub leg: String,
    pub distance_m: f64,
    pub depth: u32,
    pub leaves: u32,
    pub parents: u32,
    pub triangles: u32,
    pub original: u32,
    pub covered: u32,
    pub select_us: u32,
    pub cut_us: u32,
    pub upload_us: u32,
    pub upload_bytes: u64,
    pub draw_prepare_us: u32,
    pub renderer_cpu_us: u32,
    pub clusters: u32,
}

#[derive(Clone, Debug)]
pub struct HoleSample {
    pub camera: String,
    pub hole_ratio: f32,
    pub triangles: u32,
    pub leaves: u32,
    pub parents: u32,
}

#[derive(Clone, Debug)]
pub struct IntegrateSample {
    pub camera: String,
    pub hierarchy: bool,
    pub frustum: bool,
    pub occlusion: bool,
    pub colors_off: bool,
    pub meshlets: u32,
    pub submitted: u32,
    pub frustum_rejected: u32,
    pub occlusion_rejected: u32,
    pub conservative_visible: u32,
    pub leaves: u32,
    pub parents: u32,
    pub triangles: u32,
    pub select_us: u32,
    pub hole_ratio: f32,
}

pub fn dolly_distances(close: f64, medium: f64, far: f64, steps: usize) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    append_leg(&mut out, "out", close, medium, steps, false);
    append_leg(&mut out, "out", medium, far, steps, true);
    append_leg(&mut out, "back", far, medium, steps, true);
    append_leg(&mut out, "back", medium, close, steps, true);
    out
}

fn append_leg(out: &mut Vec<(String, f64)>, leg: &str, from: f64, to: f64, steps: usize, skip_first: bool) {
    let steps = steps.max(1);
    for index in 0..=steps {
        if skip_first && index == 0 {
            continue;
        }
        let t = index as f64 / steps as f64;
        out.push((leg.to_string(), from + (to - from) * t));
    }
}

pub fn judge_dolly(frames: &[RuntimeFrame], leaf_count: u32) -> Vec<String> {
    let mut reasons = Vec::new();
    if frames.len() < 8 {
        reasons.push(format!("\"dolly recorded {} frames\"", frames.len()));
        return reasons;
    }
    let mut disappeared = 0u32;
    let mut transitions = 0u32;
    let mut away_pops = 0u32;
    let mut toward_pops = 0u32;
    for (index, frame) in frames.iter().enumerate() {
        if frame.covered != leaf_count || frame.triangles == 0 {
            disappeared = disappeared.saturating_add(1);
        }
        let Some(previous) = frames.get(index.wrapping_sub(1)) else { continue };
        if index == 0 {
            continue;
        }
        if frame.leaves != previous.leaves || frame.parents != previous.parents {
            transitions = transitions.saturating_add(1);
        }
        let away = frame.distance_m > previous.distance_m + 0.05;
        let toward = frame.distance_m + 0.05 < previous.distance_m;
        if away && previous.triangles > 0 && frame.triangles > previous.triangles.saturating_mul(5) / 4 {
            away_pops = away_pops.saturating_add(1);
        }
        if toward && frame.triangles > 0 && previous.triangles > frame.triangles.saturating_mul(5) / 4 {
            toward_pops = toward_pops.saturating_add(1);
        }
    }
    if leaf_count < 8 || !frames.iter().any(|frame| frame.leaves == leaf_count) {
        reasons.push("\"dolly close view did not submit every leaf\"".into());
    }
    if disappeared > 0 {
        reasons.push(format!("\"dolly dropped geometry on {disappeared} frames\""));
    }
    if transitions == 0 {
        reasons.push("\"dolly never changed the parent/leaf cut\"".into());
    }
    if away_pops > 4 {
        reasons.push(format!("\"dolly popped finer {away_pops} times while moving away\""));
    }
    if toward_pops > 4 {
        reasons.push(format!("\"dolly popped coarser {toward_pops} times while moving closer\""));
    }
    let mut selects: Vec<u32> = frames.iter().map(|frame| frame.select_us).collect();
    selects.sort_unstable();
    let median = selects[selects.len() / 2];
    let max = selects.iter().copied().max().unwrap_or(0);
    if median > 25_000 {
        reasons.push(format!("\"hierarchy selection median {median} us is above 25000\""));
    }
    if max > 50_000 {
        reasons.push(format!("\"hierarchy selection max {max} us is above 50000\""));
    }
    reasons
}

pub fn judge_integration(samples: &[IntegrateSample]) -> Vec<String> {
    let mut reasons = Vec::new();
    if samples.len() < 3 {
        reasons.push("\"integration did not record close medium and far\"".into());
        return reasons;
    }
    for sample in samples {
        if !sample.hierarchy || !sample.frustum || !sample.occlusion || !sample.colors_off {
            reasons.push(format!("\"{} cull flags hierarchy {} frustum {} occlusion {} colors_off {}\"", sample.camera, sample.hierarchy, sample.frustum, sample.occlusion, sample.colors_off));
        }
        let accounted = sample.submitted.saturating_add(sample.frustum_rejected).saturating_add(sample.occlusion_rejected);
        if sample.meshlets > 0 && accounted != sample.meshlets {
            reasons.push(format!("\"{} counters {}+{}+{} != {}\"", sample.camera, sample.submitted, sample.frustum_rejected, sample.occlusion_rejected, sample.meshlets));
        }
        if sample.triangles == 0 {
            reasons.push(format!("\"{} submitted no triangles with culling on\"", sample.camera));
        }
        if sample.hole_ratio > 0.015 {
            reasons.push(format!("\"{} silhouette holes {:.2}% with culling on\"", sample.camera, sample.hole_ratio * 100.0));
        }
    }
    if samples.iter().any(|sample| sample.camera == "far" && sample.parents == 0) {
        reasons.push("\"far integration selected no parents\"".into());
    }
    if samples.iter().any(|sample| sample.camera == "close" && sample.parents > sample.leaves) {
        reasons.push("\"close integration parents outnumber leaves\"".into());
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(leg: &str, distance: f64, triangles: u32, leaves: u32, parents: u32, select_us: u32) -> RuntimeFrame {
        RuntimeFrame {
            leg: leg.into(),
            distance_m: distance,
            depth: 1,
            leaves,
            parents,
            triangles,
            original: 1000,
            covered: 10,
            select_us,
            cut_us: 100,
            upload_us: 100,
            upload_bytes: 64,
            draw_prepare_us: 50,
            renderer_cpu_us: 1000,
            clusters: leaves + parents,
        }
    }

    #[test]
    fn a_smooth_dolly_passes_and_a_drop_or_a_slow_select_fails() {
        let smooth: Vec<_> = (0..12).map(|index| frame("out", index as f64, 1000 - index * 40, 10 - index.min(9), index, 2_000)).collect();
        assert!(judge_dolly(&smooth, 10).is_empty(), "{:?}", judge_dolly(&smooth, 10));
        let mut dropped = smooth.clone();
        dropped[4].covered = 0;
        assert!(judge_dolly(&dropped, 10).iter().any(|reason| reason.contains("dropped")));
        let mut slow = smooth.clone();
        for sample in &mut slow {
            sample.select_us = 80_000;
        }
        assert!(judge_dolly(&slow, 10).iter().any(|reason| reason.contains("selection")));
    }

    #[test]
    fn integration_requires_independent_counters() {
        let good = IntegrateSample {
            camera: "far".into(),
            hierarchy: true,
            frustum: true,
            occlusion: true,
            colors_off: true,
            meshlets: 10,
            submitted: 6,
            frustum_rejected: 3,
            occlusion_rejected: 1,
            conservative_visible: 4,
            leaves: 1,
            parents: 2,
            triangles: 100,
            select_us: 1000,
            hole_ratio: 0.0,
        };
        let mut samples = vec![good.clone(), good.clone(), good];
        samples[0].camera = "close".into();
        samples[0].parents = 0;
        samples[0].leaves = 10;
        samples[1].camera = "medium".into();
        assert!(judge_integration(&samples).is_empty(), "{:?}", judge_integration(&samples));
        samples[2].submitted = 0;
        assert!(judge_integration(&samples).iter().any(|reason| reason.contains("counters")));
    }
}
