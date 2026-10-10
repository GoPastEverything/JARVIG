//! Canonical parametric solids. The saved object is the solid, not a triangle mesh.
//!
//! Meshlets, hierarchy, and Einstein detail are derived views. They do not define
//! the block, its transform, or its collision. ADR-0062.

use crate::{MaterialAssetRef, Quat, Vec3, BOOTSTRAP_ROOT_M};

/// Smallest full extent on one axis. Below this the solid stops being a usable face.
pub const BLOCK_MIN_EXTENT_M: f64 = 0.05;
/// Largest full extent on one axis for this slice.
pub const BLOCK_MAX_EXTENT_M: f64 = 1000.0;
/// Free-fly body kept outside the analytic box. Not a character capsule and not JRV-0090.
pub const FLY_COLLISION_RADIUS_M: f64 = 0.3;
/// Editor ground grid, just under Y = 0 so a block resting on the ground wins reversed-Z.
pub const REFERENCE_GRID_Y_M: f64 = -0.001;
/// Face-drag increment. The Move gizmo does not use this.
pub const BLOCK_DIMENSION_SNAP_M: f64 = 0.05;
/// Translation snap for the Snap command. The Move gizmo does not use this.
pub const BLOCK_POSITION_SNAP_M: f64 = 0.1;
/// One Extrude click pushes the selected face by this distance.
pub const BLOCK_EXTRUDE_STEP_M: f64 = 0.25;
/// Create > Plane width. Thickness is [`BLOCK_MIN_EXTENT_M`] on Y, so the sheet stays a closed solid.
pub const PLANE_WIDTH_M: f64 = 2.0;
/// Create > Plane depth.
pub const PLANE_DEPTH_M: f64 = 2.0;
/// One Inset click adds this distance on the selected face.
pub const BLOCK_INSET_STEP_M: f64 = 0.10;
/// How many modeling edits the editor log keeps. Older entries drop off the front.
/// The persistent intent tape is separate and is not capped by this limit.
pub const BLOCK_HISTORY_LIMIT: usize = 24;

/// Face order matches the derived box: +X, −X, +Y, −Y, +Z, −Z.
pub fn face_name(face: u8) -> &'static str {
    match face {
        0 => "+X",
        1 => "-X",
        2 => "+Y",
        3 => "-Y",
        4 => "+Z",
        5 => "-Z",
        _ => "?",
    }
}

/// Outward unit normal in the block frame. `None` when `face` is not 0..5.
pub fn face_normal(face: u8) -> Option<Vec3> {
    match face {
        0 => Some(Vec3::new(1.0, 0.0, 0.0)),
        1 => Some(Vec3::new(-1.0, 0.0, 0.0)),
        2 => Some(Vec3::new(0.0, 1.0, 0.0)),
        3 => Some(Vec3::new(0.0, -1.0, 0.0)),
        4 => Some(Vec3::new(0.0, 0.0, 1.0)),
        5 => Some(Vec3::new(0.0, 0.0, -1.0)),
        _ => None,
    }
}

/// In-plane axis used by the face handle cross. Same tangents as the derived box.
pub fn face_tangent(face: u8) -> Option<Vec3> {
    match face {
        0 => Some(Vec3::new(0.0, 0.0, 1.0)),
        1 => Some(Vec3::new(0.0, 0.0, -1.0)),
        2 | 3 | 4 => Some(Vec3::new(1.0, 0.0, 0.0)),
        5 => Some(Vec3::new(-1.0, 0.0, 0.0)),
        _ => None,
    }
}

const MINOR_RADIUS_M: f64 = 40.0;
const MAJOR_RADIUS_M: f64 = 180.0;
const FADE_FLOOR: f32 = 0.04;

/// Identity of one recorded operation. This is not a face, edge, or vertex id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticStepId(u64);

impl SemanticStepId {
    pub(crate) fn from_raw(number: u64) -> Option<Self> {
        if number == 0 { None } else { Some(Self(number)) }
    }

    pub fn number(self) -> u64 {
        self.0
    }
}

/// A face, edge, or vertex on the evaluated body. This is not a [`SemanticStepId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConcreteElement {
    Face(u32),
    Edge(u32),
    Vertex(u32),
}

impl ConcreteElement {
    pub fn body_id(self) -> u32 {
        match self {
            Self::Face(id) | Self::Edge(id) | Self::Vertex(id) => id,
        }
    }

    pub fn kind_name(self) -> &'static str {
        match self {
            Self::Face(_) => "face",
            Self::Edge(_) => "edge",
            Self::Vertex(_) => "vertex",
        }
    }
}

/// One operation in the persistent intent tape. The 24-entry editor log does not own this.
///
/// `groups` are the semantic references captured for the operands. A size edit has none.
/// A gap records that the edit changed geometry and was not given a semantic representation.
#[derive(Clone, Debug, PartialEq)]
pub struct IntentEntry {
    pub groups: Option<Vec<Vec<String>>>,
    pub payload: IntentPayload,
}

/// Parameters of one persistent intent operation. Concrete topology is not stored here.
#[derive(Clone, Debug, PartialEq)]
pub enum IntentPayload {
    Size { size_m: [f64; 3] },
    Subdivide { u: u32, v: u32 },
    Extrude { delta_m: [f64; 3] },
    Split,
    Bevel { width_m: f64 },
    /// Both ends of one semantically named edge moved by the same vector.
    MoveEdge { delta_m: [f64; 3] },
    /// One semantically named vertex moved. A vertex with no provenance is a gap instead.
    MoveVertex { delta_m: [f64; 3] },
    /// A new wall on one semantically named edge.
    ExtrudeEdge { delta_m: [f64; 3] },
    /// The whole reconstructed solid reflected through the local origin on this axis.
    Mirror { axis: u8 },
    /// One constructor face of an analytic box. `face` is `0..5`, not a stored topology id.
    PushFace { face: u8, distance_m: f64 },
    /// This edit changed the solid and has no semantic representation.
    Gap { operation: String },
    /// One saved analytic chart. The mesh is not stored. Version 2 of the parametric component owns this word.
    /// It is not a boundary-representation operand and it does not make a tape eligible.
    AnalyticSurface {
        identity: String,
        chart: String,
        domain_u: [f64; 2],
        domain_v: [f64; 2],
        radius_m: f64,
        translation_m: [f64; 3],
    },
    /// One analytic fillet on one semantic edge token. The mesh is not stored.
    /// Version 3 of the parametric component owns this word. Version 4 also accepts it beside a seed.
    /// The token is not a display name.
    Round { radius_m: f64 },
    /// Opens the constructor solid as the authored object. The mesh is not stored.
    /// Version 4 of the parametric component owns this word. It is the first intent entry and it names no element.
    Seed,
}

/// One retained operation, named apart from the body it affected.
///
/// `steps[i]` describes `history[i]` while the tape is aligned. The editor does not replay it.
/// `semantic` is the provenance captured when the operation committed. `None` is an older
/// operation that stored only concrete ids. Load does not invent that field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryStep {
    pub id: SemanticStepId,
    pub concrete: Vec<ConcreteElement>,
    /// One group per concrete element, in the same order. A group lists the identities
    /// attached to that element at commit. An empty group was looked up and had none.
    pub semantic: Option<Vec<Vec<String>>>,
}

/// One logged edit. The mesh is rebuilt from [`BlockRecord`]'s parameters, not by replaying this list.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockOp {
    Size { size_m: [f64; 3] },
    ExtrudeFace { face: u8, distance_m: f64 },
    InsetFace { face: u8, distance_m: f64 },
    Bevel { distance_m: f64 },
    Mirror { axis: u8 },
    /// Both ends of one edge moved by the same vector. The log is not replayed.
    MoveEdge { edge: u32, delta_m: [f64; 3] },
    /// A new edge and the wall that joins it to the old one.
    ExtrudeEdge { edge: u32, delta_m: [f64; 3] },
    /// Midpoint of one edge. The old id stays on the first half.
    SplitEdge { edge: u32 },
    /// One quad replaced by a grid of quads.
    SubdivideFace { face: u32, u: u32, v: u32 },
    /// One vertex moved. Loops that already contain it stay connected.
    MoveVertex { vertex: u32, delta_m: [f64; 3] },
    /// The named faces move by one vector. Each boundary edge gains a wall. The log is not replayed.
    ExtrudeFaces { faces: Vec<u32>, delta_m: [f64; 3] },
    /// Selected edges replaced by strips. `distance_m` is the inset that fit, and `edges` includes a straight run.
    BevelEdges { edges: Vec<u32>, distance_m: f64 },
}

impl BlockOp {
    pub fn summary(&self) -> String {
        match self {
            Self::Size { size_m } => format!("Size {} × {} × {}", meters(size_m[0]), meters(size_m[1]), meters(size_m[2])),
            Self::ExtrudeFace { face, distance_m } => format!("Extrude {} {}", face_name(*face), meters(*distance_m)),
            Self::InsetFace { face, distance_m } => format!("Inset {} {}", face_name(*face), meters(*distance_m)),
            Self::Bevel { distance_m } => format!("Bevel {}", meters(*distance_m)),
            Self::Mirror { axis } => format!("Mirror {}", axis_name(*axis)),
            Self::MoveEdge { edge, delta_m } => format!("Move edge E:{edge} {}", meters(delta_span(*delta_m))),
            Self::ExtrudeEdge { edge, delta_m } => format!("Extrude edge E:{edge} {}", meters(delta_span(*delta_m))),
            Self::SplitEdge { edge } => format!("Split edge E:{edge}"),
            Self::SubdivideFace { face, u, v } => format!("Subdivide face F:{face} {u}×{v}"),
            Self::MoveVertex { vertex, delta_m } => format!("Move vertex V:{vertex} {}", meters(delta_span(*delta_m))),
            Self::ExtrudeFaces { faces, delta_m } => {
                if faces.len() == 1 {
                    format!("Extrude face F:{} {}", faces[0], meters(delta_span(*delta_m)))
                } else {
                    format!("Extrude {} faces {}", faces.len(), meters(delta_span(*delta_m)))
                }
            }
            Self::BevelEdges { edges, distance_m } => {
                if edges.len() == 1 {
                    format!("Bevel E:{} {}", edges[0], meters(*distance_m))
                } else {
                    format!("Bevel {} edges {}", edges.len(), meters(*distance_m))
                }
            }
        }
    }

    pub(crate) fn is_topology(&self) -> bool {
        matches!(
            self,
            Self::MoveEdge { .. }
                | Self::ExtrudeEdge { .. }
                | Self::SplitEdge { .. }
                | Self::SubdivideFace { .. }
                | Self::MoveVertex { .. }
                | Self::ExtrudeFaces { .. }
                | Self::BevelEdges { .. }
        )
    }

    pub(crate) fn finite(&self) -> bool {
        match self {
            Self::Size { size_m } => size_m.iter().all(|axis| axis.is_finite()),
            Self::ExtrudeFace { face, distance_m } | Self::InsetFace { face, distance_m } => *face < 6 && distance_m.is_finite(),
            Self::Bevel { distance_m } => distance_m.is_finite(),
            Self::Mirror { axis } => *axis < 3,
            Self::MoveEdge { edge, delta_m } | Self::ExtrudeEdge { edge, delta_m } => *edge != 0 && delta_m.iter().all(|axis| axis.is_finite()),
            Self::SplitEdge { edge } => *edge != 0,
            Self::SubdivideFace { face, u, v } => *face != 0 && (1..=crate::topology::SUBDIVIDE_MAX).contains(u) && (1..=crate::topology::SUBDIVIDE_MAX).contains(v),
            Self::MoveVertex { vertex, delta_m } => *vertex != 0 && delta_m.iter().all(|axis| axis.is_finite()),
            Self::ExtrudeFaces { faces, delta_m } => {
                !faces.is_empty()
                    && faces.iter().all(|id| *id != 0)
                    && faces.iter().enumerate().all(|(index, id)| !faces[..index].contains(id))
                    && delta_m.iter().all(|axis| axis.is_finite())
                    && delta_m.iter().any(|axis| axis.abs() >= 1.0e-9)
            }
            Self::BevelEdges { edges, distance_m } => {
                !edges.is_empty()
                    && edges.iter().all(|id| *id != 0)
                    && edges.iter().enumerate().all(|(index, id)| !edges[..index].contains(id))
                    && distance_m.is_finite()
                    && *distance_m > 0.0
            }
        }
    }

    /// Stable name of this edit in the persistent intent tape.
    pub(crate) fn intent_word(&self) -> &'static str {
        match self {
            Self::Size { .. } => "size",
            Self::ExtrudeFace { .. } => "extrude-face",
            Self::InsetFace { .. } => "inset",
            Self::Bevel { .. } => "uniform-bevel",
            Self::Mirror { .. } => "mirror",
            Self::MoveEdge { .. } => "move-edge",
            Self::ExtrudeEdge { .. } => "extrude-edge",
            Self::SplitEdge { .. } => "split-edge",
            Self::SubdivideFace { .. } => "subdivide-face",
            Self::MoveVertex { .. } => "move-vertex",
            Self::ExtrudeFaces { .. } => "extrude-faces",
            Self::BevelEdges { .. } => "bevel-edges",
        }
    }

    /// Topology ids this edit named. An analytic face index is a parameter, not one of these ids.
    pub(crate) fn concrete_elements(&self) -> Vec<ConcreteElement> {
        match self {
            Self::MoveEdge { edge, .. } | Self::ExtrudeEdge { edge, .. } | Self::SplitEdge { edge } => vec![ConcreteElement::Edge(*edge)],
            Self::SubdivideFace { face, .. } => vec![ConcreteElement::Face(*face)],
            Self::MoveVertex { vertex, .. } => vec![ConcreteElement::Vertex(*vertex)],
            Self::ExtrudeFaces { faces, .. } => faces.iter().copied().map(ConcreteElement::Face).collect(),
            Self::BevelEdges { edges, .. } => edges.iter().copied().map(ConcreteElement::Edge).collect(),
            Self::Size { .. } | Self::ExtrudeFace { .. } | Self::InsetFace { .. } | Self::Bevel { .. } | Self::Mirror { .. } => Vec::new(),
        }
    }
}

/// Saved block. Full extents are meters. Inset and bevel are optional parameters.
/// The derived mesh is not this record.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockRecord {
    /// Full extents in meters.
    ///
    /// On an eligible record with no stored body this is the replay AABB cache written by
    /// `commit_class_c_intent`. Replay does not read it. A stored body and every ineligible
    /// record keep the accepted meaning of this field.
    pub size_m: [f64; 3],
    /// Size at creation. Later edits do not change it. The level writes it when the log is non-empty
    /// and when the record carries an authored seed.
    ///
    /// A shadow replay starts here. It is not a second authority, and a missing value does not invent one.
    pub seed_size_m: Option<[f64; 3]>,
    /// Meters, one per face, in [`face_name`] order. Zero keeps that face a single quad.
    pub inset_m: [f64; 6],
    /// Uniform chamfer along the outer edges, in meters. Zero keeps the sharp box.
    pub bevel_m: f64,
    pub material: MaterialAssetRef,
    pub history: Vec<BlockOp>,
    /// Aligned with `history` when a tape was recorded. Empty on a log saved before that tape existed.
    pub steps: Vec<GeometryStep>,
    /// Complete construction intent. `history` stays capped at [`BLOCK_HISTORY_LIMIT`] for the editor log.
    /// This tape is never truncated. Empty means the record is legacy concrete-only.
    /// It is not the authority, and it does not store evaluated topology.
    pub intent: Vec<IntentEntry>,
    /// Next [`SemanticStepId`]. Not the body's `next_id`. Stays 1 while `steps` is empty.
    pub next_step: u64,
    /// Present after an edge, vertex, or subdivision edit. Absent on an analytic box.
    /// This is the authoritative solid. `steps` does not replace it.
    pub body: Option<crate::topology::SolidBody>,
    /// Slots after slot 0. Slot 0 stays [`Self::material`]. Empty on a block that uses one slot.
    pub materials: Vec<MaterialAssetRef>,
    /// Face-to-slot assignments. A missing `face` is an orphan: the concrete id is gone and this
    /// slice does not rebind it. Empty provenance means no semantic name was recorded.
    pub face_materials: Vec<FaceMaterialAssignment>,
    /// Named regions of the solid. A member's face id is a cache. Provenance is the identity
    /// when a semantic token was recorded. Empty when the solid has no surface groups.
    pub surface_groups: Vec<SurfaceGroup>,
    /// Next [`SurfaceGroup::id`]. Stays 1 while no group has been created. Not reused after delete.
    pub next_surface_group: u32,
}

impl BlockRecord {
    pub fn standard(size_m: [f64; 3]) -> Result<Self, crate::AuthoringError> {
        let size_m = finite_size(size_m)?;
        Ok(Self {
            size_m,
            seed_size_m: Some(size_m),
            inset_m: [0.0; 6],
            bevel_m: 0.0,
            material: default_block_material(),
            history: Vec::new(),
            steps: Vec::new(),
            intent: Vec::new(),
            next_step: 1,
            body: None,
            materials: Vec::new(),
            face_materials: Vec::new(),
            surface_groups: Vec::new(),
            next_surface_group: 1,
        })
    }

    /// Closed sheet on the same record as a block. Width and depth are full extents.
    ///
    /// Thickness is [`BLOCK_MIN_EXTENT_M`] on Y. The level still stores `ParametricBlock`.
    /// There is no plane mesh and no second shadow evaluator.
    pub fn plane(width_m: f64, depth_m: f64) -> Result<Self, crate::AuthoringError> {
        Self::standard([width_m, BLOCK_MIN_EXTENT_M, depth_m])
    }

    /// A new Block. The seed operation is the object. `standard` is unchanged and stays ineligible.
    ///
    /// `seed_size_m` is the constructor size. The record stores no body. Replay mints the
    /// constructor faces and edges, and the mesh built from that replay is disposable.
    pub fn authored_seed(size_m: [f64; 3]) -> Result<Self, crate::AuthoringError> {
        let mut record = Self::standard(size_m)?;
        record.intent.push(IntentEntry { groups: None, payload: IntentPayload::Seed });
        Ok(record)
    }

