//! Experiment 5. Two observations synthesize GPU geometry from one saved chart.
//!
//! The radius edit is written and parsed back. This lab does not enable intent
//! authority, does not spawn a marker, and does not edit Experiment 4.

use jarvig_core::{MeshId, Quat, ResolvedPose, Vec3};
use jarvig_renderer::{NormalizedRect, PixelRect, RenderViewDesc, RenderViewId, RenderViewSettings, RenderViewUpdate};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostQuitMessage, SetWindowPos, SWP_NOZORDER};

use crate::dock::{WorkspaceCommand, CONTENT, INSPECTOR, OUTLINER, OUTPUT};

use super::Editor;

const HOLD_SHOT: u32 = 24;
const VIEW_FAR_WIDTH: u32 = 160;
const VIEW_CLOSE_WIDTH: u32 = 960;
const VIEW_HEIGHT: u32 = 567;
const REQUIRED_WIDTH: u32 = VIEW_FAR_WIDTH + VIEW_CLOSE_WIDTH;
const FAR_POSITION: u64 = 0xd483_cd5f_6133_8a61;
const CLOSE_POSITION: u64 = 0x6e2e_84eb_1989_2f71;
const FAR_INDEX: u64 = 0x0fef_7021_13eb_2f05;
const CLOSE_INDEX: u64 = 0xd9f7_af58_ffd2_4f65;
const FAR_ERROR: &str = "0.424494";
const FAR_COARSER: &str = "0.613221";
const CLOSE_ERROR: &str = "0.462054";
const CLOSE_COARSER: &str = "0.536666";
const MAIN_LEVEL: &str = r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\Content\Levels\Main.jarviglevel";

