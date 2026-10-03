//! Public detail module.
//!
//! This file is the release-candidate stand-in. It does not implement a private
//! surface rule. `linked_detail(false)` is disabled. `linked_detail(true)` is the
//! flat reference marker from `detail_provider`.

use crate::detail_provider::{public_detail, DisabledDetail, ProceduralMicrogeometry, ReferenceDetail};

pub const PROCEDURAL_MICROGEOMETRY_DEFAULT: bool = false;
pub const DETAIL_ERROR_THRESHOLD_PX: f32 = 1.0;
pub const DETAIL_FEATURE_SIZE_M: f32 = 0.02;
pub const EINSTEIN_DEBUG_UV_SCALE: f32 = 1.0;

/// How imported surfaces receive Einstein detail. Auto is the product default.
/// On is the existing enabled path. Off matches the research flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MicrogeometryMode {
    #[default]
    Auto,
    On,
    Off,
}

/// Structural facts for one surface. Names are not consulted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceDetailFacts {
    pub opaque: bool,
    pub skinned: bool,
    pub ui: bool,
    pub particle: bool,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// Authored parametric solid. Geometric error is zero. Auto still classifies it.
    pub exact: bool,
}

/// Auto allows detail on an opaque, unskinned, non-thin surface. The 1 px error
/// gate inside [`evaluate_detail`] still decides the current frame. On forces
/// that same gate. Off never asks for detail. `exact` does not turn Auto off.
pub fn microgeometry_active(mode: MicrogeometryMode, facts: &SurfaceDetailFacts) -> bool {
    match mode {
        MicrogeometryMode::Off => false,
        MicrogeometryMode::On => true,
        MicrogeometryMode::Auto => facts.opaque && !facts.skinned && !facts.ui && !facts.particle && solid_surface(facts.bounds_min, facts.bounds_max),
    }
}

fn solid_surface(min: [f32; 3], max: [f32; 3]) -> bool {
    let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    if extent.iter().any(|axis| !axis.is_finite() || *axis < 0.0) {
        return false;
    }
    let mut axes = extent;
    axes.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let thin = axes[0];
    let thick = axes[2].max(axes[1]);
    thin >= 0.01 && thick > 0.0 && thin / thick >= 0.02
}

/// How many clusters landed in each reject bucket. The sum is the cluster count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetailReasonCounts {
    pub selected: u32,
    pub below: u32,
    pub occluded: u32,
    pub offscreen: u32,
    pub replaced: u32,
    pub budget: u32,
    pub no_anchor: u32,
    pub incompatible: u32,
    pub base: u32,
    /// Exact parametric solids. Classified, and no patch is built.
    pub exact: u32,
}

impl DetailReasonCounts {
    pub fn total(self) -> u32 {
        self.selected
            .saturating_add(self.below)
            .saturating_add(self.occluded)
            .saturating_add(self.offscreen)
            .saturating_add(self.replaced)
            .saturating_add(self.budget)
            .saturating_add(self.no_anchor)
            .saturating_add(self.incompatible)
            .saturating_add(self.base)
            .saturating_add(self.exact)
    }
}

/// Visible surfaces classified for detail. `miss == 0` is not this total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetailVisibilityAccount {
    /// Clusters the view is drawing (flag 1 or 4).
    pub eligible_visible: u32,
    /// Selected for generation. A selected cluster is not yet proof the patch is published.
    pub detail_requested: u32,
    pub below_threshold: u32,
    /// Visible and over the patch budget.
    pub budget_deferred: u32,
    /// Visible, and the surface cannot take detail.
    pub unsupported: u32,
    /// A drawn cluster that landed in none of the four buckets. Acceptance is zero.
    pub unclassified: u32,
}

impl DetailVisibilityAccount {
    pub fn classified(self) -> u32 {
        self.detail_requested
            .saturating_add(self.below_threshold)
            .saturating_add(self.budget_deferred)
            .saturating_add(self.unsupported)
    }

    /// `eligible_visible = requested + below + deferred + unsupported`.
    pub fn balanced(self) -> bool {
        self.unclassified == 0 && self.eligible_visible == self.classified()
    }
}