    /// The record carries the authored seed operation.
    pub fn has_authored_seed(&self) -> bool {
        self.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::Seed))
    }

    /// A plain block writes the same JSON as before this slice: size and material only.
    pub fn is_plain(&self) -> bool {
        self.body.is_none()
            && self.bevel_m.abs() < 1.0e-12
            && self.inset_m.iter().all(|value| value.abs() < 1.0e-12)
            && self.history.is_empty()
            && self.steps.is_empty()
            && self.intent.is_empty()
    }

    /// Bevel or inset is still an analytic parameter. Edge edits wait until those are cleared.
    pub fn analytic_features(&self) -> bool {
        self.bevel_m.abs() > 1.0e-9 || self.inset_m.iter().any(|value| value.abs() > 1.0e-9)
    }

    /// Body used for picking and the overlay. A canonical box is computed and not written back.
    pub fn display_body(&self) -> Option<crate::topology::SolidBody> {
        if let Some(body) = &self.body {
            return Some(body.clone());
        }
        if self.analytic_features() {
            return None;
        }
        crate::topology::SolidBody::from_box(self.size_m).ok()
    }

    pub fn history_text(&self) -> String {
        if self.history.is_empty() {
            return "Block".into();
        }
        let start = self.history.len().saturating_sub(8);
        self.history[start..].iter().map(BlockOp::summary).collect::<Vec<_>>().join(" · ")
    }

    pub fn validate(&self) -> Result<(), crate::LevelError> {
        finite_size(self.size_m).map_err(|_| crate::LevelError::Corrupt("block size is not finite".into()))?;
        for axis in self.size_m {
            if axis < BLOCK_MIN_EXTENT_M - 1.0e-9 || axis > BLOCK_MAX_EXTENT_M + 1.0e-9 {
                return Err(crate::LevelError::Corrupt("block size is outside 0.05 m to 1000 m".into()));
            }
        }
        if !self.bevel_m.is_finite() || self.inset_m.iter().any(|value| !value.is_finite()) {
            return Err(crate::LevelError::Corrupt("block feature is not finite".into()));
        }
        if let Some(seed) = self.seed_size_m {
            if seed.iter().any(|axis| !axis.is_finite()) {
                return Err(crate::LevelError::Corrupt("block creation size is not finite".into()));
            }
            for axis in seed {
                if axis < BLOCK_MIN_EXTENT_M - 1.0e-9 || axis > BLOCK_MAX_EXTENT_M + 1.0e-9 {
                    return Err(crate::LevelError::Corrupt("block creation size is outside 0.05 m to 1000 m".into()));
                }
            }
        }
        let limit = feature_limit(self.size_m);
        if self.bevel_m < -1.0e-9 || self.bevel_m > limit + 1.0e-6 {
            return Err(crate::LevelError::Corrupt("block bevel is outside the solid".into()));
        }
        for inset in self.inset_m {
            if inset < -1.0e-9 || inset > limit + 1.0e-6 {
                return Err(crate::LevelError::Corrupt("block inset is outside the solid".into()));
            }
        }
        if self.history.iter().any(|op| !op.finite()) {
            return Err(crate::LevelError::Corrupt("block history is not finite".into()));
        }
        self.validate_steps()?;
        self.validate_intent()?;
        if let Some(body) = &self.body {
            body.validate().map_err(|_| crate::LevelError::Corrupt("block body is not a closed solid".into()))?;
            let size = body.aabb_size();
            if (0..3).any(|axis| (size[axis] - self.size_m[axis]).abs() > 1.0e-3) {
                return Err(crate::LevelError::Corrupt("block size does not match its body".into()));
            }
        }
        self.material.validate()?;
        self.validate_face_materials()?;
        self.validate_surface_groups()
    }

    pub(crate) fn clamp_features(&mut self) {
        let limit = feature_limit(self.size_m);
        self.bevel_m = if self.bevel_m.is_finite() { self.bevel_m.clamp(0.0, limit) } else { 0.0 };
        for inset in &mut self.inset_m {
            *inset = if inset.is_finite() { inset.clamp(0.0, limit) } else { 0.0 };
        }
    }

    pub(crate) fn push_op(&mut self, op: BlockOp) {
        self.push_op_with_semantics(op, None);
    }

    /// `semantic` is the provenance captured for this commit. `None` leaves the step concrete-only.
    pub(crate) fn push_op_with_semantics(&mut self, op: BlockOp, semantic: Option<Vec<Vec<String>>>) {
        let hole = semantic.as_ref().is_some_and(|groups| groups.iter().any(|group| group.is_empty()));
        let semantic = if hole { None } else { semantic };
        self.note_intent(&op, semantic.clone(), hole);
        // A log saved before the tape existed stays unrecorded. A partial suffix is not a replay source.
        let aligned = self.steps.len() == self.history.len();
        if aligned {
            if self.next_step == 0 {
                self.next_step = 1;
            }
            let id = SemanticStepId(self.next_step);
            self.next_step = self.next_step.saturating_add(1);
            self.steps.push(GeometryStep { id, concrete: op.concrete_elements(), semantic });
        }
        self.history.push(op);
        if self.history.len() > BLOCK_HISTORY_LIMIT {
            let extra = self.history.len() - BLOCK_HISTORY_LIMIT;
            self.history.drain(0..extra);
            if aligned {
                self.steps.drain(0..extra);
            }
        }
    }

    fn validate_steps(&self) -> Result<(), crate::LevelError> {
        if self.next_step == 0 || self.next_step > i64::MAX as u64 {
            return Err(crate::LevelError::Corrupt("block step counter is outside the file".into()));
        }
        if self.steps.is_empty() {
            return Ok(());
        }
        if self.steps.len() != self.history.len() {
            return Err(crate::LevelError::Corrupt("block steps do not match the log".into()));
        }
        let mut seen = Vec::new();
        for (index, step) in self.steps.iter().enumerate() {
            let number = step.id.number();
            if number == 0 || number >= self.next_step || number > i64::MAX as u64 || seen.contains(&number) {
                return Err(crate::LevelError::Corrupt("block step id is zero, repeated, or ahead of the counter".into()));
            }
            seen.push(number);
            if step.concrete != self.history[index].concrete_elements() {
                return Err(crate::LevelError::Corrupt("block step names the wrong elements".into()));
            }
            if let Some(groups) = &step.semantic {
                if groups.len() != step.concrete.len() {
                    return Err(crate::LevelError::Corrupt("block step semantic groups do not match its elements".into()));
                }
                for group in groups {
                    for token in group {
                        if token.is_empty() || token.chars().any(char::is_whitespace) {
                            return Err(crate::LevelError::Corrupt("block step semantic reference is empty".into()));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_intent(&self) -> Result<(), crate::LevelError> {
        for entry in &self.intent {
            if let Some(groups) = &entry.groups {
                for group in groups {
                    for token in group {
                        if token.is_empty() || token.chars().any(char::is_whitespace) {
                            return Err(crate::LevelError::Corrupt("block intent reference is empty".into()));
                        }
                    }
                }
            }
            match &entry.payload {
                IntentPayload::Size { size_m }
                | IntentPayload::Extrude { delta_m: size_m }
                | IntentPayload::MoveEdge { delta_m: size_m }
                | IntentPayload::MoveVertex { delta_m: size_m }
                | IntentPayload::ExtrudeEdge { delta_m: size_m } => {
                    if size_m.iter().any(|axis| !axis.is_finite()) {
                        return Err(crate::LevelError::Corrupt("block intent value is not finite".into()));
                    }
                }
                IntentPayload::Bevel { width_m } => {
                    if !width_m.is_finite() {
                        return Err(crate::LevelError::Corrupt("block intent value is not finite".into()));
                    }
                }
                IntentPayload::PushFace { face, distance_m } => {
                    if *face > 5 || !distance_m.is_finite() {
                        return Err(crate::LevelError::Corrupt("block intent face push is not usable".into()));
                    }
                }
                IntentPayload::Mirror { axis } => {
                    if *axis > 2 {
                        return Err(crate::LevelError::Corrupt("block intent mirror axis is not 0, 1, or 2".into()));
                    }
                }
                IntentPayload::Gap { operation } => {
                    if operation.is_empty() {
                        return Err(crate::LevelError::Corrupt("block intent gap names no operation".into()));
                    }
                }
                IntentPayload::AnalyticSurface { identity, chart, domain_u, domain_v, radius_m, translation_m } => {
                    if identity.is_empty() || identity.chars().any(char::is_whitespace) {
                        return Err(crate::LevelError::Corrupt("block intent analytic surface has no identity".into()));
                    }
                    if chart != "p(u,v)=(u,v,1)/sqrt(u*u+v*v+1)" {
                        return Err(crate::LevelError::Corrupt("block intent analytic surface chart is not the persisted spelling".into()));
                    }
                    if !radius_m.is_finite() || *radius_m <= 0.0 {
                        return Err(crate::LevelError::Corrupt("block intent analytic surface radius is not a positive finite length".into()));
                    }
                    if !ordered_domain(domain_u) || !ordered_domain(domain_v) || translation_m.iter().any(|axis| !axis.is_finite()) {
                        return Err(crate::LevelError::Corrupt("block intent analytic surface domain or translation is not finite".into()));
                    }
                }
                IntentPayload::Round { radius_m } => {
                    if !radius_m.is_finite() || *radius_m <= 0.0 {
                        return Err(crate::LevelError::Corrupt("block intent round radius is not a positive finite length".into()));
                    }
                    if crate::round_intent::round_group_tokens(entry.groups.as_deref()).is_err() {
                        return Err(crate::LevelError::Corrupt("block intent round does not name a semantic edge".into()));
                    }
                }
                IntentPayload::Seed => {
                    if entry.groups.is_some() {
                        return Err(crate::LevelError::Corrupt("block intent seed does not name an element".into()));
                    }
                }
                IntentPayload::Subdivide { .. } | IntentPayload::Split => {}
            }
        }
        if self.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::Round { .. }))
            && self.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::AnalyticSurface { .. }))
        {
            return Err(crate::LevelError::Corrupt("block intent cannot store round and analytic-surface together".into()));
        }
        let seeds: Vec<usize> = self
            .intent
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry.payload, IntentPayload::Seed))
            .map(|(index, _)| index)
            .collect();
        if seeds.len() > 1 || seeds.first().is_some_and(|index| *index != 0) {
            return Err(crate::LevelError::Corrupt("block intent seed is not the first operation".into()));
        }
        if !seeds.is_empty() && self.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::AnalyticSurface { .. })) {
            return Err(crate::LevelError::Corrupt("block intent cannot store seed and analytic-surface together".into()));
        }
        Ok(())
    }

    /// Appends to the persistent tape. The editor log still drops entries past [`BLOCK_HISTORY_LIMIT`].
    fn note_intent(&mut self, op: &BlockOp, semantic: Option<Vec<Vec<String>>>, hole: bool) {
        let tracking = !self.intent.is_empty();
        if !tracking {
            // A size edit stays on the capped log until a later edit opens the tape.
            // Any other geometry edit opens it, including one that can only be recorded as a gap.
            if semantic.is_none() && !hole && matches!(op, BlockOp::Size { .. }) {
                return;
            }
            if !self.history_can_open_intent() {
                return;
            }
            self.backfill_intent();
        } else if self.intent_has_gap() && !matches!(op, BlockOp::Size { .. }) {
            self.intent.push(IntentEntry { groups: None, payload: IntentPayload::Gap { operation: op.intent_word().to_string() } });
            return;
        }
        self.intent.push(self.intent_entry(op, semantic, hole));
    }

    fn history_can_open_intent(&self) -> bool {
        self.steps.len() == self.history.len()
            && self.seed_size_m.is_some()
            && self.history.iter().all(|op| matches!(op, BlockOp::Size { .. }))
            && self.steps.iter().all(|step| step.semantic.is_none())
    }

    fn backfill_intent(&mut self) {
        let sizes: Vec<[f64; 3]> = self.history.iter().filter_map(|op| match op { BlockOp::Size { size_m } => Some(*size_m), _ => None }).collect();
        for size_m in sizes {
            self.intent.push(IntentEntry { groups: None, payload: IntentPayload::Size { size_m } });
        }
    }

    fn intent_has_gap(&self) -> bool {
        self.intent.iter().any(|entry| matches!(entry.payload, IntentPayload::Gap { .. }))
    }

    fn intent_mentions_topology(&self) -> bool {
        self.intent.iter().any(|entry| {
            matches!(
                entry.payload,
                IntentPayload::Subdivide { .. }
                    | IntentPayload::Extrude { .. }
                    | IntentPayload::Split
                    | IntentPayload::Bevel { .. }
                    | IntentPayload::MoveEdge { .. }
                    | IntentPayload::MoveVertex { .. }
                    | IntentPayload::ExtrudeEdge { .. }
                    | IntentPayload::Mirror { .. }
            )
        })
    }

    fn intent_entry(&self, op: &BlockOp, semantic: Option<Vec<Vec<String>>>, hole: bool) -> IntentEntry {
        let whole_solid = matches!(op, BlockOp::Size { .. } | BlockOp::ExtrudeFace { .. } | BlockOp::Mirror { .. });
        if hole || (semantic.is_none() && !whole_solid) {
            return IntentEntry { groups: None, payload: IntentPayload::Gap { operation: op.intent_word().to_string() } };
        }
        match op {
            BlockOp::Size { size_m } => IntentEntry { groups: None, payload: IntentPayload::Size { size_m: *size_m } },
            BlockOp::SubdivideFace { u, v, .. } => IntentEntry { groups: semantic, payload: IntentPayload::Subdivide { u: *u, v: *v } },
            BlockOp::ExtrudeFaces { delta_m, .. } => IntentEntry { groups: semantic, payload: IntentPayload::Extrude { delta_m: *delta_m } },
            BlockOp::SplitEdge { .. } => IntentEntry { groups: semantic, payload: IntentPayload::Split },
            BlockOp::BevelEdges { distance_m, .. } => IntentEntry { groups: semantic, payload: IntentPayload::Bevel { width_m: *distance_m } },
            BlockOp::MoveEdge { delta_m, .. } => IntentEntry { groups: semantic, payload: IntentPayload::MoveEdge { delta_m: *delta_m } },
            BlockOp::MoveVertex { delta_m, .. } => IntentEntry { groups: semantic, payload: IntentPayload::MoveVertex { delta_m: *delta_m } },
            BlockOp::ExtrudeEdge { delta_m, .. } => IntentEntry { groups: semantic, payload: IntentPayload::ExtrudeEdge { delta_m: *delta_m } },
            BlockOp::ExtrudeFace { face, distance_m } => {
                // A constructor face is a parameter of the analytic box. After topology exists it is not that box.
                if self.body.is_some() || self.intent_mentions_topology() {
                    IntentEntry { groups: None, payload: IntentPayload::Gap { operation: op.intent_word().to_string() } }
                } else {
                    IntentEntry { groups: None, payload: IntentPayload::PushFace { face: *face, distance_m: *distance_m } }
                }
            }
            BlockOp::Mirror { axis } => {
                // Reflection is the whole solid. An analytic copy only swaps insets and has no body to reflect.
                if self.body.is_some() {
                    IntentEntry { groups: None, payload: IntentPayload::Mirror { axis: *axis } }
                } else {
                    IntentEntry { groups: None, payload: IntentPayload::Gap { operation: op.intent_word().to_string() } }
                }
            }
            BlockOp::InsetFace { .. } | BlockOp::Bevel { .. } => {
                IntentEntry { groups: None, payload: IntentPayload::Gap { operation: op.intent_word().to_string() } }
            }
        }
    }
}

/// Mesh and meshlet clusters rebuilt from one authoritative body.
///
/// Hierarchy nodes, Einstein patches, and GPU buffers are not this value and are not saved.
/// Dropping it leaves the body that produced it.
#[derive(Clone, Debug)]
pub struct DerivedRenderGeometry {
    pub mesh: crate::mesh::Mesh,
    pub meshlets: crate::meshlet::MeshletSet,
}

impl DerivedRenderGeometry {
    pub fn from_body(body: &crate::topology::SolidBody) -> Self {
        let mesh = crate::mesh::mesh_from_body(body);
        let meshlets = crate::meshlet::build_meshlets(&mesh);
        Self { mesh, meshlets }
    }

    /// Forgets the mesh and the clusters. The authoritative body is not stored here.
    pub fn discard(self) {
        drop(self);
    }

    /// Same triangles and the same clusters. Build timing is not part of the surface.
    pub fn same_surface(&self, other: &Self) -> bool {
        if self.mesh.triangle_indices() != other.mesh.triangle_indices() || self.mesh.vertex_count() != other.mesh.vertex_count() {
            return false;
        }
        if self.mesh.index_bytes() != other.mesh.index_bytes() {
            return false;
        }
        for index in 0..self.mesh.vertex_count() {
            if self.mesh.position(index) != other.mesh.position(index) {
                return false;
            }
        }
        self.meshlets.grid_resolution == other.meshlets.grid_resolution
            && self.meshlets.meshlets == other.meshlets.meshlets
            && self.meshlets.vertex_indices == other.meshlets.vertex_indices
            && self.meshlets.local_indices == other.meshlets.local_indices
    }
}

/// Largest bevel or inset that stays inside the box. 45% of the shortest side.
pub fn feature_limit(size_m: [f64; 3]) -> f64 {
    (0.45 * size_m[0].min(size_m[1]).min(size_m[2])).max(0.0)
}

/// Face-drag snap. Values under half a step become zero. This is not the Move gizmo.
pub fn snap_dimension(outward_m: f64) -> f64 {
    if !outward_m.is_finite() {
        return 0.0;
    }
    (outward_m / BLOCK_DIMENSION_SNAP_M).round() * BLOCK_DIMENSION_SNAP_M
}

/// Rounds each component to `step_m`. A non-positive step uses [`BLOCK_POSITION_SNAP_M`].
pub fn snap_translation(translation: Vec3, step_m: f64) -> Vec3 {
    let step = if step_m.is_finite() && step_m > 0.0 { step_m } else { BLOCK_POSITION_SNAP_M };
    Vec3::new(snap_step(translation.x, step), snap_step(translation.y, step), snap_step(translation.z, step))
}

/// Moves one face along its outward normal and shifts the center by half the applied change.
/// The opposite face stays where it was. `rotation` maps the block frame into `translation`'s frame.
pub fn push_face(size_m: [f64; 3], translation: Vec3, rotation: Quat, face: u8, outward_m: f64) -> Option<FacePush> {
    let normal = face_normal(face)?;
    if !outward_m.is_finite() || size_m.iter().any(|axis| !axis.is_finite()) {
        return None;
    }
    let axis = (face / 2) as usize;
    let mut size = size_m;
    let grown = (size[axis] + outward_m).clamp(BLOCK_MIN_EXTENT_M, BLOCK_MAX_EXTENT_M);
    let applied = grown - size[axis];
    size[axis] = grown;
    let delta = rotation.rotate(normal.scale(applied * 0.5));
    Some(FacePush {
        size_m: size,
        translation: Vec3::new(translation.x + delta.x, translation.y + delta.y, translation.z + delta.z),
    })
}

/// The original box face closest to a point in the block frame. Used when a click lands on the solid.
pub fn face_from_local_point(local: Vec3, size: [f64; 3]) -> Option<u8> {
    if size.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) || !local.x.is_finite() || !local.y.is_finite() || !local.z.is_finite() {
        return None;
    }
    let coords = [local.x, local.y, local.z];
    let mut best_face = 0u8;
    let mut best_gap = f64::MAX;
    for face in 0..6u8 {
        let axis = (face / 2) as usize;
        let sign = if face % 2 == 0 { 1.0 } else { -1.0 };
        let plane = sign * size[axis] * 0.5;
        let gap = (coords[axis] - plane).abs();
        if gap < best_gap {
            best_gap = gap;
            best_face = face;
        }
    }
    Some(best_face)
}

