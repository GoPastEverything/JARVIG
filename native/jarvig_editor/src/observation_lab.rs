//! Experiment 3B. Two observation-local GPU meshes in one presented frame.
//!
//! The pack does not weld vertices. Shadows, contact, and probe capture stay on
//! the control mesh. This does not start a realization worker and it does not
//! accept ADR-0074.

use jarvig_core::{MeshId, Quat, ResolvedPose, Vec3};
use jarvig_renderer::{NormalizedRect, PixelRect, RenderViewDesc, RenderViewId, RenderViewSettings, RenderViewUpdate};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostQuitMessage, SetWindowPos, SWP_NOZORDER};

use crate::dock::{WorkspaceCommand, CONTENT, INSPECTOR, OUTLINER, OUTPUT};

use super::Editor;

const HOLD_SHOT: u32 = 24;
const VIEW_A_WIDTH: u32 = 160;
const VIEW_B_WIDTH: u32 = 960;
const VIEW_HEIGHT: u32 = 567;
const REQUIRED_WIDTH: u32 = VIEW_A_WIDTH + VIEW_B_WIDTH;

pub(super) struct ObservationLab {
    stage: u8,
    arranged: bool,
    split: bool,
    grow: u8,
    mark: u32,
    proved: bool,
    captured_a: bool,
    captured_b: bool,
    right_view: Option<RenderViewId>,
    intent: Option<jarvig_core::EntityId>,
    legacy: Option<jarvig_core::EntityId>,
    object_mesh: Option<MeshId>,
    legacy_mesh: Option<MeshId>,
    mesh_a: Option<MeshId>,
    mesh_b: Option<MeshId>,
    decoy: Option<MeshId>,
    retired: Vec<MeshId>,
    key_a: String,
    key_b: String,
    pose_a: ResolvedPose,
    pose_b: ResolvedPose,
    camera: jarvig_core::Camera,
    pack_a: Option<jarvig_core::ObservationGpuPack>,
    pack_b: Option<jarvig_core::ObservationGpuPack>,
    record: Option<jarvig_core::BlockRecord>,
    legacy_record: Option<jarvig_core::BlockRecord>,
    body_hash: u64,
    revision: u64,
    mesh_count: usize,
    pose: Vec3,
    legacy_pose: Vec3,
    collision_inside: Vec3,
    collision_outside: Vec3,
    name: String,
    legacy_name: String,
    face_ids: Vec<u32>,
    edge_ids: Vec<u32>,
    vertex_ids: Vec<u32>,
    meshlet_len: usize,
    control_gpu_bytes: Option<u64>,
    cut_strict: u32,
    cut_loose: u32,
    panel: (u32, u32),
    discarded_stale_gpu: u32,
    generation_a: u64,
    generation_b: u64,
    report: String,
}

impl ObservationLab {
    pub(super) fn new() -> Self {
        Self {
            stage: 0,
            arranged: false,
            split: false,
            grow: 0,
            mark: 0,
            proved: false,
            captured_a: false,
            captured_b: false,
            right_view: None,
            intent: None,
            legacy: None,
            object_mesh: None,
            legacy_mesh: None,
            mesh_a: None,
            mesh_b: None,
            decoy: None,
            retired: Vec::new(),
            key_a: String::new(),
            key_b: String::new(),
            pose_a: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            pose_b: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            camera: jarvig_core::Camera { frame: jarvig_core::FrameId(0), vertical_fov_radians: 1.0, near_m: 0.1 },
            pack_a: None,
            pack_b: None,
            record: None,
            legacy_record: None,
            body_hash: 0,
            revision: 0,
            mesh_count: 0,
            pose: Vec3::ZERO,
            legacy_pose: Vec3::ZERO,
            collision_inside: Vec3::ZERO,
            collision_outside: Vec3::ZERO,
            name: String::new(),
            legacy_name: String::new(),
            face_ids: Vec::new(),
            edge_ids: Vec::new(),
            vertex_ids: Vec::new(),
            meshlet_len: 0,
            control_gpu_bytes: None,
            cut_strict: 0,
            cut_loose: 0,
            panel: (0, 0),
            discarded_stale_gpu: 0,
            generation_a: 0,
            generation_b: 0,
            report: String::new(),
        }
    }
}

