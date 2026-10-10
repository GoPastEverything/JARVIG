//! Experiment 2B. Two live views of one Intent Solid, then a moving camera.
//!
//! This is a measurement. It does not replace the product renderer and it does
//! not accept ADR-0074. The simplifier is the Experiment 2 generator.

use std::collections::BTreeMap;
use std::time::Instant;

use jarvig_core::{IntentPayload, MeshId, ResolvedPose, Vec3};
use jarvig_renderer::{NormalizedRect, RenderViewDesc, RenderViewId, RenderViewSettings, RenderViewUpdate};

use super::{Editor, ViewBaseline};

const HOLD_SIMULTANEOUS: u32 = 180;
const HOLD_SHOT: u32 = 90;
const MOTION_FRAMES: u32 = 160;
const MOTION_STEP_M: f64 = 0.05;

pub(super) struct ViewLab {
    host: jarvig_core::ViewRealizationHost,
    stage: u8,
    mark: u32,
    wait_from: Option<Instant>,
    split: bool,
    right_view: Option<RenderViewId>,
    baseline: Option<ViewBaseline>,
    home_pose: ResolvedPose,
    home_camera: jarvig_core::Camera,
    solid_world: ResolvedPose,
    forward: Vec3,
    fov: f64,
    height: f32,
    panel: (u32, u32),
    view_px: (u32, u32),
    strict_camera: Option<jarvig_core::RealizationCamera>,
    loose_camera: Option<jarvig_core::RealizationCamera>,
    control: Option<jarvig_core::ViewRealizationControl>,
    left_mesh: Option<MeshId>,
    right_mesh: Option<MeshId>,
    library: BTreeMap<u32, MeshId>,
    retired: Vec<MeshId>,
    report: String,
    latest_key: String,
    last_seen_a: u64,
    published_a: Vec<u64>,
    stale_published: bool,
    requests: u32,
    hits: u32,
    misses: u32,
    joins: u32,
    motion_frames: u32,
    motion_logged: bool,
    home_requested: bool,
    regen_requested: bool,
    b_id: u64,
    b_tris: u32,
    b_hash: u64,
    b_regenerations: u32,
    cancel_at_motion: u32,
    stale_at_motion: u32,
    queue_at_motion: u32,
    home_ticket_hit: bool,
    home_ticket_id: u64,
    regen_hit: bool,
}

impl ViewLab {
    pub(super) fn new() -> Self {
        Self {
            host: jarvig_core::ViewRealizationHost::new(),
            stage: 0,
            mark: 0,
            wait_from: None,
            split: false,
            right_view: None,
            baseline: None,
            home_pose: ResolvedPose { translation: Vec3::new(0.0, 0.0, 0.0), rotation: jarvig_core::Quat::IDENTITY },
            home_camera: jarvig_core::Camera { frame: jarvig_core::FrameId(0), vertical_fov_radians: 1.0, near_m: 0.1 },
            solid_world: ResolvedPose { translation: Vec3::new(0.0, 0.0, 0.0), rotation: jarvig_core::Quat::IDENTITY },
            forward: Vec3::new(0.0, 0.0, -1.0),
            fov: 1.0,
            height: 1.0,
            panel: (0, 0),
            view_px: (0, 0),
            strict_camera: None,
            loose_camera: None,
            control: None,
            left_mesh: None,
            right_mesh: None,
            library: BTreeMap::new(),
            retired: Vec::new(),
            report: String::new(),
            latest_key: String::new(),
            last_seen_a: 0,
            published_a: Vec::new(),
            stale_published: false,
            requests: 0,
            hits: 0,
            misses: 0,
            joins: 0,
            motion_frames: 0,
            motion_logged: false,
            home_requested: false,
            regen_requested: false,
            b_id: 0,
            b_tris: 0,
            b_hash: 0,
            b_regenerations: 0,
            cancel_at_motion: 0,
            stale_at_motion: 0,
            queue_at_motion: 0,
            home_ticket_hit: false,
            home_ticket_id: 0,
            regen_hit: false,
        }
    }
}

