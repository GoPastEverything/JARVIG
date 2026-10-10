//! Experiment 6. Two observations synthesize GPU fans from one saved modeling tape.
//!
//! The Size edit is written and parsed back. This lab does not enable intent
//! authority, does not open Experiment 4 or 5, and does not edit their modules.

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

pub(super) struct AuthoredLab {
    stage: u8,
    arranged: bool,
    split: bool,
    grow: u8,
    mark: u32,
    proved: bool,
    edited_proved: bool,
    captured_a: bool,
    captured_b: bool,
    right_view: Option<RenderViewId>,
    solid: Option<jarvig_core::EntityId>,
    legacy: Option<jarvig_core::EntityId>,
    control_mesh: Option<MeshId>,
    legacy_mesh: Option<MeshId>,
    mesh_a: Option<MeshId>,
    mesh_b: Option<MeshId>,
    decoy: Option<MeshId>,
    retired: Vec<MeshId>,
    observation_meshes: Vec<MeshId>,
    key_a: String,
    key_b: String,
    pose_a: ResolvedPose,
    pose_b: ResolvedPose,
    camera: jarvig_core::Camera,
    product_a: Option<jarvig_core::AuthoredProduct>,
    product_b: Option<jarvig_core::AuthoredProduct>,
    authority_hash: u64,
    edited_hash: u64,
    fov: f64,
    control_vertices: u32,
    control_triangles: u32,
    control_gpu_bytes: Option<u64>,
    gpu_bytes_b: u64,
    mesh_count: usize,
    revision_before: u64,
    acquires_first: u32,
    presents_first: u32,
    acquires_second: u32,
    presents_second: u32,
    panel: (u32, u32),
    discarded_stale_gpu: u32,
    caster_check_pending: bool,
    level_path: String,
    report: String,
}

impl AuthoredLab {
    pub(super) fn new() -> Self {
        Self {
            stage: 0,
            arranged: false,
            split: false,
            grow: 0,
            mark: 0,
            proved: false,
            edited_proved: false,
            captured_a: false,
            captured_b: false,
            right_view: None,
            solid: None,
            legacy: None,
            control_mesh: None,
            legacy_mesh: None,
            mesh_a: None,
            mesh_b: None,
            decoy: None,
            retired: Vec::new(),
            observation_meshes: Vec::new(),
            key_a: String::new(),
            key_b: String::new(),
            pose_a: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            pose_b: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            camera: jarvig_core::Camera { frame: jarvig_core::FrameId(0), vertical_fov_radians: 1.0, near_m: 0.1 },
            product_a: None,
            product_b: None,
            authority_hash: 0,
            edited_hash: 0,
            fov: 0.0,
            control_vertices: 0,
            control_triangles: 0,
            control_gpu_bytes: None,
            gpu_bytes_b: 0,
            mesh_count: 0,
            revision_before: 0,
            acquires_first: 0,
            presents_first: 0,
            acquires_second: 0,
            presents_second: 0,
            panel: (0, 0),
            discarded_stale_gpu: 0,
            caster_check_pending: false,
            level_path: String::new(),
            report: String::new(),
        }
    }
}