/// Count drawn clusters only. Off-screen, occluded, and parent-replaced clusters are outside this total.
pub fn account_visible_detail(clusters: &[DetailCluster], reasons: &[DetailReject]) -> DetailVisibilityAccount {
    let mut account = DetailVisibilityAccount::default();
    let paired = clusters.len().min(reasons.len());
    for index in 0..paired {
        if clusters[index].flag != 1 && clusters[index].flag != 4 {
            continue;
        }
        account.eligible_visible = account.eligible_visible.saturating_add(1);
        match reasons[index] {
            DetailReject::Selected => account.detail_requested = account.detail_requested.saturating_add(1),
            DetailReject::BelowThreshold | DetailReject::Exact => account.below_threshold = account.below_threshold.saturating_add(1),
            DetailReject::Budget => account.budget_deferred = account.budget_deferred.saturating_add(1),
            DetailReject::Incompatible | DetailReject::NoAnchor => account.unsupported = account.unsupported.saturating_add(1),
            _ => account.unclassified = account.unclassified.saturating_add(1),
        }
    }
    for cluster in clusters.iter().skip(paired) {
        if cluster.flag == 1 || cluster.flag == 4 {
            account.eligible_visible = account.eligible_visible.saturating_add(1);
            account.unclassified = account.unclassified.saturating_add(1);
        }
    }
    account
}

/// Count every cluster. `miss == 0` only covers `selected`. The other buckets are the legitimate reasons.
pub fn detail_reason_counts(reasons: &[DetailReject]) -> DetailReasonCounts {
    let mut counts = DetailReasonCounts::default();
    for reason in reasons {
        match reason {
            DetailReject::Selected => counts.selected = counts.selected.saturating_add(1),
            DetailReject::BelowThreshold => counts.below = counts.below.saturating_add(1),
            DetailReject::Occluded => counts.occluded = counts.occluded.saturating_add(1),
            DetailReject::OffScreen => counts.offscreen = counts.offscreen.saturating_add(1),
            DetailReject::Replaced => counts.replaced = counts.replaced.saturating_add(1),
            DetailReject::Base => counts.base = counts.base.saturating_add(1),
            DetailReject::Budget => counts.budget = counts.budget.saturating_add(1),
            DetailReject::NoAnchor => counts.no_anchor = counts.no_anchor.saturating_add(1),
            DetailReject::Incompatible => counts.incompatible = counts.incompatible.saturating_add(1),
            DetailReject::Exact => counts.exact = counts.exact.saturating_add(1),
        }
    }
    counts
}

/// Why one cluster did not become an Einstein patch. `Selected` is the set that must be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailReject {
    Selected,
    OffScreen,
    Occluded,
    BelowThreshold,
    Incompatible,
    Budget,
    Replaced,
    NoAnchor,
    /// Authored parametric surface. The cluster is classified and no patch is built.
    Exact,
    Base,
}

/// One cluster after the per-view cut. `flag` uses the meshlet visibility codes.
#[derive(Clone, Copy, Debug)]
pub struct DetailCluster {
    pub flag: u32,
    pub projected_px: f32,
    pub compatible: bool,
    pub has_anchor: bool,
    /// Exact parametric solid. Checked after visibility and before the 1 px gate.
    pub exact: bool,
}

/// Pick detail anchors from clusters the view is actually drawing.
/// Hidden and off-screen clusters are not candidates, so they cannot consume the budget.
/// The closest eligible clusters are kept. The rest of the eligible set is over budget.
pub fn select_detail_clusters(clusters: &[DetailCluster], max_patches: u32) -> Vec<DetailReject> {
    let mut reasons = vec![DetailReject::OffScreen; clusters.len()];
    let mut eligible = Vec::new();
    for (index, cluster) in clusters.iter().enumerate() {
        reasons[index] = if cluster.flag == 3 {
            DetailReject::Occluded
        } else if cluster.flag == 6 {
            DetailReject::Replaced
        } else if cluster.flag != 1 && cluster.flag != 4 {
            DetailReject::OffScreen
        } else if !cluster.compatible {
            DetailReject::Incompatible
        } else if !cluster.has_anchor {
            DetailReject::NoAnchor
        } else if cluster.exact {
            DetailReject::Exact
        } else if !cluster.projected_px.is_finite() || cluster.projected_px <= DETAIL_ERROR_THRESHOLD_PX {
            DetailReject::BelowThreshold
        } else {
            eligible.push((index as u32, cluster.projected_px));
            DetailReject::Budget
        };
    }
    eligible.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(std::cmp::Ordering::Equal).then(left.0.cmp(&right.0)));
    for (index, _) in eligible.into_iter().take(max_patches as usize) {
        reasons[index as usize] = DetailReject::Selected;
    }
    reasons
}