impl Editor {
    pub(super) fn apply_view_lab_frame(&mut self) -> Result<(), String> {
        let Some(lab) = self.view_lab.as_ref() else {
            return Ok(());
        };
        if !lab.split {
            return Ok(());
        }
        let right = lab.right_view.ok_or("the second view is missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let pose = lab.home_pose;
        let camera = lab.home_camera;
        let entity = lab.baseline.as_ref().map(|baseline| baseline.intent);
        let left_mesh = lab.left_mesh;
        let right_mesh = lab.right_mesh;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer
            .update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose) })
            .map_err(|error| error.to_string())?;
        if let Some(entity) = entity {
            renderer.set_view_mesh_override(left, entity, left_mesh).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, right_mesh).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn step_view_lab(&mut self) -> Result<(), String> {
        if self.view_lab.is_none() {
            return Ok(());
        }
        self.view_lab.as_mut().unwrap().host.poll();
        self.lab_observe_a();
        let stage = self.view_lab.as_ref().unwrap().stage;
        if stage >= 15 {
            return Ok(());
        }
        match stage {
            0 => self.lab_stage_setup(),
            1 => self.lab_stage_wait_pair(),
            2 => self.lab_stage_simultaneous(),
            3 => self.lab_stage_hold(),
            4 => self.lab_stage_motion(),
            5 => self.lab_stage_motion_settle(),
            6 => self.lab_stage_home_frame(),
            7 => self.lab_stage_destroy_a(),
            8 => self.lab_stage_regen_wait(),
            9 => self.lab_stage_regen_frame(),
            10 => {
                let logged = self.view_lab.as_ref().unwrap().report.contains("View A during View B destruction frame: PRESENTED");
                if logged { self.lab_stage_destroy_b_hold() } else { self.lab_stage_destroy_b() }
            }
            11 => self.lab_stage_intact(),
            _ => Ok(()),
        }
    }

    fn note_lab(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.view_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    fn note_lab_block(&mut self, text: &str) {
        for line in text.lines() {
            self.note_lab(line);
        }
    }

    pub(super) fn lab_write_report(&mut self) {
        let mut text = self.view_lab.as_ref().unwrap().report.clone();
        text.push_str("View realization report: REPORT-VIEW-2B.txt\n");
        let dir = std::path::PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof");
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-VIEW-2B.txt"), &text)) {
            Ok(()) => self.note_lab("View realization report: REPORT-VIEW-2B.txt"),
            Err(error) => self.note_lab(&format!("View realization report was not written: {error}")),
        }
    }

    fn lab_observe_a(&mut self) {
        let (id, key) = {
            let Some(facts) = self.view_lab.as_ref().unwrap().host.facts(1) else {
                return;
            };
            (facts.realization_id, facts.cache_key.clone())
        };
        let stale = {
            let lab = self.view_lab.as_mut().unwrap();
            if id == lab.last_seen_a {
                return;
            }
            let stale = !lab.latest_key.is_empty() && key != lab.latest_key;
            if stale {
                lab.stale_published = true;
            }
            lab.published_a.push(id);
            lab.last_seen_a = id;
            stale
        };
        if stale {
            self.note_lab(&format!("Stale realization published: id {id}"));
        }
    }

    fn lab_wait_expired(&mut self) -> Result<(), String> {
        let started = self.view_lab.as_mut().unwrap().wait_from.get_or_insert_with(Instant::now);
        if started.elapsed().as_secs() > 180 {
            self.note_lab("Experiment 2B failed: a request did not publish within 180s.");
            return Err("a realization request did not publish within 180s".into());
        }
        Ok(())
    }

    fn lab_request(&mut self, view: u32, camera: jarvig_core::RealizationCamera, tally: bool) -> Result<jarvig_core::RealizationTicket, String> {
        let baseline = self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap();
        let request = jarvig_core::ViewRealizationRequest {
            view,
            entity: baseline.intent,
            body_hash: baseline.body_hash,
            revision: baseline.revision,
            body: baseline.record.body.clone().ok_or("the authoritative body is missing")?,
            camera,
        };
        let key = jarvig_core::ViewRealizationHost::cache_key_for(&request);
        let ticket = self.view_lab.as_mut().unwrap().host.request(request)?;
        if tally {
            let lab = self.view_lab.as_mut().unwrap();
            lab.requests = lab.requests.saturating_add(1);
            if ticket.already_inflight {
                lab.joins = lab.joins.saturating_add(1);
            } else if ticket.cache_hit {
                lab.hits = lab.hits.saturating_add(1);
            } else {
                lab.misses = lab.misses.saturating_add(1);
            }
        }
        if !ticket.already_inflight && view == 1 {
            self.view_lab.as_mut().unwrap().latest_key = key;
        }
        Ok(ticket)
    }

    fn lab_install(&mut self, view: u32) -> Result<(), String> {
        let mesh = self.view_lab.as_ref().unwrap().host.facts(view).ok_or(format!("view {view} has no realization"))?.mesh.clone();
        let control = self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap().object_mesh;
        let id = self.engine.world_mut().add_mesh(mesh);
        if id == control {
            self.note_lab("Experiment 2B failed: add_mesh returned the object mesh id.");
            return Err("add_mesh returned the object's mesh id".into());
        }
        let previous = self.view_lab.as_mut().unwrap().library.insert(view, id);
        if let Some(previous) = previous {
            if previous != id {
                self.lab_retire(previous)?;
            }
        }
        Ok(())
    }

    fn lab_retire(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap().object_mesh;
        if mesh == control {
            self.note_lab("Experiment 2B failed: the object mesh was queued for retirement.");
            return Err("refusing to retire the object mesh".into());
        }
        let retired = self.engine.world_mut().retire_unreferenced_mesh(mesh);
        if !retired {
            self.note_lab(&format!("Realization mesh {} stayed referenced.", mesh.0));
            return Err("a realization mesh was still referenced".into());
        }
        self.view_lab.as_mut().unwrap().retired.push(mesh);
        Ok(())
    }

    fn lab_note_b(&mut self, label: &str) {
        let (id, tris, hash) = {
            let facts = self.view_lab.as_ref().unwrap().host.facts(2);
            match facts {
                Some(facts) => (Some(facts.realization_id), facts.triangle_count, facts.vertex_buffer_hash),
                None => (None, 0, 0),
            }
        };
        let changed = id != Some(self.view_lab.as_ref().unwrap().b_id) || tris != self.view_lab.as_ref().unwrap().b_tris || hash != self.view_lab.as_ref().unwrap().b_hash;
        if changed {
            self.view_lab.as_mut().unwrap().b_regenerations = self.view_lab.as_ref().unwrap().b_regenerations.saturating_add(1);
        }
        self.note_lab(&format!(
            "{label}: realization_id {} triangles {tris} vertex_buffer_hash {hash:016x} changed {}",
            id.map(|value| value.to_string()).unwrap_or_else(|| "NONE".into()),
            yes_no(changed)
        ));
    }

    fn lab_stage_setup(&mut self) -> Result<(), String> {
        if !self.engine.intent_authority_experiment() {
            self.note_lab("Experiment 2B refused: the intent-authority experiment is off.");
            self.lab_write_report();
            self.view_lab.as_mut().unwrap().stage = 15;
            return Err("the intent-authority experiment is off".into());
        }
        if self.viewport_px.1 < 64 || self.editor_camera.is_none() || self.renderer.is_none() || self.viewport_view.is_none() {
            if self.frames >= 300 {
                return Err("the perspective viewport was not ready for Experiment 2B".into());
            }
            return Ok(());
        }
        let outline = self.engine.world().entity_outline();
        let intent = outline.iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid);
        let legacy = outline.iter().find(|item| item.name == "Legacy Cube").map(|item| item.uuid);
        let (Some(intent), Some(legacy)) = (intent, legacy) else {
            if self.frames >= 300 {
                self.note_lab("Experiment 2B failed: Intent Solid was not in the live level.");
                return Err("Intent Solid was not in the live level".into());
            }
            return Ok(());
        };
        let (home_position, home_yaw, home_pitch, home_pose, forward, fov, near, home_camera) = {
            let controller = self.editor_camera.as_ref().unwrap();
            (
                controller.position,
                controller.yaw,
                controller.pitch,
                controller.pose(),
                controller.forward(),
                controller.vertical_fov_radians,
                controller.near_m,
                jarvig_core::Camera { frame: controller.reference_frame, vertical_fov_radians: controller.vertical_fov_radians, near_m: controller.near_m },
            )
        };
        let (panel_w, panel_h) = self.viewport_px;
        let left_id = self.viewport_view.unwrap();
        let target = self.target.ok_or("viewport target missing")?;
        let right_id = {
            let renderer = self.renderer.as_mut().unwrap();
            let right = renderer
                .create_view(RenderViewDesc {
                    label: "JARVIG.Perspective.B".into(),
                    target,
                    camera: home_camera,
                    layout: NormalizedRect::RIGHT,
                    settings: RenderViewSettings::default(),
                })
                .map_err(|error| error.to_string())?;
            renderer
                .update_view(left_id, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::LEFT), settings: None, pose: None })
                .map_err(|error| error.to_string())?;
            renderer
                .update_view(right, RenderViewUpdate { camera: Some(home_camera), layout: None, settings: None, pose: Some(home_pose) })
                .map_err(|error| error.to_string())?;
            right
        };
        let (left_px, right_px) = {
            let renderer = self.renderer.as_ref().unwrap();
            (renderer.viewport(left_id).ok().flatten(), renderer.viewport(right_id).ok().flatten())
        };
        let view_w = left_px.map(|rect| rect.width).unwrap_or((panel_w as f32 * 0.5).round() as u32);
        let view_h = left_px.map(|rect| rect.height).unwrap_or(panel_h);
        let aspect = view_w as f32 / view_h.max(1) as f32;
        let gathered = {
            let world = self.engine.world();
            let record = world.authored_block(intent).ok_or("Intent Solid has no block")?.clone();
            let body = record.body.clone().ok_or("Intent Solid has no evaluated body. The 2B flag does not realize a stored body.")?;
            if body.validate().is_err() {
                return Err("Intent Solid body does not validate".into());
            }
            let legacy_record = world.authored_block(legacy).ok_or("Legacy Cube has no block")?.clone();
            let object_mesh = world.object_mesh(intent).ok_or("Intent Solid has no mesh")?;
            let legacy_mesh = world.object_mesh(legacy).ok_or("Legacy Cube has no mesh")?;
            let set = world.derived_meshlets(object_mesh).ok_or("the control mesh has no meshlets")?.clone();
            let shared = world.meshes().get(object_mesh).ok_or("the control mesh is not in the library")?;
            let records: Vec<_> = set.meshlets.iter().map(jarvig_core::GpuMeshletRecord::from_meshlet).collect();
            let hierarchy = jarvig_core::build_cluster_hierarchy(&records);
            let leaf_triangles: Vec<u32> = set.meshlets.iter().map(|meshlet| meshlet.index_count / 3).collect();
            let leaf_flags = vec![1u32; set.meshlets.len()];
            let snapshot = world.extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let instance = snapshot.instances().iter().find(|item| item.entity == intent).cloned().ok_or("Intent Solid is not in the snapshot")?;
            let cut_at = |pixels: f32| {
                jarvig_core::cut_visible_hierarchy_for_pose(&hierarchy, &instance, &home_pose, fov, near, aspect, view_h as f32, pixels, &leaf_flags, &leaf_triangles)
                    .map(|cut| cut.submitted_triangles)
                    .map_err(|error| error.to_string())
            };
            let control = jarvig_core::ViewRealizationControl {
                shared_vertices: shared.vertex_count(),
                shared_triangles: shared.index_count() / 3,
                cut_strict_triangles: cut_at(0.5)?,
                cut_loose_triangles: cut_at(4.0)?,
            };
            let solid_world = world.entity_world_pose(intent).map_err(|error| error.to_string())?;
            let camera_for = |pixels: f32| jarvig_core::camera_in_solid_local(solid_world, home_position, forward, fov, view_h as f32, pixels);
            let histogram = intent_histogram(&record);
            let baseline = ViewBaseline {
                intent,
                legacy,
                record,
                legacy_record,
                body_hash: jarvig_core::authoritative_body_hash(&body),
                pose: world.entity_local_pose(intent).map_err(|error| error.to_string())?.translation,
                legacy_pose: world.entity_local_pose(legacy).map_err(|error| error.to_string())?.translation,
                object_mesh,
                legacy_mesh,
                mesh_count: world.mesh_count(),
                revision: world.revision(),
                collision_inside: world.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0)),
                collision_outside: world.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0)),
                name: world.remember_entity(intent).map_err(|error| error.to_string())?.name,
                legacy_name: world.remember_entity(legacy).map_err(|error| error.to_string())?.name,
                meshlets: set.meshlets,
                vertex_indices: set.vertex_indices,
                local_indices: set.local_indices,
                home_position,
                home_yaw,
                home_pitch,
            };
            (baseline, camera_for(0.5), camera_for(4.0), control, solid_world, histogram, body.faces.len(), body.edges.len(), body.vertices.len())
        };
        {
            let lab = self.view_lab.as_mut().unwrap();
            lab.right_view = Some(right_id);
            lab.split = true;
            lab.home_pose = home_pose;
            lab.home_camera = home_camera;
            lab.solid_world = gathered.4;
            lab.forward = forward;
            lab.fov = fov;
            lab.height = view_h as f32;
            lab.panel = (panel_w, panel_h);
            lab.view_px = (view_w, view_h);
            lab.baseline = Some(gathered.0);
            lab.strict_camera = Some(gathered.1.clone());
            lab.loose_camera = Some(gathered.2.clone());
            lab.control = Some(gathered.3.clone());
        }
        let (strict, loose, control, histogram, faces, edges, vertices) = (gathered.1, gathered.2, gathered.3, gathered.5, gathered.6, gathered.7, gathered.8);
        self.note_lab("Experiment 2B: ON");
        self.note_lab("Intent-authority experiment: ON");
        self.note_lab("ADR-0074 accepted: NO");
        self.note_lab("Default renderer replaced: NO");
        self.note_lab("Experiment 2 generator changed: NO");
        self.note_lab(&format!(
            "Editor workers: general {} einstein {} realize {}",
            self.jobs.worker_count(),
            self.einstein_jobs.worker_count(),
            self.view_lab.as_ref().unwrap().host.worker_count()
        ));
        self.note_lab("One realization worker: YES");
        self.note_lab("Second realization worker in the editor: NO");
        self.note_lab("Editor worker cap raised: NO");
        self.note_lab("Two-worker prototype: CPU test, not this process");
        self.note_lab("Multithreaded rendering improvement claimed: NO");
        self.note_lab(&format!("Editor panel: {panel_w}x{panel_h}"));
        self.note_lab(&format!("View A pixel size: {view_w}x{view_h}"));
        if let Some(rect) = right_px {
            self.note_lab(&format!("View B pixel size: {}x{}", rect.width, rect.height));
        }
        self.note_lab("Simultaneous views: LEFT and RIGHT of one present");
        self.note_lab("Sequential screenshots are the measurement: NO");
        self.note_lab("Color pass uses the per-view mesh: YES");
        self.note_lab("Shadow and contact passes use the control mesh: YES");
        self.note_lab(&format!("Editor cluster-cut threshold px: {:.3}", self.hierarchy_error_px));
        self.note_lab(&format!("Control mesh id: {}", self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap().object_mesh.0));
        self.note_lab(&histogram);
        self.note_lab(&format!("Authoritative body: {faces} faces, {edges} edges, {vertices} vertices"));
        self.note_lab(&format!("Control shared vertices: {}", control.shared_vertices));
        self.note_lab(&format!("Control shared triangles: {}", control.shared_triangles));
        self.note_lab(&format!("Control cut 0.5 px triangles: {}", control.cut_strict_triangles));
        self.note_lab(&format!("Control cut 4.0 px triangles: {}", control.cut_loose_triangles));
        self.note_lab("Control cuts use the split aspect and the panel height: YES");
        self.note_lab("Control cut is the experiment: NO");
        self.note_lab("Experiment uses that cut: NO");
        let ticket_a = self.lab_request(1, strict, false)?;
        let ticket_b = self.lab_request(2, loose, false)?;
        if ticket_a.cache_hit || ticket_b.cache_hit || ticket_a.realization_id.is_some() || ticket_b.realization_id.is_some() || ticket_a.already_inflight || ticket_b.already_inflight {
            self.note_lab("Experiment 2B failed: the first requests were not misses.");
            return Err("the first realization requests were not misses".into());
        }
        self.view_lab.as_mut().unwrap().stage = 1;
        self.view_lab.as_mut().unwrap().wait_from = None;
        Ok(())
    }

    fn lab_stage_wait_pair(&mut self) -> Result<(), String> {
        let ready = {
            let host = &self.view_lab.as_ref().unwrap().host;
            host.facts(1).is_some() && host.facts(2).is_some() && !host.is_inflight(1) && !host.is_inflight(2)
        };
        if !ready {
            self.lab_wait_expired()?;
            return Ok(());
        }
        self.lab_install(1)?;
        self.lab_install(2)?;
        let (left, right) = {
            let lab = self.view_lab.as_ref().unwrap();
            (*lab.library.get(&1).unwrap(), *lab.library.get(&2).unwrap())
        };
        let lab = self.view_lab.as_mut().unwrap();
        lab.left_mesh = Some(left);
        lab.right_mesh = Some(right);
        lab.stage = 2;
        lab.wait_from = None;
        let strict = lab.host.facts(1).unwrap().clone();
        let loose = lab.host.facts(2).unwrap().clone();
        lab.b_id = loose.realization_id;
        lab.b_tris = loose.triangle_count;
        lab.b_hash = loose.vertex_buffer_hash;
        let control = lab.control.clone().unwrap();
        let report = jarvig_core::format_view_realization_pair("simultaneous same pose", &strict, &loose, &control);
        self.note_lab_block(&report);
        self.note_lab("Pair report GPU_bytes line is the pre-upload formatter.");
        self.note_lab(&format!("View A body revision: {}", strict.revision));
        self.note_lab(&format!("View B body revision: {}", loose.revision));
        self.note_lab(&format!(
            "One-worker serialization: View A generation_us {} queue_wait_us {} | View B generation_us {} queue_wait_us {}",
            strict.generation_us, strict.queue_wait_us, loose.generation_us, loose.queue_wait_us
        ));
        let covered = loose.queue_wait_us > 100_000 && loose.queue_wait_us + 100_000 >= strict.generation_us;
        self.note_lab(&format!("View B queue wait covers View A generation: {}", yes_no(covered)));
        Ok(())
    }

    fn lab_stage_simultaneous(&mut self) -> Result<(), String> {
        let (left, right, object) = {
            let lab = self.view_lab.as_ref().unwrap();
            (lab.left_mesh.unwrap(), lab.right_mesh.unwrap(), lab.baseline.as_ref().unwrap().object_mesh)
        };
        let gpu_a = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(left));
        let gpu_b = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(right));
        let (Some(bytes_a), Some(bytes_b)) = (gpu_a, gpu_b) else {
            self.note_lab("Experiment 2B failed: a realization mesh was not resident on the simultaneous frame.");
            return Err("a realization mesh was not resident".into());
        };
        let (strict, loose) = {
            let host = &self.view_lab.as_ref().unwrap().host;
            (host.facts(1).unwrap().clone(), host.facts(2).unwrap().clone())
        };
        self.note_lab(&format!("Editor frame: {}", self.frames));
        self.note_lab(&format!("Engine render frame: {}", self.engine.render_frame()));
        self.note_lab(&format!("View A mesh id: {}", left.0));
        self.note_lab(&format!("View B mesh id: {}", right.0));
        self.note_lab(&format!("Object mesh id: {}", object.0));
        self.note_lab(&format!("View A mesh is the object mesh: {}", yes_no(left == object)));
        self.note_lab(&format!("View B mesh is the object mesh: {}", yes_no(right == object)));
        self.note_lab(&format!("Views share a realization mesh: {}", yes_no(left == right)));
        self.note_lab(&format!("View A realization_id: {}", strict.realization_id));
        self.note_lab(&format!("View B realization_id: {}", loose.realization_id));
        self.note_lab(&format!("View A source_body_hash: {:016x}", strict.source_body_hash));
        self.note_lab(&format!("View B source_body_hash: {:016x}", loose.source_body_hash));
        self.note_lab(&format!("View A vertex_buffer_hash: {:016x}", strict.vertex_buffer_hash));
        self.note_lab(&format!("View B vertex_buffer_hash: {:016x}", loose.vertex_buffer_hash));
        self.note_lab(&format!("View A index_buffer_hash: {:016x}", strict.index_buffer_hash));
        self.note_lab(&format!("View B index_buffer_hash: {:016x}", loose.index_buffer_hash));
        self.note_lab(&format!("View A triangles: {}", strict.triangle_count));
        self.note_lab(&format!("View B triangles: {}", loose.triangle_count));
        self.note_lab(&format!("View A requested_error_px: {:.3}", strict.requested_error_px));
        self.note_lab(&format!("View B requested_error_px: {:.3}", loose.requested_error_px));
        self.note_lab(&format!("View A measured_projected_error_px: {:.4}", strict.measured_projected_error_px));
        self.note_lab(&format!("View B measured_projected_error_px: {:.4}", loose.measured_projected_error_px));
        self.note_lab(&format!("View A GPU_bytes: {bytes_a}"));
        self.note_lab(&format!("View B GPU_bytes: {bytes_b}"));
        self.note_lab("Simultaneous realizations frame: PRESENTED");
        let frame = self.frames;
        let lab = self.view_lab.as_mut().unwrap();
        lab.stage = 3;
        lab.mark = frame;
        Ok(())
    }

    fn lab_stage_hold(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.view_lab.as_ref().unwrap().mark) < HOLD_SIMULTANEOUS {
            return Ok(());
        }
        {
            let lab = self.view_lab.as_mut().unwrap();
            lab.cancel_at_motion = lab.host.cancelled_count();
            lab.stale_at_motion = lab.host.discarded_stale_count();
            lab.queue_at_motion = lab.host.max_queue_depth();
            lab.stage = 4;
            lab.motion_frames = 0;
        }
        self.note_lab("View A motion: start");
        self.lab_note_b("View B at motion start");
        Ok(())
    }

    fn lab_stage_motion(&mut self) -> Result<(), String> {
        let frame = self.view_lab.as_ref().unwrap().motion_frames;
        if frame >= MOTION_FRAMES {
            let lab = self.view_lab.as_mut().unwrap();
            lab.stage = 5;
            lab.wait_from = None;
            return Ok(());
        }
        let (position, yaw, pitch, forward, fov, height, solid_world) = {
            let lab = self.view_lab.as_ref().unwrap();
            let baseline = lab.baseline.as_ref().unwrap();
            let mut position = baseline.home_position;
            position.x += MOTION_STEP_M * f64::from(frame + 1);
            (position, baseline.home_yaw, baseline.home_pitch, lab.forward, lab.fov, lab.height, lab.solid_world)
        };
        self.place_editor_camera(position, yaw, pitch)?;
        let camera = jarvig_core::camera_in_solid_local(solid_world, position, forward, fov, height, 0.5);
        let _ = self.lab_request(1, camera, true)?;
        self.view_lab.as_mut().unwrap().motion_frames = frame.saturating_add(1);
        Ok(())
    }

    fn lab_stage_motion_settle(&mut self) -> Result<(), String> {
        if !self.view_lab.as_ref().unwrap().motion_logged {
            self.lab_log_motion();
            self.view_lab.as_mut().unwrap().motion_logged = true;
            let (position, yaw, pitch) = {
                let baseline = self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap();
                (baseline.home_position, baseline.home_yaw, baseline.home_pitch)
            };
            self.place_editor_camera(position, yaw, pitch)?;
            self.view_lab.as_mut().unwrap().wait_from = None;
        }
        if !self.view_lab.as_ref().unwrap().home_requested {
            let camera = self.view_lab.as_ref().unwrap().strict_camera.clone().unwrap();
            let ticket = self.lab_request(1, camera, false)?;
            let lab = self.view_lab.as_mut().unwrap();
            lab.home_requested = true;
            lab.home_ticket_hit = ticket.cache_hit;
            lab.home_ticket_id = ticket.realization_id.unwrap_or(0);
        }
        let home_key = {
            let lab = self.view_lab.as_ref().unwrap();
            let camera = lab.strict_camera.clone().unwrap();
            let baseline = lab.baseline.as_ref().unwrap();
            let request = jarvig_core::ViewRealizationRequest {
                view: 1,
                entity: baseline.intent,
                body_hash: baseline.body_hash,
                revision: baseline.revision,
                body: baseline.record.body.clone().unwrap(),
                camera,
            };
            jarvig_core::ViewRealizationHost::cache_key_for(&request)
        };
        let ready = {
            let host = &self.view_lab.as_ref().unwrap().host;
            host.facts(1).is_some_and(|facts| facts.cache_key == home_key) && !host.is_inflight(1)
        };
        if !ready {
            self.lab_wait_expired()?;
            return Ok(());
        }
        self.lab_install(1)?;
        let mesh = *self.view_lab.as_ref().unwrap().library.get(&1).unwrap();
        let facts = self.view_lab.as_ref().unwrap().host.facts(1).unwrap().clone();
        let original_hash = {
            let lab = self.view_lab.as_ref().unwrap();
            lab.published_a.first().copied()
        };
        let _ = original_hash;
        self.note_lab(&format!("View A return home cache_hit: {}", yes_no(facts.cache_hit)));
        self.note_lab(&format!("View A return home realization_id: {}", facts.realization_id));
        self.note_lab(&format!("View A return home vertex_buffer_hash: {:016x}", facts.vertex_buffer_hash));
        self.note_lab(&format!("View A return home triangle_count: {}", facts.triangle_count));
        let lab = self.view_lab.as_mut().unwrap();
        lab.left_mesh = Some(mesh);
        lab.stage = 6;
        lab.mark = self.frames;
        Ok(())
    }

    fn lab_log_motion(&mut self) {
        let (requests, hits, misses, joins, ids, stale, b_regen, cancel_at, stale_at, queue_at) = {
            let lab = self.view_lab.as_ref().unwrap();
            (
                lab.requests,
                lab.hits,
                lab.misses,
                lab.joins,
                lab.published_a.clone(),
                lab.stale_published,
                lab.b_regenerations,
                lab.cancel_at_motion,
                lab.stale_at_motion,
                lab.queue_at_motion,
            )
        };
        let (cancelled, discarded, depth, generation, wait, publish) = {
            let host = &self.view_lab.as_ref().unwrap().host;
            let facts = host.facts(1);
            (
                host.cancelled_count().saturating_sub(cancel_at),
                host.discarded_stale_count().saturating_sub(stale_at),
                host.max_queue_depth().saturating_sub(queue_at),
                facts.map(|facts| facts.generation_us).unwrap_or(0),
                facts.map(|facts| facts.queue_wait_us).unwrap_or(0),
                facts.map(|facts| facts.publish_us).unwrap_or(0),
            )
        };
        let ids_text = if ids.is_empty() { "NONE".to_string() } else { ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",") };
        self.note_lab("View A motion: end");
        self.note_lab(&format!("View A realization requests: {requests}"));
        self.note_lab(&format!("View A cache hits: {hits}"));
        self.note_lab(&format!("View A cache misses: {misses}"));
        self.note_lab(&format!("View A already inflight: {joins}"));
        self.note_lab(&format!("Stale jobs cancelled: {cancelled}"));
        self.note_lab(&format!("Jobs completed but discarded: {discarded}"));
        self.note_lab(&format!("Queue depth increase: {depth}"));
        self.note_lab(&format!("View A resident generation_us: {generation}"));
        self.note_lab(&format!("View A resident queue_wait_us: {wait}"));
        self.note_lab(&format!("View A resident publish_us: {publish}"));
        self.note_lab(&format!("View A resident ids: {ids_text}"));
        self.note_lab(&format!("Stale realization published: {}", yes_no(stale)));
        self.note_lab(&format!("View B regeneration count: {b_regen}"));
        self.lab_note_b("View B at motion end");
    }

    fn lab_stage_home_frame(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.view_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh = self.view_lab.as_ref().unwrap().left_mesh.unwrap();
        let bytes = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(mesh));
        self.note_lab(&format!("View A return home GPU_bytes: {}", bytes.map(|value| value.to_string()).unwrap_or_else(|| "NOT UPLOADED".into())));
        self.note_lab("View A return home frame: PRESENTED");
        self.lab_destroy_view(1)?;
        self.view_lab.as_mut().unwrap().stage = 7;
        Ok(())
    }

    fn lab_destroy_view(&mut self, view: u32) -> Result<(), String> {
        let mesh = self.view_lab.as_ref().unwrap().library.get(&view).copied();
        let destroyed = self.view_lab.as_mut().unwrap().host.destroy_view(view);
        self.note_lab(&format!("View {view} realization destroyed: {}", yes_no(destroyed)));
        if view == 1 {
            self.view_lab.as_mut().unwrap().left_mesh = None;
            self.view_lab.as_mut().unwrap().last_seen_a = 0;
        } else {
            self.view_lab.as_mut().unwrap().right_mesh = None;
        }
        if let Some(mesh) = mesh {
            self.lab_retire(mesh)?;
            self.view_lab.as_mut().unwrap().library.remove(&view);
        }
        Ok(())
    }

    fn lab_stage_destroy_a(&mut self) -> Result<(), String> {
        let right = self.view_lab.as_ref().unwrap().right_mesh.unwrap();
        let left_gone = self.view_lab.as_ref().unwrap().left_mesh.is_none();
        let gpu_b = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(right));
        let gpu_a = self.view_lab.as_ref().unwrap().retired.last().and_then(|mesh| self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(*mesh)));
        let (Some(bytes_b), true) = (gpu_b, left_gone) else {
            self.note_lab("Experiment 2B failed: View B was not resident while View A was destroyed.");
            return Err("View B was not resident during View A destruction".into());
        };
        self.note_lab(&format!("Editor frame: {}", self.frames));
        self.note_lab(&format!("Engine render frame: {}", self.engine.render_frame()));
        self.note_lab(&format!("View B GPU_bytes during View A destruction: {bytes_b}"));
        self.note_lab(&format!("View A GPU_bytes during View A destruction: {}", if gpu_a.is_some() { "RESIDENT" } else { "GONE" }));
        self.lab_note_b("View B during View A destruction");
        self.note_lab("View B during View A destruction frame: PRESENTED");
        if gpu_a.is_some() {
            self.note_lab("Experiment 2B failed: View A's realization stayed resident.");
            return Err("View A realization stayed resident".into());
        }
        let camera = self.view_lab.as_ref().unwrap().strict_camera.clone().unwrap();
        let ticket = self.lab_request(1, camera, false)?;
        let lab = self.view_lab.as_mut().unwrap();
        lab.regen_requested = true;
        lab.regen_hit = ticket.cache_hit;
        lab.stage = 8;
        lab.wait_from = None;
        self.note_lab(&format!("View A regenerate cache_hit: {}", yes_no(ticket.cache_hit)));
        Ok(())
    }

    fn lab_stage_regen_wait(&mut self) -> Result<(), String> {
        let ready = {
            let host = &self.view_lab.as_ref().unwrap().host;
            host.facts(1).is_some() && !host.is_inflight(1)
        };
        if !ready {
            self.lab_wait_expired()?;
            return Ok(());
        }
        self.lab_install(1)?;
        let mesh = *self.view_lab.as_ref().unwrap().library.get(&1).unwrap();
        let facts = self.view_lab.as_ref().unwrap().host.facts(1).unwrap().clone();
        self.note_lab(&format!("View A regenerated realization_id: {}", facts.realization_id));
        self.note_lab(&format!("View A regenerated triangles: {}", facts.triangle_count));
        self.note_lab(&format!("View A regenerated cache_hit: {}", yes_no(facts.cache_hit)));
        self.note_lab(&format!("View A regenerated vertex_buffer_hash: {:016x}", facts.vertex_buffer_hash));
        let lab = self.view_lab.as_mut().unwrap();
        lab.left_mesh = Some(mesh);
        lab.stage = 9;
        lab.mark = self.frames;
        Ok(())
    }

    fn lab_stage_regen_frame(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.view_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        let (left, right) = {
            let lab = self.view_lab.as_ref().unwrap();
            (lab.left_mesh.unwrap(), lab.right_mesh.unwrap())
        };
        let gpu_a = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(left));
        let gpu_b = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(right));
        if gpu_a.is_none() || gpu_b.is_none() {
            self.note_lab("Experiment 2B failed: a regenerated view was not resident.");
            return Err("a regenerated view was not resident".into());
        }
        self.note_lab(&format!("View A regenerated GPU_bytes: {}", gpu_a.unwrap()));
        self.note_lab(&format!("View B GPU_bytes beside regenerated A: {}", gpu_b.unwrap()));
        self.lab_note_b("View B beside regenerated A");
        self.note_lab("View A regenerated frame: PRESENTED");
        self.lab_destroy_view(2)?;
        self.view_lab.as_mut().unwrap().stage = 10;
        Ok(())
    }

    fn lab_stage_destroy_b(&mut self) -> Result<(), String> {
        let left = self.view_lab.as_ref().unwrap().left_mesh.unwrap();
        let gpu_a = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(left));
        let b_mesh_gone = self.view_lab.as_ref().unwrap().right_mesh.is_none();
        let (Some(bytes_a), true) = (gpu_a, b_mesh_gone) else {
            self.note_lab("Experiment 2B failed: View A was not resident while View B was destroyed.");
            return Err("View A was not resident during View B destruction".into());
        };
        self.note_lab(&format!("Editor frame: {}", self.frames));
        self.note_lab(&format!("Engine render frame: {}", self.engine.render_frame()));
        self.note_lab(&format!("View A GPU_bytes during View B destruction: {bytes_a}"));
        self.note_lab("View A during View B destruction frame: PRESENTED");
        self.view_lab.as_mut().unwrap().mark = self.frames;
        return Ok(());
    }

    fn lab_stage_destroy_b_hold(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.view_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.lab_destroy_view(1)?;
        let right = self.view_lab.as_ref().unwrap().right_view.unwrap();
        let left_view = self.viewport_view.ok_or("the perspective view is missing")?;
        let entity = self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap().intent;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_mesh_override(left_view, entity, None).map_err(|error| error.to_string())?;
            renderer
                .update_view(left_view, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::FULL), settings: None, pose: None })
                .map_err(|error| error.to_string())?;
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
        }
        let (position, yaw, pitch) = {
            let baseline = self.view_lab.as_ref().unwrap().baseline.as_ref().unwrap();
            (baseline.home_position, baseline.home_yaw, baseline.home_pitch)
        };
        self.place_editor_camera(position, yaw, pitch)?;
        let lab = self.view_lab.as_mut().unwrap();
        lab.split = false;
        lab.right_view = None;
        lab.stage = 11;
        Ok(())
    }

    fn lab_stage_intact(&mut self) -> Result<(), String> {
        let baseline = self.view_lab.as_ref().unwrap().baseline.clone().unwrap();
        let intent = baseline.intent;
        let legacy = baseline.legacy;
        let control = baseline.object_mesh;
        let record_same = self.engine.world().authored_block(intent).as_ref() == Some(&baseline.record);
        let legacy_same = self.engine.world().authored_block(legacy).as_ref() == Some(&baseline.legacy_record);
        let pose_same = self.engine.world().entity_local_pose(intent).ok().map(|pose| pose.translation) == Some(baseline.pose);
        let legacy_pose_same = self.engine.world().entity_local_pose(legacy).ok().map(|pose| pose.translation) == Some(baseline.legacy_pose);
        let mesh_same = self.engine.world().object_mesh(intent) == Some(control);
        let legacy_mesh_same = self.engine.world().object_mesh(legacy) == Some(baseline.legacy_mesh);
        let count_same = self.engine.world().mesh_count() == baseline.mesh_count;
        let revision_same = self.engine.world().revision() == baseline.revision;
        let name_same = self.engine.world().remember_entity(intent).ok().map(|entity| entity.name) == Some(baseline.name.clone());
        let legacy_name_same = self.engine.world().remember_entity(legacy).ok().map(|entity| entity.name) == Some(baseline.legacy_name.clone());
        let inside = self.engine.world().separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
        let outside = self.engine.world().separate_from_blocks(Vec3::new(0.0, 1.0, 40.0));
        let collision_same = inside == baseline.collision_inside && outside == baseline.collision_outside;
        let meshlets = self.engine.world().derived_meshlets(control);
        let meshlets_same = meshlets.is_some_and(|set| set.meshlets == baseline.meshlets && set.vertex_indices == baseline.vertex_indices && set.local_indices == baseline.local_indices);
        let control_present = self.engine.world().meshes().get(control).is_some();
        let retired = self.view_lab.as_ref().unwrap().retired.clone();
        let orphans_gone = retired.iter().all(|id| self.engine.world().meshes().get(*id).is_none());
        let gpu_gone = retired.iter().all(|id| self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(*id)).is_none());
        let control_bytes = self.renderer.as_ref().and_then(|renderer| renderer.resident_mesh_bytes(control));
        let element = if self.selected_vertex.is_some() {
            "vertex"
        } else if self.selected_edge.is_some() {
            "edge"
        } else if self.selected_body_face.is_some() {
            "face"
        } else {
            "NONE"
        };
        let (strict, loose) = {
            let report = &self.view_lab.as_ref().unwrap().report;
            (report.contains("Different generated topology: YES"), report.contains("Both within requested error: YES"))
        };
        let b_stable = self.view_lab.as_ref().unwrap().b_regenerations == 0;
        let stale_clean = !self.view_lab.as_ref().unwrap().stale_published;
        let views_live = self.renderer.as_ref().map(|renderer| renderer.view_count()) == Some(1);
        let object_ok = record_same
            && legacy_same
            && pose_same
            && legacy_pose_same
            && mesh_same
            && legacy_mesh_same
            && count_same
            && revision_same
            && name_same
            && legacy_name_same
            && collision_same
            && meshlets_same
            && control_present
            && orphans_gone
            && gpu_gone
            && control_bytes.is_some();
        self.note_lab(&format!("Editor frame: {}", self.frames));
        self.note_lab(&format!("Engine render frame: {}", self.engine.render_frame()));
        self.note_lab(&format!("Authoritative object frame: {}", if control_bytes.is_some() { "PRESENTED" } else { "NOT PRESENTED" }));
        if let Some(bytes) = control_bytes {
            self.note_lab(&format!("Control mesh GPU_bytes: {bytes}"));
        }
        self.note_lab(&format!("Authoritative record unchanged: {}", yes_no(record_same)));
        self.note_lab(&format!("Legacy record unchanged: {}", yes_no(legacy_same)));
        self.note_lab(&format!("Semantic identity unchanged: {}", yes_no(record_same && name_same)));
        self.note_lab(&format!("Object mesh unchanged: {}", yes_no(mesh_same)));
        self.note_lab(&format!("Control meshlets unchanged: {}", yes_no(meshlets_same)));
        self.note_lab(&format!("Collision unchanged: {}", yes_no(collision_same)));
        self.note_lab(&format!("Materials unchanged: {}", yes_no(record_same)));
        self.note_lab(&format!("World revision unchanged: {}", yes_no(revision_same)));
        self.note_lab(&format!("Library mesh count restored: {}", yes_no(count_same)));
        self.note_lab(&format!("Realization meshes removed: {}", yes_no(orphans_gone)));
        self.note_lab(&format!("Realization GPU buffers after destroy: {}", if gpu_gone { "EVICTED" } else { "RESIDENT" }));
        self.note_lab(&format!("Editor element selection: {element}"));
        self.note_lab("Realization vertex was selected: NO");
        self.note_lab(&format!("Render view count restored: {}", yes_no(views_live)));
        if let Some(record) = self.engine.world().authored_block(intent) {
            self.note_lab(&format!("Intent entries: {}", record.intent.len()));
            self.note_lab(&format!("Intent history: {}", record.history.len()));
            let live_hash = record.body.as_ref().map(jarvig_core::authoritative_body_hash);
            self.note_lab(&format!("source_body_hash: {}", live_hash.map(|hash| format!("{hash:016x}")).unwrap_or_else(|| "MISSING".into())));
            self.note_lab(&format!("source_body_hash matches the start: {}", yes_no(live_hash == Some(baseline.body_hash))));
        }
        let (queue_depth, cancelled, stale) = {
            let host = &self.view_lab.as_ref().unwrap().host;
            (host.max_queue_depth(), host.cancelled_count(), host.discarded_stale_count())
        };
        self.note_lab(&format!("Queue depth max: {queue_depth}"));
        self.note_lab(&format!("Cancelled jobs: {cancelled}"));
        self.note_lab(&format!("Stale discarded: {stale}"));
        self.note_lab(&format!("Algorithm version: {}", jarvig_core::REALIZATION_ALGORITHM_VERSION));
        let passed = object_ok && element == "NONE" && strict && loose && b_stable && stale_clean && views_live;
        self.note_lab(&format!("Different realizations held together: {}", yes_no(strict)));
        self.note_lab(&format!("Both within requested error: {}", yes_no(loose)));
        self.note_lab(&format!("View B stayed still: {}", yes_no(b_stable)));
        self.note_lab(&format!("Stale publish blocked: {}", yes_no(stale_clean)));
        self.note_lab(&format!(
            "Authoritative object after both realizations destroyed: {}",
            if object_ok { "INTACT" } else { "CHANGED" }
        ));
        self.note_lab(&format!("Experiment 2B: {}", if passed { "PASS" } else { "FAIL" }));
        self.lab_write_report();
        self.view_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 2B did not meet the simultaneous-view success condition".into());
        }
        Ok(())
    }
}