impl Editor {
    pub(super) fn apply_observation_frame(&mut self) -> Result<(), String> {
        let Some(lab) = self.observation_lab.as_ref() else {
            return Ok(());
        };
        if !lab.split {
            return Ok(());
        }
        let right = lab.right_view.ok_or("the second observation view is missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let (pose_a, pose_b, camera, entity, mesh_a, mesh_b) = (lab.pose_a, lab.pose_b, lab.camera, lab.intent, lab.mesh_a, lab.mesh_b);
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer
            .update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_a) })
            .map_err(|error| error.to_string())?;
        renderer
            .update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_b) })
            .map_err(|error| error.to_string())?;
        renderer.set_view_pixel_rect(left, Some(PixelRect { x: 0, y: 0, width: VIEW_A_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
        renderer
            .set_view_pixel_rect(right, Some(PixelRect { x: VIEW_A_WIDTH, y: 0, width: VIEW_B_WIDTH, height: VIEW_HEIGHT }))
            .map_err(|error| error.to_string())?;
        if let Some(entity) = entity {
            renderer.set_view_mesh_override(left, entity, mesh_a).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, mesh_b).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn step_observation_lab(&mut self) -> Result<(), String> {
        if self.observation_lab.is_none() {
            return Ok(());
        }
        let stage = self.observation_lab.as_ref().unwrap().stage;
        if stage >= 15 {
            return Ok(());
        }
        let result = match stage {
            0 => self.obs_stage_setup(),
            1 => self.obs_stage_present(),
            2 => self.obs_stage_a_gone(),
            3 => self.obs_stage_b_gone(),
            4 => self.obs_stage_intact(),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.note_obs(&format!("Experiment 3B: FAIL"));
            self.note_obs(&format!("Failure: {error}"));
            self.observation_write_report();
            if let Some(lab) = self.observation_lab.as_mut() {
                lab.stage = 15;
            }
            return Err(error);
        }
        Ok(())
    }

    fn note_obs(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.observation_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    pub(super) fn observation_write_report(&mut self) {
        let Some(lab) = self.observation_lab.as_ref() else {
            return;
        };
        let mut text = lab.report.clone();
        text.push_str("Observation report: REPORT-OBS-3B.txt\n");
        let dir = std::path::PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof");
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-OBS-3B.txt"), &text)) {
            Ok(()) => self.note_obs("Observation report: REPORT-OBS-3B.txt"),
            Err(error) => self.note_obs(&format!("Observation report was not written: {error}")),
        }
    }

    fn obs_capture(&mut self, name: &str) -> Result<(), String> {
        let path = format!(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\{name}");
        let image = super::capture_window_image(self.frame)?;
        super::write_png(&path, &image)?;
        self.note_obs(&format!("Capture: {name}"));
        Ok(())
    }

    fn obs_stage_setup(&mut self) -> Result<(), String> {
        if self.view_lab.is_some() || self.view_realization.is_some() {
            return Err("Experiment 3B refuses to run beside a realization worker".into());
        }
        if !self.engine.intent_authority_experiment() {
            return Err("the intent-authority experiment is off".into());
        }
        if self.renderer.is_none() || self.editor_camera.is_none() || self.viewport_view.is_none() || self.viewport_px.1 < 64 {
            if self.frames >= 400 {
                return Err("the perspective viewport was not ready".into());
            }
            return Ok(());
        }
        if !self.observation_lab.as_ref().unwrap().arranged {
            for panel in [OUTLINER, INSPECTOR, CONTENT, OUTPUT] {
                if self.workspace.is_open(panel) {
                    self.workspace.apply(WorkspaceCommand::ClosePanel(panel)).map_err(|error| error.to_string())?;
                }
            }
            unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, 1800, 1200, SWP_NOZORDER); }
            self.realize();
            let lab = self.observation_lab.as_mut().unwrap();
            lab.arranged = true;
            lab.mark = self.frames;
            return Ok(());
        }
        let (panel_w, panel_h) = self.viewport_px;
        let renderer_size = self.renderer.as_ref().unwrap().configured_size();
        if panel_w < REQUIRED_WIDTH || panel_h < VIEW_HEIGHT || renderer_size.0 < REQUIRED_WIDTH || renderer_size.1 < VIEW_HEIGHT {
            let grow = self.observation_lab.as_ref().unwrap().grow;
            let mark = self.observation_lab.as_ref().unwrap().mark;
            if grow < 2 && self.frames.saturating_sub(mark) > 8 {
                let (width, height) = if grow == 0 { (2200, 1400) } else { (2600, 1600) };
                unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, width, height, SWP_NOZORDER); }
                self.realize();
                let lab = self.observation_lab.as_mut().unwrap();
                lab.grow = lab.grow.saturating_add(1);
                lab.mark = self.frames;
            }
            if self.frames >= 500 {
                return Err(format!("the perspective client stayed {panel_w}x{panel_h}, renderer {}x{}", renderer_size.0, renderer_size.1));
            }
            return Ok(());
        }
        let control_gpu = {
            let outline = self.engine.world().entity_outline();
            let intent = outline.iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid);
            let legacy = outline.iter().find(|item| item.name == "Legacy Cube").map(|item| item.uuid);
            let (Some(intent), Some(_legacy)) = (intent, legacy) else {
                if self.frames >= 500 {
                    return Err("Intent Solid was not in the live level".into());
                }
                return Ok(());
            };
            let object_mesh = self.engine.world().object_mesh(intent).ok_or("Intent Solid has no mesh")?;
            self.renderer.as_ref().unwrap().resident_mesh_bytes(object_mesh)
        };
        let Some(control_gpu_bytes) = control_gpu else {
            if self.frames >= 500 {
                return Err("the control mesh was not uploaded".into());
            }
            return Ok(());
        };
        self.obs_install(control_gpu_bytes, (panel_w, panel_h))?;
        Ok(())
    }

    fn obs_install(&mut self, control_gpu_bytes: u64, panel: (u32, u32)) -> Result<(), String> {
        let outline = self.engine.world().entity_outline();
        let intent = outline.iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid).ok_or("Intent Solid missing")?;
        let legacy = outline.iter().find(|item| item.name == "Legacy Cube").map(|item| item.uuid).ok_or("Legacy Cube missing")?;
        let gathered = {
            let world = self.engine.world();
            let record = world.authored_block(intent).ok_or("Intent Solid has no block")?.clone();
            let body = record.body.clone().ok_or("Intent Solid has no evaluated body")?;
            if body.validate().is_err() {
                return Err("Intent Solid body does not validate".into());
            }
            let hash = jarvig_core::authoritative_body_hash(&body);
            if format!("{hash:016x}") != "6ceeea840b7bdef2" {
                return Err(format!("body hash {hash:016x} is not the frozen Intent Solid"));
            }
            if record.intent.len() != 65 || record.history.len() != 24 {
                return Err(format!("intent {} history {} is not the frozen tape", record.intent.len(), record.history.len()));
            }
            let legacy_record = world.authored_block(legacy).ok_or("Legacy Cube has no block")?.clone();
            let object_mesh = world.object_mesh(intent).ok_or("Intent Solid has no mesh")?;
            let legacy_mesh = world.object_mesh(legacy).ok_or("Legacy Cube has no mesh")?;
            let shared = world.meshes().get(object_mesh).ok_or("the control mesh is missing")?;
            if shared.vertex_count() != 144 || shared.index_count() / 3 != 48 {
                return Err(format!("control mesh is {} vertices and {} triangles", shared.vertex_count(), shared.index_count() / 3));
            }
            let set = world.derived_meshlets(object_mesh).ok_or("the control mesh has no meshlets")?.clone();
            let snapshot = world.extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let instance = snapshot.instances().iter().find(|item| item.entity == intent).cloned().ok_or("Intent Solid is not in the snapshot")?;
            let front = world.front_camera();
            let extracted = snapshot.camera(front.frame).ok_or("front camera missing")?.clone();
            let records: Vec<_> = set.meshlets.iter().map(jarvig_core::GpuMeshletRecord::from_meshlet).collect();
            let hierarchy = jarvig_core::build_cluster_hierarchy(&records);
            let leaf_triangles: Vec<u32> = set.meshlets.iter().map(|meshlet| meshlet.index_count / 3).collect();
            let leaf_flags = vec![1u32; set.meshlets.len()];
            let cut_at = |pixels: f32| {
                jarvig_core::cut_visible_hierarchy_for_pose(
                    &hierarchy,
                    &instance,
                    &extracted.pose,
                    extracted.vertical_fov_radians,
                    extracted.near_m,
                    16.0 / 9.0,
                    1080.0,
                    pixels,
                    &leaf_flags,
                    &leaf_triangles,
                )
                .map(|cut| cut.submitted_triangles)
                .map_err(|error| error.to_string())
            };
            let (camera_a, camera_b) = jarvig_core::frozen_observation_cameras(&body, extracted.vertical_fov_radians)?;
            if camera_a.viewport_width != VIEW_A_WIDTH as f32 || camera_b.viewport_width != VIEW_B_WIDTH as f32 || camera_a.viewport_height != VIEW_HEIGHT as f32 {
                return Err("frozen cameras were not 160x567 and 960x567".into());
            }
            let mut host = jarvig_core::ObservationHost::new();
            let product_a = host
                .observe(jarvig_core::ObservationRequest { entity: intent, body_hash: hash, revision: world.revision(), body: body.clone(), camera: camera_a.clone() })
                .map_err(|error| error.to_string())?
                .product;
            let product_b = host
                .observe(jarvig_core::ObservationRequest { entity: intent, body_hash: hash, revision: world.revision(), body: body.clone(), camera: camera_b.clone() })
                .map_err(|error| error.to_string())?
                .product;
            if product_a.admitted_face_ids != jarvig_core::FROZEN_A_ADMITTED.to_vec() || product_a.omitted_face_ids != jarvig_core::FROZEN_A_OMITTED.to_vec() {
                return Err(format!("observation A faces drifted: admitted {:?} omitted {:?}", product_a.admitted_face_ids, product_a.omitted_face_ids));
            }
            if product_b.admitted_face_ids != jarvig_core::FROZEN_B_ADMITTED.to_vec() || product_b.omitted_face_ids != jarvig_core::FROZEN_B_OMITTED.to_vec() {
                return Err(format!("observation B faces drifted: admitted {:?} omitted {:?}", product_b.admitted_face_ids, product_b.omitted_face_ids));
            }
            let hash_text = format!("{hash:016x}");
            if !frozen_key(&product_a.cache_key, "w160-h567", &hash_text) || !frozen_key(&product_b.cache_key, "w960-h567", &hash_text) {
                return Err(format!("an observation key left the frozen admission\nA {}\nB {}", product_a.cache_key, product_b.cache_key));
            }
            if !product_a.uncertain_face_ids.is_empty() || !product_b.uncertain_face_ids.is_empty() {
                return Err("an observation admitted a face as uncertain".into());
            }
            let slot = |face: u32| record.bound_slot(face);
            let pack_a = jarvig_core::pack_observation_gpu(&product_a, slot).map_err(|error| error.to_string())?;
            let pack_b = jarvig_core::pack_observation_gpu(&product_b, slot).map_err(|error| error.to_string())?;
            if !face_89_absent(&pack_a.face_89) || !face_89_admitted(&pack_b.face_89) {
                return Err("face 89 provenance did not match the no-weld proof".into());
            }
            if pack_a.constructed_triangles != pack_a.packed_triangles
                || pack_a.packed_triangles != pack_a.uploaded_triangles
                || pack_b.constructed_triangles != pack_b.packed_triangles
                || pack_b.packed_triangles != pack_b.uploaded_triangles
                || pack_a.discarded_after_construction != 0
                || pack_b.discarded_after_construction != 0
                || pack_a.discarded_during_pack != 0
                || pack_b.discarded_during_pack != 0
                || pack_a.discarded_after_upload != 0
                || pack_b.discarded_after_upload != 0
                || pack_a.expected_gpu_bytes != pack_a.packed_vertex_bytes + pack_a.packed_index_bytes
                || pack_b.expected_gpu_bytes != pack_b.packed_vertex_bytes + pack_b.packed_index_bytes
            {
                return Err("an observation pack did not reconcile constructed, packed, and uploaded triangles".into());
            }
            let solid = world.entity_world_pose(intent).map_err(|error| error.to_string())?;
            let pose_a = observation_pose(solid, camera_a.eye_local, camera_a.forward_local)?;
            let pose_b = observation_pose(solid, camera_b.eye_local, camera_b.forward_local)?;
            let camera = jarvig_core::Camera {
                frame: front.frame,
                vertical_fov_radians: camera_a.vertical_fov_radians,
                near_m: camera_a.near_m as f32,
            };
            (
                record,
                legacy_record,
                body,
                hash,
                object_mesh,
                legacy_mesh,
                set.meshlets.len(),
                cut_at(0.5)?,
                cut_at(4.0)?,
                camera_a,
                camera_b,
                product_a,
                product_b,
                pack_a,
                pack_b,
                pose_a,
                pose_b,
                camera,
                world.revision(),
                world.mesh_count(),
                world.entity_local_pose(intent).map_err(|error| error.to_string())?.translation,
                world.entity_local_pose(legacy).map_err(|error| error.to_string())?.translation,
                world.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0)),
                world.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0)),
                world.remember_entity(intent).map_err(|error| error.to_string())?.name,
                world.remember_entity(legacy).map_err(|error| error.to_string())?.name,
            )
        };
        let (
            record,
            legacy_record,
            body,
            hash,
            object_mesh,
            legacy_mesh,
            meshlet_len,
            cut_strict,
            cut_loose,
            camera_a,
            camera_b,
            product_a,
            product_b,
            pack_a,
            pack_b,
            pose_a,
            pose_b,
            camera,
            revision,
            mesh_count,
            pose,
            legacy_pose,
            collision_inside,
            collision_outside,
            name,
            legacy_name,
        ) = gathered;
        if cut_strict != 48 || cut_loose != 48 {
            return Err(format!("control cuts were {cut_strict} and {cut_loose}, not 48 and 48"));
        }
        let mesh_a = self.engine.world_mut().add_mesh(pack_a.mesh.clone());
        let mesh_b = self.engine.world_mut().add_mesh(pack_b.mesh.clone());
        let decoy = self.engine.world_mut().add_mesh(jarvig_core::near_triangle_mesh());
        if mesh_a == object_mesh || mesh_b == object_mesh || decoy == object_mesh || mesh_a == mesh_b {
            return Err("an observation mesh reused the object mesh id".into());
        }
        if self.engine.world().revision() != revision {
            return Err("add_mesh revised the world".into());
        }
        if self.engine.world().derived_meshlets(mesh_a).is_some() || self.engine.world().derived_meshlets(mesh_b).is_some() {
            return Err("an observation mesh built meshlets".into());
        }
        let target = self.target.ok_or("viewport target missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let right = {
            let renderer = self.renderer.as_mut().unwrap();
            renderer
                .create_view(RenderViewDesc {
                    label: "JARVIG.Perspective.B".into(),
                    target,
                    camera,
                    layout: NormalizedRect::FULL,
                    settings: RenderViewSettings::default(),
                })
                .map_err(|error| error.to_string())?
        };
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, Some(PixelRect { x: 0, y: 0, width: VIEW_A_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
            renderer
                .set_view_pixel_rect(right, Some(PixelRect { x: VIEW_A_WIDTH, y: 0, width: VIEW_B_WIDTH, height: VIEW_HEIGHT }))
                .map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_a) }).map_err(|error| error.to_string())?;
            renderer.update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_b) }).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(left, intent, Some(mesh_a)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, intent, Some(mesh_b)).map_err(|error| error.to_string())?;
        }
        let (rect_a, rect_b) = {
            let renderer = self.renderer.as_ref().unwrap();
            (renderer.viewport(left).map_err(|error| error.to_string())?, renderer.viewport(right).map_err(|error| error.to_string())?)
        };
        let Some(rect_a) = rect_a else { return Err("view A has no pixel viewport".into()); };
        let Some(rect_b) = rect_b else { return Err("view B has no pixel viewport".into()); };
        if rect_a != (PixelRect { x: 0, y: 0, width: VIEW_A_WIDTH, height: VIEW_HEIGHT }) || rect_b != (PixelRect { x: VIEW_A_WIDTH, y: 0, width: VIEW_B_WIDTH, height: VIEW_HEIGHT }) {
            return Err(format!("viewports were {}x{} and {}x{}, not 160x567 and 960x567", rect_a.width, rect_a.height, rect_b.width, rect_b.height));
        }
        let face_ids: Vec<u32> = body.faces.iter().map(|face| face.id).collect();
        let edge_ids: Vec<u32> = body.edges.iter().map(|edge| edge.id).collect();
        let vertex_ids: Vec<u32> = body.vertices.iter().map(|vertex| vertex.id).collect();
        let fan_89 = body.face_loop(89).map(|loop_| loop_.len().saturating_sub(2)).unwrap_or(0);
        if fan_89 != 3 {
            return Err(format!("face 89 authoritative fan is {fan_89}"));
        }
        {
            let lab = self.observation_lab.as_mut().unwrap();
            lab.intent = Some(intent);
            lab.legacy = Some(legacy);
            lab.object_mesh = Some(object_mesh);
            lab.legacy_mesh = Some(legacy_mesh);
            lab.mesh_a = Some(mesh_a);
            lab.mesh_b = Some(mesh_b);
            lab.decoy = Some(decoy);
            lab.right_view = Some(right);
            lab.key_a = product_a.cache_key.clone();
            lab.key_b = product_b.cache_key.clone();
            lab.pose_a = pose_a;
            lab.pose_b = pose_b;
            lab.camera = camera;
            lab.generation_a = product_a.generation_us;
            lab.generation_b = product_b.generation_us;
            lab.pack_a = Some(pack_a);
            lab.pack_b = Some(pack_b);
            lab.record = Some(record);
            lab.legacy_record = Some(legacy_record);
            lab.body_hash = hash;
            lab.revision = revision;
            lab.mesh_count = mesh_count;
            lab.pose = pose;
            lab.legacy_pose = legacy_pose;
            lab.collision_inside = collision_inside;
            lab.collision_outside = collision_outside;
            lab.name = name;
            lab.legacy_name = legacy_name;
            lab.face_ids = face_ids;
            lab.edge_ids = edge_ids;
            lab.vertex_ids = vertex_ids;
            lab.meshlet_len = meshlet_len;
            lab.control_gpu_bytes = Some(control_gpu_bytes);
            lab.cut_strict = cut_strict;
            lab.cut_loose = cut_loose;
            lab.panel = panel;
            lab.split = true;
            lab.stage = 1;
            lab.mark = self.frames;
        }
        self.note_obs("Experiment 3B: ON");
        self.note_obs("Experiment 3A frozen: PASS");
        self.note_obs("Intent-authority experiment: ON");
        self.note_obs("ADR-0074 accepted: NO");
        self.note_obs("Default renderer replaced: NO");
        self.note_obs("Admission changed: NO");
        self.note_obs("Vertices welded across authoritative faces: NO");
        self.note_obs(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_obs("Experiment 2 worker: NOT STARTED");
        self.note_obs("jarvig-realize-0: NOT STARTED");
        self.note_obs("jarvig-realize-1: NOT STARTED");
        self.note_obs("Editor worker cap raised: NO");
        self.note_obs(&format!("Graphics adapter: {}", self.graphics_name));
        self.note_obs(&format!("Graphics vendor: {:04x}", self.graphics_vendor));
        self.note_obs(&format!("Graphics device: {:04x}", self.graphics_device));
        self.note_obs(&format!("Graphics api: {}", self.graphics_api));
        self.note_obs(&format!("Editor panel: {}x{}", panel.0, panel.1));
        self.note_obs("Observation A viewport: 160x567");
        self.note_obs("Observation B viewport: 960x567");
        self.note_obs("Re-admitted at the editor panel size: NO");
        self.note_obs(&format!("Observation A eye_local: {}", jarvig_core::observation_eye_text(&camera_a)));
        self.note_obs(&format!("Observation A forward_local: {}", jarvig_core::observation_forward_text(&camera_a)));
        self.note_obs(&format!("Observation B eye_local: {}", jarvig_core::observation_eye_text(&camera_b)));
        self.note_obs(&format!("Observation B forward_local: {}", jarvig_core::observation_forward_text(&camera_b)));
        self.note_obs(&format!("Observation A cache_key: {}", self.observation_lab.as_ref().unwrap().key_a));
        self.note_obs(&format!("Observation B cache_key: {}", self.observation_lab.as_ref().unwrap().key_b));
        self.note_obs(&format!("Observation A generation_us: {}", self.observation_lab.as_ref().unwrap().generation_a));
        self.note_obs(&format!("Observation B generation_us: {}", self.observation_lab.as_ref().unwrap().generation_b));
        self.note_obs("3A generation times compared as a speedup: NO");
        self.note_obs(&format!("Control mesh id: {}", object_mesh.0));
        self.note_obs(&format!("Observation A mesh id: {}", mesh_a.0));
        self.note_obs(&format!("Observation B mesh id: {}", mesh_b.0));
        self.note_obs(&format!("Decoy mesh id: {}", decoy.0));
        self.note_obs("Observation mesh is the object mesh: NO");
        self.note_obs("Observation meshlets: 0");
        self.note_obs(&format!("Control shared vertices: 144"));
        self.note_obs(&format!("Control shared triangles: 48"));
        self.note_obs(&format!("Control cut 0.5 px triangles: {cut_strict}"));
        self.note_obs(&format!("Control cut 4.0 px triangles: {cut_loose}"));
        self.note_obs("Experiment uses that cut: NO");
        self.note_obs(&format!("Control mesh GPU_bytes before: {control_gpu_bytes}"));
        self.note_obs("Shadow, contact, and probe capture use the control mesh: YES");
        self.note_obs("Color pass uses the observation mesh: YES");
        self.note_obs("queue: NOT USED");
        Ok(())
    }

    fn obs_stage_present(&mut self) -> Result<(), String> {
        if !self.observation_lab.as_ref().unwrap().proved {
            if !self.obs_frame_matches(true)? {
                if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) > 180 {
                    return Err("the simultaneous observation frame did not upload".into());
                }
                return Ok(());
            }
            self.obs_note_uploads()?;
            self.obs_note_passes("simultaneous")?;
            self.obs_capture("shot-obs-3b-simultaneous.png")?;
            self.obs_reject_stale()?;
            let lab = self.observation_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.obs_frame_matches(true)? {
                return Err("a later frame substituted an observation buffer".into());
            }
            if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) == 1 {
                self.note_obs("Next frame kept observation A on view A and observation B on view B: YES");
            }
            return Ok(());
        }
        self.obs_retire_mesh(self.observation_lab.as_ref().unwrap().mesh_a.ok_or("mesh A missing")?)?;
        self.observation_lab.as_mut().unwrap().mesh_a = None;
        self.observation_lab.as_mut().unwrap().stage = 2;
        self.observation_lab.as_mut().unwrap().mark = self.frames;
        self.note_obs("Observation A destroy requested");
        Ok(())
    }

    fn obs_stage_a_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh_a = *self.observation_lab.as_ref().unwrap().retired.last().ok_or("mesh A was not retired")?;
        let mesh_b = self.observation_lab.as_ref().unwrap().mesh_b.ok_or("mesh B missing")?;
        let gone = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_a).is_none();
        let bytes_b = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_b);
        if !gone || bytes_b.is_none() {
            if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) > 90 {
                return Err("destroying observation A did not leave B resident and A evicted".into());
            }
            return Ok(());
        }
        if !self.observation_lab.as_ref().unwrap().captured_a {
            let left = self.viewport_view.ok_or("view A missing")?;
            let right = self.observation_lab.as_ref().unwrap().right_view.ok_or("view B missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            let control = self.observation_lab.as_ref().unwrap().object_mesh.unwrap();
            if color_contains(&draws, left, mesh_a)
                || !color_contains(&draws, left, control)
                || !color_contains(&draws, right, mesh_b)
                || color_contains(&draws, right, control)
            {
                return Err("the frame after destroying A still mixed the observation meshes".into());
            }
            self.note_obs("Observation A GPU_bytes after destroy: GONE");
            self.note_obs(&format!("Observation B GPU_bytes while A is gone: {}", bytes_b.unwrap()));
            self.note_obs("Observation B face 89 still in B's uploaded mesh: YES");
            self.note_obs("View A color pass after observation A was destroyed: control mesh");
            self.note_obs("Face 89 pixels in that view are the control mesh, not observation A's product: YES");
            self.obs_capture("shot-obs-3b-a-destroyed.png")?;
            self.observation_lab.as_mut().unwrap().captured_a = true;
            self.observation_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.obs_retire_mesh(mesh_b)?;
        self.observation_lab.as_mut().unwrap().mesh_b = None;
        self.observation_lab.as_mut().unwrap().stage = 3;
        self.observation_lab.as_mut().unwrap().mark = self.frames;
        self.note_obs("Observation B destroy requested");
        Ok(())
    }

    fn obs_stage_b_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let meshes = self.observation_lab.as_ref().unwrap().retired.clone();
        let gone = meshes.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        if !gone {
            if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) > 90 {
                return Err("observation B stayed resident after destroy".into());
            }
            return Ok(());
        }
        if !self.observation_lab.as_ref().unwrap().captured_b {
            self.note_obs("Observation B GPU_bytes after destroy: GONE");
            self.obs_capture("shot-obs-3b-b-destroyed.png")?;
            self.observation_lab.as_mut().unwrap().captured_b = true;
            self.observation_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        let decoy = self.observation_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        self.obs_retire_mesh(decoy)?;
        self.observation_lab.as_mut().unwrap().decoy = None;
        let right = self.observation_lab.as_ref().unwrap().right_view.ok_or("view B missing")?;
        let left = self.viewport_view.ok_or("view A missing")?;
        let entity = self.observation_lab.as_ref().unwrap().intent.ok_or("intent missing")?;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_mesh_override(left, entity, None).map_err(|error| error.to_string())?;
            renderer.set_view_pixel_rect(left, None).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::FULL), settings: None, pose: None }).map_err(|error| error.to_string())?;
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
        }
        let lab = self.observation_lab.as_mut().unwrap();
        lab.split = false;
        lab.right_view = None;
        lab.stage = 4;
        lab.mark = self.frames;
        self.note_obs("Perspective restored to one full view");
        Ok(())
    }

    fn obs_stage_intact(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.observation_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let lab = self.observation_lab.as_ref().unwrap();
        let intent = lab.intent.ok_or("intent missing")?;
        let legacy = lab.legacy.ok_or("legacy missing")?;
        let control = lab.object_mesh.ok_or("control missing")?;
        let record_same = self.engine.world().authored_block(intent).as_ref() == lab.record.as_ref();
        let legacy_same = self.engine.world().authored_block(legacy).as_ref() == lab.legacy_record.as_ref();
        let pose_same = self.engine.world().entity_local_pose(intent).ok().map(|pose| pose.translation) == Some(lab.pose);
        let legacy_pose_same = self.engine.world().entity_local_pose(legacy).ok().map(|pose| pose.translation) == Some(lab.legacy_pose);
        let mesh_same = self.engine.world().object_mesh(intent) == Some(control);
        let legacy_mesh_same = self.engine.world().object_mesh(legacy) == lab.legacy_mesh;
        let count_same = self.engine.world().mesh_count() == lab.mesh_count;
        let revision_same = self.engine.world().revision() == lab.revision;
        let inside = self.engine.world().separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
        let outside = self.engine.world().separate_from_blocks(Vec3::new(0.0, 1.0, 40.0));
        let collision_same = inside == lab.collision_inside && outside == lab.collision_outside;
        let live = self.engine.world().authored_block(intent);
        let (hash_same, intent_same, history_same, ids_same, fan_same) = match live.as_ref().and_then(|record| record.body.as_ref()) {
            Some(body) => {
                let hash_same = jarvig_core::authoritative_body_hash(body) == lab.body_hash;
                let ids_same = body.faces.iter().map(|face| face.id).eq(lab.face_ids.iter().copied())
                    && body.edges.iter().map(|edge| edge.id).eq(lab.edge_ids.iter().copied())
                    && body.vertices.iter().map(|vertex| vertex.id).eq(lab.vertex_ids.iter().copied());
                let fan_same = body.face_loop(89).map(|loop_| loop_.len().saturating_sub(2)) == Some(3);
                (hash_same, live.as_ref().map(|record| record.intent.len()) == Some(65), live.as_ref().map(|record| record.history.len()) == Some(24), ids_same, fan_same)
            }
            None => (false, false, false, false, false),
        };
        let name_same = self.engine.world().remember_entity(intent).ok().map(|entity| entity.name) == Some(lab.name.clone());
        let meshlets = self.engine.world().derived_meshlets(control);
        let meshlets_same = meshlets.is_some_and(|set| set.meshlets.len() == lab.meshlet_len);
        let retired = lab.retired.clone();
        let orphans_gone = retired.iter().all(|mesh| self.engine.world().meshes().get(*mesh).is_none());
        let gpu_gone = retired.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        let control_after = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        let control_match = control_after == lab.control_gpu_bytes && control_after.is_some();
        let views = self.renderer.as_ref().unwrap().view_count() == 1;
        let element = if self.selected_vertex.is_some() {
            "vertex"
        } else if self.selected_edge.is_some() {
            "edge"
        } else if self.selected_body_face.is_some() {
            "face"
        } else if self.selection.primary_entity().is_some() {
            "entity"
        } else {
            "NONE"
        };
        let graphics_ok = self.graphics_vendor == 0x8086 && self.graphics_device == 0x9b41 && self.graphics_api == "dx12";
        let presented = self.renderer.as_ref().unwrap().presented_frames() > 0;
        let stale_ok = self.observation_lab.as_ref().unwrap().discarded_stale_gpu >= 1;
        let object_ok = record_same
            && legacy_same
            && pose_same
            && legacy_pose_same
            && mesh_same
            && legacy_mesh_same
            && count_same
            && revision_same
            && collision_same
            && hash_same
            && intent_same
            && history_same
            && ids_same
            && fan_same
            && name_same
            && meshlets_same
            && orphans_gone
            && gpu_gone
            && control_match
            && views
            && element == "NONE"
            && graphics_ok
            && presented
            && stale_ok;
        self.obs_capture("shot-obs-3b-object.png")?;
        self.note_obs(&format!("Editor frame: {}", self.frames));
        self.note_obs(&format!("Presented frames: {}", self.renderer.as_ref().unwrap().presented_frames()));
        self.note_obs(&format!("Frame: {}", if presented { "PRESENTED" } else { "NOT PRESENTED" }));
        self.note_obs(&format!("Graphics is Intel UHD DX12 8086:9b41: {}", yes_no(graphics_ok)));
        self.note_obs(&format!("Authoritative record unchanged: {}", yes_no(record_same && legacy_same)));
        self.note_obs(&format!("source_body_hash: {:016x}", self.observation_lab.as_ref().unwrap().body_hash));
        self.note_obs(&format!("source_body_hash unchanged: {}", yes_no(hash_same)));
        self.note_obs(&format!("intent entries: {}", live.as_ref().map(|record| record.intent.len()).unwrap_or(0)));
        self.note_obs(&format!("history entries: {}", live.as_ref().map(|record| record.history.len()).unwrap_or(0)));
        self.note_obs(&format!("intent unchanged: {}", yes_no(intent_same)));
        self.note_obs(&format!("history unchanged: {}", yes_no(history_same)));
        self.note_obs(&format!("collision before_after: {}", if collision_same { "MATCH" } else { "DIFFER" }));
        self.note_obs(&format!("materials before_after: {}", if record_same { "MATCH" } else { "DIFFER" }));
        self.note_obs(&format!("semantic identity before_after: {}", if ids_same && name_same { "MATCH" } else { "DIFFER" }));
        self.note_obs(&format!("face 89 still on the body: {}", yes_no(fan_same)));
        self.note_obs(&format!("face 89 authoritative fan: {}", if fan_same { "3" } else { "CHANGED" }));
        self.note_obs(&format!("Object mesh unchanged: {}", yes_no(mesh_same)));
        self.note_obs(&format!("Control meshlets unchanged: {}", yes_no(meshlets_same)));
        self.note_obs(&format!("World revision unchanged: {}", yes_no(revision_same)));
        self.note_obs(&format!("Library mesh count restored: {}", yes_no(count_same)));
        self.note_obs(&format!("Observation meshes removed: {}", yes_no(orphans_gone)));
        self.note_obs(&format!("Observation GPU buffers after destroy: {}", if gpu_gone { "EVICTED" } else { "RESIDENT" }));
        self.note_obs(&format!("Control mesh GPU_bytes after: {}", control_after.map(|value| value.to_string()).unwrap_or_else(|| "GONE".into())));
        self.note_obs(&format!("Control mesh GPU_bytes match: {}", yes_no(control_match)));
        self.note_obs(&format!("Render view count restored: {}", yes_no(views)));
        self.note_obs(&format!("Editor element selection: {element}"));
        self.note_obs(&format!("stale uploads rejected: {}", self.observation_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_obs(&format!("B received A's mesh: NO"));
        self.note_obs("Backface omission: NO");
        self.note_obs("Same-solid occlusion: NO");
        self.note_obs("Edge collapse: NO");
        self.note_obs("Simplification of admitted faces: NO");
        self.note_obs("Renderer replacement: NO");
        self.note_obs("ADR-0074 accepted: NO");
        self.note_obs(&format!("Authoritative object after both products destroyed: {}", if object_ok { "INTACT" } else { "CHANGED" }));
        let passed = object_ok && self.observation_lab.as_ref().unwrap().report.contains("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_obs(&format!("Experiment 3B: {}", if passed { "PASS" } else { "FAIL" }));
        self.observation_write_report();
        self.observation_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 3B did not meet the presented-frame success condition".into());
        }
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn obs_note_uploads(&mut self) -> Result<(), String> {
        let (mesh_a, mesh_b, pack_a, pack_b) = {
            let lab = self.observation_lab.as_ref().unwrap();
            (lab.mesh_a.unwrap(), lab.mesh_b.unwrap(), lab.pack_a.clone().unwrap(), lab.pack_b.clone().unwrap())
        };
        let record_a = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_a).ok_or("observation A was not uploaded")?;
        let record_b = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_b).ok_or("observation B was not uploaded")?;
        let prove = |label: &str, pack: &jarvig_core::ObservationGpuPack, record: jarvig_renderer::MeshUploadRecord| -> Result<(), String> {
            if record.vertex_create_bytes != pack.packed_vertex_bytes || record.index_create_bytes != pack.packed_index_bytes || record.gpu_bytes != pack.expected_gpu_bytes {
                return Err(format!("{label} create_buffer bytes did not match the provenance total"));
            }
            if record.vertex_create_bytes + record.index_create_bytes != record.gpu_bytes {
                return Err(format!("{label} GpuMesh.bytes is not the sum of the create_buffer sizes"));
            }
            let uploaded = record.index_create_bytes / u64::from(pack.index_format.byte_size()) / 3;
            if uploaded != u64::from(pack.packed_triangles) {
                return Err(format!("{label} uploaded a different triangle count"));
            }
            Ok(())
        };
        prove("Observation A", &pack_a, record_a)?;
        prove("Observation B", &pack_b, record_b)?;
        self.note_obs(&jarvig_core::format_observation_gpu(
            "Observation A",
            &pack_a,
            Some(jarvig_core::ObservationUploadProof { gpu_mesh_bytes: record_a.gpu_bytes, vertex_create_bytes: record_a.vertex_create_bytes, index_create_bytes: record_a.index_create_bytes }),
        ));
        self.note_obs(&jarvig_core::format_observation_gpu(
            "Observation B",
            &pack_b,
            Some(jarvig_core::ObservationUploadProof { gpu_mesh_bytes: record_b.gpu_bytes, vertex_create_bytes: record_b.vertex_create_bytes, index_create_bytes: record_b.index_create_bytes }),
        ));
        self.note_obs("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_obs("discarded_after_upload: 0");
        self.note_obs(&format!("Acquires this frame: {}", self.renderer.as_ref().unwrap().acquires_last_frame()));
        self.note_obs(&format!("Presents this frame: {}", self.renderer.as_ref().unwrap().presents_last_frame()));
        if self.renderer.as_ref().unwrap().acquires_last_frame() != 1 || self.renderer.as_ref().unwrap().presents_last_frame() != 1 {
            return Err("the observation frame did not use one acquire and one present".into());
        }
        Ok(())
    }

    fn obs_note_passes(&mut self, label: &str) -> Result<(), String> {
        let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
        let mesh_a = self.observation_lab.as_ref().unwrap().mesh_a.unwrap();
        let mesh_b = self.observation_lab.as_ref().unwrap().mesh_b.unwrap();
        let control = self.observation_lab.as_ref().unwrap().object_mesh.unwrap();
        let decoy = self.observation_lab.as_ref().unwrap().decoy.unwrap();
        let shadow = pass_meshes(&draws, "shadow");
        let shadow_cached = pass_meshes(&draws, "shadow-cached");
        let contact = pass_meshes(&draws, "contact");
        let probe = pass_meshes(&draws, "probe");
        let casters = if shadow.is_empty() { shadow_cached.clone() } else { shadow.clone() };
        self.note_obs(&format!("{label} shadow draws: {}", id_list(&shadow)));
        self.note_obs(&format!("{label} shadow casters: {}", id_list(&casters)));
        self.note_obs(&format!("{label} shadow maps redrawn this frame: {}", yes_no(!shadow.is_empty())));
        self.note_obs(&format!("{label} contact meshes: {}", id_list(&contact)));
        self.note_obs(&format!("{label} probe meshes: {}", id_list(&probe)));
        let observation_in = |list: &[MeshId]| list.contains(&mesh_a) || list.contains(&mesh_b) || list.contains(&decoy);
        if observation_in(&shadow) || observation_in(&shadow_cached) || observation_in(&contact) || observation_in(&probe) {
            return Err("an observation mesh was drawn by shadow, contact, or probe capture".into());
        }
        if !casters.contains(&control) {
            return Err("the control mesh was not the shadow caster".into());
        }
        if !contact.contains(&control) {
            return Err("the control mesh was not in the contact prepass".into());
        }
        self.note_obs("Observation mesh in shadow draws: NO");
        self.note_obs("Observation mesh in contact draws: NO");
        self.note_obs("Observation mesh in probe draws: NO");
        self.note_obs("Control mesh in shadow casters: YES");
        self.note_obs("A shadow of the full solid is the control caster, not an observation color mesh: YES");
        Ok(())
    }

    fn obs_frame_matches(&mut self, require_both: bool) -> Result<bool, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let left = self.viewport_view.ok_or("view A missing")?;
        let right = match self.observation_lab.as_ref().unwrap().right_view {
            Some(view) => view,
            None => return Ok(false),
        };
        let mesh_a = self.observation_lab.as_ref().unwrap().mesh_a;
        let mesh_b = self.observation_lab.as_ref().unwrap().mesh_b;
        let control = self.observation_lab.as_ref().unwrap().object_mesh.ok_or("control missing")?;
        let decoy = self.observation_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        let draws = renderer.frame_mesh_draws();
        let left_rect = renderer.viewport(left).ok().flatten();
        let right_rect = renderer.viewport(right).ok().flatten();
        let rects_ok = left_rect == Some(PixelRect { x: 0, y: 0, width: VIEW_A_WIDTH, height: VIEW_HEIGHT })
            && right_rect == Some(PixelRect { x: VIEW_A_WIDTH, y: 0, width: VIEW_B_WIDTH, height: VIEW_HEIGHT });
        if !rects_ok {
            return Ok(false);
        }
        let (Some(mesh_a), Some(mesh_b)) = (mesh_a, mesh_b) else {
            return Ok(false);
        };
        if !require_both {
            return Ok(true);
        }
        let ready = color_contains(draws, left, mesh_a)
            && !color_contains(draws, left, mesh_b)
            && !color_contains(draws, left, control)
            && !color_contains(draws, left, decoy)
            && color_contains(draws, right, mesh_b)
            && !color_contains(draws, right, mesh_a)
            && !color_contains(draws, right, control)
            && !color_contains(draws, right, decoy)
            && renderer.mesh_upload_record(mesh_a).is_some()
            && renderer.mesh_upload_record(mesh_b).is_some();
        Ok(ready)
    }

    fn obs_reject_stale(&mut self) -> Result<(), String> {
        let (left, right, mesh_a, mesh_b, decoy, key_a, key_b, entity) = {
            let lab = self.observation_lab.as_ref().unwrap();
            (
                self.viewport_view.ok_or("view A missing")?,
                lab.right_view.ok_or("view B missing")?,
                lab.mesh_a.unwrap(),
                lab.mesh_b.unwrap(),
                lab.decoy.unwrap(),
                lab.key_a.clone(),
                lab.key_b.clone(),
                lab.intent.unwrap(),
            )
        };
        self.obs_bind_if_current(left, entity, mesh_b, &key_b, &key_a)?;
        self.obs_bind_if_current(right, entity, mesh_a, &key_a, &key_b)?;
        self.obs_bind_if_current(left, entity, decoy, "stale-decoy", &key_a)?;
        if self.renderer.as_ref().unwrap().resident_mesh_bytes(decoy).is_some() {
            return Err("the refused decoy was uploaded".into());
        }
        if self.observation_lab.as_ref().unwrap().discarded_stale_gpu < 1 {
            return Err("a stale observation upload was not rejected".into());
        }
        self.note_obs("Decoy mesh GPU_bytes: NOT UPLOADED");
        self.note_obs(&format!("stale uploads rejected: {}", self.observation_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_obs("Cross-view buffer substitution: NO");
        self.note_obs("Decoy mesh bound: NO");
        Ok(())
    }

    fn obs_bind_if_current(&mut self, view: RenderViewId, entity: jarvig_core::EntityId, mesh: MeshId, product_key: &str, current_key: &str) -> Result<(), String> {
        if product_key != current_key {
            self.observation_lab.as_mut().unwrap().discarded_stale_gpu = self.observation_lab.as_ref().unwrap().discarded_stale_gpu.saturating_add(1);
            return Ok(());
        }
        self.renderer.as_mut().unwrap().set_view_mesh_override(view, entity, Some(mesh)).map_err(|error| error.to_string())
    }

    fn obs_retire_mesh(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.observation_lab.as_ref().unwrap().object_mesh.ok_or("control missing")?;
        if mesh == control {
            return Err("refusing to retire the object mesh".into());
        }
        if !self.engine.world_mut().retire_unreferenced_mesh(mesh) {
            return Err(format!("observation mesh {} stayed referenced", mesh.0));
        }
        if self.engine.world().revision() != self.observation_lab.as_ref().unwrap().revision {
            return Err("retiring an observation mesh revised the world".into());
        }
        self.observation_lab.as_mut().unwrap().retired.push(mesh);
        Ok(())
    }
}

fn observation_pose(solid: ResolvedPose, eye: [f64; 3], forward: [f64; 3]) -> Result<ResolvedPose, String> {
    let world_eye = solid.rotation.rotate(Vec3::new(eye[0], eye[1], eye[2])) + solid.translation;
    let world_forward = solid.rotation.rotate(Vec3::new(forward[0], forward[1], forward[2]));
    let up = solid.rotation.rotate(Vec3::new(0.0, 1.0, 0.0));
    Ok(ResolvedPose { translation: world_eye, rotation: look_rotation(world_forward, up)? })
}

fn look_rotation(forward: Vec3, up_hint: Vec3) -> Result<Quat, String> {
    let forward = unit(forward)?;
    let right = unit(forward.cross(up_hint))?;
    let up = right.cross(forward);
    let rotation = quat_from_columns(right, up, Vec3::new(-forward.x, -forward.y, -forward.z))?;
    let check = rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
    let delta = (check.x - forward.x).abs() + (check.y - forward.y).abs() + (check.z - forward.z).abs();
    if delta > 1.0e-6 {
        return Err(format!("the view rotation does not look along the observation forward ({delta})"));
    }
    Ok(rotation)
}

fn unit(value: Vec3) -> Result<Vec3, String> {
    let length = (value.x * value.x + value.y * value.y + value.z * value.z).sqrt();
    if !length.is_finite() || length < 1.0e-12 {
        return Err("a camera axis was degenerate".into());
    }
    Ok(value.scale(1.0 / length))
}

fn quat_from_columns(x_axis: Vec3, y_axis: Vec3, z_axis: Vec3) -> Result<Quat, String> {
    let (m00, m01, m02) = (x_axis.x, y_axis.x, z_axis.x);
    let (m10, m11, m12) = (x_axis.y, y_axis.y, z_axis.y);
    let (m20, m21, m22) = (x_axis.z, y_axis.z, z_axis.z);
    let trace = m00 + m11 + m22;
    let quat = if trace > 0.0 {
        let scale = (trace + 1.0).sqrt() * 2.0;
        Quat { w: 0.25 * scale, x: (m21 - m12) / scale, y: (m02 - m20) / scale, z: (m10 - m01) / scale }
    } else if m00 > m11 && m00 > m22 {
        let scale = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        Quat { w: (m21 - m12) / scale, x: 0.25 * scale, y: (m01 + m10) / scale, z: (m02 + m20) / scale }
    } else if m11 > m22 {
        let scale = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        Quat { w: (m02 - m20) / scale, x: (m01 + m10) / scale, y: 0.25 * scale, z: (m12 + m21) / scale }
    } else {
        let scale = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        Quat { w: (m10 - m01) / scale, x: (m02 + m20) / scale, y: (m12 + m21) / scale, z: 0.25 * scale }
    };
    if ![quat.x, quat.y, quat.z, quat.w].into_iter().all(|value| value.is_finite()) {
        return Err("the view quaternion was not finite".into());
    }
    Ok(quat)
}

fn color_contains(draws: &[jarvig_renderer::FrameMeshDraw], view: RenderViewId, mesh: MeshId) -> bool {
    draws.iter().any(|draw| draw.pass == "color" && draw.view == Some(view) && draw.mesh == mesh)
}

fn pass_meshes(draws: &[jarvig_renderer::FrameMeshDraw], pass: &str) -> Vec<MeshId> {
    let mut meshes = Vec::new();
    for draw in draws.iter().filter(|draw| draw.pass == pass) {
        if !meshes.contains(&draw.mesh) {
            meshes.push(draw.mesh);
        }
    }
    meshes
}

fn id_list(meshes: &[MeshId]) -> String {
    if meshes.is_empty() {
        "NONE".into()
    } else {
        meshes.iter().map(|mesh| mesh.0.to_string()).collect::<Vec<_>>().join(",")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}

fn frozen_key(key: &str, viewport: &str, body_hash: &str) -> bool {
    key.contains(viewport) && key.contains("px500") && key.contains("f0-0--1") && key.contains(body_hash)
}

fn face_89_absent(account: &jarvig_core::FaceGpuAccount) -> bool {
    account.cpu_triangles == 0
        && account.packed_triangles == 0
        && account.packed_vertices == 0
        && account.index_count == 0
        && account.vertex_bytes == 0
        && account.index_bytes == 0
        && account.total_gpu_bytes == 0
}

fn face_89_admitted(account: &jarvig_core::FaceGpuAccount) -> bool {
    account.cpu_triangles == 3
        && account.packed_triangles == 3
        && account.packed_vertices == 9
        && account.index_count == 9
        && account.vertex_bytes == 540
        && account.index_bytes > 0
        && account.total_gpu_bytes > 0
}
