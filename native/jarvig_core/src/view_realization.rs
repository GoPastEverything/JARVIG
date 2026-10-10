//! Disposable per-view meshes of one authoritative solid.
//!
//! The stored body, the intent tape, and `DerivedRenderGeometry` are not this
//! product. A realization is generated for one error budget and one camera,
//! then thrown away. It is not a cluster cut of the shared mesh.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::entity::EntityId;
use crate::jobs::{JobDesc, JobId, JobManager, JobState};
use crate::mesh::{create_mesh, Mesh, MeshDesc, MeshError, MeshIndexFormat, MeshTopology, MeshVertexAttribute, MeshVertexFormat, SubmeshDesc, VertexStreamDesc};
use crate::microgeometry::projected_detail_px;
use crate::space::{ResolvedPose, Vec3};
use crate::topology::SolidBody;

/// Bump when the simplifier's acceptance rule changes.
pub const REALIZATION_ALGORITHM_VERSION: u32 = 1;
/// Camera translation is quantized to this many meters in the cache key.
pub const REALIZATION_POSE_BUCKET_M: f64 = 0.5;
const WORKER_NAME: &str = "jarvig-realize";

#[derive(Clone, Debug)]
pub struct RealizationCamera {
    /// Eye in the solid's local frame. Meters.
    pub eye_local: [f64; 3],
    /// View direction in that same frame. It is normalized on use.
    pub forward_local: [f64; 3],
    pub vertical_fov_radians: f64,
    pub viewport_height: f32,
    pub requested_error_px: f32,
}

#[derive(Clone, Debug)]
pub struct ViewRealizationRequest {
    /// Caller slot. A newer request with a different cache key cancels the one still waiting.
    pub view: u32,
    pub entity: EntityId,
    pub body_hash: u64,
    pub revision: u64,
    pub body: SolidBody,
    pub camera: RealizationCamera,
}

#[derive(Clone, Debug)]
pub struct ViewRealizationFacts {
    pub realization_id: u64,
    pub view: u32,
    pub entity: EntityId,
    pub source_body_hash: u64,
    pub revision: u64,
    pub requested_error_px: f32,
    pub measured_projected_error_px: f32,
    /// Smallest collapse that was measured and refused, when one was tried.
    pub refused_collapse_px: Option<f32>,
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub microprimitive_count: u32,
    pub vertex_buffer_hash: u64,
    pub index_buffer_hash: u64,
    pub topology_hash: u64,
    pub cpu_bytes: u64,
    pub generation_us: u64,
    pub queue_wait_us: u64,
    pub queue_depth: u32,
    pub publish_us: u64,
    pub cache_hit: bool,
    pub cache_key: String,
    pub vertex_keys: Vec<(i64, i64, i64)>,
    pub mesh: Mesh,
}

#[derive(Clone, Debug)]
pub struct ViewRealizationControl {
    pub shared_vertices: u32,
    pub shared_triangles: u32,
    pub cut_strict_triangles: u32,
    pub cut_loose_triangles: u32,
}

/// What [`ViewRealizationHost::request`] decided. A miss has no id until [`ViewRealizationHost::poll`] publishes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RealizationTicket {
    pub view: u32,
    pub cache_hit: bool,
    pub realization_id: Option<u64>,
    /// The same cache key is already queued or running for this view. That job was left in place.
    pub already_inflight: bool,
}

pub struct ViewRealizationHost {
    jobs: JobManager,
    next_id: u64,
    max_queue_depth: u32,
    cancelled: u32,
    discarded_stale: u32,
    cache: BTreeMap<String, CachedMesh>,
    resident: BTreeMap<u32, ViewRealizationFacts>,
    inflight: BTreeMap<u32, JobId>,
    pending: Vec<Pending>,
}

#[derive(Clone)]
struct CachedMesh {
    measured_projected_error_px: f32,
    refused_collapse_px: Option<f32>,
    vertex_count: u32,
    triangle_count: u32,
    vertex_buffer_hash: u64,
    index_buffer_hash: u64,
    topology_hash: u64,
    cpu_bytes: u64,
    vertex_keys: Vec<(i64, i64, i64)>,
    mesh: Mesh,
}

struct Pending {
    view: u32,
    job: JobId,
    key: String,
    entity: EntityId,
    body_hash: u64,
    revision: u64,
    requested_error_px: f32,
    queue_depth: u32,
}

struct WorkerOut {
    measured_projected_error_px: f32,
    refused_collapse_px: Option<f32>,
    vertex_count: u32,
    triangle_count: u32,
    vertex_buffer_hash: u64,
    index_buffer_hash: u64,
    topology_hash: u64,
    cpu_bytes: u64,
    generation_us: u64,
    queue_wait_us: u64,
    vertex_keys: Vec<(i64, i64, i64)>,
    mesh: Mesh,
}

pub fn authoritative_body_hash(body: &SolidBody) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mix = |hash: &mut u64, bytes: &[u8]| {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    mix(&mut hash, &body.next_id.to_le_bytes());
    for vertex in &body.vertices {
        mix(&mut hash, &vertex.id.to_le_bytes());
        for axis in vertex.position {
            mix(&mut hash, &axis.to_bits().to_le_bytes());
        }
    }
    for edge in &body.edges {
        mix(&mut hash, &edge.id.to_le_bytes());
        mix(&mut hash, &edge.a.to_le_bytes());
        mix(&mut hash, &edge.b.to_le_bytes());
    }
    for face in &body.faces {
        mix(&mut hash, &face.id.to_le_bytes());
        for vertex in &face.vertices {
            mix(&mut hash, &vertex.to_le_bytes());
        }
    }
    hash
}