/// Center of one face in the same frame as `translation`.
pub fn face_center(translation: Vec3, rotation: Quat, size: [f64; 3], face: u8) -> Option<Vec3> {
    let normal = face_normal(face)?;
    let axis = (face / 2) as usize;
    if !size[axis].is_finite() {
        return None;
    }
    let offset = rotation.rotate(normal.scale(size[axis] * 0.5));
    Some(Vec3::new(translation.x + offset.x, translation.y + offset.y, translation.z + offset.z))
}

/// Closest face handle whose sphere the ray hits. `ray_direction` need not be a unit vector.
pub fn nearest_face_handle(
    translation: Vec3,
    rotation: Quat,
    size: [f64; 3],
    ray_origin: Vec3,
    ray_direction: Vec3,
    radius: f64,
) -> Option<u8> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let length = (ray_direction.x * ray_direction.x + ray_direction.y * ray_direction.y + ray_direction.z * ray_direction.z).sqrt();
    if length < 1.0e-12 {
        return None;
    }
    let direction = ray_direction.scale(1.0 / length);
    let mut best: Option<(f64, u8)> = None;
    for face in 0..6u8 {
        let center = face_center(translation, rotation, size, face)?;
        let offset = Vec3::new(ray_origin.x - center.x, ray_origin.y - center.y, ray_origin.z - center.z);
        let toward = dot_vec(offset, direction);
        let discriminant = toward * toward - (dot_vec(offset, offset) - radius * radius);
        if discriminant < 0.0 {
            continue;
        }
        let root = discriminant.sqrt();
        let near = -toward - root;
        let hit = if near > 1.0e-4 { near } else { -toward + root };
        if hit > 1.0e-4 && best.map(|(distance, _)| hit < distance).unwrap_or(true) {
            best = Some((hit, face));
        }
    }
    best.map(|(_, face)| face)
}

/// Exchanges the two insets on `axis` so a second mirror restores them.
pub fn swap_insets(inset_m: [f64; 6], axis: usize) -> Option<[f64; 6]> {
    if axis > 2 {
        return None;
    }
    let mut swapped = inset_m;
    swapped.swap(axis * 2, axis * 2 + 1);
    Some(swapped)
}

/// Places a copy so it shares the positive face on `axis`. The original stays put.
pub fn mirrored_translation(translation: Vec3, rotation: Quat, size_m: [f64; 3], axis: usize) -> Option<Vec3> {
    if axis > 2 || !size_m[axis].is_finite() {
        return None;
    }
    let mut offset = Vec3::ZERO;
    match axis {
        0 => offset.x = size_m[0],
        1 => offset.y = size_m[1],
        _ => offset.z = size_m[2],
    }
    let delta = rotation.rotate(offset);
    Some(Vec3::new(translation.x + delta.x, translation.y + delta.y, translation.z + delta.z))
}

/// Drops the lowest corner onto scene Y = 0. Rotation is unchanged. Parent rotation is identity.
pub fn align_translation_to_ground(translation: Vec3, rotation: Quat, size_m: [f64; 3]) -> Vec3 {
    let half = [size_m[0] * 0.5, size_m[1] * 0.5, size_m[2] * 0.5];
    let mut min_y = f64::MAX;
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                let corner = rotation.rotate(Vec3::new(x * half[0], y * half[1], z * half[2]));
                min_y = min_y.min(translation.y + corner.y);
            }
        }
    }
    Vec3::new(translation.x, translation.y - min_y, translation.z)
}

/// Size and the local translation after one face push.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacePush {
    pub size_m: [f64; 3],
    pub translation: Vec3,
}

fn meters(value: f64) -> String {
    format!("{value:.2}")
}

fn delta_span(delta: [f64; 3]) -> f64 {
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

fn axis_name(axis: u8) -> &'static str {
    match axis {
        0 => "X",
        1 => "Y",
        2 => "Z",
        _ => "?",
    }
}

fn ordered_domain(domain: &[f64; 2]) -> bool {
    domain.iter().all(|value| value.is_finite()) && domain[0] < domain[1]
}

fn snap_step(value: f64, step: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    (value / step).round() * step
}

fn dot_vec(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

pub fn default_block_material() -> MaterialAssetRef {
    MaterialAssetRef::builtin("standard_white", [0.62, 0.62, 0.60, 1.0], 0.0, 0.75, [0.0, 0.0, 0.0, 1.0])
}

/// Slots on one solid, including slot 0. A further slot is refused and the record stays as it was.
pub const BLOCK_MATERIAL_SLOT_LIMIT: u32 = 16;

/// One face bound to a material slot, or an orphan whose concrete face is gone.
///
/// `provenance` is a copy of semantic tokens that already named the face. It is not a geometric
/// authority, and an empty list means none were recorded. This slice does not rebind an orphan.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceMaterialAssignment {
    pub face: Option<u32>,
    pub slot: u32,
    pub provenance: Vec<String>,
}

/// Why a face-material edit was refused. The record is left unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceMaterialError {
    UnknownFace,
    UnknownSlot,
    SlotLimit,
    InvalidFactor,
}

/// Assignments after a topology edit, plus how many new faces stayed on slot 0 because neighbors disagreed.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedFaceMaterials {
    pub assignments: Vec<FaceMaterialAssignment>,
    pub defaulted: u32,
}

/// How long a surface-group name may be. A longer name is refused and the record stays as it was.
pub const SURFACE_GROUP_NAME_LIMIT: usize = 64;

/// One surface in a named group. `face` is the current binding. `provenance` is the identity
/// when a semantic token was recorded. A missing face is unresolved, not deleted.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceMember {
    pub face: Option<u32>,
    pub provenance: Vec<String>,
}

/// A named region of a solid. The id stays after rename. The slot is a material index.
/// Concrete face ids are not the long-term identity.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceGroup {
    pub id: u32,
    pub name: String,
    pub slot: u32,
    pub members: Vec<SurfaceMember>,
}

/// Why a surface-group edit was refused. The record is left unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceGroupError {
    UnknownFace,
    UnknownSlot,
    UnknownGroup,
    EmptyName,
    DuplicateName,
    NameLimit,
    AlreadyMember,
    EmptySelection,
    MixedSlots,
    UnnamedFace,
    SlotLimit,
}

/// How a face selection sits in the groups. Mixed is two groups, or a grouped face beside one that is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceGroupCover {
    None,
    Covered(u32),
    Mixed,
}

/// Groups after a topology edit. `ambiguous` counts new faces that were not joined because
/// the semantic name was missing or the neighboring groups disagreed.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedSurfaceGroups {
    pub groups: Vec<SurfaceGroup>,
    pub ambiguous: u32,
}

impl BlockRecord {
    pub fn slot_count(&self) -> u32 {
        1 + self.materials.len() as u32
    }

    pub fn material_slot(&self, slot: u32) -> Option<&MaterialAssetRef> {
        if slot == 0 {
            Some(&self.material)
        } else {
            self.materials.get(slot as usize - 1)
        }
    }

    fn material_slot_mut(&mut self, slot: u32) -> Option<&mut MaterialAssetRef> {
        if slot == 0 {
            Some(&mut self.material)
        } else {
            self.materials.get_mut(slot as usize - 1)
        }
    }

    pub(crate) fn topology_history(&self) -> bool {
        self.history.iter().any(|op| op.is_topology())
    }

    /// Body the face map may bind. A canonical box is computed and not written back.
    ///
    /// A record with topology history and no stored body does not lend its face ids to that box.
    pub fn material_body(&self) -> Option<crate::topology::SolidBody> {
        if let Some(body) = &self.body {
            return Some(body.clone());
        }
        if self.topology_history() || self.analytic_features() {
            return None;
        }
        crate::topology::SolidBody::from_box(self.size_m).ok()
    }

    fn face_bindable(&self, face: u32) -> bool {
        let Some(body) = self.material_body() else { return false };
        body.faces.iter().any(|stored| stored.id == face)
    }

    /// Provenance already stored for `face`, or the semantic name recorded for it.
    ///
    /// Empty means no name was recorded. This does not invent a face-id token.
    pub fn recorded_face_names(&self, face: u32) -> Vec<String> {
        self.face_materials
            .iter()
            .find(|entry| entry.face == Some(face) && !entry.provenance.is_empty())
            .map(|entry| entry.provenance.clone())
            .unwrap_or_else(|| crate::semantic_shadow::face_provenance_names(self, face))
    }

    /// Slot painted on a live face. Anything else, including an orphan, is slot 0.
    pub fn bound_slot(&self, face: u32) -> u32 {
        if !self.face_bindable(face) {
            return 0;
        }
        self.face_materials.iter().find(|entry| entry.face == Some(face)).map(|entry| entry.slot).unwrap_or(0)
    }

    /// Live faces on `slot`, in stored face order. Orphans are not included.
    pub fn faces_on_slot(&self, slot: u32) -> Vec<u32> {
        let Some(body) = self.material_body() else { return Vec::new() };
        body.faces.iter().filter(|face| self.bound_slot(face.id) == slot).map(|face| face.id).collect()
    }

    /// A concrete id that is not on the current body becomes an orphan. The row stays.
    pub fn orphan_unbound(&mut self) {
        let live: Vec<u32> = self.material_body().map(|body| body.faces.iter().map(|face| face.id).collect()).unwrap_or_default();
        for entry in &mut self.face_materials {
            if entry.face.is_some_and(|face| !live.contains(&face)) {
                entry.face = None;
            }
        }
        for group in &mut self.surface_groups {
            for member in &mut group.members {
                if member.face.is_some_and(|face| !live.contains(&face)) {
                    member.face = None;
                }
            }
        }
    }

    /// Every assignment loses its concrete face. Extra slots, groups, and provenance stay.
    pub fn orphan_all_assignments(&mut self) {
        for entry in &mut self.face_materials {
            entry.face = None;
        }
        for group in &mut self.surface_groups {
            for member in &mut group.members {
                member.face = None;
            }
        }
    }

    /// Copies slot 0 into the next slot. The builtin name stays `standard_white`.
    pub fn add_material_slot(&mut self) -> Result<u32, FaceMaterialError> {
        self.add_material_slot_from(0)
    }

    /// Copies `source` into the next slot. The builtin name stays `standard_white`.
    pub fn add_material_slot_from(&mut self, source: u32) -> Result<u32, FaceMaterialError> {
        if self.slot_count() >= BLOCK_MATERIAL_SLOT_LIMIT {
            return Err(FaceMaterialError::SlotLimit);
        }
        let material = self.material_slot(source).cloned().ok_or(FaceMaterialError::UnknownSlot)?;
        self.materials.push(material);
        Ok(self.slot_count() - 1)
    }

    /// Copies the slot these faces share into a new slot and paints only those faces.
    ///
    /// Faces that disagree copy slot 0. An unknown id or a 17th slot writes nothing.
    /// An empty list changes nothing. This does not append the intent tape.
    pub fn make_faces_unique(&mut self, face_ids: &[u32], names_of: impl Fn(u32) -> Vec<String>) -> Result<Option<u32>, FaceMaterialError> {
        if face_ids.is_empty() {
            return Ok(None);
        }
        let mut unique = Vec::new();
        for id in face_ids {
            if *id == 0 || !self.face_bindable(*id) {
                return Err(FaceMaterialError::UnknownFace);
            }
            if !unique.contains(id) {
                unique.push(*id);
            }
        }
        let mut seen = Vec::new();
        for face in &unique {
            let slot = self.bound_slot(*face);
            if !seen.contains(&slot) {
                seen.push(slot);
            }
        }
        let source = if seen.len() == 1 { seen[0] } else { 0 };
        let added = self.add_material_slot_from(source)?;
        match self.assign_faces(&unique, added, names_of) {
            Ok(true) => Ok(Some(added)),
            Ok(false) => {
                self.materials.pop();
                Ok(None)
            }
            Err(error) => {
                self.materials.pop();
                Err(error)
            }
        }
    }

    /// Paints `face_ids` with `slot`. Empty input changes nothing. An unknown id writes nothing.
    ///
    /// `names_of` is read only for a face that does not already have provenance. Slot 0 with empty
    /// provenance removes the row so a plain cube stays plain. Slot 0 with provenance keeps the row.
    pub fn assign_faces(&mut self, face_ids: &[u32], slot: u32, names_of: impl Fn(u32) -> Vec<String>) -> Result<bool, FaceMaterialError> {
        if face_ids.is_empty() {
            return Ok(false);
        }
        if slot >= BLOCK_MATERIAL_SLOT_LIMIT {
            return Err(FaceMaterialError::SlotLimit);
        }
        if slot >= self.slot_count() {
            return Err(FaceMaterialError::UnknownSlot);
        }
        let mut unique = Vec::new();
        for id in face_ids {
            if *id == 0 || !self.face_bindable(*id) {
                return Err(FaceMaterialError::UnknownFace);
            }
            if !unique.contains(id) {
                unique.push(*id);
            }
        }
        let before = self.face_materials.clone();
        for face in unique {
            let provenance = self
                .face_materials
                .iter()
                .find(|entry| entry.face == Some(face))
                .map(|entry| entry.provenance.clone())
                .filter(|names| !names.is_empty())
                .unwrap_or_else(|| names_of(face));
            self.face_materials.retain(|entry| entry.face != Some(face));
            if slot == 0 && provenance.is_empty() {
                continue;
            }
            self.face_materials.push(FaceMaterialAssignment { face: Some(face), slot, provenance });
        }
        Ok(self.face_materials != before)
    }

    /// Edits one slot's base color, roughness, and metallic. Topology and face ids stay.
    pub fn set_material_factors(&mut self, slot: u32, base_color: [f32; 3], roughness: f32, metallic: f32) -> Result<bool, FaceMaterialError> {
        if slot >= self.slot_count() {
            return Err(if slot >= BLOCK_MATERIAL_SLOT_LIMIT { FaceMaterialError::SlotLimit } else { FaceMaterialError::UnknownSlot });
        }
        if !factor_color(base_color) || !unit_factor(roughness) || !unit_factor(metallic) {
            return Err(FaceMaterialError::InvalidFactor);
        }
        let material = self.material_slot_mut(slot).ok_or(FaceMaterialError::UnknownSlot)?;
        let next_color = [base_color[0], base_color[1], base_color[2], 1.0];
        if material.base_color == next_color && material.roughness == roughness && material.metallic == metallic {
            return Ok(false);
        }
        material.base_color = next_color;
        material.roughness = roughness;
        material.metallic = metallic;
        Ok(true)
    }

    /// Projects `baseline` across a topology edit. New faces inherit a slot or stay on slot 0.
    ///
    /// A face id that disappeared becomes an orphan. Provenance on that row is kept. A new face
    /// starts with empty provenance. Mixed neighbors do not average a color.
    pub fn project_face_materials(
        source: &crate::topology::SolidBody,
        edited: &crate::topology::SolidBody,
        lineage: &crate::topology::TopologyLineage,
        baseline: &[FaceMaterialAssignment],
    ) -> ProjectedFaceMaterials {
        let slot_of = |face: u32| baseline.iter().find(|entry| entry.face == Some(face)).map(|entry| entry.slot).unwrap_or(0);
        let live = |id: u32| edited.faces.iter().any(|face| face.id == id);
        let mut assignments = Vec::new();
        let mut bound = Vec::new();
        for entry in baseline {
            match entry.face {
                None => assignments.push(entry.clone()),
                Some(id) if live(id) => {
                    if !bound.contains(&id) {
                        bound.push(id);
                        assignments.push(entry.clone());
                    }
                }
                Some(_) => assignments.push(FaceMaterialAssignment { face: None, slot: entry.slot, provenance: entry.provenance.clone() }),
            }
        }
        let mut defaulted = 0u32;
        let mut seen = Vec::new();
        for birth in &lineage.births {
            if birth.kind != crate::topology::ElementKind::Face || !live(birth.id) || bound.contains(&birth.id) || seen.contains(&birth.id) {
                continue;
            }
            seen.push(birth.id);
            let (slot, mixed) = inherited_slot(source, birth, lineage, &slot_of);
            if mixed {
                defaulted = defaulted.saturating_add(1);
            }
            if slot != 0 {
                bound.push(birth.id);
                assignments.push(FaceMaterialAssignment { face: Some(birth.id), slot, provenance: Vec::new() });
            }
        }
        ProjectedFaceMaterials { assignments, defaulted }
    }

    fn validate_face_materials(&self) -> Result<(), crate::LevelError> {
        if self.materials.len() >= BLOCK_MATERIAL_SLOT_LIMIT as usize {
            return Err(crate::LevelError::Corrupt("block material slots exceed 16".into()));
        }
        for material in std::iter::once(&self.material).chain(self.materials.iter()) {
            material.validate()?;
            if !factor_color([material.base_color[0], material.base_color[1], material.base_color[2]])
                || (material.base_color[3] - 1.0).abs() > 1.0e-5
                || !unit_factor(material.roughness)
                || !unit_factor(material.metallic)
            {
                return Err(crate::LevelError::Corrupt("block material factors are outside 0 to 1".into()));
            }
        }
        for material in &self.materials {
            if material.scheme != crate::MaterialScheme::Builtin || material.name != "standard_white" {
                return Err(crate::LevelError::Corrupt("block material slot is not standard_white".into()));
            }
        }
        let mut seen = Vec::new();
        for entry in &self.face_materials {
            if entry.slot >= self.slot_count() {
                return Err(crate::LevelError::Corrupt("block face names a missing material slot".into()));
            }
            if let Some(face) = entry.face {
                if face == 0 || seen.contains(&face) {
                    return Err(crate::LevelError::Corrupt("block face material id is zero or repeated".into()));
                }
                seen.push(face);
            }
            for token in &entry.provenance {
                crate::semantic_shadow::semantic_reference_token(token).map_err(|_| crate::LevelError::Corrupt("block face provenance is not a semantic reference".into()))?;
            }
        }
        Ok(())
    }

    fn validate_surface_groups(&self) -> Result<(), crate::LevelError> {
        if self.next_surface_group == 0 {
            return Err(crate::LevelError::Corrupt("block surface group counter is behind its groups".into()));
        }
        let mut ids = Vec::new();
        let mut names = Vec::new();
        let mut faces = Vec::new();
        for group in &self.surface_groups {
            if group.id == 0 || group.id >= self.next_surface_group || ids.contains(&group.id) {
                return Err(crate::LevelError::Corrupt("block surface group id is zero or repeated".into()));
            }
            ids.push(group.id);
            if !surface_group_name_ok(&group.name) {
                return Err(crate::LevelError::Corrupt("block surface group name is empty".into()));
            }
            if names.iter().any(|stored: &String| stored == &group.name) {
                return Err(crate::LevelError::Corrupt("block surface group name is duplicated".into()));
            }
            names.push(group.name.clone());
            if group.slot >= self.slot_count() {
                return Err(crate::LevelError::Corrupt("block surface group names a missing material slot".into()));
            }
            for member in &group.members {
                if let Some(face) = member.face {
                    if face == 0 || faces.contains(&face) {
                        return Err(crate::LevelError::Corrupt("block surface group face is zero or repeated".into()));
                    }
                    faces.push(face);
                }
                for token in &member.provenance {
                    crate::semantic_shadow::semantic_reference_token(token)
                        .map_err(|_| crate::LevelError::Corrupt("block surface group provenance is not a semantic reference".into()))?;
                }
            }
        }
        Ok(())
    }