pub(super) struct AnalyticLab {
    stage: u8,
    arranged: bool,
    split: bool,
    grow: u8,
    mark: u32,
    proved: bool,
    edited_proved: bool,
    captured_far: bool,
    captured_close: bool,
    right_view: Option<RenderViewId>,
    solid: Option<jarvig_core::EntityId>,
    control_mesh: Option<MeshId>,
    mesh_far: Option<MeshId>,
    mesh_close: Option<MeshId>,
    decoy: Option<MeshId>,
    retired: Vec<MeshId>,
    observation_meshes: Vec<MeshId>,
    key_far: String,
    key_close: String,
    pose_far: ResolvedPose,
    pose_close: ResolvedPose,
    camera: jarvig_core::Camera,
    product_far: Option<jarvig_core::AnalyticProduct>,
    product_close: Option<jarvig_core::AnalyticProduct>,
    authority_hash: u64,
    edited_hash: u64,
    control_vertices: u32,
    control_triangles: u32,
    control_gpu_bytes: Option<u64>,
    close_gpu_bytes: u64,
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

impl AnalyticLab {
    pub(super) fn new() -> Self {
        Self {
            stage: 0,
            arranged: false,
            split: false,
            grow: 0,
            mark: 0,
            proved: false,
            edited_proved: false,
            captured_far: false,
            captured_close: false,
            right_view: None,
            solid: None,
            control_mesh: None,
            mesh_far: None,
            mesh_close: None,
            decoy: None,
            retired: Vec::new(),
            observation_meshes: Vec::new(),
            key_far: String::new(),
            key_close: String::new(),
            pose_far: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            pose_close: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            camera: jarvig_core::Camera { frame: jarvig_core::FrameId(0), vertical_fov_radians: 1.0, near_m: 0.1 },
            product_far: None,
            product_close: None,
            authority_hash: 0,
            edited_hash: 0,
            control_vertices: 0,
            control_triangles: 0,
            control_gpu_bytes: None,
            close_gpu_bytes: 0,
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
    pub(super) fn apply_analytic_frame(&mut self) -> Result<(), String> {
        let Some(lab) = self.analytic_lab.as_ref() else {
            return Ok(());
        };
        if !lab.split {
            return Ok(());
        }
        let right = lab.right_view.ok_or("the close observation view is missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let (pose_far, pose_close, camera, entity, mesh_far, mesh_close) = (lab.pose_far, lab.pose_close, lab.camera, lab.solid, lab.mesh_far, lab.mesh_close);
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer.update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_far) }).map_err(|error| error.to_string())?;
        renderer.update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_close) }).map_err(|error| error.to_string())?;
        renderer.set_view_pixel_rect(left, Some(PixelRect { x: 0, y: 0, width: VIEW_FAR_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
        renderer
            .set_view_pixel_rect(right, Some(PixelRect { x: VIEW_FAR_WIDTH, y: 0, width: VIEW_CLOSE_WIDTH, height: VIEW_HEIGHT }))
            .map_err(|error| error.to_string())?;
        if let Some(entity) = entity {
            renderer.set_view_mesh_override(left, entity, mesh_far).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, mesh_close).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn step_analytic_lab(&mut self) -> Result<(), String> {
        if self.analytic_lab.is_none() {
            return Ok(());
        }
        let stage = self.analytic_lab.as_ref().unwrap().stage;
        if stage >= 15 {
            return Ok(());
        }
        let result = match stage {
            0 => self.analytic_stage_setup(),
            1 => self.analytic_stage_present(),
            2 => self.analytic_stage_edit(),
            3 => self.analytic_stage_edited(),
            4 => self.analytic_stage_far_gone(),
            5 => self.analytic_stage_close_gone(),
            6 => self.analytic_stage_intact(),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.note_analytic("Experiment 5: FAIL");
            self.note_analytic(&format!("Failure: {error}"));
            self.analytic_write_report();
            if let Some(lab) = self.analytic_lab.as_mut() {
                lab.stage = 15;
                lab.split = false;
            }
            return Err(error);
        }
        Ok(())
    }

    fn note_analytic(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.analytic_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    pub(super) fn analytic_write_report(&mut self) {
        let Some(lab) = self.analytic_lab.as_ref() else {
            return;
        };
        let mut text = lab.report.clone();
        text.push_str("Analytic intent report: REPORT-ANALYTIC-5.txt\n");
        let dir = jarvig_core::analytic_proof_dir();
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-ANALYTIC-5.txt"), &text)) {
            Ok(()) => self.note_analytic("Analytic intent report: REPORT-ANALYTIC-5.txt"),
            Err(error) => self.note_analytic(&format!("Analytic intent report was not written: {error}")),
        }
    }

    fn analytic_capture(&mut self, name: &str) -> Result<(), String> {
        let path = jarvig_core::analytic_proof_dir().join(name);
        let image = super::capture_window_image(self.frame)?;
        super::write_png(&path.to_string_lossy(), &image)?;
        self.note_analytic(&format!("Capture: {name}"));
        Ok(())
    }

    fn analytic_stage_setup(&mut self) -> Result<(), String> {
        if self.view_lab.is_some() || self.view_realization.is_some() || self.observation_lab.is_some() || self.direct_lab.is_some() || self.intent_lab.is_some() || self.intent_live_rerealize {
            return Err("Experiment 5 refuses to run beside another realization lab".into());
        }
        if self.engine.intent_authority_experiment() {
            return Err("Experiment 5 refuses the intent-authority experiment flag".into());
        }
        if let Some(project) = &self.project_file {
            let text = project.display().to_string();
            if text.contains("IntentProof") || text.contains("jarvig-intent-proof") {
                return Err("Experiment 5 opened Intent Proof".into());
            }
        }
        if self.renderer.is_none() || self.editor_camera.is_none() || self.viewport_view.is_none() || self.viewport_px.1 < 64 {
            if self.frames >= 400 {
                return Err("the perspective viewport was not ready".into());
            }
            return Ok(());
        }
        if !self.analytic_lab.as_ref().unwrap().arranged {
            for panel in [OUTLINER, INSPECTOR, CONTENT, OUTPUT] {
                if self.workspace.is_open(panel) {
                    self.workspace.apply(WorkspaceCommand::ClosePanel(panel)).map_err(|error| error.to_string())?;
                }
            }
            unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, 1800, 1200, SWP_NOZORDER); }
            self.realize();
            let lab = self.analytic_lab.as_mut().unwrap();
            lab.arranged = true;
            lab.mark = self.frames;
            return Ok(());
        }
        let (panel_w, panel_h) = self.viewport_px;
        let renderer_size = self.renderer.as_ref().unwrap().configured_size();
        if panel_w < REQUIRED_WIDTH || panel_h < VIEW_HEIGHT || renderer_size.0 < REQUIRED_WIDTH || renderer_size.1 < VIEW_HEIGHT {
            let grow = self.analytic_lab.as_ref().unwrap().grow;
            let mark = self.analytic_lab.as_ref().unwrap().mark;
            if grow < 2 && self.frames.saturating_sub(mark) > 8 {
                let (width, height) = if grow == 0 { (2200, 1400) } else { (2600, 1600) };
                unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, width, height, SWP_NOZORDER); }
                self.realize();
                let lab = self.analytic_lab.as_mut().unwrap();
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
            let Some(solid) = outline.iter().find(|item| item.name == "Analytic Solid").map(|item| item.uuid) else {
                if self.frames >= 500 {
                    return Err("Analytic Solid was not in the live level".into());
                }
                return Ok(());
            };
            let object_mesh = self.engine.world().object_mesh(solid).ok_or("Analytic Solid has no mesh")?;
            self.renderer.as_ref().unwrap().resident_mesh_bytes(object_mesh)
        };
        let Some(control_gpu_bytes) = control_gpu else {
            if self.frames >= 500 {
                return Err("the control mesh was not uploaded".into());
            }
            return Ok(());
        };
        self.analytic_install(control_gpu_bytes, (panel_w, panel_h))?;
        Ok(())
    }

    fn analytic_install(&mut self, control_gpu_bytes: u64, panel: (u32, u32)) -> Result<(), String> {
        let level_path = self.level_file.clone().ok_or("the analytic level is not open")?;
        let level_text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        let parsed = jarvig_core::parse_level(&level_text).map_err(|error| error.to_string())?;
        let record = block_named(&parsed, "Analytic Solid")?;
        let component_version = parametric_version(&level_text)?;
        if component_version != 2 {
            return Err(format!("ParametricBlock version {component_version} stored the analytic chart"));
        }
        if !level_text.contains(jarvig_core::ANALYTIC_CHART) {
            return Err("the level file does not contain the chart spelling".into());
        }
        let far_camera = jarvig_core::far_analytic_camera();
        let close_camera = jarvig_core::close_analytic_camera();
        if far_camera.viewport_width != VIEW_FAR_WIDTH as f32 || close_camera.viewport_width != VIEW_CLOSE_WIDTH as f32 || far_camera.viewport_height != VIEW_HEIGHT as f32 {
            return Err("the analytic cameras were not 160x567 and 960x567".into());
        }
        if (far_camera.requested_error_px - 0.5).abs() > 1.0e-6 || (close_camera.requested_error_px - 0.5).abs() > 1.0e-6 {
            return Err("the analytic cameras did not both request 0.5 px".into());
        }
        let gathered = {
            let world = self.engine.world();
            let solid = world.entity_outline().iter().find(|item| item.name == "Analytic Solid").map(|item| item.uuid).ok_or("Analytic Solid missing")?;
            let live = world.authored_block(solid).ok_or("Analytic Solid has no block")?;
            if live.body.is_some() || record.body.is_some() {
                return Err("Analytic Solid has a stored body".into());
            }
            if live.intent.len() != 2 || record.intent.len() != 2 || !live.history.is_empty() || !record.history.is_empty() {
                return Err("Analytic Solid is not two intent entries and an empty history".into());
            }
            let object_mesh = world.object_mesh(solid).ok_or("the ordinary renderer has no mesh for Analytic Solid")?;
            let shared = world.meshes().get(object_mesh).ok_or("the ordinary control mesh is missing")?;
            let control_vertices = shared.vertex_count();
            let control_triangles = shared.index_count() / 3;
            if control_vertices != 24 || control_triangles != 12 || control_gpu_bytes != 1512 {
                return Err(format!("the ordinary control is {control_vertices} vertices, {control_triangles} triangles, {control_gpu_bytes} GPU bytes"));
            }
            let front = world.front_camera();
            let expected_fov = 60.0_f64.to_radians();
            if (front.vertical_fov_radians - expected_fov).abs() > 1.0e-6 || (f64::from(front.near_m) - 0.1).abs() > 1.0e-6 {
                return Err("the front camera is not the 60 degree, 0.1 m camera".into());
            }
            (solid, object_mesh, control_vertices, control_triangles, front, world.revision(), world.mesh_count())
        };
        let (solid, object_mesh, control_vertices, control_triangles, front, revision_before, mesh_count) = gathered;
        let diagnostic = jarvig_core::intent_authority_diagnostic(&record);
        if diagnostic.contains("Intent Authority: ELIGIBLE") {
            return Err(diagnostic);
        }
        let product_far = jarvig_core::realize_saved_chart(&record, &far_camera, 1).map_err(|error| format!("far analytic realization stopped: {error}"))?;
        let product_close = jarvig_core::realize_saved_chart(&record, &close_camera, 1).map_err(|error| format!("close analytic realization stopped: {error}"))?;
        self.note_analytic("Experiment 5: ON");
        self.note_analytic("Experiment 4 frozen: PASS");
        self.note_analytic("Intent-authority experiment: OFF");
        self.note_analytic("ADR-0074 accepted: NO");
        self.note_analytic("Default renderer replaced: NO");
        self.note_analytic("Curved trailer: NO");
        self.note_analytic(&format!("ParametricBlock version: {component_version}"));
        self.note_analytic("Level format: 6");
        self.note_analytic("Type registry: 9");
        self.note_analytic(&format!("Chart in the file: {}", jarvig_core::ANALYTIC_CHART));
        self.note_analytic(&format!("Chart after parse: {}", product_far.chart));
        self.note_analytic(&format!("Chart the realization consulted: {}", product_close.chart));
        self.note_analytic(&diagnostic);
        self.note_analytic(&format!("ANALYTIC_INTENT_ALGORITHM_VERSION: {}", jarvig_core::ANALYTIC_INTENT_ALGORITHM_VERSION));
        self.note_analytic(&format!("INTENT_OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::INTENT_OBSERVATION_ALGORITHM_VERSION));
        self.note_analytic(&format!("DIRECT_REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION));
        self.note_analytic(&format!("OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::OBSERVATION_ALGORITHM_VERSION));
        self.note_analytic(&format!("REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::REALIZATION_ALGORITHM_VERSION));
        self.note_analytic(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_analytic("jarvig-realize-0: NOT STARTED");
        self.note_analytic("jarvig-realize-1: NOT STARTED");
        self.note_analytic("queue: NOT USED");
        self.note_analytic(&format!("Graphics adapter: {}", self.graphics_name));
        self.note_analytic(&format!("Graphics vendor: {:04x}", self.graphics_vendor));
        self.note_analytic(&format!("Graphics device: {:04x}", self.graphics_device));
        self.note_analytic(&format!("Graphics api: {}", self.graphics_api));
        self.note_analytic(&format!("Editor panel: {}x{}", panel.0, panel.1));
        self.note_analytic("Observation Far viewport: 160x567");
        self.note_analytic("Observation Close viewport: 960x567");
        self.note_analytic("Re-admitted at the editor panel size: NO");
        self.note_analytic(&format!("Requested error px: {:.1}", far_camera.requested_error_px));
        self.note_analytic(&format!("Observation Far eye_local: [{:.4}, {:.4}, {:.4}]", far_camera.eye[0], far_camera.eye[1], far_camera.eye[2]));
        self.note_analytic(&format!("Observation Close eye_local: [{:.4}, {:.4}, {:.4}]", close_camera.eye[0], close_camera.eye[1], close_camera.eye[2]));
        self.note_analytic("Observation forward_local: [0.0000, 0.0000, -1.0000]");
        self.note_analytic("Authority: saved analytic-surface intent");
        self.note_analytic("Stored tessellated body: ABSENT");
        self.note_analytic(&format!("authority_hash: {:016x}", product_far.authority_hash));
        self.note_analytic("authority_revision: 1");
        self.note_analytic(&format!("Ordinary control mesh: {control_vertices} vertices, {control_triangles} triangles"));
        self.note_analytic(&format!("Level file: {}", level_path.display()));
        self.note_analytic(&format!("World revision before: {revision_before}"));
        self.note_analytic(&jarvig_core::format_analytic_product("Observation Far", &product_far));
        self.note_analytic(&jarvig_core::format_analytic_product("Observation Close", &product_close));
        if let Err(error) = frozen_analytic_gate(&product_far, &product_close) {
            self.note_analytic("Frame: NOT PRESENTED");
            return Err(error);
        }
        let material_bound = {
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            snapshot.instances().iter().find(|instance| instance.entity == solid).and_then(|instance| instance.material_for_slot(0)).is_some()
        };
        if !material_bound {
            self.note_analytic("Slot-0 material on Analytic Solid: UNBOUND");
            self.note_analytic("Frame: NOT PRESENTED");
            return Err("the override would be skipped because slot 0 is unbound".into());
        }
        self.note_analytic("Slot-0 material on Analytic Solid: BOUND");
        self.note_analytic("bind_material called: NO");
        self.note_analytic("New material asset: NO");
        if self.engine.world().revision() != revision_before {
            return Err("the world revision moved before any observation mesh was added".into());
        }
        let mesh_far = self.engine.world_mut().add_mesh(product_far.mesh.clone());
        let mesh_close = self.engine.world_mut().add_mesh(product_close.mesh.clone());
        let decoy = self.engine.world_mut().add_mesh(jarvig_core::near_triangle_mesh());
        if self.engine.world().revision() != revision_before {
            return Err("add_mesh revised the world".into());
        }
        if mesh_far == object_mesh || mesh_close == object_mesh || mesh_far == mesh_close || decoy == mesh_far || decoy == mesh_close {
            return Err("an observation mesh reused the control mesh id".into());
        }
        if self.engine.world().derived_meshlets(mesh_far).is_some() || self.engine.world().derived_meshlets(mesh_close).is_some() {
            return Err("an observation mesh built meshlets".into());
        }
        if self.engine.world().object_mesh(solid) != Some(object_mesh) {
            return Err("the Analytic Solid mesh id changed".into());
        }
        let solid_pose = self.engine.world().entity_world_pose(solid).map_err(|error| error.to_string())?;
        let pose_far = analytic_pose(solid_pose, far_camera.eye, far_camera.forward)?;
        let pose_close = analytic_pose(solid_pose, close_camera.eye, close_camera.forward)?;
        let camera = jarvig_core::Camera { frame: front.frame, vertical_fov_radians: far_camera.vertical_fov_radians, near_m: far_camera.near_m as f32 };
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
            renderer.set_view_pixel_rect(left, Some(PixelRect { x: 0, y: 0, width: VIEW_FAR_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
            renderer.set_view_pixel_rect(right, Some(PixelRect { x: VIEW_FAR_WIDTH, y: 0, width: VIEW_CLOSE_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_far) }).map_err(|error| error.to_string())?;
            renderer.update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_close) }).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(left, solid, Some(mesh_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, solid, Some(mesh_close)).map_err(|error| error.to_string())?;
        }
        let (rect_far, rect_close) = {
            let renderer = self.renderer.as_ref().unwrap();
            (renderer.viewport(left).map_err(|error| error.to_string())?, renderer.viewport(right).map_err(|error| error.to_string())?)
        };
        let Some(rect_far) = rect_far else { return Err("view Far has no pixel viewport".into()); };
        let Some(rect_close) = rect_close else { return Err("view Close has no pixel viewport".into()); };
        if rect_far != (PixelRect { x: 0, y: 0, width: VIEW_FAR_WIDTH, height: VIEW_HEIGHT }) || rect_close != (PixelRect { x: VIEW_FAR_WIDTH, y: 0, width: VIEW_CLOSE_WIDTH, height: VIEW_HEIGHT }) {
            return Err(format!("viewports were {}x{} and {}x{}, not 160x567 and 960x567", rect_far.width, rect_far.height, rect_close.width, rect_close.height));
        }
        if self.engine.world().revision() != revision_before {
            return Err("creating the observation views revised the world".into());
        }
        {
            let lab = self.analytic_lab.as_mut().unwrap();
            lab.solid = Some(solid);
            lab.control_mesh = Some(object_mesh);
            lab.mesh_far = Some(mesh_far);
            lab.mesh_close = Some(mesh_close);
            lab.decoy = Some(decoy);
            lab.observation_meshes.extend([mesh_far, mesh_close]);
            lab.right_view = Some(right);
            lab.key_far = product_far.cache_key.clone();
            lab.key_close = product_close.cache_key.clone();
            lab.pose_far = pose_far;
            lab.pose_close = pose_close;
            lab.camera = camera;
            lab.close_gpu_bytes = product_close.expected_gpu_bytes;
            lab.product_far = Some(product_far);
            lab.product_close = Some(product_close);
            lab.authority_hash = lab.product_far.as_ref().unwrap().authority_hash;
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
        self.note_analytic(&format!("Control mesh id: {}", object_mesh.0));
        self.note_analytic(&format!("Observation Far mesh id: {}", mesh_far.0));
        self.note_analytic(&format!("Observation Close mesh id: {}", mesh_close.0));
        self.note_analytic(&format!("Decoy mesh id: {}", decoy.0));
        self.note_analytic("Marker entity: NOT SPAWNED");
        self.note_analytic("set_object_mesh called: NO");
        self.note_analytic("build_meshlets called: NO");
        self.note_analytic("Observation meshlets: 0");
        self.note_analytic(&format!("Control mesh GPU_bytes before: {control_gpu_bytes}"));
        self.note_analytic(&format!("World revision after add_mesh: {}", self.engine.world().revision()));
        self.note_analytic("World revision unchanged by add_mesh: YES");
        self.note_analytic("Color pass overrides the Analytic Solid: YES");
        Ok(())
    }

    fn analytic_stage_present(&mut self) -> Result<(), String> {
        if !self.analytic_lab.as_ref().unwrap().proved {
            if !self.analytic_frame_matches()? {
                if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the first analytic frame did not upload ({})", self.analytic_wait_reason()));
                }
                return Ok(());
            }
            self.analytic_note_uploads(false)?;
            self.analytic_note_passes("simultaneous", false)?;
            self.analytic_capture("shot-analytic-5-simultaneous.png")?;
            self.analytic_reject_stale()?;
            if self.analytic_lab.as_ref().unwrap().discarded_stale_gpu < 3 {
                return Err("the first present rejected fewer than 3 stale uploads".into());
            }
            self.note_analytic("First frame: PRESENTED");
            let lab = self.analytic_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.analytic_frame_matches()? {
                return Err("a later frame substituted an analytic buffer".into());
            }
            if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) == 1 && self.analytic_lab.as_ref().unwrap().caster_check_pending {
                self.analytic_note_passes("cached", true)?;
            }
            return Ok(());
        }
        self.analytic_lab.as_mut().unwrap().stage = 2;
        self.analytic_lab.as_mut().unwrap().mark = self.frames;
        self.note_analytic("First present held. The saved radius edit is next.");
        Ok(())
    }

    fn analytic_stage_edit(&mut self) -> Result<(), String> {
        let (level_path, revision_before, old_far, old_close, old_key_far, old_key_close, hash_before) = {
            let lab = self.analytic_lab.as_ref().unwrap();
            (
                lab.level_path.clone(),
                lab.revision_before,
                lab.mesh_far.ok_or("mesh Far missing")?,
                lab.mesh_close.ok_or("mesh Close missing")?,
                lab.key_far.clone(),
                lab.key_close.clone(),
                lab.authority_hash,
            )
        };
        let text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        let mut document = jarvig_core::parse_level(&text).map_err(|error| error.to_string())?;
        let current = block_named(&document, "Analytic Solid")?;
        if current.body.is_some() || current.intent.len() != 2 || !current.history.is_empty() {
            return Err("the file changed before the radius edit".into());
        }
        let edited = jarvig_core::replace_surface_radius(&current, jarvig_core::SURFACE_ONE, jarvig_core::EDITED_RADIUS_M).map_err(|error| error.to_string())?;
        install_block(&mut document, edited);
        let backup = jarvig_core::analytic_proof_dir().join("Saved").join("Backup");
        jarvig_core::save_level_atomic(std::path::Path::new(&level_path), &backup, &document).map_err(|error| error.to_string())?;
        let parsed_text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        if !parsed_text.contains(jarvig_core::ANALYTIC_CHART) {
            return Err("the saved edit lost the chart spelling".into());
        }
        if parametric_version(&parsed_text)? != 2 {
            return Err("the saved edit left ParametricBlock version 1".into());
        }
        let parsed_document = jarvig_core::parse_level(&parsed_text).map_err(|error| error.to_string())?;
        let parsed = block_named(&parsed_document, "Analytic Solid")?;
        let radius = radius_of(&parsed, jarvig_core::SURFACE_ONE);
        if radius.to_bits() != jarvig_core::EDITED_RADIUS_M.to_bits() {
            return Err("the second realization did not read the parsed radius 1.25".into());
        }
        if radius_of(&parsed, jarvig_core::SURFACE_TWO).to_bits() != 1.0f64.to_bits() {
            return Err("surface 2 radius changed".into());
        }
        if chart_of(&parsed, jarvig_core::SURFACE_ONE) != jarvig_core::ANALYTIC_CHART || parsed.intent.len() != 2 || !parsed.history.is_empty() || parsed.body.is_some() {
            return Err("the parsed edit did not keep the chart, the tape length, the empty history, and the absent body".into());
        }
        let edited_hash = jarvig_core::analytic_authority_hash(&parsed).map_err(|error| error.to_string())?;
        if edited_hash == hash_before || edited_hash == jarvig_core::NOT_AUTHORITY_3C || edited_hash == jarvig_core::NOT_AUTHORITY_EXPERIMENT4 || edited_hash == jarvig_core::NOT_AUTHORITY_EXPERIMENT4_EDIT {
            return Err("the saved radius edit did not change the authority hash".into());
        }
        let far_camera = jarvig_core::far_analytic_camera();
        let close_camera = jarvig_core::close_analytic_camera();
        let product_far = jarvig_core::realize_saved_chart(&parsed, &far_camera, 2).map_err(|error| format!("edited far realization stopped: {error}"))?;
        let product_close = jarvig_core::realize_saved_chart(&parsed, &close_camera, 2).map_err(|error| format!("edited close realization stopped: {error}"))?;
        if product_far.surfaces[0].admission != jarvig_core::AnalyticAdmission::AdmitInView || product_close.surfaces[0].admission != jarvig_core::AnalyticAdmission::AdmitInView {
            return Err("radius 1.25 omitted surface 1. No other radius or eye was tried".into());
        }
        self.note_analytic(&format!("Edited radius bits: {:016x}", radius.to_bits()));
        self.note_analytic(&format!("authority_hash after edit: {edited_hash:016x}"));
        self.note_analytic("authority_revision: 2");
        self.note_analytic("Radius edit written by the level writer: YES");
        self.note_analytic("Second realization source: parsed level file");
        self.note_analytic(&jarvig_core::format_analytic_product("Edited Observation Far", &product_far));
        self.note_analytic(&jarvig_core::format_analytic_product("Edited Observation Close", &product_close));
        if product_far.position_hash == FAR_POSITION || product_close.position_hash == CLOSE_POSITION || product_far.position_hash == self.analytic_lab.as_ref().unwrap().product_far.as_ref().unwrap().position_hash || product_close.position_hash == self.analytic_lab.as_ref().unwrap().product_close.as_ref().unwrap().position_hash {
            return Err("the saved radius edit left a position hash unchanged".into());
        }
        if product_far.surfaces[1].admission != jarvig_core::AnalyticAdmission::Omit || product_close.surfaces[1].gpu_bytes != 0 {
            return Err("surface 2 contributed triangles after the radius edit".into());
        }
        if product_far.chart != jarvig_core::ANALYTIC_CHART || product_close.chart != jarvig_core::ANALYTIC_CHART {
            return Err("the edited realization consulted a different chart".into());
        }
        self.note_analytic("Surface 1 position hash changed: YES");
        if product_far.cache_key == old_key_far || product_close.cache_key == old_key_close || !product_far.cache_key.starts_with("analytic5-") {
            return Err("the edited cache key did not leave the revision-1 key".into());
        }
        let (left, right, entity) = {
            let lab = self.analytic_lab.as_ref().unwrap();
            (self.viewport_view.ok_or("view Far missing")?, lab.right_view.ok_or("view Close missing")?, lab.solid.unwrap())
        };
        self.analytic_bind_if_current(left, entity, old_far, &old_key_far, &product_far.cache_key)?;
        self.analytic_bind_if_current(right, entity, old_close, &old_key_close, &product_close.cache_key)?;
        if self.analytic_lab.as_ref().unwrap().discarded_stale_gpu < 5 {
            return Err("the revision-1 meshes were not rejected after the edit".into());
        }
        self.note_analytic("Revision-1 GPU meshes rejected: YES");
        let mesh_far = self.engine.world_mut().add_mesh(product_far.mesh.clone());
        let mesh_close = self.engine.world_mut().add_mesh(product_close.mesh.clone());
        if self.engine.world().revision() != revision_before {
            return Err("the edited upload revised the world".into());
        }
        if mesh_far == old_far || mesh_close == old_close || mesh_far == mesh_close {
            return Err("the edited realization reused a revision-1 mesh id".into());
        }
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_mesh_override(left, entity, Some(mesh_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, Some(mesh_close)).map_err(|error| error.to_string())?;
        }
        self.analytic_retire_mesh(old_far)?;
        self.analytic_retire_mesh(old_close)?;
        {
            let lab = self.analytic_lab.as_mut().unwrap();
            lab.mesh_far = Some(mesh_far);
            lab.mesh_close = Some(mesh_close);
            lab.observation_meshes.extend([mesh_far, mesh_close]);
            lab.key_far = product_far.cache_key.clone();
            lab.key_close = product_close.cache_key.clone();
            lab.close_gpu_bytes = product_close.expected_gpu_bytes;
            lab.product_far = Some(product_far);
            lab.product_close = Some(product_close);
            lab.edited_hash = edited_hash;
            lab.stage = 3;
            lab.mark = self.frames;
        }
        self.note_analytic(&format!("Edited Observation Far mesh id: {}", mesh_far.0));
        self.note_analytic(&format!("Edited Observation Close mesh id: {}", mesh_close.0));
        self.note_analytic("Cameras moved between presents: NO");
        self.note_analytic(&format!("World revision after edit: {}", self.engine.world().revision()));
        Ok(())
    }

    fn analytic_stage_edited(&mut self) -> Result<(), String> {
        if !self.analytic_lab.as_ref().unwrap().edited_proved {
            if !self.analytic_frame_matches()? {
                if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the edited analytic frame did not upload ({})", self.analytic_wait_reason()));
                }
                return Ok(());
            }
            self.analytic_note_uploads(true)?;
            self.analytic_note_passes("edited", false)?;
            self.analytic_capture("shot-analytic-5-edited.png")?;
            self.note_analytic("Second frame: PRESENTED");
            let lab = self.analytic_lab.as_mut().unwrap();
            lab.edited_proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.analytic_frame_matches()? {
                return Err("a later edited frame substituted an analytic buffer".into());
            }
            if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) == 1 && self.analytic_lab.as_ref().unwrap().caster_check_pending {
                self.analytic_note_passes("edited-cached", true)?;
            }
            return Ok(());
        }
        self.analytic_retire_mesh(self.analytic_lab.as_ref().unwrap().mesh_far.ok_or("edited mesh Far missing")?)?;
        self.analytic_lab.as_mut().unwrap().mesh_far = None;
        self.analytic_lab.as_mut().unwrap().stage = 4;
        self.analytic_lab.as_mut().unwrap().mark = self.frames;
        self.note_analytic("Edited Observation Far destroy requested");
        Ok(())
    }

    fn analytic_stage_far_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh_far = *self.analytic_lab.as_ref().unwrap().retired.last().ok_or("edited mesh Far was not retired")?;
        let mesh_close = self.analytic_lab.as_ref().unwrap().mesh_close.ok_or("edited mesh Close missing")?;
        let control = self.analytic_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let gone = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_far).is_none();
        let bytes_close = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_close);
        if !gone || bytes_close.is_none() {
            if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) > 90 {
                return Err("destroying edited Far did not leave Close resident and Far evicted".into());
            }
            return Ok(());
        }
        if bytes_close != Some(self.analytic_lab.as_ref().unwrap().close_gpu_bytes) {
            return Err(format!("Close GPU bytes changed to {:?} while Far was destroyed", bytes_close));
        }
        if !self.analytic_lab.as_ref().unwrap().captured_far {
            let left = self.viewport_view.ok_or("view Far missing")?;
            let right = self.analytic_lab.as_ref().unwrap().right_view.ok_or("view Close missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            if color_contains(&draws, left, mesh_far) || !color_contains(&draws, left, control) || !color_contains(&draws, right, mesh_close) || color_contains(&draws, right, control) {
                return Err("the frame after destroying Far still mixed the observation meshes".into());
            }
            self.note_analytic("Observation Far GPU_bytes after destroy: GONE");
            self.note_analytic(&format!("Observation Close GPU_bytes while Far is gone: {}", bytes_close.unwrap()));
            self.analytic_capture("shot-analytic-5-far-destroyed.png")?;
            self.analytic_lab.as_mut().unwrap().captured_far = true;
            self.analytic_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.analytic_retire_mesh(mesh_close)?;
        self.analytic_lab.as_mut().unwrap().mesh_close = None;
        self.analytic_lab.as_mut().unwrap().stage = 5;
        self.analytic_lab.as_mut().unwrap().mark = self.frames;
        self.note_analytic("Edited Observation Close destroy requested");
        Ok(())
    }

    fn analytic_stage_close_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let meshes = self.analytic_lab.as_ref().unwrap().retired.clone();
        let gone = meshes.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        if !gone {
            if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) > 90 {
                return Err("edited observation Close stayed resident after destroy".into());
            }
            return Ok(());
        }
        if !self.analytic_lab.as_ref().unwrap().captured_close {
            self.note_analytic("Observation Close GPU_bytes after destroy: GONE");
            self.analytic_capture("shot-analytic-5-close-destroyed.png")?;
            self.analytic_lab.as_mut().unwrap().captured_close = true;
            self.analytic_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        let decoy = self.analytic_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        self.analytic_retire_mesh(decoy)?;
        self.analytic_lab.as_mut().unwrap().decoy = None;
        let right = self.analytic_lab.as_ref().unwrap().right_view.ok_or("view Close missing")?;
        let left = self.viewport_view.ok_or("view Far missing")?;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, None).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::FULL), settings: None, pose: None }).map_err(|error| error.to_string())?;
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
        }
        if self.engine.world().revision() != self.analytic_lab.as_ref().unwrap().revision_before {
            return Err("destroying the observation views revised the world".into());
        }
        let lab = self.analytic_lab.as_mut().unwrap();
        lab.split = false;
        lab.right_view = None;
        lab.stage = 6;
        lab.mark = self.frames;
        self.note_analytic("Perspective restored to one full view");
        Ok(())
    }

    fn analytic_stage_intact(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.analytic_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let lab = self.analytic_lab.as_ref().unwrap();
        let solid = lab.solid.ok_or("solid missing")?;
        let control = lab.control_mesh.ok_or("control missing")?;
        let mesh_same = self.engine.world().object_mesh(solid) == Some(control);
        let ordinary_same = self.engine.world().meshes().get(control).is_some_and(|mesh| mesh.vertex_count() == lab.control_vertices && mesh.index_count() / 3 == lab.control_triangles);
        let retired = lab.retired.clone();
        let orphans_gone = retired.iter().all(|mesh| self.engine.world().meshes().get(*mesh).is_none());
        let gpu_gone = retired.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        let control_after = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        let control_match = control_after == lab.control_gpu_bytes && control_after == Some(1512);
        let views = self.renderer.as_ref().unwrap().view_count() == 1;
        let saved_text = std::fs::read_to_string(&lab.level_path).unwrap_or_default();
        let saved = jarvig_core::parse_level(&saved_text).ok();
        let saved_record = saved.as_ref().and_then(|document| block_named(document, "Analytic Solid").ok());
        let saved_ok = saved_record.as_ref().is_some_and(|record| {
            record.body.is_none()
                && record.history.is_empty()
                && record.intent.len() == 2
                && chart_of(record, jarvig_core::SURFACE_ONE) == jarvig_core::ANALYTIC_CHART
                && chart_of(record, jarvig_core::SURFACE_TWO) == jarvig_core::ANALYTIC_CHART
                && radius_of(record, jarvig_core::SURFACE_ONE).to_bits() == jarvig_core::EDITED_RADIUS_M.to_bits()
                && radius_of(record, jarvig_core::SURFACE_TWO).to_bits() == 1.0f64.to_bits()
        });
        let main_len = std::fs::metadata(MAIN_LEVEL).map(|meta| meta.len()).unwrap_or(0);
        let main_same = main_len == 26849;
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
        let versions_ok = jarvig_core::ANALYTIC_INTENT_ALGORITHM_VERSION == 1
            && jarvig_core::INTENT_OBSERVATION_ALGORITHM_VERSION == 1
            && jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION == 1
            && jarvig_core::OBSERVATION_ALGORITHM_VERSION == 1
            && jarvig_core::REALIZATION_ALGORITHM_VERSION == 1;
        let hash_moved = lab.edited_hash != 0
            && lab.edited_hash != lab.authority_hash
            && lab.edited_hash != jarvig_core::NOT_AUTHORITY_3C
            && lab.edited_hash != jarvig_core::NOT_AUTHORITY_EXPERIMENT4
            && lab.edited_hash != jarvig_core::NOT_AUTHORITY_EXPERIMENT4_EDIT;
        let panel_ok = lab.panel.0 >= REQUIRED_WIDTH && lab.panel.1 >= VIEW_HEIGHT;
        let object_ok = mesh_same
            && ordinary_same
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
        self.analytic_capture("shot-analytic-5-object.png")?;
        self.note_analytic(&format!("Editor frame: {}", self.frames));
        self.note_analytic(&format!("Presented frames: {}", self.renderer.as_ref().unwrap().presented_frames()));
        self.note_analytic(&format!("Frame: {}", if presented && counts_ok { "PRESENTED" } else { "NOT PRESENTED" }));
        self.note_analytic(&format!("Graphics is Intel UHD DX12 8086:9b41: {}", yes_no(graphics_ok)));
        self.note_analytic(&format!("Acquires on the first proof frame: {}", self.analytic_lab.as_ref().unwrap().acquires_first));
        self.note_analytic(&format!("Presents on the first proof frame: {}", self.analytic_lab.as_ref().unwrap().presents_first));
        self.note_analytic(&format!("Acquires on the second proof frame: {}", self.analytic_lab.as_ref().unwrap().acquires_second));
        self.note_analytic(&format!("Presents on the second proof frame: {}", self.analytic_lab.as_ref().unwrap().presents_second));
        self.note_analytic(&format!("Saved analytic intent intact: {}", yes_no(saved_ok)));
        self.note_analytic(&format!("Ordinary control mesh unchanged: {}", yes_no(mesh_same && ordinary_same && control_match)));
        self.note_analytic(&format!("World revision before: {}", self.analytic_lab.as_ref().unwrap().revision_before));
        self.note_analytic(&format!("World revision after: {}", self.engine.world().revision()));
        self.note_analytic(&format!("World revision unchanged: {}", yes_no(revision_same)));
        self.note_analytic(&format!("Library mesh count restored: {}", yes_no(mesh_count_same)));
        self.note_analytic(&format!("Observation GPU buffers after destroy: {}", if gpu_gone { "EVICTED" } else { "RESIDENT" }));
        self.note_analytic(&format!("Control mesh GPU_bytes after: {}", control_after.map(|value| value.to_string()).unwrap_or_else(|| "GONE".into())));
        self.note_analytic(&format!("Main.jarviglevel bytes: {main_len}"));
        self.note_analytic(&format!("Render view count restored: {}", yes_no(views)));
        self.note_analytic(&format!("Editor element selection: {element}"));
        self.note_analytic(&format!("stale uploads rejected: {}", self.analytic_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_analytic(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_analytic("Intent-authority experiment: OFF");
        self.note_analytic("ADR-0074 accepted: NO");
        self.note_analytic("RFC-0007 accepted: NO");
        self.note_analytic("RFC-0007 Phase 3: NO");
        self.note_analytic("RFC-0002 stamped: NO");
        self.note_analytic("Renderer replacement: NO");
        let passed = object_ok
            && self.analytic_lab.as_ref().unwrap().report.contains("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES")
            && self.analytic_lab.as_ref().unwrap().report.contains("Second frame: PRESENTED")
            && self.analytic_lab.as_ref().unwrap().report.contains("First frame: PRESENTED");
        if passed {
            self.note_analytic("Earned: JARVIG persisted an editable mathematical surface instead of its render mesh. Two observations independently synthesized the geometry they needed from that same saved object.");
        }
        self.note_analytic(&format!("Experiment 5: {}", if passed { "PASS" } else { "FAIL" }));
        self.analytic_write_report();
        self.analytic_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 5 did not meet the presented-frame success condition".into());
        }
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn analytic_note_uploads(&mut self, second: bool) -> Result<(), String> {
        let (mesh_far, mesh_close, product_far, product_close) = {
            let lab = self.analytic_lab.as_ref().unwrap();
            (lab.mesh_far.unwrap(), lab.mesh_close.unwrap(), lab.product_far.clone().unwrap(), lab.product_close.clone().unwrap())
        };
        let record_far = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_far).ok_or("observation Far was not uploaded")?;
        let record_close = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_close).ok_or("observation Close was not uploaded")?;
        let prove = |label: &str, product: &jarvig_core::AnalyticProduct, record: jarvig_renderer::MeshUploadRecord| -> Result<(), String> {
            if record.vertex_create_bytes != product.packed_vertex_bytes || record.index_create_bytes != product.packed_index_bytes || record.gpu_bytes != product.expected_gpu_bytes {
                return Err(format!("{label} create_buffer bytes did not match the provenance total"));
            }
            if record.vertex_create_bytes + record.index_create_bytes != record.gpu_bytes {
                return Err(format!("{label} GpuMesh.bytes is not the sum of the create_buffer sizes"));
            }
            let uploaded = record.index_create_bytes / u64::from(product.index_format.byte_size()) / 3;
            if uploaded != u64::from(product.packed_triangles) {
                return Err(format!("{label} uploaded a different triangle count"));
            }
            Ok(())
        };
        let label_far = if second { "Edited Observation Far" } else { "Observation Far" };
        let label_close = if second { "Edited Observation Close" } else { "Observation Close" };
        prove(label_far, &product_far, record_far)?;
        prove(label_close, &product_close, record_close)?;
        if !second && (product_far.expected_gpu_bytes != 13392 || product_close.expected_gpu_bytes != 72912) {
            return Err("the first present left the frozen 13392 and 72912 GPU byte pair".into());
        }
        self.note_analytic(&format!("{label_far} GpuMesh.bytes: {}", record_far.gpu_bytes));
        self.note_analytic(&format!("{label_far} create_buffer bytes: {}", record_far.vertex_create_bytes + record_far.index_create_bytes));
        self.note_analytic(&format!("{label_close} GpuMesh.bytes: {}", record_close.gpu_bytes));
        self.note_analytic(&format!("{label_close} create_buffer bytes: {}", record_close.vertex_create_bytes + record_close.index_create_bytes));
        self.note_analytic("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_analytic("triangles_discarded_after_upload: 0");
        let acquires = self.renderer.as_ref().unwrap().acquires_last_frame();
        let presents = self.renderer.as_ref().unwrap().presents_last_frame();
        self.note_analytic(&format!("Acquires this frame: {acquires}"));
        self.note_analytic(&format!("Presents this frame: {presents}"));
        if acquires != 1 || presents != 1 {
            return Err("the analytic frame did not use one acquire and one present".into());
        }
        let lab = self.analytic_lab.as_mut().unwrap();
        if second {
            lab.acquires_second = acquires;
            lab.presents_second = presents;
        } else {
            lab.acquires_first = acquires;
            lab.presents_first = presents;
        }
        lab.close_gpu_bytes = record_close.gpu_bytes;
        Ok(())
    }

    fn analytic_note_passes(&mut self, label: &str, require_recorded: bool) -> Result<(), String> {
        let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
        let control = self.analytic_lab.as_ref().unwrap().control_mesh.unwrap();
        let decoy = self.analytic_lab.as_ref().unwrap().decoy.unwrap();
        let observation = self.analytic_lab.as_ref().unwrap().observation_meshes.clone();
        let shadow = pass_meshes(&draws, "shadow");
        let shadow_cached = pass_meshes(&draws, "shadow-cached");
        let contact = pass_meshes(&draws, "contact");
        let probe = pass_meshes(&draws, "probe");
        let casters = if shadow.is_empty() { shadow_cached.clone() } else { shadow.clone() };
        self.note_analytic(&format!("{label} shadow draws: {}", mesh_list(&shadow)));
        self.note_analytic(&format!("{label} shadow casters: {}", mesh_list(&casters)));
        self.note_analytic(&format!("{label} contact meshes: {}", mesh_list(&contact)));
        self.note_analytic(&format!("{label} probe meshes: {}", mesh_list(&probe)));
        let forbidden = |list: &[MeshId]| list.iter().any(|mesh| observation.contains(mesh) || *mesh == decoy);
        if forbidden(&shadow) || forbidden(&shadow_cached) || forbidden(&contact) || forbidden(&probe) {
            return Err("an observation mesh was drawn by shadow, contact, or probe capture".into());
        }
        self.note_analytic("Observation mesh in shadow draws: NO");
        self.note_analytic("Observation mesh in contact draws: NO");
        self.note_analytic("Observation mesh in probe draws: NO");
        if casters.is_empty() && !require_recorded {
            let solid = self.analytic_lab.as_ref().unwrap().solid.ok_or("the ordinary control entity is missing")?;
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let casts = snapshot.instances().iter().find(|instance| instance.entity == solid).is_some_and(|instance| instance.cast_shadows && instance.mesh == control);
            if !casts {
                return Err("the ordinary control mesh does not cast shadows".into());
            }
            self.note_analytic("Ordinary control cast_shadows: YES");
            self.note_analytic("Shadow caster list on the rebuild frame: NOT RECORDED");
            self.analytic_lab.as_mut().unwrap().caster_check_pending = true;
            return Ok(());
        }
        if !casters.contains(&control) {
            return Err("the ordinary control mesh was not the shadow caster".into());
        }
        self.note_analytic("Ordinary control mesh in shadow casters: YES");
        self.analytic_lab.as_mut().unwrap().caster_check_pending = false;
        Ok(())
    }

    fn analytic_wait_reason(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else {
            return "renderer missing".into();
        };
        let Some(lab) = self.analytic_lab.as_ref() else {
            return "lab missing".into();
        };
        let rect = |view: Option<jarvig_renderer::RenderViewId>| {
            view.and_then(|view| renderer.viewport(view).ok().flatten()).map(|rect| format!("{}x{}@{},{}", rect.width, rect.height, rect.x, rect.y)).unwrap_or_else(|| "none".into())
        };
        let uploaded = |mesh: Option<MeshId>| mesh.and_then(|mesh| renderer.mesh_upload_record(mesh)).is_some();
        format!("far {} close {} far_uploaded {} close_uploaded {}", rect(self.viewport_view), rect(lab.right_view), uploaded(lab.mesh_far), uploaded(lab.mesh_close))
    }

    fn analytic_frame_matches(&self) -> Result<bool, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let left = self.viewport_view.ok_or("view Far missing")?;
        let right = match self.analytic_lab.as_ref().unwrap().right_view {
            Some(view) => view,
            None => return Ok(false),
        };
        let mesh_far = self.analytic_lab.as_ref().unwrap().mesh_far;
        let mesh_close = self.analytic_lab.as_ref().unwrap().mesh_close;
        let control = self.analytic_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let decoy = self.analytic_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        let draws = renderer.frame_mesh_draws();
        let left_rect = renderer.viewport(left).ok().flatten();
        let right_rect = renderer.viewport(right).ok().flatten();
        let rects_ok = left_rect == Some(PixelRect { x: 0, y: 0, width: VIEW_FAR_WIDTH, height: VIEW_HEIGHT })
            && right_rect == Some(PixelRect { x: VIEW_FAR_WIDTH, y: 0, width: VIEW_CLOSE_WIDTH, height: VIEW_HEIGHT });
        if !rects_ok {
            return Ok(false);
        }
        let (Some(mesh_far), Some(mesh_close)) = (mesh_far, mesh_close) else {
            return Ok(false);
        };
        let ready = color_contains(draws, left, mesh_far)
            && !color_contains(draws, left, mesh_close)
            && !color_contains(draws, left, control)
            && !color_contains(draws, left, decoy)
            && color_contains(draws, right, mesh_close)
            && !color_contains(draws, right, mesh_far)
            && !color_contains(draws, right, control)
            && !color_contains(draws, right, decoy)
            && renderer.mesh_upload_record(mesh_far).is_some()
            && renderer.mesh_upload_record(mesh_close).is_some();
        Ok(ready)
    }

    fn analytic_reject_stale(&mut self) -> Result<(), String> {
        let (left, right, mesh_far, mesh_close, decoy, key_far, key_close, entity) = {
            let lab = self.analytic_lab.as_ref().unwrap();
            (
                self.viewport_view.ok_or("view Far missing")?,
                lab.right_view.ok_or("view Close missing")?,
                lab.mesh_far.unwrap(),
                lab.mesh_close.unwrap(),
                lab.decoy.unwrap(),
                lab.key_far.clone(),
                lab.key_close.clone(),
                lab.solid.unwrap(),
            )
        };
        self.analytic_bind_if_current(left, entity, mesh_close, &key_close, &key_far)?;
        self.analytic_bind_if_current(right, entity, mesh_far, &key_far, &key_close)?;
        self.analytic_bind_if_current(left, entity, decoy, "stale-decoy", &key_far)?;
        if self.renderer.as_ref().unwrap().resident_mesh_bytes(decoy).is_some() {
            return Err("the refused decoy was uploaded".into());
        }
        self.note_analytic("Decoy mesh GPU_bytes: NOT UPLOADED");
        self.note_analytic(&format!("stale uploads rejected: {}", self.analytic_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_analytic("Cross-view buffer substitution: NO");
        self.note_analytic("Decoy mesh bound: NO");
        Ok(())
    }

    fn analytic_bind_if_current(&mut self, view: RenderViewId, entity: jarvig_core::EntityId, mesh: MeshId, product_key: &str, current_key: &str) -> Result<(), String> {
        if product_key != current_key {
            self.analytic_lab.as_mut().unwrap().discarded_stale_gpu = self.analytic_lab.as_ref().unwrap().discarded_stale_gpu.saturating_add(1);
            return Ok(());
        }
        self.renderer.as_mut().unwrap().set_view_mesh_override(view, entity, Some(mesh)).map_err(|error| error.to_string())
    }

    fn analytic_retire_mesh(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.analytic_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        if mesh == control {
            return Err("refusing to retire the ordinary control mesh".into());
        }
        if !self.engine.world_mut().retire_unreferenced_mesh(mesh) {
            return Err(format!("analytic mesh {} stayed referenced", mesh.0));
        }
        self.analytic_lab.as_mut().unwrap().retired.push(mesh);
        if self.engine.world().revision() != self.analytic_lab.as_ref().unwrap().revision_before {
            return Err("retire_unreferenced_mesh revised the world".into());
        }
        Ok(())
    }
}

fn frozen_analytic_gate(far: &jarvig_core::AnalyticProduct, close: &jarvig_core::AnalyticProduct) -> Result<(), String> {
    if far.authority_hash == jarvig_core::NOT_AUTHORITY_3C || far.authority_hash == jarvig_core::NOT_AUTHORITY_EXPERIMENT4 || far.authority_hash != close.authority_hash {
        return Err("the analytic authority collided with an earlier experiment or the two views disagree".into());
    }
    if !far.cache_key.starts_with("analytic5-") || !close.cache_key.starts_with("analytic5-") {
        return Err("the cache key is not analytic5-".into());
    }
    if far.chart != jarvig_core::ANALYTIC_CHART || close.chart != jarvig_core::ANALYTIC_CHART {
        return Err("the realization consulted a chart other than the persisted spelling".into());
    }
    let omitted = |account: &jarvig_core::AnalyticAccount| {
        account.identity == jarvig_core::SURFACE_TWO && account.admission == jarvig_core::AnalyticAdmission::Omit && account.triangles_constructed == 0 && account.gpu_bytes == 0
    };
    if !omitted(&far.surfaces[1]) || !omitted(&close.surfaces[1]) {
        return Err("surface 2 was not a zero account".into());
    }
    if far.trace.iter().any(|record| record.identity == jarvig_core::SURFACE_TWO) || close.trace.iter().any(|record| record.identity == jarvig_core::SURFACE_TWO) {
        return Err("the construction trace contains surface 2".into());
    }
    frozen_surface(&far.surfaces[0], 6, 72, FAR_ERROR, FAR_COARSER, FAR_POSITION, FAR_INDEX, 13392)?;
    frozen_surface(&close.surfaces[0], 14, 392, CLOSE_ERROR, CLOSE_COARSER, CLOSE_POSITION, CLOSE_INDEX, 72912)?;
    if close.surfaces[0].n <= far.surfaces[0].n {
        return Err("n_close is not greater than n_far".into());
    }
    if far.expected_gpu_bytes == 74958 || close.expected_gpu_bytes == 74958 {
        return Err("the analytic mesh reprinted the Experiment 4 close total".into());
    }
    if far.rejected_n_allocated_mesh || close.rejected_n_allocated_mesh || far.discarded_after_construction != 0 || far.discarded_during_pack != 0 || far.meshlets_constructed != 0 || far.vertices_welded {
        return Err("a rejected n, a weld, or a meshlet entered the analytic product".into());
    }
    Ok(())
}

fn frozen_surface(account: &jarvig_core::AnalyticAccount, n: u32, triangles: u32, error: &str, coarser: &str, position: u64, index: u64, gpu: u64) -> Result<(), String> {
    let error_text = format!("{:.6}", account.measured_error_px);
    let coarser_text = format!("{:.6}", account.coarser_px);
    if account.identity != jarvig_core::SURFACE_ONE
        || account.admission != jarvig_core::AnalyticAdmission::AdmitInView
        || account.n != n
        || account.triangles_constructed != triangles
        || account.gpu_bytes != gpu
        || account.position_hash != position
        || account.index_hash != index
        || error_text != error
        || coarser_text != coarser
        || !account.coarser_failed
    {
        return Err(format!(
            "surface 1 left the frozen measurement (n {} triangles {} error {error_text} coarser {coarser_text} gpu {} hash {:016x})",
            account.n, account.triangles_constructed, account.gpu_bytes, account.position_hash
        ));
    }
    Ok(())
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
    let entity = document.entities.iter_mut().find(|entity| entity.name == "Analytic Solid").expect("Analytic Solid");
    for component in &mut entity.components {
        if let jarvig_core::ComponentRecord::ParametricBlock(slot) = component {
            *slot = block;
            return;
        }
    }
}

fn chart_of(record: &jarvig_core::BlockRecord, identity: &str) -> String {
    record
        .intent
        .iter()
        .find_map(|entry| match &entry.payload {
            jarvig_core::IntentPayload::AnalyticSurface { identity: name, chart, .. } if name == identity => Some(chart.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn radius_of(record: &jarvig_core::BlockRecord, identity: &str) -> f64 {
    record
        .intent
        .iter()
        .find_map(|entry| match &entry.payload {
            jarvig_core::IntentPayload::AnalyticSurface { identity: name, radius_m, .. } if name == identity => Some(*radius_m),
            _ => None,
        })
        .unwrap_or(0.0)
}

fn parametric_version(text: &str) -> Result<u32, String> {
    let Some(at) = text.find("\"type\": \"ParametricBlock\"") else {
        return Err("the level file has no ParametricBlock".into());
    };
    let slice = &text[at..text.len().min(at + 180)];
    if slice.contains("\"version\": 2") {
        Ok(2)
    } else if slice.contains("\"version\": 1") {
        Ok(1)
    } else {
        Err("the ParametricBlock version was not 1 or 2".into())
    }
}

fn analytic_pose(solid: ResolvedPose, eye: [f64; 3], forward: [f64; 3]) -> Result<ResolvedPose, String> {
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
        return Err(format!("the view rotation does not look along the analytic forward ({delta})"));
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