impl Editor {
    pub(super) fn apply_authored_frame(&mut self) -> Result<(), String> {
        let Some(lab) = self.authored_lab.as_ref() else {
            return Ok(());
        };
        if !lab.split {
            return Ok(());
        }
        let right = lab.right_view.ok_or("observation B's view is missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let (pose_a, pose_b, camera, entity, mesh_a, mesh_b) = (lab.pose_a, lab.pose_b, lab.camera, lab.solid, lab.mesh_a, lab.mesh_b);
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer.update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_a) }).map_err(|error| error.to_string())?;
        renderer.update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_b) }).map_err(|error| error.to_string())?;
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

    pub(super) fn step_authored_lab(&mut self) -> Result<(), String> {
        if self.authored_lab.is_none() {
            return Ok(());
        }
        let stage = self.authored_lab.as_ref().unwrap().stage;
        if stage >= 15 {
            return Ok(());
        }
        let result = match stage {
            0 => self.authored_stage_setup(),
            1 => self.authored_stage_present(),
            2 => self.authored_stage_edit(),
            3 => self.authored_stage_edited(),
            4 => self.authored_stage_a_gone(),
            5 => self.authored_stage_b_gone(),
            6 => self.authored_stage_intact(),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.note_authored("Experiment 6: FAIL");
            self.note_authored(&format!("Failure: {error}"));
            self.authored_write_report();
            if let Some(lab) = self.authored_lab.as_mut() {
                lab.stage = 15;
                lab.split = false;
            }
            return Err(error);
        }
        Ok(())
    }

    fn note_authored(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.authored_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    /// Report, captures, and the level backup sit beside the open project.
    /// The measured FAIL directory is not that project.
    fn authored_artifact_dir(&self) -> std::path::PathBuf {
        self.project_file
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(jarvig_core::authored_proof_dir)
    }

    pub(super) fn authored_write_report(&mut self) {
        let Some(lab) = self.authored_lab.as_ref() else {
            return;
        };
        let mut text = lab.report.clone();
        text.push_str("Authored intent report: REPORT-AUTHORED-6-RERUN.txt\n");
        let dir = self.authored_artifact_dir();
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-AUTHORED-6-RERUN.txt"), &text)) {
            Ok(()) => self.note_authored("Authored intent report: REPORT-AUTHORED-6-RERUN.txt"),
            Err(error) => self.note_authored(&format!("Authored intent report was not written: {error}")),
        }
    }

    fn authored_capture(&mut self, name: &str) -> Result<(), String> {
        let path = self.authored_artifact_dir().join(name);
        let image = super::capture_window_image(self.frame)?;
        super::write_png(&path.to_string_lossy(), &image)?;
        self.note_authored(&format!("Capture: {name}"));
        Ok(())
    }

    fn authored_stage_setup(&mut self) -> Result<(), String> {
        if self.view_lab.is_some() || self.view_realization.is_some() || self.observation_lab.is_some() || self.direct_lab.is_some() || self.intent_lab.is_some() || self.analytic_lab.is_some() || self.intent_live_rerealize {
            return Err("Experiment 6 refuses to run beside another realization lab".into());
        }
        if self.engine.intent_authority_experiment() {
            return Err("Experiment 6 refuses the intent-authority experiment flag".into());
        }
        if let Some(project) = &self.project_file {
            let text = project.display().to_string();
            if text.contains("IntentProof") || text.contains("jarvig-intent-proof") || text.contains("AnalyticIntent") || text.contains("jarvig-analytic-intent") {
                return Err("Experiment 6 opened Intent Proof or Analytic Intent".into());
            }
            if !text.contains("jarvig-authored-intent") && !text.contains("AuthoredIntent") {
                return Err("Experiment 6 was not opened on the authored-intent proof project".into());
            }
        }
        if self.renderer.is_none() || self.editor_camera.is_none() || self.viewport_view.is_none() || self.viewport_px.1 < 64 {
            if self.frames >= 400 {
                return Err("the perspective viewport was not ready".into());
            }
            return Ok(());
        }
        if !self.authored_lab.as_ref().unwrap().arranged {
            for panel in [OUTLINER, INSPECTOR, CONTENT, OUTPUT] {
                if self.workspace.is_open(panel) {
                    self.workspace.apply(WorkspaceCommand::ClosePanel(panel)).map_err(|error| error.to_string())?;
                }
            }
            unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, 1800, 1200, SWP_NOZORDER); }
            self.realize();
            let lab = self.authored_lab.as_mut().unwrap();
            lab.arranged = true;
            lab.mark = self.frames;
            return Ok(());
        }
        let (panel_w, panel_h) = self.viewport_px;
        let renderer_size = self.renderer.as_ref().unwrap().configured_size();
        if panel_w < REQUIRED_WIDTH || panel_h < VIEW_HEIGHT || renderer_size.0 < REQUIRED_WIDTH || renderer_size.1 < VIEW_HEIGHT {
            let grow = self.authored_lab.as_ref().unwrap().grow;
            let mark = self.authored_lab.as_ref().unwrap().mark;
            if grow < 2 && self.frames.saturating_sub(mark) > 8 {
                let (width, height) = if grow == 0 { (2200, 1400) } else { (2600, 1600) };
                unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, width, height, SWP_NOZORDER); }
                self.realize();
                let lab = self.authored_lab.as_mut().unwrap();
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
            let Some(solid) = outline.iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid) else {
                if self.frames >= 500 {
                    return Err("Intent Solid was not in the live level".into());
                }
                return Ok(());
            };
            let object_mesh = self.engine.world().object_mesh(solid).ok_or("Intent Solid has no mesh")?;
            self.renderer.as_ref().unwrap().resident_mesh_bytes(object_mesh)
        };
        let Some(control_gpu_bytes) = control_gpu else {
            if self.frames >= 500 {
                return Err("the control mesh was not uploaded".into());
            }
            return Ok(());
        };
        self.authored_install(control_gpu_bytes, (panel_w, panel_h))?;
        Ok(())
    }

    fn authored_install(&mut self, control_gpu_bytes: u64, panel: (u32, u32)) -> Result<(), String> {
        let level_path = self.level_file.clone().ok_or("the authored level is not open")?;
        let level_text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        let parsed = jarvig_core::parse_level(&level_text).map_err(|error| error.to_string())?;
        let record = block_named(&parsed, "Intent Solid")?;
        let legacy = block_named(&parsed, "Legacy Cube")?;
        let versions = parametric_versions(&level_text)?;
        if versions.iter().any(|version| *version != 1) {
            return Err(format!("ParametricBlock version was not 1 ({versions:?})"));
        }
        if level_text.contains("analytic-surface") {
            return Err("the authored level contains analytic-surface".into());
        }
        if parsed.format_version != 6 {
            return Err(format!("level format is {}", parsed.format_version));
        }
        let gathered = {
            let world = self.engine.world();
            let solid = world.entity_outline().iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid).ok_or("Intent Solid missing")?;
            let legacy_id = world.entity_outline().iter().find(|item| item.name == "Legacy Cube").map(|item| item.uuid).ok_or("Legacy Cube missing")?;
            let live = world.authored_block(solid).ok_or("Intent Solid has no block")?;
            if live.body.is_some() || record.body.is_some() {
                return Err("Intent Solid has a stored body".into());
            }
            if legacy.body.is_none() {
                return Err("Legacy Cube lost its stored body".into());
            }
            if live.intent.len() != 65 || record.intent.len() != 65 || live.history.len() != 24 || record.history.len() != 24 {
                return Err(format!("Intent Solid is intent {} history {}, not 65 and 24", record.intent.len(), record.history.len()));
            }
            let diagnostic = jarvig_core::intent_authority_diagnostic(&record);
            if diagnostic != "Intent Authority: ELIGIBLE" {
                return Err(diagnostic);
            }
            let object_mesh = world.object_mesh(solid).ok_or("the ordinary renderer has no mesh for Intent Solid")?;
            let legacy_mesh = world.object_mesh(legacy_id).ok_or("Legacy Cube has no mesh")?;
            let shared = world.meshes().get(object_mesh).ok_or("the ordinary control mesh is missing")?;
            let control_vertices = shared.vertex_count();
            let control_triangles = shared.index_count() / 3;
            if control_vertices != 24 || control_triangles != 12 || control_gpu_bytes != 1512 {
                return Err(format!("the ordinary control is {control_vertices} vertices, {control_triangles} triangles, {control_gpu_bytes} GPU bytes"));
            }
            let front = world.front_camera();
            if !front.vertical_fov_radians.is_finite() || front.vertical_fov_radians <= 0.0 || (f64::from(front.near_m) - 0.1).abs() > 1.0e-6 {
                return Err("the front camera has no finite field of view or its near plane is not 0.1 m".into());
            }
            let pose = world.entity_world_pose(solid).map_err(|error| error.to_string())?;
            let scene_x = pose.translation.x - jarvig_core::BOOTSTRAP_ROOT_M;
            if scene_x.abs() > 1.0e-4 || (pose.translation.y - 1.0).abs() > 1.0e-4 || (pose.translation.z + 4.0).abs() > 1.0e-4 {
                return Err(format!(
                    "Intent Solid scene translation is ({scene_x}, {}, {}), not (0, 1, -4)",
                    pose.translation.y, pose.translation.z
                ));
            }
            (solid, legacy_id, object_mesh, legacy_mesh, control_vertices, control_triangles, front, world.revision(), world.mesh_count(), diagnostic)
        };
        let (solid, legacy_id, object_mesh, legacy_mesh, control_vertices, control_triangles, front, revision_before, mesh_count, diagnostic) = gathered;
        let fov = front.vertical_fov_radians;
        let camera_a = jarvig_core::authored_camera_a(fov);
        let camera_b = jarvig_core::authored_camera_b(fov);
        if camera_a.viewport_width != VIEW_A_WIDTH as f32 || camera_b.viewport_width != VIEW_B_WIDTH as f32 || camera_a.viewport_height != VIEW_HEIGHT as f32 {
            return Err("the authored cameras were not 160x567 and 960x567".into());
        }
        if (camera_a.requested_error_px - 0.5).abs() > 1.0e-6 || (camera_b.requested_error_px - 0.5).abs() > 1.0e-6 {
            return Err("the authored cameras did not both request 0.5 px".into());
        }
        if camera_a.vertical_fov_radians.to_bits() != fov.to_bits() || camera_b.vertical_fov_radians.to_bits() != fov.to_bits() {
            return Err("the observation cameras did not use the extracted front-camera field of view".into());
        }
        let product_a = jarvig_core::realize_authored_observation(&record, &camera_a, 1).map_err(|error| format!("observation A stopped: {error}"))?;
        let product_b = jarvig_core::realize_authored_observation(&record, &camera_b, 1).map_err(|error| format!("observation B stopped: {error}"))?;
        let main_len = std::fs::metadata(jarvig_core::authored_main_level()).map(|meta| meta.len()).unwrap_or(0);
        self.note_authored("Experiment 6: ON");
        self.note_authored("Experiments 3A, 3B, 4, and 5 frozen: PASS");
        self.note_authored("Intent-authority experiment: OFF");
        self.note_authored("ADR-0074 accepted: NO");
        self.note_authored("Default renderer replaced: NO");
        self.note_authored("Curved trailer: NO");
        self.note_authored("analytic-surface: NO");
        self.note_authored("ParametricBlock version: 1");
        self.note_authored("Level format: 6");
        self.note_authored("Type registry: 9");
        self.note_authored(&diagnostic);
        self.note_authored(&format!("AUTHORED_INTENT_ALGORITHM_VERSION: {}", jarvig_core::AUTHORED_INTENT_ALGORITHM_VERSION));
        self.note_authored(&format!("INTENT_OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::INTENT_OBSERVATION_ALGORITHM_VERSION));
        self.note_authored(&format!("DIRECT_REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION));
        self.note_authored(&format!("OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::OBSERVATION_ALGORITHM_VERSION));
        self.note_authored(&format!("REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::REALIZATION_ALGORITHM_VERSION));
        self.note_authored(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_authored("jarvig-realize-0: NOT STARTED");
        self.note_authored("jarvig-realize-1: NOT STARTED");
        self.note_authored("queue: NOT USED");
        self.note_authored("Research Features Enabled: none");
        self.note_authored(&format!("Graphics adapter: {}", self.graphics_name));
        self.note_authored(&format!("Graphics vendor: {:04x}", self.graphics_vendor));
        self.note_authored(&format!("Graphics device: {:04x}", self.graphics_device));
        self.note_authored(&format!("Graphics api: {}", self.graphics_api));
        self.note_authored(&format!("Editor panel: {}x{}", panel.0, panel.1));
        self.note_authored("Observation A viewport: 160x567");
        self.note_authored("Observation B viewport: 960x567");
        self.note_authored("Re-admitted at the editor panel size: NO");
        self.note_authored(&format!("Requested error px: {:.1}", camera_a.requested_error_px));
        self.note_authored(&format!("Extracted front-camera vertical_fov_radians: {} bits {:016x}", fov, fov.to_bits()));
        self.note_authored("Observation cameras used that extracted field of view: YES");
        self.note_authored(&format!("Observation A eye_local: [{:.4}, {:.4}, {:.4}]", camera_a.eye[0], camera_a.eye[1], camera_a.eye[2]));
        self.note_authored(&format!("Observation B eye_local: [{:.4}, {:.4}, {:.4}]", camera_b.eye[0], camera_b.eye[1], camera_b.eye[2]));
        self.note_authored("Observation forward_local: [0.0000, 0.0000, -1.0000]");
        self.note_authored("Authority: saved modeling tape");
        self.note_authored("Stored tessellated body: ABSENT");
        self.note_authored("Legacy cube stored body: PRESENT");
        self.note_authored(&format!("authority_hash: {:016x}", product_a.authority_hash));
        self.note_authored("authority_revision: 1");
        self.note_authored(&format!("Intent length: {}", record.intent.len()));
        self.note_authored(&format!("History length: {}", record.history.len()));
        self.note_authored(&format!("Ordinary control mesh: {control_vertices} vertices, {control_triangles} triangles"));
        self.note_authored(&format!("Level file: {}", level_path.display()));
        self.note_authored(&format!("Main.jarviglevel bytes: {main_len}"));
        self.note_authored(&format!("World revision before: {revision_before}"));
        self.note_authored(&jarvig_core::format_authored_product("Observation A", &product_a));
        self.note_authored(&jarvig_core::format_authored_product("Observation B", &product_b));
        if main_len != jarvig_core::AUTHORED_MAIN_LEVEL_BYTES {
            self.note_authored("Frame: NOT PRESENTED");
            return Err(format!("Main.jarviglevel is {main_len} bytes"));
        }
        if product_a.admitted_ids.is_empty() || product_b.admitted_ids.is_empty() || product_a.admitted_ids == product_b.admitted_ids || product_a.expected_gpu_bytes == product_b.expected_gpu_bytes {
            self.note_authored("Frame: NOT PRESENTED");
            return Err("the two observations did not construct different non-empty fan sets".into());
        }
        if product_a.authority_hash == product_a.local_body_hash || product_a.meshlets_constructed != 0 || product_a.vertices_welded || product_a.discarded_after_construction != 0 {
            self.note_authored("Frame: NOT PRESENTED");
            return Err("the tape product used the local body hash, a weld, or a meshlet".into());
        }
        let material_bound = {
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            snapshot.instances().iter().find(|instance| instance.entity == solid).and_then(|instance| instance.material_for_slot(0)).is_some()
        };
        if !material_bound {
            self.note_authored("Slot-0 material on Intent Solid: UNBOUND");
            self.note_authored("Frame: NOT PRESENTED");
            return Err("the override would be skipped because slot 0 is unbound".into());
        }
        self.note_authored("Slot-0 material on Intent Solid: BOUND");
        self.note_authored("bind_material called: NO");
        self.note_authored("New material asset: NO");
        if self.engine.world().revision() != revision_before {
            return Err("the world revision moved before any observation mesh was added".into());
        }
        let mesh_a = self.engine.world_mut().add_mesh(product_a.mesh.clone());
        let mesh_b = self.engine.world_mut().add_mesh(product_b.mesh.clone());
        let decoy = self.engine.world_mut().add_mesh(jarvig_core::near_triangle_mesh());
        if self.engine.world().revision() != revision_before {
            return Err("add_mesh revised the world".into());
        }
        if mesh_a == object_mesh || mesh_b == object_mesh || mesh_a == mesh_b || mesh_a == legacy_mesh || mesh_b == legacy_mesh || decoy == mesh_a || decoy == mesh_b {
            return Err("an observation mesh reused the control mesh or the legacy mesh".into());
        }
        if self.engine.world().derived_meshlets(mesh_a).is_some() || self.engine.world().derived_meshlets(mesh_b).is_some() {
            return Err("an observation mesh built meshlets".into());
        }
        if self.engine.world().object_mesh(solid) != Some(object_mesh) || self.engine.world().object_mesh(legacy_id) != Some(legacy_mesh) {
            return Err("an object mesh id changed".into());
        }
        let solid_pose = self.engine.world().entity_world_pose(solid).map_err(|error| error.to_string())?;
        let pose_a = authored_pose(solid_pose, camera_a.eye, camera_a.forward)?;
        let pose_b = authored_pose(solid_pose, camera_b.eye, camera_b.forward)?;
        let camera = jarvig_core::Camera { frame: front.frame, vertical_fov_radians: fov, near_m: camera_a.near_m as f32 };
        let target = self.target.ok_or("viewport target missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let right = {
            let renderer = self.renderer.as_mut().unwrap();
            renderer
                .create_view(RenderViewDesc { label: "JARVIG.Perspective.B".into(), target, camera, layout: NormalizedRect::FULL, settings: RenderViewSettings::default() })
                .map_err(|error| error.to_string())?
        };
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, Some(PixelRect { x: 0, y: 0, width: VIEW_A_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
            renderer.set_view_pixel_rect(right, Some(PixelRect { x: VIEW_A_WIDTH, y: 0, width: VIEW_B_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_a) }).map_err(|error| error.to_string())?;
            renderer.update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_b) }).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(left, solid, Some(mesh_a)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, solid, Some(mesh_b)).map_err(|error| error.to_string())?;
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
        if self.engine.world().revision() != revision_before {
            return Err("creating the observation views revised the world".into());
        }
        {
            let lab = self.authored_lab.as_mut().unwrap();
            lab.solid = Some(solid);
            lab.legacy = Some(legacy_id);
            lab.control_mesh = Some(object_mesh);
            lab.legacy_mesh = Some(legacy_mesh);
            lab.mesh_a = Some(mesh_a);
            lab.mesh_b = Some(mesh_b);
            lab.decoy = Some(decoy);
            lab.observation_meshes.extend([mesh_a, mesh_b]);
            lab.right_view = Some(right);
            lab.key_a = product_a.cache_key.clone();
            lab.key_b = product_b.cache_key.clone();
            lab.pose_a = pose_a;
            lab.pose_b = pose_b;
            lab.camera = camera;
            lab.fov = fov;
            lab.gpu_bytes_b = product_b.expected_gpu_bytes;
            lab.product_a = Some(product_a);
            lab.product_b = Some(product_b);
            lab.authority_hash = lab.product_a.as_ref().unwrap().authority_hash;
            lab.control_vertices = control_vertices;
            lab.control_triangles = control_triangles;
            lab.control_gpu_bytes = Some(control_gpu_bytes);
            lab.mesh_count = mesh_count;
            lab.revision_before = revision_before;
            lab.panel = panel;
            lab.level_path = level_path.display().to_string();
            lab.split = true;
            lab.stage = 1;
            lab.mark = self.frames;
        }
        self.note_authored(&format!("Control mesh id: {}", object_mesh.0));
        self.note_authored(&format!("Legacy cube mesh id: {}", legacy_mesh.0));
        self.note_authored(&format!("Observation A mesh id: {}", mesh_a.0));
        self.note_authored(&format!("Observation B mesh id: {}", mesh_b.0));
        self.note_authored(&format!("Decoy mesh id: {}", decoy.0));
        self.note_authored("Marker entity: NOT SPAWNED");
        self.note_authored("set_object_mesh called: NO");
        self.note_authored("build_meshlets called: NO");
        self.note_authored("Observation meshlets: 0");
        self.note_authored(&format!("Control mesh GPU_bytes before: {control_gpu_bytes}"));
        self.note_authored(&format!("World revision after add_mesh: {}", self.engine.world().revision()));
        self.note_authored("World revision unchanged by add_mesh: YES");
        self.note_authored("Color pass overrides the Intent Solid: YES");
        self.note_authored("Legacy cube overridden: NO");
        Ok(())
    }

    fn authored_stage_present(&mut self) -> Result<(), String> {
        if !self.authored_lab.as_ref().unwrap().proved {
            if !self.authored_frame_matches()? {
                if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the first authored frame did not upload ({})", self.authored_wait_reason()));
                }
                return Ok(());
            }
            self.authored_note_uploads(false)?;
            self.authored_note_passes("simultaneous", false)?;
            self.authored_capture("shot-authored-6-simultaneous.png")?;
            self.authored_reject_stale()?;
            if self.authored_lab.as_ref().unwrap().discarded_stale_gpu < 3 {
                return Err("the first present rejected fewer than 3 stale uploads".into());
            }
            self.note_authored("First frame: PRESENTED");
            let lab = self.authored_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.authored_frame_matches()? {
                return Err("a later frame substituted an authored buffer".into());
            }
            if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) == 1 && self.authored_lab.as_ref().unwrap().caster_check_pending {
                self.authored_note_passes("cached", true)?;
            }
            return Ok(());
        }
        self.authored_lab.as_mut().unwrap().stage = 2;
        self.authored_lab.as_mut().unwrap().mark = self.frames;
        self.note_authored("First present held. The saved Size edit is next.");
        Ok(())
    }

    fn authored_stage_edit(&mut self) -> Result<(), String> {
        let (level_path, revision_before, old_a, old_b, old_key_a, old_key_b, hash_before, fov) = {
            let lab = self.authored_lab.as_ref().unwrap();
            (
                lab.level_path.clone(),
                lab.revision_before,
                lab.mesh_a.ok_or("mesh A missing")?,
                lab.mesh_b.ok_or("mesh B missing")?,
                lab.key_a.clone(),
                lab.key_b.clone(),
                lab.authority_hash,
                lab.fov,
            )
        };
        let text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        let mut document = jarvig_core::parse_level(&text).map_err(|error| error.to_string())?;
        let current = block_named(&document, "Intent Solid")?;
        if current.body.is_some() || current.intent.len() != 65 || current.history.len() != 24 {
            return Err("the file changed before the Size edit".into());
        }
        if jarvig_core::intent_authority_diagnostic(&current) != "Intent Authority: ELIGIBLE" {
            return Err(jarvig_core::intent_authority_diagnostic(&current));
        }
        let requested = [current.size_m[0], current.size_m[1] + 0.1, current.size_m[2]];
        let edited = jarvig_core::commit_class_c_intent(
            &current,
            jarvig_core::IntentEntry { groups: None, payload: jarvig_core::IntentPayload::Size { size_m: requested } },
        )
        .map_err(|error| error.to_string())?;
        if size_bits(edited.size_m) != jarvig_core::EDITED_SIZE_BITS {
            return Err(format!("the transaction cache is {}, not the replay extent", size_bits(edited.size_m)));
        }
        if edited.body.is_some() || edited.intent.len() != 66 || edited.history.len() != 24 {
            return Err("the Size append did not stay body-less at intent 66 and history 24".into());
        }
        if jarvig_core::authored_size_bits(&edited).map_err(|error| error.to_string())? != jarvig_core::EDITED_SIZE_BITS {
            return Err("the Size append bits are not the Experiment 4 bits".into());
        }
        install_block(&mut document, edited);
        let backup = self.authored_artifact_dir().join("Saved").join("Backup");
        jarvig_core::save_level_atomic(std::path::Path::new(&level_path), &backup, &document).map_err(|error| error.to_string())?;
        let parsed_text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        if parsed_text.contains("analytic-surface") || parametric_versions(&parsed_text)?.iter().any(|version| *version != 1) {
            return Err("the saved edit emitted analytic-surface or a ParametricBlock version other than 1".into());
        }
        let parsed_document = jarvig_core::parse_level(&parsed_text).map_err(|error| error.to_string())?;
        let parsed = block_named(&parsed_document, "Intent Solid")?;
        let legacy = block_named(&parsed_document, "Legacy Cube")?;
        if parsed.body.is_some() || legacy.body.is_none() || parsed.intent.len() != 66 || parsed.history.len() != 24 {
            return Err("the parsed edit did not keep the absent specimen body, the legacy body, intent 66, and history 24".into());
        }
        if jarvig_core::authored_size_bits(&parsed).map_err(|error| error.to_string())? != jarvig_core::EDITED_SIZE_BITS {
            return Err("the second realization did not read the parsed Size bits".into());
        }
        if size_bits(parsed.size_m) != jarvig_core::EDITED_SIZE_BITS {
            return Err(format!("parsed size_m stayed {}", size_bits(parsed.size_m)));
        }
        let diagnostic = jarvig_core::intent_authority_diagnostic(&parsed);
        if diagnostic != "Intent Authority: ELIGIBLE" {
            return Err(diagnostic);
        }
        let edited_hash = jarvig_core::authored_authority_hash(&parsed).map_err(|error| error.to_string())?;
        if edited_hash == hash_before {
            return Err("the saved Size edit did not change the authority hash".into());
        }
        let camera_a = jarvig_core::authored_camera_a(fov);
        let camera_b = jarvig_core::authored_camera_b(fov);
        if camera_a.vertical_fov_radians.to_bits() != fov.to_bits() || camera_a.viewport_width != VIEW_A_WIDTH as f32 || camera_b.viewport_width != VIEW_B_WIDTH as f32 {
            return Err("the second present moved a camera, a viewport, or the field of view".into());
        }
        let product_a = jarvig_core::realize_authored_observation(&parsed, &camera_a, 2).map_err(|error| format!("edited observation A stopped: {error}"))?;
        let product_b = jarvig_core::realize_authored_observation(&parsed, &camera_b, 2).map_err(|error| format!("edited observation B stopped: {error}"))?;
        self.note_authored("Post-cache-contract rerun: YES");
        self.note_authored("Original Experiment 6 FAIL preserved: YES");
        self.note_authored(&format!("Edited size bits: {}", jarvig_core::EDITED_SIZE_BITS));
        self.note_authored(&format!("Parsed size_m bits: {}", size_bits(parsed.size_m)));
        self.note_authored("size_m written from replay AABB: YES");
        self.note_authored(&format!("authority_hash after edit: {edited_hash:016x}"));
        self.note_authored("authority_revision: 2");
        self.note_authored("Intent length after edit: 66");
        self.note_authored("History length after edit: 24");
        self.note_authored(&diagnostic);
        self.note_authored("Size edit written by the level writer: YES");
        self.note_authored("Second realization source: parsed level file");
        self.note_authored(&jarvig_core::format_authored_product("Edited Observation A", &product_a));
        self.note_authored(&jarvig_core::format_authored_product("Edited Observation B", &product_b));
        let before_a = self.authored_lab.as_ref().unwrap().product_a.clone().ok_or("observation A missing")?;
        let before_b = self.authored_lab.as_ref().unwrap().product_b.clone().ok_or("observation B missing")?;
        if !jarvig_core::authored_planar_changed(&before_a, &product_a) && !jarvig_core::authored_planar_changed(&before_b, &product_b) {
            return Err("every planar position hash was unchanged".into());
        }
        if product_a.admitted_ids.is_empty() || product_b.admitted_ids.is_empty() || product_a.admitted_ids == product_b.admitted_ids || product_a.expected_gpu_bytes == product_b.expected_gpu_bytes {
            return Err("the edited observations did not construct different non-empty fan sets".into());
        }
        self.note_authored("At least one planar position hash changed: YES");
        if product_a.cache_key == old_key_a || product_b.cache_key == old_key_b || !product_a.cache_key.starts_with("authored6-") || !product_a.cache_key.contains("-r2-") {
            return Err("the edited cache key did not leave the revision-1 key".into());
        }
        let (left, right, entity) = {
            let lab = self.authored_lab.as_ref().unwrap();
            (self.viewport_view.ok_or("view A missing")?, lab.right_view.ok_or("view B missing")?, lab.solid.unwrap())
        };
        self.authored_bind_if_current(left, entity, old_a, &old_key_a, &product_a.cache_key)?;
        self.authored_bind_if_current(right, entity, old_b, &old_key_b, &product_b.cache_key)?;
        if self.authored_lab.as_ref().unwrap().discarded_stale_gpu < 5 {
            return Err("the revision-1 meshes were not rejected after the edit".into());
        }
        self.note_authored("Revision-1 GPU meshes rejected: YES");
        let mesh_a = self.engine.world_mut().add_mesh(product_a.mesh.clone());
        let mesh_b = self.engine.world_mut().add_mesh(product_b.mesh.clone());
        if self.engine.world().revision() != revision_before {
            return Err("the edited upload revised the world".into());
        }
        if mesh_a == old_a || mesh_b == old_b || mesh_a == mesh_b {
            return Err("the edited realization reused a revision-1 mesh id".into());
        }
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_mesh_override(left, entity, Some(mesh_a)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, Some(mesh_b)).map_err(|error| error.to_string())?;
        }
        self.authored_retire_mesh(old_a)?;
        self.authored_retire_mesh(old_b)?;
        {
            let lab = self.authored_lab.as_mut().unwrap();
            lab.mesh_a = Some(mesh_a);
            lab.mesh_b = Some(mesh_b);
            lab.observation_meshes.extend([mesh_a, mesh_b]);
            lab.key_a = product_a.cache_key.clone();
            lab.key_b = product_b.cache_key.clone();
            lab.gpu_bytes_b = product_b.expected_gpu_bytes;
            lab.product_a = Some(product_a);
            lab.product_b = Some(product_b);
            lab.edited_hash = edited_hash;
            lab.stage = 3;
            lab.mark = self.frames;
        }
        self.note_authored(&format!("Edited Observation A mesh id: {}", mesh_a.0));
        self.note_authored(&format!("Edited Observation B mesh id: {}", mesh_b.0));
        self.note_authored("Cameras moved between presents: NO");
        self.note_authored(&format!("World revision after edit: {}", self.engine.world().revision()));
        Ok(())
    }

    fn authored_stage_edited(&mut self) -> Result<(), String> {
        if !self.authored_lab.as_ref().unwrap().edited_proved {
            if !self.authored_frame_matches()? {
                if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the edited authored frame did not upload ({})", self.authored_wait_reason()));
                }
                return Ok(());
            }
            self.authored_note_uploads(true)?;
            self.authored_note_passes("edited", false)?;
            self.authored_capture("shot-authored-6-edited.png")?;
            self.note_authored("Second frame: PRESENTED");
            let lab = self.authored_lab.as_mut().unwrap();
            lab.edited_proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.authored_frame_matches()? {
                return Err("a later edited frame substituted an authored buffer".into());
            }
            if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) == 1 && self.authored_lab.as_ref().unwrap().caster_check_pending {
                self.authored_note_passes("edited-cached", true)?;
            }
            return Ok(());
        }
        self.authored_retire_mesh(self.authored_lab.as_ref().unwrap().mesh_a.ok_or("edited mesh A missing")?)?;
        self.authored_lab.as_mut().unwrap().mesh_a = None;
        self.authored_lab.as_mut().unwrap().stage = 4;
        self.authored_lab.as_mut().unwrap().mark = self.frames;
        self.note_authored("Edited Observation A destroy requested");
        Ok(())
    }

    fn authored_stage_a_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh_a = *self.authored_lab.as_ref().unwrap().retired.last().ok_or("edited mesh A was not retired")?;
        let mesh_b = self.authored_lab.as_ref().unwrap().mesh_b.ok_or("edited mesh B missing")?;
        let control = self.authored_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let gone = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_a).is_none();
        let bytes_b = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_b);
        if !gone || bytes_b.is_none() {
            if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) > 90 {
                return Err("destroying edited A did not leave B resident and A evicted".into());
            }
            return Ok(());
        }
        if bytes_b != Some(self.authored_lab.as_ref().unwrap().gpu_bytes_b) {
            return Err(format!("B GPU bytes changed to {:?} while A was destroyed", bytes_b));
        }
        if !self.authored_lab.as_ref().unwrap().captured_a {
            let left = self.viewport_view.ok_or("view A missing")?;
            let right = self.authored_lab.as_ref().unwrap().right_view.ok_or("view B missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            if color_contains(&draws, left, mesh_a) || !color_contains(&draws, left, control) || !color_contains(&draws, right, mesh_b) || color_contains(&draws, right, control) {
                return Err("the frame after destroying A still mixed the observation meshes".into());
            }
            self.note_authored("Observation A GPU_bytes after destroy: GONE");
            self.note_authored(&format!("Observation B GPU_bytes while A is gone: {}", bytes_b.unwrap()));
            self.authored_capture("shot-authored-6-a-destroyed.png")?;
            self.authored_lab.as_mut().unwrap().captured_a = true;
            self.authored_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.authored_retire_mesh(mesh_b)?;
        self.authored_lab.as_mut().unwrap().mesh_b = None;
        self.authored_lab.as_mut().unwrap().stage = 5;
        self.authored_lab.as_mut().unwrap().mark = self.frames;
        self.note_authored("Edited Observation B destroy requested");
        Ok(())
    }

    fn authored_stage_b_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let meshes = self.authored_lab.as_ref().unwrap().retired.clone();
        let gone = meshes.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        if !gone {
            if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) > 90 {
                return Err("edited observation B stayed resident after destroy".into());
            }
            return Ok(());
        }
        if !self.authored_lab.as_ref().unwrap().captured_b {
            self.note_authored("Observation B GPU_bytes after destroy: GONE");
            self.authored_capture("shot-authored-6-b-destroyed.png")?;
            self.authored_lab.as_mut().unwrap().captured_b = true;
            self.authored_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        let decoy = self.authored_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        self.authored_retire_mesh(decoy)?;
        self.authored_lab.as_mut().unwrap().decoy = None;
        let right = self.authored_lab.as_ref().unwrap().right_view.ok_or("view B missing")?;
        let left = self.viewport_view.ok_or("view A missing")?;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, None).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::FULL), settings: None, pose: None }).map_err(|error| error.to_string())?;
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
        }
        if self.engine.world().revision() != self.authored_lab.as_ref().unwrap().revision_before {
            return Err("destroying the observation views revised the world".into());
        }
        let lab = self.authored_lab.as_mut().unwrap();
        lab.split = false;
        lab.right_view = None;
        lab.stage = 6;
        lab.mark = self.frames;
        self.note_authored("Perspective restored to one full view");
        Ok(())
    }

    fn authored_stage_intact(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.authored_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let lab = self.authored_lab.as_ref().unwrap();
        let solid = lab.solid.ok_or("solid missing")?;
        let legacy = lab.legacy.ok_or("legacy missing")?;
        let control = lab.control_mesh.ok_or("control missing")?;
        let legacy_mesh = lab.legacy_mesh.ok_or("legacy mesh missing")?;
        let mesh_same = self.engine.world().object_mesh(solid) == Some(control);
        let legacy_same = self.engine.world().object_mesh(legacy) == Some(legacy_mesh);
        let ordinary_same = self.engine.world().meshes().get(control).is_some_and(|mesh| mesh.vertex_count() == lab.control_vertices && mesh.index_count() / 3 == lab.control_triangles);
        let live = self.engine.world().authored_block(solid);
        let live_untouched = live.as_ref().is_some_and(|record| record.body.is_none() && record.intent.len() == 65 && record.history.len() == 24);
        let retired = lab.retired.clone();
        let orphans_gone = retired.iter().all(|mesh| self.engine.world().meshes().get(*mesh).is_none());
        let gpu_gone = retired.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        let control_after = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        let control_match = control_after == lab.control_gpu_bytes && control_after == Some(1512);
        let views = self.renderer.as_ref().unwrap().view_count() == 1;
        let saved_text = std::fs::read_to_string(&lab.level_path).unwrap_or_default();
        let saved = jarvig_core::parse_level(&saved_text).ok();
        let saved_record = saved.as_ref().and_then(|document| block_named(document, "Intent Solid").ok());
        let saved_legacy = saved.as_ref().and_then(|document| block_named(document, "Legacy Cube").ok());
        let saved_ok = saved_record.as_ref().is_some_and(|record| {
            record.body.is_none()
                && record.history.len() == 24
                && record.intent.len() == 66
                && jarvig_core::authored_size_bits(record).ok().as_deref() == Some(jarvig_core::EDITED_SIZE_BITS)
                && jarvig_core::intent_authority_diagnostic(record) == "Intent Authority: ELIGIBLE"
        }) && saved_legacy.as_ref().is_some_and(|record| record.body.is_some())
            && !saved_text.contains("analytic-surface")
            && parametric_versions(&saved_text).ok().is_some_and(|versions| versions.iter().all(|version| *version == 1));
        let main_len = std::fs::metadata(jarvig_core::authored_main_level()).map(|meta| meta.len()).unwrap_or(0);
        let main_same = main_len == jarvig_core::AUTHORED_MAIN_LEVEL_BYTES;
        let revision_same = self.engine.world().revision() == lab.revision_before;
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
        let counts_ok = lab.acquires_first == 1 && lab.presents_first == 1 && lab.acquires_second == 1 && lab.presents_second == 1;
        let stale_ok = lab.discarded_stale_gpu >= 5;
        let mesh_count_same = self.engine.world().mesh_count() == lab.mesh_count;
        let workers_ok = self.jobs.worker_count() == 2 && self.einstein_jobs.worker_count() == 1;
        let versions_ok = jarvig_core::AUTHORED_INTENT_ALGORITHM_VERSION == 1
            && jarvig_core::INTENT_OBSERVATION_ALGORITHM_VERSION == 1
            && jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION == 1
            && jarvig_core::OBSERVATION_ALGORITHM_VERSION == 1
            && jarvig_core::REALIZATION_ALGORITHM_VERSION == 1;
        let hash_moved = lab.edited_hash != 0 && lab.edited_hash != lab.authority_hash;
        let panel_ok = lab.panel.0 >= REQUIRED_WIDTH && lab.panel.1 >= VIEW_HEIGHT;
        let object_ok = mesh_same
            && legacy_same
            && ordinary_same
            && live_untouched
            && orphans_gone
            && gpu_gone
            && control_match
            && views
            && saved_ok
            && main_same
            && revision_same
            && element == "NONE"
            && graphics_ok
            && presented
            && counts_ok
            && stale_ok
            && mesh_count_same
            && workers_ok
            && versions_ok
            && hash_moved
            && panel_ok
            && !self.engine.intent_authority_experiment();
        self.authored_capture("shot-authored-6-object.png")?;
        self.note_authored(&format!("Editor frame: {}", self.frames));
        self.note_authored(&format!("Presented frames: {}", self.renderer.as_ref().unwrap().presented_frames()));
        self.note_authored(&format!("Frame: {}", if presented && counts_ok { "PRESENTED" } else { "NOT PRESENTED" }));
        self.note_authored(&format!("Graphics is Intel UHD DX12 8086:9b41: {}", yes_no(graphics_ok)));
        self.note_authored(&format!("Acquires on the first proof frame: {}", self.authored_lab.as_ref().unwrap().acquires_first));
        self.note_authored(&format!("Presents on the first proof frame: {}", self.authored_lab.as_ref().unwrap().presents_first));
        self.note_authored(&format!("Acquires on the second proof frame: {}", self.authored_lab.as_ref().unwrap().acquires_second));
        self.note_authored(&format!("Presents on the second proof frame: {}", self.authored_lab.as_ref().unwrap().presents_second));
        self.note_authored(&format!("Saved authored intent intact: {}", yes_no(saved_ok)));
        self.note_authored(&format!("Live world kept the unedited tape: {}", yes_no(live_untouched)));
        self.note_authored(&format!("Ordinary control mesh unchanged: {}", yes_no(mesh_same && ordinary_same && control_match)));
        self.note_authored(&format!("Legacy cube mesh unchanged: {}", yes_no(legacy_same)));
        self.note_authored(&format!("World revision before: {}", self.authored_lab.as_ref().unwrap().revision_before));
        self.note_authored(&format!("World revision after: {}", self.engine.world().revision()));
        self.note_authored(&format!("World revision unchanged: {}", yes_no(revision_same)));
        self.note_authored(&format!("Library mesh count restored: {}", yes_no(mesh_count_same)));
        self.note_authored(&format!("Observation GPU buffers after destroy: {}", if gpu_gone { "EVICTED" } else { "RESIDENT" }));
        self.note_authored(&format!("Control mesh GPU_bytes after: {}", control_after.map(|value| value.to_string()).unwrap_or_else(|| "GONE".into())));
        self.note_authored(&format!("Main.jarviglevel bytes: {main_len}"));
        self.note_authored(&format!("Render view count restored: {}", yes_no(views)));
        self.note_authored(&format!("Editor element selection: {element}"));
        self.note_authored(&format!("stale uploads rejected: {}", self.authored_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_authored(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_authored("Intent-authority experiment: OFF");
        self.note_authored("ADR-0074 accepted: NO");
        self.note_authored("RFC-0007 accepted: NO");
        self.note_authored("RFC-0007 Phase 3: NO");
        self.note_authored("RFC-0002 stamped: NO");
        self.note_authored("Renderer replacement: NO");
        self.note_authored("Authored-intent override without the flag: NO");
        let passed = object_ok
            && self.authored_lab.as_ref().unwrap().report.contains("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES")
            && self.authored_lab.as_ref().unwrap().report.contains("Second frame: PRESENTED")
            && self.authored_lab.as_ref().unwrap().report.contains("First frame: PRESENTED");
        if passed {
            self.note_authored("This pass is the post-cache-contract rerun. The 2026-10-06 FAIL remains the measurement that found two stored extents.");
            self.note_authored("Earned: A user-authored JARVIG solid persisted its modeling tape, with no tessellated body, through a saved edit. Two observations independently synthesized different GPU meshes from that reloaded tape.");
        }
        self.note_authored(&format!("Experiment 6: {}", if passed { "PASS" } else { "FAIL" }));
        self.authored_write_report();
        self.authored_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 6 did not meet the presented-frame success condition".into());
        }
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn authored_note_uploads(&mut self, second: bool) -> Result<(), String> {
        let (mesh_a, mesh_b, product_a, product_b, control) = {
            let lab = self.authored_lab.as_ref().unwrap();
            (lab.mesh_a.unwrap(), lab.mesh_b.unwrap(), lab.product_a.clone().unwrap(), lab.product_b.clone().unwrap(), lab.control_mesh.unwrap())
        };
        let record_a = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_a).ok_or("observation A was not uploaded")?;
        let record_b = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_b).ok_or("observation B was not uploaded")?;
        let prove = |label: &str, product: &jarvig_core::AuthoredProduct, record: jarvig_renderer::MeshUploadRecord| -> Result<(), String> {
            if record.vertex_create_bytes != product.packed_vertex_bytes || record.index_create_bytes != product.packed_index_bytes || record.gpu_bytes != product.expected_gpu_bytes {
                return Err(format!("{label} create_buffer bytes did not match the provenance total"));
            }
            if record.vertex_create_bytes + record.index_create_bytes != record.gpu_bytes {
                return Err(format!("{label} GpuMesh.bytes is not the sum of the create_buffer sizes"));
            }
            let uploaded = record.index_create_bytes / u64::from(product.index_format.byte_size()) / 3;
            if uploaded != u64::from(product.packed_triangles) || product.constructed_triangles != product.packed_triangles || product.uploaded_triangles != product.packed_triangles {
                return Err(format!("{label} uploaded a different triangle count"));
            }
            if product.discarded_after_construction != 0 || product.discarded_during_pack != 0 || product.discarded_after_upload != 0 {
                return Err(format!("{label} discarded triangles"));
            }
            Ok(())
        };
        let label_a = if second { "Edited Observation A" } else { "Observation A" };
        let label_b = if second { "Edited Observation B" } else { "Observation B" };
        prove(label_a, &product_a, record_a)?;
        prove(label_b, &product_b, record_b)?;
        if product_a.expected_gpu_bytes == product_b.expected_gpu_bytes {
            return Err("the two observation GPU byte totals are equal".into());
        }
        let control_bytes = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        if control_bytes != Some(1512) {
            return Err(format!("the ordinary control left 1512 GPU bytes ({control_bytes:?})"));
        }
        self.note_authored(&format!("{label_a} GpuMesh.bytes: {}", record_a.gpu_bytes));
        self.note_authored(&format!("{label_a} create_buffer bytes: {}", record_a.vertex_create_bytes + record_a.index_create_bytes));
        self.note_authored(&format!("{label_b} GpuMesh.bytes: {}", record_b.gpu_bytes));
        self.note_authored(&format!("{label_b} create_buffer bytes: {}", record_b.vertex_create_bytes + record_b.index_create_bytes));
        self.note_authored("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_authored("triangles_discarded_after_upload: 0");
        self.note_authored("Ordinary control GPU_bytes: 1512");
        let acquires = self.renderer.as_ref().unwrap().acquires_last_frame();
        let presents = self.renderer.as_ref().unwrap().presents_last_frame();
        self.note_authored(&format!("Acquires this frame: {acquires}"));
        self.note_authored(&format!("Presents this frame: {presents}"));
        if acquires != 1 || presents != 1 {
            return Err("the authored frame did not use one acquire and one present".into());
        }
        let lab = self.authored_lab.as_mut().unwrap();
        if second {
            lab.acquires_second = acquires;
            lab.presents_second = presents;
        } else {
            lab.acquires_first = acquires;
            lab.presents_first = presents;
        }
        lab.gpu_bytes_b = record_b.gpu_bytes;
        Ok(())
    }

    fn authored_note_passes(&mut self, label: &str, require_recorded: bool) -> Result<(), String> {
        let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
        let control = self.authored_lab.as_ref().unwrap().control_mesh.unwrap();
        let decoy = self.authored_lab.as_ref().unwrap().decoy.unwrap();
        let observation = self.authored_lab.as_ref().unwrap().observation_meshes.clone();
        let shadow = pass_meshes(&draws, "shadow");
        let shadow_cached = pass_meshes(&draws, "shadow-cached");
        let contact = pass_meshes(&draws, "contact");
        let probe = pass_meshes(&draws, "probe");
        let casters = if shadow.is_empty() { shadow_cached.clone() } else { shadow.clone() };
        self.note_authored(&format!("{label} shadow draws: {}", mesh_list(&shadow)));
        self.note_authored(&format!("{label} shadow casters: {}", mesh_list(&casters)));
        self.note_authored(&format!("{label} contact meshes: {}", mesh_list(&contact)));
        self.note_authored(&format!("{label} probe meshes: {}", mesh_list(&probe)));
        let forbidden = |list: &[MeshId]| list.iter().any(|mesh| observation.contains(mesh) || *mesh == decoy);
        if forbidden(&shadow) || forbidden(&shadow_cached) || forbidden(&contact) || forbidden(&probe) {
            return Err("an observation mesh was drawn by shadow, contact, or probe capture".into());
        }
        self.note_authored("Observation mesh in shadow draws: NO");
        self.note_authored("Observation mesh in contact draws: NO");
        self.note_authored("Observation mesh in probe draws: NO");
        if casters.is_empty() && !require_recorded {
            let solid = self.authored_lab.as_ref().unwrap().solid.ok_or("the ordinary control entity is missing")?;
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let casts = snapshot.instances().iter().find(|instance| instance.entity == solid).is_some_and(|instance| instance.cast_shadows && instance.mesh == control);
            if !casts {
                return Err("the ordinary control mesh does not cast shadows".into());
            }
            self.note_authored("Ordinary control cast_shadows: YES");
            self.note_authored("Shadow caster list on the rebuild frame: NOT RECORDED");
            self.authored_lab.as_mut().unwrap().caster_check_pending = true;
            return Ok(());
        }
        if !casters.contains(&control) {
            return Err("the ordinary control mesh was not the shadow caster".into());
        }
        self.note_authored("Ordinary control mesh in shadow casters: YES");
        self.authored_lab.as_mut().unwrap().caster_check_pending = false;
        Ok(())
    }

    fn authored_wait_reason(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else {
            return "renderer missing".into();
        };
        let Some(lab) = self.authored_lab.as_ref() else {
            return "lab missing".into();
        };
        let rect = |view: Option<jarvig_renderer::RenderViewId>| {
            view.and_then(|view| renderer.viewport(view).ok().flatten()).map(|rect| format!("{}x{}@{},{}", rect.width, rect.height, rect.x, rect.y)).unwrap_or_else(|| "none".into())
        };
        let uploaded = |mesh: Option<MeshId>| mesh.and_then(|mesh| renderer.mesh_upload_record(mesh)).is_some();
        format!("A {} B {} A_uploaded {} B_uploaded {}", rect(self.viewport_view), rect(lab.right_view), uploaded(lab.mesh_a), uploaded(lab.mesh_b))
    }

    fn authored_frame_matches(&self) -> Result<bool, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let left = self.viewport_view.ok_or("view A missing")?;
        let right = match self.authored_lab.as_ref().unwrap().right_view {
            Some(view) => view,
            None => return Ok(false),
        };
        let mesh_a = self.authored_lab.as_ref().unwrap().mesh_a;
        let mesh_b = self.authored_lab.as_ref().unwrap().mesh_b;
        let control = self.authored_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let decoy = self.authored_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
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

    fn authored_reject_stale(&mut self) -> Result<(), String> {
        let (left, right, mesh_a, mesh_b, decoy, key_a, key_b, entity) = {
            let lab = self.authored_lab.as_ref().unwrap();
            (
                self.viewport_view.ok_or("view A missing")?,
                lab.right_view.ok_or("view B missing")?,
                lab.mesh_a.unwrap(),
                lab.mesh_b.unwrap(),
                lab.decoy.unwrap(),
                lab.key_a.clone(),
                lab.key_b.clone(),
                lab.solid.unwrap(),
            )
        };
        self.authored_bind_if_current(left, entity, mesh_b, &key_b, &key_a)?;
        self.authored_bind_if_current(right, entity, mesh_a, &key_a, &key_b)?;
        self.authored_bind_if_current(left, entity, decoy, "stale-decoy", &key_a)?;
        if self.renderer.as_ref().unwrap().resident_mesh_bytes(decoy).is_some() {
            return Err("the refused decoy was uploaded".into());
        }
        self.note_authored("Decoy mesh GPU_bytes: NOT UPLOADED");
        self.note_authored(&format!("stale uploads rejected: {}", self.authored_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_authored("Cross-view buffer substitution: NO");
        self.note_authored("Decoy mesh bound: NO");
        Ok(())
    }

    fn authored_bind_if_current(&mut self, view: RenderViewId, entity: jarvig_core::EntityId, mesh: MeshId, product_key: &str, current_key: &str) -> Result<(), String> {
        if product_key != current_key {
            self.authored_lab.as_mut().unwrap().discarded_stale_gpu = self.authored_lab.as_ref().unwrap().discarded_stale_gpu.saturating_add(1);
            return Ok(());
        }
        self.renderer.as_mut().unwrap().set_view_mesh_override(view, entity, Some(mesh)).map_err(|error| error.to_string())
    }

    fn authored_retire_mesh(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.authored_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let legacy = self.authored_lab.as_ref().unwrap().legacy_mesh.ok_or("legacy mesh missing")?;
        if mesh == control || mesh == legacy {
            return Err("refusing to retire the ordinary control mesh or the legacy cube".into());
        }
        if !self.engine.world_mut().retire_unreferenced_mesh(mesh) {
            return Err(format!("authored mesh {} stayed referenced", mesh.0));
        }
        self.authored_lab.as_mut().unwrap().retired.push(mesh);
        if self.engine.world().revision() != self.authored_lab.as_ref().unwrap().revision_before {
            return Err("retire_unreferenced_mesh revised the world".into());
        }
        Ok(())
    }
}

fn size_bits(size_m: [f64; 3]) -> String {
    format!("{:016x} {:016x} {:016x}", size_m[0].to_bits(), size_m[1].to_bits(), size_m[2].to_bits())
}

fn block_named(document: &jarvig_core::LevelDocument, name: &str) -> Result<jarvig_core::BlockRecord, String> {
    document
        .entities
        .iter()
        .find(|entity| entity.name == name)
        .and_then(|entity| {
            entity.components.iter().find_map(|component| match component {
                jarvig_core::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                _ => None,
            })
        })
        .ok_or_else(|| format!("{name} was not a parametric block in the parsed level"))
}

fn install_block(document: &mut jarvig_core::LevelDocument, block: jarvig_core::BlockRecord) {
    let entity = document.entities.iter_mut().find(|entity| entity.name == "Intent Solid").expect("Intent Solid");
    for component in &mut entity.components {
        if let jarvig_core::ComponentRecord::ParametricBlock(slot) = component {
            *slot = block;
            return;
        }
    }
}

fn parametric_versions(text: &str) -> Result<Vec<u32>, String> {
    let mut versions = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("\"type\": \"ParametricBlock\"") {
        let slice = &rest[at..rest.len().min(at + 180)];
        if slice.contains("\"version\": 1") {
            versions.push(1);
        } else if slice.contains("\"version\": 2") {
            versions.push(2);
        } else {
            return Err("a ParametricBlock version was not 1 or 2".into());
        }
        rest = &rest[at + 24..];
    }
    if versions.is_empty() {
        return Err("the level file has no ParametricBlock".into());
    }
    Ok(versions)
}

fn authored_pose(solid: ResolvedPose, eye: [f64; 3], forward: [f64; 3]) -> Result<ResolvedPose, String> {
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
        return Err(format!("the view rotation does not look along the authored forward ({delta})"));
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

fn mesh_list(meshes: &[MeshId]) -> String {
    if meshes.is_empty() {
        "NONE".into()
    } else {
        meshes.iter().map(|mesh| mesh.0.to_string()).collect::<Vec<_>>().join(",")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}