/// Desired, in-flight, and published keys for one view. Zero means nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MicroLifecycle {
    pub desired: u64,
    pub inflight: u64,
    pub published: u64,
}

/// What the host should do after a view change or a finished build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MicroLifecycleStep {
    pub queue: bool,
    pub discard_stale: bool,
    pub publish: bool,
}

/// Remember the key this view wants. A build that is already running is left to finish.
pub fn micro_note_desired(state: &mut MicroLifecycle, desired: u64) -> MicroLifecycleStep {
    state.desired = desired;
    if desired == 0 || state.published == desired || state.inflight != 0 {
        return MicroLifecycleStep::default();
    }
    state.inflight = desired;
    MicroLifecycleStep { queue: true, discard_stale: false, publish: false }
}

/// A build finished. A key that is no longer desired is discarded, then the current desired build is queued.
pub fn micro_note_finished(state: &mut MicroLifecycle, key: u64) -> MicroLifecycleStep {
    if state.inflight == key {
        state.inflight = 0;
    }
    if key != 0 && key == state.desired {
        state.published = key;
        return MicroLifecycleStep { queue: false, discard_stale: false, publish: true };
    }
    let mut step = MicroLifecycleStep { queue: false, discard_stale: true, publish: false };
    if state.inflight == 0 && state.desired != 0 && state.published != state.desired {
        state.inflight = state.desired;
        step.queue = true;
    }
    step
}

/// The worker failed or was dropped. The desired key is queued again when nothing else is running.
pub fn micro_note_failed(state: &mut MicroLifecycle, key: u64) -> MicroLifecycleStep {
    if state.inflight == key {
        state.inflight = 0;
    }
    if state.inflight == 0 && state.desired != 0 && state.published != state.desired {
        state.inflight = state.desired;
        return MicroLifecycleStep { queue: true, discard_stale: false, publish: false };
    }
    MicroLifecycleStep::default()
}

/// Which microgeometry debug picture is on. Off leaves the shaded surface alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MicroDebugMode {
    #[default]
    Off,
    Active,
    Eligible,
    Error,
    State,
    Reject,
}

/// Where the asynchronous build is. Base is the ordinary surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicroDebugStage {
    Ready,
    Queued,
    Building,
    Uploading,
    Stale,
    Base,
}

/// A color for one cluster in a debug mode. `None` means the overlay does not draw it.
pub fn micro_debug_color(mode: MicroDebugMode, reason: DetailReject, projected_px: f32, stage: MicroDebugStage) -> Option<[f32; 3]> {
    if mode == MicroDebugMode::Off || mode == MicroDebugMode::Active || reason == DetailReject::OffScreen {
        return None;
    }
    let base = [0.22, 0.24, 0.28];
    let below = [0.25, 0.45, 0.95];
    let incompatible = [0.62, 0.28, 0.82];
    let budget = [0.92, 0.18, 0.16];
    let occluded = [0.12, 0.28, 0.55];
    let selected = [0.20, 0.90, 0.35];
    let exact = [0.45, 0.70, 0.62];
    match mode {
        MicroDebugMode::Off | MicroDebugMode::Active => None,
        MicroDebugMode::Eligible => Some(match reason {
            DetailReject::Selected => selected,
            DetailReject::Budget => [0.95, 0.55, 0.12],
            DetailReject::BelowThreshold => below,
            DetailReject::Incompatible => incompatible,
            DetailReject::Occluded => occluded,
            DetailReject::Replaced | DetailReject::Base => base,
            DetailReject::Exact => exact,
            DetailReject::NoAnchor => [0.55, 0.36, 0.18],
            DetailReject::OffScreen => return None,
        }),
        MicroDebugMode::Error => Some(match reason {
            DetailReject::Selected | DetailReject::Budget => error_heat(projected_px),
            DetailReject::BelowThreshold | DetailReject::Replaced | DetailReject::Base | DetailReject::Exact => base,
            DetailReject::Incompatible => incompatible,
            DetailReject::Occluded => occluded,
            DetailReject::NoAnchor => [0.55, 0.36, 0.18],
            DetailReject::OffScreen => return None,
        }),
        MicroDebugMode::State => Some(detail_state_color(detail_lifecycle_from_debug(reason, stage))),
        MicroDebugMode::Reject => Some(match reason {
            DetailReject::Selected => selected,
            DetailReject::BelowThreshold => below,
            DetailReject::Incompatible => incompatible,
            DetailReject::Occluded => occluded,
            DetailReject::Budget => budget,
            DetailReject::Replaced | DetailReject::Base => base,
            DetailReject::Exact => exact,
            DetailReject::NoAnchor => [0.55, 0.36, 0.18],
            DetailReject::OffScreen => return None,
        }),
    }
}