    /// The group that contains this live face, if it is in exactly one.
    pub fn surface_group_of_face(&self, face: u32) -> Option<u32> {
        let mut found = None;
        for group in &self.surface_groups {
            if group.members.iter().any(|member| member.face == Some(face)) {
                if found.is_some() {
                    return None;
                }
                found = Some(group.id);
            }
        }
        found
    }

    pub fn surface_group(&self, id: u32) -> Option<&SurfaceGroup> {
        self.surface_groups.iter().find(|group| group.id == id)
    }

    /// Live faces of one group, in member order. Unresolved members are omitted.
    pub fn resolved_group_faces(&self, id: u32) -> Vec<u32> {
        let Some(group) = self.surface_group(id) else { return Vec::new() };
        let mut faces = Vec::new();
        for member in &group.members {
            if let Some(face) = member.face {
                if self.face_bindable(face) && !faces.contains(&face) {
                    faces.push(face);
                }
            }
        }
        faces
    }

    /// `Covered` when every face is in the same group. `Mixed` when the selection disagrees.
    /// An empty list, and a selection with no grouped face, are `None`.
    pub fn surface_group_cover(&self, faces: &[u32]) -> SurfaceGroupCover {
        let mut covered = None;
        let mut any_grouped = false;
        let mut any_free = false;
        for face in faces {
            match self.surface_group_of_face(*face) {
                Some(id) => {
                    any_grouped = true;
                    match covered {
                        None => covered = Some(id),
                        Some(current) if current != id => return SurfaceGroupCover::Mixed,
                        Some(_) => {}
                    }
                }
                None => any_free = true,
            }
        }
        if any_grouped && any_free {
            return SurfaceGroupCover::Mixed;
        }
        match covered {
            Some(id) => SurfaceGroupCover::Covered(id),
            None => SurfaceGroupCover::None,
        }
    }

    /// Names the selected faces as one group. They must share a slot and a semantic name.
    ///
    /// A slot shared with faces outside the selection is copied first, and only the selection
    /// is assigned. A face that is already in a group writes nothing. This does not append
    /// the intent tape and does not write a body.
    pub fn create_surface_group(&mut self, name: &str, face_ids: &[u32], names_of: impl Fn(u32) -> Vec<String>) -> Result<u32, SurfaceGroupError> {
        let name = clean_surface_group_name(name)?;
        if self.surface_groups.iter().any(|group| group.name == name) {
            return Err(SurfaceGroupError::DuplicateName);
        }
        let unique = unique_bindable_faces(self, face_ids)?;
        if unique.is_empty() {
            return Err(SurfaceGroupError::EmptySelection);
        }
        if unique.iter().any(|face| self.surface_group_of_face(*face).is_some()) {
            return Err(SurfaceGroupError::AlreadyMember);
        }
        let mut slots = Vec::new();
        for face in &unique {
            let slot = self.bound_slot(*face);
            if !slots.contains(&slot) {
                slots.push(slot);
            }
        }
        let Some(shared) = slots.first().copied().filter(|_| slots.len() == 1) else {
            return Err(SurfaceGroupError::MixedSlots);
        };
        let named = named_faces(self, &unique, &names_of)?;
        let slot = if self.selection_owns_slot(&unique, shared) {
            self.assign_faces(&unique, shared, |face| names_from(&named, face)).map_err(surface_group_material_error)?;
            shared
        } else {
            self.make_faces_unique(&unique, |face| names_from(&named, face))
                .map_err(surface_group_material_error)?
                .ok_or(SurfaceGroupError::UnknownFace)?
        };
        let id = self.next_surface_group.max(1);
        self.next_surface_group = id.saturating_add(1);
        self.surface_groups.push(SurfaceGroup {
            id,
            name,
            slot,
            members: named.into_iter().map(|(face, provenance)| SurfaceMember { face: Some(face), provenance }).collect(),
        });
        Ok(id)
    }

    /// Changes the visible name. The id stays. An empty or duplicate name writes nothing.
    pub fn rename_surface_group(&mut self, id: u32, name: &str) -> Result<bool, SurfaceGroupError> {
        let name = clean_surface_group_name(name)?;
        if self.surface_groups.iter().any(|group| group.id != id && group.name == name) {
            return Err(SurfaceGroupError::DuplicateName);
        }
        let group = self.surface_groups.iter_mut().find(|group| group.id == id).ok_or(SurfaceGroupError::UnknownGroup)?;
        if group.name == name {
            return Ok(false);
        }
        group.name = name;
        Ok(true)
    }

    /// Puts the selected faces in `id` and paints them with that group's slot.
    ///
    /// A face in another group, or a face with no semantic name, writes nothing.
    pub fn add_surface_group_faces(&mut self, id: u32, face_ids: &[u32], names_of: impl Fn(u32) -> Vec<String>) -> Result<bool, SurfaceGroupError> {
        if self.surface_group(id).is_none() {
            return Err(SurfaceGroupError::UnknownGroup);
        }
        let unique = unique_bindable_faces(self, face_ids)?;
        if unique.is_empty() {
            return Err(SurfaceGroupError::EmptySelection);
        }
        for face in &unique {
            if let Some(existing) = self.surface_group_of_face(*face) {
                if existing != id {
                    return Err(SurfaceGroupError::AlreadyMember);
                }
            }
        }
        let fresh: Vec<u32> = unique.into_iter().filter(|face| self.surface_group_of_face(*face) != Some(id)).collect();
        if fresh.is_empty() {
            return Ok(false);
        }
        let named = named_faces(self, &fresh, &names_of)?;
        let slot = self.surface_group(id).map(|group| group.slot).ok_or(SurfaceGroupError::UnknownGroup)?;
        self.assign_faces(&fresh, slot, |face| names_from(&named, face)).map_err(surface_group_material_error)?;
        let group = self.surface_groups.iter_mut().find(|group| group.id == id).ok_or(SurfaceGroupError::UnknownGroup)?;
        for (face, provenance) in named {
            group.members.push(SurfaceMember { face: Some(face), provenance });
        }
        Ok(true)
    }

    /// Drops the selected faces from the group. Geometry and the painted slot stay.
    pub fn remove_surface_group_faces(&mut self, id: u32, face_ids: &[u32]) -> Result<bool, SurfaceGroupError> {
        let group = self.surface_groups.iter_mut().find(|group| group.id == id).ok_or(SurfaceGroupError::UnknownGroup)?;
        let before = group.members.len();
        group.members.retain(|member| member.face.is_none_or(|face| !face_ids.contains(&face)));
        Ok(group.members.len() != before)
    }

    /// Points the group at `slot` and paints its resolved members. Unresolved members stay.
    pub fn set_surface_group_slot(&mut self, id: u32, slot: u32, names_of: impl Fn(u32) -> Vec<String>) -> Result<bool, SurfaceGroupError> {
        if self.surface_group(id).is_none() {
            return Err(SurfaceGroupError::UnknownGroup);
        }
        if slot >= self.slot_count() {
            return Err(if slot >= BLOCK_MATERIAL_SLOT_LIMIT { SurfaceGroupError::SlotLimit } else { SurfaceGroupError::UnknownSlot });
        }
        let faces = self.resolved_group_faces(id);
        let previous_slot = self.surface_group(id).map(|group| group.slot).ok_or(SurfaceGroupError::UnknownGroup)?;
        let previous_materials = self.face_materials.clone();
        if let Some(group) = self.surface_groups.iter_mut().find(|group| group.id == id) {
            group.slot = slot;
        }
        if let Err(error) = self.assign_faces(&faces, slot, names_of) {
            if let Some(group) = self.surface_groups.iter_mut().find(|group| group.id == id) {
                group.slot = previous_slot;
            }
            self.face_materials = previous_materials;
            return Err(surface_group_material_error(error));
        }
        let slot_changed = previous_slot != slot;
        let materials_changed = self.face_materials != previous_materials;
        if !slot_changed && !materials_changed {
            return Ok(false);
        }
        Ok(true)
    }

    /// Removes the group. Faces, slots, and geometry stay. The id is not reused.
    pub fn delete_surface_group(&mut self, id: u32) -> Result<bool, SurfaceGroupError> {
        let before = self.surface_groups.len();
        self.surface_groups.retain(|group| group.id != id);
        if self.surface_groups.len() == before {
            return Err(SurfaceGroupError::UnknownGroup);
        }
        Ok(true)
    }

    fn selection_owns_slot(&self, faces: &[u32], slot: u32) -> bool {
        let owners = self.faces_on_slot(slot);
        !owners.is_empty() && owners.iter().all(|face| faces.contains(face))
    }

    /// Rebinds group membership across one topology edit.
    ///
    /// A member whose face id disappeared stays, with the face omitted. A new face joins the
    /// group it descends from only when that descent has one semantic name. Neighbors that
    /// disagree, and a face that cannot be named, increment `ambiguous` and are not stored
    /// as a face id alone.
    pub fn project_surface_groups(
        source: &crate::topology::SolidBody,
        edited: &crate::topology::SolidBody,
        lineage: &crate::topology::TopologyLineage,
        baseline: &[SurfaceGroup],
        edge_names: &[(u32, Vec<String>)],
    ) -> ProjectedSurfaceGroups {
        use crate::topology::{BirthRole, ElementKind};
        let live = |id: u32| edited.faces.iter().any(|face| face.id == id);
        let mut groups = baseline.to_vec();
        for group in &mut groups {
            for member in &mut group.members {
                if member.face.is_some_and(|face| !live(face)) {
                    member.face = None;
                }
            }
        }
        let mut births: Vec<(u32, Vec<(BirthRole, Option<u32>)>)> = Vec::new();
        for birth in &lineage.births {
            if birth.kind != ElementKind::Face || birth.id == 0 || !live(birth.id) {
                continue;
            }
            if let Some((_, roles)) = births.iter_mut().find(|(id, _)| *id == birth.id) {
                roles.push((birth.role, birth.source));
            } else {
                births.push((birth.id, vec![(birth.role, birth.source)]));
            }
        }
        let mut ambiguous = 0u32;
        for (id, roles) in births {
            if groups.iter().any(|group| group.members.iter().any(|member| member.face == Some(id))) {
                continue;
            }
            match group_birth(&groups, source, &roles, edge_names) {
                GroupBirth::Skip => {}
                GroupBirth::Ambiguous => ambiguous = ambiguous.saturating_add(1),
                GroupBirth::Join { group, provenance } => {
                    if let Some(stored) = groups.get_mut(group) {
                        stored.members.push(SurfaceMember { face: Some(id), provenance });
                    }
                }
            }
        }
        ProjectedSurfaceGroups { groups, ambiguous }
    }
}

fn surface_group_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= SURFACE_GROUP_NAME_LIMIT
        && name.chars().all(|glyph| !glyph.is_control())
        && name == name.trim()
}

fn clean_surface_group_name(name: &str) -> Result<String, SurfaceGroupError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(SurfaceGroupError::EmptyName);
    }
    if name.chars().count() > SURFACE_GROUP_NAME_LIMIT || name.chars().any(|glyph| glyph.is_control()) {
        return Err(SurfaceGroupError::NameLimit);
    }
    Ok(name.to_string())
}

fn unique_bindable_faces(record: &BlockRecord, face_ids: &[u32]) -> Result<Vec<u32>, SurfaceGroupError> {
    let mut unique = Vec::new();
    for id in face_ids {
        if *id == 0 || !record.face_bindable(*id) {
            return Err(SurfaceGroupError::UnknownFace);
        }
        if !unique.contains(id) {
            unique.push(*id);
        }
    }
    Ok(unique)
}

fn named_faces(record: &BlockRecord, faces: &[u32], names_of: &impl Fn(u32) -> Vec<String>) -> Result<Vec<(u32, Vec<String>)>, SurfaceGroupError> {
    let mut named = Vec::new();
    for face in faces {
        let provenance = record
            .face_materials
            .iter()
            .find(|entry| entry.face == Some(*face))
            .map(|entry| entry.provenance.clone())
            .filter(|names| !names.is_empty())
            .unwrap_or_else(|| names_of(*face));
        if provenance.is_empty() {
            return Err(SurfaceGroupError::UnnamedFace);
        }
        named.push((*face, provenance));
    }
    Ok(named)
}

fn names_from(named: &[(u32, Vec<String>)], face: u32) -> Vec<String> {
    named.iter().find(|(id, _)| *id == face).map(|(_, names)| names.clone()).unwrap_or_default()
}

fn surface_group_material_error(error: FaceMaterialError) -> SurfaceGroupError {
    match error {
        FaceMaterialError::UnknownFace => SurfaceGroupError::UnknownFace,
        FaceMaterialError::UnknownSlot => SurfaceGroupError::UnknownSlot,
        FaceMaterialError::SlotLimit => SurfaceGroupError::SlotLimit,
        FaceMaterialError::InvalidFactor => SurfaceGroupError::UnknownSlot,
    }
}

enum GroupBirth {
    Skip,
    Ambiguous,
    Join { group: usize, provenance: Vec<String> },
}

fn group_birth(
    groups: &[SurfaceGroup],
    source: &crate::topology::SolidBody,
    roles: &[(crate::topology::BirthRole, Option<u32>)],
    edge_names: &[(u32, Vec<String>)],
) -> GroupBirth {
    use crate::topology::BirthRole;
    if roles.iter().any(|(role, _)| matches!(role, BirthRole::SubdivCell { .. })) {
        let Some((u, v, parent)) = uniform_subdiv_cell(roles) else { return GroupBirth::Ambiguous };
        return join_named(groups, parent, |provenance| compose_each(provenance, |token| format!("F:cell({token},{u},{v})")));
    }
    if roles.iter().any(|(role, _)| matches!(role, BirthRole::ExtrudeSide { .. })) {
        let mut owners = Vec::new();
        let mut edges = Vec::new();
        for (role, _) in roles {
            if let BirthRole::ExtrudeSide { face, boundary } = role {
                if !owners.contains(face) {
                    owners.push(*face);
                }
                if !edges.contains(boundary) {
                    edges.push(*boundary);
                }
            }
        }
        if owners.len() != 1 || edges.len() != 1 {
            return GroupBirth::Ambiguous;
        }
        let edge = names_of_edge(edge_names, edges[0]);
        return join_named(groups, owners[0], |provenance| {
            if edge.is_empty() {
                Vec::new()
            } else {
                compose_pairs(provenance, edge, |face, boundary| format!("F:side({face},{boundary})"))
            }
        });
    }
    if roles.iter().any(|(role, _)| matches!(role, BirthRole::BevelFace { .. } | BirthRole::ExtrudeEdgeWall)) {
        let mut edges = Vec::new();
        for (role, birth_source) in roles {
            match role {
                BirthRole::BevelFace { source: edge } => {
                    if !edges.contains(edge) {
                        edges.push(*edge);
                    }
                }
                BirthRole::ExtrudeEdgeWall => {
                    if let Some(edge) = birth_source {
                        if !edges.contains(edge) {
                            edges.push(*edge);
                        }
                    }
                }
                _ => {}
            }
        }
        if edges.is_empty() {
            return GroupBirth::Ambiguous;
        }
        let mut neighbors = Vec::new();
        for edge in &edges {
            for face in source.faces_of_edge(*edge) {
                if !neighbors.contains(&face) {
                    neighbors.push(face);
                }
            }
        }
        let group = match agreed_group(groups, &neighbors) {
            Ok(Some(index)) => index,
            Ok(None) => return GroupBirth::Skip,
            Err(()) => return GroupBirth::Ambiguous,
        };
        let label = if roles.iter().any(|(role, _)| matches!(role, BirthRole::BevelFace { .. })) { "F:bevel" } else { "F:edge-wall" };
        let mut provenance = Vec::new();
        for edge in edges {
            let names = names_of_edge(edge_names, edge);
            if names.len() != 1 {
                return GroupBirth::Ambiguous;
            }
            let token = format!("{label}({})", names[0]);
            if token_recorded(groups, &token) {
                return GroupBirth::Ambiguous;
            }
            if !provenance.contains(&token) {
                provenance.push(token);
            }
        }
        if provenance.is_empty() {
            return GroupBirth::Ambiguous;
        }
        return GroupBirth::Join { group, provenance };
    }
    GroupBirth::Skip
}

fn uniform_subdiv_cell(roles: &[(crate::topology::BirthRole, Option<u32>)]) -> Option<(u32, u32, u32)> {
    use crate::topology::BirthRole;
    let (BirthRole::SubdivCell { u, v }, parent) = roles.first()? else { return None };
    let parent = (*parent)?;
    let same = roles.iter().all(|(role, source)| {
        matches!(role, BirthRole::SubdivCell { u: cell_u, v: cell_v } if *cell_u == *u && *cell_v == *v) && *source == Some(parent)
    });
    if same { Some((*u, *v, parent)) } else { None }
}

fn factor_color(color: [f32; 3]) -> bool {
    color.iter().all(|channel| unit_factor(*channel))
}

fn join_named(groups: &[SurfaceGroup], face: u32, names_of: impl FnOnce(&[String]) -> Vec<String>) -> GroupBirth {
    let Some(group) = groups.iter().position(|stored| stored.members.iter().any(|member| member.face == Some(face))) else {
        return GroupBirth::Skip;
    };
    let provenance = groups[group]
        .members
        .iter()
        .find(|member| member.face == Some(face))
        .map(|member| names_of(&member.provenance))
        .unwrap_or_default();
    let mut unique = Vec::new();
    for token in provenance {
        if !unique.contains(&token) {
            unique.push(token);
        }
    }
    if unique.is_empty() {
        GroupBirth::Ambiguous
    } else {
        GroupBirth::Join { group, provenance: unique }
    }
}

fn compose_each(provenance: &[String], token_of: impl Fn(&str) -> String) -> Vec<String> {
    provenance.iter().map(|token| token_of(token)).collect()
}

fn compose_pairs(faces: &[String], edges: &[String], token_of: impl Fn(&str, &str) -> String) -> Vec<String> {
    let mut names = Vec::new();
    for face in faces {
        for edge in edges {
            names.push(token_of(face, edge));
        }
    }
    names
}

fn names_of_edge(edge_names: &[(u32, Vec<String>)], edge: u32) -> &[String] {
    edge_names.iter().find(|(id, _)| *id == edge).map(|(_, names)| names.as_slice()).unwrap_or(&[])
}

fn agreed_group(groups: &[SurfaceGroup], faces: &[u32]) -> Result<Option<usize>, ()> {
    if faces.is_empty() {
        return Ok(None);
    }
    let mut found = None;
    let mut free = false;
    for face in faces {
        match groups.iter().position(|group| group.members.iter().any(|member| member.face == Some(*face))) {
            Some(index) => {
                if found.is_some_and(|current| current != index) {
                    return Err(());
                }
                found = Some(index);
            }
            None => free = true,
        }
    }
    if free && found.is_some() {
        return Err(());
    }
    if free {
        return Ok(None);
    }
    Ok(found)
}

