//! Experiment 5. The saved analytic chart is the intent record.
//!
//! The mesh is built after a parse. This module does not call `realize_direct`,
//! `AnalyticObject::specimen`, or `intent_observation`.

use std::path::{Path, PathBuf};

use crate::mesh::{
    create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc,
};
use crate::microgeometry::projected_detail_px;
use crate::{BlockRecord, IntentEntry, IntentPayload};

pub const ANALYTIC_INTENT_ALGORITHM_VERSION: u32 = 1;
pub const ANALYTIC_CHART: &str = "p(u,v)=(u,v,1)/sqrt(u*u+v*v+1)";
pub const SURFACE_ONE: &str = "S:spherical-square/1";
pub const SURFACE_TWO: &str = "S:spherical-square/2";
pub const EDITED_RADIUS_M: f64 = 1.25;
pub const NOT_AUTHORITY_3C: u64 = 0xac80_f43b_acf3_c609;
pub const NOT_AUTHORITY_EXPERIMENT4: u64 = 0xfbdd_144e_5cd4_b21f;
pub const NOT_AUTHORITY_EXPERIMENT4_EDIT: u64 = 0xd5f4_177d_cd12_550c;

const ANALYTIC_DOMAIN: [f64; 2] = [-0.5, 0.5];
const N_CAP: u32 = 64;
const ERROR_SLACK_PX: f32 = 0.05;
const VERTEX_STRIDE: u32 = 60;
const PROJECT_UUID: &str = "a5a5a5a5-a5a5-45a5-85a5-a5a5a5a5a5a5";
const LEVEL_UUID: &str = "a5a5a5a5-a5a5-45a5-85a5-a5a5a5a5a5a6";
const SETTINGS_UUID: &str = "a5a5a5a5-a5a5-45a5-85a5-a5a5a5a5a5a7";
const SOLID_UUID: &str = "a5a5a5a5-a5a5-45a5-85a5-a5a5a5a5a5a8";

#[derive(Clone, Copy, Debug)]
pub struct AnalyticCamera {
    pub eye: [f64; 3],
    pub forward: [f64; 3],
    pub up: [f64; 3],
    pub vertical_fov_radians: f64,
    pub near_m: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticAdmission {
    Omit,
    AdmitInView,
    AdmitBehindCamera,
    AdmitCrossesNear,
    AdmitUncertain,
}

impl AnalyticAdmission {
    pub fn name(self) -> &'static str {
        match self {
            Self::Omit => "Omit",
            Self::AdmitInView => "AdmitInView",
            Self::AdmitBehindCamera => "AdmitBehindCamera",
            Self::AdmitCrossesNear => "AdmitCrossesNear",
            Self::AdmitUncertain => "AdmitUncertain",
        }
    }