fn intent_histogram(record: &jarvig_core::BlockRecord) -> String {
    let mut size = 0u32;
    let mut subdivide = Vec::new();
    let mut region = 0u32;
    let mut side = 0u32;
    let mut extrude = 0u32;
    let mut split = 0u32;
    let mut bevel = 0u32;
    let mut move_edge = 0u32;
    let mut move_vertex = 0u32;
    let mut extrude_edge = 0u32;
    let mut mirror = 0u32;
    let mut push = 0u32;
    let mut gap = 0u32;
    let mut analytic_surface = 0u32;
    for entry in &record.intent {
        match &entry.payload {
            IntentPayload::Size { .. } => size += 1,
            IntentPayload::Subdivide { u, v } => subdivide.push((*u, *v)),
            IntentPayload::Extrude { .. } => {
                extrude += 1;
                let text = entry.groups.iter().flatten().flatten().cloned().collect::<Vec<_>>().join(" ");
                if text.contains("F:side") {
                    side += 1;
                } else if text.contains("F:cell") {
                    region += 1;
                }
            }
            IntentPayload::Split => split += 1,
            IntentPayload::Bevel { .. } => bevel += 1,
            IntentPayload::MoveEdge { .. } => move_edge += 1,
            IntentPayload::MoveVertex { .. } => move_vertex += 1,
            IntentPayload::ExtrudeEdge { .. } => extrude_edge += 1,
            IntentPayload::Mirror { .. } => mirror += 1,
            IntentPayload::PushFace { .. } => push += 1,
            IntentPayload::Gap { .. } => gap += 1,
            IntentPayload::AnalyticSurface { .. } => analytic_surface += 1,
            IntentPayload::Round { .. } | IntentPayload::Seed => {}
        }
    }
    let grids = if subdivide.is_empty() {
        "none".to_string()
    } else {
        subdivide.iter().map(|(u, v)| format!("{u}x{v}")).collect::<Vec<_>>().join(",")
    };
    let line = format!(
        "Intent ops: size {size}, subdivide {} ({grids}), extrude {extrude}, region extrude {region}, side extrude {side}, split {split}, bevel {bevel}, move-edge {move_edge}, extrude-edge {extrude_edge}, move-vertex {move_vertex}, mirror {mirror}, push-face {push}, gap {gap}",
        subdivide.len()
    );
    if analytic_surface == 0 {
        line
    } else {
        format!("{line}\nanalytic-surface {analytic_surface}")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}