/// Eye and forward in the solid's local frame. The solid's world pose is rigid.
pub fn camera_in_solid_local(solid_world: ResolvedPose, eye_world: Vec3, forward_world: Vec3, vertical_fov_radians: f64, viewport_height: f32, requested_error_px: f32) -> RealizationCamera {
    let into = solid_world.rotation.conjugate();
    let offset = Vec3::new(eye_world.x - solid_world.translation.x, eye_world.y - solid_world.translation.y, eye_world.z - solid_world.translation.z);
    let eye = into.rotate(offset);
    let forward = into.rotate(forward_world);
    RealizationCamera {
        eye_local: [eye.x, eye.y, eye.z],
        forward_local: [forward.x, forward.y, forward.z],
        vertical_fov_radians,
        viewport_height,
        requested_error_px,
    }
}

impl ViewRealizationHost {
    /// One worker. The name is `jarvig-realize-0`. This is not the Einstein queue and not the general pool.
    pub fn new() -> Self {
        Self::with_workers(1)
    }

    /// A separate measurement pool. The editor constructor stays at one worker.
    /// `JobManager` clamps the count to 1..=4. This does not change the simplifier.
    pub fn with_workers(workers: usize) -> Self {
        Self {
            jobs: JobManager::named(WORKER_NAME, workers),
            next_id: 1,
            max_queue_depth: 0,
            cancelled: 0,
            discarded_stale: 0,
            cache: BTreeMap::new(),
            resident: BTreeMap::new(),
            inflight: BTreeMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn worker_count(&self) -> usize {
        self.jobs.worker_count()
    }

    /// The cache key `request` would use. View id is not part of it.
    pub fn cache_key_for(request: &ViewRealizationRequest) -> String {
        cache_key(request)
    }

    pub fn max_queue_depth(&self) -> u32 {
        self.max_queue_depth
    }

    pub fn cancelled_count(&self) -> u32 {
        self.cancelled
    }

    pub fn discarded_stale_count(&self) -> u32 {
        self.discarded_stale
    }

    pub fn resident_count(&self) -> usize {
        self.resident.len()
    }

    pub fn facts(&self, view: u32) -> Option<&ViewRealizationFacts> {
        self.resident.get(&view)
    }

    pub fn is_inflight(&self, view: u32) -> bool {
        self.inflight.contains_key(&view)
    }

    pub fn named_job_running(&self, name: &str) -> bool {
        self.jobs.snapshots().iter().any(|job| job.name == name && matches!(job.state, JobState::Running | JobState::CancelRequested))
    }

    /// Blocks the single worker until `release` is set. A realization submitted
    /// while this runs stays queued, so a newer request can cancel it.
    pub fn hold_worker(&self, release: Arc<AtomicBool>) {
        let _ = self.jobs.submit(JobDesc { name: "realize-hold".into(), asset: None, cache_key: None, dependencies: Vec::new() }, move |ctx| {
            while !release.load(Ordering::Relaxed) && !ctx.cancel_requested() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            if ctx.cancel_requested() {
                return Err("cancelled".to_string());
            }
            Ok(())
        });
    }

    pub fn request(&mut self, request: ViewRealizationRequest) -> Result<RealizationTicket, String> {
        if request.camera.requested_error_px <= 0.0 || !request.camera.requested_error_px.is_finite() {
            return Err("the error budget is not a positive pixel count".into());
        }
        if request.body.faces.is_empty() {
            return Err("the authoritative body has no faces".into());
        }
        let key = cache_key(&request);
        if self.resident.get(&request.view).is_some_and(|facts| facts.cache_key == key) {
            if let Some(old) = self.inflight.remove(&request.view) {
                self.jobs.cancel(old);
            }
            let id = self.resident[&request.view].realization_id;
            return Ok(RealizationTicket { view: request.view, cache_hit: true, realization_id: Some(id), already_inflight: false });
        }
        if let Some(job) = self.inflight.get(&request.view).copied() {
            if self.pending.iter().any(|item| item.view == request.view && item.job == job && item.key == key) {
                return Ok(RealizationTicket { view: request.view, cache_hit: false, realization_id: None, already_inflight: true });
            }
        }
        if let Some(cached) = self.cache.get(&key).cloned() {
            if let Some(old) = self.inflight.remove(&request.view) {
                self.jobs.cancel(old);
            }
            let id = self.next_id;
            self.next_id = self.next_id.saturating_add(1);
            let started = Instant::now();
            self.resident.insert(request.view, facts_from_cache(id, &request, &key, &cached, started.elapsed().as_micros() as u64));
            return Ok(RealizationTicket { view: request.view, cache_hit: true, realization_id: Some(id), already_inflight: false });
        }
        if let Some(old) = self.inflight.remove(&request.view) {
            self.jobs.cancel(old);
        }
        let view = request.view;
        let entity = request.entity;
        let body_hash = request.body_hash;
        let revision = request.revision;
        let requested_error_px = request.camera.requested_error_px;
        let submitted = Instant::now();
        let body = request.body;
        let camera = request.camera;
        let key_for_job = key.clone();
        let job = self.jobs.submit(
            JobDesc { name: format!("view-realization-{view}"), asset: None, cache_key: Some(key.clone()), dependencies: Vec::new() },
            move |ctx| {
                let queue_wait_us = submitted.elapsed().as_micros() as u64;
                let generation = Instant::now();
                if ctx.cancel_requested() {
                    return Err("cancelled".to_string());
                }
                let built = build_realization(&body, &camera, || ctx.cancel_requested())?;
                if ctx.cancel_requested() {
                    return Err("cancelled".to_string());
                }
                Ok(WorkerOut {
                    measured_projected_error_px: built.measured_projected_error_px,
                    refused_collapse_px: built.refused_collapse_px,
                    vertex_count: built.vertex_count,
                    triangle_count: built.triangle_count,
                    vertex_buffer_hash: built.vertex_buffer_hash,
                    index_buffer_hash: built.index_buffer_hash,
                    topology_hash: built.topology_hash,
                    cpu_bytes: built.cpu_bytes,
                    generation_us: generation.elapsed().as_micros() as u64,
                    queue_wait_us,
                    vertex_keys: built.vertex_keys,
                    mesh: built.mesh,
                })
            },
        );
        let depth = self.jobs.snapshots().iter().filter(|job| !job.state.finished()).count() as u32;
        self.max_queue_depth = self.max_queue_depth.max(depth);
        self.inflight.insert(view, job);
        self.pending.push(Pending { view, job, key: key_for_job, entity, body_hash, revision, requested_error_px, queue_depth: depth });
        Ok(RealizationTicket { view, cache_hit: false, realization_id: None, already_inflight: false })
    }

    pub fn poll(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        let mut still = Vec::new();
        for item in pending {
            match self.jobs.take_result::<WorkerOut>(item.job) {
                None => still.push(item),
                Some(Err(_)) => {
                    self.cancelled = self.cancelled.saturating_add(1);
                    if self.inflight.get(&item.view) == Some(&item.job) {
                        self.inflight.remove(&item.view);
                    }
                }
                Some(Ok(out)) => {
                    if self.inflight.get(&item.view) != Some(&item.job) {
                        self.discarded_stale = self.discarded_stale.saturating_add(1);
                        continue;
                    }
                    self.inflight.remove(&item.view);
                    let publish = Instant::now();
                    let cached = CachedMesh {
                        measured_projected_error_px: out.measured_projected_error_px,
                        refused_collapse_px: out.refused_collapse_px,
                        vertex_count: out.vertex_count,
                        triangle_count: out.triangle_count,
                        vertex_buffer_hash: out.vertex_buffer_hash,
                        index_buffer_hash: out.index_buffer_hash,
                        topology_hash: out.topology_hash,
                        cpu_bytes: out.cpu_bytes,
                        vertex_keys: out.vertex_keys,
                        mesh: out.mesh,
                    };
                    let id = self.next_id;
                    self.next_id = self.next_id.saturating_add(1);
                    let publish_us = publish.elapsed().as_micros() as u64;
                    self.resident.insert(
                        item.view,
                        ViewRealizationFacts {
                            realization_id: id,
                            view: item.view,
                            entity: item.entity,
                            source_body_hash: item.body_hash,
                            revision: item.revision,
                            requested_error_px: item.requested_error_px,
                            measured_projected_error_px: cached.measured_projected_error_px,
                            refused_collapse_px: cached.refused_collapse_px,
                            vertex_count: cached.vertex_count,
                            triangle_count: cached.triangle_count,
                            microprimitive_count: 0,
                            vertex_buffer_hash: cached.vertex_buffer_hash,
                            index_buffer_hash: cached.index_buffer_hash,
                            topology_hash: cached.topology_hash,
                            cpu_bytes: cached.cpu_bytes,
                            generation_us: out.generation_us,
                            queue_wait_us: out.queue_wait_us,
                            queue_depth: item.queue_depth,
                            publish_us,
                            cache_hit: false,
                            cache_key: item.key.clone(),
                            vertex_keys: cached.vertex_keys.clone(),
                            mesh: cached.mesh.clone(),
                        },
                    );
                    self.cache.insert(item.key, cached);
                }
            }
        }
        self.pending = still;
    }

    pub fn destroy_view(&mut self, view: u32) -> bool {
        if let Some(job) = self.inflight.remove(&view) {
            self.jobs.cancel(job);
        }
        let Some(facts) = self.resident.remove(&view) else { return false };
        self.cache.remove(&facts.cache_key);
        true
    }

    pub fn destroy_all(&mut self) {
        let views: Vec<u32> = self.resident.keys().copied().collect();
        for view in views {
            self.destroy_view(view);
        }
    }
}

fn facts_from_cache(id: u64, request: &ViewRealizationRequest, key: &str, cached: &CachedMesh, publish_us: u64) -> ViewRealizationFacts {
    ViewRealizationFacts {
        realization_id: id,
        view: request.view,
        entity: request.entity,
        source_body_hash: request.body_hash,
        revision: request.revision,
        requested_error_px: request.camera.requested_error_px,
        measured_projected_error_px: cached.measured_projected_error_px,
        refused_collapse_px: cached.refused_collapse_px,
        vertex_count: cached.vertex_count,
        triangle_count: cached.triangle_count,
        microprimitive_count: 0,
        vertex_buffer_hash: cached.vertex_buffer_hash,
        index_buffer_hash: cached.index_buffer_hash,
        topology_hash: cached.topology_hash,
        cpu_bytes: cached.cpu_bytes,
        generation_us: 0,
        queue_wait_us: 0,
        queue_depth: 0,
        publish_us,
        cache_hit: true,
        cache_key: key.to_string(),
        vertex_keys: cached.vertex_keys.clone(),
        mesh: cached.mesh.clone(),
    }
}

fn cache_key(request: &ViewRealizationRequest) -> String {
    let bucket = pose_bucket(&request.camera);
    let milli = (request.camera.requested_error_px * 1000.0).round() as i32;
    format!(
        "e{}-h{:016x}-r{}-px{}-p{}-{}-{}-{}-{}-{}-a{}",
        request.entity, request.body_hash, request.revision, milli, bucket.0, bucket.1, bucket.2, bucket.3, bucket.4, bucket.5, REALIZATION_ALGORITHM_VERSION
    )
}

fn pose_bucket(camera: &RealizationCamera) -> (i32, i32, i32, i8, i8, i8) {
    let quantize = |value: f64| (value / REALIZATION_POSE_BUCKET_M).round() as i32;
    let forward = unit(camera.forward_local).unwrap_or([0.0, 0.0, -1.0]);
    let sign = |value: f64| if value > 0.25 { 1 } else if value < -0.25 { -1 } else { 0 };
    (quantize(camera.eye_local[0]), quantize(camera.eye_local[1]), quantize(camera.eye_local[2]), sign(forward[0]), sign(forward[1]), sign(forward[2]))
}

pub fn format_view_realization_pair(label: &str, strict: &ViewRealizationFacts, loose: &ViewRealizationFacts, control: &ViewRealizationControl) -> String {
    let same_vertex = strict.vertex_buffer_hash == loose.vertex_buffer_hash;
    let same_index = strict.index_buffer_hash == loose.index_buffer_hash;
    let different_topology = strict.topology_hash != loose.topology_hash;
    let different_count = strict.triangle_count != loose.triangle_count;
    let strict_ok = strict.measured_projected_error_px <= strict.requested_error_px + 0.05;
    let loose_ok = loose.measured_projected_error_px <= loose.requested_error_px + 0.05;
    let one_discretization = !different_topology && !different_count && same_vertex && same_index;
    let strict_subset = is_subset(&strict.vertex_keys, &loose.vertex_keys);
    let loose_subset = is_subset(&loose.vertex_keys, &strict.vertex_keys);
    let mut report = String::new();
    report.push_str(&format!("Measurement: {label}\n"));
    report.push_str("Shared authoritative object: YES\n");
    report.push_str("Shared stored render mesh: NO\n");
    report.push_str(&format!("Same vertex buffer: {}\n", yes_no(same_vertex)));
    report.push_str(&format!("Same index buffer: {}\n", yes_no(same_index)));
    report.push_str(&format!("Different generated topology: {}\n", yes_no(different_topology)));
    report.push_str(&format!("Different primitive count: {}\n", yes_no(different_count)));
    report.push_str(&format!("Both within requested error: {}\n", yes_no(strict_ok && loose_ok)));
    report.push_str(&format!("Budgets produced one discretization: {}\n", yes_no(one_discretization)));
    report.push_str(&format!("Strict vertices subset of loose: {}\n", yes_no(strict_subset)));
    report.push_str(&format!("Loose vertices subset of strict: {}\n", yes_no(loose_subset)));
    report.push_str("Control path replaced: NO\n");
    report.push_str(&format!("Control shared vertices: {}\n", control.shared_vertices));
    report.push_str(&format!("Control shared triangles: {}\n", control.shared_triangles));
    report.push_str(&format!("Control cut 0.5 px triangles: {}\n", control.cut_strict_triangles));
    report.push_str(&format!("Control cut 4.0 px triangles: {}\n", control.cut_loose_triangles));
    report.push_str("Control cut is the experiment: NO\n");
    push_view(&mut report, "View A", strict);
    push_view(&mut report, "View B", loose);
    report
}

fn push_view(report: &mut String, name: &str, facts: &ViewRealizationFacts) {
    report.push_str(&format!("{name}\n"));
    report.push_str(&format!("realization_id: {}\n", facts.realization_id));
    report.push_str(&format!("source_body_hash: {:016x}\n", facts.source_body_hash));
    report.push_str(&format!("requested_error_px: {:.3}\n", facts.requested_error_px));
    report.push_str(&format!("measured_projected_error_px: {:.4}\n", facts.measured_projected_error_px));
    report.push_str(&format!("refused_collapse_px: {}\n", facts.refused_collapse_px.map(|value| format!("{value:.4}")).unwrap_or_else(|| "NONE".into())));
    report.push_str(&format!("vertex_count: {}\n", facts.vertex_count));
    report.push_str(&format!("triangle_count: {}\n", facts.triangle_count));
    report.push_str(&format!("microprimitive_count: {}\n", facts.microprimitive_count));
    report.push_str(&format!("vertex_buffer_hash: {:016x}\n", facts.vertex_buffer_hash));
    report.push_str(&format!("index_buffer_hash: {:016x}\n", facts.index_buffer_hash));
    report.push_str(&format!("CPU_bytes: {}\n", facts.cpu_bytes));
    report.push_str("GPU_bytes: NOT UPLOADED\n");
    report.push_str(&format!("generation_us: {}\n", facts.generation_us));
    report.push_str(&format!("queue_wait_us: {}\n", facts.queue_wait_us));
    report.push_str(&format!("queue_depth: {}\n", facts.queue_depth));
    report.push_str(&format!("publish_us: {}\n", facts.publish_us));
    report.push_str(&format!("cache_hit: {}\n", yes_no(facts.cache_hit)));
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn is_subset(left: &[(i64, i64, i64)], right: &[(i64, i64, i64)]) -> bool {
    left.iter().all(|key| right.contains(key))
}

struct Built {
    mesh: Mesh,
    measured_projected_error_px: f32,
    refused_collapse_px: Option<f32>,
    vertex_count: u32,
    triangle_count: u32,
    vertex_buffer_hash: u64,
    index_buffer_hash: u64,
    topology_hash: u64,
    cpu_bytes: u64,
    vertex_keys: Vec<(i64, i64, i64)>,
}

fn build_realization(body: &SolidBody, camera: &RealizationCamera, cancel: impl Fn() -> bool) -> Result<Built, String> {
    let surface = authoritative_triangles(body)?;
    let mut soup = soup_from_surface(&surface);
    let mut refused: Option<f32> = None;
    let budget = camera.requested_error_px;
    let tan_half = ((camera.vertical_fov_radians * 0.5) as f32).tan();
    loop {
        if cancel() {
            return Err("cancelled".into());
        }
        let mut edges = soup.edges();
        edges.sort_by(|left, right| edge_length(&soup, *left).total_cmp(&edge_length(&soup, *right)).then(left.0.cmp(&right.0)).then(left.1.cmp(&right.1)));
        let mut best: Option<(f32, Soup)> = None;
        for edge in edges.into_iter().take(64) {
            if cancel() {
                return Err("cancelled".into());
            }
            let (a, b) = edge;
            let midpoint = scale(add(soup.verts[a as usize], soup.verts[b as usize]), 0.5);
            for target in [soup.verts[a as usize], soup.verts[b as usize], midpoint] {
                let Some(candidate) = soup.collapse(a, b, target) else { continue };
                if candidate.verts.len() >= soup.verts.len() || candidate.tris.is_empty() {
                    continue;
                }
                let error = projected_error(&candidate, &surface, camera, tan_half);
                if error <= budget + 1.0e-3 {
                    let replace = match &best {
                        None => true,
                        Some((current, _)) => error < *current - 1.0e-4,
                    };
                    if replace {
                        best = Some((error, candidate));
                    }
                } else {
                    refused = Some(refused.map_or(error, |current| current.min(error)));
                }
            }
        }
        let Some((_, next)) = best else { break };
        let confirmed = projected_error(&next, &surface, camera, tan_half);
        if confirmed > budget + 0.05 {
            refused = Some(refused.map_or(confirmed, |current| current.min(confirmed)));
            break;
        }
        soup = next;
    }
    let measured = projected_error(&soup, &surface, camera, tan_half);
    emit_mesh(&soup, measured, refused)
}

fn emit_mesh(soup: &Soup, measured: f32, refused: Option<f32>) -> Result<Built, String> {
    let mut order: Vec<[u32; 3]> = soup.tris.clone();
    order.sort_by_key(|tri| triangle_key(soup, *tri));
    let mut vertices: Vec<([f32; 3], [f32; 3])> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut keys = Vec::new();
    for tri in order {
        let positions = [soup.verts[tri[0] as usize], soup.verts[tri[1] as usize], soup.verts[tri[2] as usize]];
        let normal = face_normal(positions[0], positions[1], positions[2]);
        if dot(normal, normal) < 1.0e-16 {
            continue;
        }
        for position in positions {
            let key = quantize_vertex(position);
            let found = vertices.iter().position(|(stored, stored_normal)| quantize_vertex(to_f64(*stored)) == key && dot(sub(to_f64(*stored_normal), normal), sub(to_f64(*stored_normal), normal)) < 1.0e-6);
            let index = if let Some(index) = found {
                index as u32
            } else {
                vertices.push((to_f32(position), to_f32(normal)));
                (vertices.len() - 1) as u32
            };
            indices.push(index);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    if indices.len() < 3 || vertices.is_empty() {
        return Err("the realization produced no triangles".into());
    }
    keys.sort_unstable();
    let topology_hash = hash_topology(soup);
    let mesh = mesh_from_flat(&vertices, &indices)?;
    let mut position_bytes = Vec::new();
    for index in 0..mesh.vertex_count() {
        let position = mesh.position(index).unwrap_or([0.0, 0.0, 0.0]);
        for value in position {
            position_bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let cpu_bytes = mesh.streams().iter().map(|stream| stream.bytes.len() as u64).sum::<u64>() + mesh.index_bytes().len() as u64;
    Ok(Built {
        vertex_count: mesh.vertex_count(),
        triangle_count: mesh.index_count() / 3,
        vertex_buffer_hash: fnv(&position_bytes),
        index_buffer_hash: fnv(mesh.index_bytes()),
        topology_hash,
        cpu_bytes,
        vertex_keys: keys,
        measured_projected_error_px: measured,
        refused_collapse_px: refused,
        mesh,
    })
}

fn mesh_from_flat(vertices: &[([f32; 3], [f32; 3])], indices: &[u32]) -> Result<Mesh, String> {
    let mut bytes = Vec::new();
    for (position, normal) in vertices {
        let edge = [1.0, 0.0, 0.0];
        let tangent = [edge[0], edge[1], edge[2], 1.0];
        for value in position.iter().chain([1.0, 1.0, 1.0].iter()).chain([0.0, 0.0].iter()).chain(normal.iter()).chain(tangent.iter()) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut index_bytes = Vec::new();
    let format = if vertices.len() > usize::from(u16::MAX) { MeshIndexFormat::Uint32 } else { MeshIndexFormat::Uint16 };
    for index in indices {
        match format {
            MeshIndexFormat::Uint16 => index_bytes.extend_from_slice(&(*index as u16).to_le_bytes()),
            MeshIndexFormat::Uint32 => index_bytes.extend_from_slice(&index.to_le_bytes()),
        }
    }
    create_mesh(MeshDesc {
        streams: vec![VertexStreamDesc {
            stride: 60,
            attributes: vec![
                MeshVertexAttribute { shader_location: 0, offset: 0, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 1, offset: 12, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 2, offset: 24, format: MeshVertexFormat::Float32x2 },
                MeshVertexAttribute { shader_location: 3, offset: 32, format: MeshVertexFormat::Float32x3 },
                MeshVertexAttribute { shader_location: 4, offset: 44, format: MeshVertexFormat::Float32x4 },
            ],
            bytes,
        }],
        index_format: format,
        index_bytes,
        submeshes: vec![SubmeshDesc {
            first_index: 0,
            index_count: indices.len() as u32,
            base_vertex: 0,
            topology: MeshTopology::TriangleList,
            material_slot: 0,
        }],
    })
    .map_err(|error: MeshError| error.to_string())
}

struct Soup {
    verts: Vec<[f64; 3]>,
    tris: Vec<[u32; 3]>,
}

impl Soup {
    fn edges(&self) -> Vec<(u32, u32)> {
        let mut edges = Vec::new();
        for tri in &self.tris {
            for pair in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let edge = if pair.0 < pair.1 { (pair.0, pair.1) } else { (pair.1, pair.0) };
                if edge.0 != edge.1 && !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
        edges
    }

    fn collapse(&self, a: u32, b: u32, target: [f64; 3]) -> Option<Self> {
        if a == b || a as usize >= self.verts.len() || b as usize >= self.verts.len() {
            return None;
        }
        let mut verts = self.verts.clone();
        verts[a as usize] = target;
        let mut tris = Vec::new();
        for tri in &self.tris {
            let mut corner = *tri;
            for slot in &mut corner {
                if *slot == b {
                    *slot = a;
                }
            }
            if corner[0] == corner[1] || corner[1] == corner[2] || corner[2] == corner[0] {
                continue;
            }
            let area = face_normal(verts[corner[0] as usize], verts[corner[1] as usize], verts[corner[2] as usize]);
            if dot(area, area) < 1.0e-16 {
                continue;
            }
            tris.push(corner);
        }
        if tris.len() < 4 {
            return None;
        }
        let mut used = vec![false; verts.len()];
        for tri in &tris {
            used[tri[0] as usize] = true;
            used[tri[1] as usize] = true;
            used[tri[2] as usize] = true;
        }
        let mut remap = vec![u32::MAX; verts.len()];
        let mut compact = Vec::new();
        for (index, live) in used.into_iter().enumerate() {
            if live {
                remap[index] = compact.len() as u32;
                compact.push(verts[index]);
            }
        }
        for tri in &mut tris {
            tri[0] = remap[tri[0] as usize];
            tri[1] = remap[tri[1] as usize];
            tri[2] = remap[tri[2] as usize];
        }
        Some(Self { verts: compact, tris })
    }
}

fn soup_from_surface(surface: &[[ [f64; 3]; 3 ]]) -> Soup {
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    for tri in surface {
        let mut index = [0u32; 3];
        for corner in 0..3 {
            let position = tri[corner];
            let found = verts.iter().position(|stored: &[f64; 3]| distance(*stored, position) < 1.0e-7);
            index[corner] = if let Some(found) = found {
                found as u32
            } else {
                verts.push(position);
                (verts.len() - 1) as u32
            };
        }
        if index[0] != index[1] && index[1] != index[2] && index[2] != index[0] {
            tris.push(index);
        }
    }
    Soup { verts, tris }
}

fn authoritative_triangles(body: &SolidBody) -> Result<Vec<[[f64; 3]; 3]>, String> {
    let mut triangles = Vec::new();
    for face in &body.faces {
        let Some(positions) = body.face_positions(face.id) else { continue };
        if positions.len() < 3 {
            continue;
        }
        for index in 1..positions.len() - 1 {
            let tri = [positions[0], positions[index], positions[index + 1]];
            let normal = face_normal(tri[0], tri[1], tri[2]);
            if dot(normal, normal) >= 1.0e-16 {
                triangles.push(tri);
            }
        }
    }
    if triangles.is_empty() {
        Err("the authoritative body produced no triangles".into())
    } else {
        Ok(triangles)
    }
}

/// Largest projected gap between the two surfaces. A deleted feature costs its own projected size.
fn projected_error(soup: &Soup, surface: &[[[f64; 3]; 3]], camera: &RealizationCamera, tan_half: f32) -> f32 {
    let forward = unit(camera.forward_local).unwrap_or([0.0, 0.0, -1.0]);
    let mut max_px = 0.0f32;
    for sample in samples(soup) {
        max_px = max_px.max(deviation_px(sample, surface_distance(sample, surface), camera, &forward, tan_half));
    }
    let realized = soup_triangles(soup);
    for triangle in surface {
        for sample in triangle_samples(*triangle) {
            max_px = max_px.max(deviation_px(sample, surface_distance(sample, &realized), camera, &forward, tan_half));
        }
    }
    max_px
}

fn deviation_px(sample: [f64; 3], deviation: f64, camera: &RealizationCamera, forward: &[f64; 3], tan_half: f32) -> f32 {
    if deviation < 1.0e-6 {
        return 0.0;
    }
    let depth = dot(sub(sample, camera.eye_local), *forward);
    if depth < 0.05 {
        return 0.0;
    }
    projected_detail_px(deviation as f32, depth as f32, camera.viewport_height, tan_half)
}

fn soup_triangles(soup: &Soup) -> Vec<[[f64; 3]; 3]> {
    soup.tris
        .iter()
        .filter_map(|tri| {
            let triangle = [soup.verts[tri[0] as usize], soup.verts[tri[1] as usize], soup.verts[tri[2] as usize]];
            let normal = face_normal(triangle[0], triangle[1], triangle[2]);
            (dot(normal, normal) >= 1.0e-16).then_some(triangle)
        })
        .collect()
}

fn triangle_samples(triangle: [[f64; 3]; 3]) -> [[f64; 3]; 7] {
    [
        triangle[0],
        triangle[1],
        triangle[2],
        scale(add(triangle[0], triangle[1]), 0.5),
        scale(add(triangle[1], triangle[2]), 0.5),
        scale(add(triangle[2], triangle[0]), 0.5),
        scale(add(add(triangle[0], triangle[1]), triangle[2]), 1.0 / 3.0),
    ]
}

fn samples(soup: &Soup) -> Vec<[f64; 3]> {
    let mut out = soup.verts.clone();
    for tri in &soup.tris {
        for pair in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            out.push(scale(add(soup.verts[pair.0 as usize], soup.verts[pair.1 as usize]), 0.5));
        }
        let center = scale(add(add(soup.verts[tri[0] as usize], soup.verts[tri[1] as usize]), soup.verts[tri[2] as usize]), 1.0 / 3.0);
        out.push(center);
    }
    out
}

fn surface_distance(point: [f64; 3], surface: &[[[f64; 3]; 3]]) -> f64 {
    let mut best = f64::MAX;
    for tri in surface {
        best = best.min(point_triangle_distance(point, tri[0], tri[1], tri[2]));
    }
    best
}

fn point_triangle_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(point, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return length(ap);
    }
    let bp = sub(point, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return length(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return length(sub(ap, scale(ab, v)));
    }
    let cp = sub(point, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return length(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return length(sub(ap, scale(ac, w)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return length(sub(bp, scale(sub(c, b), w)));
    }
    let denom = va + vb + vc;
    if denom.abs() < 1.0e-18 {
        return length(ap).min(length(bp)).min(length(cp));
    }
    let v = vb / denom;
    let w = vc / denom;
    length(sub(ap, add(scale(ab, v), scale(ac, w))))
}

fn edge_length(soup: &Soup, edge: (u32, u32)) -> f64 {
    distance(soup.verts[edge.0 as usize], soup.verts[edge.1 as usize])
}

fn triangle_key(soup: &Soup, tri: [u32; 3]) -> [i64; 9] {
    let mut corners = [quantize_vertex(soup.verts[tri[0] as usize]), quantize_vertex(soup.verts[tri[1] as usize]), quantize_vertex(soup.verts[tri[2] as usize])];
    corners.sort_unstable();
    [corners[0].0, corners[0].1, corners[0].2, corners[1].0, corners[1].1, corners[1].2, corners[2].0, corners[2].1, corners[2].2]
}

fn hash_topology(soup: &Soup) -> u64 {
    let mut keys: Vec<[i64; 9]> = soup.tris.iter().copied().map(|tri| triangle_key(soup, tri)).collect();
    keys.sort_unstable();
    let mut bytes = Vec::new();
    for key in keys {
        for value in key {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    fnv(&bytes)
}

fn quantize_vertex(position: [f64; 3]) -> (i64, i64, i64) {
    ((position[0] * 1.0e5).round() as i64, (position[1] * 1.0e5).round() as i64, (position[2] * 1.0e5).round() as i64)
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn face_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    cross(sub(b, a), sub(c, a))
}

fn unit(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = length(value);
    if length < 1.0e-12 { None } else { Some(scale(value, 1.0 / length)) }
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
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

fn length(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    length(sub(left, right))
}

fn to_f32(value: [f64; 3]) -> [f32; 3] {
    [value[0] as f32, value[1] as f32, value[2] as f32]
}

fn to_f64(value: [f32; 3]) -> [f64; 3] {
    [f64::from(value[0]), f64::from(value[1]), f64::from(value[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(host: &mut ViewRealizationHost, view: u32) {
        let started = Instant::now();
        loop {
            host.poll();
            if host.facts(view).is_some() && !host.is_inflight(view) {
                return;
            }
            if started.elapsed().as_secs() > 60 {
                panic!("view {view} did not publish");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn a_small_extrusion_is_eligible_for_the_looser_budget_only() {
        let body = SolidBody::from_box([1.0, 1.0, 1.0]).unwrap();
        let edited = body.extrude_edge(9, [0.0, -0.02, -0.02]).expect("extrude").body;
        edited.validate().unwrap();
        let hash = authoritative_body_hash(&edited);
        let camera = |pixels: f32| RealizationCamera {
            eye_local: [0.0, 0.0, 10.0],
            forward_local: [0.0, 0.0, -1.0],
            vertical_fov_radians: 60.0_f64.to_radians(),
            viewport_height: 1080.0,
            requested_error_px: pixels,
        };
        let request = |view, pixels| ViewRealizationRequest {
            view,
            entity: EntityId::parse("11111111-1111-4111-8111-111111111111").expect("entity"),
            body_hash: hash,
            revision: 1,
            body: edited.clone(),
            camera: camera(pixels),
        };
        let mut host = ViewRealizationHost::new();
        assert_eq!(host.worker_count(), 1);
        host.request(request(1, 0.5)).unwrap();
        host.request(request(2, 4.0)).unwrap();
        wait(&mut host, 1);
        wait(&mut host, 2);
        let strict = host.facts(1).unwrap().clone();
        let loose = host.facts(2).unwrap().clone();
        assert!(!strict.cache_hit && !loose.cache_hit);
        assert_ne!(strict.realization_id, loose.realization_id);
        assert_eq!(strict.source_body_hash, hash);
        assert_eq!(loose.source_body_hash, hash);
        assert!(strict.measured_projected_error_px <= 0.55, "{}", strict.measured_projected_error_px);
        assert!(loose.measured_projected_error_px <= 4.05, "{}", loose.measured_projected_error_px);
        assert_eq!(strict.microprimitive_count, 0);
        println!(
            "synthetic strict tris {} error {:.4} refused {} | loose tris {} error {:.4} refused {}",
            strict.triangle_count,
            strict.measured_projected_error_px,
            strict.refused_collapse_px.map(|value| format!("{value:.4}")).unwrap_or_else(|| "NONE".into()),
            loose.triangle_count,
            loose.measured_projected_error_px,
            loose.refused_collapse_px.map(|value| format!("{value:.4}")).unwrap_or_else(|| "NONE".into())
        );
        assert_ne!(strict.topology_hash, loose.topology_hash, "a 0.02 m extrusion at this camera is inside 4 px and outside 0.5 px");
        assert_ne!(strict.vertex_buffer_hash, loose.vertex_buffer_hash);
        assert_ne!(strict.triangle_count, loose.triangle_count);
    }

    #[test]
    fn a_repeated_same_key_request_stays_inflight() {
        let body = SolidBody::from_box([1.0, 1.0, 1.0]).unwrap();
        let hash = authoritative_body_hash(&body);
        let camera = RealizationCamera {
            eye_local: [0.0, 0.0, 8.0],
            forward_local: [0.0, 0.0, -1.0],
            vertical_fov_radians: 60.0_f64.to_radians(),
            viewport_height: 567.0,
            requested_error_px: 0.5,
        };
        let request = ViewRealizationRequest {
            view: 1,
            entity: EntityId::parse("11111111-1111-4111-8111-111111111111").expect("entity"),
            body_hash: hash,
            revision: 1,
            body,
            camera,
        };
        let mut host = ViewRealizationHost::new();
        let release = Arc::new(AtomicBool::new(false));
        host.hold_worker(Arc::clone(&release));
        let held = Instant::now();
        while !host.named_job_running("realize-hold") {
            assert!(held.elapsed().as_secs() < 5, "the realization worker did not take the hold");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let first = host.request(request.clone()).unwrap();
        let second = host.request(request).unwrap();
        assert!(!first.cache_hit && first.realization_id.is_none() && !first.already_inflight);
        assert!(second.already_inflight && !second.cache_hit && second.realization_id.is_none());
        assert_eq!(host.cancelled_count(), 0);
        assert!(host.is_inflight(1));
        release.store(true, Ordering::Relaxed);
        wait(&mut host, 1);
        assert_eq!(host.cancelled_count(), 0);
        assert_eq!(host.discarded_stale_count(), 0);
        assert_eq!(host.resident_count(), 1);
        let facts = host.facts(1).unwrap();
        assert!(!facts.cache_hit);
        assert!(facts.measured_projected_error_px <= 0.55);
    }
}