    fn uncertain(self) -> bool {
        matches!(self, Self::AdmitBehindCamera | Self::AdmitCrossesNear | Self::AdmitUncertain)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalyticAccount {
    pub identity: String,
    pub surface_id: u32,
    pub admission: AnalyticAdmission,
    pub n: u32,
    pub triangles_constructed: u32,
    pub candidate_grids_emitted: u32,
    pub candidate_grids_discarded: u32,
    pub resolution_predicate_evaluations: u32,
    pub coarser_failed: bool,
    pub coarser_px: f32,
    pub measured_error_px: f32,
    pub packed_triangles: u32,
    pub packed_vertices: u32,
    pub index_count: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub gpu_bytes: u64,
    pub position_hash: u64,
    pub index_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyticTraceRecord {
    pub identity: String,
    pub surface_id: u32,
    pub cell_i: u32,
    pub cell_j: u32,
    pub triangle_in_cell: u8,
}

#[derive(Clone, Debug)]
pub struct AnalyticProduct {
    pub authority_hash: u64,
    pub revision: u64,
    pub chart: String,
    pub cache_key: String,
    pub surfaces: [AnalyticAccount; 2],
    pub trace: Vec<AnalyticTraceRecord>,
    pub mesh: Mesh,
    pub constructed_triangles: u32,
    pub packed_triangles: u32,
    pub uploaded_triangles: u32,
    pub discarded_after_construction: u32,
    pub discarded_during_pack: u32,
    pub discarded_after_upload: u32,
    pub packed_vertices: u32,
    pub packed_indices: u32,
    pub packed_vertex_bytes: u64,
    pub packed_index_bytes: u64,
    pub expected_gpu_bytes: u64,
    pub cpu_bytes: u64,
    pub index_format: MeshIndexFormat,
    pub position_hash: u64,
    pub index_hash: u64,
    pub meshlets_constructed: u32,
    pub vertices_welded: bool,
    pub rejected_n_allocated_mesh: bool,
    pub object_mesh_consulted: bool,
    pub record_body_consulted: bool,
    pub realize_direct_consulted: bool,
    pub analytic_specimen_consulted: bool,
    pub authoritative_triangles_consulted: bool,
    pub build_realization_consulted: bool,
    pub einstein_consulted: bool,
}

struct ChartSurface {
    identity: String,
    chart: String,
    domain_u: [f64; 2],
    domain_v: [f64; 2],
    radius_m: f64,
    translation_m: [f64; 3],
    surface_id: u32,
}

struct Chosen {
    n: u32,
    evaluations: u32,
    coarser_failed: bool,
    coarser_px: f32,
}

struct ScalarPredicate {
    max_px: f32,
}

pub fn analytic_proof_dir() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Jeramiah\AppData\Local"));
    root.join("Temp").join("jarvig-analytic-intent")
}

pub fn far_analytic_camera() -> AnalyticCamera {
    analytic_camera([0.0, 0.0, 8.0], 160.0)
}

pub fn close_analytic_camera() -> AnalyticCamera {
    analytic_camera([0.0, 0.0, 2.2], 960.0)
}

pub fn analytic_authority_hash(record: &BlockRecord) -> Result<u64, String> {
    Ok(fnv64(authority_text(record)?.as_bytes()))
}

pub fn replace_surface_radius(record: &BlockRecord, identity: &str, radius_m: f64) -> Result<BlockRecord, String> {
    if record.body.is_some() {
        return Err("the radius edit would keep a stored body".into());
    }
    if !radius_m.is_finite() || radius_m <= 0.0 {
        return Err("the edited radius is not a positive finite length".into());
    }
    let mut edited = record.clone();
    let mut found = false;
    for entry in &mut edited.intent {
        if let IntentPayload::AnalyticSurface { identity: name, radius_m: slot, .. } = &mut entry.payload {
            if name == identity {
                *slot = radius_m;
                found = true;
            }
        }
    }
    if !found {
        return Err(format!("the saved record has no {identity}"));
    }
    Ok(edited)
}

/// Admit or omit from the parsed chart, then emit the first passing grid once.
pub fn realize_saved_chart(record: &BlockRecord, camera: &AnalyticCamera, revision: u64) -> Result<AnalyticProduct, String> {
    if revision == 0 {
        return Err("an analytic realization has no revision".into());
    }
    if record.body.is_some() {
        return Err("the saved analytic surface has a stored tessellated body".into());
    }
    let charts = saved_charts(record)?;
    let authority_hash = analytic_authority_hash(record)?;
    if authority_hash == NOT_AUTHORITY_3C || authority_hash == NOT_AUTHORITY_EXPERIMENT4 || authority_hash == NOT_AUTHORITY_EXPERIMENT4_EDIT {
        return Err("the analytic authority hash collided with an earlier experiment".into());
    }
    let mut accounts = [zero_account(&charts[0]), zero_account(&charts[1])];
    let mut trace = Vec::new();
    let mut corners = Vec::new();
    for (index, surface) in charts.iter().enumerate() {
        let admission = classify_chart(surface, camera);
        if admission.uncertain() {
            return Err(format!("{} is {}; this chart does not construct a grid", surface.identity, admission.name()));
        }
        if admission == AnalyticAdmission::Omit {
            accounts[index].admission = AnalyticAdmission::Omit;
            continue;
        }
        if admission != AnalyticAdmission::AdmitInView {
            return Err(format!("{} admission {} is not a direct decision", surface.identity, admission.name()));
        }
        let chosen = choose_n(surface, camera)?;
        let measured = predicate_max_px(surface, chosen.n, camera)?;
        let limit = camera.requested_error_px + ERROR_SLACK_PX;
        if !measured.max_px.is_finite() || measured.max_px > limit {
            return Err(format!("{} measured {:.6} px after emit, above {:.6}", surface.identity, measured.max_px, limit));
        }
        if chosen.n > 1 && (!chosen.coarser_failed || chosen.coarser_px <= camera.requested_error_px) {
            return Err(format!("{} emitted n {} without a failing coarser grid", surface.identity, chosen.n));
        }
        let (mut records, mut grid) = emit_grid(surface, chosen.n)?;
        let triangles = records.len() as u32;
        if triangles != chosen.n.saturating_mul(chosen.n).saturating_mul(2) {
            return Err(format!("{} emitted {triangles} triangles for n {}", surface.identity, chosen.n));
        }
        accounts[index] = AnalyticAccount {
            identity: surface.identity.clone(),
            surface_id: surface.surface_id,
            admission,
            n: chosen.n,
            triangles_constructed: triangles,
            candidate_grids_emitted: 1,
            candidate_grids_discarded: 0,
            resolution_predicate_evaluations: chosen.evaluations,
            coarser_failed: chosen.coarser_failed,
            coarser_px: chosen.coarser_px,
            measured_error_px: measured.max_px,
            packed_triangles: 0,
            packed_vertices: 0,
            index_count: 0,
            vertex_bytes: 0,
            index_bytes: 0,
            gpu_bytes: 0,
            position_hash: 0,
            index_hash: 0,
        };
        trace.append(&mut records);
        corners.append(&mut grid);
    }
    if trace.iter().any(|record| accounts.iter().any(|account| account.identity == record.identity && account.admission == AnalyticAdmission::Omit)) {
        return Err("an omitted surface entered the construction trace".into());
    }
    let packed = pack_chart(&trace, &corners)?;
    for account in &mut accounts {
        if account.admission == AnalyticAdmission::Omit {
            continue;
        }
        account.packed_triangles = packed.packed_triangles;
        account.packed_vertices = packed.packed_vertices;
        account.index_count = packed.packed_indices;
        account.vertex_bytes = packed.packed_vertex_bytes;
        account.index_bytes = packed.packed_index_bytes;
        account.gpu_bytes = packed.expected_gpu_bytes;
        account.position_hash = packed.position_hash;
        account.index_hash = packed.index_hash;
    }
    if packed.discarded_during_pack != 0 || packed.constructed_triangles != packed.packed_triangles {
        return Err("the analytic pack discarded or split triangles".into());
    }
    Ok(AnalyticProduct {
        cache_key: cache_key(camera, authority_hash, revision),
        authority_hash,
        revision,
        chart: charts[0].chart.clone(),
        surfaces: accounts,
        constructed_triangles: packed.constructed_triangles,
        packed_triangles: packed.packed_triangles,
        uploaded_triangles: packed.packed_triangles,
        discarded_after_construction: 0,
        discarded_during_pack: packed.discarded_during_pack,
        discarded_after_upload: 0,
        packed_vertices: packed.packed_vertices,
        packed_indices: packed.packed_indices,
        packed_vertex_bytes: packed.packed_vertex_bytes,
        packed_index_bytes: packed.packed_index_bytes,
        expected_gpu_bytes: packed.expected_gpu_bytes,
        cpu_bytes: packed.packed_vertex_bytes + packed.packed_index_bytes,
        index_format: packed.index_format,
        position_hash: packed.position_hash,
        index_hash: packed.index_hash,
        meshlets_constructed: 0,
        vertices_welded: false,
        rejected_n_allocated_mesh: false,
        object_mesh_consulted: false,
        record_body_consulted: false,
        realize_direct_consulted: false,
        analytic_specimen_consulted: false,
        authoritative_triangles_consulted: false,
        build_realization_consulted: false,
        einstein_consulted: false,
        mesh: packed.mesh,
        trace,
    })
}

pub fn format_analytic_product(label: &str, product: &AnalyticProduct) -> String {
    let mut out = String::new();
    out.push_str(&format!("{label}\n"));
    out.push_str(&format!("authority_hash: {:016x}\n", product.authority_hash));
    out.push_str(&format!("authority_revision: {}\n", product.revision));
    out.push_str(&format!("chart: {}\n", product.chart));
    out.push_str(&format!("cache_key: {}\n", product.cache_key));
    out.push_str(&format!("ANALYTIC_INTENT_ALGORITHM_VERSION: {ANALYTIC_INTENT_ALGORITHM_VERSION}\n"));
    for account in &product.surfaces {
        out.push_str(&format!("surface {} {} admission: {}\n", account.surface_id, account.identity, account.admission.name()));
        out.push_str(&format!("surface {} n: {}\n", account.identity, account.n));
        out.push_str(&format!("surface {} triangles_constructed: {}\n", account.identity, account.triangles_constructed));
        out.push_str(&format!("surface {} measured_projected_error_px: {:.6}\n", account.identity, account.measured_error_px));
        out.push_str(&format!("surface {} coarser_predicate_failed: {}\n", account.identity, yes(account.coarser_failed)));
        out.push_str(&format!("surface {} coarser_px: {:.6}\n", account.identity, account.coarser_px));
        out.push_str(&format!("surface {} gpu_bytes: {}\n", account.identity, account.gpu_bytes));
        out.push_str(&format!("surface {} position_hash: {:016x}\n", account.identity, account.position_hash));
        out.push_str(&format!("surface {} index_hash: {:016x}\n", account.identity, account.index_hash));
    }
    out.push_str(&format!("triangles_directly_constructed: {}\n", product.constructed_triangles));
    out.push_str(&format!("packed_triangles: {}\n", product.packed_triangles));
    out.push_str(&format!("uploaded_triangles: {}\n", product.uploaded_triangles));
    out.push_str(&format!("expected_gpu_bytes: {}\n", product.expected_gpu_bytes));
    out.push_str(&format!("position_hash: {:016x}\n", product.position_hash));
    out.push_str(&format!("index_hash: {:016x}\n", product.index_hash));
    out.push_str(&format!("triangles_discarded_after_construction: {}\n", product.discarded_after_construction));
    out.push_str(&format!("triangles_dropped_during_pack: {}\n", product.discarded_during_pack));
    out.push_str(&format!("triangles_discarded_after_upload: {}\n", product.discarded_after_upload));
    out.push_str("rejected n allocated a mesh: NO\n");
    out.push_str(&format!("meshlets_constructed: {}\n", product.meshlets_constructed));
    out.push_str(&format!("construction_trace_contains_surface_2: {}\n", yes(product.trace.iter().any(|record| record.identity == SURFACE_TWO))));
    out
}

pub fn write_analytic_intent_project(directory: &Path) -> Result<PathBuf, String> {
    let project_file = directory.join("AnalyticIntent.jarvigproject");
    let project = crate::ProjectDocument {
        format_version: crate::PROJECT_FORMAT_VERSION,
        project_uuid: crate::EntityId::parse(PROJECT_UUID).ok_or("analytic project uuid")?,
        display_name: "Analytic Intent".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        startup_level: "Content/Levels/Analytic.jarviglevel".into(),
        content_directory: "Content".into(),
        saved_directory: "Saved".into(),
        config_directory: "Config".into(),
        intermediate_directory: "Intermediate".into(),
        settings: "Config/Project.jarvigsettings".into(),
    };
    crate::create_project_directories(&project_file, &project).map_err(|error| error.to_string())?;
    let backup = directory.join("Saved").join("Backup");
    crate::save_project_atomic(&project_file, &backup, &project).map_err(|error| error.to_string())?;
    let level = analytic_level(fixture_record(1.0)?)?;
    crate::save_level_atomic(&project.startup_level_path(&project_file).map_err(|error| error.to_string())?, &backup, &level).map_err(|error| error.to_string())?;
    let settings = directory.join(&project.settings);
    if let Some(parent) = settings.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(settings, "JARVIG project settings placeholder. This is not a level and not a GPU resource.\n").map_err(|error| error.to_string())?;
    Ok(project_file)
}

fn analytic_level(block: BlockRecord) -> Result<crate::LevelDocument, String> {
    let settings = crate::EntityId::parse(SETTINGS_UUID).ok_or("settings uuid")?;
    let solid = crate::EntityId::parse(SOLID_UUID).ok_or("solid uuid")?;
    Ok(crate::LevelDocument {
        format_version: crate::LEVEL_BLOCK_VERSION,
        level_uuid: crate::EntityId::parse(LEVEL_UUID).ok_or("level uuid")?,
        name: "Analytic Intent".into(),
        world_settings: crate::WorldSettingsRecord {
            entity: settings,
            enabled: true,
            intensity: crate::BOOTSTRAP_ENVIRONMENT_INTENSITY,
            upper: crate::BOOTSTRAP_UPPER_HEMISPHERE_LINEAR,
            lower: crate::BOOTSTRAP_LOWER_HEMISPHERE_LINEAR,
            probe_update_policy: crate::ProbeUpdatePolicy::Static,
            startup_camera: None,
        },
        organization: crate::SceneOrganization::default(),
        entities: vec![
            crate::EntityRecord {
                uuid: settings,
                name: "World Settings".into(),
                parent_uuid: None,
                components: vec![crate::ComponentRecord::WorldSettings],
            },
            crate::EntityRecord {
                uuid: solid,
                name: "Analytic Solid".into(),
                parent_uuid: None,
                components: vec![
                    crate::ComponentRecord::Transform {
                        translation: crate::Vec3::new(0.0, 1.0, -4.0),
                        rotation: crate::Quat::IDENTITY,
                        scale: crate::Vec3::new(1.0, 1.0, 1.0),
                    },
                    crate::ComponentRecord::ParametricBlock(block),
                ],
            },
        ],
    })
}

fn fixture_record(radius_one: f64) -> Result<BlockRecord, String> {
    let mut record = BlockRecord::standard([2.0, 2.0, 2.0]).map_err(|error| error.to_string())?;
    record.seed_size_m = None;
    record.intent = vec![
        surface_entry(SURFACE_ONE, radius_one, [0.0, 0.0, 0.0])?,
        surface_entry(SURFACE_TWO, 1.0, [3.0, 0.0, 0.0])?,
    ];
    Ok(record)
}

fn surface_entry(identity: &str, radius_m: f64, translation_m: [f64; 3]) -> Result<IntentEntry, String> {
    Ok(IntentEntry {
        groups: None,
        payload: IntentPayload::AnalyticSurface {
            identity: identity.to_string(),
            chart: ANALYTIC_CHART.to_string(),
            domain_u: ANALYTIC_DOMAIN,
            domain_v: ANALYTIC_DOMAIN,
            radius_m,
            translation_m,
        },
    })
}

fn saved_charts(record: &BlockRecord) -> Result<[ChartSurface; 2], String> {
    if record.intent.len() != 2 {
        return Err(format!("the analytic record has {} intent entries, not 2", record.intent.len()));
    }
    let mut charts = Vec::new();
    for entry in &record.intent {
        charts.push(accept_chart(entry)?);
    }
    if charts[0].identity != SURFACE_ONE || charts[1].identity != SURFACE_TWO {
        return Err("the saved tape is not surface 1 then surface 2".into());
    }
    Ok([charts.remove(0), charts.remove(0)])
}

fn accept_chart(entry: &IntentEntry) -> Result<ChartSurface, String> {
    if entry.groups.is_some() {
        return Err("analytic-surface identity is not a face group".into());
    }
    let IntentPayload::AnalyticSurface { identity, chart, domain_u, domain_v, radius_m, translation_m } = &entry.payload else {
        return Err("the analytic tape contains an operation other than analytic-surface".into());
    };
    if chart.as_str() != ANALYTIC_CHART {
        return Err("the persisted chart spelling does not match p(u,v)=(u,v,1)/sqrt(u*u+v*v+1)".into());
    }
    if domain_u[0].to_bits() != (-0.5f64).to_bits()
        || domain_u[1].to_bits() != 0.5f64.to_bits()
        || domain_v[0].to_bits() != (-0.5f64).to_bits()
        || domain_v[1].to_bits() != 0.5f64.to_bits()
    {
        return Err("the persisted domain is not [-0.5, 0.5]".into());
    }
    if !radius_m.is_finite() || *radius_m <= 0.0 || translation_m.iter().any(|axis| !axis.is_finite()) {
        return Err("the persisted radius or translation is not finite".into());
    }
    let surface_id = if identity == SURFACE_ONE {
        1
    } else if identity == SURFACE_TWO {
        2
    } else {
        return Err(format!("the saved chart identity {identity} is not one of the two spherical squares"));
    };
    Ok(ChartSurface {
        identity: identity.clone(),
        chart: chart.clone(),
        domain_u: *domain_u,
        domain_v: *domain_v,
        radius_m: *radius_m,
        translation_m: *translation_m,
        surface_id,
    })
}

fn authority_text(record: &BlockRecord) -> Result<String, String> {
    let charts = saved_charts(record)?;
    let mut text = String::from("analytic-intent-v1\n");
    for surface in &charts {
        text.push_str(&format!("identity {}\n", surface.identity));
        text.push_str(&format!("chart {}\n", surface.chart));
        text.push_str(&format!("domain_u {} {}\n", bits(surface.domain_u[0]), bits(surface.domain_u[1])));
        text.push_str(&format!("domain_v {} {}\n", bits(surface.domain_v[0]), bits(surface.domain_v[1])));
        text.push_str(&format!(
            "translation {} {} {}\n",
            bits(surface.translation_m[0]),
            bits(surface.translation_m[1]),
            bits(surface.translation_m[2])
        ));
        text.push_str(&format!("radius {}\n", bits(surface.radius_m)));
    }
    Ok(text)
}

fn analytic_camera(eye: [f64; 3], width: f32) -> AnalyticCamera {
    AnalyticCamera {
        eye,
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        vertical_fov_radians: 60.0_f64.to_radians(),
        near_m: 0.1,
        viewport_width: width,
        viewport_height: 567.0,
        requested_error_px: 0.5,
    }
}

fn classify_chart(surface: &ChartSurface, camera: &AnalyticCamera) -> AnalyticAdmission {
    let Ok(corners) = patch_corners(surface) else {
        return AnalyticAdmission::AdmitUncertain;
    };
    let Some((right, up, forward)) = camera_basis(camera.forward, camera.up) else {
        return AnalyticAdmission::AdmitUncertain;
    };
    let width = f64::from(camera.viewport_width);
    let height = f64::from(camera.viewport_height);
    let error = f64::from(camera.requested_error_px);
    if !camera.near_m.is_finite() || camera.near_m <= 0.0 || !error.is_finite() || error < 0.0 || width < 1.0 || height < 1.0 {
        return AnalyticAdmission::AdmitUncertain;
    }
    let half = camera.vertical_fov_radians * 0.5;
    if !half.is_finite() || half <= 0.0 || half >= std::f64::consts::FRAC_PI_2 {
        return AnalyticAdmission::AdmitUncertain;
    }
    let tan_half_v = half.tan();
    let tan_half_h = tan_half_v * (width / height);
    if !tan_half_v.is_finite() || !tan_half_h.is_finite() || tan_half_v <= 0.0 || tan_half_h <= 0.0 {
        return AnalyticAdmission::AdmitUncertain;
    }
    let mut depths = [0.0; 8];
    for (index, corner) in corners.iter().enumerate() {
        depths[index] = dot(sub(*corner, camera.eye), forward);
    }
    if depths.iter().any(|depth| !depth.is_finite()) {
        return AnalyticAdmission::AdmitUncertain;
    }
    if depths.iter().any(|depth| *depth <= 0.0) {
        return AnalyticAdmission::AdmitBehindCamera;
    }
    if depths.iter().any(|depth| *depth <= camera.near_m) {
        return AnalyticAdmission::AdmitCrossesNear;
    }
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for (corner, depth) in corners.iter().zip(depths.iter()) {
        let relative = sub(*corner, camera.eye);
        let px = (dot(relative, right) / depth) / tan_half_h;
        let py = (dot(relative, up) / depth) / tan_half_v;
        if !px.is_finite() || !py.is_finite() {
            return AnalyticAdmission::AdmitUncertain;
        }
        let screen_x = (px * 0.5 + 0.5) * width;
        let screen_y = (py * 0.5 + 0.5) * height;
        min_x = min_x.min(screen_x);
        max_x = max_x.max(screen_x);
        min_y = min_y.min(screen_y);
        max_y = max_y.max(screen_y);
    }
    let misses = max_x < -error || min_x > width + error || max_y < -error || min_y > height + error;
    if misses { AnalyticAdmission::Omit } else { AnalyticAdmission::AdmitInView }
}

fn predicate_max_px(surface: &ChartSurface, n: u32, camera: &AnalyticCamera) -> Result<ScalarPredicate, String> {
    if n == 0 || n > N_CAP {
        return Err(format!("n {n} is outside 1..={N_CAP}"));
    }
    let Some((_, _, forward)) = camera_basis(camera.forward, camera.up) else {
        return Err("the analytic camera basis is degenerate".into());
    };
    let tan_half = ((camera.vertical_fov_radians * 0.5) as f32).tan();
    if !tan_half.is_finite() || tan_half <= 0.0 || !camera.requested_error_px.is_finite() {
        return Err("the analytic camera field of view is not usable".into());
    }
    let mut max_px = 0.0f32;
    let mut samples = 0u32;
    for j in 0..n {
        for i in 0..n {
            for which in [0u8, 1] {
                let triangle = triangle_corners(surface, n, i, j, which)?;
                let centroid = parameter_centroid(surface, n, i, j, which)?;
                let point = place(surface, centroid.0, centroid.1)?;
                let depth = dot(sub(point, camera.eye), forward);
                if !depth.is_finite() {
                    return Err(format!("{} sample depth was not finite", surface.identity));
                }
                if depth < 0.05 {
                    return Err(format!("{} sample depth {depth} m is under 0.05 m", surface.identity));
                }
                let Some(deviation) = plane_distance(point, triangle[0], triangle[1], triangle[2]) else {
                    max_px = f32::MAX;
                    samples = samples.saturating_add(1);
                    continue;
                };
                if !deviation.is_finite() {
                    max_px = f32::MAX;
                    samples = samples.saturating_add(1);
                    continue;
                }
                let px = projected_detail_px(deviation as f32, depth as f32, camera.viewport_height, tan_half);
                if !px.is_finite() {
                    max_px = f32::MAX;
                } else {
                    max_px = max_px.max(px);
                }
                samples = samples.saturating_add(1);
            }
        }
    }
    let expected = n.checked_mul(n).and_then(|value| value.checked_mul(2)).ok_or("predicate sample count overflowed")?;
    if samples != expected {
        return Err(format!("predicate sampled {samples} centroids, not {expected}"));
    }
    Ok(ScalarPredicate { max_px })
}

fn choose_n(surface: &ChartSurface, camera: &AnalyticCamera) -> Result<Chosen, String> {
    let mut n = 1u32;
    let mut evaluations = 0u32;
    let mut coarser_failed = false;
    let mut coarser_px = 0.0f32;
    loop {
        if n > N_CAP {
            return Err(format!("{} stayed above {:.3} px through n = {N_CAP}", surface.identity, camera.requested_error_px));
        }
        let scalar = predicate_max_px(surface, n, camera)?;
        evaluations = evaluations.saturating_add(1);
        if scalar.max_px <= camera.requested_error_px {
            return Ok(Chosen { n, evaluations, coarser_failed, coarser_px });
        }
        coarser_failed = true;
        coarser_px = scalar.max_px;
        n = n.saturating_add(1);
    }
}

fn emit_grid(surface: &ChartSurface, n: u32) -> Result<(Vec<AnalyticTraceRecord>, Vec<[[f64; 3]; 3]>), String> {
    let mut records = Vec::new();
    let mut corners = Vec::new();
    for j in 0..n {
        for i in 0..n {
            for which in [0u8, 1] {
                let triangle = triangle_corners(surface, n, i, j, which)?;
                records.push(AnalyticTraceRecord {
                    identity: surface.identity.clone(),
                    surface_id: surface.surface_id,
                    cell_i: i,
                    cell_j: j,
                    triangle_in_cell: which,
                });
                corners.push(triangle);
            }
        }
    }
    Ok((records, corners))
}

struct PackedChart {
    mesh: Mesh,
    constructed_triangles: u32,
    packed_triangles: u32,
    packed_vertices: u32,
    packed_indices: u32,
    packed_vertex_bytes: u64,
    packed_index_bytes: u64,
    expected_gpu_bytes: u64,
    index_format: MeshIndexFormat,
    position_hash: u64,
    index_hash: u64,
    discarded_during_pack: u32,
}

fn pack_chart(records: &[AnalyticTraceRecord], corners: &[[[f64; 3]; 3]]) -> Result<PackedChart, String> {
    if records.is_empty() || records.len() != corners.len() {
        return Err("the analytic product constructed no triangles to pack".into());
    }
    let packed_triangles = u32::try_from(records.len()).map_err(|_| "too many triangles")?;
    let packed_vertices = packed_triangles.checked_mul(3).ok_or("vertex count overflowed")?;
    let packed_indices = packed_vertices;
    let index_format = if packed_vertices > u32::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    let packed_vertex_bytes = u64::from(packed_vertices) * u64::from(VERTEX_STRIDE);
    let packed_index_bytes = u64::from(packed_indices) * u64::from(index_format.byte_size());
    let mut bytes = Vec::with_capacity(packed_vertex_bytes as usize);
    let mut index_bytes = Vec::with_capacity(packed_index_bytes as usize);
    let mut position_bytes = Vec::with_capacity((packed_vertices as usize) * 12);
    for (triangle_index, (record, triangle)) in records.iter().zip(corners.iter()).enumerate() {
        if record.surface_id != 1 && record.surface_id != 2 {
            return Err(format!("trace names unknown surface {}", record.surface_id));
        }
        if record.triangle_in_cell > 1 {
            return Err("trace names a triangle outside the cell".into());
        }
        let packed = [f32_corner(triangle[0])?, f32_corner(triangle[1])?, f32_corner(triangle[2])?];
        let (normal, _) = packed_normal(packed[0], packed[1], packed[2]);
        let base = (triangle_index as u32).saturating_mul(3);
        for corner in packed {
            for value in corner {
                position_bytes.extend_from_slice(&value.to_le_bytes());
            }
            push_vertex(&mut bytes, corner, normal);
        }
        for offset in 0..3u32 {
            push_index(&mut index_bytes, index_format, base + offset);
        }
    }
    if bytes.len() as u64 != packed_vertex_bytes || index_bytes.len() as u64 != packed_index_bytes {
        return Err("packed byte lengths drifted from the triangle count".into());
    }
    let mesh = create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: VERTEX_STRIDE,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format,
        index_bytes: index_bytes.clone(),
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: packed_indices,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .map_err(|error: MeshError| error.to_string())?;
    Ok(PackedChart {
        mesh,
        constructed_triangles: packed_triangles,
        packed_triangles,
        packed_vertices,
        packed_indices,
        packed_vertex_bytes,
        packed_index_bytes,
        expected_gpu_bytes: packed_vertex_bytes + packed_index_bytes,
        index_format,
        position_hash: fnv64(&position_bytes),
        index_hash: fnv64(&index_bytes),
        discarded_during_pack: 0,
    })
}

fn zero_account(surface: &ChartSurface) -> AnalyticAccount {
    AnalyticAccount {
        identity: surface.identity.clone(),
        surface_id: surface.surface_id,
        admission: AnalyticAdmission::AdmitUncertain,
        n: 0,
        triangles_constructed: 0,
        candidate_grids_emitted: 0,
        candidate_grids_discarded: 0,
        resolution_predicate_evaluations: 0,
        coarser_failed: false,
        coarser_px: 0.0,
        measured_error_px: 0.0,
        packed_triangles: 0,
        packed_vertices: 0,
        index_count: 0,
        vertex_bytes: 0,
        index_bytes: 0,
        gpu_bytes: 0,
        position_hash: 0,
        index_hash: 0,
    }
}

fn cache_key(camera: &AnalyticCamera, hash: u64, revision: u64) -> String {
    let milli = (f64::from(camera.requested_error_px) * 1000.0).round() as i32;
    let near = (camera.near_m / 0.01).round() as i32;
    format!(
        "analytic5-h{hash:016x}-r{revision}-e{:.4}-{:.4}-{:.4}-f{:.0}-{:.0}-{:.0}-px{milli}-w{:.0}-h{:.0}-n{near}-a{ANALYTIC_INTENT_ALGORITHM_VERSION}",
        camera.eye[0], camera.eye[1], camera.eye[2], camera.forward[0], camera.forward[1], camera.forward[2], camera.viewport_width, camera.viewport_height,
    )
}

fn patch_corners(surface: &ChartSurface) -> Result<[[f64; 3]; 8], String> {
    let (lo_u, hi_u) = (surface.domain_u[0], surface.domain_u[1]);
    let (lo_v, hi_v) = (surface.domain_v[0], surface.domain_v[1]);
    let mid_u = (lo_u + hi_u) * 0.5;
    let mid_v = (lo_v + hi_v) * 0.5;
    let x_pos = place(surface, hi_u, mid_v)?;
    let x_neg = place(surface, lo_u, mid_v)?;
    let y_pos = place(surface, mid_u, hi_v)?;
    let y_neg = place(surface, mid_u, lo_v)?;
    let z_max_point = place(surface, mid_u, mid_v)?;
    let z00 = place(surface, lo_u, lo_v)?;
    let z01 = place(surface, lo_u, hi_v)?;
    let z10 = place(surface, hi_u, lo_v)?;
    let z11 = place(surface, hi_u, hi_v)?;
    let xmin = x_neg[0].min(x_pos[0]);
    let xmax = x_neg[0].max(x_pos[0]);
    let ymin = y_neg[1].min(y_pos[1]);
    let ymax = y_neg[1].max(y_pos[1]);
    let zmin = z00[2].min(z01[2]).min(z10[2]).min(z11[2]);
    let zmax = z_max_point[2];
    let mut corners = [[0.0; 3]; 8];
    let mut index = 0;
    for z in [zmin, zmax] {
        for y in [ymin, ymax] {
            for x in [xmin, xmax] {
                corners[index] = [x, y, z];
                index += 1;
            }
        }
    }
    Ok(corners)
}

fn parameter(domain: [f64; 2], n: u32, index: u32) -> f64 {
    domain[0] + f64::from(index) / f64::from(n) * (domain[1] - domain[0])
}

fn parameter_centroid(surface: &ChartSurface, n: u32, i: u32, j: u32, which: u8) -> Result<(f64, f64), String> {
    let uv = triangle_parameters(surface, n, i, j, which)?;
    Ok(((uv[0].0 + uv[1].0 + uv[2].0) / 3.0, (uv[0].1 + uv[1].1 + uv[2].1) / 3.0))
}

fn triangle_parameters(surface: &ChartSurface, n: u32, i: u32, j: u32, which: u8) -> Result<[(f64, f64); 3], String> {
    if i >= n || j >= n {
        return Err("cell is outside the grid".into());
    }
    let (a, b, c) = match which {
        0 => ((i, j), (i + 1, j), (i + 1, j + 1)),
        1 => ((i, j), (i + 1, j + 1), (i, j + 1)),
        _ => return Err("a cell has only two triangles".into()),
    };
    Ok([
        (parameter(surface.domain_u, n, a.0), parameter(surface.domain_v, n, a.1)),
        (parameter(surface.domain_u, n, b.0), parameter(surface.domain_v, n, b.1)),
        (parameter(surface.domain_u, n, c.0), parameter(surface.domain_v, n, c.1)),
    ])
}

fn triangle_corners(surface: &ChartSurface, n: u32, i: u32, j: u32, which: u8) -> Result<[[f64; 3]; 3], String> {
    let uv = triangle_parameters(surface, n, i, j, which)?;
    Ok([place(surface, uv[0].0, uv[0].1)?, place(surface, uv[1].0, uv[1].1)?, place(surface, uv[2].0, uv[2].1)?])
}

fn place(surface: &ChartSurface, u: f64, v: f64) -> Result<[f64; 3], String> {
    let divisor = (u * u + v * v + 1.0).sqrt();
    if !divisor.is_finite() || divisor <= 0.0 {
        return Err("spherical square radius was not finite".into());
    }
    let point = [
        surface.radius_m * (u / divisor) + surface.translation_m[0],
        surface.radius_m * (v / divisor) + surface.translation_m[1],
        surface.radius_m * (1.0 / divisor) + surface.translation_m[2],
    ];
    if point.iter().any(|axis| !axis.is_finite()) {
        return Err("spherical square point was not finite".into());
    }
    Ok(point)
}

fn plane_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let normal = cross(sub(b, a), sub(c, a));
    let length = (dot(normal, normal)).sqrt();
    if !length.is_finite() || length < 1.0e-18 {
        return None;
    }
    Some(dot(sub(point, a), normal).abs() / length)
}

fn f32_corner(corner: [f64; 3]) -> Result<[f32; 3], String> {
    let packed = [corner[0] as f32, corner[1] as f32, corner[2] as f32];
    if packed.iter().any(|axis| !axis.is_finite()) {
        return Err("a packed corner was not finite".into());
    }
    Ok(packed)
}

fn packed_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> ([f32; 3], bool) {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
    let length_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    if !length_sq.is_finite() || length_sq < 1.0e-16 {
        return ([0.0, 1.0, 0.0], true);
    }
    let inverse = 1.0 / length_sq.sqrt();
    ([normal[0] * inverse, normal[1] * inverse, normal[2] * inverse], false)
}

fn push_vertex(bytes: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3]) {
    for value in position.into_iter().chain([1.0, 1.0, 1.0]).chain([0.0, 0.0]).chain(normal).chain([1.0, 0.0, 0.0, 1.0]) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_index(bytes: &mut Vec<u8>, format: MeshIndexFormat, index: u32) {
    match format {
        MeshIndexFormat::Uint16 => bytes.extend_from_slice(&(index as u16).to_le_bytes()),
        MeshIndexFormat::Uint32 => bytes.extend_from_slice(&index.to_le_bytes()),
    }
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn yes(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn camera_basis(forward: [f64; 3], up_hint: [f64; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let forward = normalize(forward)?;
    let hint = match normalize(up_hint) {
        Some(hint) if dot(hint, forward).abs() <= 0.999 => hint,
        _ => {
            if forward[1].abs() < 0.9 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0, 0.0, 1.0]
            }
        }
    };
    let right = normalize(cross(forward, hint))?;
    let up = cross(right, forward);
    Some((right, up, forward))
}

fn normalize(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = (dot(value, value)).sqrt();
    if !length.is_finite() || length < 1.0e-12 {
        None
    } else {
        Some(scale(value, 1.0 / length))
    }
}

fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[1] * right[2] - left[2] * right[1], left[2] * right[0] - left[0] * right[2], left[0] * right[1] - left[1] * right[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAR_POSITION: u64 = 0xd483_cd5f_6133_8a61;
    const CLOSE_POSITION: u64 = 0x6e2e_84eb_1989_2f71;
    const FAR_INDEX: u64 = 0x0fef_7021_13eb_2f05;
    const CLOSE_INDEX: u64 = 0xd9f7_af58_ffd2_4f65;

    #[test]
    fn analytic_intent_reloads_the_chart_without_presenting() {
        println!("Frame: NOT PRESENTED");
        let plain = BlockRecord::standard([2.0, 2.0, 2.0]).expect("plain block");
        let plain_level = analytic_level(plain).expect("plain level");
        let plain_json = plain_level.to_json();
        assert!(plain_json.contains("\"version\": 1"), "a planar block left version 1");
        assert!(!plain_json.contains("\"version\": 2"));
        assert!(!plain_json.contains("analytic-surface"));
        let plain_parsed = crate::parse_level(&plain_json).expect("plain v1 reload");
        assert!(plain_parsed.entities.iter().any(|entity| entity.components.iter().any(|component| matches!(component, crate::ComponentRecord::ParametricBlock(block) if block.intent.is_empty()))));

        let directory = analytic_proof_dir();
        let project = write_analytic_intent_project(&directory).expect("proof project");
        let level_path = directory.join("Content").join("Levels").join("Analytic.jarviglevel");
        let text = std::fs::read_to_string(&level_path).expect("level text");
        assert!(text.contains(ANALYTIC_CHART));
        assert!(text.contains("\"version\": 2"));
        assert!(!text.contains("\"body\""));
        assert!(!text.contains("\"vertices\""));
        assert_eq!(crate::LEVEL_BLOCK_VERSION, 6);
        assert_eq!(crate::TYPE_REGISTRY_VERSION, 9);
        let document = crate::parse_level(&text).expect("v2 reload");
        assert_eq!(document.format_version, 6);
        let record = analytic_record(&document);
        assert!(record.body.is_none());
        assert!(record.history.is_empty());
        assert_eq!(record.intent.len(), 2);
        assert_eq!(chart_of(&record, SURFACE_ONE), ANALYTIC_CHART);
        assert_eq!(chart_of(&record, SURFACE_TWO), ANALYTIC_CHART);
        assert_eq!(radius_of(&record, SURFACE_ONE).to_bits(), 1.0f64.to_bits());
        assert_eq!(radius_of(&record, SURFACE_TWO).to_bits(), 1.0f64.to_bits());
        assert_eq!(translation_of(&record, SURFACE_TWO), [3.0, 0.0, 0.0]);
        let diagnostic = crate::intent_authority_diagnostic(&record);
        assert!(diagnostic.contains("Intent Authority: INELIGIBLE"), "{diagnostic}");
        assert!(!diagnostic.contains("Intent Authority: ELIGIBLE"), "{diagnostic}");
        let hash = analytic_authority_hash(&record).expect("hash");
        assert_ne!(hash, NOT_AUTHORITY_3C);
        assert_ne!(hash, NOT_AUTHORITY_EXPERIMENT4);
        assert_ne!(hash, NOT_AUTHORITY_EXPERIMENT4_EDIT);
        let far = realize_saved_chart(&record, &far_analytic_camera(), 1).expect("far");
        let close = realize_saved_chart(&record, &close_analytic_camera(), 1).expect("close");
        println!("{}", format_analytic_product("Observation Far", &far));
        println!("{}", format_analytic_product("Observation Close", &close));
        assert_eq!(far.chart, ANALYTIC_CHART);
        assert_eq!(close.chart, ANALYTIC_CHART);
        assert_eq!(far.authority_hash, hash);
        assert_eq!(far.revision, 1);
        assert!(far.cache_key.starts_with("analytic5-"));
        assert!(!far.cache_key.contains("intent4-") && !far.cache_key.contains("direct-"));
        assert_eq!(far.surfaces[1].admission, AnalyticAdmission::Omit);
        assert_eq!(close.surfaces[1].admission, AnalyticAdmission::Omit);
        assert_eq!(far.surfaces[1].gpu_bytes, 0);
        assert_eq!(close.surfaces[1].triangles_constructed, 0);
        assert!(!far.trace.iter().any(|record| record.identity == SURFACE_TWO));
        assert_surface(&far.surfaces[0], 6, 72, "0.424494", "0.613221", FAR_POSITION, FAR_INDEX, 13392);
        assert_surface(&close.surfaces[0], 14, 392, "0.462054", "0.536666", CLOSE_POSITION, CLOSE_INDEX, 72912);
        assert!(close.surfaces[0].n > far.surfaces[0].n);
        assert_ne!(far.expected_gpu_bytes, 74958);
        assert_eq!(far.expected_gpu_bytes, far.surfaces[0].gpu_bytes);
        assert!(!far.rejected_n_allocated_mesh);
        assert_eq!(far.discarded_after_construction, 0);
        assert_eq!(far.discarded_during_pack, 0);

        let edited = replace_surface_radius(&record, SURFACE_ONE, EDITED_RADIUS_M).expect("radius edit");
        let mut edited_document = document.clone();
        install_block(&mut edited_document, edited);
        let backup = directory.join("Saved").join("Backup");
        crate::save_level_atomic(&level_path, &backup, &edited_document).expect("save edit");
        let edited_text = std::fs::read_to_string(&level_path).expect("edited text");
        assert!(edited_text.contains(ANALYTIC_CHART));
        assert!(edited_text.contains("\"version\": 2"));
        assert!(!edited_text.contains("\"body\""));
        let reloaded = crate::parse_level(&edited_text).expect("edited reload");
        let edited_record = analytic_record(&reloaded);
        assert_eq!(radius_of(&edited_record, SURFACE_ONE).to_bits(), EDITED_RADIUS_M.to_bits());
        assert_eq!(radius_of(&edited_record, SURFACE_TWO).to_bits(), 1.0f64.to_bits());
        assert_eq!(chart_of(&edited_record, SURFACE_ONE), ANALYTIC_CHART);
        assert!(edited_record.history.is_empty());
        assert!(edited_record.body.is_none());
        assert_eq!(edited_record.intent.len(), 2);
        let edited_hash = analytic_authority_hash(&edited_record).expect("edited hash");
        assert_ne!(edited_hash, hash);
        let edited_far = realize_saved_chart(&edited_record, &far_analytic_camera(), 2).expect("edited far");
        let edited_close = realize_saved_chart(&edited_record, &close_analytic_camera(), 2).expect("edited close");
        println!("{}", format_analytic_product("Edited Observation Far", &edited_far));
        println!("{}", format_analytic_product("Edited Observation Close", &edited_close));
        assert_eq!(edited_far.revision, 2);
        assert_ne!(edited_far.position_hash, far.position_hash);
        assert_ne!(edited_close.position_hash, close.position_hash);
        assert_ne!(edited_far.position_hash, FAR_POSITION);
        assert_ne!(edited_close.position_hash, CLOSE_POSITION);
        assert_eq!(edited_far.surfaces[1].admission, AnalyticAdmission::Omit);
        assert_eq!(edited_close.surfaces[1].gpu_bytes, 0);
        assert_eq!(edited_far.chart, ANALYTIC_CHART);

        let downgraded = text.replace("\"version\": 2", "\"version\": 1");
        let downgrade_error = crate::parse_level(&downgraded).expect_err("v1 analytic").to_string();
        assert!(downgrade_error.contains("ParametricBlock version 1"), "{downgrade_error}");
        assert!(downgrade_error.contains("requires ParametricBlock version 2"), "{downgrade_error}");
        assert!(!downgrade_error.contains("unknown block intent operation"), "{downgrade_error}");
        let future = text.replace("\"version\": 2", "\"version\": 3");
        let future_error = crate::parse_level(&future).expect_err("version 3").to_string();
        assert_eq!(future_error, "ParametricBlock version 3 is not supported; this reader supports versions 1 and 2");
        let typo = text.replace("analytic-surface", "analytic-surf");
        let typo_error = crate::parse_level(&typo).expect_err("typo").to_string();
        assert!(typo_error.contains("unknown block intent operation analytic-surf"), "{typo_error}");

        write_analytic_intent_project(&directory).expect("restore radius 1 for the editor");
        let restored = std::fs::read_to_string(&level_path).expect("restored level");
        let restored_record = analytic_record(&crate::parse_level(&restored).expect("restored parse"));
        assert_eq!(radius_of(&restored_record, SURFACE_ONE).to_bits(), 1.0f64.to_bits());
        assert!(restored.contains("\"version\": 2"));
        let main = PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\Content\Levels\Main.jarviglevel");
        assert_eq!(std::fs::metadata(&main).expect("Main.jarviglevel").len(), 26849);
        assert!(project.ends_with("AnalyticIntent.jarvigproject"));
        println!("Frame: NOT PRESENTED");
    }

    fn analytic_record(document: &crate::LevelDocument) -> BlockRecord {
        document
            .entities
            .iter()
            .find(|entity| entity.name == "Analytic Solid")
            .and_then(|entity| entity.components.iter().find_map(|component| match component {
                crate::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                _ => None,
            }))
            .expect("Analytic Solid")
    }

    fn install_block(document: &mut crate::LevelDocument, block: BlockRecord) {
        let entity = document.entities.iter_mut().find(|entity| entity.name == "Analytic Solid").expect("solid");
        for component in &mut entity.components {
            if let crate::ComponentRecord::ParametricBlock(slot) = component {
                *slot = block;
                return;
            }
        }
        panic!("no block");
    }

    fn chart_of(record: &BlockRecord, identity: &str) -> String {
        record
            .intent
            .iter()
            .find_map(|entry| match &entry.payload {
                IntentPayload::AnalyticSurface { identity: name, chart, .. } if name == identity => Some(chart.clone()),
                _ => None,
            })
            .expect(identity)
    }

    fn radius_of(record: &BlockRecord, identity: &str) -> f64 {
        record
            .intent
            .iter()
            .find_map(|entry| match &entry.payload {
                IntentPayload::AnalyticSurface { identity: name, radius_m, .. } if name == identity => Some(*radius_m),
                _ => None,
            })
            .expect(identity)
    }

    fn translation_of(record: &BlockRecord, identity: &str) -> [f64; 3] {
        record
            .intent
            .iter()
            .find_map(|entry| match &entry.payload {
                IntentPayload::AnalyticSurface { identity: name, translation_m, .. } if name == identity => Some(*translation_m),
                _ => None,
            })
            .expect(identity)
    }

    fn assert_surface(account: &AnalyticAccount, n: u32, triangles: u32, error: &str, coarser: &str, position: u64, index: u64, gpu: u64) {
        assert_eq!(account.identity, SURFACE_ONE);
        assert_eq!(account.admission, AnalyticAdmission::AdmitInView);
        assert_eq!(account.n, n);
        assert_eq!(account.triangles_constructed, triangles);
        assert_eq!(format!("{:.6}", account.measured_error_px), error);
        assert_eq!(format!("{:.6}", account.coarser_px), coarser);
        assert_eq!(account.position_hash, position);
        assert_eq!(account.index_hash, index);
        assert_eq!(account.gpu_bytes, gpu);
    }
}