fn token_recorded(groups: &[SurfaceGroup], token: &str) -> bool {
    let later = format!("{token},");
    groups.iter().any(|group| {
        group.members.iter().any(|member| member.provenance.iter().any(|stored| stored == token || stored.starts_with(&later)))
    })
}

fn unit_factor(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn inherited_slot(
    source: &crate::topology::SolidBody,
    birth: &crate::topology::ElementBirth,
    lineage: &crate::topology::TopologyLineage,
    slot_of: &impl Fn(u32) -> u32,
) -> (u32, bool) {
    use crate::topology::{BirthRole, ElementKind};
    match birth.role {
        BirthRole::SubdivCell { .. } => (slot_of(birth.source.unwrap_or(0)), false),
        BirthRole::ExtrudeSide { face, .. } => (slot_of(face), false),
        BirthRole::ExtrudeCap => (slot_of(birth.source.unwrap_or(birth.id)), false),
        BirthRole::BevelFace { .. } => {
            let mut edges = Vec::new();
            for other in &lineage.births {
                if other.kind == ElementKind::Face && other.id == birth.id {
                    if let BirthRole::BevelFace { source: edge } = other.role {
                        if !edges.contains(&edge) {
                            edges.push(edge);
                        }
                    }
                }
            }
            shared_neighbor_slot(source, &edges, slot_of)
        }
        BirthRole::ExtrudeEdgeWall => shared_neighbor_slot(source, &[birth.source.unwrap_or(0)], slot_of),
        _ => (0, false),
    }
}

fn shared_neighbor_slot(source: &crate::topology::SolidBody, edges: &[u32], slot_of: &impl Fn(u32) -> u32) -> (u32, bool) {
    let mut slots = Vec::new();
    for edge in edges {
        for face in source.faces_of_edge(*edge) {
            let slot = slot_of(face);
            if !slots.contains(&slot) {
                slots.push(slot);
            }
        }
    }
    match slots.as_slice() {
        [slot] => (*slot, false),
        [] => (0, false),
        _ => (0, true),
    }
}

/// Rejects a non-finite axis. Clamps a finite axis into the supported range.
pub fn finite_size(size_m: [f64; 3]) -> Result<[f64; 3], crate::AuthoringError> {
    let mut out = [0.0; 3];
    for axis in 0..3 {
        if !size_m[axis].is_finite() {
            return Err(crate::AuthoringError::InvalidValue);
        }
        out[axis] = size_m[axis].clamp(BLOCK_MIN_EXTENT_M, BLOCK_MAX_EXTENT_M);
    }
    Ok(out)
}

/// One scene-local solid. Translation and rotation are the block frame in the scene.
#[derive(Clone, Copy, Debug)]
pub struct BlockSolid {
    pub translation: Vec3,
    pub rotation: Quat,
    pub size_m: [f64; 3],
}

/// Pushes `point` out of one expanded box. A point already outside stays.
///
/// The box is centered on `translation` and oriented by `rotation`. Half extents are
/// `size / 2` plus `radius`. The push is along the nearest face, then rotated back.
pub fn keep_outside_box(point: Vec3, solid: BlockSolid, radius: f64) -> Vec3 {
    let delta = Vec3::new(point.x - solid.translation.x, point.y - solid.translation.y, point.z - solid.translation.z);
    let local = solid.rotation.conjugate().rotate(delta);
    let half = [
        solid.size_m[0].abs() * 0.5 + radius,
        solid.size_m[1].abs() * 0.5 + radius,
        solid.size_m[2].abs() * 0.5 + radius,
    ];
    let abs = [local.x.abs(), local.y.abs(), local.z.abs()];
    if abs[0] >= half[0] || abs[1] >= half[1] || abs[2] >= half[2] {
        return point;
    }
    let penetration = [half[0] - abs[0], half[1] - abs[1], half[2] - abs[2]];
    let axis = if penetration[0] <= penetration[1] && penetration[0] <= penetration[2] {
        0
    } else if penetration[1] <= penetration[2] {
        1
    } else {
        2
    };
    let mut ejected = local;
    let signed = half[axis].copysign(match axis {
        0 => local.x,
        1 => local.y,
        _ => local.z,
    });
    match axis {
        0 => ejected.x = signed,
        1 => ejected.y = signed,
        _ => ejected.z = signed,
    }
    let world = solid.rotation.rotate(ejected);
    Vec3::new(solid.translation.x + world.x, solid.translation.y + world.y, solid.translation.z + world.z)
}

/// A few passes so overlapping boxes still eject a point that started inside.
pub fn keep_outside_blocks(point: Vec3, solids: &[BlockSolid], radius: f64) -> Vec3 {
    let mut point = point;
    for _ in 0..4 {
        for solid in solids {
            point = keep_outside_box(point, *solid, radius);
        }
    }
    point
}

/// One editor reference line in scene-local meters. Not an entity and not a saved mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceSegment {
    pub from: Vec3,
    pub to: Vec3,
    pub color: [f32; 4],
}

/// Ground grid on the XZ plane and a Y-up axis triad at the scene origin.
///
/// `camera_scene_x` and `camera_scene_z` are meters in the scene frame. The returned
/// points are scene-local. World X of the origin is [`BOOTSTRAP_ROOT_M`]. Lines far
/// from the camera are omitted. The grid sits at [`REFERENCE_GRID_Y_M`].
pub fn editor_reference_segments(camera_scene_x: f64, camera_scene_z: f64, axis_length_m: f64) -> Vec<ReferenceSegment> {
    let mut segments = Vec::new();
    let length = if axis_length_m.is_finite() { axis_length_m.clamp(0.35, 40.0) } else { 1.0 };
    let short = length * 0.45;
    push_axis(&mut segments, Vec3::new(length, 0.0, 0.0), [0.90, 0.16, 0.14, 1.0]);
    push_axis(&mut segments, Vec3::new(-short, 0.0, 0.0), [0.40, 0.07, 0.06, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, length, 0.0), [0.20, 0.78, 0.28, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, -short, 0.0), [0.09, 0.35, 0.12, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, 0.0, length), [0.20, 0.45, 0.95, 1.0]);
    push_axis(&mut segments, Vec3::new(0.0, 0.0, -short), [0.09, 0.20, 0.42, 1.0]);
    if !camera_scene_x.is_finite() || !camera_scene_z.is_finite() {
        return segments;
    }
    push_grid(&mut segments, camera_scene_x, camera_scene_z, 1.0, MINOR_RADIUS_M, [0.28, 0.30, 0.32], true);
    push_grid(&mut segments, camera_scene_x, camera_scene_z, 10.0, MAJOR_RADIUS_M, [0.48, 0.50, 0.52], false);
    segments
}

fn push_axis(segments: &mut Vec<ReferenceSegment>, end: Vec3, color: [f32; 4]) {
    segments.push(ReferenceSegment { from: Vec3::ZERO, to: end, color });
}

fn push_grid(segments: &mut Vec<ReferenceSegment>, camera_x: f64, camera_z: f64, step: f64, radius: f64, color: [f32; 3], skip_major: bool) {
    let y = REFERENCE_GRID_Y_M;
    let min_x = ((camera_x - radius) / step).floor() as i32;
    let max_x = ((camera_x + radius) / step).ceil() as i32;
    let min_z = ((camera_z - radius) / step).floor() as i32;
    let max_z = ((camera_z + radius) / step).ceil() as i32;
    let z0 = camera_z - radius;
    let z1 = camera_z + radius;
    let x0 = camera_x - radius;
    let x1 = camera_x + radius;
    for index in min_x..=max_x {
        let x = index as f64 * step;
        if skip_major && (x / 10.0).round() * 10.0 == snapped(x) {
            continue;
        }
        let fade = fade_of((x - camera_x).abs(), radius);
        if fade < FADE_FLOOR {
            continue;
        }
        segments.push(ReferenceSegment {
            from: Vec3::new(x, y, z0),
            to: Vec3::new(x, y, z1),
            color: [color[0] * fade, color[1] * fade, color[2] * fade, 1.0],
        });
    }
    for index in min_z..=max_z {
        let z = index as f64 * step;
        if skip_major && (z / 10.0).round() * 10.0 == snapped(z) {
            continue;
        }
        let fade = fade_of((z - camera_z).abs(), radius);
        if fade < FADE_FLOOR {
            continue;
        }
        segments.push(ReferenceSegment {
            from: Vec3::new(x0, y, z),
            to: Vec3::new(x1, y, z),
            color: [color[0] * fade, color[1] * fade, color[2] * fade, 1.0],
        });
    }
}