fn error_heat(projected_px: f32) -> [f32; 3] {
    let t = ((projected_px - DETAIL_ERROR_THRESHOLD_PX) / 4.0).clamp(0.0, 1.0);
    // Eight bands. A continuous ramp would be a new color per cluster.
    let t = (t * 7.0).round() / 7.0;
    [0.15 + 0.80 * t, 0.35, 0.95 - 0.75 * t]
}

/// Where one visible cluster is in the detail pipeline.
/// A cluster that does not require detail is `NotEligible`. The base mesh stays either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailLifecycle {
    NotEligible,
    EligibleNotRequested,
    Requested,
    Generating,
    ReadyCpu,
    Uploading,
    ReadyGpu,
    Published,
    Drawn,
    Stale,
    Rejected,
    Error,
}

/// One phase of generation, upload, publication, or drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DetailPhase {
    #[default]
    Idle,
    Active,
    Done,
    Failed,
}

/// Identity of one candidate. The hashes use these fields, not the camera floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailCandidateId {
    pub mesh_id: u64,
    pub meshlet_id: u32,
    pub cluster_id: u32,
    pub triangle_first: u32,
    pub triangle_count: u32,
    /// Projected size of the 2 cm feature, in pixels.
    pub screen_error: f32,
    /// Same feature, in pixels. The cluster's exact pixel footprint is not measured.
    pub projected_area: f32,
}

/// One cluster after classification. The draw packets are not in this record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailCandidate {
    pub mesh_id: u64,
    pub meshlet_id: u32,
    pub cluster_id: u32,
    pub triangle_first: u32,
    pub triangle_count: u32,
    pub screen_error: f32,
    pub projected_area: f32,
    pub visible: bool,
    pub eligible: bool,
    pub requested: bool,
    pub generation: DetailPhase,
    pub upload: DetailPhase,
    pub publication: DetailPhase,
    pub draw: DetailPhase,
    pub reject: DetailReject,
    pub state: DetailLifecycle,
}

/// Whole-set progress. Publication is one mesh, so every selected cluster shares this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DetailPipe {
    pub queued: bool,
    pub building: bool,
    pub cpu_ready: bool,
    pub uploading: bool,
    pub gpu_ready: bool,
    pub published: bool,
    pub drawn: bool,
    pub stale: bool,
    pub failed: bool,
}

impl DetailPipe {
    pub fn lifecycle(self) -> DetailLifecycle {
        if self.failed {
            DetailLifecycle::Error
        } else if self.drawn {
            DetailLifecycle::Drawn
        } else if self.published {
            DetailLifecycle::Published
        } else if self.gpu_ready {
            DetailLifecycle::ReadyGpu
        } else if self.uploading {
            DetailLifecycle::Uploading
        } else if self.cpu_ready {
            DetailLifecycle::ReadyCpu
        } else if self.building {
            DetailLifecycle::Generating
        } else if self.stale {
            DetailLifecycle::Stale
        } else if self.queued {
            DetailLifecycle::Requested
        } else {
            DetailLifecycle::EligibleNotRequested
        }
    }
}

/// Counts for the visible eligible set. `miss == 0` is not this total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetailCoverageAccount {
    pub visible_eligible: u32,
    pub requested: u32,
    pub generating: u32,
    pub ready: u32,
    pub published: u32,
    pub drawn: u32,
    pub rejected: u32,
    pub pending: u32,
    pub unclassified: u32,
}

impl DetailCoverageAccount {
    /// `visible_eligible = drawn + pending + rejected`, and `unclassified == 0`.
    pub fn balanced(self) -> bool {
        self.unclassified == 0 && self.visible_eligible == self.drawn.saturating_add(self.pending).saturating_add(self.rejected)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetailSetHashes {
    pub requested: u64,
    pub ready: u64,
    pub published: u64,
    pub drawn: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetailCoverage {
    pub candidates: Vec<DetailCandidate>,
    pub account: DetailCoverageAccount,
    pub hashes: DetailSetHashes,
}

/// A drawn cluster that clears the 1 px gate on a compatible anchored surface.
/// An exact parametric solid is classified and does not require a patch.
pub fn cluster_requires_detail(cluster: &DetailCluster) -> bool {
    !cluster.exact
        && (cluster.flag == 1 || cluster.flag == 4)
        && cluster.compatible
        && cluster.has_anchor
        && cluster.projected_px.is_finite()
        && cluster.projected_px > DETAIL_ERROR_THRESHOLD_PX
}

fn detail_lifecycle_from_debug(reason: DetailReject, stage: MicroDebugStage) -> DetailLifecycle {
    match reason {
        DetailReject::Budget => DetailLifecycle::Rejected,
        DetailReject::Selected => match stage {
            MicroDebugStage::Queued => DetailLifecycle::Requested,
            MicroDebugStage::Building => DetailLifecycle::Generating,
            MicroDebugStage::Uploading => DetailLifecycle::Uploading,
            MicroDebugStage::Stale => DetailLifecycle::Stale,
            MicroDebugStage::Ready => DetailLifecycle::Drawn,
            MicroDebugStage::Base => DetailLifecycle::EligibleNotRequested,
        },
        _ => DetailLifecycle::NotEligible,
    }
}

/// Color for Einstein State. Other visualizations keep their own colors.
pub fn detail_state_color(state: DetailLifecycle) -> [f32; 3] {
    match state {
        DetailLifecycle::NotEligible => [0.35, 0.45, 0.62],
        DetailLifecycle::EligibleNotRequested => [0.95, 0.85, 0.20],
        DetailLifecycle::Requested => [0.95, 0.70, 0.15],
        DetailLifecycle::Generating => [0.95, 0.50, 0.12],
        DetailLifecycle::ReadyCpu => [0.65, 0.90, 0.25],
        DetailLifecycle::Uploading => [0.20, 0.80, 0.90],
        DetailLifecycle::ReadyGpu => [0.15, 0.75, 0.65],
        DetailLifecycle::Published => [0.20, 0.85, 0.55],
        DetailLifecycle::Drawn => [0.20, 0.90, 0.35],
        DetailLifecycle::Stale => [0.90, 0.25, 0.75],
        DetailLifecycle::Rejected => [0.92, 0.18, 0.16],
        DetailLifecycle::Error => [0.85, 0.10, 0.40],
    }
}

fn phases_for(state: DetailLifecycle) -> (DetailPhase, DetailPhase, DetailPhase, DetailPhase) {
    use DetailLifecycle::*;
    use DetailPhase::*;
    match state {
        NotEligible | EligibleNotRequested | Requested | Rejected => (Idle, Idle, Idle, Idle),
        Generating => (Active, Idle, Idle, Idle),
        ReadyCpu => (Done, Idle, Idle, Idle),
        Uploading => (Done, Active, Idle, Idle),
        ReadyGpu => (Done, Done, Idle, Idle),
        Published => (Done, Done, Done, Idle),
        Drawn => (Done, Done, Done, Done),
        Stale => (Idle, Idle, Active, Idle),
        Error => (Failed, Idle, Idle, Idle),
    }
}

fn requested_state(state: DetailLifecycle) -> bool {
    matches!(
        state,
        DetailLifecycle::Requested
            | DetailLifecycle::Generating
            | DetailLifecycle::ReadyCpu
            | DetailLifecycle::Uploading
            | DetailLifecycle::ReadyGpu
            | DetailLifecycle::Published
            | DetailLifecycle::Drawn
            | DetailLifecycle::Stale
    )
}

fn note_coverage(account: &mut DetailCoverageAccount, state: DetailLifecycle) {
    match state {
        DetailLifecycle::Drawn => {
            account.drawn = account.drawn.saturating_add(1);
            account.published = account.published.saturating_add(1);
            account.ready = account.ready.saturating_add(1);
            account.requested = account.requested.saturating_add(1);
        }
        DetailLifecycle::Published => {
            account.published = account.published.saturating_add(1);
            account.ready = account.ready.saturating_add(1);
            account.requested = account.requested.saturating_add(1);
        }
        DetailLifecycle::ReadyGpu | DetailLifecycle::ReadyCpu | DetailLifecycle::Uploading => {
            account.ready = account.ready.saturating_add(1);
            account.requested = account.requested.saturating_add(1);
        }
        DetailLifecycle::Generating => {
            account.generating = account.generating.saturating_add(1);
            account.requested = account.requested.saturating_add(1);
        }
        DetailLifecycle::Requested | DetailLifecycle::Stale => {
            account.requested = account.requested.saturating_add(1);
        }
        DetailLifecycle::Rejected | DetailLifecycle::Error => {
            account.rejected = account.rejected.saturating_add(1);
        }
        DetailLifecycle::EligibleNotRequested => {}
        DetailLifecycle::NotEligible => {
            account.unclassified = account.unclassified.saturating_add(1);
        }
    }
}

fn candidate_identity(candidate: &DetailCandidate) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for lane in [
        candidate.mesh_id,
        u64::from(candidate.meshlet_id),
        u64::from(candidate.cluster_id),
        u64::from(candidate.triangle_first),
        u64::from(candidate.triangle_count),
    ] {
        hash ^= lane;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hash_matching(candidates: &[DetailCandidate], keep: impl Fn(&DetailCandidate) -> bool) -> u64 {
    let mut ids: Vec<u64> = candidates.iter().filter(|candidate| keep(candidate)).map(candidate_identity).collect();
    ids.sort_unstable();
    let mut hash = 0xcbf29ce484222325u64;
    for id in &ids {
        hash ^= *id;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^ u64::from(ids.len() as u32)
}

/// Classify every cluster. A visible eligible cluster is drawn, still pending, or rejected with a reason.
pub fn classify_detail_coverage(ids: &[DetailCandidateId], clusters: &[DetailCluster], reasons: &[DetailReject], pipe: DetailPipe) -> DetailCoverage {
    let count = ids.len().min(clusters.len());
    let mut candidates = Vec::with_capacity(count);
    let mut account = DetailCoverageAccount::default();
    for index in 0..count {
        let cluster = clusters[index];
        let reason = reasons.get(index).copied();
        let required = cluster_requires_detail(&cluster);
        let state = if !required {
            DetailLifecycle::NotEligible
        } else if reason.is_none() {
            account.visible_eligible = account.visible_eligible.saturating_add(1);
            account.unclassified = account.unclassified.saturating_add(1);
            DetailLifecycle::Error
        } else {
            account.visible_eligible = account.visible_eligible.saturating_add(1);
            match reason.unwrap_or(DetailReject::Base) {
                DetailReject::Selected => pipe.lifecycle(),
                DetailReject::Budget => DetailLifecycle::Rejected,
                _ => DetailLifecycle::Error,
            }
        };
        if required && reason.is_some() {
            note_coverage(&mut account, state);
        }
        let (generation, upload, publication, draw) = phases_for(state);
        let id = ids[index];
        let visible = cluster.flag == 1 || cluster.flag == 4;
        candidates.push(DetailCandidate {
            mesh_id: id.mesh_id,
            meshlet_id: id.meshlet_id,
            cluster_id: id.cluster_id,
            triangle_first: id.triangle_first,
            triangle_count: id.triangle_count,
            screen_error: id.screen_error,
            projected_area: id.projected_area,
            visible,
            eligible: required,
            requested: requested_state(state),
            generation,
            upload,
            publication,
            draw,
            reject: reason.unwrap_or(DetailReject::Base),
            state,
        });
    }
    for cluster in clusters.iter().skip(count) {
        if cluster_requires_detail(cluster) {
            account.visible_eligible = account.visible_eligible.saturating_add(1);
            account.unclassified = account.unclassified.saturating_add(1);
        }
    }
    account.pending = account
        .visible_eligible
        .saturating_sub(account.drawn)
        .saturating_sub(account.rejected)
        .saturating_sub(account.unclassified);
    let hashes = DetailSetHashes {
        requested: hash_matching(&candidates, |candidate| candidate.requested),
        ready: hash_matching(&candidates, |candidate| {
            matches!(
                candidate.state,
                DetailLifecycle::ReadyCpu | DetailLifecycle::Uploading | DetailLifecycle::ReadyGpu | DetailLifecycle::Published | DetailLifecycle::Drawn
            )
        }),
        published: hash_matching(&candidates, |candidate| matches!(candidate.state, DetailLifecycle::Published | DetailLifecycle::Drawn)),
        drawn: hash_matching(&candidates, |candidate| candidate.state == DetailLifecycle::Drawn),
    };
    DetailCoverage { candidates, account, hashes }
}

/// Detail is added on top of the base triangle count. A missing patch leaves the base count.
pub fn surface_with_detail(base_triangles: u32, detail_triangles: u32, state: DetailLifecycle) -> (u32, u32) {
    let detail = if state == DetailLifecycle::Drawn { detail_triangles } else { 0 };
    (base_triangles, detail)
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailQuery {
    pub enabled: bool,
    pub seed: u64,
    pub uv: [f32; 2],
    pub projected_error_px: f32,
}

impl DetailQuery {
    pub fn new(seed: u64, uv: [f32; 2], projected_error_px: f32) -> Self {
        Self { enabled: PROCEDURAL_MICROGEOMETRY_DEFAULT, seed, uv, projected_error_px }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailRule {
    EinsteinHat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailSample {
    pub rule: DetailRule,
    pub feature: u32,
    pub local: [f32; 2],
    pub offset: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DetailDecision {
    OrdinaryMaps,
    Fine(DetailSample),
}

pub fn evaluate_detail(query: &DetailQuery, sample: impl Fn(u64, [f32; 2]) -> DetailSample) -> DetailDecision {
    if !query.enabled || !query.projected_error_px.is_finite() || query.projected_error_px <= DETAIL_ERROR_THRESHOLD_PX || !query.uv[0].is_finite() || !query.uv[1].is_finite() {
        return DetailDecision::OrdinaryMaps;
    }
    DetailDecision::Fine(sample(query.seed, query.uv))
}

pub fn evaluate_procedural_microgeometry(query: &DetailQuery) -> DetailDecision {
    evaluate_detail(query, zero_sample)
}

pub fn projected_detail_px(feature_size_m: f32, depth_m: f32, viewport_height: f32, tan_half_fov: f32) -> f32 {
    if !feature_size_m.is_finite() || !depth_m.is_finite() || !viewport_height.is_finite() || !tan_half_fov.is_finite() {
        return 0.0;
    }
    let depth = depth_m.max(0.05);
    let tan_half = tan_half_fov.max(0.0001);
    feature_size_m * (viewport_height.max(1.0) * 0.5) / (depth * tan_half)
}

pub fn einstein_debug_color(_sample: &DetailSample) -> [f32; 3] {
    [0.5, 0.5, 0.5]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetailProbe {
    pub samples: u32,
    pub ordinary: u32,
    pub einstein: u32,
    pub generation_us: u32,
    pub fingerprint: u64,
}

pub fn probe_detail(uvs: &[[f32; 2]], seed: u64, projected_error_px: f32, enabled: bool) -> DetailProbe {
    let mut ordinary = 0u32;
    let mut fine = 0u32;
    for uv in uvs {
        match evaluate_procedural_microgeometry(&DetailQuery { enabled, seed, uv: *uv, projected_error_px }) {
            DetailDecision::OrdinaryMaps => ordinary = ordinary.saturating_add(1),
            DetailDecision::Fine(_) => fine = fine.saturating_add(1),
        }
    }
    DetailProbe { samples: uvs.len() as u32, ordinary, einstein: fine, generation_us: 0, fingerprint: seed ^ u64::from(ordinary) ^ u64::from(fine) }
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceAnchor {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub depth_m: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MicroBudget {
    pub max_patches: u32,
    pub max_vertices: u32,
    pub max_triangles: u32,
    pub amplitude_m: f32,
    pub patch_size_m: f32,
}

impl MicroBudget {
    pub const fn standard() -> Self {
        Self { max_patches: 1, max_vertices: 3, max_triangles: 1, amplitude_m: 0.0, patch_size_m: 0.05 }
    }

    pub const fn surface() -> Self {
        Self::standard()
    }
}

pub struct LocalPatch {
    pub positions: [[f32; 3]; 9],
    pub triangles: [[u8; 3]; 8],
    pub triangle_count: u8,
}

pub fn einstein_local_patch(_sample: &DetailSample) -> LocalPatch {
    LocalPatch { positions: [[0.0; 3]; 9], triangles: [[0, 0, 0]; 8], triangle_count: 0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MicroSpan {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Clone, Debug)]
pub struct MicroMesh {
    pub vertices: Vec<u8>,
    pub indices: Vec<u32>,
    pub spans: Vec<MicroSpan>,
    pub patches: u32,
    pub samples: u32,
    pub ordinary: u32,
    pub triangle_count: u32,
    pub vertex_count: u32,
    pub fallbacks: u32,
    pub max_displacement_m: f32,
    pub fingerprint: u64,
    pub generation_us: u32,
    pub invalid: u32,
}

pub fn group_micro_spans(mesh: &MicroMesh, _max_vertex_bytes: usize) -> Vec<MicroSpan> {
    if mesh.vertex_count == 0 || mesh.indices.is_empty() {
        return Vec::new();
    }
    vec![MicroSpan { vertex_start: 0, vertex_count: mesh.vertex_count, index_start: 0, index_count: mesh.indices.len() as u32 }]
}

/// Names kept so the current editor source still typechecks. Neither variant selects a private rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailProvider {
    EinsteinHat,
    EinsteinSurface,
}

pub fn surface_level(projected_px: f32) -> u32 {
    u32::from(projected_px > DETAIL_ERROR_THRESHOLD_PX)
}

pub fn build_procedural_microtriangles(
    anchors: &[SurfaceAnchor],
    seed: u64,
    enabled: bool,
    viewport_height: f32,
    tan_half_fov: f32,
    _budget: &MicroBudget,
    provider: DetailProvider,
) -> MicroMesh {
    build_procedural_microtriangles_cancellable(anchors, seed, enabled, viewport_height, tan_half_fov, _budget, provider, &|| false).unwrap_or_else(|_| blank(seed))
}

pub fn build_procedural_microtriangles_cancellable(
    anchors: &[SurfaceAnchor],
    seed: u64,
    enabled: bool,
    viewport_height: f32,
    tan_half_fov: f32,
    _budget: &MicroBudget,
    provider: DetailProvider,
    cancel: &dyn Fn() -> bool,
) -> Result<MicroMesh, String> {
    let detail: Box<dyn ProceduralMicrogeometry> = match provider {
        DetailProvider::EinsteinHat => Box::new(DisabledDetail),
        DetailProvider::EinsteinSurface => Box::new(ReferenceDetail),
    };
    detail.build(anchors, seed, enabled, viewport_height, tan_half_fov, cancel)
}

pub fn build_einstein_microtriangles(anchors: &[SurfaceAnchor], seed: u64, enabled: bool, viewport_height: f32, tan_half_fov: f32, budget: &MicroBudget) -> MicroMesh {
    build_procedural_microtriangles(anchors, seed, enabled, viewport_height, tan_half_fov, budget, DetailProvider::EinsteinHat)
}

pub fn einstein_hat_sample(seed: u64, uv: [f32; 2]) -> DetailSample {
    zero_sample(seed, uv)
}

pub fn linked_detail(surface: bool) -> Box<dyn ProceduralMicrogeometry> {
    public_detail(surface)
}

fn zero_sample(_seed: u64, _uv: [f32; 2]) -> DetailSample {
    DetailSample { rule: DetailRule::EinsteinHat, feature: 0, local: [0.0, 0.0], offset: [0.0, 0.0] }
}

fn blank(seed: u64) -> MicroMesh {
    MicroMesh {
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
        fingerprint: seed,
        generation_us: 0,
        invalid: 0,
    }
}