fn snapped(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn fade_of(distance: f64, radius: f64) -> f32 {
    (1.0 - distance / radius).clamp(0.0, 1.0) as f32
}

/// Scene-local origin, expressed in the root frame. The scene sits under the bootstrap root.
pub fn scene_origin_world_x() -> f64 {
    BOOTSTRAP_ROOT_M
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        empty_world_level, parse_level, ComponentRecord, GameSettings, LevelDocument, LevelError, PawnSelection, PhysicalControl, PlayControl,
        Quat, StartupCameraPolicy, Vec3, LEVEL_BLOCK_VERSION, LEVEL_FORMAT_VERSION,
    };

    fn solid(translation: Vec3, rotation: Quat, size: [f64; 3]) -> BlockSolid {
        BlockSolid { translation, rotation, size_m: size }
    }

    fn painted_cube() -> BlockRecord {
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        record.add_material_slot().unwrap();
        assert!(record.assign_faces(&[1, 3], 1, |face| vec![format!("F:seed/{}", face - 1)]).unwrap());
        record
    }

    #[test]
    fn assigning_two_faces_can_be_reselected_and_a_bad_edit_writes_nothing() {
        let mut record = painted_cube();
        assert!(record.body.is_none());
        assert!(record.is_plain());
        assert_eq!(record.faces_on_slot(1), vec![1, 3]);
        assert_eq!(record.faces_on_slot(0), vec![2, 4, 5, 6]);
        assert_eq!(record.bound_slot(1), 1);
        assert_eq!(record.bound_slot(2), 0);
        assert_eq!(record.face_materials.iter().map(|entry| entry.provenance.clone()).collect::<Vec<_>>(), vec![vec!["F:seed/0".to_string()], vec!["F:seed/2".to_string()]]);
        let intent = record.intent.len();
        assert!(!record.assign_faces(&[], 1, |_| Vec::new()).unwrap());
        assert_eq!(record.intent.len(), intent);

        let before = record.clone();
        assert_eq!(record.assign_faces(&[99], 1, |_| Vec::new()), Err(FaceMaterialError::UnknownFace));
        assert_eq!(record.assign_faces(&[1, 99], 1, |_| Vec::new()), Err(FaceMaterialError::UnknownFace));
        assert_eq!(record, before);

        for _ in 0..14 {
            record.add_material_slot().unwrap();
        }
        assert_eq!(record.slot_count(), BLOCK_MATERIAL_SLOT_LIMIT);
        let full = record.clone();
        assert_eq!(record.add_material_slot(), Err(FaceMaterialError::SlotLimit));
        assert_eq!(record.assign_faces(&[1], BLOCK_MATERIAL_SLOT_LIMIT, |_| Vec::new()), Err(FaceMaterialError::SlotLimit));
        assert_eq!(record, full);

        let body = record.body.clone();
        let faces = record.face_materials.clone();
        assert!(record.set_material_factors(1, [0.2, 0.4, 0.6], 0.3, 0.8).unwrap());
        assert_eq!(record.body, body);
        assert_eq!(record.face_materials, faces);
        assert_eq!(record.material_slot(1).unwrap().base_color, [0.2, 0.4, 0.6, 1.0]);
        assert!((record.material_slot(1).unwrap().roughness - 0.3).abs() < 1.0e-6);
        assert!((record.material_slot(1).unwrap().metallic - 0.8).abs() < 1.0e-6);
        assert_eq!(record.material.base_color, before.material.base_color);
        let edited = record.clone();
        assert_eq!(record.set_material_factors(1, [1.2, 0.0, 0.0], 0.3, 0.8), Err(FaceMaterialError::InvalidFactor));
        assert_eq!(record, edited);
    }

    #[test]
    fn make_unique_copies_the_shared_slot_and_a_bad_face_writes_nothing() {
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        let plain = record.clone();
        let intent = record.intent.len();
        assert_eq!(record.make_faces_unique(&[], |_| Vec::new()).unwrap(), None);
        assert_eq!(record.make_faces_unique(&[99], |_| Vec::new()), Err(FaceMaterialError::UnknownFace));
        assert_eq!(record.make_faces_unique(&[1, 99], |_| Vec::new()), Err(FaceMaterialError::UnknownFace));
        assert_eq!(record, plain);

        let slot = record.make_faces_unique(&[1, 3], |face| vec![format!("F:seed/{}", face - 1)]).unwrap();
        assert_eq!(slot, Some(1));
        assert!(record.body.is_none());
        assert!(record.is_plain());
        assert_eq!(record.intent.len(), intent);
        assert_eq!(record.faces_on_slot(1), vec![1, 3]);
        assert_eq!(record.faces_on_slot(0), vec![2, 4, 5, 6]);
        assert_eq!(record.material_slot(1).unwrap().base_color, plain.material.base_color);
        assert_eq!(record.material_slot(1).unwrap().roughness, plain.material.roughness);
        assert_eq!(record.material_slot(1).unwrap().metallic, plain.material.metallic);
        assert_eq!(record.material_slot(1).unwrap().name, "standard_white");

        assert!(record.set_material_factors(1, [0.2, 0.4, 0.6], 0.3, 0.8).unwrap());
        let copied = record.make_faces_unique(&[1], |_| vec!["F:seed/0".into()]).unwrap();
        assert_eq!(copied, Some(2));
        assert_eq!(record.bound_slot(1), 2);
        assert_eq!(record.bound_slot(3), 1);
        assert_eq!(record.material_slot(2).unwrap().base_color, [0.2, 0.4, 0.6, 1.0]);
        assert!((record.material_slot(2).unwrap().roughness - 0.3).abs() < 1.0e-6);
        assert!((record.material_slot(2).unwrap().metallic - 0.8).abs() < 1.0e-6);
        assert_eq!(record.material.base_color, plain.material.base_color);
        assert_eq!(record.intent.len(), intent);

        let mut mixed = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        mixed.add_material_slot().unwrap();
        assert!(mixed.set_material_factors(1, [1.0, 0.0, 0.0], 0.2, 0.1).unwrap());
        assert!(mixed.assign_faces(&[1], 1, |_| vec!["F:seed/0".into()]).unwrap());
        let mixed_slot = mixed.make_faces_unique(&[1, 2], |face| vec![format!("F:seed/{}", face - 1)]).unwrap();
        assert_eq!(mixed_slot, Some(2));
        assert_eq!(mixed.bound_slot(1), 2);
        assert_eq!(mixed.bound_slot(2), 2);
        assert_eq!(mixed.material_slot(2).unwrap().base_color, mixed.material.base_color);
        assert_eq!(mixed.material_slot(1).unwrap().base_color, [1.0, 0.0, 0.0, 1.0]);

        let mut full = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        for _ in 0..15 {
            full.add_material_slot().unwrap();
        }
        let saved = full.clone();
        assert_eq!(full.make_faces_unique(&[1], |_| Vec::new()), Err(FaceMaterialError::SlotLimit));
        assert_eq!(full, saved);
        assert_eq!(full.intent.len(), intent);
    }

    #[test]
    fn subdivision_and_extrude_inherit_and_a_mixed_bevel_stays_on_slot_zero() {
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        record.add_material_slot().unwrap();
        assert!(record.assign_faces(&[3], 1, |_| vec!["F:seed/2".into()]).unwrap());
        let source = record.material_body().unwrap();
        let (divided, lineage) = source.subdivide_face_traced(3, 2, 2).unwrap();
        let projected = BlockRecord::project_face_materials(&source, &divided.body, &lineage, &record.face_materials);
        assert_eq!(projected.defaulted, 0);
        for face in &divided.body.faces {
            let slot = projected.assignments.iter().find(|entry| entry.face == Some(face.id)).map(|entry| entry.slot).unwrap_or(0);
            if face.id == 3 || face.id > 6 {
                assert_eq!(slot, 1, "face {}", face.id);
            } else {
                assert_eq!(slot, 0, "face {}", face.id);
            }
        }
        divided.body.validate().unwrap();

        let (extruded, lineage) = source.extrude_faces_traced(&[3], [0.0, 0.25, 0.0]).unwrap();
        let projected = BlockRecord::project_face_materials(&source, &extruded.body, &lineage, &record.face_materials);
        assert_eq!(projected.defaulted, 0);
        extruded.body.validate().unwrap();
        for face in &extruded.body.faces {
            let slot = projected.assignments.iter().find(|entry| entry.face == Some(face.id)).map(|entry| entry.slot).unwrap_or(0);
            let cap_or_wall = face.id == 3 || face.id > 6;
            assert_eq!(slot, u32::from(cap_or_wall), "face {}", face.id);
        }

        let mut mixed = painted_cube();
        mixed.add_material_slot().unwrap();
        assert!(mixed.assign_faces(&[5], 2, |_| vec!["F:seed/4".into()]).unwrap());
        let (cut, lineage) = source.bevel_edges_traced(&[16], 0.05).unwrap();
        let projected = BlockRecord::project_face_materials(&source, &cut.edit.body, &lineage, &mixed.face_materials);
        assert!(projected.defaulted > 0);
        cut.edit.body.validate().unwrap();
        let original = source.faces.iter().map(|face| face.id).collect::<Vec<_>>();
        for face in &cut.edit.body.faces {
            let slot = projected.assignments.iter().find(|entry| entry.face == Some(face.id)).map(|entry| entry.slot).unwrap_or(0);
            if face.id == 1 {
                assert_eq!(slot, 1);
            } else if face.id == 5 {
                assert_eq!(slot, 2);
            } else if !original.contains(&face.id) {
                assert_eq!(slot, 0, "new face {}", face.id);
            }
        }
    }

    #[test]
    fn a_disappeared_face_stays_an_orphan_and_is_not_reattached() {
        let source = crate::topology::SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        let (divided, lineage) = source.subdivide_face_traced(3, 2, 2).unwrap();
        assert!(divided.body.faces.iter().any(|face| face.id == 3));
        let orphan = FaceMaterialAssignment { face: None, slot: 1, provenance: vec!["F:seed/2".into()] };
        let projected = BlockRecord::project_face_materials(&source, &divided.body, &lineage, &[orphan.clone()]);
        assert_eq!(projected.assignments, vec![orphan]);
        assert!(projected.assignments.iter().all(|entry| entry.face.is_none()));

        let gone = FaceMaterialAssignment { face: Some(99), slot: 1, provenance: vec!["F:seed/2".into()] };
        let projected = BlockRecord::project_face_materials(&source, &source, &crate::topology::TopologyLineage::default(), &[gone]);
        assert_eq!(projected.assignments, vec![FaceMaterialAssignment { face: None, slot: 1, provenance: vec!["F:seed/2".into()] }]);
    }

    fn cube_edge_names(body: &crate::topology::SolidBody) -> Vec<(u32, Vec<String>)> {
        body.edges.iter().map(|edge| (edge.id, vec![format!("E:seed-edge/{}", edge.id.saturating_sub(9))])).collect()
    }

    #[test]
    fn surface_groups_rebind_upper_rim_and_refuse_a_mixed_bevel() {
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        let plain = record.clone();
        let intent = record.intent.clone();
        assert_eq!(record.create_surface_group("  ", &[], |_| Vec::new()), Err(SurfaceGroupError::EmptyName));
        assert_eq!(record.create_surface_group("Upper Rim", &[], |_| Vec::new()), Err(SurfaceGroupError::EmptySelection));
        assert_eq!(record.create_surface_group("Upper Rim", &[0], |_| vec!["F:seed/0".into()]), Err(SurfaceGroupError::UnknownFace));
        assert_eq!(record.create_surface_group("Upper Rim", &[3], |_| Vec::new()), Err(SurfaceGroupError::UnnamedFace));
        assert_eq!(record, plain);

        let mut mixed = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        mixed.add_material_slot().unwrap();
        assert!(mixed.assign_faces(&[1], 1, |_| vec!["F:seed/0".into()]).unwrap());
        let saved = mixed.clone();
        assert_eq!(
            mixed.create_surface_group("Upper Rim", &[1, 2], |face| vec![format!("F:seed/{}", face - 1)]),
            Err(SurfaceGroupError::MixedSlots)
        );
        assert_eq!(mixed, saved);

        let id = record.create_surface_group("Upper Rim", &[3, 5], |face| vec![format!("F:seed/{}", face - 1)]).unwrap();
        assert_eq!(id, 1);
        assert_eq!(record.next_surface_group, 2);
        assert!(record.body.is_none());
        assert_eq!(record.intent, intent);
        assert_eq!(record.surface_groups[0].name, "Upper Rim");
        assert_eq!(record.surface_groups[0].slot, 1);
        assert_eq!(record.resolved_group_faces(id), vec![3, 5]);
        assert!(record.surface_groups[0].members.iter().any(|member| member.face == Some(3) && member.provenance == vec!["F:seed/2".to_string()]));
        assert!(record.surface_groups[0].members.iter().any(|member| member.face == Some(5) && member.provenance == vec!["F:seed/4".to_string()]));
        assert_eq!(record.bound_slot(3), 1);
        assert_eq!(record.bound_slot(1), 0);
        let grouped = record.clone();
        assert_eq!(record.create_surface_group("Upper Rim", &[1], |_| vec!["F:seed/0".into()]), Err(SurfaceGroupError::DuplicateName));
        assert_eq!(record.create_surface_group("Trim", &[3], |_| vec!["F:seed/2".into()]), Err(SurfaceGroupError::AlreadyMember));
        assert_eq!(record, grouped);
        assert!(!record.rename_surface_group(id, "Upper Rim").unwrap());
        assert!(record.rename_surface_group(id, "Trim").unwrap());
        assert_eq!(record.surface_group(id).unwrap().name, "Trim");
        assert_eq!(record.surface_group(id).unwrap().id, id);
        assert!(record.rename_surface_group(id, "Upper Rim").unwrap());

        let source = record.material_body().unwrap();
        let edge_names = cube_edge_names(&source);
        let (divided, lineage) = source.subdivide_face_traced(3, 2, 2).unwrap();
        let projected = BlockRecord::project_surface_groups(&source, &divided.body, &lineage, &record.surface_groups, &edge_names);
        assert_eq!(projected.ambiguous, 0);
        let members = |groups: &[SurfaceGroup], face: u32| groups.iter().any(|group| group.members.iter().any(|member| member.face == Some(face)));
        for face in &divided.body.faces {
            let joined = members(&projected.groups, face.id);
            if face.id == 3 || face.id == 5 || face.id > 6 {
                assert!(joined, "face {}", face.id);
            } else {
                assert!(!joined, "face {}", face.id);
            }
        }
        let parent = projected.groups[0].members.iter().find(|member| member.face == Some(3)).unwrap();
        assert!(parent.provenance.iter().any(|token| token == "F:seed/2"));
        assert!(projected.groups[0].members.iter().any(|member| {
            member.face.is_some_and(|face| face > 6) && member.provenance.iter().any(|token| token.contains("F:cell(F:seed/2,"))
        }));
        let mut hosted = record.clone();
        hosted.body = Some(divided.body.clone());
        hosted.size_m = divided.size_m;
        hosted.surface_groups = projected.groups;
        hosted.validate().unwrap();

        let (extruded, lineage) = source.extrude_faces_traced(&[3], [0.0, 0.25, 0.0]).unwrap();
        let projected = BlockRecord::project_surface_groups(&source, &extruded.body, &lineage, &record.surface_groups, &edge_names);
        assert_eq!(projected.ambiguous, 0);
        for face in &extruded.body.faces {
            let joined = members(&projected.groups, face.id);
            if face.id == 3 || face.id == 5 || face.id > 6 {
                assert!(joined, "extrude face {}", face.id);
            } else {
                assert!(!joined, "extrude face {}", face.id);
            }
        }
        assert!(projected.groups[0].members.iter().any(|member| {
            member.face.is_some_and(|face| face > 6) && member.provenance.iter().any(|token| token.starts_with("F:side(F:seed/2,E:seed-edge/"))
        }));

        let (split, lineage) = source.split_edge_traced(16).unwrap();
        let projected = BlockRecord::project_surface_groups(&source, &split.body, &lineage, &record.surface_groups, &edge_names);
        assert_eq!(projected.ambiguous, 0);
        assert!(members(&projected.groups, 3));
        assert!(members(&projected.groups, 5));
        assert_eq!(projected.groups[0].members.iter().filter(|member| member.face.is_some()).count(), 2);

        let mut sides = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        sides.create_surface_group("Exterior", &[1], |_| vec!["F:seed/0".into()]).unwrap();
        sides.create_surface_group("Interior", &[5], |_| vec!["F:seed/4".into()]).unwrap();
        let (cut, lineage) = source.bevel_edges_traced(&[16], 0.05).unwrap();
        let projected = BlockRecord::project_surface_groups(&source, &cut.edit.body, &lineage, &sides.surface_groups, &edge_names);
        assert!(projected.ambiguous > 0);
        let original = source.faces.iter().map(|face| face.id).collect::<Vec<_>>();
        for face in &cut.edit.body.faces {
            if !original.contains(&face.id) {
                assert!(!members(&projected.groups, face.id), "bevel face {}", face.id);
            }
        }
        assert!(members(&projected.groups, 1));
        assert!(members(&projected.groups, 5));

        let mut orphan = record.surface_groups.clone();
        orphan[0].members.push(SurfaceMember { face: Some(99), provenance: vec!["F:seed/2".into()] });
        let projected = BlockRecord::project_surface_groups(&source, &source, &crate::topology::TopologyLineage::default(), &orphan, &edge_names);
        let kept = projected.groups[0].members.iter().find(|member| member.provenance == vec!["F:seed/2".to_string()] && member.face.is_none());
        assert!(kept.is_some());
        assert!(members(&projected.groups, 3));

        assert!(record.add_surface_group_faces(id, &[1], |_| vec!["F:seed/0".into()]).unwrap());
        assert_eq!(record.resolved_group_faces(id), vec![3, 5, 1]);
        assert_eq!(record.bound_slot(1), 1);
        assert!(!record.add_surface_group_faces(id, &[1], |_| vec!["F:seed/0".into()]).unwrap());
        assert!(record.remove_surface_group_faces(id, &[1]).unwrap());
        assert_eq!(record.bound_slot(1), 1);
        assert!(!record.resolved_group_faces(id).contains(&1));
        assert!(record.delete_surface_group(id).unwrap());
        assert!(record.surface_groups.is_empty());
        assert_eq!(record.next_surface_group, 2);
        assert_eq!(record.bound_slot(3), 1);
        assert!(record.body.is_none());
        let again = record.create_surface_group("Trim", &[2], |_| vec!["F:seed/1".into()]).unwrap();
        assert_eq!(again, 2);
        assert_ne!(again, id);
    }

    #[test]
    fn an_inside_point_is_pushed_out_and_an_outside_point_stays() {
        let box_solid = solid(Vec3::ZERO, Quat::IDENTITY, [2.0, 2.0, 2.0]);
        let inside = keep_outside_box(Vec3::ZERO, box_solid, FLY_COLLISION_RADIUS_M);
        assert!((inside.x - 1.3).abs() < 1.0e-9, "{inside:?}");
        assert!(inside.y.abs() < 1.0e-9 && inside.z.abs() < 1.0e-9, "{inside:?}");
        let outside = Vec3::new(2.0, 0.0, 0.0);
        let stayed = keep_outside_box(outside, box_solid, FLY_COLLISION_RADIUS_M);
        assert!((stayed.x - 2.0).abs() < 1.0e-9 && stayed.y.abs() < 1.0e-9 && stayed.z.abs() < 1.0e-9, "{stayed:?}");
    }

    #[test]
    fn a_rotated_box_pushes_along_the_rotated_face() {
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let box_solid = solid(Vec3::ZERO, yaw, [2.0, 2.0, 2.0]);
        let pushed = keep_outside_box(Vec3::new(0.0, 0.0, -0.2), box_solid, FLY_COLLISION_RADIUS_M);
        assert!(pushed.x.abs() < 1.0e-6 && pushed.y.abs() < 1.0e-6, "{pushed:?}");
        assert!((pushed.z + 1.3).abs() < 1.0e-6, "{pushed:?}");
    }

    #[test]
    fn empty_level_stays_world_settings_and_a_block_round_trips_without_triangles() {
        let empty = empty_world_level();
        assert_eq!(empty.format_version, LEVEL_FORMAT_VERSION);
        assert_eq!(empty.entities.len(), 1);
        assert!(empty.entities[0].components.iter().all(|component| matches!(component, ComponentRecord::WorldSettings)));
        let mut world = empty.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 3.0, 4.0]).unwrap()).unwrap();
        assert_eq!(world.entity_outline().iter().find(|row| row.uuid == id).unwrap().class, crate::AuthoringClass::Block);
        assert!(!world.entity_ownership(id).unwrap().capabilities.mesh_renderer);
        let mesh_id = world.object_mesh(id).unwrap();
        let set = world.derived_meshlets(mesh_id).unwrap();
        let coverage = crate::meshlet_coverage(world.meshes().get(mesh_id).unwrap(), set);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert_eq!(coverage.canonical_triangles, 12);
        let cluster_count = set.meshlets.len();
        assert_eq!(cluster_count, 12);
        let captured = LevelDocument::capture(&world, empty.level_uuid, "Empty").unwrap();
        assert_eq!(captured.format_version, LEVEL_BLOCK_VERSION);
        let json = captured.to_json();
        assert!(json.contains("ParametricBlock"));
        assert!(json.contains("2") && json.contains("3") && json.contains("4"));
        assert!(
            !json.contains("bevel_m") && !json.contains("inset_m") && !json.contains("history") && !json.contains("seed_size_m") && !json.contains("\"body\"") && !json.contains("\"steps\"") && !json.contains("next_step"),
            "a plain block keeps the size-only record"
        );
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the block level");
        }
        let mut loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let record = loaded.authored_block(id).unwrap();
        assert_eq!(record.size_m, [2.0, 3.0, 4.0]);
        let reloaded = loaded.object_mesh(id).unwrap();
        assert_eq!(loaded.derived_meshlets(reloaded).unwrap().meshlets.len(), cluster_count);
        let facts = crate::SurfaceDetailFacts {
            opaque: true,
            skinned: false,
            ui: false,
            particle: false,
            bounds_min: loaded.meshes().get(reloaded).unwrap().bounds().aabb.min,
            bounds_max: loaded.meshes().get(reloaded).unwrap().bounds().aabb.max,
            exact: true,
        };
        assert!(crate::microgeometry_active(crate::MicrogeometryMode::Auto, &facts));
        let flat = vec![
            crate::DetailCluster { flag: 1, projected_px: 0.2, compatible: true, has_anchor: true, exact: false };
            cluster_count
        ];
        let reasons = crate::select_detail_clusters(&flat, 256);
        assert!(reasons.iter().all(|reason| *reason == crate::DetailReject::BelowThreshold));
        let exact = vec![
            crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true };
            cluster_count
        ];
        let exact_reasons = crate::select_detail_clusters(&exact, 256);
        assert!(exact_reasons.iter().all(|reason| *reason == crate::DetailReject::Exact));
        assert_eq!(exact_reasons.iter().filter(|reason| **reason == crate::DetailReject::Selected).count(), 0);
        let pose = loaded.authored_local_pose(id).unwrap().0;
        assert!((pose.y - 1.0).abs() < 1.0e-9 && (pose.z + 4.0).abs() < 1.0e-9, "{pose:?}");
        assert_eq!(loaded.set_block_extent(id, 0, 6.0).unwrap(), crate::AuthoringResult::Applied);
        assert!(loaded.derived_meshlets(reloaded).is_none());
        let ownership = loaded.entity_ownership(id).unwrap();
        let resized = ownership.mesh.unwrap();
        assert_ne!(resized, reloaded);
        let resized_set = loaded.derived_meshlets(resized).unwrap();
        let resized_coverage = crate::meshlet_coverage(loaded.meshes().get(resized).unwrap(), resized_set);
        assert_eq!(resized_coverage.missing_triangles, 0);
        assert_eq!(resized_coverage.duplicate_triangles, 0);
        assert_eq!(resized_coverage.canonical_triangles, 12);
        let bounds = loaded.meshes().get(resized).unwrap().bounds();
        assert!((bounds.aabb.max[0] - 3.0).abs() < 1.0e-4, "{:?}", bounds.aabb);
        assert!((bounds.aabb.max[1] - 1.5).abs() < 1.0e-4, "{:?}", bounds.aabb);
        assert!((bounds.aabb.min[2] + 2.0).abs() < 1.0e-4, "{:?}", bounds.aabb);
        let again = LevelDocument::capture(&loaded, empty.level_uuid, "Empty").unwrap();
        assert_eq!(parse_level(&again.to_json()).unwrap().instantiate().unwrap().authored_block(id).unwrap().size_m[0], 6.0);
        let solids = loaded.block_solids();
        assert_eq!(solids.len(), 1);
        assert!((solids[0].size_m[0] - 6.0).abs() < 1.0e-9);
    }

    fn lane3(mesh: &crate::Mesh, index: u32, offset: usize) -> [f32; 3] {
        let stream = &mesh.streams()[0];
        let start = index as usize * stream.stride as usize + offset;
        let bytes = &stream.bytes;
        [
            f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()),
            f32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()),
            f32::from_le_bytes(bytes[start + 8..start + 12].try_into().unwrap()),
        ]
    }

    fn quantize(position: [f32; 3]) -> [i32; 3] {
        position.map(|lane| (lane * 10_000.0).round() as i32)
    }

    fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
        [
            left[1] * right[2] - left[2] * right[1],
            left[2] * right[0] - left[0] * right[2],
            left[0] * right[1] - left[1] * right[0],
        ]
    }

    fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
        left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
    }

    fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
        let delta = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
        (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
    }

    /// Derived triangles, meshlets, and hierarchy spheres. Einstein is not part of this audit.
    fn audit_block_surface(mesh: &crate::Mesh, set: &crate::MeshletSet, half: [f32; 3]) {
        let triangles = mesh.triangle_indices();
        assert_eq!(triangles.len(), 12);
        assert_eq!(mesh.vertex_count(), 24);
        let mut corners = Vec::new();
        for index in 0..mesh.vertex_count() {
            let position = mesh.position(index).unwrap();
            for axis in 0..3 {
                assert!((position[axis].abs() - half[axis]).abs() < 1.0e-4, "{position:?} is not a corner of {half:?}");
            }
            let key = quantize(position);
            if !corners.contains(&key) {
                corners.push(key);
            }
        }
        assert_eq!(corners.len(), 8, "a box has eight logical corners");
        let mut face_keys = Vec::new();
        let mut face_counts = Vec::new();
        let mut edges: Vec<(([i32; 3], [i32; 3]), Vec<[i32; 3]>)> = Vec::new();
        for triangle in &triangles {
            let positions = [mesh.position(triangle[0]).unwrap(), mesh.position(triangle[1]).unwrap(), mesh.position(triangle[2]).unwrap()];
            let stored = lane3(mesh, triangle[0], 32);
            let geometric = cross(
                [positions[1][0] - positions[0][0], positions[1][1] - positions[0][1], positions[1][2] - positions[0][2]],
                [positions[2][0] - positions[0][0], positions[2][1] - positions[0][1], positions[2][2] - positions[0][2]],
            );
            assert!(dot(geometric, stored) > 0.0, "winding {positions:?} against {stored:?}");
            let normal_key = stored.map(|lane| lane.round() as i32);
            for position in positions {
                for axis in 0..3 {
                    if stored[axis].abs() > 0.5 {
                        assert!((position[axis] - stored[axis] * half[axis]).abs() < 1.0e-4, "triangle leaves its face");
                    }
                }
            }
            if let Some(slot) = face_keys.iter().position(|key| *key == normal_key) {
                face_counts[slot] += 1;
            } else {
                face_keys.push(normal_key);
                face_counts.push(1);
            }
            let keys = [quantize(positions[0]), quantize(positions[1]), quantize(positions[2])];
            for pair in [(0, 1), (1, 2), (2, 0)] {
                let mut ends = [keys[pair.0], keys[pair.1]];
                if ends[0] > ends[1] {
                    ends.swap(0, 1);
                }
                let key = (ends[0], ends[1]);
                if let Some((_, uses)) = edges.iter_mut().find(|(found, _)| *found == key) {
                    uses.push(normal_key);
                } else {
                    edges.push((key, vec![normal_key]));
                }
            }
        }
        assert_eq!(face_keys.len(), 6);
        assert!(face_counts.iter().all(|count| *count == 2), "{face_counts:?}");
        let mut cube_edges = 0;
        let mut diagonals = 0;
        for ((start, end), uses) in &edges {
            let differ = (0..3).filter(|axis| start[*axis] != end[*axis]).count();
            if differ == 1 {
                cube_edges += 1;
                assert_eq!(uses.len(), 2, "a shared face boundary is one coincident edge");
                assert_ne!(uses[0], uses[1]);
            } else {
                assert_eq!(differ, 2);
                diagonals += 1;
                assert_eq!(uses.len(), 2);
                assert_eq!(uses[0], uses[1]);
            }
        }
        assert_eq!(cube_edges, 12);
        assert_eq!(diagonals, 6);
        let coverage = crate::meshlet_coverage(mesh, set);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert_eq!(coverage.canonical_triangles, 12);
        assert_eq!(set.meshlets.len(), 12);
        let draw = crate::meshlet_draw(set);
        assert_eq!(draw.indices.len(), 36);
        for index in &draw.indices {
            let position = mesh.position(*index).expect("meshlet index");
            for axis in 0..3 {
                assert!(position[axis].abs() <= half[axis] + 1.0e-4, "{position:?} outside {half:?}");
            }
        }
        for meshlet in &set.meshlets {
            for axis in 0..3 {
                assert!(meshlet.bounds_min[axis] >= -half[axis] - 1.0e-4);
                assert!(meshlet.bounds_max[axis] <= half[axis] + 1.0e-4);
            }
        }
        let records: Vec<_> = set.meshlets.iter().map(crate::GpuMeshletRecord::from_meshlet).collect();
        let hierarchy = crate::build_cluster_hierarchy(&records);
        assert_eq!(hierarchy.leaf_count, 12);
        for node in &hierarchy.nodes {
            for slot in 0..node.child_count as usize {
                let child = &hierarchy.nodes[node.children[slot] as usize];
                let reach = distance(node.center, child.center) + child.radius;
                assert!(reach <= node.radius + 1.0e-4, "parent sphere {reach} does not contain {child:?}");
            }
        }
    }

    #[test]
    fn a_two_meter_block_stays_inside_its_bounds_until_einstein_relief() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mesh_id = world.object_mesh(id).unwrap();
        let mesh = world.meshes().get(mesh_id).unwrap();
        let set = world.derived_meshlets(mesh_id).unwrap();
        audit_block_surface(mesh, set, [1.0, 1.0, 1.0]);
        let first = mesh.triangle_indices()[0][0];
        let corner = mesh.position(first).unwrap();
        assert!((corner[0] - 1.0).abs() < 1.0e-4 && (corner[1] + 1.0).abs() < 1.0e-4 && (corner[2] + 1.0).abs() < 1.0e-4, "{corner:?}");
        let relief = crate::build_procedural_microtriangles(
            &[crate::SurfaceAnchor {
                position: corner,
                normal: [1.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                depth_m: 1.0,
            }],
            1,
            true,
            940.0,
            0.5,
            &crate::MicroBudget::surface(),
            crate::DetailProvider::EinsteinSurface,
        );
        assert!(relief.triangle_count > 0, "submitting the anchor still builds the public reference marker");
        let mut past_face = false;
        for vertex in 0..relief.vertex_count as usize {
            let start = vertex * 60;
            let x = f32::from_le_bytes(relief.vertices[start..start + 4].try_into().unwrap());
            past_face |= x > 1.0 + 1.0e-4;
        }
        assert!(!past_face, "the public provider stays on the face");
        let close = crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true };
        let reasons = crate::select_detail_clusters(&[close], 256);
        assert_eq!(reasons, vec![crate::DetailReject::Exact]);
        assert_eq!(world.set_block_extent(id, 0, 6.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 1, 3.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.set_block_extent(id, 2, 4.0).unwrap(), crate::AuthoringResult::Applied);
        let resized = world.entity_ownership(id).unwrap().mesh.unwrap();
        audit_block_surface(world.meshes().get(resized).unwrap(), world.derived_meshlets(resized).unwrap(), [3.0, 1.5, 2.0]);
    }

    #[test]
    fn pushing_a_face_keeps_the_opposite_face_and_the_center() {
        let pushed = push_face([2.0, 2.0, 2.0], Vec3::ZERO, Quat::IDENTITY, 0, 1.0).unwrap();
        assert_eq!(pushed.size_m, [3.0, 2.0, 2.0]);
        assert!((pushed.translation.x - 0.5).abs() < 1.0e-9, "{:?}", pushed.translation);
        let opposite = pushed.translation.x - pushed.size_m[0] * 0.5;
        assert!((opposite + 1.0).abs() < 1.0e-9, "opposite face moved to {opposite}");
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let turned = push_face([2.0, 2.0, 2.0], Vec3::ZERO, yaw, 0, 1.0).unwrap();
        assert!(turned.translation.x.abs() < 1.0e-6 && turned.translation.y.abs() < 1.0e-6, "{:?}", turned.translation);
        assert!((turned.translation.z + 0.5).abs() < 1.0e-6, "{:?}", turned.translation);
        let local_opposite = yaw.rotate(Vec3::new(-turned.size_m[0] * 0.5, 0.0, 0.0));
        let world_opposite = Vec3::new(turned.translation.x + local_opposite.x, turned.translation.y + local_opposite.y, turned.translation.z + local_opposite.z);
        assert!(world_opposite.x.abs() < 1.0e-6 && (world_opposite.z - 1.0).abs() < 1.0e-6, "{world_opposite:?}");
        assert_eq!(snap_dimension(0.02), 0.0);
        assert!((snap_dimension(0.03) - 0.05).abs() < 1.0e-12);
        assert!((snap_dimension(-0.06) + 0.05).abs() < 1.0e-12);
    }

    #[test]
    fn align_snap_and_mirror_math_stay_on_the_solid() {
        let seated = align_translation_to_ground(Vec3::new(1.0, 3.0, -2.0), Quat::IDENTITY, [2.0, 2.0, 2.0]);
        assert!((seated.y - 1.0).abs() < 1.0e-9 && (seated.x - 1.0).abs() < 1.0e-9, "{seated:?}");
        let yaw = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
        let turned = align_translation_to_ground(Vec3::new(0.0, 0.0, 0.0), yaw, [4.0, 2.0, 2.0]);
        assert!(turned.y > 0.9, "lowest corner was below the ground: {turned:?}");
        let snapped = snap_translation(Vec3::new(1.06, -0.04, 2.0), BLOCK_POSITION_SNAP_M);
        assert!((snapped.x - 1.1).abs() < 1.0e-9 && snapped.y.abs() < 1.0e-9 && (snapped.z - 2.0).abs() < 1.0e-9, "{snapped:?}");
        let copy = mirrored_translation(Vec3::new(0.0, 1.0, -4.0), Quat::IDENTITY, [2.0, 3.0, 4.0], 0).unwrap();
        assert!((copy.x - 2.0).abs() < 1.0e-9 && (copy.y - 1.0).abs() < 1.0e-9, "{copy:?}");
        let swapped = swap_insets([0.2, 0.0, 0.0, 0.0, 0.0, 0.0], 0).unwrap();
        assert_eq!(swapped, [0.0, 0.2, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(swap_insets(swapped, 0).unwrap()[0], 0.2);
    }

    #[test]
    fn a_face_handle_hit_ignores_a_ray_that_misses_the_sphere() {
        let size = [2.0, 2.0, 2.0];
        let hit = nearest_face_handle(Vec3::ZERO, Quat::IDENTITY, size, Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0), 0.2);
        assert_eq!(hit, Some(4), "+Z is the face the ray meets");
        let miss = nearest_face_handle(Vec3::ZERO, Quat::IDENTITY, size, Vec3::new(0.0, 3.0, 5.0), Vec3::new(0.0, 0.0, -1.0), 0.2);
        assert_eq!(miss, None);
        let on_face = face_from_local_point(Vec3::new(1.0, 0.2, -0.1), size).unwrap();
        assert_eq!(on_face, 0);
    }

    #[test]
    fn a_block_advertises_solid_tools_and_a_mesh_does_not() {
        let world = crate::SceneWorld::bootstrap();
        let near = world.entity_outline().iter().find(|row| row.name == "Near Triangle").unwrap().uuid;
        let light = world.entity_outline().iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let settings = world.entity_outline().iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let near_caps = crate::AuthoringCapabilities::from_entity(world.entity_ownership(near).unwrap().capabilities);
        let light_caps = crate::AuthoringCapabilities::from_entity(world.entity_ownership(light).unwrap().capabilities);
        let settings_caps = crate::AuthoringCapabilities::from_entity(world.entity_ownership(settings).unwrap().capabilities);
        assert!(near_caps.material_assignable);
        assert!(!near_caps.parametric_solid && !near_caps.face_editable && !near_caps.patternable && !near_caps.boolean_operand);
        assert!(!light_caps.parametric_solid && !light_caps.face_editable && !light_caps.collidable);
        assert!(light_caps.component_host);
        assert!(!settings_caps.component_host && !settings_caps.parametric_solid);
        let mut empty = empty_world_level().instantiate().unwrap();
        let block = empty.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let block_caps = crate::AuthoringCapabilities::from_entity(empty.entity_ownership(block).unwrap().capabilities);
        assert!(block_caps.transformable && block_caps.parametric_solid && block_caps.face_editable && block_caps.patternable);
        assert!(block_caps.material_assignable && block_caps.collidable && block_caps.component_host);
        assert!(!block_caps.boolean_operand);
    }

    #[test]
    fn a_face_push_moves_the_center_and_commit_logs_once() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let baseline = world.entity_local_pose(id).unwrap().translation;
        assert_eq!(
            world.push_block_face(id, 0, [2.0, 2.0, 2.0], baseline, 1.0).unwrap(),
            crate::AuthoringResult::Applied
        );
        assert_eq!(world.authored_block(id).unwrap().size_m, [3.0, 2.0, 2.0]);
        assert!(world.authored_block(id).unwrap().history.is_empty());
        let moved = world.entity_local_pose(id).unwrap().translation;
        assert!((moved.x - 0.5).abs() < 1.0e-9 && (moved.y - 1.0).abs() < 1.0e-9 && (moved.z + 4.0).abs() < 1.0e-9, "{moved:?}");
        assert_eq!(world.commit_block_face(id, 0, [2.0, 2.0, 2.0]).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        assert_eq!(world.restore_block_drag(id, [2.0, 2.0, 2.0], [0.0; 6], 0.0, baseline).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.restore_block_drag(id, [2.0, 2.0, 2.0], [0.0; 6], 0.0, baseline).unwrap(), crate::AuthoringResult::Unchanged);
        let restored = world.authored_block(id).unwrap();
        assert_eq!(restored.size_m, [2.0, 2.0, 2.0]);
        assert_eq!(restored.history.len(), 1, "cancel does not append or erase the log");
        assert_eq!(world.entity_local_pose(id).unwrap().translation.x.abs(), 0.0);
    }

    #[test]
    fn a_bevel_preview_logs_once_on_commit_and_cancel_restores_it() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.preview_block_bevel(id, 0.2).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.preview_block_bevel(id, 0.4).unwrap(), crate::AuthoringResult::Applied);
        assert!(world.authored_block(id).unwrap().history.is_empty());
        assert!((world.authored_block(id).unwrap().bevel_m - 0.4).abs() < 1.0e-9);
        assert_eq!(world.commit_block_bevel(id, 0.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        assert_eq!(world.commit_block_bevel(id, 0.4).unwrap(), crate::AuthoringResult::Unchanged);
        let pose = world.entity_local_pose(id).unwrap().translation;
        assert_eq!(world.restore_block_drag(id, [2.0, 2.0, 2.0], [0.0; 6], 0.0, pose).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().bevel_m, 0.0);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 1);
        assert_eq!(world.preview_block_inset(id, 2, 0.15).unwrap(), crate::AuthoringResult::Applied);
        assert!(world.authored_block(id).unwrap().history.len() == 1);
        assert_eq!(world.commit_block_inset(id, 2, 0.0).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.authored_block(id).unwrap().history.len(), 2);
    }

    #[test]
    fn inset_and_bevel_stay_inside_the_analytic_box_and_round_trip() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let plain = world.object_mesh(id).unwrap();
        assert_eq!(world.derived_meshlets(plain).unwrap().meshlets.len(), 12);
        assert_eq!(world.set_block_bevel(id, 0.1).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(world.set_block_inset(id, 0, 0.1).unwrap(), crate::AuthoringResult::Applied);
        let featured = world.object_mesh(id).unwrap();
        let mesh = world.meshes().get(featured).unwrap();
        assert!(mesh.index_count() > 36);
        for index in 0..mesh.vertex_count() {
            let position = mesh.position(index).unwrap();
            assert!(position.iter().all(|axis| axis.abs() <= 1.0 + 1.0e-3), "{position:?}");
        }
        let coverage = crate::meshlet_coverage(mesh, world.derived_meshlets(featured).unwrap());
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert!(coverage.canonical_triangles > 12);
        let captured = LevelDocument::capture(&world, empty_world_level().level_uuid, "Featured").unwrap();
        let json = captured.to_json();
        assert!(json.contains("bevel_m") && json.contains("inset_m") && json.contains("history"));
        assert!(json.contains("\"op\":\"bevel\"") || json.contains("\"op\": \"bevel\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the featured block");
        }
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let record = loaded.authored_block(id).unwrap();
        assert!((record.bevel_m - 0.1).abs() < 1.0e-9);
        assert!((record.inset_m[0] - 0.1).abs() < 1.0e-9);
        assert!(record.inset_m[1..].iter().all(|value| value.abs() < 1.0e-9));
        assert!(record.history.len() >= 2);
        let reloaded_count = loaded.derived_meshlets(loaded.object_mesh(id).unwrap()).unwrap().meshlets.len();
        assert_eq!(reloaded_count, world.derived_meshlets(featured).unwrap().meshlets.len());
    }

    #[test]
    fn mirror_copies_the_solid_and_leaves_the_original() {
        let mut world = empty_world_level().instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 3.0, 4.0]).unwrap()).unwrap();
        world.set_block_inset(id, 0, 0.2).unwrap();
        let before = world.entity_local_pose(id).unwrap().translation;
        let copy = world.mirror_block(id, 0).unwrap();
        assert_ne!(copy, id);
        let original = world.authored_block(id).unwrap();
        let mirrored = world.authored_block(copy).unwrap();
        assert!((original.inset_m[0] - 0.2).abs() < 1.0e-9 && original.inset_m[1].abs() < 1.0e-9);
        assert!(mirrored.inset_m[0].abs() < 1.0e-9 && (mirrored.inset_m[1] - 0.2).abs() < 1.0e-9);
        assert!(matches!(mirrored.history.last(), Some(BlockOp::Mirror { axis: 0 })));
        assert!(!original.history.iter().any(|op| matches!(op, BlockOp::Mirror { .. })));
        let stayed = world.entity_local_pose(id).unwrap().translation;
        let moved = world.entity_local_pose(copy).unwrap().translation;
        assert!((stayed.x - before.x).abs() < 1.0e-9 && (stayed.z - before.z).abs() < 1.0e-9);
        assert!((moved.x - (before.x + 2.0)).abs() < 1.0e-6 && (moved.y - before.y).abs() < 1.0e-6, "{moved:?}");
        let grounded = world.align_block_to_ground(id).unwrap();
        assert_eq!(grounded, crate::AuthoringResult::Applied);
        assert!((world.entity_local_pose(id).unwrap().translation.y - 1.5).abs() < 1.0e-6);
        world.set_entity_local_translation(copy, Vec3::new(1.06, 1.0, -4.04)).unwrap();
        assert_eq!(world.snap_block_translation(copy, BLOCK_POSITION_SNAP_M).unwrap(), crate::AuthoringResult::Applied);
        let snapped = world.entity_local_pose(copy).unwrap().translation;
        assert!((snapped.x - 1.1).abs() < 1.0e-9 && (snapped.z + 4.0).abs() < 1.0e-9, "{snapped:?}");
        let duplicated = world.duplicate_authored(id).unwrap();
        assert!((world.authored_block(duplicated).unwrap().inset_m[0] - 0.2).abs() < 1.0e-9);
        assert_eq!(world.reset_block_shape(id).unwrap(), crate::AuthoringResult::Applied);
        let reset = world.authored_block(id).unwrap();
        assert!(reset.is_plain() && reset.size_m == [2.0, 2.0, 2.0]);
        assert_eq!(world.derived_meshlets(world.object_mesh(id).unwrap()).unwrap().meshlets.len(), 12);
    }

    #[test]
    fn a_version_1_block_is_corrupt_and_a_future_level_is_rejected() {
        let mut document = empty_world_level();
        document.entities.push(crate::EntityRecord {
            uuid: crate::EntityId::parse("44444444-4444-4444-8444-444444444444").unwrap(),
            name: "Block".into(),
            parent_uuid: None,
            components: vec![
                ComponentRecord::Transform { translation: Vec3::new(0.0, 1.0, 0.0), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) },
                ComponentRecord::ParametricBlock(BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()),
            ],
        });
        assert!(matches!(document.validate(), Err(LevelError::Corrupt(_))));
        document.format_version = LEVEL_BLOCK_VERSION;
        document.validate().unwrap();
        document.format_version = LEVEL_BLOCK_VERSION + 1;
        assert!(matches!(document.validate(), Err(LevelError::UnsupportedVersion(7))));
    }

    #[test]
    fn reference_axes_sit_on_the_scene_origin_and_far_grid_lines_are_omitted() {
        let segments = editor_reference_segments(0.0, 0.0, 2.0);
        assert!(segments.iter().any(|segment| segment.from == Vec3::ZERO && (segment.to.x - 2.0).abs() < 1.0e-9 && segment.to.y == 0.0));
        assert!(segments.iter().any(|segment| segment.from == Vec3::ZERO && (segment.to.y - 2.0).abs() < 1.0e-9));
        assert!(segments.iter().any(|segment| segment.from == Vec3::ZERO && (segment.to.z - 2.0).abs() < 1.0e-9));
        assert!((scene_origin_world_x() - BOOTSTRAP_ROOT_M).abs() < 1.0e-6);
        assert!(segments.iter().filter(|segment| segment.from.y < 0.0).all(|segment| (segment.from.y - REFERENCE_GRID_Y_M).abs() < 1.0e-9));
        assert!(segments.iter().any(|segment| segment.from.y < 0.0));
        let far = editor_reference_segments(10_000.0, 0.0, 2.0);
        assert!(far.iter().filter(|segment| segment.from.y < 0.0).all(|segment| segment.from.x > 9_000.0));
    }

    #[test]
    fn free_fly_stops_short_of_the_analytic_box() {
        let document = empty_world_level();
        let mut runtime = document.instantiate().unwrap();
        runtime.create_block(Vec3::new(0.0, 1.0, 0.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut settings = GameSettings::inactive();
        settings.player_controller = true;
        settings.pawn = PawnSelection::DefaultFreeFly;
        settings.mapping_context = Some("JARVIG.Default".into());
        settings.startup_camera = StartupCameraPolicy::Pawn;
        let mut control = PlayControl::attach(&mut runtime, &settings).unwrap();
        let pawn = control.pawn().unwrap();
        control.place_free_fly_view(&mut runtime, Vec3::new(0.0, 1.6, 2.0), 0.0, 0.0).unwrap();
        control.device_mut().held.insert(PhysicalControl::KeyW);
        for _ in 0..20 {
            control.tick(&mut runtime, 0.1).unwrap();
        }
        let stopped = runtime.authored_local_pose(pawn).unwrap().0;
        assert!(stopped.z > 1.2 && stopped.z < 1.45, "{stopped:?}");
        assert!(stopped.z > 0.0, "{stopped:?}");
    }

    #[test]
    fn free_fly_inside_a_block_is_ejected() {
        let document = empty_world_level();
        let mut runtime = document.instantiate().unwrap();
        runtime.create_block(Vec3::new(0.0, 1.0, 0.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut settings = GameSettings::inactive();
        settings.player_controller = true;
        settings.pawn = PawnSelection::DefaultFreeFly;
        settings.mapping_context = Some("JARVIG.Default".into());
        settings.startup_camera = StartupCameraPolicy::Pawn;
        let mut control = PlayControl::attach(&mut runtime, &settings).unwrap();
        let pawn = control.pawn().unwrap();
        control.place_free_fly_view(&mut runtime, Vec3::new(0.0, 1.0, 0.0), 0.0, 0.0).unwrap();
        control.tick(&mut runtime, 0.1).unwrap();
        let ejected = runtime.authored_local_pose(pawn).unwrap().0;
        assert!((ejected.x - 1.3).abs() < 1.0e-6, "{ejected:?}");
        assert!((ejected.y - 1.0).abs() < 1.0e-6 && ejected.z.abs() < 1.0e-6, "{ejected:?}");
    }

    #[test]
    fn geometry_truth_save_discards_derived_geometry_and_reloads_the_same_body() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.split_block_edge(id, 12).unwrap(), crate::AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        let authoritative = record.body.clone().unwrap();
        authoritative.validate().unwrap();
        assert_eq!(record.steps.len(), 1);
        assert_eq!(record.steps[0].id.number(), 1);
        assert_eq!(record.steps[0].concrete, vec![ConcreteElement::Edge(12)]);
        assert_eq!(record.next_step, 2);
        assert!(authoritative.vertices.iter().any(|vertex| vertex.id == 1));
        assert!(authoritative.edges.iter().any(|edge| edge.id == 12));
        assert!(!record.steps[0].concrete.contains(&ConcreteElement::Vertex(1)));
        let saved_steps = record.steps.clone();
        let derived = DerivedRenderGeometry::from_body(&authoritative);
        let mesh_id = world.object_mesh(id).unwrap();
        let resident = DerivedRenderGeometry {
            mesh: world.meshes().get(mesh_id).unwrap().clone(),
            meshlets: world.derived_meshlets(mesh_id).unwrap().clone(),
        };
        assert!(derived.same_surface(&resident));
        let triangles = derived.mesh.triangle_indices();
        let positions: Vec<_> = (0..derived.mesh.vertex_count()).map(|index| derived.mesh.position(index).unwrap()).collect();
        let meshlets = derived.meshlets.meshlets.clone();
        let vertex_indices = derived.meshlets.vertex_indices.clone();
        let local_indices = derived.meshlets.local_indices.clone();
        let grid = derived.meshlets.grid_resolution;
        assert_eq!(triangles.len(), 14);
        let captured = LevelDocument::capture(&world, document.level_uuid, "Truth").unwrap();
        assert_eq!(captured.format_version, LEVEL_BLOCK_VERSION);
        let json = captured.to_json();
        assert!(json.contains("\"body\"") && json.contains("\"steps\"") && json.contains("next_step") && json.contains("split-edge"));
        assert!(json.contains("\"kind\": \"edge\""));
        for forbidden in ["meshlet", "einstein", "Einstein", "indices", "triangle", "hierarchy"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into the block level");
        }
        derived.discard();
        resident.discard();
        drop(world);
        let parsed = parse_level(&json).unwrap();
        let loaded = parsed.instantiate().unwrap();
        let again = loaded.authored_block(id).unwrap();
        let loaded_body = again.body.clone().unwrap();
        loaded_body.validate().unwrap();
        assert_eq!(loaded_body, authoritative);
        assert_eq!(again.steps, saved_steps);
        assert_eq!(again.next_step, 2);
        assert!(matches!(again.history.last(), Some(BlockOp::SplitEdge { edge: 12 })));
        let regenerated = DerivedRenderGeometry::from_body(&loaded_body);
        let loaded_mesh = loaded.object_mesh(id).unwrap();
        let loaded_resident = DerivedRenderGeometry {
            mesh: loaded.meshes().get(loaded_mesh).unwrap().clone(),
            meshlets: loaded.derived_meshlets(loaded_mesh).unwrap().clone(),
        };
        assert!(regenerated.same_surface(&loaded_resident));
        assert_eq!(regenerated.mesh.triangle_indices(), triangles);
        let positions_now: Vec<_> = (0..regenerated.mesh.vertex_count()).map(|index| regenerated.mesh.position(index).unwrap()).collect();
        assert_eq!(positions_now, positions);
        assert_eq!(regenerated.meshlets.meshlets, meshlets);
        assert_eq!(regenerated.meshlets.vertex_indices, vertex_indices);
        assert_eq!(regenerated.meshlets.local_indices, local_indices);
        assert_eq!(regenerated.meshlets.grid_resolution, grid);
        let coverage = crate::meshlet_coverage(&regenerated.mesh, &regenerated.meshlets);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert_eq!(coverage.canonical_triangles, triangles.len() as u32);
        let mut legacy = parsed.clone();
        for entity in &mut legacy.entities {
            for component in &mut entity.components {
                if let ComponentRecord::ParametricBlock(block) = component {
                    block.steps.clear();
                    block.next_step = 1;
                }
            }
        }
        let legacy_json = legacy.to_json();
        assert!(!legacy_json.contains("\"steps\"") && !legacy_json.contains("next_step"));
        assert!(legacy_json.contains("\"body\"") && legacy_json.contains("split-edge"));
        let legacy_loaded = parse_level(&legacy_json).unwrap().instantiate().unwrap();
        let legacy_record = legacy_loaded.authored_block(id).unwrap();
        assert!(legacy_record.steps.is_empty());
        assert_eq!(legacy_record.next_step, 1);
        assert_eq!(legacy_record.body.unwrap(), authoritative);
        let legacy_surface = DerivedRenderGeometry::from_body(&authoritative);
        assert_eq!(legacy_surface.mesh.triangle_indices(), triangles);
        assert_eq!(legacy_surface.meshlets.vertex_indices, vertex_indices);
        assert_eq!(legacy_surface.meshlets.local_indices, local_indices);
    }

    #[test]
    fn geometry_truth_steps_stay_beside_the_body() {
        let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        record.push_op(BlockOp::ExtrudeFace { face: 0, distance_m: 0.25 });
        assert!(record.steps[0].concrete.is_empty());
        assert_eq!(record.steps[0].id.number(), 1);
        assert!(record.body.is_none());
        record.history.clear();
        record.steps.clear();
        record.next_step = 1;
        record.history.push(BlockOp::Size { size_m: [2.0, 2.0, 2.0] });
        record.push_op(BlockOp::Mirror { axis: 0 });
        assert!(record.steps.is_empty());
        assert_eq!(record.history.len(), 2);
        assert!(record.body.is_none());
        record.validate().unwrap();
        let mut fresh = BlockRecord::standard([2.0, 2.0, 2.0]).unwrap();
        for _ in 0..BLOCK_HISTORY_LIMIT + 1 {
            fresh.push_op(BlockOp::SplitEdge { edge: 12 });
        }
        assert_eq!(fresh.history.len(), BLOCK_HISTORY_LIMIT);
        assert_eq!(fresh.steps.len(), BLOCK_HISTORY_LIMIT);
        assert_eq!(fresh.steps[0].id.number(), 2);
        assert_eq!(fresh.steps.last().unwrap().id.number(), BLOCK_HISTORY_LIMIT as u64 + 1);
        assert_eq!(fresh.next_step, BLOCK_HISTORY_LIMIT as u64 + 2);
        assert!(fresh.steps.iter().all(|step| step.concrete == vec![ConcreteElement::Edge(12)]));
        assert!(fresh.body.is_none());
        fresh.validate().unwrap();
        fresh.steps[0].concrete = vec![ConcreteElement::Vertex(1)];
        assert!(fresh.validate().is_err());
    }

    #[test]
    fn a_split_edge_round_trips_and_a_body_is_not_a_plain_box() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let plain = LevelDocument::capture(&world, document.level_uuid, "Split").unwrap().to_json();
        assert!(!plain.contains("\"body\""));
        assert!(world.authored_block(id).unwrap().is_plain());
        assert_eq!(world.split_block_edge(id, 12).unwrap(), crate::AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        assert!(!record.is_plain());
        assert_eq!(record.body.as_ref().unwrap().vertices.len(), 9);
        assert!(matches!(record.history.last(), Some(BlockOp::SplitEdge { edge: 12 })));
        let mesh = world.object_mesh(id).unwrap();
        assert_eq!(world.meshes().get(mesh).unwrap().triangle_indices().len(), 14);
        let json = LevelDocument::capture(&world, document.level_uuid, "Split").unwrap().to_json();
        assert!(json.contains("\"body\"") && json.contains("split-edge"));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = loaded.authored_block(id).unwrap();
        assert_eq!(again.body.as_ref().unwrap().vertices.len(), 9);
        assert!(again.body.as_ref().unwrap().edges.iter().any(|edge| edge.id == 12));
        assert!(world.push_block_face(id, 0, [2.0, 2.0, 2.0], Vec3::new(0.0, 1.0, -4.0), 0.25).is_err());
        assert!(world.set_block_bevel(id, 0.1).is_err());
    }

    #[test]
    fn a_beveled_edge_round_trips_as_bevel_edges() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let body = crate::topology::SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        let cut = body.bevel_edges(&[16], 0.2).unwrap();
        assert!((cut.width_m - 0.2).abs() < 1.0e-9 && !cut.clamped && !cut.expanded);
        let translation = Vec3::new(cut.edit.shift[0], 1.0 + cut.edit.shift[1], -4.0 + cut.edit.shift[2]);
        assert_eq!(world.preview_block_body(id, cut.edit.body.clone(), translation).unwrap(), crate::AuthoringResult::Applied);
        assert_eq!(
            world
                .commit_block_topology(id, BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m })
                .unwrap(),
            crate::AuthoringResult::Applied
        );
        let record = world.authored_block(id).unwrap();
        assert!(matches!(
            record.history.last(),
            Some(BlockOp::BevelEdges { edges, distance_m }) if edges.as_slice() == [16] && (*distance_m - 0.2).abs() < 1.0e-9
        ));
        assert_eq!(record.history.last().unwrap().summary(), "Bevel E:16 0.20");
        record.body.as_ref().unwrap().validate().unwrap();
        assert!(world.set_block_bevel(id, 0.1).is_err());
        let json = LevelDocument::capture(&world, document.level_uuid, "Bevel").unwrap().to_json();
        assert!(json.contains("bevel-edges") && json.contains("\"body\""));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = loaded.authored_block(id).unwrap();
        assert!(matches!(
            again.history.last(),
            Some(BlockOp::BevelEdges { edges, distance_m }) if edges.as_slice() == [16] && (*distance_m - 0.2).abs() < 1.0e-9
        ));
        let loaded_body = again.body.unwrap();
        loaded_body.validate().unwrap();
        assert_eq!(loaded_body.faces.len(), cut.edit.body.faces.len());
        assert_eq!(loaded_body.vertices.len(), cut.edit.body.vertices.len());
        assert!(loaded_body.edges.iter().all(|edge| edge.id != 16));
    }

    #[test]
    fn a_subdivided_face_extrudes_twice_and_reloads_as_the_same_body() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.subdivide_block_face(id, 3, 4, 4).unwrap(), crate::AuthoringResult::Applied);
        let divided = world.authored_block(id).unwrap().body.unwrap();
        let cap = divided
            .faces
            .iter()
            .find(|face| {
                divided.unit_normal(face.id).unwrap()[1] > 0.9
                    && divided.face_positions(face.id).unwrap().iter().all(|position| position[0].abs() <= 0.51 && position[2].abs() <= 0.51)
            })
            .unwrap()
            .id;
        let before_faces = divided.faces.len();
        let before_vertices = divided.vertices.len();
        assert_eq!(world.extrude_block_faces(id, &[cap], [0.0, 0.4, 0.0]).unwrap(), crate::AuthoringResult::Applied);
        let raised = world.authored_block(id).unwrap();
        assert!(matches!(raised.history.last(), Some(BlockOp::ExtrudeFaces { faces, .. }) if faces.as_slice() == [cap]));
        let raised_body = raised.body.unwrap();
        assert_eq!(raised_body.faces.len(), before_faces + 4);
        assert!(raised_body.vertices.len() > before_vertices);
        raised_body.validate().unwrap();
        let cap_loop = raised_body.face_loop(cap).unwrap().to_vec();
        let cap_y = raised_body.face_positions(cap).unwrap().iter().map(|position| position[1]).sum::<f64>() / cap_loop.len() as f64;
        let neighbor_id = raised_body
            .faces
            .iter()
            .find(|face| face.id != cap && raised_body.unit_normal(face.id).unwrap()[1] > 0.9)
            .unwrap()
            .id;
        let neighbor_positions = raised_body.face_positions(neighbor_id).unwrap();
        let neighbor_y = neighbor_positions.iter().map(|position| position[1]).sum::<f64>() / neighbor_positions.len() as f64;
        assert!((cap_y - neighbor_y - 0.4).abs() < 1.0e-6, "{cap_y} {neighbor_y}");
        assert_eq!(raised_body.face_loop(neighbor_id).unwrap().iter().filter(|vertex| cap_loop.contains(vertex)).count(), 0);
        let side = raised_body
            .faces
            .iter()
            .find(|face| {
                let cap_loop = raised_body.face_loop(cap).unwrap();
                let shared = face.vertices.iter().filter(|vertex| cap_loop.contains(vertex)).count();
                face.id != cap && shared == 2 && raised_body.unit_normal(face.id).unwrap()[1].abs() < 0.2
            })
            .unwrap()
            .id;
        let normal = raised_body.unit_normal(side).unwrap();
        assert_eq!(
            world.extrude_block_faces(id, &[side], [normal[0] * 0.4, normal[1] * 0.4, normal[2] * 0.4]).unwrap(),
            crate::AuthoringResult::Applied
        );
        let edited = world.authored_block(id).unwrap();
        let face_count = edited.body.as_ref().unwrap().faces.len();
        let vertex_ids: Vec<u32> = edited.body.as_ref().unwrap().vertices.iter().map(|vertex| vertex.id).collect();
        let edge_ids: Vec<u32> = edited.body.as_ref().unwrap().edges.iter().map(|edge| edge.id).collect();
        let face_ids: Vec<u32> = edited.body.as_ref().unwrap().faces.iter().map(|face| face.id).collect();
        assert!(face_ids.contains(&cap) && face_ids.contains(&side));
        assert_eq!(edited.history.iter().filter(|op| matches!(op, BlockOp::ExtrudeFaces { .. })).count(), 2);
        let mesh_id = world.object_mesh(id).unwrap();
        let mesh = world.meshes().get(mesh_id).unwrap();
        let set = world.derived_meshlets(mesh_id).unwrap();
        let coverage = crate::meshlet_coverage(mesh, set);
        assert_eq!(coverage.missing_triangles, 0);
        assert_eq!(coverage.duplicate_triangles, 0);
        assert!(coverage.canonical_triangles >= 12);
        let json = LevelDocument::capture(&world, document.level_uuid, "Extrude").unwrap().to_json();
        assert!(json.contains("\"body\"") && json.contains("extrude-faces") && json.contains("subdivide-face"));
        let loaded = parse_level(&json).unwrap().instantiate().unwrap();
        let again = loaded.authored_block(id).unwrap();
        let loaded_body = again.body.unwrap();
        loaded_body.validate().unwrap();
        assert_eq!(loaded_body.faces.len(), face_count);
        assert_eq!(loaded_body.vertices.iter().map(|vertex| vertex.id).collect::<Vec<_>>(), vertex_ids);
        assert_eq!(loaded_body.edges.iter().map(|edge| edge.id).collect::<Vec<_>>(), edge_ids);
        assert_eq!(loaded_body.faces.iter().map(|face| face.id).collect::<Vec<_>>(), face_ids);
        assert_eq!(again.history.len(), edited.history.len());
        assert!(again.history.iter().any(|op| matches!(op, BlockOp::SubdivideFace { face: 3, u: 4, v: 4 })));
        assert_eq!(again.history.iter().filter(|op| matches!(op, BlockOp::ExtrudeFaces { .. })).count(), 2);
        let reloaded_mesh = loaded.object_mesh(id).unwrap();
        let reloaded_set = loaded.derived_meshlets(reloaded_mesh).unwrap();
        let reloaded_coverage = crate::meshlet_coverage(loaded.meshes().get(reloaded_mesh).unwrap(), reloaded_set);
        assert_eq!(reloaded_coverage.missing_triangles, 0);
        assert_eq!(reloaded_coverage.duplicate_triangles, 0);
        assert_eq!(reloaded_coverage.canonical_triangles, coverage.canonical_triangles);
        let mesh_ref = loaded.meshes().get(reloaded_mesh).unwrap();
        let facts = crate::SurfaceDetailFacts {
            opaque: true,
            skinned: false,
            ui: false,
            particle: false,
            bounds_min: mesh_ref.bounds().aabb.min,
            bounds_max: mesh_ref.bounds().aabb.max,
            exact: true,
        };
        assert!(crate::microgeometry_active(crate::MicrogeometryMode::Auto, &facts));
        let clusters = vec![
            crate::DetailCluster { flag: 1, projected_px: 5.0, compatible: true, has_anchor: true, exact: true };
            reloaded_set.meshlets.len()
        ];
        let reasons = crate::select_detail_clusters(&clusters, 256);
        assert!(reasons.iter().all(|reason| *reason == crate::DetailReject::Exact));
        assert_eq!(reasons.iter().filter(|reason| **reason == crate::DetailReject::Selected).count(), 0);
    }

    #[test]
    fn sizing_a_body_scales_about_the_center_and_keeps_ids() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::ZERO, BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        world.split_block_edge(id, 12).unwrap();
        assert_eq!(world.set_block_extent(id, 0, 4.0).unwrap(), crate::AuthoringResult::Applied);
        let record = world.authored_block(id).unwrap();
        assert!((record.size_m[0] - 4.0).abs() < 1.0e-6, "{:?}", record.size_m);
        let body = record.body.unwrap();
        assert!(body.edges.iter().any(|edge| edge.id == 12));
        assert!((body.vertex_position(2).unwrap()[0] - 2.0).abs() < 1.0e-6);
        let pose = world.authored_local_pose(id).unwrap().0;
        assert!(pose.x.abs() < 1.0e-6 && pose.y.abs() < 1.0e-6, "{pose:?}");
    }

    #[test]
    fn an_invalid_body_preview_leaves_the_record_alone() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, 0.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut bad = crate::SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        bad.vertices.clear();
        assert!(world.preview_block_body(id, bad, Vec3::new(0.0, 1.0, 0.0)).is_err());
        assert!(world.authored_block(id).unwrap().body.is_none());
        assert_eq!(world.authored_block(id).unwrap().size_m, [2.0, 2.0, 2.0]);
        let baked = crate::SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        world.preview_block_body(id, baked, Vec3::new(0.0, 1.0, 0.0)).unwrap();
        let mesh = world.object_mesh(id).unwrap();
        assert_eq!(world.meshes().get(mesh).unwrap().triangle_indices().len(), 12);
    }
}
